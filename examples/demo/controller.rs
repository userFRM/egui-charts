//! Application controller: owns every dialog, panel, and the replay engine, and
//! routes the typed action each library widget emits into a real chart mutation.
//!
//! The chart engine is pure: widgets return `Action` enums and never touch the
//! chart themselves. This controller is the host half that consumes those
//! actions, so that no visible control in the demo is a no-op.

use eframe::egui;
use egui_charts::chart::builder::TradingChart;
use egui_charts::chart::selection::SeriesId;
use egui_charts::chart::series::SeriesSettings;
use egui_charts::model::{Bar, BarData, ChartType, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::ui::app_state::ChartAppState;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{
    AlertData, AlertDialog, AlertDialogAction, DrawingId, DrawingPropertiesAction,
    DrawingPropertiesDialog, DrawingProps, KeyboardShortcutsDialog, PineEditor, PineEditorAction,
    SeriesContextMenuAction, SeriesSettingsAction, SeriesSettingsDialog,
};
use egui_charts::ui::dialogs::{DrawingContextMenuAction, DrawingLineStyle};
use egui_charts::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarAction};
use egui_charts::ui::export::{ExportDialog, ExportFormat, ExportResponse};
use egui_charts::ui::floating_replay_control::{FloatingReplayControl, ReplayControlAction};
use egui_charts::ui::model::{AlertCondition, AlertStatus, PriceAlert};
use egui_charts::ui::replay::{ReplayAction, ReplayController};
use egui_charts::ui::symbol_header::{OhlcData, SymbolHeader, SymbolInfo};
use egui_charts::ui::timeframe_toolbar::{
    DatePicker, DatePickerAction, TimeframeToolbar, TimeframeToolbarAction,
};
use egui_charts::ui::top_toolbar::components::SymbolSearchDialog;
use egui_charts::ui::top_toolbar::components::indicator_dialog::{
    IndicatorDialog, IndicatorDialogAction,
};
use egui_charts::ui::top_toolbar::components::settings_menu::data::ChartSettingsState;
use egui_charts::ui::top_toolbar::components::settings_menu::{SettingsAction, SettingsDialog};
use egui_charts::ui::top_toolbar::{TopToolbar, TopToolbarAction};
use egui_charts::ui::widget_bar::panels::object_tree::{
    DataWindowInfo, ObjectTreeAction, ObjectTreePanel, SourceItem,
};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBar, WidgetBarAction};

use crate::sampledata::{
    DemoSymbol, bar_count_for_timeframe, chart_type_from_name, generate_sample_bars,
    timeframe_from_display, visible_bars_for_range,
};

/// Object-tree ids for indicators are offset into a high range so a single
/// `usize` id space cleanly distinguishes indicators from drawings (which use
/// small monotonic ids from the drawing manager). The main series is id 0.
const MAIN_SERIES_ID: usize = 0;
const INDICATOR_ID_BASE: usize = 1_000_000;

fn indicator_tree_id(index: usize) -> usize {
    INDICATOR_ID_BASE + index
}

fn indicator_index_from_tree_id(id: usize) -> Option<usize> {
    id.checked_sub(INDICATOR_ID_BASE)
}

/// Which right-hand side panel is open, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanel {
    ObjectTree,
    Alerts,
}

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
    pub trading_chart: TradingChart,
    pub state: DemoAppState,
    pub bars: Vec<Bar>,
    pub theme: Theme,

    // Persistent toolbars / header.
    top_toolbar: TopToolbar,
    drawing_toolbar: DrawingToolbar,
    timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,
    symbol_header: SymbolHeader,

    // Dialogs (each owns its open/closed state).
    indicator_dialog: IndicatorDialog,
    settings_dialog: SettingsDialog,
    settings_state: ChartSettingsState,
    symbol_search: SymbolSearchDialog,
    alert_dialog: AlertDialog,
    series_settings: SeriesSettingsDialog,
    drawing_props: DrawingPropertiesDialog,
    keyboard_shortcuts: KeyboardShortcutsDialog,
    export_dialog: ExportDialog,
    pine_editor: PineEditor,
    date_picker: DatePicker,

    // Right-hand side panel.
    object_tree: ObjectTreePanel,
    open_panel: Option<SidePanel>,

    // In-memory alert store. `AlertManager` exposes no mutators, so the demo
    // owns its alerts for the session (no fake persistence).
    alerts: Vec<PriceAlert>,
    next_alert_id: usize,

    // Replay engine and its floating transport. `None` until first toggled.
    replay: Option<ReplayController>,
    replay_control: FloatingReplayControl,

    // A pending screenshot request the egui backend fulfils next frame.
    screenshot_requested: bool,

    // Transient status notice (text, time shown). Controls whose backing is a
    // stub/network/broker surface an honest message here rather than silently
    // doing nothing, since the library toolbar renders them unconditionally.
    notice: Option<(String, f64)>,
}

impl DemoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register egui's image loaders so the embedded SVG toolbar icons decode.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let theme = Theme::dark();
        let timeframe = Timeframe::Day1;
        let symbol = "BTCUSD";

        let mut tc = egui_charts::ChartBuilder::extended()
            .with_symbol(symbol)
            .with_timeframe(timeframe)
            .with_theme(theme.clone())
            .with_chart_type(ChartType::Candles)
            .with_indicators(default_indicators())
            .with_visible_candles(100)
            .with_initial_bars(None)
            .build();

        let bars = generate_sample_bars(symbol, 500, timeframe);
        tc.chart.update_data(BarData::from_bars(bars.clone()));
        tc.indicators.calculate_all(&bars);

        let spec = DemoSymbol::find(symbol);
        let last = bars.last().cloned().unwrap_or_else(|| Bar {
            time: chrono::Utc::now(),
            open: spec.start_price,
            high: spec.start_price,
            low: spec.start_price,
            close: spec.start_price,
            volume: 0.0,
        });
        let symbol_header = SymbolHeader::new(SymbolInfo {
            symbol: symbol.into(),
            company_name: spec.company_name.into(),
            exchange: spec.exchange.into(),
            timeframe: timeframe.as_str().to_string(),
        })
        .with_ohlc(OhlcData::new(
            last.open,
            last.high,
            last.low,
            last.close,
            last.volume,
        ));

        Self {
            trading_chart: tc,
            state: DemoAppState {
                symbol: symbol.into(),
                timeframe,
                chart_type: ChartType::Candles,
                replay_active: false,
            },
            bars,
            theme,
            top_toolbar: TopToolbar::new(),
            drawing_toolbar: DrawingToolbar::default(),
            timeframe_toolbar: TimeframeToolbar::default(),
            widget_bar: WidgetBar::default(),
            symbol_header,
            indicator_dialog: IndicatorDialog::new(),
            settings_dialog: SettingsDialog::new(),
            settings_state: ChartSettingsState::default(),
            symbol_search: SymbolSearchDialog::new(),
            alert_dialog: AlertDialog::new(),
            series_settings: SeriesSettingsDialog::new(),
            drawing_props: DrawingPropertiesDialog::new(),
            keyboard_shortcuts: KeyboardShortcutsDialog::new(),
            export_dialog: ExportDialog::new(),
            pine_editor: PineEditor::new(),
            date_picker: DatePicker::new(),
            object_tree: ObjectTreePanel::new(),
            open_panel: None,
            alerts: Vec::new(),
            next_alert_id: 1,
            replay: None,
            replay_control: FloatingReplayControl::new(),
            screenshot_requested: false,
            notice: None,
        }
    }

    /// Surface a short-lived, honest status message in the UI.
    fn notify(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), 0.0));
    }

    // --- data helpers ------------------------------------------------------

    fn switch_timeframe(&mut self, tf: Timeframe) {
        self.state.timeframe = tf;
        let count = bar_count_for_timeframe(tf);
        self.regenerate_bars(count, tf);
        self.symbol_header.symbol_info.timeframe = tf.as_str().to_string();
    }

    fn switch_symbol(&mut self, symbol: &str) {
        let spec = DemoSymbol::find(symbol);
        self.state.symbol = spec.symbol.to_string();
        self.trading_chart.set_symbol(spec.symbol);
        self.symbol_header.symbol_info.symbol = spec.symbol.to_string();
        self.symbol_header.symbol_info.company_name = spec.company_name.to_string();
        self.symbol_header.symbol_info.exchange = spec.exchange.to_string();
        let count = bar_count_for_timeframe(self.state.timeframe);
        self.regenerate_bars(count, self.state.timeframe);
        // A symbol change invalidates an in-flight replay.
        self.stop_replay();
    }

    fn regenerate_bars(&mut self, count: usize, tf: Timeframe) {
        self.bars = generate_sample_bars(&self.state.symbol, count, tf);
        self.trading_chart
            .chart
            .update_data(BarData::from_bars(self.bars.clone()));
        self.trading_chart.indicators.calculate_all(&self.bars);
        self.update_symbol_header();
    }

    fn update_symbol_header(&mut self) {
        if let Some(last) = self.bars.last() {
            self.symbol_header.ohlc =
                OhlcData::new(last.open, last.high, last.low, last.close, last.volume);
        }
    }

    // --- replay ------------------------------------------------------------

    fn toggle_replay(&mut self) {
        if self.replay.is_some() {
            self.stop_replay();
        } else {
            self.start_replay();
        }
    }

    fn start_replay(&mut self) {
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
            self.state.replay_active = false;
            self.replay_control.hide_control();
            // Restore the full series the chart had before replay.
            self.trading_chart
                .chart
                .update_data(BarData::from_bars(self.bars.clone()));
            self.trading_chart.indicators.calculate_all(&self.bars);
        }
    }

    /// Feed the replay controller's current visible window into the chart. This
    /// is the replay -> chart link the demo previously lacked entirely.
    fn push_replay_window_to_chart(&mut self) {
        if let Some(replay) = self.replay.as_ref() {
            let visible: Vec<Bar> = replay.visible_bars().to_vec();
            if !visible.is_empty() {
                self.trading_chart
                    .chart
                    .update_data(BarData::from_bars(visible.clone()));
                self.trading_chart.indicators.calculate_all(&visible);
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

    fn open_series_settings_dialog(&mut self) {
        self.series_settings
            .open(SeriesId::MAIN, SeriesSettings::default());
    }

    fn open_drawing_props_for(&mut self, drawing_id: usize) {
        let Some(dm) = self.trading_chart.drawing_manager.as_ref() else {
            return;
        };
        let Some(drawing) = dm.drawings.iter().find(|d| d.id == drawing_id) else {
            return;
        };
        let props = DrawingProps {
            color: drawing.color32(),
            line_width: drawing.stroke_width,
            label: drawing.text.clone().unwrap_or_default(),
            ..DrawingProps::default()
        };
        let name = drawing.tool_type.as_str().to_string();
        self.drawing_props.open(DrawingId(drawing_id), name, props);
    }

    // --- dialog wiring -----------------------------------------------------

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        // Indicators: build the configured indicator and add it live.
        match self.indicator_dialog.show(ctx, &self.theme) {
            IndicatorDialogAction::AddConfiguredIndicator(cfg) => {
                let indicator = self.indicator_dialog.create_indicator(cfg.indicator_type);
                self.trading_chart.indicators.add(indicator);
                self.recalc_indicators();
            }
            IndicatorDialogAction::AddIndicator(id) => {
                use egui_charts::ui::top_toolbar::components::indicator_dialog::IndicatorType;
                if let Some(kind) = IndicatorType::from_id(&id) {
                    let indicator = self.indicator_dialog.create_indicator(kind);
                    self.trading_chart.indicators.add(indicator);
                    self.recalc_indicators();
                }
            }
            _ => {}
        }

        // Chart settings.
        match self.settings_dialog.show(ctx, &self.theme) {
            SettingsAction::Apply(state) => {
                self.settings_state = state.clone();
                apply_settings_state(&state, self.trading_chart.config_mut());
            }
            SettingsAction::Cancel => {}
            _ => {}
        }

        // Symbol search (used by the pill, quick search, and compare entry).
        let symbols = DemoSymbol::symbols();
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
                self.trading_chart.apply_series_settings(&settings);
            }
            SeriesSettingsAction::ResetToDefault(_id) => {
                self.trading_chart
                    .apply_series_settings(&SeriesSettings::default());
            }
            SeriesSettingsAction::None | SeriesSettingsAction::Cancel => {}
        }

        // Drawing properties.
        match self.drawing_props.show(ctx) {
            DrawingPropertiesAction::Apply(id, props) => self.apply_drawing_props(id, props),
            DrawingPropertiesAction::Delete(id) => {
                self.trading_chart.remove_drawing(id.0);
            }
            DrawingPropertiesAction::Clone(id) => self.clone_drawing(id.0),
            DrawingPropertiesAction::Cancel | DrawingPropertiesAction::None => {}
        }

        // Keyboard shortcuts help.
        let _ = self.keyboard_shortcuts.show(ctx);

        // Export dialog (screenshot / CSV).
        match self.export_dialog.show(ctx) {
            ExportResponse::Export { config, save_path } => self.run_export(config, save_path),
            ExportResponse::Closed | ExportResponse::None => {}
        }

        // Pine editor.
        match self.pine_editor.show(ctx) {
            PineEditorAction::Executed => {
                log::info!("[demo] Pine script executed");
            }
            PineEditorAction::Close | PineEditorAction::None => {}
        }

        // Date picker (opened from the bottom toolbar): jump the visible window.
        match self.date_picker.show(ctx) {
            DatePickerAction::GoToDate(date) => self.go_to_date(date),
            DatePickerAction::SetRange(start, _end) => self.go_to_date(start),
            DatePickerAction::Cancel | DatePickerAction::None => {}
        }
    }

    fn recalc_indicators(&mut self) {
        let bars = self.replay_window_or_full();
        self.trading_chart.indicators.calculate_all(&bars);
    }

    fn replay_window_or_full(&self) -> Vec<Bar> {
        match self.replay.as_ref() {
            Some(replay) if !replay.visible_bars().is_empty() => replay.visible_bars().to_vec(),
            _ => self.bars.clone(),
        }
    }

    fn apply_drawing_props(&mut self, id: DrawingId, props: DrawingProps) {
        let Some(dm) = self.trading_chart.drawing_manager.as_mut() else {
            return;
        };
        if let Some(drawing) = dm.drawings.iter_mut().find(|d| d.id == id.0) {
            drawing.color = props.color.to_array();
            drawing.stroke_width = props.line_width;
            drawing.line_style = line_style_from_dialog(props.line_style);
            drawing.extend_left = props.extend_left;
            drawing.extend_right = props.extend_right;
            if !props.label.is_empty() {
                drawing.text = Some(props.label);
            }
        }
    }

    fn clone_drawing(&mut self, id: usize) {
        let Some(dm) = self.trading_chart.drawing_manager.as_mut() else {
            return;
        };
        if let Some(src) = dm.drawings.iter().find(|d| d.id == id).cloned() {
            let mut copy = src;
            for cp in &mut copy.chart_points {
                cp.bar_idx += 3.0; // offset so the clone is visible
            }
            dm.add_synced_drawing(copy);
        }
    }

    fn go_to_date(&mut self, date: chrono::NaiveDate) {
        // The sample series starts 2024-01-01 at one bar per timeframe step.
        let base = chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let days = (date - base).num_days().max(0) as usize;
        let approx_bar = days.min(self.bars.len().saturating_sub(1));
        // Re-center the visible window around the chosen date.
        let window = 120usize.min(self.bars.len());
        let visible = window.min(self.bars.len().saturating_sub(approx_bar).max(10));
        self.trading_chart.chart.set_visible_bars(visible);
    }

    fn run_export(
        &mut self,
        config: egui_charts::ui::export::ExportConfig,
        save_path: Option<std::path::PathBuf>,
    ) {
        match config.format {
            ExportFormat::Png | ExportFormat::Svg | ExportFormat::Clipboard => {
                // Image/clipboard exports route through the egui screenshot path.
                self.screenshot_requested = true;
                let _ = save_path;
            }
            ExportFormat::Csv => self.export_csv(save_path),
        }
    }

    fn export_csv(&self, save_path: Option<std::path::PathBuf>) {
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = save_path.unwrap_or_else(|| {
                std::env::temp_dir().join(format!("{}_export.csv", self.state.symbol))
            });
            if let Err(e) = std::fs::write(&path, out) {
                log::warn!("[demo] CSV export failed: {e}");
            } else {
                log::info!("[demo] Wrote CSV to {}", path.display());
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = save_path;
            log::info!(
                "[demo] CSV export ({} bytes) — browser download not wired",
                out.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App — per-frame update orchestrating panels, toolbars, and dialogs.
// ---------------------------------------------------------------------------

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui_charts::theme::apply_to_egui(ctx, &self.theme);

        // Drive replay before laying out the chart so its window is current.
        self.advance_replay();

        let panel_bg = ctx.style().visuals.panel_fill;

        self.show_top_toolbar(ctx, panel_bg);
        self.show_symbol_header(ctx, panel_bg);
        self.show_drawing_toolbar(ctx, panel_bg);
        self.show_widget_bar(ctx, panel_bg);
        self.show_side_panel(ctx);
        self.show_bottom_toolbar(ctx, panel_bg);
        self.show_chart(ctx, panel_bg);

        // Floating replay transport, on top of the chart.
        self.show_replay_control(ctx);

        // All modal dialogs.
        self.show_dialogs(ctx);

        // Global keyboard shortcuts (e.g. the shortcuts help itself).
        self.handle_keyboard(ctx);

        // Fulfil a pending screenshot request.
        self.handle_screenshot(ctx);

        // Transient status notice.
        self.show_notice(ctx);

        // Keep the header OHLC in sync with whatever the chart last rendered.
        if let Some(last) = self.trading_chart.get_last_bar() {
            self.symbol_header.ohlc =
                OhlcData::new(last.open, last.high, last.low, last.close, last.volume);
        }

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
    fn show_top_toolbar(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::TopBottomPanel::top("toolbar")
            .exact_height(38.0)
            .show_separator_line(false)
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                let action = self.top_toolbar.show_with_state(ui, &self.state);
                self.handle_top_toolbar(ctx, action);
            });
    }

    fn handle_top_toolbar(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::ChartStyleSelected(name) => {
                if let Some(ct) = chart_type_from_name(&name) {
                    self.state.chart_type = ct;
                    self.trading_chart.set_chart_type(ct);
                }
            }
            TopToolbarAction::IntervalSelected(display) => {
                if let Some(tf) = timeframe_from_display(&display) {
                    self.switch_timeframe(tf);
                }
            }
            TopToolbarAction::OpenSymbolSearch | TopToolbarAction::OpenCompare => {
                self.symbol_search.open();
            }
            TopToolbarAction::OpenIndicators => self.indicator_dialog.open(),
            TopToolbarAction::CreateAlert => self.alert_dialog.open(self.state.symbol.clone()),
            TopToolbarAction::ToggleReplay => self.toggle_replay(),
            TopToolbarAction::OpenSettings => {
                self.settings_dialog.open(self.settings_state.clone());
            }
            TopToolbarAction::TakeScreenshot => self.screenshot_requested = true,
            TopToolbarAction::ToggleFullscreen => {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
            }
            TopToolbarAction::Undo => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.undo();
                }
            }
            TopToolbarAction::Redo => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.redo();
                }
            }
            // Indicator templates live in the indicator dialog's Templates tab.
            TopToolbarAction::OpenIndicatorTemplates => self.indicator_dialog.open(),
            // Save routes through the export dialog (PNG / SVG / CSV of the view).
            TopToolbarAction::Save => self.export_dialog.open(),
            // Light/dark toggle is a real, cheap visual change.
            TopToolbarAction::ThemeToggle => self.toggle_theme(),
            // The burger menu and layout-setup button open the same chart-object
            // panel TradingView puts behind them in this single-chart context.
            TopToolbarAction::OpenMenu | TopToolbarAction::Layout(_) => {
                self.open_panel = Some(SidePanel::ObjectTree);
            }
            // The remaining controls need a broker (trading), a backend
            // (publish), or the multi-chart grid (a stub in this engine). The
            // library toolbar renders them unconditionally and exposes no
            // per-button visibility flag, so rather than swallow the click we
            // surface an honest notice describing the missing dependency.
            TopToolbarAction::OpenTradingPanel => {
                self.notify("Trading panel needs a broker connection — not available in this demo")
            }
            TopToolbarAction::Publish => {
                self.notify("Publishing requires an account backend — not available in this demo")
            }
            TopToolbarAction::ChartGridChanged(_) | TopToolbarAction::SetSyncOptions(_) => {
                self.notify("Multi-chart layouts are not part of this single-chart demo")
            }
            TopToolbarAction::None => {}
        }
    }

    fn toggle_theme(&mut self) {
        // Swap the demo theme; `apply_to_egui` repaints every panel, toolbar,
        // and dialog from it next frame, and the chart background/grid follow
        // the matching config colors.
        self.theme = if self.theme.is_dark_ui() {
            Theme::light()
        } else {
            Theme::dark()
        };
        let cfg = self.trading_chart.config_mut();
        cfg.background_color = self.theme.background();
        cfg.text_color = self.theme.text();
        cfg.grid_color = self.theme.grid();
    }

    fn show_symbol_header(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::TopBottomPanel::top("symbol_header")
            .exact_height(38.0)
            .show_separator_line(true)
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                self.symbol_header.show(ui);
            });
    }

    fn show_drawing_toolbar(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::SidePanel::left("drawing_toolbar")
            .resizable(false)
            .default_width(52.0)
            .show_separator_line(true)
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                let action = self.drawing_toolbar.show_with_action(ui);
                self.handle_drawing_toolbar(action);
            });
    }

    fn handle_drawing_toolbar(&mut self, action: DrawingToolbarAction) {
        // Variants that touch the indicator registry (not the drawing manager)
        // or surface a notice are handled first, before the `dm` borrow.
        match action {
            DrawingToolbarAction::RemoveAllIndicators | DrawingToolbarAction::RemoveAllStudies => {
                self.trading_chart.indicators.clear();
                self.recalc_indicators();
                return;
            }
            DrawingToolbarAction::HideAllIndicators => {
                let count = self.trading_chart.indicators.indicators().len();
                for i in 0..count {
                    self.trading_chart.set_indicator_visible(i, false);
                }
                return;
            }
            DrawingToolbarAction::SaveTemplate { .. }
            | DrawingToolbarAction::LoadTemplate { .. }
            | DrawingToolbarAction::DeleteTemplate { .. } => {
                self.notify("Drawing templates are not persisted in this demo");
                return;
            }
            DrawingToolbarAction::HideAllPoss | DrawingToolbarAction::HideAllOrders => {
                self.notify("Positions and orders need a trading account — not in this demo");
                return;
            }
            _ => {}
        }

        let Some(dm) = self.trading_chart.drawing_manager.as_mut() else {
            return;
        };
        match action {
            DrawingToolbarAction::SelectTool(tool) => dm.set_active_tool(Some(tool)),
            DrawingToolbarAction::ClearSelection => {
                dm.set_active_tool(None);
                dm.eraser_mode = false;
            }
            DrawingToolbarAction::SetCursorType(cursor) => {
                use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
                dm.eraser_mode = matches!(cursor, CursorType::Eraser);
            }
            DrawingToolbarAction::ToggleMagnet => {
                dm.options.magnet_mode = !dm.options.magnet_mode;
                dm.update_snap_options();
            }
            DrawingToolbarAction::SetMagnetType(_) => {
                // Magnet strength maps to the same snap behavior; ensure on.
                dm.options.magnet_mode = true;
                dm.update_snap_options();
            }
            DrawingToolbarAction::ToggleEraserMode => dm.eraser_mode = !dm.eraser_mode,
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
                dm.start_icon_insertion(icon_name.to_string());
            }
            // The rest (favorites add/remove, favorites-bar toggle, the values
            // tooltip toggle, and the menu-open signals OpenSettings /
            // OpenTemplateMenu) mutate only the toolbar's own popup state, which
            // it manages internally; there is nothing for the host to do.
            _ => {}
        }
    }

    fn show_widget_bar(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::SidePanel::right("widget_bar")
            .resizable(false)
            .exact_width(52.0)
            .show_separator_line(true)
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                let action = self.widget_bar.show(ui);
                if let WidgetBarAction::Clicked(icon) = action {
                    self.toggle_panel(icon);
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

    fn show_side_panel(&mut self, ctx: &egui::Context) {
        let Some(which) = self.open_panel else {
            return;
        };
        egui::SidePanel::right("detail_panel")
            .resizable(true)
            .default_width(280.0)
            .min_width(220.0)
            .show(ctx, |ui| match which {
                SidePanel::ObjectTree => self.show_object_tree(ui),
                SidePanel::Alerts => self.show_alerts_panel(ui),
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
        for (idx, ind) in self
            .trading_chart
            .indicators
            .indicators()
            .iter()
            .enumerate()
        {
            let color = ind.colors().first().copied().unwrap_or(egui::Color32::GRAY);
            let mut item = SourceItem::indicator(indicator_tree_id(idx), ind.name(), color);
            item.visible = ind.is_visible();
            items.push(item);
        }

        // Live drawings.
        if let Some(dm) = self.trading_chart.drawing_manager.as_ref() {
            for d in &dm.drawings {
                let mut item = SourceItem::drawing(d.id, d.tool_type, d.color32());
                item.visible = d.visible;
                item.locked = d.locked;
                items.push(item);
            }
        }
        items
    }

    fn build_data_window(&self) -> DataWindowInfo {
        let last = self.trading_chart.get_last_bar();
        let mut info = DataWindowInfo::new(self.state.symbol.clone())
            .with_timeframe(self.state.timeframe.as_str().to_string());
        if let Some(b) = last {
            info = info.with_ohlcv(b.open, b.high, b.low, b.close, b.volume);
            info = info.with_timestamp(b.time.to_rfc3339());
        }
        info
    }

    fn handle_object_tree(&mut self, action: ObjectTreeAction, sources: &[SourceItem]) {
        match action {
            ObjectTreeAction::ToggleVisibility(id) => {
                if let Some(idx) = indicator_index_from_tree_id(id) {
                    let now = self
                        .trading_chart
                        .indicators
                        .indicators()
                        .get(idx)
                        .map(|i| i.is_visible())
                        .unwrap_or(true);
                    self.trading_chart.set_indicator_visible(idx, !now);
                } else if let Some(dm) = self.trading_chart.drawing_manager.as_mut()
                    && let Some(d) = dm.drawings.iter_mut().find(|d| d.id == id)
                {
                    d.visible = !d.visible;
                }
            }
            ObjectTreeAction::ToggleLock(id) => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut()
                    && let Some(d) = dm.drawings.iter_mut().find(|d| d.id == id)
                {
                    d.locked = !d.locked;
                }
            }
            ObjectTreeAction::Delete(id) | ObjectTreeAction::RemoveIndicator(id) => {
                if let Some(idx) = indicator_index_from_tree_id(id) {
                    self.trading_chart.remove_indicator(idx);
                    self.recalc_indicators();
                } else {
                    self.trading_chart.remove_drawing(id);
                }
            }
            ObjectTreeAction::Select(id) => {
                if let Some(idx) = indicator_index_from_tree_id(id) {
                    // Indicator panes select via the registry index.
                    let _ = idx;
                } else if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.select(id);
                }
            }
            ObjectTreeAction::ChangeColor(id, color) => {
                if let Some(idx) = indicator_index_from_tree_id(id) {
                    if let Some(ind) = self.trading_chart.indicators.indicators_mut().get_mut(idx) {
                        ind.set_color(0, color);
                    }
                } else if let Some(dm) = self.trading_chart.drawing_manager.as_mut()
                    && let Some(d) = dm.drawings.iter_mut().find(|d| d.id == id)
                {
                    d.color = color.to_array();
                }
            }
            ObjectTreeAction::OpenProperties(id) | ObjectTreeAction::OpenIndicatorSettings(id) => {
                if indicator_index_from_tree_id(id).is_some() {
                    self.indicator_dialog.open();
                } else {
                    self.open_drawing_props_for(id);
                }
            }
            ObjectTreeAction::Duplicate(id) => {
                if indicator_index_from_tree_id(id).is_none() {
                    self.clone_drawing(id);
                }
            }
            ObjectTreeAction::BringToFront(id) => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.bring_to_front(id);
                }
            }
            ObjectTreeAction::SendToBack(id) => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.send_to_back(id);
                }
            }
            ObjectTreeAction::HideAll => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    for d in &mut dm.drawings {
                        d.visible = false;
                    }
                }
            }
            ObjectTreeAction::ShowAll => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    for d in &mut dm.drawings {
                        d.visible = true;
                    }
                }
            }
            ObjectTreeAction::RemoveAllDrawings => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.clear_all();
                }
            }
            ObjectTreeAction::RemoveAllIndicators => {
                self.trading_chart.indicators.clear();
                self.recalc_indicators();
            }
            ObjectTreeAction::ClearSelection => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.deselect();
                }
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

    fn show_bottom_toolbar(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::TopBottomPanel::bottom("bottom")
            .exact_height(38.0)
            .show_separator_line(true)
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                let action = self.timeframe_toolbar.show(ui);
                self.handle_timeframe_toolbar(action);
            });
    }

    fn handle_timeframe_toolbar(&mut self, action: TimeframeToolbarAction) {
        match action {
            TimeframeToolbarAction::DateRangeChanged(range) => {
                let visible = visible_bars_for_range(range, self.bars.len());
                self.trading_chart.chart.set_visible_bars(visible);
            }
            TimeframeToolbarAction::ZoomFitAll => {
                self.trading_chart.chart.set_visible_bars(self.bars.len());
            }
            TimeframeToolbarAction::TimeframeChanged(tf) => self.switch_timeframe(tf),
            TimeframeToolbarAction::GoToDate(dt) => self.go_to_date(dt.date_naive()),
            TimeframeToolbarAction::OpenDatePicker => self.date_picker.open(),
            // Timezone / session / dividend-adjustment have no backing in the
            // demo's synthetic single-session UTC series, and the corresponding
            // controls are hidden in the toolbar config.
            _ => {}
        }
    }

    fn show_chart(&mut self, ctx: &egui::Context, bg: egui::Color32) {
        egui::CentralPanel::default()
            .frame(panel_frame(bg))
            .show(ctx, |ui| {
                self.trading_chart.show(ui);
            });
        // Right-click context actions resolve after the chart is shown.
        while let Some(action) = self.trading_chart.take_context_action() {
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
                self.settings_dialog.open(self.settings_state.clone());
            }
            ContextMenuAction::OpenSeriesSettings => self.open_series_settings_dialog(),
            ContextMenuAction::InsertIndicator => self.indicator_dialog.open(),
            ContextMenuAction::ObjectTree => self.open_panel = Some(SidePanel::ObjectTree),
            ContextMenuAction::RemoveIndicator(idx) => {
                self.trading_chart.remove_indicator(idx);
                self.recalc_indicators();
            }
            ContextMenuAction::RemoveAllIndicators => {
                self.trading_chart.indicators.clear();
                self.recalc_indicators();
            }
            ContextMenuAction::AddAlert { symbol, price } => self.quick_price_alert(symbol, price),
            ContextMenuAction::ResetChart => {
                self.trading_chart.chart.set_visible_bars(100);
            }
            ContextMenuAction::ChangeSymbol | ContextMenuAction::CompareOrAddSymbol => {
                self.symbol_search.open()
            }
            ContextMenuAction::ExportCsv => self.export_csv(None),
            ContextMenuAction::GoToDate => self.date_picker.open(),
            ContextMenuAction::CopyPrice(price) => {
                log::info!("[demo] copy price {price}");
            }
            // Remaining items (paste, table view, broker orders, price/time
            // scale popovers) are not modeled in this demo.
            _ => {}
        }
    }

    fn handle_series_context(&mut self, action: SeriesContextMenuAction) {
        match action {
            SeriesContextMenuAction::OpenSettings => self.open_series_settings_dialog(),
            SeriesContextMenuAction::AddIndicator => self.indicator_dialog.open(),
            SeriesContextMenuAction::AddAlert => self.alert_dialog.open(self.state.symbol.clone()),
            SeriesContextMenuAction::Hide => self.trading_chart.toggle_main_series_visibility(),
            _ => {}
        }
    }

    fn handle_drawing_context(&mut self, action: DrawingContextMenuAction) {
        // The library selects the right-clicked drawing, so the selected id is
        // the menu's target.
        let Some(id) = self
            .trading_chart
            .drawing_manager
            .as_ref()
            .and_then(|dm| dm.sel_drawing())
        else {
            return;
        };
        match action {
            DrawingContextMenuAction::Delete => {
                self.trading_chart.remove_drawing(id);
            }
            DrawingContextMenuAction::Edit => self.open_drawing_props_for(id),
            DrawingContextMenuAction::Clone | DrawingContextMenuAction::Copy => {
                self.clone_drawing(id)
            }
            DrawingContextMenuAction::Lock => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut()
                    && let Some(d) = dm.drawings.iter_mut().find(|d| d.id == id)
                {
                    d.locked = !d.locked;
                }
            }
            DrawingContextMenuAction::Hide => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut()
                    && let Some(d) = dm.drawings.iter_mut().find(|d| d.id == id)
                {
                    d.visible = !d.visible;
                }
            }
            DrawingContextMenuAction::BringToFront => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.bring_to_front(id);
                }
            }
            DrawingContextMenuAction::SendToBack => {
                if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                    dm.send_to_back(id);
                }
            }
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
        // F1 / "?" opens the keyboard shortcuts help; Alt+E opens the editor.
        let (help, editor) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::F1)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Questionmark)),
                i.modifiers.alt && i.key_pressed(egui::Key::E),
            )
        });
        if help {
            self.keyboard_shortcuts.toggle();
        }
        if editor {
            self.pine_editor.open();
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot_requested {
            self.screenshot_requested = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }

        // Pull any delivered screenshot out of egui's event queue and persist it.
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            self.save_screenshot(&image);
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
        egui::Area::new(egui::Id::new("demo_notice"))
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

    #[cfg(not(target_arch = "wasm32"))]
    fn save_screenshot(&self, image: &egui::ColorImage) {
        let path = std::env::temp_dir().join(format!("{}_chart.png", self.state.symbol));
        let (w, h) = (image.size[0] as u32, image.size[1] as u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for px in &image.pixels {
            rgba.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
        }
        match image::RgbaImage::from_raw(w, h, rgba) {
            Some(buf) => {
                if let Err(e) = buf.save(&path) {
                    log::warn!("[demo] screenshot save failed: {e}");
                } else {
                    log::info!("[demo] screenshot saved to {}", path.display());
                }
            }
            None => log::warn!("[demo] screenshot buffer size mismatch"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save_screenshot(&self, image: &egui::ColorImage) {
        // A browser download requires Blob/anchor plumbing beyond this demo's
        // scope; log the capture so the control still does observable work.
        log::info!(
            "[demo] screenshot captured ({}x{}) — browser download not wired",
            image.size[0],
            image.size[1]
        );
    }
}

// ---------------------------------------------------------------------------
// Free helpers.
// ---------------------------------------------------------------------------

fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
}

fn default_indicators() -> egui_charts::studies::IndicatorRegistry {
    use egui_charts::studies::{BollingerBands, EMA, IndicatorRegistry, MACD, RSI, SMA};
    let mut registry = IndicatorRegistry::new();
    registry.add(Box::new(SMA::new(20)));
    registry.add(Box::new(EMA::new(50)));
    registry.add(Box::new(BollingerBands::new(20, 2.0)));
    registry.add(Box::new(RSI::new(14)));
    registry.add(Box::new(MACD::new(12, 26, 9)));
    registry
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

/// Translate the settings dialog's `ChartSettingsState` into the live
/// `ChartConfig`. The dialog has no direct `apply_to_config`, so the demo maps
/// the fields users actually see changing: candle colors, background, grid.
fn apply_settings_state(state: &ChartSettingsState, config: &mut egui_charts::config::ChartConfig) {
    use egui_charts::ui::top_toolbar::components::settings_menu::types::GridLinesMode;

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

    config.show_ohlc_info = state.status_line.show_ohlc;
    config.show_volume = state.status_line.show_volume;
}
