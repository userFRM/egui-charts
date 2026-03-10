//! Floating chart control bar at bottom-right of chart area.
//!
//! Provides zoom controls and scale toggles in a compact floating bar.

use crate::icons::{Icon, icons as embedded_icons};
use egui::{Pos2, Rect, Sense, Ui, Vec2};

use super::actions::ChartControlAction;
use super::config::ChartControlBarConfig;
use crate::tokens::DESIGN_TOKENS;

/// State for the chart control bar
#[derive(Clone, Debug, Default)]
pub struct ChartControlBarState {
    /// Whether auto-scale is enabled
    pub auto_scale: bool,
    /// Whether log scale is enabled
    pub log_scale: bool,
    /// Whether percentage mode is enabled
    pub percentage: bool,
}

/// Floating chart control bar
pub struct ChartControlBar {
    pub config: ChartControlBarConfig,
    pub state: ChartControlBarState,
}

impl Default for ChartControlBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartControlBar {
    /// Create a new chart control bar with default configuration
    pub fn new() -> Self {
        Self {
            config: ChartControlBarConfig::default(),
            state: ChartControlBarState::default(),
        }
    }

    /// Set a custom configuration
    pub fn with_config(mut self, config: ChartControlBarConfig) -> Self {
        self.config = config;
        self
    }

    /// Sync chart control bar state from external values.
    ///
    /// Call this at the beginning of show() to ensure local state reflects
    /// the central state after any actions have been dispatched.
    pub fn sync_from(&mut self, auto_scale: bool, log_scale: bool, percentage: bool) {
        self.state.auto_scale = auto_scale;
        self.state.log_scale = log_scale;
        self.state.percentage = percentage;
    }

    /// Show the control bar at the specified position (bottom-right corner)
    /// Returns any action triggered
    pub fn show(&mut self, ui: &mut Ui, chart_rect: Rect) -> ChartControlAction {
        let mut action = ChartControlAction::None;

        // Get colors from theme
        let bg_color = ui.style().visuals.window_fill;
        let border_stroke = ui.style().visuals.window_stroke;
        let icon_color = ui.style().visuals.widgets.inactive.fg_stroke.color;
        let icon_hover_color = ui.style().visuals.widgets.hovered.fg_stroke.color;
        let active_color = ui.style().visuals.selection.bg_fill;
        let btn_hover_bg = ui.style().visuals.widgets.hovered.bg_fill;
        let sep_stroke = ui.style().visuals.widgets.noninteractive.bg_stroke;

        // Calculate bar dimensions
        // Buttons: ZoomIn, ZoomOut, Reset | Auto, Log, %
        let num_buttons = 6;
        let separator_width = DESIGN_TOKENS.spacing.sm;
        let bar_width = (self.config.button_size * num_buttons as f32)
            + (self.config.button_gap * (num_buttons - 1) as f32)
            + separator_width
            + (self.config.padding * 2.0);
        let bar_height = self.config.button_size + (self.config.padding * 2.0);

        // Position at bottom-right of chart rect
        let bar_pos = Pos2::new(
            chart_rect.max.x - bar_width - self.config.offset_x,
            chart_rect.max.y - bar_height - self.config.offset_y,
        );
        let bar_rect = Rect::from_min_size(bar_pos, Vec2::new(bar_width, bar_height));

        // Allocate the bar area
        let bar_response = ui.allocate_rect(bar_rect, Sense::hover());

        if ui.is_rect_visible(bar_rect) {
            // Draw bar background
            ui.painter().rect(
                bar_rect,
                self.config.rounding,
                bg_color,
                border_stroke,
                egui::StrokeKind::Inside,
            );

            // Button definitions
            let buttons: [(&Icon, &str, ChartControlAction, bool); 3] = [
                (
                    &embedded_icons::ZOOM_IN,
                    "Zoom In",
                    ChartControlAction::ZoomIn,
                    false,
                ),
                (
                    &embedded_icons::ZOOM_OUT,
                    "Zoom Out",
                    ChartControlAction::ZoomOut,
                    false,
                ),
                (
                    &embedded_icons::MEASURE,
                    "Reset Zoom",
                    ChartControlAction::ResetZoom,
                    false,
                ),
            ];

            let toggles: [(&Icon, &str, ChartControlAction, bool); 3] = [
                (
                    &embedded_icons::SETTINGS,
                    "Auto Scale",
                    ChartControlAction::ToggleAutoScale,
                    self.state.auto_scale,
                ),
                (
                    &embedded_icons::LAYOUT_GRID,
                    "Log Scale",
                    ChartControlAction::ToggleLogScale,
                    self.state.log_scale,
                ),
                (
                    &embedded_icons::TEXT,
                    "Percentage",
                    ChartControlAction::TogglePercentage,
                    self.state.percentage,
                ),
            ];

            let mut x = bar_rect.min.x + self.config.padding;
            let center_y = bar_rect.center().y;
            let icon_size = 16.0;

            // Draw zoom buttons
            for (icon, tooltip, btn_action, is_active) in buttons {
                let btn_rect = Rect::from_center_size(
                    Pos2::new(x + self.config.button_size / 2.0, center_y),
                    Vec2::splat(self.config.button_size),
                );

                let btn_response = ui.allocate_rect(btn_rect, Sense::click());
                let is_hovered = btn_response.hovered();
                let was_clicked = btn_response.clicked();

                // Button background on hover
                if is_hovered {
                    ui.painter()
                        .rect_filled(btn_rect, DESIGN_TOKENS.rounding.button, btn_hover_bg);
                }
                if is_active {
                    ui.painter()
                        .rect_filled(btn_rect, DESIGN_TOKENS.rounding.button, active_color);
                }

                // Icon
                let icon_rect = Rect::from_center_size(btn_rect.center(), Vec2::splat(icon_size));
                let icon_color_final = if is_hovered {
                    icon_hover_color
                } else {
                    icon_color
                };
                icon.as_image_tinted(Vec2::splat(icon_size), icon_color_final)
                    .paint_at(ui, icon_rect);

                btn_response.on_hover_text(tooltip);

                if was_clicked {
                    action = btn_action;
                }

                x += self.config.button_size + self.config.button_gap;
            }

            // Separator
            x += separator_width / 2.0;
            let sep_y1 = bar_rect.min.y + self.config.padding;
            let sep_y2 = bar_rect.max.y - self.config.padding;
            ui.painter()
                .line_segment([Pos2::new(x, sep_y1), Pos2::new(x, sep_y2)], sep_stroke);
            x += separator_width / 2.0 + self.config.button_gap;

            // Draw toggle buttons
            for (icon, tooltip, btn_action, is_active) in toggles {
                let btn_rect = Rect::from_center_size(
                    Pos2::new(x + self.config.button_size / 2.0, center_y),
                    Vec2::splat(self.config.button_size),
                );

                let btn_response = ui.allocate_rect(btn_rect, Sense::click());
                let is_hovered = btn_response.hovered();
                let was_clicked = btn_response.clicked();

                // Button background
                if is_active {
                    ui.painter()
                        .rect_filled(btn_rect, DESIGN_TOKENS.rounding.button, active_color);
                } else if is_hovered {
                    ui.painter()
                        .rect_filled(btn_rect, DESIGN_TOKENS.rounding.button, btn_hover_bg);
                }

                // Icon
                let icon_rect = Rect::from_center_size(btn_rect.center(), Vec2::splat(icon_size));
                let icon_color_final = if is_active || is_hovered {
                    icon_hover_color
                } else {
                    icon_color
                };
                icon.as_image_tinted(Vec2::splat(icon_size), icon_color_final)
                    .paint_at(ui, icon_rect);

                btn_response.on_hover_text(tooltip);

                if was_clicked {
                    action = btn_action.clone();
                    // Update state
                    match btn_action {
                        ChartControlAction::ToggleAutoScale => {
                            self.state.auto_scale = !self.state.auto_scale;
                        }
                        ChartControlAction::ToggleLogScale => {
                            self.state.log_scale = !self.state.log_scale;
                        }
                        ChartControlAction::TogglePercentage => {
                            self.state.percentage = !self.state.percentage;
                        }
                        _ => {}
                    }
                }

                x += self.config.button_size + self.config.button_gap;
            }
        }

        // Consume hover to prevent chart interaction under the bar
        if bar_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }

        action
    }
}
