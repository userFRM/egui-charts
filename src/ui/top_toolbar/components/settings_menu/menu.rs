//! Quick settings menu popup.
//!
//! A compact popup menu for quick chart configuration changes (chart type,
//! grid visibility, scale mode, and label options).

use super::{config::ChartSettings, types::ScaleMode};
use crate::config::ChartConfig;
use crate::ext::UiExt;
use crate::model::ChartType;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Context, Id, Pos2, Rect, Response, Ui};

// ============================================================================
// Settings Menu (Quick Popup)
// ============================================================================

/// Quick settings popup menu for chart configuration
pub struct SettingsMenu {
    /// Current chart settings
    pub settings: ChartSettings,
    /// Whether the menu popup is open
    pub is_open: bool,
    /// Rect of the trigger button (for popup positioning)
    btn_rect: Option<Rect>,
}

impl SettingsMenu {
    /// Create a new settings menu with default chart settings
    pub fn new() -> Self {
        Self {
            settings: ChartSettings::default(),
            is_open: false,
            btn_rect: None,
        }
    }

    /// Get a reference to the current chart settings
    pub fn settings(&self) -> &ChartSettings {
        &self.settings
    }

    /// Get a mutable reference to the current chart settings
    pub fn settings_mut(&mut self) -> &mut ChartSettings {
        &mut self.settings
    }

    /// Apply current settings to a ChartConfig
    pub fn apply_to_config(&self, mut config: ChartConfig) -> ChartConfig {
        config.show_grid = self.settings.show_horizontal_grid;
        config.show_horizontal_grid = self.settings.show_horizontal_grid;
        config.show_vertical_grid = self.settings.show_vertical_grid;
        config.show_right_axis = self.settings.show_right_axis;
        config.show_left_axis = self.settings.show_left_axis;
        config.show_symbol_labels = self.settings.show_symbol_labels;
        config.show_symbol_last_val = self.settings.show_symbol_last_val;
        config.show_symbol_prev_close = self.settings.show_symbol_prev_close;
        config.show_indicator_labels = self.settings.show_indicator_labels;
        config.show_indicator_last_val = self.settings.show_indicator_last_val;
        config.show_countdown = self.settings.show_countdown;
        config
    }

    /// Get the currently selected chart type
    pub fn chart_type(&self) -> ChartType {
        self.settings.chart_type
    }

    /// Show the settings button (bottom-right corner)
    pub fn show_btn(&mut self, ui: &mut Ui, chart_rect: Rect) -> Response {
        let btn_size = egui::vec2(32.0, 32.0);
        let btn_pos = Pos2::new(
            chart_rect.max.x - btn_size.x - 10.0,
            chart_rect.max.y - btn_size.y - 10.0,
        );

        let btn_rect = Rect::from_min_size(btn_pos, btn_size);
        let btn_id = ui.id().with("settings_btn");

        let response = ui.interact(btn_rect, btn_id, egui::Sense::click());

        let visuals = ui.style().interact(&response);
        let bg_color = if response.hovered() {
            visuals.bg_fill
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };

        ui.painter()
            .rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().text(
            btn_rect.center(),
            Align2::CENTER_CENTER,
            "S",
            egui::FontId::proportional(typography::TITLE),
            visuals.text_color(),
        );

        if response.clicked() {
            self.is_open = !self.is_open;
        }

        self.btn_rect = Some(btn_rect);
        response
    }

    /// Show the settings popup menu
    pub fn show_menu(&mut self, ctx: &Context) {
        if !self.is_open {
            return;
        }
        let Some(btn_rect) = self.btn_rect else {
            return;
        };

        let popup_id = Id::new("settings_popup");
        let popup_pos = Pos2::new(btn_rect.min.x - 200.0, btn_rect.min.y - 400.0);

        egui::Area::new(popup_id)
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.set_max_width(220.0);
                    self.draw_menu_sections(ui);
                });
            });

        self.handle_click_outside(ctx, btn_rect, popup_pos);
    }

    fn draw_menu_sections(&mut self, ui: &mut egui::Ui) {
        self.draw_chart_type_section(ui);
        self.draw_appearance_section(ui);
        self.draw_scale_section(ui);
        self.draw_labels_section(ui);

        ui.separator();
        if ui.button("Scales Properties...").clicked() {
            self.is_open = false;
        }
    }

    fn draw_chart_type_section(&mut self, ui: &mut egui::Ui) {
        ui.strong_label("Chart Type");
        ui.separator();

        for chart_type in &[
            ChartType::Candles,
            ChartType::Bars,
            ChartType::Line,
            ChartType::Area,
        ] {
            if ui
                .selectable_label(self.settings.chart_type == *chart_type, chart_type.as_str())
                .clicked()
            {
                self.settings.chart_type = *chart_type;
            }
        }
        ui.separator();
    }

    fn draw_appearance_section(&mut self, ui: &mut egui::Ui) {
        ui.strong_label("Appearance");
        ui.separator();
        ui.checkbox(&mut self.settings.show_horizontal_grid, "Horizontal Grid");
        ui.checkbox(&mut self.settings.show_vertical_grid, "Vertical Grid");
        ui.separator();
    }

    fn draw_scale_section(&mut self, ui: &mut egui::Ui) {
        ui.strong_label("Scale");
        ui.separator();

        if ui.button("Reset Scale").clicked() {
            self.is_open = false;
        }

        ui.checkbox(&mut self.settings.show_left_axis, "Left Axis");
        ui.checkbox(&mut self.settings.show_right_axis, "Right Axis");

        if ui
            .selectable_label(self.settings.scale_mode == ScaleMode::Auto, "Auto Scale")
            .clicked()
        {
            self.settings.scale_mode = ScaleMode::Auto;
        }

        ui.checkbox(&mut self.settings.lock_scale, "Lock Scale");
        ui.checkbox(
            &mut self.settings.scale_price_chart_only,
            "Scale Price Chart Only",
        );
        ui.separator();

        if ui
            .selectable_label(
                self.settings.scale_mode == ScaleMode::Percentage,
                "Percentage",
            )
            .clicked()
        {
            self.settings.scale_mode = ScaleMode::Percentage;
        }
        if ui
            .selectable_label(
                self.settings.scale_mode == ScaleMode::Logarithmic,
                "Log Scale",
            )
            .clicked()
        {
            self.settings.scale_mode = ScaleMode::Logarithmic;
        }
        ui.separator();
    }

    fn draw_labels_section(&mut self, ui: &mut egui::Ui) {
        ui.strong_label("Labels");
        ui.separator();

        ui.checkbox(&mut self.settings.show_symbol_labels, "Symbol Labels");
        ui.checkbox(&mut self.settings.show_symbol_last_val, "Symbol Last Value");
        ui.checkbox(
            &mut self.settings.show_symbol_prev_close,
            "Symbol Prev. Close Value",
        );
        ui.separator();

        ui.checkbox(&mut self.settings.show_indicator_labels, "Indicator Labels");
        ui.checkbox(
            &mut self.settings.show_indicator_last_val,
            "Indicator Last Value",
        );
        ui.separator();

        ui.checkbox(&mut self.settings.show_countdown, "Countdown");
        ui.checkbox(
            &mut self.settings.no_overlapping_labels,
            "No Overlapping Labels",
        );
    }

    fn handle_click_outside(&mut self, ctx: &Context, btn_rect: Rect, popup_pos: Pos2) {
        if ctx.input(|i| i.pointer.any_released())
            && let Some(pos) = ctx.input(|i| i.pointer.interact_pos())
        {
            let menu_rect = Rect::from_min_size(
                popup_pos,
                egui::vec2(
                    DESIGN_TOKENS.sizing.dialog.menu_settings_width,
                    DESIGN_TOKENS.sizing.dialog.menu_settings_height,
                ),
            );
            if !menu_rect.contains(pos) && !btn_rect.contains(pos) {
                self.is_open = false;
            }
        }
    }
}

impl Default for SettingsMenu {
    fn default() -> Self {
        Self::new()
    }
}
