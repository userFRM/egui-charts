//! Multi-Chart Sync Options Button
//!
//! A button with dropdown for configuring multi-chart synchronization options.
//! Only visible when in multi-chart layout mode.

use crate::icons::icons as embedded_icons;
use crate::styles::typography;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::multi_chart::ChartSyncOptions;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Sync options button widget
pub struct SyncButton {
    /// Is dropdown open
    is_open: bool,
    /// Current sync options
    options: ChartSyncOptions,
}

impl Default for SyncButton {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncButton {
    /// Create new sync button with default options
    pub fn new() -> Self {
        Self {
            is_open: false,
            options: ChartSyncOptions::default(),
        }
    }

    /// Set current sync options
    pub fn set_options(&mut self, options: ChartSyncOptions) {
        self.options = options;
    }

    /// Get current sync options
    pub fn options(&self) -> &ChartSyncOptions {
        &self.options
    }

    /// Show the button and return updated options if changed
    pub fn show(&mut self, ui: &mut Ui) -> Option<ChartSyncOptions> {
        let mut result = None;

        // Main button
        let btn_response = self.draw_button(ui);
        if btn_response.clicked() {
            self.is_open = !self.is_open;
        }

        // Dropdown
        if self.is_open {
            let btn_rect = btn_response.rect;
            if let Some(new_options) = self.draw_dropdown(ui, btn_rect) {
                self.options = new_options.clone();
                result = Some(new_options);
            }

            // Close on click outside
            if ui.input(|i| i.pointer.any_click()) {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let dropdown_rect = self.dropdown_rect(btn_rect);
                    if !btn_rect.contains(pos) && !dropdown_rect.contains(pos) {
                        self.is_open = false;
                    }
                }
            }
        }

        result
    }

    fn draw_button(&self, ui: &mut Ui) -> Response {
        let visuals = ui.style().visuals.clone();
        let hover_color = visuals.widgets.hovered.bg_fill;
        let border_color = visuals.widgets.noninteractive.bg_stroke.color;
        let accent_color = visuals.selection.bg_fill;

        // Check if any sync is enabled
        let any_sync_enabled = self.options.sync_time_axis
            || self.options.sync_crosshair
            || self.options.sync_drawings
            || self.options.sync_symbol
            || self.options.sync_timeframe;

        let desired_size = Vec2::splat(DESIGN_TOKENS.sizing.button_md);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Background - highlight if sync is enabled
        let bg_color = if self.is_open || response.hovered() {
            hover_color
        } else if any_sync_enabled {
            accent_color.gamma_multiply(0.3)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().rect_stroke(
            rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Sync icon (using layout grid as visual representation)
        let icon_rect =
            Rect::from_center_size(rect.center(), Vec2::splat(DESIGN_TOKENS.sizing.icon_sm));
        let icon_color = if any_sync_enabled {
            visuals.selection.stroke.color
        } else if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        embedded_icons::LAYOUT_GRID
            .as_image_tinted(Vec2::splat(DESIGN_TOKENS.sizing.icon_sm), icon_color)
            .paint_at(ui, icon_rect);

        response.on_hover_text("Chart Sync Options")
    }

    fn dropdown_rect(&self, btn_rect: Rect) -> Rect {
        let item_height = DESIGN_TOKENS.sizing.button_md;
        let item_count = 5; // 5 sync options
        let dropdown_height = item_count as f32 * item_height + DESIGN_TOKENS.spacing.md * 2.0;
        let dropdown_width = 180.0;
        Rect::from_min_size(
            Pos2::new(btn_rect.min.x, btn_rect.max.y + DESIGN_TOKENS.spacing.xs),
            Vec2::new(dropdown_width, dropdown_height),
        )
    }

    fn draw_dropdown(&mut self, ui: &mut Ui, btn_rect: Rect) -> Option<ChartSyncOptions> {
        let visuals = ui.style().visuals.clone();
        let bg_color = visuals.window_fill;
        let border_color = visuals.widgets.noninteractive.bg_stroke.color;
        let hover_color = visuals.widgets.hovered.bg_fill;
        let text_color = visuals.text_color();
        let accent_color = visuals.selection.bg_fill;
        let check_color = visuals.selection.stroke.color;

        let dropdown_rect = self.dropdown_rect(btn_rect);

        // Shadow
        let shadow_rect = dropdown_rect.translate(Vec2::splat(DESIGN_TOKENS.shadow.offset_sm));
        ui.painter().rect_filled(
            shadow_rect,
            DESIGN_TOKENS.rounding.md,
            Color32::from_black_alpha(60),
        );

        // Background
        ui.painter()
            .rect_filled(dropdown_rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().rect_stroke(
            dropdown_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        let mut changed = false;
        let mut new_options = self.options.clone();
        let item_height = DESIGN_TOKENS.sizing.button_md;
        let mut y = dropdown_rect.min.y + DESIGN_TOKENS.spacing.sm;

        // Render each sync option as a checkbox row
        let sync_items = [
            ("Time Axis", new_options.sync_time_axis, 0),
            ("Crosshair", new_options.sync_crosshair, 1),
            ("Drawings", new_options.sync_drawings, 2),
            ("Symbol", new_options.sync_symbol, 3),
            ("Timeframe", new_options.sync_timeframe, 4),
        ];

        for (label, enabled, idx) in sync_items {
            let item_rect = Rect::from_min_size(
                Pos2::new(dropdown_rect.min.x + DESIGN_TOKENS.spacing.sm, y),
                Vec2::new(
                    dropdown_rect.width() - DESIGN_TOKENS.spacing.md * 2.0,
                    item_height,
                ),
            );

            let item_response = ui.allocate_rect(item_rect, Sense::click());

            // Highlight on hover
            if item_response.hovered() {
                ui.painter()
                    .rect_filled(item_rect, DESIGN_TOKENS.rounding.sm, hover_color);
            }

            // Checkbox
            let checkbox_size = 14.0;
            let checkbox_rect = Rect::from_min_size(
                Pos2::new(
                    item_rect.min.x + DESIGN_TOKENS.spacing.sm,
                    item_rect.center().y - checkbox_size / 2.0,
                ),
                Vec2::splat(checkbox_size),
            );

            if enabled {
                ui.painter().rect_filled(checkbox_rect, 2.0, accent_color);
                // Checkmark
                ui.painter().text(
                    checkbox_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{2713}",
                    egui::FontId::proportional(checkbox_size * 0.8),
                    check_color,
                );
            } else {
                ui.painter().rect_stroke(
                    checkbox_rect,
                    2.0,
                    Stroke::new(1.0, border_color),
                    StrokeKind::Inside,
                );
            }

            // Label
            ui.painter().text(
                Pos2::new(
                    item_rect.min.x
                        + DESIGN_TOKENS.spacing.sm
                        + checkbox_size
                        + DESIGN_TOKENS.spacing.sm,
                    item_rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(typography::SM),
                text_color,
            );

            if item_response.clicked() {
                // Toggle the appropriate option
                match idx {
                    0 => new_options.sync_time_axis = !enabled,
                    1 => new_options.sync_crosshair = !enabled,
                    2 => new_options.sync_drawings = !enabled,
                    3 => new_options.sync_symbol = !enabled,
                    4 => new_options.sync_timeframe = !enabled,
                    _ => {}
                }
                changed = true;
            }

            y += item_height;
        }

        if changed { Some(new_options) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_button_default() {
        let btn = SyncButton::new();
        assert!(btn.options().sync_time_axis);
        assert!(btn.options().sync_crosshair);
        assert!(!btn.options().sync_drawings);
    }

    #[test]
    fn test_set_options() {
        let mut btn = SyncButton::new();
        let mut opts = ChartSyncOptions::default();
        opts.sync_drawings = true;
        btn.set_options(opts.clone());
        assert!(btn.options().sync_drawings);
    }
}
