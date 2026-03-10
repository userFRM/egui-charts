//! Chart Grid Layout Selector
//!
//! A dropdown menu for selecting multi-chart grid layouts (1x1, 2x1, 2x2, etc.)

use crate::icons::icons as embedded_icons;
use crate::styles::typography;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::ChartLayoutMode;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Chart grid selector widget
pub struct ChartGridSelector {
    /// Is dropdown open
    is_open: bool,
    /// Current layout mode
    current: ChartLayoutMode,
}

impl Default for ChartGridSelector {
    fn default() -> Self {
        Self::new(ChartLayoutMode::Single)
    }
}

impl ChartGridSelector {
    /// Create new selector with initial layout
    pub fn new(initial: ChartLayoutMode) -> Self {
        Self {
            is_open: false,
            current: initial,
        }
    }

    /// Get current layout mode
    pub fn current(&self) -> ChartLayoutMode {
        self.current
    }

    /// Set current layout mode
    pub fn set_layout(&mut self, layout: ChartLayoutMode) {
        self.current = layout;
    }

    /// Show the selector and return action if layout changed
    pub fn show(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        let mut result = None;

        // Main button
        let btn_response = self.draw_button(ui);
        if btn_response.clicked() {
            self.is_open = !self.is_open;
        }

        // Dropdown
        if self.is_open {
            let btn_rect = btn_response.rect;
            if let Some(selected) = self.draw_dropdown(ui, btn_rect) {
                self.current = selected;
                self.is_open = false;
                result = Some(selected);
            }

            // Close on click outside
            if ui.input(|i| i.pointer.any_click())
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            {
                let dropdown_rect = self.dropdown_rect(btn_rect);
                if !btn_rect.contains(pos) && !dropdown_rect.contains(pos) {
                    self.is_open = false;
                }
            }
        }

        result
    }

    fn draw_button(&self, ui: &mut Ui) -> Response {
        let visuals = ui.style().visuals.clone();
        let hover_color = visuals.widgets.hovered.bg_fill;
        let border_color = visuals.widgets.noninteractive.bg_stroke.color;
        let text_color = visuals.text_color();
        let muted_color = visuals.widgets.noninteractive.fg_stroke.color;

        let desired_size = Vec2::new(
            DESIGN_TOKENS.sizing.button_lg + DESIGN_TOKENS.spacing.xxxl,
            DESIGN_TOKENS.sizing.button_md,
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Background
        let bg_color = if self.is_open || response.hovered() {
            hover_color
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

        // Grid icon
        let icon_rect = Rect::from_min_size(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.sm,
                rect.center().y - DESIGN_TOKENS.spacing.lg,
            ),
            Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
        );
        let icon_color = if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        embedded_icons::LAYOUT_GRID
            .as_image_tinted(Vec2::splat(DESIGN_TOKENS.sizing.icon_sm), icon_color)
            .paint_at(ui, icon_rect);

        // Layout label
        let label = self.current.label();
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.sm + DESIGN_TOKENS.sizing.icon_md,
                rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(typography::SM),
            text_color,
        );

        // Dropdown arrow
        ui.painter().text(
            Pos2::new(rect.right() - DESIGN_TOKENS.spacing.md, rect.center().y),
            egui::Align2::CENTER_CENTER,
            "▾",
            egui::FontId::proportional(typography::XS),
            muted_color,
        );

        response.on_hover_text("Chart Layout")
    }

    fn dropdown_rect(&self, btn_rect: Rect) -> Rect {
        let item_height = DESIGN_TOKENS.sizing.button_md;
        let item_count = ChartLayoutMode::all().len();
        let dropdown_height = item_count as f32 * item_height + DESIGN_TOKENS.spacing.md * 2.0;
        Rect::from_min_size(
            Pos2::new(btn_rect.min.x, btn_rect.max.y + DESIGN_TOKENS.spacing.xs),
            Vec2::new(
                DESIGN_TOKENS.sizing.dialog.grid_dropdown_width,
                dropdown_height,
            ),
        )
    }

    fn draw_dropdown(&mut self, ui: &mut Ui, btn_rect: Rect) -> Option<ChartLayoutMode> {
        let visuals = ui.style().visuals.clone();
        let bg_color = visuals.window_fill;
        let border_color = visuals.widgets.noninteractive.bg_stroke.color;
        let hover_color = visuals.widgets.hovered.bg_fill;
        let accent_color = visuals.selection.bg_fill;
        let text_color = visuals.text_color();
        let strong_color = visuals.strong_text_color();

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

        let mut selected = None;
        let item_height = 28.0;
        let mut y = dropdown_rect.min.y + DESIGN_TOKENS.spacing.sm;

        for layout in ChartLayoutMode::all() {
            let is_current = *layout == self.current;
            let item_rect = Rect::from_min_size(
                Pos2::new(dropdown_rect.min.x + DESIGN_TOKENS.spacing.sm, y),
                Vec2::new(
                    dropdown_rect.width() - DESIGN_TOKENS.spacing.md * 2.0,
                    item_height,
                ),
            );

            let item_response = ui.allocate_rect(item_rect, Sense::click());

            // Highlight current or hovered
            if is_current || item_response.hovered() {
                let bg = if is_current {
                    accent_color
                } else {
                    hover_color
                };
                ui.painter()
                    .rect_filled(item_rect, DESIGN_TOKENS.rounding.sm, bg);
            }

            // Grid icon (mini representation)
            let (cols, rows) = layout.grid_size();
            let icon_size = 12.0;
            let icon_x = item_rect.min.x + DESIGN_TOKENS.spacing.sm;
            let icon_y = item_rect.center().y - icon_size / 2.0;
            self.draw_grid_icon(
                ui,
                Pos2::new(icon_x, icon_y),
                cols,
                rows,
                icon_size,
                text_color,
            );

            // Label
            let label_color = if is_current { strong_color } else { text_color };
            ui.painter().text(
                Pos2::new(
                    item_rect.min.x
                        + DESIGN_TOKENS.spacing.sm
                        + icon_size
                        + DESIGN_TOKENS.spacing.sm,
                    item_rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                layout.label(),
                egui::FontId::proportional(typography::SM),
                label_color,
            );

            if item_response.clicked() {
                selected = Some(*layout);
            }

            y += item_height;
        }

        selected
    }

    /// Draw a mini grid icon representing the layout
    fn draw_grid_icon(
        &self,
        ui: &mut Ui,
        pos: Pos2,
        cols: usize,
        rows: usize,
        size: f32,
        color: Color32,
    ) {
        let cell_w = size / cols.max(1) as f32;
        let cell_h = size / rows.max(1) as f32;
        let gap = 1.0;

        for row in 0..rows {
            for col in 0..cols {
                let x = pos.x + col as f32 * cell_w + gap / 2.0;
                let y = pos.y + row as f32 * cell_h + gap / 2.0;
                let rect =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w - gap, cell_h - gap));
                ui.painter().rect_filled(rect, 1.0, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_grid_selector_default() {
        let selector = ChartGridSelector::default();
        assert_eq!(selector.current(), ChartLayoutMode::Single);
    }

    #[test]
    fn test_set_layout() {
        let mut selector = ChartGridSelector::new(ChartLayoutMode::Single);
        selector.set_layout(ChartLayoutMode::Four);
        assert_eq!(selector.current(), ChartLayoutMode::Four);
    }
}
