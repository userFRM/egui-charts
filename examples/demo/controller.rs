//! Application controller: owns every dialog, panel, and the replay engine, and
//! routes the typed action each library widget emits into a real chart mutation.
//!
//! The chart engine is pure: widgets return `Action` enums and never touch the
//! chart themselves. This controller is the host half that consumes those
//! actions. Indicator templates live in host memory. Controls that need a trading
//! account show a notice, and the context menu entries it does not model do nothing.

#[path = "commands.rs"]
mod commands;
#[path = "interval.rs"]
mod interval;
#[path = "live.rs"]
mod live;

use eframe::egui;
use egui_charts::Chart;
use egui_charts::chart::selection::SeriesId;
use egui_charts::chart::series::SeriesSettings;
use egui_charts::model::{Bar, BarData, ChartType, RenkoConfig, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::ui::app_state::ChartAppState;
use egui_charts::ui::context_menu::{ChartContextAction, ChartMenus, ContextMenuAction};
use egui_charts::ui::dialogs::{
    AlertData, AlertDialog, AlertDialogAction, DrawingId, DrawingPropertiesAction,
    DrawingPropertiesDialog, DrawingProps, KeyboardShortcutsDialog, SeriesContextMenuAction,
    SeriesSettingsAction, SeriesSettingsDialog,
};
use egui_charts::ui::dialogs::{DrawingContextMenuAction, DrawingLineStyle};
use egui_charts::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarAction};
use egui_charts::ui::export::{ExportDialog, ExportFormat, ExportResponse};
use egui_charts::ui::floating_replay_control::{FloatingReplayControl, ReplayControlAction};
use egui_charts::ui::model::{AlertCondition, AlertStatus, PriceAlert};
use egui_charts::ui::replay::{ReplayAction, ReplayController};
use egui_charts::ui::timeframe_toolbar::{
    DatePicker, DatePickerAction, SessionType, TimeframeToolbar, TimeframeToolbarAction,
};
use egui_charts::ui::top_toolbar::components::SymbolSearchDialog;
use egui_charts::ui::top_toolbar::components::indicator_dialog::{
    IndicatorDialog, IndicatorDialogAction, IndicatorSettingsDialog,
};
use egui_charts::ui::top_toolbar::components::settings_menu::data::ChartSettingsState;
use egui_charts::ui::top_toolbar::components::settings_menu::{SettingsAction, SettingsDialog};
use egui_charts::ui::top_toolbar::{TopToolbar, TopToolbarAction};
use egui_charts::ui::widget_bar::panels::object_tree::{
    DataWindowInfo, ObjectTreeAction, ObjectTreePanel, SourceItem,
};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBar, WidgetBarAction};
use live::LiveFeed;
use std::sync::{Arc, mpsc};

use crate::sampledata::{DemoSymbol, chart_type_from_name, visible_bars_for_range};

struct SavedIndicators {
    name: String,
    studies: Vec<Box<dyn egui_charts::studies::Indicator>>,
    inputs: Vec<String>,
}

/// Object-tree ids for indicators are offset into a high range so a single
/// `usize` id space cleanly distinguishes indicators from drawings (which use
/// small monotonic ids from the drawing manager). The main series is id 0.
const MAIN_SERIES_ID: usize = 0;
const INDICATOR_ID_BASE: usize = 1_000_000;

fn indicator_tree_id(id: SeriesId) -> usize {
    INDICATOR_ID_BASE + id.0
}

fn indicator_from_tree_id(id: usize) -> Option<SeriesId> {
    id.checked_sub(INDICATOR_ID_BASE).map(SeriesId)
}

/// Which right-hand side panel is open, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanel {
    ObjectTree,
    Alerts,
    DataWindow,
}

/// Export formats the demo can perform, with browser downloads on the web.
const EXPORT_FORMATS: &[ExportFormat] = &[
    ExportFormat::Png,
    ExportFormat::Csv,
    ExportFormat::Clipboard,
];

/// Where an export screenshot goes once egui delivers it.
enum ScreenshotTarget {
    File(Option<std::path::PathBuf>),
    Clipboard,
}

/// A captured screenshot and where it goes.
type Screenshot = (Arc<egui::ColorImage>, ScreenshotTarget);

/// App state surfaced to the toolbars via `ChartAppState`.
pub struct DemoAppState {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub chart_type: ChartType,
    pub replay_active: bool,
}

impl ChartAppState for DemoAppState {
    fn active_symbol(&self) -> &str {
        &self.symbol
    }
    fn active_timeframe(&self) -> &Timeframe {
        &self.timeframe
    }
    fn chart_type(&self) -> ChartType {
        self.chart_type
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn is_replay_active(&self) -> bool {
        self.replay_active
    }
}

/// The full demo application.
pub struct DemoApp {
    pub chart: Chart,
    live: Arc<LiveFeed>,
    /// Whether an object tree colour picker is editing a drawing, as one undo step.
    colour_drag: bool,
    footprint: Option<egui_charts::data::Footprint>,
    footprint_scale: Option<(f32, f32)>,
    feed_before_footprint: bool,
    menus: ChartMenus,
    pub state: DemoAppState,
    pub bars: Vec<Bar>,
    pub theme: Theme,

    // Persistent toolbars.
    top_toolbar: TopToolbar,
    drawing_toolbar: DrawingToolbar,
    timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,

    // Dialogs (each owns its open/closed state).
    indicator_dialog: IndicatorDialog,
    indicator_settings: IndicatorSettingsDialog,
    settings_dialog: SettingsDialog,
    settings_state: ChartSettingsState,
    symbol_search: SymbolSearchDialog,
    indicator_templates: Vec<SavedIndicators>,
    quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler,
    alert_dialog: AlertDialog,
    series_settings: SeriesSettingsDialog,
    drawing_props: DrawingPropertiesDialog,
    keyboard_shortcuts: KeyboardShortcutsDialog,
    interval_entry: Option<String>,
    interval_focus: bool,
    copied_drawing: Option<(String, egui_charts::drawings::Drawing)>,
    export_dialog: ExportDialog,
    date_picker: DatePicker,

    // Right-hand side panel.
    object_tree: ObjectTreePanel,
    open_panel: Option<SidePanel>,
    data_window_expanded: bool,

    // In-memory alert store. `AlertManager` exposes no mutators, so the demo
    // owns its alerts for the session (no fake persistence).
    alerts: Vec<PriceAlert>,
    next_alert_id: usize,

    // Replay engine and its floating transport. `None` until first toggled.
    replay: Option<ReplayController>,
    live_was_paused: bool,
    replay_control: FloatingReplayControl,

    // A pending screenshot request the egui backend fulfils next frame.
    screenshot_requested: Option<ScreenshotTarget>,
    // egui's capture callback sends each screenshot here; the next frame delivers it.
    screenshots: (mpsc::Sender<Screenshot>, mpsc::Receiver<Screenshot>),

    // Transient status notice (text, time shown). Controls whose backing is a
    // stub/network/broker surface an honest message here rather than silently
    // doing nothing, since the library toolbar renders them unconditionally.
    notice: Option<(String, f64)>,
}

impl DemoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register egui's image loaders so the embedded SVG toolbar icons decode.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let theme = Theme::light();
        let timeframe = Timeframe::Min1;
        let symbol = "SAMPLE";
        let mut options = egui_charts::config::ChartOptions::default();
        options.time_scale.bar_spacing = 6.1;
        options.crosshair.style = egui_charts::config::CrosshairStyle::Arrow;
        options.tooltip.on_long_press = true;
        options.tooltip.show_time = false;
        options.tooltip.font_size = 14.0;
        // A full-history range must also fit in a narrow window.
        options.time_scale.min_bar_spacing = 0.01;
        // The scale reserves half a slot and one pixel after the last centre.
        options.time_scale.right_offset_pixels = Some(65.0 - 6.1 * 0.5 - 1.0);
        options.time_scale.time_visible = true;
        options.time_scale.seconds_visible = true;
        let live = Arc::new(LiveFeed::new(symbol, chrono::Utc::now()));
        let mut chart = egui_charts::ChartBuilder::new()
            .with_feed(live.clone())
            .with_symbol(symbol)
            .with_timeframe(timeframe)
            .with_theme(theme.clone())
            .with_visible_candles(210)
            .with_chart_options(options)
            .build();
        chart.config_mut().volume_height_fraction = 0.25;
        chart.config_mut().volume_baseline_inset = 0.0;
        chart.config_mut().candle_width = 0.75;
        chart.config_mut().right_axis_width = 59.0;
        chart.config_mut().skip_background = true;
        chart.set_symbol(symbol);
        chart.set_timeframe(timeframe);

        let mut top_toolbar = TopToolbar::new().with_config(
            egui_charts::ui::top_toolbar::TopToolbarConfig::default().with_height(38.0),
        );
        top_toolbar.timeframe_selector.current =
            egui_charts::ui::top_toolbar::components::timeframe_selector::Timeframe::new(
                1,
                egui_charts::ui::top_toolbar::components::timeframe_selector::TimeframeUnit::Minute,
            );
        let mut widget_bar = WidgetBar::default();
        widget_bar.config.width = 45.0;
        widget_bar
            .widgets
            .push(egui_charts::ui::widget_bar::WidgetBarWidget {
                id: 1,
                icon: egui_charts::icons::icons::WATCHLIST,
                tooltip: "Data Window".into(),
                section: egui_charts::ui::widget_bar::WidgetBarSection::Top,
            });
        let mut timeframe_toolbar = TimeframeToolbar::default();
        timeframe_toolbar.config.active_bg = egui::Color32::TRANSPARENT;
        timeframe_toolbar.state.timeframe = timeframe;
        timeframe_toolbar.set_session(SessionType::Full24x7);

        let mut app = Self {
            chart,
            live,
            colour_drag: false,
            footprint: None,
            footprint_scale: None,
            feed_before_footprint: false,
            menus: ChartMenus::default(),
            state: DemoAppState {
                symbol: symbol.into(),
                timeframe,
                chart_type: ChartType::Candles,
                replay_active: false,
            },
            bars: Vec::new(),
            theme,
            top_toolbar,
            drawing_toolbar: DrawingToolbar::default(),
            timeframe_toolbar,
            widget_bar,
            indicator_dialog: IndicatorDialog::new(),
            indicator_settings: IndicatorSettingsDialog::default(),
            settings_dialog: SettingsDialog::new(),
            settings_state: ChartSettingsState::default(),
            symbol_search: SymbolSearchDialog::new(),
            indicator_templates: Vec::new(),
            quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler::new(
                commands::quick_search_commands(),
            ),
            alert_dialog: AlertDialog::new(),
            series_settings: SeriesSettingsDialog::new(),
            drawing_props: DrawingPropertiesDialog::new(),
            keyboard_shortcuts: KeyboardShortcutsDialog::new(),
            interval_entry: None,
            interval_focus: false,
            copied_drawing: None,
            export_dialog: ExportDialog::new().with_formats(EXPORT_FORMATS),
            date_picker: DatePicker::new(),
            object_tree: ObjectTreePanel::new(),
            open_panel: None,
            data_window_expanded: true,
            alerts: Vec::new(),
            next_alert_id: 1,
            replay: None,
            live_was_paused: false,
            replay_control: FloatingReplayControl::new(),
            screenshot_requested: None,
            screenshots: mpsc::channel(),
            notice: None,
        };
        app.apply_session();
        app
    }

    /// Surface a short-lived, honest status message in the UI.
    fn notify(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), 0.0));
    }

    // --- data helpers ------------------------------------------------------

    fn switch_timeframe(&mut self, tf: Timeframe) {
        self.copied_drawing = None;
        self.state.timeframe = tf;
        self.chart.set_timeframe(tf);
        self.regenerate_bars();
        self.timeframe_toolbar.state.timeframe = tf;
        // A replay continues at its current time on the new interval.
        if let Some(replay) = self.replay.as_mut() {
            let ts = self.bars.iter().map(|b| b.time).collect();
            replay.load_data(self.bars.clone(), ts, self.state.symbol.clone());
            self.push_replay_window_to_chart();
        }
    }

    pub(crate) fn switch_symbol(&mut self, symbol: &str) {
        self.copied_drawing = None;
        let spec = DemoSymbol::find(symbol);
        self.live.set_symbol(spec.symbol);
        self.timeframe_toolbar.state.session_menu_open = false;
        self.state.symbol = spec.symbol.to_string();
        self.chart.set_symbol(spec.symbol);
        for control in [
            egui_charts::ui::Control::Session,
            egui_charts::ui::Control::DividendAdjustment,
        ] {
            self.timeframe_toolbar
                .set_control_visible(control, spec.symbol == "SAMPLE");
        }
        self.timeframe_toolbar
            .set_session(if matches!(spec.exchange, "Crypto" | "Forex") {
                SessionType::Full24x7
            } else {
                SessionType::RTH
            });
        self.regenerate_bars();
        // A symbol change invalidates an in-flight replay.
        self.stop_replay();
    }

    fn regenerate_bars(&mut self) {
        self.apply_session();
        self.size_price_based_charts();
    }

    /// The sizes of the price-based charts are in price units: take them from the bars on show.
    fn size_price_based_charts(&mut self) {
        let chart = &mut self.chart;
        let size = RenkoConfig::from_atr(chart.bars(SeriesId::MAIN), 14, 0.5).brick_size;
        if size > 0.0 && size.is_finite() {
            let mut params = chart.chart_type_params().clone();
            params.renko_brick_size = size;
            params.kagi_reversal = size;
            params.range_size = size;
            params.pnf_box_size = size;
            chart.set_chart_type_params(params);
        }
    }

    fn apply_session(&mut self) {
        self.live.set_view(
            self.timeframe_toolbar.state.session_type,
            self.timeframe_toolbar.state.adjust_for_dividends,
        );
        self.chart.set_symbol("");
        self.chart.set_symbol(&self.state.symbol);
        self.chart
            .set_extended_hours(self.timeframe_toolbar.state.session_type != SessionType::RTH);
        let tf = self.state.timeframe;
        let clock = self.chart.bar_clock();
        self.bars = self.live.snapshot(tf, clock, self.chart.extended_hours());
        self.chart.set_data(SeriesId::MAIN, self.bars.clone());
        if self.footprint.is_some() {
            self.rebuild_footprint();
        }
        self.chart.set_instrument(Some(self.live.instrument()));
        self.chart
            .set_legend_statuses(&[egui_charts::widget::LegendStatus {
                text: "SIM",
                color: egui::Color32::from_rgb(38, 166, 154),
                background: egui::Color32::TRANSPARENT,
            }]);
        self.chart.set_quote(None);
        self.chart.set_events(Vec::new());
    }

    // --- replay ------------------------------------------------------------

    fn toggle_replay(&mut self) {
        if self.replay.is_some() && self.replay_control.is_user_dismissed() {
            self.replay_control.show_control();
        } else if self.replay.is_some() {
            self.stop_replay();
        } else {
            self.start_replay();
        }
    }

    fn start_replay(&mut self) {
        self.live_was_paused = if self.footprint.is_some() {
            self.feed_before_footprint
        } else {
            self.live.paused()
        };
        self.live.set_paused(true);
        self.chart.set_feed_paused(true);
        self.bars = self.chart.bars(SeriesId::MAIN).to_vec();
        let ts: Vec<_> = self.bars.iter().map(|b| b.time).collect();
        let mut controller =
            ReplayController::new(self.bars.clone(), ts).with_symbol(self.state.symbol.clone());
        controller.handle_action(ReplayAction::EnterReplayMode);
        // Begin a third of the way in so there is room to step both directions.
        controller.handle_action(ReplayAction::JumpToPercent(0.33));
        controller.handle_action(ReplayAction::Play);
        self.replay = Some(controller);
        self.state.replay_active = true;
        self.replay_control.set_symbol(&self.state.symbol);
        self.replay_control.show_control();
        self.push_replay_window_to_chart();
    }

    fn stop_replay(&mut self) {
        if self.replay.take().is_some() {
            self.chart.set_feed_paused(self.footprint.is_some());
            self.live
                .set_paused(self.footprint.is_some() || self.live_was_paused);
            self.state.replay_active = false;
            self.replay_control.hide_control();
            // Restore the full series the chart had before replay.
            self.chart.set_data(SeriesId::MAIN, self.bars.clone());
        }
    }

    /// Feed the replay controller's current visible window into the chart. This
    /// is the replay -> chart link the demo previously lacked entirely.
    fn push_replay_window_to_chart(&mut self) {
        if let Some(replay) = self.replay.as_ref() {
            let visible: Vec<Bar> = replay.visible_bars().to_vec();
            if !visible.is_empty() {
                self.chart.set_data(SeriesId::MAIN, visible);
            }
        }
    }

    // --- alerts ------------------------------------------------------------

    fn add_price_alert(&mut self, data: AlertData) {
        if let AlertData::Price {
            symbol,
            price,
            condition,
            message,
            repeating,
        } = data
        {
            self.alerts.push(PriceAlert {
                id: self.next_alert_id,
                symbol,
                target_price: price,
                condition: condition_from_label(&condition),
                status: AlertStatus::Active,
                message: (!message.is_empty()).then_some(message),
                repeating,
            });
            self.next_alert_id += 1;
        }
    }

    fn quick_price_alert(&mut self, symbol: String, price: f64) {
        self.alerts.push(PriceAlert {
            id: self.next_alert_id,
            symbol,
            target_price: price,
            condition: AlertCondition::CrossesAbove,
            status: AlertStatus::Active,
            message: None,
            repeating: false,
        });
        self.next_alert_id += 1;
    }

    // --- selection / context helpers --------------------------------------

    fn open_chart_settings_dialog(&mut self) {
        let config = self.chart.config();
        let settings = &mut self.settings_state;
        settings.timezone = self.timeframe_toolbar.state.timezone.clone();
        settings.candle_colors.body_up = config.bullish_color;
        settings.candle_colors.body_down = config.bearish_color;
        settings.candle_colors.border_up =
            config.bullish_border_color.unwrap_or(config.bullish_color);
        settings.candle_colors.border_down =
            config.bearish_border_color.unwrap_or(config.bearish_color);
        settings.candle_colors.wick_up = config
            .bullish_wick_color
            .unwrap_or(settings.candle_colors.border_up);
        settings.candle_colors.wick_down = config
            .bearish_wick_color
            .unwrap_or(settings.candle_colors.border_down);
        settings.chart_basic_styles.background_color = config.background_color;
        settings.grid_lines.horizontal_color = config.grid_color;
        settings.grid_lines.vertical_color = config.grid_color;
        settings.status_line.show_symbol = config.show_symbol_labels;
        settings.status_line.title_as_description = config.title_as_description;
        settings.status_line.show_quote_boxes = config.show_quote_boxes;
        settings.status_line.show_ohlc = config.show_ohlc_info;
        settings.status_line.show_change = config.show_ohlc_change;
        settings.status_line.show_change_percent = config.show_ohlc_change_percent;
        settings.status_line.show_volume = config.show_legend_volume;
        settings.status_line.show_indicator_titles = config.show_indicator_labels;
        settings.status_line.show_indicator_inputs = config.show_indicator_inputs;
        settings.status_line.show_indicator_values = config.show_indicator_last_val;
        settings.scales_and_lines.price_scale_left =
            !config.show_right_axis && config.show_left_axis;
        settings.scales_and_lines.show_last_price_label = config.show_symbol_last_val;
        settings.scales_and_lines.show_price_line = config.show_price_line;
        settings.scales_and_lines.show_prev_close_label = config.show_symbol_prev_close;
        settings.scales_and_lines.show_prev_close = config.show_prev_close_line;
        settings.scales_and_lines.show_countdown = config.show_countdown;
        let scale = self.chart.state().price_scale();
        settings.scales_and_lines.auto_scale = !scale.is_manual();
        settings.scales_and_lines.log_scale =
            scale.options().mode == egui_charts::scales::PriceScaleMode::Logarithmic;
        settings.scales_and_lines.percentage_scale =
            scale.options().mode == egui_charts::scales::PriceScaleMode::Percentage;
        settings.scales_and_lines.invert_scale = scale.options().invert_scale;
        self.settings_dialog.open(settings.clone());
    }

    fn open_series_settings_dialog(&mut self) {
        self.series_settings
            .open(SeriesId::MAIN, SeriesSettings::default());
    }

    fn open_drawing_props_for(&mut self, drawing_id: usize) {
        let dm = self.chart.drawings();
        let Some(drawing) = dm.drawings.iter().find(|d| d.id == drawing_id) else {
            return;
        };
        let props = DrawingProps {
            color: drawing.color32(),
            line_width: drawing.stroke_width,
            line_style: match drawing.line_style {
                egui_charts::drawings::LineStyle::Solid => DrawingLineStyle::Solid,
                egui_charts::drawings::LineStyle::Dashed => DrawingLineStyle::Dashed,
                egui_charts::drawings::LineStyle::Dotted => DrawingLineStyle::Dotted,
            },
            fill_color: drawing.fill_color32(),
            fill_enabled: drawing.fill_color.is_some(),
            extend_left: drawing.extend_left,
            extend_right: drawing.extend_right,
            font_size: drawing.font_size,
            visible_all_timeframes: drawing.timeframe_visibility.show_on_all,
            visible_timeframes: drawing
                .timeframe_visibility
                .visible_timeframes
                .iter()
                .cloned()
                .collect(),
            label: drawing.text.clone().unwrap_or_default(),
            ..DrawingProps::default()
        };
        let name = drawing.tool_type.as_str().to_string();
        self.drawing_props.open(DrawingId(drawing_id), name, props);
    }

    fn open_indicator_settings(&mut self, id: SeriesId) {
        if let Some((_, study)) = self
            .chart
            .indicators()
            .find(|(candidate, _)| *candidate == id)
        {
            self.indicator_settings
                .open(id, study, self.chart.timeframe());
        }
    }

    // --- dialog wiring -----------------------------------------------------

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.quick_search.show(ctx) {
            self.execute_search_command(ctx, &id);
        }
        // Indicators: add the study the dialog created.
        if let IndicatorDialogAction::AddIndicator(study) =
            self.indicator_dialog.show(ctx, &self.theme)
        {
            let inputs = self.indicator_dialog.input_summary(study.name());
            let id = self.chart.add_indicator(study);
            self.chart.set_indicator_inputs(id, inputs);
        }

        if let Some(edit) = self.indicator_settings.show(ctx, &self.theme) {
            let result = self
                .chart
                .indicator_mut(edit.id)
                .ok_or("Study was removed")
                .and_then(|study| {
                    study.apply_settings(&edit.inputs, edit.settings)?;
                    Ok(study
                        .inputs()
                        .iter()
                        .map(|input| match (&input.kind, &input.value) {
                            (
                                egui_charts::studies::InputKind::Choice(choices),
                                egui_charts::studies::InputValue::Choice(index),
                            ) => choices.get(*index).cloned().unwrap_or_default(),
                            (_, egui_charts::studies::InputValue::Length(value)) => {
                                value.to_string()
                            }
                            (_, egui_charts::studies::InputValue::Integer(value)) => {
                                value.to_string()
                            }
                            (_, egui_charts::studies::InputValue::Number(value)) => {
                                value.to_string()
                            }
                            (_, egui_charts::studies::InputValue::Toggle(value)) => {
                                value.to_string()
                            }
                            _ => String::new(),
                        })
                        .collect::<Vec<_>>()
                        .join(" "))
                });
            match result {
                Ok(inputs) => {
                    self.chart.set_indicator_inputs(edit.id, inputs);
                }
                Err(error) => self.notify(error),
            }
        }

        // Chart settings.
        match self.settings_dialog.show(ctx, &self.theme) {
            SettingsAction::Apply(state) => {
                self.settings_state = state.clone();
                apply_settings_state(&state, &mut self.chart, &mut self.timeframe_toolbar);
                let scale = self.chart.price_scale_mut();
                let mode = if state.scales_and_lines.log_scale {
                    egui_charts::scales::PriceScaleMode::Logarithmic
                } else if state.scales_and_lines.percentage_scale {
                    egui_charts::scales::PriceScaleMode::Percentage
                } else {
                    egui_charts::scales::PriceScaleMode::Normal
                };
                scale.options_mut().mode = mode;
                scale.options_mut().invert_scale = state.scales_and_lines.invert_scale;
                if state.scales_and_lines.auto_scale {
                    scale.reset();
                } else {
                    let range = scale.range();
                    scale.set_range(range.min, range.max);
                }
            }
            SettingsAction::Cancel => {}
            _ => {}
        }

        // Symbol search (used by the pill, quick search, and compare entry).
        let symbols = DemoSymbol::symbols();
        self.symbol_search
            .set_recent_symbols(vec![self.state.symbol.clone()]);
        if let Some(picked) = self.symbol_search.show(ctx, &symbols) {
            self.switch_symbol(&picked.name);
        }

        // Alert creation.
        match self.alert_dialog.show(ctx) {
            AlertDialogAction::Create(data) => self.add_price_alert(data),
            AlertDialogAction::Cancel | AlertDialogAction::None => {}
        }

        // Series settings.
        match self.series_settings.show(ctx) {
            SeriesSettingsAction::Apply(_id, settings) => {
                self.chart.apply_series_settings(&settings);
            }
            SeriesSettingsAction::None | SeriesSettingsAction::Cancel => {}
        }

        // Drawing properties.
        match self.drawing_props.show(ctx) {
            DrawingPropertiesAction::Apply(id, props) => self.apply_drawing_props(id, props),
            DrawingPropertiesAction::Delete(id) => {
                self.chart.drawings_mut().delete_drawing(id.0);
            }
            DrawingPropertiesAction::Clone(id) => self.clone_drawing(id.0),
            DrawingPropertiesAction::Cancel | DrawingPropertiesAction::None => {}
        }

        // Keyboard shortcuts help.
        let _ = self
            .keyboard_shortcuts
            .show_with_shortcuts(ctx, self.chart.shortcuts());
        self.show_interval_entry(ctx);

        // Export dialog (screenshot / CSV).
        match self.export_dialog.show(ctx) {
            ExportResponse::Export { config, save_path } => self.run_export(ctx, config, save_path),
            ExportResponse::Closed | ExportResponse::None => {}
        }

        // Date picker (opened from the bottom toolbar): jump the visible window.
        match self.date_picker.show(ctx) {
            DatePickerAction::GoToDate(date) => self.go_to_date(date),
            DatePickerAction::SetRange(start, _end) => self.go_to_date(start),
            DatePickerAction::Cancel | DatePickerAction::None => {}
        }
    }

    fn remove_all_indicators(&mut self) {
        let chart = &mut self.chart;
        let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
        for id in ids {
            chart.remove_series(id);
        }
    }

    fn apply_drawing_props(&mut self, id: DrawingId, props: DrawingProps) {
        self.chart.drawings_mut().edit_drawing(id.0, |drawing| {
            drawing.color = props.color.to_srgba_unmultiplied();
            drawing.stroke_width = props.line_width;
            drawing.line_style = line_style_from_dialog(props.line_style);
            drawing.extend_left = props.extend_left;
            drawing.extend_right = props.extend_right;
            drawing.text = (!props.label.is_empty()).then_some(props.label);
            drawing.fill_color = props
                .fill_color
                .filter(|_| props.fill_enabled)
                .map(|c| c.to_srgba_unmultiplied());
            drawing.font_size = props.font_size;
            drawing.timeframe_visibility = egui_charts::drawings::TimeframeVisibility {
                show_on_all: props.visible_all_timeframes,
                visible_timeframes: props.visible_timeframes.into_iter().collect(),
            };
        });
    }

    fn set_all_drawings_visible(&mut self, visible: bool) {
        let dm = self.chart.drawings_mut();
        dm.set_visibility_all(visible);
    }

    fn clone_drawing(&mut self, id: usize) {
        let dm = self.chart.drawings_mut();
        if let Some(src) = dm.drawings.iter().find(|d| d.id == id).cloned() {
            let mut copy = src;
            for cp in &mut copy.chart_points {
                cp.bar_idx += 3.0; // offset so the clone is visible
            }
            dm.add_synced_drawing(copy);
        }
    }

    fn go_to_date(&mut self, date: chrono::NaiveDate) {
        let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let target = self
            .bars
            .iter()
            .find(|bar| bar.time >= start)
            .or_else(|| self.bars.last())
            .map_or(start, |bar| bar.time);
        self.chart.scroll_to_time(target);
    }

    fn run_export(
        &mut self,
        ctx: &egui::Context,
        config: egui_charts::ui::export::ExportConfig,
        save_path: Option<std::path::PathBuf>,
    ) {
        // Image exports go through egui's screenshot request, which hands the
        // captured image to `target`.
        let target = match config.format {
            ExportFormat::Png => ScreenshotTarget::File(save_path),
            ExportFormat::Clipboard => ScreenshotTarget::Clipboard,
            ExportFormat::Csv => return self.export_csv(save_path),
        };
        // Capture the next frame after the dialog and any previous notice are gone.
        self.export_dialog.close();
        self.notice = None;
        self.screenshot_requested = Some(target);
        ctx.request_repaint();
    }

    fn export_csv(&mut self, save_path: Option<std::path::PathBuf>) {
        let from_dialog = save_path.is_some();
        let mut out = String::from("time,open,high,low,close,volume\n");
        for b in &self.bars {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                b.time.to_rfc3339(),
                b.open,
                b.high,
                b.low,
                b.close,
                b.volume
            ));
        }
        let path = save_path.unwrap_or_else(|| {
            let filename = format!("{}_export.csv", self.state.symbol);
            #[cfg(not(target_arch = "wasm32"))]
            return std::env::temp_dir().join(filename);
            #[cfg(target_arch = "wasm32")]
            std::path::PathBuf::from(filename)
        });
        #[cfg(not(target_arch = "wasm32"))]
        let result = std::fs::write(&path, out).map_err(|e| e.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = browser_download(out.as_bytes(), &path);
        match result {
            Ok(()) => {
                if from_dialog {
                    self.export_dialog.mark_success();
                }
                self.notice = Some((format!("Export successful: {}", path.display()), 0.0));
            }
            Err(error) => {
                if from_dialog {
                    self.export_dialog.mark_error(error.clone());
                }
                self.notice = Some((format!("Export failed: {error}"), 0.0));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App: per-frame update that drives the panels, toolbars and dialogs.
// ---------------------------------------------------------------------------

impl eframe::App for DemoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui_charts::theme::apply_to_egui(&ctx, &self.theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ctx.global_style());

        // Fulfil requests from the previous frame, before a dialog can submit another.
        self.handle_screenshot(&ctx);

        // Drive replay before laying out the chart so its window is current.
        self.advance_replay();
        if self.replay.is_none() {
            let delay = self.live.advance(chrono::Utc::now());
            if !self.live.paused() {
                ctx.request_repaint_after(delay);
            }
        }

        // A colour picker drag ends, as one edit, when the pointer is released.
        if self.colour_drag && !ctx.input(|i| i.pointer.any_down()) {
            self.colour_drag = false;
            self.chart.drawings_mut().end_drag_handle();
        }

        let panel_bg = ui.style().visuals.panel_fill;
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, self.theme.ui.border);

        // Dialogs consume their keys before chart commands and the replay transport.
        let dialog_open = self.keyboard_shortcuts.is_open
            || self.quick_search.is_open()
            || self.symbol_search.is_open()
            || self.indicator_dialog.is_open()
            || self.settings_dialog.is_open
            || self.drawing_props.is_open
            || self.export_dialog.is_open()
            || self.alert_dialog.is_open
            || self.series_settings.is_open
            || self.indicator_settings.is_open()
            || self.interval_entry.is_some()
            || self.date_picker.is_open;
        if dialog_open {
            self.chart.suppress_shortcuts(&ctx);
        } else {
            self.handle_keyboard(&ctx);
        }
        self.show_top_toolbar(ui, panel_bg);
        self.show_drawing_toolbar(ui, panel_bg);
        self.show_widget_bar(ui, panel_bg);
        self.show_side_panel(ui);
        self.show_bottom_toolbar(ui, panel_bg);
        self.show_chart(ui, panel_bg);
        self.show_replay_control(&ctx);
        self.show_dialogs(&ctx);

        // Transient status notice.
        self.show_notice(&ctx);

        // Repaint continuously while replay is playing.
        if self
            .replay
            .as_ref()
            .map(|r| r.state.is_playing())
            .unwrap_or(false)
        {
            ctx.request_repaint();
        }
    }
}

// ---------------------------------------------------------------------------
// Per-region rendering + action routing.
// ---------------------------------------------------------------------------

impl DemoApp {
    fn show_top_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        let drawings = self.chart.drawings();
        self.top_toolbar.state.can_undo = drawings.can_undo();
        self.top_toolbar.state.can_redo = drawings.can_redo();
        egui::Panel::top("toolbar")
            .exact_size(42.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                bottom: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                use egui_charts::icons::icons;
                use egui_charts::ui_kit::buttons::IconButton;
                let mut menu_action = TopToolbarAction::None;
                let mut search = false;
                let mut trade = false;
                let mut publish = false;
                let mut footprint_enabled = self.footprint.is_some();
                let mut paused = self.live.paused();
                let action = self.top_toolbar.show_with_slots(
                    ui,
                    &self.state,
                    |ui| {
                        let response = IconButton::new(&icons::MENU).tooltip("Menu").show(ui);
                        egui::Popup::menu(&response).show(|ui| {
                            ui.checkbox(&mut footprint_enabled, "Footprint (synthetic)")
                                .on_hover_text("Pause the feed to inspect the footprint sample");
                            for (label, action) in [
                                ("Quick search", TopToolbarAction::OpenSymbolSearch),
                                ("Switch theme", TopToolbarAction::ThemeToggle),
                            ] {
                                if ui.button(label).clicked() {
                                    menu_action = action;
                                    ui.close();
                                }
                            }
                        });
                    },
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add_space(8.0);
                        if egui_charts::ui_kit::buttons::Button::new(if paused {
                            "Resume feed"
                        } else {
                            "Pause feed"
                        })
                        .small()
                        .ghost()
                        .enabled(self.replay.is_none() && self.footprint.is_none())
                        .tooltip("Synthetic executions")
                        .show(ui)
                        .clicked()
                        {
                            paused = !paused;
                        }
                        publish = egui_charts::ui::top_toolbar::buttons::toolbar_tooltip(
                            ui.add(
                                egui::Button::new(
                                    egui::RichText::new("Publish")
                                        .size(14.0)
                                        .color(self.theme.ui.panel_bg),
                                )
                                .fill(self.theme.ui.text)
                                .min_size(egui::vec2(72.0, 28.0))
                                .corner_radius(16),
                            ),
                            "Export chart",
                            None,
                        )
                        .clicked();
                        trade = egui_charts::ui::top_toolbar::buttons::toolbar_tooltip(
                            ui.add(
                                egui::Button::new(egui::RichText::new("Trade").size(14.0))
                                    .fill(bg)
                                    .stroke(egui::Stroke::new(1.0, self.theme.ui.border))
                                    .min_size(egui::vec2(60.0, 28.0))
                                    .corner_radius(16),
                            ),
                            "Trading account",
                            None,
                        )
                        .clicked();
                        search = egui_charts::ui::top_toolbar::buttons::toolbar_tooltip(
                            IconButton::new(&icons::QUICK_SEARCH)
                                .icon_scale(24.0 / 28.0)
                                .show(ui),
                            "Quick search",
                            Some(egui::KeyboardShortcut::new(
                                egui::Modifiers::COMMAND,
                                egui::Key::K,
                            )),
                        )
                        .clicked();
                    },
                );
                self.live.set_paused(paused);
                if footprint_enabled != self.footprint.is_some() {
                    if footprint_enabled {
                        self.feed_before_footprint = self.live.paused();
                        self.live.set_paused(true);
                        self.chart.set_feed_paused(true);
                        self.footprint_scale = Some(self.chart.get_time_scale_state());
                        self.rebuild_footprint();
                        if self.footprint.is_some() {
                            self.chart.apply_synced_time_scale(100.0, 0.0);
                        }
                    } else {
                        self.footprint = None;
                        self.restore_feed_after_footprint();
                        if let Some((spacing, offset)) = self.footprint_scale.take() {
                            self.chart.apply_synced_time_scale(spacing, offset);
                        }
                    }
                }
                self.handle_top_toolbar(ui.ctx(), action);
                self.handle_top_toolbar(ui.ctx(), menu_action);
                if search {
                    self.quick_search.open();
                }
                if trade {
                    self.notify(
                        "Connect a trading account to place orders; the demo uses synthetic data",
                    );
                }
                if publish {
                    self.export_dialog.open();
                }
            });
    }

    pub(crate) fn handle_top_toolbar(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::ChartStyleSelected(name) => {
                if let Some(ct) = chart_type_from_name(&name) {
                    self.state.chart_type = ct;
                    self.chart.set_chart_type(ct);
                    self.size_price_based_charts();
                }
            }
            TopToolbarAction::IntervalSelected(tf) => self.switch_timeframe(tf),
            TopToolbarAction::OpenSymbolSearch | TopToolbarAction::OpenCompare => {
                self.symbol_search.open();
            }
            TopToolbarAction::OpenIndicators => self.indicator_dialog.open(),
            TopToolbarAction::CreateAlert => self.alert_dialog.open(self.state.symbol.clone()),
            TopToolbarAction::ToggleReplay => self.toggle_replay(),
            TopToolbarAction::OpenSettings => {
                self.open_chart_settings_dialog();
            }
            TopToolbarAction::TakeScreenshot => {
                self.export_dialog.close();
                self.notice = None;
                self.screenshot_requested = Some(ScreenshotTarget::File(None));
                ctx.request_repaint();
            }
            TopToolbarAction::ToggleFullscreen => {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
            }
            TopToolbarAction::Undo => {
                self.chart.drawings_mut().undo();
            }
            TopToolbarAction::Redo => {
                self.chart.drawings_mut().redo();
            }
            TopToolbarAction::SaveIndicatorTemplate => {
                let name = format!("Study set {}", self.indicator_templates.len() + 1);
                let studies = self
                    .chart
                    .indicators()
                    .map(|(_, study)| study.clone_box())
                    .collect();
                let inputs = self
                    .chart
                    .indicators()
                    .map(|(id, _)| {
                        self.chart
                            .indicator_inputs(id)
                            .unwrap_or_default()
                            .to_owned()
                    })
                    .collect();
                self.indicator_templates.push(SavedIndicators {
                    name: name.clone(),
                    studies,
                    inputs,
                });
                self.top_toolbar
                    .indicator_templates
                    .push((name.clone(), name));
            }
            TopToolbarAction::ApplyIndicatorTemplate(id) => {
                if let Some(saved) = self
                    .indicator_templates
                    .iter()
                    .find(|saved| saved.name == id)
                {
                    let studies = saved.studies.clone();
                    let inputs = saved.inputs.clone();
                    self.remove_all_indicators();
                    for (study, inputs) in studies.into_iter().zip(inputs) {
                        let id = self.chart.add_indicator(study);
                        self.chart.set_indicator_inputs(id, inputs);
                    }
                }
            }
            // Save routes through the export dialog (PNG / CSV of the view).
            TopToolbarAction::Save => self.export_dialog.open(),
            // Light/dark toggle is a real, cheap visual change.
            TopToolbarAction::ThemeToggle => self.toggle_theme(),
            // The layout-setup button opens the chart-object panel in this
            // single-chart context.
            TopToolbarAction::Layout(_) => {
                self.open_panel = Some(SidePanel::ObjectTree);
            }
            TopToolbarAction::None => {}
        }
    }

    fn toggle_theme(&mut self) {
        // Swap the demo theme; `apply_to_egui` repaints every panel, toolbar,
        // and dialog from it next frame, and the chart background/grid follow
        // the matching config colors.
        self.theme = if self.theme.dark {
            Theme::light()
        } else {
            Theme::dark()
        };
        let cfg = self.chart.config_mut();
        cfg.background_color = self.theme.chart.bg;
        cfg.text_color = self.theme.chart.axis_text;
        cfg.grid_color = self.theme.chart.grid_line;
    }

    fn show_drawing_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        let drawings = &self.chart.drawings().drawings;
        self.drawing_toolbar.state.lock_all =
            !drawings.is_empty() && drawings.iter().all(|d| d.locked);
        egui::Panel::left("drawing_toolbar")
            .resizable(false)
            .exact_size(56.0)
            .show_separator_line(false)
            .frame(
                panel_frame(bg)
                    .corner_radius(egui::CornerRadius {
                        ne: 4,
                        ..egui::CornerRadius::ZERO
                    })
                    .outer_margin(egui::Margin {
                        right: 4,
                        ..egui::Margin::ZERO
                    }),
            )
            .show(ui, |ui| {
                let action = self.drawing_toolbar.show_with_action(ui);
                self.handle_drawing_toolbar(action);
            });
    }

    pub(crate) fn handle_drawing_toolbar(&mut self, action: DrawingToolbarAction) {
        // Variants that touch the indicator registry (not the drawing manager)
        // or surface a notice are handled first, before the `dm` borrow.
        match action {
            DrawingToolbarAction::RemoveAllIndicators | DrawingToolbarAction::RemoveAllStudies => {
                self.remove_all_indicators();
                return;
            }
            DrawingToolbarAction::HideAllIndicators => {
                let chart = &mut self.chart;
                let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    chart.set_indicator_visible(id, false);
                }
                return;
            }
            DrawingToolbarAction::SaveTemplate { .. }
            | DrawingToolbarAction::LoadTemplate { .. } => {
                self.notify("Drawing templates are not persisted in this demo");
                return;
            }
            DrawingToolbarAction::HideAllPoss | DrawingToolbarAction::HideAllOrders => {
                self.notify(
                    "Positions and orders need a trading account, which the demo does not have",
                );
                return;
            }
            _ => {}
        }

        match action {
            DrawingToolbarAction::SetCursorType(cursor) => {
                use egui_charts::config::CrosshairStyle;
                use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
                self.chart.set_crosshair_style(match cursor {
                    CursorType::Cross => CrosshairStyle::Full,
                    CursorType::Dot => CrosshairStyle::Dot,
                    CursorType::Arrow | CursorType::Eraser => CrosshairStyle::Arrow,
                    CursorType::Demonstration => CrosshairStyle::Demonstration,
                });
                self.chart.drawings_mut().set_active_tool(None);
                self.chart
                    .drawings_mut()
                    .set_eraser_mode(cursor == CursorType::Eraser);
                return;
            }
            DrawingToolbarAction::ToggleValuesTooltip => {
                self.chart
                    .set_tooltip_enabled(self.drawing_toolbar.state.values_tooltip_on_long_press);
                return;
            }
            _ => {}
        }

        let dm = self.chart.drawings_mut();
        match action {
            DrawingToolbarAction::SelectTool(tool) => dm.set_active_tool(Some(tool)),
            DrawingToolbarAction::ToggleMagnet => {
                dm.options.magnet_mode = !dm.options.magnet_mode;
                dm.options.strong_magnet = self.drawing_toolbar.state.magnet_type
                    != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
            }
            DrawingToolbarAction::SetMagnetType(kind) => {
                dm.options.magnet_mode = true;
                dm.options.strong_magnet =
                    kind != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
            }
            DrawingToolbarAction::ToggleEraserMode => {
                let on = !dm.eraser_mode;
                dm.set_eraser_mode(on);
            }
            DrawingToolbarAction::ToggleStayInDrawingMode => {
                dm.stay_in_drawing_mode = !dm.stay_in_drawing_mode;
            }
            DrawingToolbarAction::ColorChanged(rgba) => dm.options.default_color = rgba,
            DrawingToolbarAction::ClearAllDrawings | DrawingToolbarAction::RemoveAllDrawings => {
                dm.clear_all()
            }
            DrawingToolbarAction::HideAllDrawings => dm.toggle_visibility_all(),
            DrawingToolbarAction::LockAllDrawings => dm.toggle_lock_all(),
            DrawingToolbarAction::ZoomIn => {
                let _ = dm.redo();
            }
            DrawingToolbarAction::ZoomOut => {
                let _ = dm.undo();
            }
            DrawingToolbarAction::InsertEmoji { icon_name } => {
                dm.start_icon_insertion(icon_name.to_string())
            }
            // The rest (the values tooltip toggle and
            // OpenTemplateMenu) mutate only the toolbar's own popup state, which
            // it manages internally; there is nothing for the host to do.
            _ => {}
        }
    }

    fn show_widget_bar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        egui::Panel::right("widget_bar")
            .resizable(false)
            .exact_size(49.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                left: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                let action = self.widget_bar.show(ui);
                match action {
                    WidgetBarAction::Clicked(icon) => self.toggle_panel(icon),
                    WidgetBarAction::WidgetClicked(1) => {
                        self.open_panel = if self.open_panel == Some(SidePanel::DataWindow) {
                            None
                        } else {
                            Some(SidePanel::DataWindow)
                        };
                    }
                    _ => {}
                }
            });
    }

    fn toggle_panel(&mut self, icon: RightBarIcon) {
        let target = match icon {
            RightBarIcon::ObjectTree => Some(SidePanel::ObjectTree),
            RightBarIcon::Alerts => Some(SidePanel::Alerts),
            RightBarIcon::Help => {
                self.keyboard_shortcuts.toggle();
                None
            }
        };
        if let Some(target) = target {
            self.open_panel = if self.open_panel == Some(target) {
                None
            } else {
                Some(target)
            };
        }
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui) {
        let Some(which) = self.open_panel else {
            return;
        };
        egui::Panel::right("detail_panel")
            .resizable(true)
            .default_size(280.0)
            .min_size(220.0)
            .show(ui, |ui| match which {
                SidePanel::ObjectTree => self.show_object_tree(ui),
                SidePanel::Alerts => self.show_alerts_panel(ui),
                SidePanel::DataWindow => {
                    self.data_window_expanded =
                        egui_charts::ui::widget_bar::panels::object_tree::show_data_window(
                            ui,
                            &self.build_data_window(),
                            self.data_window_expanded,
                        );
                }
            });
    }

    fn show_object_tree(&mut self, ui: &mut egui::Ui) {
        // Build the source list from live chart state each frame.
        let mut sources = self.build_source_items();
        let data_window = self.build_data_window();
        let action = self.object_tree.show(ui, Some(&data_window), &mut sources);
        self.handle_object_tree(action, &sources);
    }

    fn build_source_items(&self) -> Vec<SourceItem> {
        let mut items = Vec::new();

        // Main data source row.
        let tf = self.state.timeframe.as_str();
        items.push(SourceItem::data_source(
            MAIN_SERIES_ID,
            format!("{} · {}", self.state.symbol, tf),
        ));

        // Live indicators.
        for (id, ind) in self.chart.indicators() {
            let color = ind.colors().first().copied().unwrap_or(egui::Color32::GRAY);
            let mut item = SourceItem::indicator(indicator_tree_id(id), ind.name(), color);
            item.visible = ind.is_visible();
            items.push(item);
        }

        // Live drawings.
        for d in &self.chart.drawings().drawings {
            let mut item = SourceItem::drawing(d.id, d.tool_type, d.color32());
            item.visible = d.visible;
            item.locked = d.locked;
            items.push(item);
        }
        items
    }

    fn build_data_window(&self) -> DataWindowInfo {
        let bars = self.chart.state().shown();
        let index = self
            .chart
            .get_hover_bar_idx()
            .filter(|index| index.is_finite() && index.round() >= 0.0)
            .map(|index| index.round() as usize)
            .filter(|index| *index < bars.len())
            .unwrap_or_else(|| bars.len().saturating_sub(1));
        let last = bars.get(index);
        let mut info = DataWindowInfo::new(self.state.symbol.clone())
            .with_timeframe(self.state.timeframe.as_str().to_string());
        if let Some(b) = last {
            info = info.with_ohlcv(b.open, b.high, b.low, b.close, b.volume);
            info = info.with_timestamp(self.chart.format_time(b.time));
            if let Some(previous) = index.checked_sub(1).and_then(|index| bars.get(index)) {
                info = info.with_prev_close(previous.close);
            }
        }
        info
    }

    fn handle_object_tree(&mut self, action: ObjectTreeAction, sources: &[SourceItem]) {
        match action {
            ObjectTreeAction::ToggleVisibility(id) => {
                let chart = &mut self.chart;
                if let Some(study) = indicator_from_tree_id(id) {
                    let now = chart
                        .indicators()
                        .find(|(id, _)| *id == study)
                        .is_none_or(|(_, i)| i.is_visible());
                    chart.set_indicator_visible(study, !now);
                } else {
                    chart
                        .drawings_mut()
                        .edit_drawing(id, |d| d.visible = !d.visible);
                }
            }
            ObjectTreeAction::ToggleLock(id) => {
                self.chart
                    .drawings_mut()
                    .edit_drawing(id, |d| d.locked = !d.locked);
            }
            ObjectTreeAction::Delete(id) | ObjectTreeAction::RemoveIndicator(id) => {
                let chart = &mut self.chart;
                if let Some(study) = indicator_from_tree_id(id) {
                    chart.remove_series(study);
                } else {
                    chart.drawings_mut().delete_drawing(id);
                }
            }
            ObjectTreeAction::Select(id) => {
                if indicator_from_tree_id(id).is_none() {
                    self.chart.drawings_mut().select(id);
                }
            }
            ObjectTreeAction::ChangeColor(id, color) => {
                let chart = &mut self.chart;
                if let Some(study) = indicator_from_tree_id(id) {
                    if let Some(ind) = chart.indicator_mut(study) {
                        ind.set_color(0, color);
                    }
                } else {
                    chart
                        .drawings_mut()
                        .drag_edit_drawing(id, |d| d.color = color.to_srgba_unmultiplied());
                    self.colour_drag = true;
                }
            }
            ObjectTreeAction::OpenProperties(id) | ObjectTreeAction::OpenIndicatorSettings(id) => {
                if let Some(study) = indicator_from_tree_id(id) {
                    self.open_indicator_settings(study);
                } else {
                    self.open_drawing_props_for(id);
                }
            }
            ObjectTreeAction::Duplicate(id) => {
                if indicator_from_tree_id(id).is_none() {
                    self.clone_drawing(id);
                }
            }
            ObjectTreeAction::BringToFront(id) => {
                self.chart.drawings_mut().bring_to_front(id);
            }
            ObjectTreeAction::SendToBack(id) => {
                self.chart.drawings_mut().send_to_back(id);
            }
            ObjectTreeAction::HideAll => self.set_all_drawings_visible(false),
            ObjectTreeAction::ShowAll => self.set_all_drawings_visible(true),
            ObjectTreeAction::RemoveAllDrawings => {
                self.chart.drawings_mut().clear_all();
            }
            ObjectTreeAction::RemoveAllIndicators => self.remove_all_indicators(),
            ObjectTreeAction::ClearSelection => {
                self.chart.drawings_mut().deselect();
            }
            _ => {
                let _ = sources;
            }
        }
    }

    fn show_alerts_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Alerts");
            if ui.button("+ New").clicked() {
                self.alert_dialog.open(self.state.symbol.clone());
            }
        });
        ui.separator();
        if self.alerts.is_empty() {
            ui.label("No alerts. Click + New, the bell button, or right-click the chart.");
            return;
        }
        let mut remove: Option<usize> = None;
        for alert in &self.alerts {
            ui.horizontal(|ui| {
                let dot = match alert.status {
                    AlertStatus::Active => egui::Color32::from_rgb(38, 166, 154),
                    AlertStatus::Triggered => egui::Color32::from_rgb(239, 83, 80),
                    _ => egui::Color32::GRAY,
                };
                ui.colored_label(dot, "●");
                ui.label(format!(
                    "{} {} {:.2}",
                    alert.symbol,
                    alert.condition.as_str(),
                    alert.target_price
                ));
                if ui.small_button("✕").clicked() {
                    remove = Some(alert.id);
                }
            });
        }
        if let Some(id) = remove {
            self.alerts.retain(|a| a.id != id);
        }
    }

    fn show_bottom_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        egui::Panel::bottom("bottom")
            .exact_size(39.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).corner_radius(4).outer_margin(egui::Margin {
                top: 1,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                let action = self.timeframe_toolbar.show(ui);
                self.handle_timeframe_toolbar(action);
                self.show_timeframe_menus(ui.ctx());
            });
    }

    fn handle_timeframe_toolbar(&mut self, action: TimeframeToolbarAction) {
        match action {
            TimeframeToolbarAction::DateRangeChanged(range) => {
                let visible = visible_bars_for_range(range, self.chart.bars(SeriesId::MAIN));
                let width = self.chart.get_rendered_price_rect().width();
                // Include the latest slot in the scale's exclusive data range.
                let spacing = width.max(1.0) / (visible.max(1) as f32 + 0.5);
                self.chart.disable_tracking_mode();
                self.chart.apply_synced_time_scale(spacing, 0.5);
            }
            TimeframeToolbarAction::OpenDatePicker => self.date_picker.open(),
            TimeframeToolbarAction::OpenTimezoneMenu => {
                self.timeframe_toolbar.state.timezone_menu_open = true;
            }
            TimeframeToolbarAction::OpenSessionMenu => {
                self.timeframe_toolbar.state.session_menu_open = true;
            }
            TimeframeToolbarAction::AdjustDividendsToggled(adjusted) => {
                self.timeframe_toolbar.state.adjust_for_dividends = adjusted;
                self.stop_replay();
                self.apply_session();
            }
            TimeframeToolbarAction::None => {}
        }
    }

    fn show_timeframe_menus(&mut self, ctx: &egui::Context) {
        if self.timeframe_toolbar.state.session_menu_open {
            let mut open = true;
            egui::Window::new("Synthetic session")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    for session in [SessionType::RTH, SessionType::ETH] {
                        if ui
                            .selectable_label(
                                self.timeframe_toolbar.state.session_type == session,
                                session.tooltip(),
                            )
                            .clicked()
                        {
                            self.timeframe_toolbar.set_session(session);
                            self.timeframe_toolbar.state.session_menu_open = false;
                            self.stop_replay();
                            self.apply_session();
                        }
                    }
                    ui.label(
                        "SAMPLE: weekdays, regular 14:30–21:00 UTC; extended 08:30–21:00 UTC.",
                    );
                });
            self.timeframe_toolbar.state.session_menu_open &= open;
        }
        if self.timeframe_toolbar.state.timezone_menu_open {
            let mut open = true;
            egui::Window::new("Chart timezone")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    for label in ["UTC", "America/New_York"] {
                        if ui
                            .selectable_label(self.timeframe_toolbar.state.timezone == label, label)
                            .clicked()
                        {
                            apply_timezone(&mut self.chart, &mut self.timeframe_toolbar, label);
                            self.timeframe_toolbar.state.timezone_menu_open = false;
                        }
                    }
                });
            self.timeframe_toolbar.state.timezone_menu_open &= open;
        }
    }

    fn restore_feed_after_footprint(&mut self) {
        self.chart.set_feed_paused(self.replay.is_some());
        if self.replay.is_none() {
            self.live.set_paused(self.feed_before_footprint);
        }
    }

    fn rebuild_footprint(&mut self) {
        let bars = self.chart.bars(SeriesId::MAIN);
        let step = if bars.last().is_some_and(|bar| bar.close.abs() >= 100.0) {
            0.05
        } else {
            0.0001
        };
        let mut clock = egui_charts::model::BarClock::new(self.state.timeframe);
        if let Some(zone) = self.chart.chart_options().time_scale.timezone.to_tz() {
            clock = clock.with_timezone(zone);
        }
        let mut flow = egui_charts::data::Footprint::new(clock, step, 1)
            .expect("fixture has a positive price step");
        for trade in crate::sampledata::synthetic_trades(bars) {
            flow.push(&trade)
                .expect("fixture executions are finite and chronological");
        }
        if flow.bars().len() == bars.len() {
            self.footprint = Some(flow);
        } else {
            self.footprint = None;
            self.restore_feed_after_footprint();
            if let Some((spacing, offset)) = self.footprint_scale.take() {
                self.chart.apply_synced_time_scale(spacing, offset);
            }
            self.notify("Footprint example needs one source bar per selected interval");
        }
    }

    fn show_chart(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        let chart = egui::CentralPanel::default()
            .frame(panel_frame(bg).corner_radius(4))
            .show(ui, |ui| {
                if let Some(flow) = &self.footprint {
                    let config = egui_charts::chart::overlays::FootprintConfig {
                        text_color: self.theme.ui.text,
                        neutral_color: self.theme.ui.text,
                        ask_color: self.chart.config().bullish_color,
                        bid_color: self.chart.config().bearish_color,
                        ..Default::default()
                    };
                    self.chart.show_footprint(ui, flow, &config);
                    if self.chart.chart_type().is_time_independent() {
                        ui.painter().text(self.chart.get_rendered_price_rect().center(), egui::Align2::CENTER_CENTER,
                            "Footprint requires a time-based chart", egui::FontId::proportional(14.0), self.theme.ui.text);
                    }
                } else {
                    self.chart.show(ui);
                }
                if let Some(id) = self.chart.take_indicator_settings_request() {
                    self.open_indicator_settings(id);
                }
                if self.chart.take_scale_settings_request() {
                    self.open_series_settings_dialog();
                    self.series_settings.active_tab = egui_charts::ui::dialogs::series_settings::SeriesSettingsTab::ScalesAndLines;
                }
                if self.chart.take_event_click().is_some() {
                    self.notify("Synthetic session: generated example data");
                }
                if let Some(side) = self.chart.take_quote_action() {
                    self.notify(format!("{side:?} quote selected; synthetic data, no order submitted"));
                }
            });
        self.replay_control.set_dock_rect(chart.response.rect);
        // Right-click context actions resolve after the chart is shown.
        if let Some(action) = self.menus.show(ui.ctx(), &mut self.chart) {
            self.handle_context_action(action);
        }
    }

    fn handle_context_action(&mut self, action: ChartContextAction) {
        match action {
            ChartContextAction::Chart(a) => self.handle_chart_context(a),
            ChartContextAction::Series(a) => self.handle_series_context(a),
            ChartContextAction::Drawing(a) => self.handle_drawing_context(a),
        }
    }

    fn handle_chart_context(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::OpenSettings => {
                self.open_chart_settings_dialog();
            }
            ContextMenuAction::OpenSeriesSettings => self.open_series_settings_dialog(),
            ContextMenuAction::ObjectTree => self.open_panel = Some(SidePanel::ObjectTree),
            ContextMenuAction::RemoveAllIndicators => self.remove_all_indicators(),
            ContextMenuAction::AddAlert { symbol, price } => self.quick_price_alert(symbol, price),
            ContextMenuAction::ResetChart => {
                self.chart.apply_synced_time_scale(6.1, 64.0 / 6.1 - 0.5);
                self.chart.enable_tracking_mode();
            }
            ContextMenuAction::ExportCsv => self.export_csv(None),
            ContextMenuAction::CopyPrice(price) => {
                log::info!("[demo] copy price {price}");
            }
            // Sell limit and buy stop need a broker, which the demo does not have.
            _ => {}
        }
    }

    fn handle_series_context(&mut self, action: SeriesContextMenuAction) {
        match action {
            SeriesContextMenuAction::OpenSettings => self.open_series_settings_dialog(),
            SeriesContextMenuAction::AddIndicator => self.indicator_dialog.open(),
            SeriesContextMenuAction::AddAlert => self.alert_dialog.open(self.state.symbol.clone()),
            _ => {}
        }
    }

    fn handle_drawing_context(&mut self, action: DrawingContextMenuAction) {
        // The library selects the right-clicked drawing, so the selected id is
        // the menu's target.
        let Some(id) = self.chart.drawings().sel_drawing() else {
            return;
        };
        let dm = self.chart.drawings_mut();
        match action {
            DrawingContextMenuAction::Delete => dm.delete_drawing(id),
            DrawingContextMenuAction::Edit => self.open_drawing_props_for(id),
            DrawingContextMenuAction::Clone => self.clone_drawing(id),
            DrawingContextMenuAction::Lock => dm.edit_drawing(id, |d| d.locked = !d.locked),
            DrawingContextMenuAction::Hide => dm.edit_drawing(id, |d| d.visible = !d.visible),
            DrawingContextMenuAction::BringToFront => dm.bring_to_front(id),
            DrawingContextMenuAction::SendToBack => dm.send_to_back(id),
            DrawingContextMenuAction::None => {}
        }
    }

    fn show_replay_control(&mut self, ctx: &egui::Context) {
        if self.replay.is_none() {
            return;
        }
        // Sync the transport from the controller, then route its actions back.
        if let Some(replay) = self.replay.as_ref() {
            self.replay_control.sync_from(
                replay.state.is_active(),
                replay.state.is_playing(),
                replay.state.speed.multiplier(),
                replay.state.curr_bar,
                replay.state.total_bars,
            );
        }
        let action = self.replay_control.show(ctx);
        self.handle_replay_control(action);
    }

    fn handle_replay_control(&mut self, action: ReplayControlAction) {
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        match action {
            ReplayControlAction::Play => replay.handle_action(ReplayAction::Play),
            ReplayControlAction::Pause => replay.handle_action(ReplayAction::Pause),
            ReplayControlAction::StepForward | ReplayControlAction::NextBar => {
                replay.handle_action(ReplayAction::StepForward);
                self.push_replay_window_to_chart();
            }
            ReplayControlAction::StepBackward | ReplayControlAction::PreviousBar => {
                replay.handle_action(ReplayAction::StepBackward);
                self.push_replay_window_to_chart();
            }
            ReplayControlAction::GoToStart => {
                replay.handle_action(ReplayAction::JumpToStart);
                self.push_replay_window_to_chart();
            }
            ReplayControlAction::GoToEnd => {
                replay.handle_action(ReplayAction::JumpToEnd);
                self.push_replay_window_to_chart();
            }
            ReplayControlAction::SetSpeed(s) => replay.handle_action(ReplayAction::SetSpeed(s)),
            ReplayControlAction::Stop => self.stop_replay(),
            ReplayControlAction::Close => {
                self.replay_control.dismiss();
            }
            ReplayControlAction::None => {}
        }
    }

    fn advance_replay(&mut self) {
        let advanced = match self.replay.as_mut() {
            Some(replay) => replay.update(),
            None => return,
        };
        if advanced > 0 {
            self.push_replay_window_to_chart();
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        use egui_charts::config::KeyboardAction as A;
        use egui_charts::widget::ShortcutRequest;
        self.chart.dispatch_shortcuts(ctx);
        for request in self.chart.take_shortcut_requests() {
            match request {
                ShortcutRequest::PasteDrawing(text) => {
                    if self
                        .copied_drawing
                        .as_ref()
                        .is_some_and(|(copied, _)| *copied == text)
                    {
                        self.paste_drawing();
                    }
                }
                ShortcutRequest::SymbolSearch(text) => {
                    self.symbol_search.open_with_query(text);
                }
                ShortcutRequest::IntervalEntry(text) => {
                    self.interval_entry = Some(text);
                    self.interval_focus = true;
                }
                ShortcutRequest::Action(action) => match action {
                    A::OpenQuickSearch => self.quick_search.open(),
                    A::ShowKeyboardHelp => self.keyboard_shortcuts.toggle(),
                    A::OpenIndicatorSearch => self.indicator_dialog.open(),
                    A::OpenSymbolSearch => self.symbol_search.open(),
                    A::GoToDate => self.date_picker.open(),
                    A::Snapshot => self.handle_top_toolbar(ctx, TopToolbarAction::TakeScreenshot),
                    A::CreateAlert => self.alert_dialog.open(self.state.symbol.clone()),
                    A::MaximizeChart => {
                        self.handle_top_toolbar(ctx, TopToolbarAction::ToggleFullscreen)
                    }
                    A::Undo => self.handle_top_toolbar(ctx, TopToolbarAction::Undo),
                    A::Redo => self.handle_top_toolbar(ctx, TopToolbarAction::Redo),
                    A::CopyDrawing => {
                        self.copied_drawing = self
                            .chart
                            .drawings()
                            .sel_drawing()
                            .and_then(|id| {
                                self.chart
                                    .drawings()
                                    .drawings
                                    .iter()
                                    .find(|drawing| drawing.id == id)
                                    .cloned()
                            })
                            .map(|drawing| {
                                let text = format!("Drawing clipboard\n{drawing:#?}");
                                ctx.copy_text(text.clone());
                                (text, drawing)
                            });
                    }
                    A::PasteDrawing => self.paste_drawing(),
                    // There is one chart in this host; egui still moves keyboard focus with Tab.
                    A::CycleChartFocus | A::CycleChartFocusBack => {}
                    _ => {}
                },
            }
        }
    }

    // The demo keeps a session clipboard; hosts can decode their persistent drawing format instead.
    fn paste_drawing(&mut self) {
        if let Some((_, mut drawing)) = self.copied_drawing.clone() {
            drawing.stored_id = None;
            drawing.created_at = None;
            for point in &mut drawing.chart_points {
                point.bar_idx += 3.0;
            }
            self.chart.drawings_mut().add_synced_drawing(drawing);
        }
    }

    fn request_screenshot(&self, ctx: &egui::Context, target: ScreenshotTarget) {
        let (sender, repaint) = (self.screenshots.0.clone(), ctx.clone());
        ctx.request_screenshot(move |image| {
            // The receiver lives as long as the app, so this fails only during shutdown.
            let _ = sender.send((image, target));
            // egui does not repaint when the capture arrives.
            repaint.request_repaint();
        });
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if let Some(target) = self.screenshot_requested.take() {
            self.request_screenshot(ctx, target);
        }

        // Deliver the screenshots captured since the last frame.
        while let Ok((image, target)) = self.screenshots.1.try_recv() {
            match target {
                ScreenshotTarget::Clipboard => {
                    ctx.copy_image((*image).clone());
                    self.notice = Some(("Image sent to clipboard".into(), 0.0));
                }
                ScreenshotTarget::File(path) => self.save_screenshot(&image, path),
            }
        }
    }

    fn show_notice(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let text = {
            let Some((msg, shown_at)) = self.notice.as_mut() else {
                return;
            };
            if *shown_at == 0.0 {
                *shown_at = now;
            }
            if now - *shown_at > 4.0 {
                None
            } else {
                Some(msg.clone())
            }
        };
        let Some(text) = text else {
            self.notice = None;
            return;
        };
        egui::Area::new(egui::Id::unique("demo_notice"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -56.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(ui.visuals().widgets.active.bg_fill)
                    .show(ui, |ui| {
                        ui.label(text);
                    });
            });
        ctx.request_repaint();
    }

    fn save_screenshot(&mut self, image: &egui::ColorImage, path: Option<std::path::PathBuf>) {
        let path = path.unwrap_or_else(|| {
            let filename = format!("{}_chart.png", self.state.symbol);
            #[cfg(not(target_arch = "wasm32"))]
            return std::env::temp_dir().join(filename);
            #[cfg(target_arch = "wasm32")]
            std::path::PathBuf::from(filename)
        });
        let (w, h) = (image.size[0] as u32, image.size[1] as u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for px in &image.pixels {
            rgba.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
        }
        let result = image::RgbaImage::from_raw(w, h, rgba)
            .ok_or_else(|| "Screenshot buffer size mismatch".to_string())
            .and_then(|buf| {
                #[cfg(not(target_arch = "wasm32"))]
                return buf
                    .save_with_format(&path, image::ImageFormat::Png)
                    .map_err(|e| e.to_string());
                #[cfg(target_arch = "wasm32")]
                {
                    let mut png = std::io::Cursor::new(Vec::new());
                    buf.write_to(&mut png, image::ImageFormat::Png)
                        .map_err(|e| e.to_string())?;
                    browser_download(png.get_ref(), &path)
                }
            });
        match result {
            Ok(()) => {
                self.notice = Some((format!("Export successful: {}", path.display()), 0.0));
            }
            Err(error) => {
                self.notice = Some((format!("Export failed: {error}"), 0.0));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free helpers.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
fn browser_download(bytes: &[u8], path: &std::path::Path) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, HtmlAnchorElement, Url, js_sys};

    let download = || -> Result<(), wasm_bindgen::JsValue> {
        let document = web_sys::window().unwrap().document().unwrap();
        let anchor = document
            .create_element("a")?
            .dyn_into::<HtmlAnchorElement>()?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence(&parts)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        anchor.set_href(&url);
        anchor.set_download(&path.file_name().unwrap_or_default().to_string_lossy());
        anchor.click();
        Url::revoke_object_url(&url)
    };
    download().map_err(|e| format!("Browser download failed: {e:?}"))
}

fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .corner_radius(4)
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
}

fn condition_from_label(label: &str) -> AlertCondition {
    match label.to_lowercase().as_str() {
        l if l.contains("below") && l.contains("cross") => AlertCondition::CrossesBelow,
        l if l.contains("cross") => AlertCondition::CrossesAbove,
        l if l.contains("below") || l.contains("less") => AlertCondition::Below,
        _ => AlertCondition::Above,
    }
}

fn line_style_from_dialog(style: DrawingLineStyle) -> egui_charts::drawings::LineStyle {
    use egui_charts::drawings::LineStyle;
    match style {
        DrawingLineStyle::Solid => LineStyle::Solid,
        DrawingLineStyle::Dashed => LineStyle::Dashed,
        DrawingLineStyle::Dotted => LineStyle::Dotted,
    }
}

fn apply_timezone(chart: &mut Chart, toolbar: &mut TimeframeToolbar, name: &str) {
    use egui_charts::config::TimezoneMode;
    let timezone = if name == "UTC" {
        TimezoneMode::Utc
    } else if let Ok(zone) = name.parse() {
        TimezoneMode::Timezone(zone)
    } else {
        return;
    };
    if chart.chart_options().time_scale.timezone != timezone {
        let mut options = chart.chart_options().clone();
        options.time_scale.timezone = timezone;
        // Preserve the current pan and zoom when applying display options.
        let (spacing, offset) = chart.get_time_scale_state();
        let previous = std::mem::replace(chart, Chart::new(BarData::new()));
        *chart = previous.with_chart_options(options);
        chart.apply_synced_time_scale(spacing, offset);
    }
    toolbar.set_timezone(name.to_owned());
}

/// Apply display settings to the live chart and its clock.
fn apply_settings_state(
    state: &ChartSettingsState,
    chart: &mut Chart,
    toolbar: &mut TimeframeToolbar,
) {
    use egui_charts::config::NavigationButtons;
    use egui_charts::ui::top_toolbar::components::settings_menu::types::{
        ButtonVisibility, GridLinesMode,
    };
    apply_timezone(chart, toolbar, &state.timezone);
    let config = chart.config_mut();

    config.bullish_color = state.candle_colors.body_up;
    config.bearish_color = state.candle_colors.body_down;
    config.bullish_border_color = Some(state.candle_colors.border_up);
    config.bearish_border_color = Some(state.candle_colors.border_down);
    config.bullish_wick_color = Some(state.candle_colors.wick_up);
    config.bearish_wick_color = Some(state.candle_colors.wick_down);

    config.background_color = state.chart_basic_styles.background_color;
    config.grid_color = state.grid_lines.horizontal_color;

    let (h, v) = match state.grid_lines.mode {
        GridLinesMode::None => (false, false),
        GridLinesMode::Horizontal => (true, false),
        GridLinesMode::Vertical => (false, true),
        GridLinesMode::Both => (true, true),
    };
    config.show_horizontal_grid = h;
    config.show_vertical_grid = v;
    config.show_grid = h || v;

    config.navigation_buttons = match state.btns.navigation {
        ButtonVisibility::AlwaysVisible => NavigationButtons::Always,
        ButtonVisibility::VisibleOnMouseOver => NavigationButtons::OnHover,
        ButtonVisibility::Hidden => NavigationButtons::Never,
    };
    config.show_ohlc_info = state.status_line.show_ohlc;
    config.show_symbol_labels = state.status_line.show_symbol;
    config.title_as_description = state.status_line.title_as_description;
    config.show_quote_boxes = state.status_line.show_quote_boxes;
    config.show_ohlc_change = state.status_line.show_change;
    config.show_ohlc_change_percent = state.status_line.show_change_percent;
    config.show_legend_volume = state.status_line.show_volume;
    config.show_indicator_labels = state.status_line.show_indicator_titles;
    config.show_indicator_inputs = state.status_line.show_indicator_inputs;
    config.show_indicator_last_val = state.status_line.show_indicator_values;
    config.show_right_axis = !state.scales_and_lines.price_scale_left;
    config.show_left_axis = state.scales_and_lines.price_scale_left;
    config.show_symbol_last_val = state.scales_and_lines.show_last_price_label;
    config.show_price_line = state.scales_and_lines.show_price_line;
    config.show_symbol_prev_close = state.scales_and_lines.show_prev_close_label;
    config.show_prev_close_line = state.scales_and_lines.show_prev_close;
    config.show_countdown = state.scales_and_lines.show_countdown;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::ui::timeframe_toolbar::DateRange;

    /// Native integrations send clipboard events instead of key events. Only our session
    /// clipboard pastes a drawing, and the pasted copy is an ordinary undoable addition.
    #[test]
    fn native_drawing_clipboard_pastes_once_and_undoes_the_copy() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 120.0)];
        drawing.completed = true;
        app.chart.drawings_mut().drawings.push(drawing);
        app.chart.drawings_mut().select(7);
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ui| app.handle_keyboard(ui.ctx()),
        );
        let text = output.platform_output.commands.iter().find_map(|command| {
            if let egui::OutputCommand::CopyText(text) = command {
                Some(text.clone())
            } else {
                None
            }
        });
        output.drop_without_applying_deltas();
        let text = text.expect("copy must provide system clipboard text");
        for (event, count) in [
            (egui::Event::Paste("unrelated clipboard text".into()), 1),
            (egui::Event::Paste(text), 2),
            (
                egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                1,
            ),
        ] {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| app.handle_keyboard(ui.ctx()),
            )
            .drop_without_applying_deltas();
            assert_eq!(app.chart.drawings().drawings.len(), count);
        }
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[0],
            ChartPoint::new(10.0, 100.0)
        );
    }

    /// A bare number enters minutes and Enter applies it through the normal host path.
    #[test]
    fn typed_interval_entry_applies_minutes() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        for events in [
            vec![egui::Event::Text("15".into())],
            vec![],
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(app.state.timeframe, Timeframe::Min15);
        assert_eq!(app.state.timeframe.duration_ms(), 900_000);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min15));
    }

    /// Quick search executes catalogue entries as well as opening settings.
    #[test]
    fn quick_search_executes_studies_tools_and_settings() {
        use egui::{Event, Key, Modifiers, RawInput};
        for query in ["RSI", "Trend Line", "Chart settings"] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            let key = |key, modifiers| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
            for (modifiers, events) in [
                (
                    Modifiers::CTRL.plus(Modifiers::COMMAND),
                    vec![key(Key::K, Modifiers::CTRL.plus(Modifiers::COMMAND))],
                ),
                (Modifiers::NONE, vec![Event::Text(query.into())]),
                (Modifiers::NONE, vec![key(Key::Enter, Modifiers::NONE)]),
            ] {
                ctx.run_ui(
                    RawInput {
                        events: std::iter::once(Event::ModifiersChanged(modifiers))
                            .chain(events)
                            .collect(),
                        ..Default::default()
                    },
                    |ui| {
                        app.handle_keyboard(ui.ctx());
                        app.show_dialogs(ui.ctx());
                    },
                )
                .drop_without_applying_deltas();
            }
            match query {
                "RSI" => assert!(
                    app.chart
                        .indicators()
                        .any(|(_, study)| study.name() == "RSI")
                ),
                "Trend Line" => assert_eq!(
                    app.chart.drawings().active_tool,
                    Some(egui_charts::drawings::DrawingToolType::TrendLine)
                ),
                _ => assert!(app.settings_dialog.is_open),
            }
        }
    }

    /// The dialog gets Escape before the chart or replay transport.
    #[test]
    fn escape_in_shortcuts_leaves_drawing_selection_and_replay_intact() {
        use eframe::App;
        use egui_charts::drawings::{Drawing, DrawingToolType};
        for replay in [false, true] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            if replay {
                app.start_replay();
            }
            app.chart
                .drawings_mut()
                .drawings
                .push(Drawing::new(7, DrawingToolType::TrendLine));
            app.chart.drawings_mut().select(7);
            app.chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::TrendLine));
            app.keyboard_shortcuts.open();
            for pass in 0..3 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events: if pass == 2 {
                            vec![egui::Event::Key {
                                key: egui::Key::Escape,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            }]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                )
                .drop_without_applying_deltas();
            }
            assert!(!app.keyboard_shortcuts.is_open);
            assert_eq!(app.chart.drawings().sel_drawing(), Some(7));
            assert_eq!(
                app.chart.drawings().active_tool,
                Some(DrawingToolType::TrendLine)
            );
            assert_eq!(app.replay.is_some(), replay);
            assert_eq!(app.replay_control.is_visible(), replay);
        }
    }

    /// Existing drawing settings survive opening and applying the dialog, and the toolbar
    /// reflects locks changed through other drawing controls rather than an optimistic toggle.
    #[test]
    fn drawing_controls_round_trip_visibility_fill_fonts_and_locks() {
        use egui_charts::drawings::{Drawing, DrawingToolType, LineStyle, TimeframeVisibility};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut drawing = Drawing::new(7, DrawingToolType::TextLabel);
        drawing.line_style = LineStyle::Dotted;
        drawing.fill_color = Some([51, 102, 153, 85]);
        drawing.font_size = 24.0;
        drawing.extend_left = true;
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        drawing.text = Some("Annotation".into());
        drawing.locked = true;
        app.chart.drawings_mut().drawings.push(drawing);
        app.open_drawing_props_for(7);
        let props = app.drawing_props.props.clone();
        assert!(!props.visible_all_timeframes);
        assert_eq!(props.visible_timeframes, ["1h"]);
        assert!(props.fill_enabled);
        assert_eq!(
            props.fill_color.unwrap().to_srgba_unmultiplied(),
            [51, 102, 153, 85]
        );
        assert_eq!(props.font_size, 24.0);
        assert!(matches!(props.line_style, DrawingLineStyle::Dotted));
        assert!(props.extend_left);
        app.apply_drawing_props(
            DrawingId(7),
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            },
        );
        assert_eq!(app.chart.drawings().drawings[0].font_size, 24.0);
        app.chart
            .drawings_mut()
            .edit_drawing(7, |d| d.locked = false);
        app.apply_drawing_props(DrawingId(7), props);
        let changed = DrawingProps {
            visible_all_timeframes: false,
            visible_timeframes: vec!["4h".into()],
            font_size: 28.0,
            fill_enabled: false,
            ..app.drawing_props.props.clone()
        };
        app.apply_drawing_props(DrawingId(7), changed);
        let saved = &app.chart.drawings().drawings[0];
        assert!(saved.timeframe_visibility.is_visible_on("4h"));
        assert!(!saved.timeframe_visibility.is_visible_on("1h"));
        assert_eq!(saved.fill_color, None);
        assert_eq!(saved.font_size, 28.0);
        app.handle_object_tree(
            ObjectTreeAction::ChangeColor(
                7,
                egui::Color32::from_rgba_unmultiplied(51, 102, 153, 85),
            ),
            &[],
        );
        assert_eq!(app.chart.drawings().drawings[0].color, [51, 102, 153, 85]);
        app.chart.drawings_mut().end_drag_handle();
        app.chart
            .drawings_mut()
            .edit_drawing(7, |d| d.locked = true);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK)
        })
        .drop_without_applying_deltas();
        assert!(app.drawing_toolbar.state.lock_all);
        app.chart.drawings_mut().drawings[0].locked = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK)
        })
        .drop_without_applying_deltas();
        assert!(!app.drawing_toolbar.state.lock_all);
    }

    #[test]
    fn opening_settings_keeps_the_charts_current_display() {
        for (_, theme) in egui_charts::theme::presets() {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            let mut config = theme.apply_to_config(app.chart.config().clone());
            config.show_quote_boxes = false;
            config.show_left_axis = true;
            config.show_right_axis = false;
            config.show_countdown = false;
            app.chart.price_scale_mut().options_mut().mode =
                egui_charts::scales::PriceScaleMode::Percentage;
            app.chart.price_scale_mut().set_range(-5.0, 5.0);
            *app.chart.config_mut() = config.clone();
            app.handle_top_toolbar(&ctx, TopToolbarAction::OpenSettings);
            let state = app.settings_state.clone();
            apply_settings_state(&state, &mut app.chart, &mut app.timeframe_toolbar);
            let applied = app.chart.config();
            assert!(!applied.show_quote_boxes);
            assert!(applied.show_left_axis && !applied.show_right_axis);
            assert!(!applied.show_countdown);
            assert!(state.scales_and_lines.percentage_scale);
            assert!(!state.scales_and_lines.auto_scale);
            assert_eq!(applied.grid_color, config.grid_color);
            assert_eq!(applied.background_color, config.background_color);
            assert_eq!(applied.bullish_color, config.bullish_color);
            assert_eq!(applied.bearish_color, config.bearish_color);
            assert_eq!(
                applied
                    .bullish_border_color
                    .unwrap_or(applied.bullish_color),
                config.bullish_border_color.unwrap_or(config.bullish_color)
            );
            assert_eq!(
                applied
                    .bearish_border_color
                    .unwrap_or(applied.bearish_color),
                config.bearish_border_color.unwrap_or(config.bearish_color)
            );
            assert_eq!(
                applied.bullish_wick_color.unwrap_or(applied.bullish_color),
                config.bullish_wick_color.unwrap_or(config.bullish_color)
            );
            assert_eq!(
                applied.bearish_wick_color.unwrap_or(applied.bearish_color),
                config.bearish_wick_color.unwrap_or(config.bearish_color)
            );
        }
    }

    #[test]
    fn data_window_reads_the_hovered_rendered_element() {
        for kind in [ChartType::Candles, ChartType::Renko] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.chart.set_feed_paused(true);
            app.chart.set_chart_type(kind);
            let frame = |app: &mut DemoApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.chart.show(ui);
                    },
                )
                .drop_without_applying_deltas();
            };
            for _ in 0..6 {
                frame(&mut app, vec![]);
            }
            let (start, end) = app.chart.state().visible_range();
            let index = (start + end) / 2;
            let map = app.chart.get_chart_mapping();
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(egui::pos2(
                    map.idx_to_x(index),
                    map.rect.center().y,
                ))],
            );
            let info = app.build_data_window();
            let bars = app.chart.state().shown();
            assert_eq!(info.close, bars[index].close, "{kind:?}");
            assert_eq!(
                info.timestamp,
                bars[index].time.format("%a %d %b '%y %H:%M").to_string(),
                "{kind:?}"
            );
            assert_eq!(info.prev_close, bars[index - 1].close, "{kind:?}");
            let mut settings = app.settings_state.clone();
            settings.timezone = "Europe/Paris".to_owned();
            let view = app.chart.get_time_scale_state();
            apply_settings_state(&settings, &mut app.chart, &mut app.timeframe_toolbar);
            assert_eq!(
                app.chart
                    .format_time("2024-10-27T01:00:00Z".parse().unwrap()),
                "Sun 27 Oct '24 02:00"
            );
            assert_eq!(app.timeframe_toolbar.state.timezone, "Europe/Paris");
            assert_eq!(app.chart.get_time_scale_state(), view);
            app.open_chart_settings_dialog();
            assert_eq!(app.settings_state.timezone, "Europe/Paris");
        }
    }

    #[test]
    fn ranges_fit_displayed_history_in_both_themes_and_window_sizes() {
        for theme in [Theme::light(), Theme::dark()] {
            for size in [egui::vec2(1316.0, 871.0), egui::vec2(640.0, 450.0)] {
                for (interval, replay) in [(Timeframe::Hour1, true), (Timeframe::Min1, false)] {
                    let ctx = egui::Context::default();
                    let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                    app.theme = theme.clone();
                    egui_charts::theme::apply_to_egui(&ctx, &theme);
                    let config = app.chart.config_mut();
                    *config = theme.apply_to_config(config.clone());
                    app.switch_timeframe(interval);
                    app.timeframe_toolbar.set_session(SessionType::ETH);
                    app.apply_session();
                    app.chart.set_feed_paused(true);
                    let frame = |app: &mut DemoApp| {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| app.show_chart(ui, theme.chart.bg),
                        )
                        .drop_without_applying_deltas();
                    };
                    for _ in 0..3 {
                        frame(&mut app);
                    }
                    if replay {
                        app.start_replay();
                        frame(&mut app);
                    }
                    for range in [DateRange::Day1, DateRange::Month1, DateRange::All] {
                        let bars = app.chart.bars(SeriesId::MAIN);
                        let end = bars.len();
                        let start = if range == DateRange::Day1 {
                            let cutoff = bars.last().unwrap().time - chrono::Duration::days(1);
                            bars.iter().position(|b| b.time >= cutoff).unwrap()
                        } else {
                            0
                        };
                        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(
                            range,
                        ));
                        frame(&mut app);
                        let (shown_start, shown_end) = app.chart.state().visible_range();
                        assert_eq!(shown_end, end, "{range:?}");
                        assert!(shown_start.abs_diff(start) <= 1, "{range:?}");
                        let expected = app.chart.get_rendered_price_rect().width()
                            / ((end - start) as f32 + 0.5);
                        assert!((app.chart.get_time_scale_state().0 - expected).abs() < 0.001);
                        assert_eq!(app.replay.is_some(), replay);
                    }
                }
            }
        }
    }
    /// Selecting seconds must rebuild the same executions, including the current partial minute.
    #[test]
    fn interval_switch_rebuilds_the_same_executions() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let frame = |app: &mut DemoApp| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        for _ in 0..3 {
            frame(&mut app);
        }
        let tick_size = app.chart.instrument().unwrap().tick_size.unwrap();
        for bar in app.chart.bars(SeriesId::MAIN) {
            for price in [bar.open, bar.high, bar.low, bar.close] {
                assert!(
                    (price / tick_size - (price / tick_size).round()).abs() < 1e-7,
                    "backfill and executions must share the instrument's tick size"
                );
            }
        }
        let minute = app.chart.bars(SeriesId::MAIN).last().unwrap().clone();
        app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Sec1));
        for _ in 0..3 {
            frame(&mut app);
        }
        let clock = egui_charts::model::BarClock::new(Timeframe::Min1);
        let seconds: Vec<_> = app
            .chart
            .bars(SeriesId::MAIN)
            .iter()
            .filter(|bar| clock.bar_open(bar.time) == minute.time)
            .collect();
        assert!(
            !seconds.is_empty(),
            "seconds must cover the same partial minute"
        );
        assert_eq!(seconds[0].open, minute.open);
        assert_eq!(seconds.last().unwrap().close, minute.close);
        assert_eq!(
            seconds
                .iter()
                .map(|bar| bar.high)
                .fold(f64::NEG_INFINITY, f64::max),
            minute.high
        );
        assert_eq!(
            seconds
                .iter()
                .map(|bar| bar.low)
                .fold(f64::INFINITY, f64::min),
            minute.low
        );
        assert_eq!(
            seconds.iter().map(|bar| bar.volume).sum::<f64>(),
            minute.volume
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Min1));
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(app.chart.bars(SeriesId::MAIN).last(), Some(&minute));
        app.start_replay();
        app.switch_timeframe(Timeframe::Sec1);
        let expected = app.chart.bars(SeriesId::MAIN).to_vec();
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(
            app.chart.bars(SeriesId::MAIN),
            expected,
            "feed must not overwrite replay"
        );
        app.stop_replay();
        let mut options = app.chart.chart_options().clone();
        options.time_scale.timezone = egui_charts::config::TimezoneMode::nyse();
        let chart = std::mem::replace(&mut app.chart, Chart::new(BarData::new()));
        app.chart = chart.with_chart_options(options);
        let timeframe = "45min".parse().unwrap();
        app.switch_timeframe(timeframe);
        for _ in 0..3 {
            frame(&mut app);
        }
        let clock = egui_charts::model::BarClock::new(timeframe)
            .with_timezone(egui_charts::config::TimezoneMode::nyse().to_tz().unwrap());
        assert!(
            app.chart.bars(SeriesId::MAIN).len() >= 210,
            "coarse intervals need older bar history beyond the trade buffer"
        );
        let last_trade = app.chart.trades(SeriesId::MAIN).last().unwrap().ts;
        assert_eq!(
            app.chart.bars(SeriesId::MAIN).last().unwrap().time,
            clock.bar_open(last_trade),
            "feed must use the selected local calendar"
        );
    }
}
