//! Tabbed dialog for editing chart display settings.
//!
//! Open [`SettingsDialog`] with the chart's settings and apply the returned
//! [`SettingsAction`] in the host.

use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::widget::settings::{ChartSettingsState, PrecisionMode};
mod tabs;

use crate::icons::icons as embedded_icons;
use crate::services::StoreKind;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui::dialogs::templates::TemplateStore;
use crate::ui::top_toolbar::components::settings_menu::{
    actions::SettingsAction, config::SettingsDialogConfig, types::SettingsTab,
};
use crate::ui_kit::ColorPickerState;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};
use egui::{Color32, Context, Rect, Response, RichText, Ui, Vec2};
use std::collections::HashSet;

// ============================================================================
// Settings Dialog (Modal)
// ============================================================================

/// Width of the tab sidebar, before it yields to narrow windows.
const SIDEBAR_WIDTH: f32 = 206.0;

/// Outer height of the tallest tab, whose centred top edge every tab keeps.
const TALLEST_TAB_HEIGHT: f32 = 840.0;

/// Modal settings dialog with tabbed interface.
///
/// Edits chart display settings, event category visibility and the host trading and
/// alert-overlay preferences.
#[derive(Debug)]
pub struct SettingsDialog {
    /// Whether the dialog is currently open
    pub is_open: bool,
    active_tab: SettingsTab,
    settings: ChartSettingsState,
    /// Current template name (empty if using default/unsaved settings)
    template_name: String,
    /// Whether the template dropdown menu is open
    template_menu_open: bool,
    /// Host-readable chart templates and their optional storage connection.
    pub templates: TemplateStore<ChartSettingsState>,
    save_name: Option<String>,
    timezones: Vec<crate::model::TimezoneId>,
    /// Dialog styling configuration (width, height overrides)
    config: SettingsDialogConfig,
    color_picker_states: ColorPickerStates,
    margin_top_str: String,
    margin_bottom_str: String,
    margin_right_str: String,
    /// Controls the host hid.
    hidden: HashSet<Control>,
    apply_to_all: bool,
    /// Whether the Symbol tab offers the futures data choices.
    futures_data: bool,
    /// The local sounds the host can play, by name, offered on the Trading and Alerts tabs. The host
    /// plays the chosen one; see
    /// [`Trading::take_execution_sound`](crate::ui::trading::Trading::take_execution_sound).
    /// Without any, the tab offers no choice.
    pub execution_sounds: Vec<String>,
    sound_preview: Option<(String, f32)>,
    /// Heatmap, book, individual orders and bubbles offered by the host.
    order_flow_layers: [bool; 4],
    /// Sampling used to estimate depth history memory and the longest retention offered.
    depth_config: crate::data::DepthHistoryConfig,
    release_depth_history: bool,
    /// Whether the host's feed answers analyst price targets for the chart's instrument.
    price_targets_offered: bool,
    /// Whether regular/extended and overnight hours can be chosen.
    hours_offered: (bool, bool),
    /// The axis font family the dialog was opened with, shown while the settings hold none.
    opened_font_family: Option<egui::FontFamily>,
    /// The precision the dialog was opened with, listed while the list lacks it.
    opened_precision: PrecisionMode,
    /// The top and bottom margins in percent the margin rows were last filled with.
    loaded_margins: (f32, f32),
}

#[derive(Clone, Debug, Default)]
struct ColorPickerStates {
    body_up: ColorPickerState,
    body_down: ColorPickerState,
    border_up: ColorPickerState,
    border_down: ColorPickerState,
    wick_up: ColorPickerState,
    wick_down: ColorPickerState,
    background: ColorPickerState,
    background_gradient_top: ColorPickerState,
    background_gradient_bottom: ColorPickerState,
    grid_h: ColorPickerState,
    grid_v: ColorPickerState,
    pane_separator: ColorPickerState,
    crosshair: ColorPickerState,
    watermark: ColorPickerState,
    scales_text: ColorPickerState,
    scales_lines: ColorPickerState,
    alert: ColorPickerState,
    symbol_value: ColorPickerState,
    study_value: ColorPickerState,
    extended_value: ColorPickerState,
    price_target_positive: ColorPickerState,
    price_target_negative: ColorPickerState,
    prev_close_line: ColorPickerState,
    session_background: [ColorPickerState; 3],
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    /// Create a [`SettingsDialog`] with default settings.
    pub fn new() -> Self {
        let settings = ChartSettingsState::default();
        Self {
            is_open: false,
            active_tab: SettingsTab::Symbol,
            color_picker_states: ColorPickerStates {
                body_up: ColorPickerState::new(settings.candle_colors.body_up),
                body_down: ColorPickerState::new(settings.candle_colors.body_down),
                border_up: ColorPickerState::new(settings.candle_colors.border_up),
                border_down: ColorPickerState::new(settings.candle_colors.border_down),
                wick_up: ColorPickerState::new(settings.candle_colors.wick_up),
                wick_down: ColorPickerState::new(settings.candle_colors.wick_down),
                background: ColorPickerState::new(settings.chart_basic_styles.background_color),
                background_gradient_top: ColorPickerState::new(
                    settings.chart_basic_styles.background_gradient_top,
                ),
                background_gradient_bottom: ColorPickerState::new(
                    settings.chart_basic_styles.background_gradient_bottom,
                ),
                grid_h: ColorPickerState::new(settings.grid_lines.horizontal_color),
                grid_v: ColorPickerState::new(settings.grid_lines.vertical_color),
                pane_separator: ColorPickerState::default(),
                crosshair: ColorPickerState::new(settings.crosshair.color),
                watermark: ColorPickerState::new(settings.watermark.color),
                scales_text: ColorPickerState::new(settings.scales_appearance.text_color),
                scales_lines: ColorPickerState::new(settings.scales_appearance.lines_color),
                alert: ColorPickerState::new(settings.alerts.alert_color),
                prev_close_line: ColorPickerState::new(Color32::PLACEHOLDER),
                session_background: Default::default(),
                symbol_value: ColorPickerState::new(
                    settings
                        .scales_and_lines
                        .symbol_price_color
                        .unwrap_or(settings.candle_colors.body_up),
                ),
                study_value: ColorPickerState::new(
                    settings
                        .scales_and_lines
                        .study_value_color
                        .unwrap_or(settings.scales_appearance.text_color),
                ),
                extended_value: ColorPickerState::new(
                    settings.scales_and_lines.extended_session_price_color,
                ),
                price_target_positive: ColorPickerState::default(),
                price_target_negative: ColorPickerState::default(),
            },
            margin_top_str: format!("{}", settings.margins.top_percent),
            margin_bottom_str: format!("{}", settings.margins.bottom_percent),
            margin_right_str: format!("{}", settings.margins.right_bars),
            settings,
            template_name: String::new(),
            template_menu_open: false,
            templates: TemplateStore::new(StoreKind::ChartTemplates),
            save_name: None,
            timezones: crate::ui::timeframe_toolbar::default_time_zones(),
            config: SettingsDialogConfig::default(),
            hidden: HashSet::new(),
            apply_to_all: false,
            futures_data: false,
            execution_sounds: Vec::new(),
            sound_preview: None,
            order_flow_layers: [true; 4],
            depth_config: Default::default(),
            release_depth_history: false,
            price_targets_offered: true,
            hours_offered: (true, true),
            opened_font_family: None,
            opened_precision: PrecisionMode::Default,
            loaded_margins: (0.0, 0.0),
        }
    }

    /// Offers `zones` in the Symbol tab's time zone list after UTC, exchange time and local time,
    /// in place of [`default_time_zones`](crate::ui::timeframe_toolbar::default_time_zones).
    pub fn set_time_zones(&mut self, zones: Vec<crate::model::TimezoneId>) {
        self.timezones = zones;
    }

    /// Offers a button that applies the dialog's settings to all charts of the layout and closes
    /// the dialog, for a host showing several charts. It returns as
    /// [`SettingsAction::ApplyToAll`]. Off by default.
    pub fn offer_apply_to_all(&mut self, offered: bool) {
        self.apply_to_all = offered;
    }

    /// Offers continuous futures back adjustment and settlement closes on the Symbol tab for
    /// a host that reloads its history for these choices. They return in
    /// [`ChartSettingsState::back_adjust_futures`] and
    /// [`ChartSettingsState::settlement_as_close`]. Off by default; the host offers these only
    /// for supported futures history. Settlement affects daily and longer bars only.
    pub fn offer_futures_data(&mut self, offered: bool) {
        self.futures_data = offered;
    }

    /// Shows or hides `control`: Templates (the template button and menu; Reset to defaults
    /// then takes the button's place). The dialog ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
        if control == Control::Templates && !visible {
            self.template_menu_open = false;
        }
    }

    /// Restrict Order flow controls and the Analyst price targets setting to services explicitly
    /// declared for the chart's instrument
    /// ([`FeedService::PriceTargets`](crate::model::FeedService::PriceTargets) for the targets).
    /// Pass [`Chart::feed_capabilities`](crate::Chart::feed_capabilities) and
    /// [`Chart::instrument`](crate::Chart::instrument) again when they change. A missing instrument
    /// hides both; a non-positive, non-finite or missing tick size hides the Order flow tab only.
    /// Before the first call, everything is offered. Hidden controls retain their saved values.
    /// The Symbol tab omits Session for an instrument without trading hours and offers overnight
    /// hours with overnight windows or while chosen; no instrument offers regular and extended hours.
    pub fn set_feed_capabilities(
        &mut self,
        capabilities: Option<&crate::data::FeedCapabilities>,
        instrument: Option<&crate::model::Instrument>,
    ) {
        use crate::model::FeedService;
        let declared = |service| {
            instrument.is_some_and(|instrument| {
                capabilities
                    .unwrap_or(&crate::data::FeedCapabilities::default())
                    .declares(service, instrument)
            })
        };
        // The tick size gates Order flow only: targets are prices from the feed.
        let ticked = instrument
            .and_then(|instrument| instrument.tick_size)
            .is_some_and(|tick| tick.is_finite() && tick > 0.0);
        let declares = |service| ticked && declared(service);
        let depth = declares(FeedService::Depth);
        self.order_flow_layers = [
            depth || declares(FeedService::DepthHistory),
            depth,
            depth && declares(FeedService::OrderDepth),
            declares(FeedService::Trades) || declares(FeedService::LiveTrades),
        ];
        self.price_targets_offered = declared(FeedService::PriceTargets);
        self.hours_offered = (
            crate::ui::timeframe_toolbar::state::has_trading_hours(instrument),
            crate::ui::timeframe_toolbar::state::has_overnight_windows(
                instrument.and_then(|instrument| instrument.session.as_ref()),
            ),
        );
        if !self.order_flow_layers.iter().any(|available| *available)
            && self.active_tab == SettingsTab::OrderFlow
        {
            self.active_tab = SettingsTab::Symbol;
        }
    }

    /// Supplies depth sampling for the Order flow tab's memory estimate and retention choices.
    /// Pass `chart.depth_history(SeriesId::MAIN).map(DepthHistory::config)` when opening the
    /// dialog. `None`, and a dialog before this call, assume default depth sampling.
    pub fn set_depth_history_config(&mut self, config: Option<&crate::data::DepthHistoryConfig>) {
        self.depth_config = config.cloned().unwrap_or_default();
    }

    /// Open the dialog with the supplied state.
    pub fn open(&mut self, settings: ChartSettingsState) {
        self.is_open = true;
        self.template_menu_open = false;
        self.save_name = None;
        self.sound_preview = None;
        self.release_depth_history = false;
        self.opened_font_family = settings.scales_appearance.font_family.clone();
        self.opened_precision = settings.precision;
        self.settings = settings;
        self.active_tab = SettingsTab::Symbol;
        self.update_color_pickers_from_settings();
    }

    /// Sync all color picker states from the current settings
    fn update_color_pickers_from_settings(&mut self) {
        self.color_picker_states.body_up.sel_color = self.settings.candle_colors.body_up;
        self.color_picker_states.body_down.sel_color = self.settings.candle_colors.body_down;
        self.color_picker_states.border_up.sel_color = self.settings.candle_colors.border_up;
        self.color_picker_states.border_down.sel_color = self.settings.candle_colors.border_down;
        self.color_picker_states.wick_up.sel_color = self.settings.candle_colors.wick_up;
        self.color_picker_states.wick_down.sel_color = self.settings.candle_colors.wick_down;
        self.color_picker_states.background.sel_color =
            self.settings.chart_basic_styles.background_color;
        self.color_picker_states.background_gradient_top.sel_color =
            self.settings.chart_basic_styles.background_gradient_top;
        self.color_picker_states
            .background_gradient_bottom
            .sel_color = self.settings.chart_basic_styles.background_gradient_bottom;
        self.color_picker_states.grid_h.sel_color = self.settings.grid_lines.horizontal_color;
        self.color_picker_states.grid_v.sel_color = self.settings.grid_lines.vertical_color;
        self.color_picker_states.crosshair.sel_color = self.settings.crosshair.color;
        self.color_picker_states.watermark.sel_color = self.settings.watermark.color;
        self.color_picker_states.scales_text.sel_color = self.settings.scales_appearance.text_color;
        self.color_picker_states.scales_lines.sel_color =
            self.settings.scales_appearance.lines_color;

        self.color_picker_states.alert.sel_color = self.settings.alerts.alert_color;
        self.color_picker_states.symbol_value.sel_color = self
            .settings
            .scales_and_lines
            .symbol_price_color
            .unwrap_or(self.settings.candle_colors.body_up);
        self.color_picker_states.study_value.sel_color = self
            .settings
            .scales_and_lines
            .study_value_color
            .unwrap_or(self.settings.scales_appearance.text_color);
        self.color_picker_states.extended_value.sel_color =
            self.settings.scales_and_lines.extended_session_price_color;

        self.margin_top_str = format!("{}", self.settings.margins.top_percent);
        self.margin_bottom_str = format!("{}", self.settings.margins.bottom_percent);
        self.margin_right_str = format!("{}", self.settings.margins.right_bars);
        self.loaded_margins = (
            self.settings.margins.top_percent,
            self.settings.margins.bottom_percent,
        );
    }

    /// Close the dialog.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Read the display settings being edited.
    pub fn settings(&self) -> &ChartSettingsState {
        &self.settings
    }

    /// Show the dialog and return the requested action.
    pub fn show(&mut self, ctx: &Context, theme: &Theme) -> SettingsAction {
        self.templates.poll(ctx);
        let mut action = SettingsAction::None;

        if !self.is_open {
            return action;
        }

        let basic = &self.settings.chart_basic_styles;
        for (picker, color, themed) in [
            (
                &mut self.color_picker_states.background,
                basic.background_color,
                theme.chart.bg,
            ),
            (
                &mut self.color_picker_states.background_gradient_top,
                basic.background_gradient_top,
                theme.chart.bg,
            ),
            (
                &mut self.color_picker_states.background_gradient_bottom,
                basic.background_gradient_bottom,
                theme.chart.bg_axis,
            ),
            (
                &mut self.color_picker_states.crosshair,
                self.settings.crosshair.color,
                theme.chart.crosshair_line,
            ),
            (
                &mut self.color_picker_states.price_target_positive,
                self.settings
                    .price_target_positive_color
                    .unwrap_or(Color32::PLACEHOLDER),
                crate::config::ChartConfig::price_target_colors(
                    Color32::PLACEHOLDER,
                    theme.chart.candle_up,
                )
                .1,
            ),
            (
                &mut self.color_picker_states.price_target_negative,
                self.settings
                    .price_target_negative_color
                    .unwrap_or(Color32::PLACEHOLDER),
                crate::config::ChartConfig::price_target_colors(
                    Color32::PLACEHOLDER,
                    theme.chart.candle_down,
                )
                .1,
            ),
            (
                &mut self.color_picker_states.prev_close_line,
                self.settings
                    .scales_and_lines
                    .prev_close_line
                    .and_then(|line| line.color)
                    .unwrap_or(Color32::PLACEHOLDER),
                crate::config::ChartConfig::prev_close_fallback(theme),
            ),
        ] {
            picker.sel_color = if color == Color32::PLACEHOLDER {
                themed
            } else {
                color
            };
        }

        // Until a colour is chosen the well shows the theme's, which the separators follow.
        self.color_picker_states.pane_separator.sel_color = self
            .settings
            .pane_separator_color
            .unwrap_or(theme.chart.grid_line);

        let mut is_open = self.is_open;
        let fixed_height = (self.config.height > 0.0).then_some(self.config.height);
        let height = fixed_height.unwrap_or(match self.active_tab {
            SettingsTab::Symbol => 616.0,
            SettingsTab::Alerts | SettingsTab::Events => 500.0,
            _ => TALLEST_TAB_HEIGHT,
        });
        let width = if self.config.width > 0.0 {
            self.config.width
        } else {
            752.0
        };
        let screen = ctx.content_rect();
        // The tallest tab sits centred in the window; shorter tabs keep its top edge.
        let tallest_height = fixed_height.unwrap_or(TALLEST_TAB_HEIGHT);
        let tallest_height =
            tallest_height.min((screen.height() - 2.0 * ctx.active_theme().spacing.xxxl).max(0.0));
        Modal::new("chart_settings", "Chart settings")
            .width(width)
            .height(height)
            .top(screen.center().y - tallest_height / 2.0)
            .dim_background(false)
            .show(ctx, &mut is_open, |ui| {
                ui.add_space(theme.spacing.xxxl);
                action = self.draw_content(ui, theme);
            });

        // Only update is_open from egui's Window if btns didn't already close it
        // (btns set self.is_open = false directly)
        if self.is_open && !is_open {
            // Window was closed via egui's mechanism (outside click, etc)
            self.is_open = false;
            action = SettingsAction::Cancel;
        }

        if let SettingsAction::Apply(settings) = &mut action {
            let basic = &mut settings.chart_basic_styles;
            for (color, themed) in [
                (&mut basic.background_color, theme.chart.bg),
                (&mut basic.background_gradient_top, theme.chart.bg),
                (&mut basic.background_gradient_bottom, theme.chart.bg_axis),
            ] {
                if *color == Color32::PLACEHOLDER {
                    *color = themed;
                }
            }
        }

        if let Some((name, volume)) = self.sound_preview.take() {
            return SettingsAction::PreviewSound { name, volume };
        }
        if std::mem::take(&mut self.release_depth_history) {
            return SettingsAction::ClearDepthHistory;
        }
        action
    }

    fn draw_content(&mut self, ui: &mut Ui, theme: &Theme) -> SettingsAction {
        let mut action = SettingsAction::None;

        let content_height = (ui.available_height() - 68.0).max(0.0);

        let sidebar_width = SIDEBAR_WIDTH.min(ui.available_width() * 0.4);
        let mut active = self.active_tab;
        let order_flow_available = self.order_flow_layers.iter().any(|available| *available);
        ui.allocate_ui(Vec2::new(ui.available_width(), content_height), |ui| {
            SidebarLayout::new(sidebar_width).show(
                ui,
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("settings_tabs")
                        .show(ui, |ui| {
                            for tab in SettingsTab::all() {
                                if *tab == SettingsTab::OrderFlow && !order_flow_available {
                                    continue;
                                }
                                if SidebarTab::new(tab.name(), active == *tab)
                                    .icon(tab.icon())
                                    .show(ui)
                                {
                                    active = *tab;
                                }
                            }
                        });
                },
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(theme.spacing.lg);
                            match self.active_tab {
                                SettingsTab::Symbol => self.draw_symbol_tab(ui, theme),
                                SettingsTab::OrderFlow => self.draw_order_flow_tab(ui, theme),
                                SettingsTab::StatusLine => self.draw_status_line_tab(ui, theme),
                                SettingsTab::ScalesAndLines => {
                                    self.draw_scales_and_lines_tab(ui, theme)
                                }
                                SettingsTab::Canvas => self.draw_canvas_tab(ui, theme),
                                SettingsTab::Trading => self.draw_trading_tab(ui, theme),
                                SettingsTab::Alerts => self.draw_alerts_tab(ui, theme),
                                SettingsTab::Events => self.draw_events_tab(ui, theme),
                            }
                        });
                },
            );
        });
        self.active_tab = active;

        let error = self
            .settings
            .validate()
            .err()
            .map(|error| error.to_string())
            .or_else(|| {
                // Margins leaving no room for prices are kept when unedited; the fit ignores them.
                let (top, bottom) = (
                    self.settings.margins.top_percent,
                    self.settings.margins.bottom_percent,
                );
                (top + bottom >= 100.0 && (top, bottom) != self.loaded_margins).then(|| {
                    crate::widget::settings::ChartSettingsError::Margins { top, bottom }.to_string()
                })
            })
            .or_else(|| {
                (self.margin_top_str.parse::<f32>().is_err()
                    || self.margin_bottom_str.parse::<f32>().is_err()
                    || self.margin_right_str.parse::<f32>().is_err())
                .then(|| "Enter valid margins".to_owned())
            });
        let (ok, cancel) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .primary_enabled(error.is_none())
            .show_with_left(ui, |ui| {
                if let Some(error) = &error {
                    crate::ui_kit::status_message::status_message(
                        ui,
                        crate::ui_kit::status_message::MessageKind::Error,
                        error,
                    );
                } else if self.hidden.contains(&Control::Templates) {
                    self.draw_reset_button(ui, theme);
                } else {
                    let button = self.draw_template_button(ui, theme);
                    if button.clicked() {
                        self.template_menu_open = !self.template_menu_open;
                    }
                    if self.template_menu_open {
                        action = self.draw_template_menu(ui, theme, &button, action.clone());
                    }
                }
                if error.is_none()
                    && self.apply_to_all
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new(l10n::text(ui.ctx(), "Apply to all"))
                                    .size(theme.text.title)
                                    .color(theme.ui.text),
                            )
                            .fill(theme.ui.panel_bg)
                            .stroke(egui::Stroke::new(
                                DESIGN_TOKENS.stroke.hairline,
                                theme.ui.border_control,
                            ))
                            .corner_radius(theme.radius.md as u8)
                            .min_size(Vec2::new(
                                0.0,
                                DESIGN_TOKENS.sizing.settings_dialog.button_height,
                            )),
                        )
                        .clicked()
                {
                    action = SettingsAction::ApplyToAll(Box::new(self.settings.clone()));
                    self.is_open = false;
                }
            });
        if ok {
            action = SettingsAction::Apply(Box::new(self.settings.clone()));
            self.is_open = false;
        } else if cancel {
            action = SettingsAction::Cancel;
            self.is_open = false;
        }
        action
    }

    fn draw_template_button(&self, ui: &mut Ui, theme: &Theme) -> Response {
        let template_label = if self.template_name.is_empty() {
            "Template".to_string()
        } else {
            self.template_name.clone()
        };
        ui.add_sized(
            Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.template_button_width,
                DESIGN_TOKENS.sizing.settings_dialog.button_height,
            ),
            egui::Button::new((
                RichText::new(l10n::text(ui.ctx(), &template_label))
                    .size(theme.text.title)
                    .color(theme.ui.text),
                embedded_icons::CHEVRON_DOWN.as_image_tinted(Vec2::splat(12.0), theme.ui.text),
            ))
            .truncate()
            .fill(theme.ui.panel_bg)
            .stroke(egui::Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                theme.ui.border_control,
            ))
            .corner_radius(theme.radius.md as u8)
            .min_size(Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.template_button_width,
                DESIGN_TOKENS.sizing.settings_dialog.button_height,
            )),
        )
        .on_hover_text(l10n::text(ui.ctx(), template_label))
    }

    fn draw_template_menu(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        template_btn: &Response,
        action: SettingsAction,
    ) -> SettingsAction {
        let menu_id = ui.scope_id().with("template_menu");
        let menu_rect = Rect::from_min_size(
            template_btn.rect.left_bottom() + Vec2::new(0.0, theme.spacing.xs),
            Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.template_menu_width,
                DESIGN_TOKENS
                    .sizing
                    .settings_dialog
                    .template_menu_max_height,
            ),
        );

        let result = egui::Area::new(menu_id)
            .fixed_pos(menu_rect.min)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme.ui.panel_bg)
                    .corner_radius(theme.radius.md as u8)
                    .stroke(egui::Stroke::new(
                        DESIGN_TOKENS.stroke.hairline,
                        theme.ui.border_control,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(DESIGN_TOKENS.sizing.settings_dialog.template_menu_width);
                        egui::ScrollArea::vertical()
                            .max_height(
                                DESIGN_TOKENS
                                    .sizing
                                    .settings_dialog
                                    .template_menu_max_height,
                            )
                            .show(ui, |ui| self.draw_template_menu_items(ui, theme, action))
                            .inner
                    })
                    .inner
            });

        self.handle_template_menu_close(ui, template_btn, result.response.rect);
        result.inner
    }

    fn draw_template_menu_items(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        mut action: SettingsAction,
    ) -> SettingsAction {
        action = self.draw_save_as_button(ui, action);
        action = self.draw_update_current_button(ui, action);
        ui.separator();
        action = self.draw_template_list(ui, theme, action);
        ui.separator();
        self.draw_reset_button(ui, theme);
        action
    }

    fn draw_save_as_button(&mut self, ui: &mut Ui, mut action: SettingsAction) -> SettingsAction {
        if let Some(name) = &mut self.save_name {
            let label = ui.label(l10n::text(ui.ctx(), "Template name"));
            ui.add(egui::TextEdit::singleline(name).desired_width(ui.available_width()))
                .labelled_by(label.id);
            let name = name.trim().to_owned();
            let label = if self.templates.entries().contains_key(&name) {
                "Replace"
            } else {
                "Save"
            };
            let save = ui
                .add_enabled(
                    self.templates.ready() && !name.is_empty(),
                    egui::Button::new(l10n::text(ui.ctx(), label)),
                )
                .clicked();
            let cancel = ui.button(l10n::text(ui.ctx(), "Cancel")).clicked();
            if save {
                match self.templates.save(&name, self.settings.clone()) {
                    Ok(()) => {
                        self.template_name = name.clone();
                        action = SettingsAction::SaveTemplate(name);
                        self.save_name = None;
                    }
                    Err(error) => self.templates.error = Some(error),
                }
            } else if cancel {
                self.save_name = None;
            }
        } else if ui
            .button(
                RichText::new(l10n::text(ui.ctx(), "Save as...")).size(ui.active_theme().text.body),
            )
            .clicked()
        {
            self.save_name = Some(String::new());
        }
        if let Some(error) = &self.templates.error {
            ui.colored_label(ui.visuals().error_fg_color, l10n::text(ui.ctx(), error));
            if !self.templates.ready()
                && ui
                    .button(l10n::text(ui.ctx(), "Retry loading templates"))
                    .clicked()
            {
                self.templates.reload();
            }
        }

        action
    }

    fn draw_update_current_button(
        &mut self,
        ui: &mut Ui,
        mut action: SettingsAction,
    ) -> SettingsAction {
        if !self.template_name.is_empty()
            && ui
                .add_enabled(
                    self.templates.ready(),
                    egui::Button::new(
                        RichText::new(l10n::text(ui.ctx(), "Update current"))
                            .size(ui.active_theme().text.body),
                    ),
                )
                .clicked()
        {
            match self
                .templates
                .save(&self.template_name, self.settings.clone())
            {
                Ok(()) => action = SettingsAction::SaveTemplate(self.template_name.clone()),
                Err(error) => self.templates.error = Some(error),
            }
        }
        action
    }

    fn draw_template_list(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        mut action: SettingsAction,
    ) -> SettingsAction {
        let templates: Vec<String> = self.templates.entries().keys().cloned().collect();

        if templates.is_empty() {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "No saved templates"))
                    .size(theme.text.small)
                    .color(theme.ui.text_secondary),
            );
        } else {
            for name in templates {
                action = self.draw_template_item(ui, theme, name, action);
            }
        }
        action
    }

    fn draw_template_item(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        name: String,
        mut action: SettingsAction,
    ) -> SettingsAction {
        let is_sel = self.template_name == name;
        let text = if is_sel {
            RichText::new(l10n::text(ui.ctx(), &name))
                .size(theme.text.body)
                .color(theme.ui.text)
        } else {
            RichText::new(l10n::text(ui.ctx(), &name)).size(theme.text.body)
        };

        if ui
            .add_enabled(self.templates.ready(), egui::Button::new(text).truncate())
            .on_hover_text(l10n::text(ui.ctx(), &name))
            .clicked()
        {
            if let Some(template) = self.templates.entries().get(&name) {
                let kind = self.settings.chart_type;
                let params = template
                    .chart_type_params
                    .clone()
                    .or_else(|| self.settings.chart_type_params.clone());
                let source = template.price_source.or(self.settings.price_source);
                // Choices a template saved before they existed lacks stay as the dialog shows
                // them. Its font family stays unset: that keeps the chart's, and tells
                // `apply_to_chart` that the template holds the whole bars of a right margin.
                let scales = &self.settings.scales_and_lines;
                let line_source = template
                    .scales_and_lines
                    .price_line_source
                    .or(scales.price_line_source);
                let line_width = template
                    .scales_and_lines
                    .price_line_width
                    .or(scales.price_line_width);
                let line_style = template
                    .scales_and_lines
                    .price_line_style
                    .or(scales.price_line_style);
                let bid_ask_lines = template
                    .scales_and_lines
                    .show_bid_ask_lines
                    .or(scales.show_bid_ask_lines);
                let high_low_lines = template
                    .scales_and_lines
                    .show_high_low_lines
                    .or(scales.show_high_low_lines);
                let prev_close_line = template
                    .scales_and_lines
                    .prev_close_line
                    .or(scales.prev_close_line);
                let separator = template
                    .scales_and_lines
                    .thousands_sep
                    .or(scales.thousands_sep);
                let extended_hours = template.extended_hours.or(self.settings.extended_hours);
                let overnight_hours = template.overnight_hours.or(self.settings.overnight_hours);
                let session_background = template
                    .session_background
                    .clone()
                    .or_else(|| self.settings.session_background.clone());
                let date_format = template
                    .scales_and_lines
                    .date_format
                    .clone()
                    .or_else(|| scales.date_format.clone());
                let use_24_hour = template.scales_and_lines.use_24_hour.or(scales.use_24_hour);
                let weekday_names = template
                    .scales_and_lines
                    .weekday_names
                    .or(scales.weekday_names);
                let session_breaks = template
                    .show_session_breaks
                    .or(self.settings.show_session_breaks);
                let crosshair_lines = template.crosshair_lines.or(self.settings.crosshair_lines);
                let positive = template
                    .price_target_positive_color
                    .or(self.settings.price_target_positive_color);
                let negative = template
                    .price_target_negative_color
                    .or(self.settings.price_target_negative_color);
                // The whole bars of the margin shown, or its pixels, are an unedited margin, which
                // stays as shown.
                let shown = &self.settings.margins;
                let right_bars = if (template.scales_appearance.font_family.is_none()
                    && template.margins.right_bars == shown.right_bars.trunc())
                    || (template.margins.right_pixels.is_some()
                        && template.margins.right_pixels == shown.right_pixels)
                {
                    shown.right_bars
                } else {
                    template.margins.right_bars
                };
                self.settings = template.clone();
                self.settings.chart_type = kind;
                self.settings.chart_type_params = params;
                self.settings.price_source = source;
                self.settings.scales_and_lines.price_line_source = line_source;
                self.settings.scales_and_lines.price_line_width = line_width;
                self.settings.scales_and_lines.price_line_style = line_style;
                self.settings.scales_and_lines.show_bid_ask_lines = bid_ask_lines;
                self.settings.scales_and_lines.show_high_low_lines = high_low_lines;
                self.settings.scales_and_lines.prev_close_line = prev_close_line;
                self.settings.scales_and_lines.thousands_sep = separator;
                self.settings.scales_and_lines.date_format = date_format;
                self.settings.scales_and_lines.use_24_hour = use_24_hour;
                self.settings.scales_and_lines.weekday_names = weekday_names;
                self.settings.show_session_breaks = session_breaks;
                self.settings.crosshair_lines = crosshair_lines;
                self.settings.extended_hours = extended_hours;
                self.settings.overnight_hours = overnight_hours;
                self.settings.session_background = session_background;
                self.settings.price_target_positive_color = positive;
                self.settings.price_target_negative_color = negative;
                self.settings.margins.right_bars = right_bars;
                // A template saved before volume candles had a style shows what they drew, which
                // Ok then applies with any edits made before it.
                if kind == crate::model::ChartType::VolumeCandles
                    && let Some(params) = &mut self.settings.chart_type_params
                    && (template.chart_type_params.is_none()
                        || params.styles.volume_candles.is_saved_without_style())
                {
                    let colors = &mut self.settings.candle_colors;
                    let style = crate::model::VolumeCandlesStyle::drawn_before_style(
                        colors.body_up,
                        colors.body_down,
                    );
                    *colors = crate::widget::settings::CandleColorConfig {
                        color_by_previous_close: colors.color_by_previous_close,
                        show_body: style.body_visible,
                        show_border: style.border_visible,
                        show_wick: style.wick_visible,
                        body_up: style.body_up,
                        body_down: style.body_down,
                        border_up: style.border_up,
                        border_down: style.border_down,
                        wick_up: style.wick_up,
                        wick_down: style.wick_down,
                    };
                    params.styles.volume_candles = style;
                }
                self.template_name = name.clone();
                self.update_color_pickers_from_settings();
                action = SettingsAction::LoadTemplate(name.clone());
            }
            self.template_menu_open = false;
        }
        if ui
            .add_enabled(
                self.templates.ready(),
                egui::Button::new(l10n::format_text(
                    ui.ctx(),
                    "Delete {name}",
                    &[("name", name.to_string())],
                ))
                .truncate(),
            )
            .clicked()
        {
            match self.templates.delete(&name) {
                Ok(()) => {
                    if self.template_name == name {
                        self.template_name.clear();
                    }
                }
                Err(error) => self.templates.error = Some(error),
            }
        }
        action
    }

    fn draw_reset_button(&mut self, ui: &mut Ui, theme: &Theme) {
        if ui
            .button(
                RichText::new(l10n::text(ui.ctx(), "Reset to defaults"))
                    .size(ui.active_theme().text.body),
            )
            .clicked()
        {
            self.settings = ChartSettingsState {
                chart_type: self.settings.chart_type,
                chart_type_params: Some(Default::default()),
                price_source: Some(Default::default()),
                show_session_breaks: Some(false),
                crosshair_lines: Some(Default::default()),
                price_target_positive_color: Some(Color32::PLACEHOLDER),
                price_target_negative_color: Some(Color32::PLACEHOLDER),
                ..Default::default()
            };
            // What a new chart holds, where the default settings hold no choice.
            self.settings.extended_hours = Some(false);
            self.settings.session_background = Some(Default::default());
            self.settings.scales_appearance.font_family = Some(Default::default());
            self.settings.scales_and_lines.price_line_source = Some(Default::default());
            let style = crate::model::SeriesStyle::default();
            self.settings.scales_and_lines.price_line_width = Some(style.price_line_width);
            self.settings.scales_and_lines.price_line_style = Some(style.price_line_style);
            self.settings.scales_and_lines.show_bid_ask_lines = Some(false);
            self.settings.scales_and_lines.show_high_low_lines = Some(false);
            self.settings.scales_and_lines.prev_close_line = Some(Default::default());
            self.settings.scales_and_lines.thousands_sep =
                Some(crate::widget::settings::ThousandsSeparator::Comma);
            self.settings.scales_and_lines.date_format =
                Some(crate::widget::settings::DateFormat::Default);
            self.settings.scales_and_lines.use_24_hour = Some(true);
            self.settings.scales_and_lines.weekday_names = Some(Default::default());
            let palette = crate::config::ChartConfig::default().with_theme(theme);
            // The default styles of the chart types hold the dark preset's candle colours.
            let params = self.settings.chart_type_params.get_or_insert_default();
            crate::widget::palette::follow_candle_colors(
                &mut params.styles,
                &[crate::config::ChartConfig::default()],
                &palette,
            );
            self.settings.candle_colors =
                crate::widget::settings::CandleColorConfig::from_theme(theme);
            self.settings.scales_appearance.text_color = theme.chart.axis_text;
            self.settings.grid_lines.horizontal_color = theme.chart.grid_line;
            self.settings.grid_lines.vertical_color = theme.chart.grid_line;
            // What a new chart holds: the colour follows the theme until one is chosen.
            self.settings.crosshair.color = Color32::PLACEHOLDER;
            self.settings.watermark.color = palette.watermark_color;
            if matches!(
                self.settings.chart_type,
                crate::model::ChartType::Heikin
                    | crate::model::ChartType::HollowCandles
                    | crate::model::ChartType::VolumeCandles
            ) {
                self.settings.candle_colors.show_border = true;
            }
            self.template_name.clear();
            self.update_color_pickers_from_settings();
            self.template_menu_open = false;
        }
    }

    fn handle_template_menu_close(&mut self, ui: &Ui, template_btn: &Response, menu_rect: Rect) {
        if ui.input(|i| i.pointer.any_click())
            && !template_btn.hovered()
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && !menu_rect.contains(pos)
        {
            self.template_menu_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    #[test]
    fn reset_colours_match_a_new_chart_under_each_theme() {
        // The dialog is shown with `theme` while the application theme is `app`; `prev_close`
        // is the previous close well's colour without a colour of its own, which follows `theme`.
        for (preset, theme, app, prev_close) in [
            (
                "light",
                Theme::light(),
                Theme::light(),
                Color32::from_rgb(85, 85, 85),
            ),
            (
                "dark",
                Theme::dark(),
                Theme::dark(),
                Color32::from_rgb(184, 184, 184),
            ),
            (
                "high contrast",
                Theme::high_contrast(),
                Theme::high_contrast(),
                Color32::from_rgb(235, 235, 235),
            ),
            (
                "light dialog in a dark app",
                Theme::light(),
                Theme::dark(),
                Color32::from_rgb(85, 85, 85),
            ),
            (
                "dark dialog in a light app",
                Theme::dark(),
                Theme::light(),
                Color32::from_rgb(184, 184, 184),
            ),
            (
                "high contrast dialog in a light app",
                Theme::high_contrast(),
                Theme::light(),
                Color32::from_rgb(235, 235, 235),
            ),
        ] {
            for (templates_visible, size) in [
                (true, Vec2::new(1440.0, 900.0)),
                (false, Vec2::new(1440.0, 900.0)),
                (true, Vec2::new(320.0, 450.0)),
                (false, Vec2::new(320.0, 450.0)),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &app);
                let mut chart = crate::ChartBuilder::new().with_theme(theme.clone()).build();
                let expected = ChartSettingsState::from_chart(&chart);
                chart.config_mut().price_target_positive_color = Color32::RED;
                chart.config_mut().price_target_negative_color = Color32::GREEN;
                chart.pane_options_mut().separator_color = Some(Color32::GREEN);
                chart.set_date_format(Some("%d.%m.%Y".into()));
                let mut options = chart.chart_options().clone();
                options.crosshair.vert_line_visible = false;
                options.crosshair.horz_line_visible = false;
                options.crosshair.vert_label_visible = false;
                options.crosshair.horz_label_visible = false;
                options.time_scale.use_24_hour = false;
                options.time_scale.weekday_names = crate::scales::WeekdayNames::Never;
                chart.set_chart_options(options);
                // A customised last price line, which Reset returns to a new chart's.
                if let Some(style) = chart.series_style_mut(crate::widget::SeriesId::MAIN) {
                    style.price_line_width = 3.0;
                    style.price_line_style = crate::model::LineStyle::Solid;
                }
                // A chosen previous close colour, which Reset clears, so the well shows the
                // theme's again.
                chart.config_mut().prev_close_line.color = Some(Color32::GOLD);
                chart.config_mut().prev_close_line.width = 3.0;
                chart.config_mut().show_bid_ask_lines = Some(true);
                chart.config_mut().show_high_low_lines = Some(true);
                let mut settings = ChartSettingsState::from_chart(&chart);
                settings.scales_appearance.text_color = Color32::RED;
                settings.grid_lines.horizontal_color = Color32::GREEN;
                settings.grid_lines.vertical_color = Color32::BLUE;
                settings.crosshair.color = Color32::YELLOW;
                settings.watermark.color = Color32::RED;
                settings.margins.top_percent = 12.5;
                settings.margins.bottom_percent = 7.5;
                settings.pane_separator_color = Some(Color32::GREEN);
                let mut dialog = SettingsDialog::new();
                dialog.set_control_visible(Control::Templates, templates_visible);
                dialog.open(settings);
                let mut frame = |events, label: &str| {
                    let mut action = SettingsAction::None;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            events,
                            ..Default::default()
                        },
                        |ui| action = dialog.show(ui.ctx(), &theme),
                    );
                    let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    (action, pos)
                };
                let mut action = SettingsAction::None;
                let buttons: &[&str] = if templates_visible {
                    &["Template", "Reset to defaults", "Ok"]
                } else {
                    &["Reset to defaults", "Ok"]
                };
                for label in buttons {
                    frame(vec![], label);
                    let pos = frame(vec![], label).1.expect(label);
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains(pos),
                        "{label} at {size:?}"
                    );
                    frame(vec![egui::Event::PointerMoved(pos)], label);
                    for pressed in [true, false] {
                        action = frame(
                            vec![egui::Event::PointerButton {
                                pos,
                                pressed,
                                button: egui::PointerButton::Primary,
                                modifiers: Default::default(),
                            }],
                            label,
                        )
                        .0;
                    }
                }
                let SettingsAction::Apply(settings) = action else {
                    panic!("Ok must apply the reset settings");
                };
                assert!(!dialog.is_open);
                // Saved settings and templates keep a theme-following crosshair as it is.
                let saved: ChartSettingsState =
                    serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
                assert_eq!(saved.crosshair.color, Color32::PLACEHOLDER);
                settings.apply_to_chart(&mut chart).unwrap();
                let actual = ChartSettingsState::from_chart(&chart);
                let scales = &actual.scales_and_lines;
                assert_eq!(
                    (
                        scales.date_format.clone(),
                        scales.use_24_hour,
                        scales.weekday_names
                    ),
                    (
                        Some(crate::widget::settings::DateFormat::Default),
                        Some(true),
                        Some(crate::scales::WeekdayNames::Default)
                    ),
                    "{preset}"
                );
                assert_eq!(
                    (
                        actual.scales_and_lines.price_line_width,
                        actual.scales_and_lines.price_line_style,
                        actual.scales_and_lines.prev_close_line,
                        chart
                            .config()
                            .show_bid_ask_lines
                            .unwrap_or(chart.config().show_bid_ask),
                        chart
                            .config()
                            .show_high_low_lines
                            .unwrap_or(chart.config().show_high_low),
                    ),
                    (
                        Some(1.0),
                        Some(crate::model::LineStyle::Dashed),
                        Some(crate::config::ChartLineStyle::default()),
                        false,
                        false
                    ),
                    "{preset}"
                );
                let mismatches: Vec<_> = [
                    (
                        "axis text",
                        actual.scales_appearance.text_color,
                        expected.scales_appearance.text_color,
                    ),
                    (
                        "horizontal grid",
                        actual.grid_lines.horizontal_color,
                        expected.grid_lines.horizontal_color,
                    ),
                    (
                        "vertical grid",
                        actual.grid_lines.vertical_color,
                        expected.grid_lines.vertical_color,
                    ),
                    (
                        "horizontal crosshair",
                        chart.chart_options().crosshair.horz_line_color,
                        Color32::PLACEHOLDER,
                    ),
                    (
                        "vertical crosshair",
                        chart.chart_options().crosshair.vert_line_color,
                        Color32::PLACEHOLDER,
                    ),
                    (
                        "analyst target positive",
                        chart.config().price_target_positive_color,
                        Color32::PLACEHOLDER,
                    ),
                    (
                        "analyst target negative",
                        chart.config().price_target_negative_color,
                        Color32::PLACEHOLDER,
                    ),
                    (
                        "analyst target positive swatch",
                        dialog.color_picker_states.price_target_positive.sel_color,
                        Color32::from_rgba_unmultiplied(
                            theme.chart.candle_up.r(),
                            theme.chart.candle_up.g(),
                            theme.chart.candle_up.b(),
                            51,
                        ),
                    ),
                    (
                        "analyst target negative swatch",
                        dialog.color_picker_states.price_target_negative.sel_color,
                        Color32::from_rgba_unmultiplied(
                            theme.chart.candle_down.r(),
                            theme.chart.candle_down.g(),
                            theme.chart.candle_down.b(),
                            51,
                        ),
                    ),
                    (
                        "crosshair swatch",
                        dialog.color_picker_states.crosshair.sel_color,
                        theme.chart.crosshair_line,
                    ),
                    (
                        "pane separator swatch",
                        dialog.color_picker_states.pane_separator.sel_color,
                        theme.chart.grid_line,
                    ),
                    (
                        "candle up",
                        actual.candle_colors.body_up,
                        expected.candle_colors.body_up,
                    ),
                    (
                        "candle down",
                        actual.candle_colors.body_down,
                        expected.candle_colors.body_down,
                    ),
                    (
                        "candle border up",
                        actual.candle_colors.border_up,
                        expected.candle_colors.border_up,
                    ),
                    (
                        "candle border down",
                        actual.candle_colors.border_down,
                        expected.candle_colors.border_down,
                    ),
                    (
                        "candle wick up",
                        actual.candle_colors.wick_up,
                        expected.candle_colors.wick_up,
                    ),
                    (
                        "candle wick down",
                        actual.candle_colors.wick_down,
                        expected.candle_colors.wick_down,
                    ),
                    (
                        "HLC area high line",
                        chart.chart_type_params().styles.hlc_area.high_line.color,
                        theme.chart.candle_up,
                    ),
                    (
                        "HLC area low line",
                        chart.chart_type_params().styles.hlc_area.low_line.color,
                        theme.chart.candle_down,
                    ),
                    (
                        "watermark",
                        actual.watermark.color,
                        expected.watermark.color,
                    ),
                    (
                        "previous close swatch",
                        dialog.color_picker_states.prev_close_line.sel_color,
                        prev_close,
                    ),
                ]
                .into_iter()
                .filter(|(_, actual, expected)| actual != expected)
                .collect();
                assert!(
                    mismatches.is_empty(),
                    "{preset}, templates visible: {templates_visible}, {size:?}: {mismatches:?}"
                );
                assert_eq!(
                    chart.pane_options().separator_color,
                    None,
                    "{preset}, templates visible: {templates_visible}, {size:?}: pane separators"
                );
                let crosshair = &chart.chart_options().crosshair;
                assert!(crosshair.vert_line_visible && crosshair.horz_line_visible);
                assert!(
                    crosshair.label_visible
                        && crosshair.vert_label_visible
                        && crosshair.horz_label_visible
                );
                // A new chart keeps a tenth of the pane free above, 8% below, and 2.5 bars after
                // the last one.
                let price = chart.state().price_scale().options().scale_margins;
                assert_eq!(
                    (
                        price.top,
                        price.bottom,
                        chart.chart_options().time_scale.right_offset
                    ),
                    (0.1, 0.08, 2.5),
                    "{preset}: margins"
                );
            }
        }
    }

    /// A palette swatch recolours a colour of the Canvas, Symbol, Scales and lines or Alerts tab at
    /// the opacity its well shows, which no other control of the tab sets; an invisible colour
    /// takes the swatch's opacity. Expected: the swatch rgb(49, 27, 146) at the seeded alpha of
    /// 128, and opaque where the well showed an invisible colour or the theme's opaque crosshair.
    #[test]
    fn palette_colour_keeps_the_opacity_the_well_shows_on_every_tab() {
        use crate::widget::settings::{BackgroundType, GridLinesMode};
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        type Row = fn(&mut ChartSettingsState) -> &mut Color32;
        let translucent = Color32::from_rgba_unmultiplied(255, 0, 255, 128);
        let theme = Theme::light();
        let rows: [(&str, Row, Color32, u8); 25] = [
            ("watermark", |s| &mut s.watermark.color, translucent, 128),
            (
                "background",
                |s| &mut s.chart_basic_styles.background_color,
                translucent,
                128,
            ),
            (
                "gradient top",
                |s| &mut s.chart_basic_styles.background_gradient_top,
                translucent,
                128,
            ),
            (
                "gradient bottom",
                |s| &mut s.chart_basic_styles.background_gradient_bottom,
                translucent,
                128,
            ),
            (
                "horizontal grid",
                |s| &mut s.grid_lines.horizontal_color,
                translucent,
                128,
            ),
            (
                "vertical grid",
                |s| &mut s.grid_lines.vertical_color,
                translucent,
                128,
            ),
            ("crosshair", |s| &mut s.crosshair.color, translucent, 128),
            (
                "scale text",
                |s| &mut s.scales_appearance.text_color,
                translucent,
                128,
            ),
            (
                "scale lines",
                |s| &mut s.scales_appearance.lines_color,
                translucent,
                128,
            ),
            (
                "invisible scale lines",
                |s| &mut s.scales_appearance.lines_color,
                Color32::TRANSPARENT,
                255,
            ),
            // The well shows the theme's opaque crosshair, not the translucent placeholder.
            (
                "crosshair following the theme",
                |s| &mut s.crosshair.color,
                Color32::PLACEHOLDER,
                255,
            ),
            (
                "rising body",
                |s| &mut s.candle_colors.body_up,
                translucent,
                128,
            ),
            (
                "falling border",
                |s| &mut s.candle_colors.border_down,
                translucent,
                128,
            ),
            (
                "rising wick",
                |s| &mut s.candle_colors.wick_up,
                translucent,
                128,
            ),
            (
                "symbol price",
                |s| {
                    s.scales_and_lines
                        .symbol_price_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "study values",
                |s| {
                    s.scales_and_lines
                        .study_value_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "extended-session price",
                |s| &mut s.scales_and_lines.extended_session_price_color,
                translucent,
                128,
            ),
            (
                "profile POC",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile.poc_color.get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile value area high",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile
                        .value_area_high_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile value area low",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile
                        .value_area_low_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile up volume",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile.up_volume_color.get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile down volume",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile
                        .down_volume_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile up volume in value area",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile
                        .value_area_up_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            (
                "profile down volume in value area",
                |s| {
                    let profile = &mut s.chart_type_params.get_or_insert_default().profile;
                    profile
                        .value_area_down_color
                        .get_or_insert(Color32::TRANSPARENT)
                },
                translucent,
                128,
            ),
            ("alert", |s| &mut s.alerts.alert_color, translucent, 128),
        ];
        for (name, row, seeded, alpha) in rows {
            let shown = if seeded == Color32::PLACEHOLDER {
                theme.chart.crosshair_line
            } else {
                seeded
            };
            let mut settings = ChartSettingsState::default();
            settings.grid_lines.mode = GridLinesMode::Both;
            if name.starts_with("gradient") {
                settings.chart_basic_styles.background_type = BackgroundType::VerticalGradient;
            }
            if name.starts_with("profile") {
                settings.chart_type = crate::model::ChartType::SessionVolumeProfile;
            }
            settings.candle_colors.show_border = true;
            *row(&mut settings) = seeded;
            let mut dialog = SettingsDialog::new();
            dialog.set_control_visible(Control::Templates, false);
            dialog.open(settings);
            dialog.active_tab = match name {
                "rising body" | "falling border" | "rising wick" => SettingsTab::Symbol,
                "symbol price" | "study values" | "extended-session price" => {
                    SettingsTab::ScalesAndLines
                }
                "alert" => SettingsTab::Alerts,
                _ if name.starts_with("profile") => SettingsTab::Symbol,
                _ => SettingsTab::Canvas,
            };
            let (shown_theme, setup_theme) = (theme.clone(), theme.clone());
            let mut harness = Harness::builder()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &setup_theme);
                })
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                        if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), &shown_theme)
                        {
                            *accepted = Some(settings);
                        }
                    },
                    (dialog, None),
                );
            harness.run();
            let [r, g, b, _] = shown.to_srgba_unmultiplied();
            let well = format!("#{r:02X}{g:02X}{b:02X}");
            // Some wells of the Scales and lines tab sit below the fold of a 900 px window.
            harness
                .get_by_role_and_label(Role::ColorWell, &well)
                .scroll_to_me();
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, &well)
                .click();
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, "#311B92")
                .click();
            harness.run();
            harness.get_by_label("Ok").click();
            harness.run();
            let (_, accepted) = harness.state_mut();
            let accepted: &mut ChartSettingsState = accepted.as_mut().expect("accepted");
            assert_eq!(
                *row(accepted),
                Color32::from_rgba_unmultiplied(49, 27, 146, alpha),
                "{name}"
            );
        }
    }

    #[test]
    fn enabled_sound_controls_offer_a_preview_without_applying_settings() {
        let ctx = Context::default();
        let mut settings = ChartSettingsState::default();
        settings.trading.play_execution_sound = true;
        settings.trading.execution_sound = "Bell".into();
        settings.trading.execution_sound_volume = 0.4;
        let mut dialog = SettingsDialog::new();
        dialog.execution_sounds = vec!["Chime".into(), "Bell".into()];
        dialog.open(settings.clone());
        dialog.active_tab = SettingsTab::Trading;
        let mut preview = None;
        let mut action = SettingsAction::None;
        for step in 0..5 {
            let events = match (step, preview) {
                (3, Some(pos)) | (4, Some(pos)) => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed: step == 3,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
                _ => vec![],
            };
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| action = dialog.show(ui.ctx(), &Theme::light()),
            );
            preview = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Test sound" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            if step >= 2 {
                assert!(
                    preview.is_some(),
                    "an enabled sound needs a local preview control"
                );
            }
        }
        assert!(matches!(
            action,
            SettingsAction::PreviewSound { name, volume } if name == "Bell" && volume == 0.4
        ));
        assert!(dialog.is_open);
        assert_eq!(dialog.settings(), &settings);
    }

    #[test]
    fn symbol_controls_follow_the_chart_type_and_retain_its_parameters() {
        use crate::{
            Chart,
            model::{BarData, ChartType, PriceSource},
        };
        for (kind, label) in [
            (ChartType::Heikin, "Body"),
            (ChartType::HollowCandles, "Body"),
            (
                ChartType::VolumeCandles,
                "Color bars based on previous close",
            ),
            (ChartType::Bars, "Up and down colors"),
            (ChartType::Line, "Line width"),
            (ChartType::Baseline, "Base level"),
            (ChartType::Renko, "Size method"),
            (ChartType::Range, "Size method"),
        ] {
            let mut chart = Chart::new(BarData::new());
            chart.set_chart_type(kind);
            let mut params = chart.chart_type_params().clone();
            params.styles.line.width = 4.5;
            params.renko_brick_size = 7.0;
            chart.set_chart_type_params(params.clone());
            chart.config_mut().price_source = PriceSource::HL2;
            let settings = ChartSettingsState::from_chart(&chart);
            let mut dialog = SettingsDialog::new();
            dialog.open(settings.clone());
            let ctx = Context::default();
            let output = ctx.run_ui(Default::default(), |ui| {
                dialog.draw_symbol_tab(ui, &Theme::light())
            });
            let has_label = output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == label)
            });
            let candle_colors = output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "CANDLES")
            });
            output.drop_without_applying_deltas();
            assert!(has_label, "{kind:?} must offer {label}");
            assert!(!candle_colors);
            let mut restored = Chart::new(BarData::new());
            restored.set_chart_type(kind);
            settings.apply_to_chart(&mut restored).unwrap();
            assert_eq!(restored.chart_type_params(), &params);
            assert_eq!(restored.config().price_source, PriceSource::HL2);
        }
    }

    #[test]
    fn offered_time_zones_and_session_presets_are_available() {
        use crate::config::SessionConfig;
        let mut zones: Vec<String> = crate::ui::timeframe_toolbar::toolbar::DEFAULT_TIME_ZONES
            .iter()
            .map(|zone| (*zone).to_owned())
            .collect();
        zones.extend(
            [
                SessionConfig::default(),
                SessionConfig::nyse(),
                SessionConfig::lse(),
                SessionConfig::tse(),
                SessionConfig::crypto_24_7(),
                SessionConfig::forex(),
            ]
            .map(|session| session.timezone),
        );
        for zone in zones {
            assert!(zone.parse::<crate::model::TimezoneId>().is_ok(), "{zone}");
        }
    }

    /// The time zone list names exchange time and local time as rows, shows the chart's mode by
    /// its row and applies the row picked, while settings keep the text saved before: #2885.
    #[test]
    fn time_zone_dropdown_offers_exchange_and_local_time() {
        use crate::{Chart, model::BarData, scales::TimezoneMode};
        let mut chart = Chart::new(BarData::new());
        let exchange = TimezoneMode::Exchange("+05:45".parse().unwrap());
        chart.chart_options.time_scale.timezone = exchange.clone();
        let saved = ChartSettingsState::from_chart(&chart);
        assert_eq!(saved.timezone, "Exchange:+05:45");
        let mut dialog = SettingsDialog::new();
        dialog.open(saved.clone());
        let SettingsAction::Apply(settings) = click_through(
            &mut dialog,
            &["scroll", "Exchange time", "Local time", "Ok"],
        ) else {
            panic!("Ok must apply the settings");
        };
        assert_eq!(settings.timezone, "Local");
        settings.apply_to_chart(&mut chart).unwrap();
        assert_eq!(
            chart.chart_options().time_scale.timezone,
            TimezoneMode::Local
        );
        saved.apply_to_chart(&mut chart).unwrap();
        assert_eq!(chart.chart_options().time_scale.timezone, exchange);
    }

    #[test]
    fn save_as_prompts_for_a_name_without_saving_automatically() {
        let ctx = Context::default();
        let mut dialog = SettingsDialog::new();
        let mut frame = |events| {
            let mut action = SettingsAction::None;
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = dialog.draw_save_as_button(ui, SettingsAction::None);
                },
            );
            let save_as = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Save as..." => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            let prompt = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Template name"));
            output.drop_without_applying_deltas();
            assert!(
                matches!(action, SettingsAction::None),
                "opening the name form must not save a template"
            );
            (save_as, prompt)
        };
        let pos = frame(vec![]).0.unwrap();
        frame(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(vec![egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            }]);
        }
        assert!(frame(vec![]).1);
    }

    #[test]
    fn grid_colour_pickers_follow_the_visible_directions() {
        use crate::widget::settings::GridLinesMode;
        for theme in [Theme::light(), Theme::dark()] {
            for (mode, horizontal, vertical) in [
                (GridLinesMode::None, false, false),
                (GridLinesMode::Horizontal, true, false),
                (GridLinesMode::Vertical, false, true),
                (GridLinesMode::Both, true, true),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut settings = ChartSettingsState::default();
                settings.grid_lines.mode = mode;
                settings.grid_lines.horizontal_color = Color32::from_rgb(151, 32, 19);
                settings.grid_lines.vertical_color = Color32::from_rgb(29, 43, 157);
                let mut dialog = SettingsDialog::new();
                dialog.open(settings.clone());
                let output = ctx.run_ui(Default::default(), |ui| {
                    dialog.draw_canvas_tab(ui, &theme);
                });
                let shown = [settings.grid_lines.horizontal_color, settings.grid_lines.vertical_color].map(|colour| output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == colour)));
                output.drop_without_applying_deltas();
                assert_eq!(shown, [horizontal, vertical], "{mode:?}");
            }
        }
    }

    #[test]
    fn settings_tracks_theme_without_changing_defaults_until_apply() {
        for (width, apply) in [
            (1440.0, false),
            (480.0, false),
            (1440.0, true),
            (480.0, true),
        ] {
            for configured in [Color32::PLACEHOLDER, Color32::from_rgb(80, 90, 100)] {
                let ctx = Context::default();
                let mut dialog = SettingsDialog::new();
                let mut settings = ChartSettingsState::default();
                settings.chart_basic_styles.background_color = configured;
                settings.margins.top_percent = 12.5;
                settings.margins.bottom_percent = 7.5;

                dialog.open(settings.clone());
                assert_eq!(dialog.margin_top_str, "12.5");
                assert_eq!(dialog.margin_bottom_str, "7.5");
                let mut frame = |theme: &Theme, events| {
                    crate::theme::apply_to_egui(&ctx, theme);
                    let mut action = SettingsAction::None;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = dialog.show(ui.ctx(), theme);
                        },
                    );
                    let symbol = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Body"));
                    let ok = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Ok" => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    assert_eq!(dialog.settings, settings);
                    assert_eq!(
                        dialog.color_picker_states.background.sel_color,
                        if configured == Color32::PLACEHOLDER {
                            theme.chart.bg
                        } else {
                            configured
                        }
                    );
                    assert_eq!(
                        dialog.color_picker_states.background_gradient_top.sel_color,
                        theme.chart.bg
                    );
                    assert_eq!(
                        dialog
                            .color_picker_states
                            .background_gradient_bottom
                            .sel_color,
                        theme.chart.bg_axis
                    );
                    (action, symbol, ok)
                };
                for theme in [Theme::light(), Theme::dark(), Theme::light()] {
                    frame(&theme, vec![]);
                    assert!(
                        frame(&theme, vec![]).1,
                        "settings starts with the symbol controls"
                    );
                }
                let action = if apply {
                    let pos = frame(&Theme::light(), vec![]).2.unwrap();
                    let button = |pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(&Theme::light(), vec![egui::Event::PointerMoved(pos)]);
                    frame(&Theme::light(), vec![button(true)]);
                    frame(&Theme::light(), vec![button(false)]).0
                } else {
                    frame(
                        &Theme::light(),
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        }],
                    )
                    .0
                };
                if apply {
                    let SettingsAction::Apply(saved) = action else {
                        panic!("expected Apply");
                    };
                    let expected = if configured == Color32::PLACEHOLDER {
                        Theme::light().chart.bg
                    } else {
                        configured
                    };
                    assert_eq!(saved.chart_basic_styles.background_color, expected);
                    assert_eq!(
                        saved.chart_basic_styles.background_gradient_top,
                        Theme::light().chart.bg
                    );
                    assert_eq!(
                        saved.chart_basic_styles.background_gradient_bottom,
                        Theme::light().chart.bg_axis
                    );
                } else {
                    assert!(matches!(action, SettingsAction::Cancel));
                }
                assert!(!dialog.is_open);
                assert_eq!(dialog.settings, settings);
            }
        }
    }
    #[test]
    fn scale_controls_show_small_ratios_and_keep_modes_exclusive() {
        let ctx = Context::default();
        let mut dialog = SettingsDialog::new();
        let theme = Theme::light();
        for (label, expected, ratio) in [
            ("Log scale", (true, false), 0.005),
            ("Percentage scale", (false, true), 0.00000001),
            ("Log scale", (true, false), 1.0),
        ] {
            dialog.settings.scales_and_lines.price_to_bar_ratio = ratio;
            dialog.settings.scales_and_lines.lock_price_to_bar_ratio = true;
            let mut frame =
                |events| {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            dialog.draw_scales_and_lines_tab(ui, &theme);
                        },
                    );
                    let ratio_shown = output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text().parse::<f64>() == Ok(ratio)
                    ));
                    let pos = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        })
                        .unwrap();
                    output.drop_without_applying_deltas();
                    assert!(ratio_shown, "entered ratio {ratio} must remain legible");
                    pos
                };
            let pos = frame(vec![]);
            frame(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(vec![egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                }]);
            }
            assert_eq!(
                (
                    dialog.settings.scales_and_lines.log_scale,
                    dialog.settings.scales_and_lines.percentage_scale
                ),
                expected
            );
        }
    }

    #[test]
    fn order_flow_controls_follow_declared_services_and_tick_size() {
        use crate::data::FeedCapabilities;
        use crate::model::{FeedService::*, Instrument};
        let feed = FeedCapabilities::new().with_services([
            Trades,
            LiveTrades,
            Depth,
            DepthHistory,
            OrderDepth,
            PriceTargets,
        ]);
        for (services, tick, expected, targets) in [
            (None, Some(0.25), [true, true, true, true], true),
            (
                Some(vec![LiveTrades]),
                Some(0.25),
                [false, false, false, true],
                false,
            ),
            (
                Some(vec![Trades]),
                Some(0.25),
                [false, false, false, true],
                false,
            ),
            (
                Some(vec![Depth]),
                Some(0.25),
                [true, true, false, false],
                false,
            ),
            (
                Some(vec![DepthHistory]),
                Some(0.25),
                [true, false, false, false],
                false,
            ),
            (
                Some(vec![Depth, OrderDepth]),
                Some(0.25),
                [true, true, true, false],
                false,
            ),
            (Some(vec![OrderDepth]), Some(0.25), [false; 4], false),
            (Some(vec![]), Some(0.25), [false; 4], false),
            (Some(vec![Depth]), None, [false; 4], false),
            (Some(vec![Depth]), Some(0.0), [false; 4], false),
            (Some(vec![Depth]), Some(f64::NAN), [false; 4], false),
            (Some(vec![PriceTargets]), Some(0.25), [false; 4], true),
            (Some(vec![PriceTargets]), None, [false; 4], true),
        ] {
            let ctx = Context::default();
            let theme = Theme::light();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut dialog = SettingsDialog::new();
            dialog.open(ChartSettingsState {
                show_price_targets: true,
                ..Default::default()
            });
            let mut instrument = Instrument::new("A");
            instrument.services = services;
            instrument.tick_size = tick;
            dialog.set_feed_capabilities(Some(&feed), Some(&instrument));
            assert_eq!(dialog.order_flow_layers, expected);
            for pass in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        dialog.show(ui.ctx(), &theme);
                    },
                );
                let shown = |label: &str| {
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == label)
                    })
                };
                let (tab, offered) = (shown("Order flow"), shown("Analyst price targets"));
                output.drop_without_applying_deltas();
                if pass == 2 {
                    assert_eq!(tab, expected.iter().any(|available| *available));
                    assert_eq!(offered, targets, "price targets");
                }
            }
            assert!(
                dialog.settings.show_price_targets,
                "a hidden setting keeps its value"
            );
            // Existing overlay configs define fallback behaviour for these values.
            dialog.settings.order_flow.heatmap.full_intensity_size = Some(0.0);
            dialog.settings.order_flow.volume_bubbles.grouping =
                crate::chart::overlays::BubbleGrouping::Executions(0);
            let before = dialog.settings.clone();
            let output = ctx.run_ui(Default::default(), |ui| {
                dialog.draw_order_flow_tab(ui, &theme);
            });
            for (label, shown) in [
                ("Market depth heatmap", expected[0]),
                ("Order book", expected[1]),
                ("Volume bubbles", expected[3]),
            ] {
                assert_eq!(
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == label)),
                    shown,
                    "{label}"
                );
            }
            output.drop_without_applying_deltas();
            assert_eq!(
                dialog.settings, before,
                "viewing controls must preserve host preferences"
            );
            dialog.active_tab = SettingsTab::OrderFlow;
            dialog.set_feed_capabilities(Some(&feed), None);
            assert_eq!(dialog.active_tab, SettingsTab::Symbol);
            assert!(!dialog.price_targets_offered);
        }
        let mut dialog = SettingsDialog::new();
        assert_eq!(dialog.order_flow_layers, [true; 4]);
        assert!(dialog.price_targets_offered);
        let instrument = Instrument::new("A")
            .with_tick_size(0.25)
            .with_services([Depth]);
        dialog.set_feed_capabilities(None, Some(&instrument));
        assert_eq!(dialog.order_flow_layers, [true, true, false, false]);
        assert!(!dialog.price_targets_offered);
        dialog.set_feed_capabilities(
            Some(&FeedCapabilities::new().with_services([])),
            Some(&instrument),
        );
        assert_eq!(dialog.order_flow_layers, [false; 4]);
        dialog.set_feed_capabilities(None, Some(&Instrument::new("A").with_tick_size(0.25)));
        assert_eq!(dialog.order_flow_layers, [false; 4]);
        dialog.set_feed_capabilities(
            None,
            Some(&Instrument::new("A").with_services([PriceTargets])),
        );
        assert!(dialog.price_targets_offered);
    }

    #[test]
    fn order_flow_heatmap_switch_applies_without_enabling_other_layers() {
        let ctx = Context::default();
        let theme = Theme::light();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut dialog = SettingsDialog::new();
        let mut settings = ChartSettingsState::default();
        settings.order_flow.heatmap.lower_cutoff = 0.00000001;
        settings.order_flow.heatmap.full_intensity_size = Some(0.00000002);
        dialog.open(settings);
        dialog.active_tab = SettingsTab::OrderFlow;
        let mut frame = |events| {
            let mut action = SettingsAction::None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = dialog.show(ui.ctx(), &theme);
                },
            );
            let positions = ["Market depth heatmap", "Ok"].map(|label| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
            });
            let sizes_visible = [0.00000001, 0.00000002].map(|value| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text().parse::<f64>() == Ok(value))
                })
            });
            output.drop_without_applying_deltas();
            (action, positions, sizes_visible)
        };
        frame(vec![]);
        frame(vec![]);
        for index in [0, 1] {
            let pos = frame(vec![]).1[index].unwrap();
            frame(vec![egui::Event::PointerMoved(pos)]);
            frame(vec![egui::Event::PointerButton {
                pos,
                pressed: true,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            }]);
            let (action, _, _) = frame(vec![egui::Event::PointerButton {
                pos,
                pressed: false,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            }]);
            if index == 0 {
                assert_eq!(
                    frame(vec![]).2,
                    [true; 2],
                    "small sizes must remain legible"
                );
            }
            if index == 1 {
                let SettingsAction::Apply(saved) = action else {
                    panic!("expected Apply")
                };
                assert!(saved.order_flow.show_heatmap);
                assert!(!saved.order_flow.show_volume_bubbles);
                assert!(!saved.order_flow.show_order_book);
                dialog.open(*saved);
                assert!(dialog.settings().order_flow.show_heatmap);
                assert!(!dialog.settings().order_flow.show_volume_bubbles);
                break;
            }
        }
    }

    #[test]
    fn changing_settings_tabs_keeps_the_top_edge_and_fits_the_window() {
        let mut scaled = Theme::light();
        scaled.text.small = 17.0;
        scaled.spacing.lg = 12.0;
        scaled.spacing.xxxl = 32.0;
        for theme in [Theme::light(), Theme::dark(), scaled] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut dialog = SettingsDialog::new();
            dialog.open(ChartSettingsState::default());
            dialog.template_name = "Long template name ".repeat(20);
            for size in [
                Vec2::new(1440.0, 900.0),
                Vec2::new(600.0, 400.0),
                Vec2::new(320.0, 450.0),
                Vec2::new(1440.0, 900.0),
            ] {
                let mut tops = Vec::new();
                for tab in SettingsTab::all().iter().chain(&[
                    SettingsTab::Events,
                    SettingsTab::Symbol,
                    SettingsTab::Events,
                ]) {
                    dialog.active_tab = *tab;
                    for pass in 0..3 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                dialog.show(ui.ctx(), &theme);
                            },
                        );
                        let sidebar = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Rect(row)
                                if row.rect.height() == 40.0
                                    && row.rect.width() <= SIDEBAR_WIDTH =>
                            {
                                Some(row.rect)
                            }
                            _ => None,
                        });
                        let labels_fit = sidebar.is_none_or(|row| {
                            output.shapes.iter().all(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if SettingsTab::all()
                                        .iter()
                                        .any(|tab| tab.name() == text.galley.text()) =>
                                {
                                    text.pos.x + text.galley.size().x <= row.right() + 0.5
                                }
                                _ => true,
                            })
                        });
                        let heading = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "CANDLES" => {
                                Some(text.galley.job.sections[0].format.font_id.size)
                            }
                            _ => None,
                        });
                        output.drop_without_applying_deltas();
                        if pass == 2 {
                            assert!(
                                labels_fit,
                                "tab labels must stay in the sidebar at {size:?}"
                            );
                        }
                        // The target colour rows can put the candle heading below the fold.
                        if *tab == SettingsTab::Symbol
                            && pass == 2
                            && (size.y >= TALLEST_TAB_HEIGHT || heading.is_some())
                        {
                            assert_eq!(heading, Some(theme.text.small));
                        }
                    }
                    let rect = ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("chart_settings")))
                        .unwrap();
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                        "{tab:?}: {rect:?} in {size:?}"
                    );
                    tops.push(rect.top());
                }
                assert!(
                    tops.iter().all(|top| (top - tops[0]).abs() < 1.0),
                    "{tops:?}"
                );
            }
        }
    }

    /// A template saved before volume candles had a style, with or without chart type
    /// parameters, holds the candle settings of the candles chart type, of which volume candles
    /// drew only the body colours. Loaded for a volume candles chart it shows them as drawn, the
    /// body colours on the body and both wicks without a border, and an edit made before Ok
    /// applies.
    #[test]
    fn old_templates_load_volume_candles_as_drawn_and_keep_edits() {
        use crate::model::{Bar, BarData, ChartType};
        use crate::widget::settings::CandleColorConfig;
        let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
        let (border, wick) = (Color32::from_rgb(3, 4, 102), Color32::from_rgb(105, 106, 6));
        for (parent, entry) in [
            ("/chart_type_params/styles", "volume_candles"),
            ("", "chart_type_params"),
        ] {
            let mut chart = crate::Chart::new(BarData::from_bars(vec![Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                10.0,
                18.0,
                8.0,
                14.0,
                100.0,
            )]));
            chart.set_chart_type(ChartType::VolumeCandles);
            let mut saved = serde_json::to_value(ChartSettingsState::from_chart(&chart)).unwrap();
            let object = saved.pointer_mut(parent).unwrap().as_object_mut().unwrap();
            object.remove(entry).unwrap();
            saved["candle_colors"] = serde_json::to_value(CandleColorConfig {
                color_by_previous_close: true,
                show_body: true,
                show_border: true,
                show_wick: false,
                body_up: up,
                body_down: down,
                border_up: border,
                border_down: border,
                wick_up: wick,
                wick_down: wick,
            })
            .unwrap();
            let mut dialog = SettingsDialog::new();
            dialog
                .templates
                .save("Old", serde_json::from_value(saved).unwrap())
                .unwrap();
            dialog.open(ChartSettingsState::from_chart(&chart));
            load_template(&mut dialog, "Old");
            assert_eq!(
                dialog.settings().candle_colors,
                CandleColorConfig {
                    color_by_previous_close: true,
                    show_body: true,
                    show_border: false,
                    show_wick: true,
                    body_up: up,
                    body_down: down,
                    border_up: up,
                    border_down: down,
                    wick_up: up,
                    wick_down: down,
                },
                "{entry}"
            );
            dialog.settings.candle_colors.show_border = true;
            dialog.settings().apply_to_chart(&mut chart).unwrap();
            let style = &chart.chart_type_params().styles.volume_candles;
            assert!(style.border_visible, "{entry}");
            assert_eq!([style.border_up, style.wick_down], [up, down], "{entry}");
        }
    }

    /// Clicks the texts `labels` name one after the other in a 1440 by 900 window, as opening a
    /// dropdown, picking an entry and pressing Ok do, and returns the action of the last click.
    /// The label `scroll` scrolls the tab to its end instead, for the rows below its fold.
    fn click_through(dialog: &mut SettingsDialog, labels: &[&str]) -> SettingsAction {
        fn center(shape: &egui::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| center(shape, label)),
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            }
        }
        let ctx = Context::default();
        let theme = Theme::light();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut frame = |events, label: &str| {
            let mut action = SettingsAction::None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| action = dialog.show(ui.ctx(), &theme),
            );
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| center(&shape.shape, label));
            output.drop_without_applying_deltas();
            (action, pos)
        };
        let mut action = SettingsAction::None;
        for label in labels {
            if *label == "scroll" {
                let pos = Pos2::new(900.0, 450.0);
                frame(vec![egui::Event::PointerMoved(pos)], label);
                frame(
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        source: egui::MouseWheelSource::Wheel,
                        phase: egui::TouchPhase::Move,
                        delta: Vec2::new(0.0, -2000.0),
                        modifiers: Default::default(),
                    }],
                    label,
                );
                for _ in 0..30 {
                    frame(vec![], label);
                }
                continue;
            }
            frame(vec![], label);
            let pos = frame(vec![], label).1.expect(label);
            frame(vec![egui::Event::PointerMoved(pos)], label);
            for pressed in [true, false] {
                action = frame(
                    vec![egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    }],
                    label,
                )
                .0;
            }
        }
        action
    }

    /// Clicks the entry of the template `name` in the template menu, which loads it.
    fn load_template(dialog: &mut SettingsDialog, name: &str) {
        let ctx = Context::default();
        let mut item = None;
        for step in 0..4 {
            let events = match (step, item) {
                (2, Some(pos)) | (3, Some(pos)) => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed: step == 2,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
                _ => vec![],
            };
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    dialog.draw_template_item(
                        ui,
                        &Theme::light(),
                        name.into(),
                        SettingsAction::None,
                    );
                },
            );
            item = item.or(output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == name => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            }));
            output.drop_without_applying_deltas();
        }
        assert_eq!(dialog.template_name, name);
    }

    /// Choosing a family next to the font size draws the labels of the price, study and time
    /// axes in it.
    #[test]
    fn axis_font_family_is_chosen_in_the_dialog() {
        use crate::widget::test_frames::{chart, frame, shapes};
        use egui::FontFamily;
        let ctx = Context::default();
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
        frame(&ctx, &mut chart, 0.0, Vec::new());
        let mut dialog = SettingsDialog::new();
        dialog.open(ChartSettingsState::from_chart(&chart));
        dialog.active_tab = SettingsTab::Canvas;
        let action = click_through(&mut dialog, &["Proportional", "Monospace", "Ok"]);
        let SettingsAction::Apply(settings) = action else {
            panic!("Ok must apply the settings");
        };
        settings.apply_to_chart(&mut chart).unwrap();
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let painted = shapes(&ctx, &mut chart, 0.2, Vec::new());
        let plot = chart.get_rendered_price_rect();
        let panes = chart.get_rendered_indicator_panes();
        let pane = panes[0].chart_rect;
        // The time axis lies below every pane.
        let panes_bottom = panes.iter().fold(plot.bottom(), |bottom, pane| {
            bottom.max(pane.panel_rect.bottom())
        });
        let mut families = [Vec::new(), Vec::new(), Vec::new()];
        for shape in &painted {
            let egui::Shape::Text(text) = shape else {
                continue;
            };
            let axis = if text.pos.x >= plot.right() && plot.y_range().contains(text.pos.y) {
                0
            } else if text.pos.x >= pane.right() && pane.y_range().contains(text.pos.y) {
                1
            } else if text.pos.y >= panes_bottom && text.pos.x < plot.right() {
                2
            } else {
                continue;
            };
            let sections = &text.galley.job.sections;
            families[axis].extend(
                sections
                    .iter()
                    .map(|section| &section.format.font_id.family),
            );
        }
        for (axis, families) in ["price", "study", "time"].iter().zip(&families) {
            assert!(
                !families.is_empty() && families.iter().all(|f| **f == FontFamily::Monospace),
                "{axis} axis: {families:?}"
            );
        }
    }

    /// Each Canvas switch reaches the painted stroke or axis label and reads back, including
    /// enabling one label while the chart's master label switch is off.
    #[test]
    fn crosshair_parts_are_chosen_in_the_dialog() {
        use crate::widget::settings::CrosshairVisibility;
        use crate::widget::test_frames::{chart, frame, shapes};
        use egui::{Event, Shape};
        for (master, label, expected) in [
            (true, "Vertical line", (false, true, true, true)),
            (true, "Horizontal line", (true, false, true, true)),
            (true, "Time label", (true, true, false, true)),
            (true, "Price label", (true, true, true, false)),
            (false, "Time label", (true, true, true, false)),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            let mut options = chart.chart_options().clone();
            options.crosshair.label_visible = master;
            options.crosshair.vert_label_background_color = Color32::DARK_RED;
            options.crosshair.horz_label_background_color = Color32::DARK_GREEN;
            chart.set_chart_options(options);
            let mut settings = ChartSettingsState::from_chart(&chart);
            let color = Color32::from_rgb(1, 2, 3);
            settings.crosshair.color = color;
            settings.crosshair.line_style = crate::model::LineStyle::Solid;
            let mut dialog = SettingsDialog::new();
            dialog.open(settings);
            dialog.active_tab = SettingsTab::Canvas;
            let SettingsAction::Apply(settings) = click_through(&mut dialog, &[label, "Ok"]) else {
                panic!("Ok must apply the settings");
            };
            let settings: ChartSettingsState =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
            frame(&ctx, &mut chart, 0.0, vec![]);
            let pointer = chart.get_rendered_price_rect().center();
            let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pointer)]);
            let line = |vertical| {
                painted.iter().any(|shape| {
                    matches!(shape,
                Shape::LineSegment { points, stroke } if stroke.color == color &&
                    if vertical { points[0].x == points[1].x } else { points[0].y == points[1].y })
                })
            };
            let fill = |color| {
                painted
                    .iter()
                    .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == color))
            };
            assert_eq!(
                (
                    line(true),
                    line(false),
                    fill(Color32::DARK_RED),
                    fill(Color32::DARK_GREEN)
                ),
                expected,
                "{master} {label}"
            );
            assert_eq!(
                ChartSettingsState::from_chart(&chart).crosshair_lines,
                Some(CrosshairVisibility {
                    vertical_line: expected.0,
                    horizontal_line: expected.1,
                    time_label: expected.2,
                    price_label: expected.3,
                })
            );
        }
    }

    /// Retention edits apply only with Ok; releasing memory is immediate and keeps editing open,
    /// including while the heatmap is hidden. Cancel returns its action.
    #[test]
    fn order_flow_retention_and_release_memory() {
        use chrono::TimeDelta;
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        let mut dialog = SettingsDialog::new();
        let config = crate::data::DepthHistoryConfig {
            columns: 1_000,
            ..Default::default()
        };
        dialog.set_depth_history_config(Some(&config));
        dialog.open(ChartSettingsState::default());
        dialog.active_tab = SettingsTab::OrderFlow;
        let theme = Theme::light();
        let mut harness = Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &Theme::light());
            })
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                move |ui, (dialog, actions): &mut (SettingsDialog, Vec<SettingsAction>)| {
                    let action = dialog.show(ui.ctx(), &theme);
                    if !matches!(action, SettingsAction::None) {
                        actions.push(action);
                    }
                },
                (dialog, Vec::new()),
            );
        harness.run();
        // An unchanged choice keeps the host's 1,000 columns, rather than restoring 600.
        harness.get_by_label(if usize::BITS == 64 {
            "Up to 26.8 MB"
        } else {
            "Up to 26.3 MB"
        });
        harness
            .get_by_role_and_label(Role::ComboBox, "History retention")
            .click();
        harness.run();
        harness.get_by_label("1 hour").click();
        harness.run();
        harness.get_by_label("Up to 0.4 MB");
        harness
            .get_by_role_and_label(Role::Button, "Release memory")
            .click();
        harness.run();
        assert!(matches!(
            &harness.state().1[..],
            [SettingsAction::ClearDepthHistory]
        ));
        assert!(harness.state().0.is_open);
        harness.get_by_label("Ok").click();
        harness.run();
        let SettingsAction::Apply(settings) = harness.state().1.last().unwrap() else {
            panic!("Ok applies the settings");
        };
        assert_eq!(
            settings.order_flow.heatmap_retention,
            Some(TimeDelta::hours(1))
        );
        let accepted = settings.clone();
        let (dialog, actions) = harness.state_mut();
        dialog.open(*accepted);
        dialog.active_tab = SettingsTab::OrderFlow;
        actions.clear();
        harness.run();
        harness
            .get_by_role_and_label(Role::ComboBox, "History retention")
            .click();
        harness.run();
        harness.get_by_label("4 hours").click();
        harness.run();
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(matches!(&harness.state().1[..], [SettingsAction::Cancel]));
    }

    /// Choosing a pane separator colour on the Canvas tab paints the separator above the study
    /// pane at the opacity its well showed, in one undoable step.
    #[test]
    fn pane_separator_colour_is_chosen_in_the_dialog() {
        use crate::widget::test_frames::{chart, frame, shapes};
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = Context::default();
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
        // Half opaque, and the only well of the tab labelled #FF00FF.
        let had = Color32::from_rgba_unmultiplied(255, 0, 255, 128);
        chart.pane_options_mut().separator_color = Some(had);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        let mut dialog = SettingsDialog::new();
        dialog.open(ChartSettingsState::from_chart(&chart));
        dialog.active_tab = SettingsTab::Canvas;
        let theme = Theme::light();
        let mut harness = Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &Theme::light());
            })
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                move |ui, (dialog, accepted): &mut (SettingsDialog, _)| {
                    if let SettingsAction::Apply(settings) = dialog.show(ui.ctx(), &theme) {
                        *accepted = Some(settings);
                    }
                },
                (dialog, None),
            );
        harness.run();
        for well in ["#FF00FF", "#311B92"] {
            harness.get_by_role_and_label(Role::ColorWell, well).click();
            harness.run();
        }
        harness.get_by_label("Ok").click();
        harness.run();
        let settings = harness
            .state_mut()
            .1
            .take()
            .expect("Ok applies the settings");
        // Saved in a template or layout and loaded again.
        let settings: ChartSettingsState =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let painted = shapes(&ctx, &mut chart, 0.2, Vec::new());
        // The palette swatch rgb(49, 27, 146) retains the well's alpha of 128.
        let chosen = Color32::from_rgba_unmultiplied(49, 27, 146, 128);
        let top = chart.get_rendered_indicator_panes()[0].panel_rect.top();
        assert!(
            painted
                .iter()
                .any(|shape| matches!(shape, egui::Shape::Rect(rect)
                if rect.fill == chosen && (rect.rect.bottom() - top).abs() < 1.0)),
            "no separator in {chosen:?} at {top}"
        );
        assert!(chart.undo());
        assert_eq!(chart.pane_options().separator_color, Some(had));
        assert!(!chart.undo(), "only the separator changed");
        assert!(chart.redo());
        assert_eq!(chart.pane_options().separator_color, Some(chosen));
    }

    /// Background edits wait for acceptance, retain opacity in templates and reach every chart
    /// through the settings copy used by Apply to all. Cancel reports dismissal of the edits.
    #[test]
    fn session_background_edits_accept_cancel_save_and_copy() {
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        for accept in [false, true] {
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            chart.set_chart_type(crate::model::ChartType::Line);
            chart.config_mut().premarket_color =
                Some(Color32::from_rgba_unmultiplied(255, 0, 255, 128));
            let mut dialog = SettingsDialog::new();
            dialog.offer_apply_to_all(true);
            dialog.set_control_visible(Control::Templates, false);
            dialog.set_feed_capabilities(None, None);
            dialog.open(ChartSettingsState::from_chart(&chart));
            let mut harness = Harness::builder()
                .with_size(Vec2::new(1440.0, 900.0))
                .allow_missing_glyphs()
                .with_context_setup(|ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &Theme::light());
                })
                .build_ui_state(
                    move |ui, (dialog, action): &mut (SettingsDialog, _)| {
                        let next = dialog.show(ui.ctx(), &Theme::light());
                        if !matches!(next, SettingsAction::None) {
                            *action = next;
                        }
                    },
                    (dialog, SettingsAction::None),
                );
            harness.run();
            harness.hover_at(Pos2::new(900.0, 450.0));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Wheel,
                phase: egui::TouchPhase::Move,
                delta: Vec2::new(0.0, -250.0),
                modifiers: Default::default(),
            });
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, "#FF00FF")
                .click();
            harness.run();
            harness
                .get_by_role_and_label(Role::ColorWell, "#311B92")
                .click();
            harness.run();
            harness.get_by_label("Post-market").click();
            harness.run();
            harness.get_by_label("Highlight extended sessions").click();
            harness.run();
            if !accept {
                harness.get_by_label("Cancel").click();
                harness.run();
                assert!(matches!(harness.state().1, SettingsAction::Cancel));
                continue;
            }
            harness.get_by_label("Apply to all").click();
            harness.run();
            let SettingsAction::ApplyToAll(settings) = &harness.state().1 else {
                panic!("Apply to all must carry the edited settings");
            };
            let saved: ChartSettingsState =
                serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();
            let chosen = saved.session_background.as_ref().unwrap();
            assert!(!chosen.show_extended_hours);
            assert!(!chosen.shade_postmarket);
            assert_eq!(
                chosen.premarket_color,
                Some(Color32::from_rgba_unmultiplied(49, 27, 146, 128))
            );
            let mut second = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            // Every recipient receives this same settings value; layout copy calls apply_to_chart.
            for target in [&mut chart, &mut second] {
                saved.apply_to_chart(target).unwrap();
            }
            assert_eq!(
                ChartSettingsState::from_chart(&chart).session_background,
                Some(chosen.clone())
            );
            assert_eq!(
                ChartSettingsState::from_chart(&second).session_background,
                Some(chosen.clone())
            );
            harness.state_mut().0.open(saved.clone());
            harness.run();
            harness.get_by_label("Ok").click();
            harness.run();
            let SettingsAction::Apply(settings) = &harness.state().1 else {
                panic!("Ok must accept");
            };
            assert_eq!(settings.session_background, saved.session_background);
        }
    }

    /// Choosing a thousands separator on the Scales and lines tab groups the digits of the last
    /// price tag with it, or leaves them plain.
    #[test]
    fn thousands_separator_is_chosen_in_the_dialog() {
        use crate::widget::test_frames::{chart, frame, shapes};
        let ctx = Context::default();
        let mut chart = chart(&[42_500.0], &[42_541.77]);
        chart.config_mut().show_volume = false;
        chart.set_instrument(Some(
            crate::model::Instrument::new("SAMPLE").with_tick_size(0.01),
        ));
        for (step, (labels, last)) in [
            (["Comma", "Space", "Ok"], "42 541.77"),
            (["Space", "None", "Ok"], "42541.77"),
        ]
        .into_iter()
        .enumerate()
        {
            let time = step as f64;
            frame(&ctx, &mut chart, time, Vec::new());
            let mut dialog = SettingsDialog::new();
            dialog.open(ChartSettingsState::from_chart(&chart));
            dialog.active_tab = SettingsTab::ScalesAndLines;
            let SettingsAction::Apply(settings) = click_through(&mut dialog, &labels) else {
                panic!("Ok must apply the settings");
            };
            settings.apply_to_chart(&mut chart).unwrap();
            frame(&ctx, &mut chart, time + 0.1, Vec::new());
            let painted = shapes(&ctx, &mut chart, time + 0.2, Vec::new());
            let plot = chart.get_rendered_price_rect();
            let axis: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) if text.pos.x > plot.right() => {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            assert!(axis.contains(&last), "{last}: {axis:?}");
        }
    }

    /// High and low can show only their tags or only their lines from the Scales and lines tab.
    #[test]
    fn high_and_low_value_or_line_is_chosen_in_the_dialog() {
        use crate::widget::test_frames::{chart, frame, shapes};
        for (choice, tags, lines) in [("Value", true, false), ("Line", false, true)] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[110.0, 120.0]);
            let high = Color32::from_rgb(201, 11, 11);
            let low = Color32::from_rgb(11, 201, 11);
            let config = chart.config_mut();
            config.show_volume = false;
            config.show_quote_boxes = false;
            config.high_line.color = Some(high);
            config.low_line.color = Some(low);
            config.high_line.style = crate::model::LineStyle::Solid;
            config.low_line.style = crate::model::LineStyle::Solid;
            // Keep the other dropdowns distinct from this pair's Hidden and Value choices.
            config.show_symbol_prev_close = true;
            config.show_prev_close_line = true;
            config.show_bid_ask = true;
            config.show_symbol_last_change = true;
            let mut dialog = SettingsDialog::new();
            dialog.open(ChartSettingsState::from_chart(&chart));
            dialog.active_tab = SettingsTab::ScalesAndLines;
            let SettingsAction::Apply(settings) =
                click_through(&mut dialog, &["scroll", "Hidden", choice, "Ok"])
            else {
                panic!("Ok must apply the settings");
            };
            settings.apply_to_chart(&mut chart).unwrap();
            assert_eq!(
                (
                    chart.config().show_high_low,
                    chart.config().show_high_low_lines
                ),
                (tags, Some(lines)),
                "{choice}"
            );
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
            let plot = chart.get_rendered_price_rect();
            for (name, price, color) in [("High ", 120.0, high), ("Low ", 100.0, low)] {
                let tag = painted.iter().any(|shape| {
                    matches!(shape,
                    egui::Shape::Text(text) if text.pos.x > plot.right()
                        && text.galley.text().starts_with(name))
                });
                // The fixture's opens are 100 and its closes alternate 110 and 120.
                let y = chart.last_price_map.price_to_y(price);
                let line = painted.iter().any(|shape| {
                    matches!(shape,
                    egui::Shape::LineSegment { points, stroke } if stroke.color == color
                        && points.iter().all(|point| (point.y - y).abs() < 0.01))
                });
                assert_eq!((tag, line), (tags, lines), "{choice}: {name}");
            }
        }
    }

    /// Time scale controls display a host pattern by its sample, and choose the date, clock and
    /// weekday names. Invalid host patterns stay listed without a formatting panic.
    #[test]
    fn time_label_choices_are_made_in_the_dialog() {
        use crate::scales::WeekdayNames;
        use crate::widget::settings::DateFormat;
        for (pattern, sample) in [("%A %d %B %Y", "Wednesday 13 March 2024"), ("%Q", "%Q")] {
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            chart.set_date_format(Some(pattern.into()));
            let mut dialog = SettingsDialog::new();
            dialog.open(ChartSettingsState::from_chart(&chart));
            dialog.active_tab = SettingsTab::ScalesAndLines;
            let labels = [
                "scroll",
                sample,
                "13.03.2024",
                "24-hour",
                "12-hour",
                "Default",
                "Always",
                "Ok",
            ];
            let SettingsAction::Apply(settings) = click_through(&mut dialog, &labels) else {
                panic!("Ok must apply the settings");
            };
            let scales = &settings.scales_and_lines;
            assert_eq!(
                (
                    scales.date_format.clone(),
                    scales.use_24_hour,
                    scales.weekday_names
                ),
                (
                    Some(DateFormat::Pattern("%d.%m.%Y".into())),
                    Some(false),
                    Some(WeekdayNames::Always)
                )
            );
        }
    }

    /// Templates saved before the fractional margin, font family, thousands separator, session
    /// breaks, analyst target colours, last price bar and time label choices keep the chart's
    /// choices. Their whole-bar margin keeps the chart's fractional margin when it is the same
    /// margin unedited, whether the chart's margin is in bars or pixels.
    /// Their pane separators follow the theme, as they did when such a template was saved.
    #[test]
    fn old_templates_keep_the_right_margin_and_the_choices_they_lack() {
        use crate::model::PriceLineSource;
        use crate::scales::WeekdayNames;
        use crate::widget::settings::DateFormat;
        use crate::widget::{SeriesId, settings::ThousandsSeparator, test_frames::chart};
        use egui::FontFamily;
        // Serialized on origin/main d61351c84 with analyst targets enabled, before colour fields.
        let older = r#"{"chart_type":"Candles","chart_type_params":null,"price_source":null,"candle_colors":{"color_by_previous_close":false,"show_body":true,"show_border":false,"show_wick":true,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"precision":"Default","timezone":"UTC","show_price_targets":true,"adjust_for_dividends":false,"order_flow":{"show_heatmap":false,"heatmap":{"brightness":1.0,"lower_cutoff":0.0,"full_intensity_size":null},"show_order_book":false,"order_book":{"filter":{"min_size":0.0,"min_age":null,"max_age":null,"exclude_participants":[]},"profile":"Total","quantity_unit":null,"tiers":null,"tier_colors":null},"show_volume_bubbles":false,"volume_bubbles":{"min_diameter":4.0,"max_diameter":40.0,"opacity":128,"buy_color":[8,153,129,255],"sell_color":[242,54,69,255],"neutral_color":[150,150,150,255],"color_by":"Side","scaling_mode":"Area","clamp_std_devs":null,"show_border":false,"border_color":[100,100,100,100],"min_size_filter":null,"grouping":"Ungrouped","shaded":true}},"status_line":{"show_logo":true,"show_symbol":true,"title_as_description":true,"show_market_status":true,"show_quote_boxes":true,"show_indicator_titles":true,"show_indicator_inputs":true,"show_indicator_values":true,"show_ohlc":true,"show_change":true,"show_change_percent":true,"show_volume":true,"show_last_day_change":false,"indicator_background_opacity":60},"scales_and_lines":{"axis_layout":{"side":null,"hide_unused":true},"show_currency":true,"show_unit":true,"show_scale_mode_buttons":true,"show_plus_button":true,"allow_label_overlap":false,"lock_price_to_bar_ratio":false,"price_to_bar_ratio":1.0,"symbol_price_color":null,"show_study_values":true,"study_value_color":null,"show_symbol_name":false,"show_study_names":false,"show_extended_session_price":true,"extended_session_price_color":[251,142,1,255],"auto_scale":true,"log_scale":false,"percentage_scale":false,"invert_scale":false,"price_scale_left":false,"show_last_price_label":true,"show_prev_close_label":false,"show_price_line":true,"price_line_source":null,"thousands_sep":null,"show_countdown":true,"show_bid_ask":false,"show_high_low":false,"show_prev_close":false},"chart_basic_styles":{"background_type":"Solid","background_color":[64,254,0,128],"background_gradient_top":[64,254,0,128],"background_gradient_bottom":[64,254,0,128]},"grid_lines":{"mode":"Both","horizontal_color":[34,34,34,255],"vertical_color":[34,34,34,255]},"crosshair":{"color":[184,184,184,255],"line_width":1.0,"line_style":"Dashed"},"watermark":{"mode":"ReplayMode","color":[33,33,33,51]},"scales_appearance":{"text_color":[184,184,184,255],"font_size":12,"lines_color":[0,0,0,0]},"btns":{"navigation":"Hidden","pane":"VisibleOnMouseOver"},"margins":{"top_percent":10.0,"bottom_percent":10.0,"right_bars":2.5},"trading":{"instant_orders":false,"bid_ask_labels":false,"play_execution_sound":false,"execution_sound":"","execution_sound_volume":1.0,"notifications":"All","show_positions":true,"position_pnl":true,"position_pnl_unit":"Money","reverse_button":true,"extend_lines":true,"label_alignment":"Right","include_in_image":false,"show_orders":true,"bracket_pnl":true,"bracket_pnl_unit":"Money","show_executions":true,"show_execution_labels":true},"alerts":{"play_sound":false,"sound":"","sound_volume":1.0,"show_alerts":true,"show_alert_labels":true,"alert_color":[255,152,0,255],"show_active_only":true,"notification_seconds":20,"close_toasts":true,"desktop_notifications":false},"events":{"show_dividends":true,"show_splits":true,"show_earnings":false,"show_earnings_breaks":false,"show_economic_events":false,"show_news":true,"show_news_notifications":false,"show_ideas":true}}"#;
        // The margin of a new chart, and 60 pixels at the default bar spacing of 6 points; the
        // margin row after loading the template.
        for (pixels, row, literal) in [
            (None, 2.5, None),
            (Some(60.0), 2.0, None),
            (None, 2.5, Some(older)),
        ] {
            let mut saved =
                serde_json::to_value(ChartSettingsState::from_chart(&chart(&[100.0], &[110.0])))
                    .unwrap();
            saved["margins"] = serde_json::from_str(
                r#"{"top_percent":10.0,"bottom_percent":10.0,"right_bars":2}"#,
            )
            .unwrap();
            for (group, choice) in [
                ("", "show_session_breaks"),
                ("", "crosshair_lines"),
                ("", "extended_hours"),
                ("", "overnight_hours"),
                ("", "session_background"),
                ("", "back_adjust_futures"),
                ("", "settlement_as_close"),
                ("", "price_target_positive_color"),
                ("", "price_target_negative_color"),
                ("/scales_and_lines", "price_line_source"),
                ("/scales_and_lines", "prev_close_line"),
                ("/scales_and_lines", "show_bid_ask_lines"),
                ("/scales_and_lines", "show_high_low_lines"),
                ("/scales_and_lines", "thousands_sep"),
                ("/scales_and_lines", "date_format"),
                ("/scales_and_lines", "use_24_hour"),
                ("/scales_and_lines", "weekday_names"),
                ("/scales_appearance", "font_family"),
            ] {
                let group = saved.pointer_mut(group).unwrap().as_object_mut().unwrap();
                group.remove(choice).expect(choice);
            }
            if let Some(document) = literal {
                saved = serde_json::from_str(document).unwrap();
            }
            let mut options = crate::config::ChartOptions::default();
            options.crosshair.horz_line_visible = false;
            options.time_scale.right_offset_pixels = pixels;
            options.time_scale.use_24_hour = false;
            options.time_scale.weekday_names = WeekdayNames::Never;
            let mut chart = chart(&[100.0], &[110.0]).with_chart_options(options);
            chart.set_date_format(Some("%d.%m.%Y".into()));
            chart.config_mut().axis_font_family = FontFamily::Monospace;
            chart.config_mut().thousands_sep = Some('\'');
            chart.config_mut().show_session_breaks = true;
            chart.set_overnight_hours(true);
            chart.config_mut().show_extended_hours = false;
            chart.config_mut().premarket_color = Some(Color32::GOLD);
            chart.config_mut().price_target_positive_color = Color32::BLUE;
            chart.config_mut().price_target_negative_color = Color32::YELLOW;
            chart.pane_options_mut().separator_color = Some(Color32::RED);
            chart.config_mut().prev_close_line.width = 3.0;
            chart.config_mut().show_bid_ask_lines = Some(true);
            chart.config_mut().show_high_low_lines = Some(true);
            let style = chart.series_style_mut(SeriesId::MAIN).unwrap();
            style.price_line_source = PriceLineSource::LastVisible;
            let margin = chart.chart_options().time_scale.clone();
            let mut dialog = SettingsDialog::new();
            dialog
                .templates
                .save("Old", serde_json::from_value(saved).unwrap())
                .unwrap();
            let current = ChartSettingsState::from_chart(&chart);
            assert!(current.settlement_as_close);
            assert!(!current.back_adjust_futures);
            dialog.open(current);
            load_template(&mut dialog, "Old");
            let shown = dialog.settings();
            // Older templates hold the default futures data choices.
            assert!(shown.settlement_as_close);
            assert!(!shown.back_adjust_futures);
            assert_eq!(shown.extended_hours, Some(true), "{pixels:?}");
            assert_eq!(
                shown.crosshair_lines,
                Some(crate::widget::settings::CrosshairVisibility {
                    horizontal_line: false,
                    ..Default::default()
                })
            );
            assert_eq!(shown.overnight_hours, Some(true), "{pixels:?}");
            let backgrounds = shown.session_background.as_ref().unwrap();
            assert!(!backgrounds.show_extended_hours);
            assert_eq!(backgrounds.premarket_color, Some(Color32::GOLD));
            assert_eq!(shown.margins.right_bars, row, "{pixels:?}");
            assert_eq!(dialog.margin_right_str, row.to_string(), "{pixels:?}");
            assert_eq!(
                (
                    shown
                        .scales_and_lines
                        .prev_close_line
                        .map(|line| line.width),
                    shown.scales_and_lines.show_bid_ask_lines,
                    shown.scales_and_lines.show_high_low_lines,
                    shown.show_session_breaks,
                    shown.price_target_positive_color,
                    shown.price_target_negative_color,
                    shown.scales_and_lines.price_line_source,
                    shown.scales_and_lines.thousands_sep,
                    shown.pane_separator_color,
                    shown.scales_and_lines.date_format.clone(),
                    shown.scales_and_lines.use_24_hour,
                    shown.scales_and_lines.weekday_names,
                ),
                (
                    Some(3.0),
                    Some(true),
                    Some(true),
                    Some(true),
                    Some(Color32::BLUE),
                    Some(Color32::YELLOW),
                    Some(PriceLineSource::LastVisible),
                    Some(ThousandsSeparator::Apostrophe),
                    None,
                    Some(DateFormat::Pattern("%d.%m.%Y".into())),
                    Some(false),
                    Some(WeekdayNames::Never),
                ),
                "{pixels:?}"
            );
            dialog.settings().apply_to_chart(&mut chart).unwrap();
            assert_eq!(chart.chart_options().time_scale, margin, "{pixels:?}");
            assert_eq!(
                ChartSettingsState::from_chart(&chart)
                    .scales_and_lines
                    .date_format,
                Some(DateFormat::Pattern("%d.%m.%Y".into()))
            );
            let config = chart.config();
            assert_eq!(config.axis_font_family, FontFamily::Monospace);
            assert_eq!(config.thousands_sep, Some('\''));
            assert!(config.show_session_breaks);
            assert!(chart.extended_hours());
            assert!(!chart.chart_options().crosshair.horz_line_visible);
            let margins = chart.state().price_scale().options().scale_margins;
            assert_eq!((margins.top, margins.bottom), (0.1, 0.1));
            assert!(chart.overnight_hours());
            assert!(!config.show_extended_hours);
            assert_eq!(config.premarket_color, Some(Color32::GOLD));
            assert_eq!(
                (
                    config.price_target_positive_color,
                    config.price_target_negative_color
                ),
                (Color32::BLUE, Color32::YELLOW)
            );
            assert_eq!(config.prev_close_line.width, 3.0);
            assert_eq!(config.show_bid_ask_lines, Some(true));
            assert_eq!(config.show_high_low_lines, Some(true));
            let style = chart.series_style(SeriesId::MAIN).unwrap();
            assert_eq!(style.price_line_source, PriceLineSource::LastVisible);
            assert_eq!(chart.pane_options().separator_color, None, "{pixels:?}");
        }
    }

    /// A template saved from a chart that keeps its right margin in pixels, loaded on a chart that
    /// keeps the same pixels at another bar spacing, shows that chart's margin in bars, and Ok keeps
    /// its margin and its view. An edit of the margin row then sets the margin in bars.
    #[test]
    fn templates_keep_a_pixel_right_margin_and_edits_set_bars() {
        use crate::widget::test_frames::chart;
        let mut options = crate::config::ChartOptions::default();
        options.time_scale.right_offset_pixels = Some(60.0);
        let first = chart(&[100.0], &[110.0]).with_chart_options(options.clone());
        let mut second = chart(&[100.0], &[110.0]).with_chart_options(options);
        second.apply_synced_time_scale(12.0, -20.0);
        let saved = serde_json::to_value(ChartSettingsState::from_chart(&first)).unwrap();
        let mut dialog = SettingsDialog::new();
        dialog
            .templates
            .save("Pixels", serde_json::from_value(saved).unwrap())
            .unwrap();
        dialog.open(ChartSettingsState::from_chart(&second));
        load_template(&mut dialog, "Pixels");
        // 60 pixels are 10 bars at the 6 points per bar of the first chart and 5 at 12.
        assert_eq!(dialog.settings().margins.right_bars, 5.0);
        dialog.settings().apply_to_chart(&mut second).unwrap();
        let margin = |chart: &crate::Chart| {
            let time_scale = &chart.chart_options().time_scale;
            (
                time_scale.right_offset,
                time_scale.right_offset_pixels,
                chart.get_time_scale_state(),
            )
        };
        assert_eq!(margin(&second), (2.5, Some(60.0), (12.0, -20.0)));

        dialog.margin_right_str = "7.5".into();
        dialog.active_tab = SettingsTab::Canvas;
        let SettingsAction::Apply(settings) = click_through(&mut dialog, &["Ok"]) else {
            panic!("Ok must apply the settings");
        };
        settings.apply_to_chart(&mut second).unwrap();
        assert_eq!(margin(&second), (7.5, None, (12.0, 7.5)));
    }

    /// Margins that leave no room for prices, which the fit ignores, apply with Ok as a chart
    /// holds them and as a template saved from that chart loads them on another chart. Typed in,
    /// they keep Ok disabled.
    #[test]
    fn margins_without_room_apply_unless_typed() {
        use crate::scales::PriceScaleMargins;
        use crate::widget::test_frames::chart;
        let without_room = PriceScaleMargins {
            top: 0.6,
            bottom: 0.4,
        };
        let mut held = chart(&[100.0], &[110.0]);
        held.price_scale_mut().options_mut().scale_margins = without_room;
        let mut dialog = SettingsDialog::new();
        dialog
            .templates
            .save("No room", ChartSettingsState::from_chart(&held))
            .unwrap();
        let other = PriceScaleMargins::default();
        for (margins, template, typed, applied) in [
            (without_room, None, None, Some(without_room)),
            (other, Some("No room"), None, Some(without_room)),
            (other, None, Some(("60", "40")), None),
        ] {
            let mut chart = chart(&[100.0], &[110.0]);
            chart.price_scale_mut().options_mut().scale_margins = margins;
            dialog.open(ChartSettingsState::from_chart(&chart));
            if let Some(name) = template {
                load_template(&mut dialog, name);
            }
            if let Some((top, bottom)) = typed {
                dialog.margin_top_str = top.into();
                dialog.margin_bottom_str = bottom.into();
                dialog.active_tab = SettingsTab::Canvas;
            }
            let shown = match click_through(&mut dialog, &["Ok"]) {
                SettingsAction::Apply(settings) => {
                    settings.apply_to_chart(&mut chart).unwrap();
                    Some(chart.state().price_scale().options().scale_margins)
                }
                _ => None,
            };
            assert_eq!(shown, applied, "{template:?} {typed:?}");
        }
    }
}
