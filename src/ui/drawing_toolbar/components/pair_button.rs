//! Pair button component: [icon button | arrow button]
//!
//! Used for: cursors, tool categories, magnet, hide, remove btns.

use crate::icons::Icon;
use crate::theming;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::svg_helpers::render_svg_at_rect_themed;
use crate::tokens::DESIGN_TOKENS;

/// Response from a pair button interaction
#[derive(Debug, Clone)]
pub struct PairButtonResponse {
    /// Whether the icon (left) part was clicked
    pub icon_clicked: bool,
    /// Whether the arrow (right) part was clicked
    pub arrow_clicked: bool,
    /// Whether icon part is hovered
    pub icon_hovered: bool,
    /// Whether arrow part is hovered
    pub arrow_hovered: bool,
    /// Rect of the icon button (for anchoring submenus)
    pub icon_rect: Rect,
    /// Rect of the arrow button (for anchoring submenus)
    pub arrow_rect: Rect,
}

/// Direction for arrow indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Pair button: [icon button | arrow button]
///
/// Layout: [left_padding | icon centered | arrow]
/// - Total width: responsive (default 48px)
/// - Total height: icon_size + padding*2 (default 48px)
/// - Arrow width: padding (10px)
/// - Left padding: padding (10px)
///
/// # Example
///
/// ```ignore
/// let btn = PairButton {
///     icon: &icons::ERASER,
///     tooltip: "Cross cursor",
///     arrow_tooltip: "More cursors",
///     is_icon_active: false,
///     is_expanded: false,
/// };
/// let response = btn.show(&mut ui, 48.0, 48.0, 28.0, 10.0);
/// if response.icon_clicked {
///     // Handle icon click
/// }
/// if response.arrow_clicked {
///     // Handle arrow click (expand submenu)
/// }
/// ```
pub struct PairButton<'a> {
    /// Icon to display
    pub icon: &'a Icon,
    /// Tooltip for the icon button
    pub tooltip: &'a str,
    /// Tooltip for the arrow button
    pub arrow_tooltip: &'a str,
    /// Whether the icon is in active state
    pub is_icon_active: bool,
    /// Whether the category is expanded (affects arrow direction)
    pub is_expanded: bool,
}

impl<'a> PairButton<'a> {
    /// Render the pair button and return interaction response
    pub fn show(
        &self,
        ui: &mut Ui,
        width: f32,
        height: f32,
        icon_size: f32,
        padding: f32,
    ) -> PairButtonResponse {
        let cursor_pos = ui.cursor().min;

        // Full button rect
        let pair_rect = Rect::from_min_size(cursor_pos, Vec2::new(width, height));

        // Layout: [left_padding | icon | arrow]
        let arrow_width = padding;
        let _left_padding = arrow_width;

        // Icon button (centered in full width for visual balance)
        let icon_rect = Rect::from_center_size(pair_rect.center(), Vec2::splat(icon_size));
        let icon_res = ui.allocate_rect(icon_rect, Sense::click());

        // Arrow button (right edge)
        let arrow_rect = Rect::from_min_size(
            Pos2::new(pair_rect.max.x - arrow_width, pair_rect.min.y),
            Vec2::new(arrow_width, height),
        );
        let arrow_res = ui.allocate_rect(arrow_rect, Sense::click());

        // Draw icon button background (with visibility check)
        if ui.is_rect_visible(icon_rect) {
            let icon_bg = if self.is_icon_active && icon_res.hovered() {
                theming::active_hover_color(ui)
            } else if self.is_icon_active {
                theming::sel_color(ui)
            } else if icon_res.hovered() {
                theming::hover_color(ui)
            } else {
                Color32::TRANSPARENT // Let parent Frame's background show through
            };
            ui.painter()
                .rect_filled(icon_rect, DESIGN_TOKENS.rounding.button, icon_bg);
        }

        // Draw icon centered in interactive area (excluding left padding and arrow section)
        let interactive_width = width - padding - arrow_width;
        let icon_center_x = pair_rect.min.x + padding + interactive_width / 2.0;
        let icon_rect_center = Rect::from_center_size(
            Pos2::new(icon_center_x, pair_rect.center().y),
            Vec2::splat(icon_size),
        );
        if ui.is_rect_visible(icon_rect_center) {
            render_svg_at_rect_themed(
                ui,
                self.icon,
                icon_rect_center,
                icon_res.hovered(),
                self.is_icon_active,
            );
        }

        // Draw arrow (with visibility check)
        if ui.is_rect_visible(arrow_rect) {
            let arrow_stroke = Stroke::new(
                1.2,
                if arrow_res.hovered() {
                    theming::active_icon_color(ui)
                } else {
                    theming::icon_color(ui)
                },
            );

            let arrow_center = arrow_rect.center();
            draw_arrow(
                ui,
                arrow_center,
                if self.is_expanded {
                    ArrowDirection::Up
                } else {
                    ArrowDirection::Down
                },
                arrow_stroke,
            );
        }

        // Extract clicked state before consuming with on_hover_text
        let icon_clicked = icon_res.clicked();
        let arrow_clicked = arrow_res.clicked();
        let icon_hovered = icon_res.hovered();
        let arrow_hovered = arrow_res.hovered();

        // Tooltips (consumes the responses)
        icon_res.on_hover_text(self.tooltip);
        arrow_res.on_hover_text(self.arrow_tooltip);

        PairButtonResponse {
            icon_clicked,
            arrow_clicked,
            icon_hovered,
            arrow_hovered,
            icon_rect,
            arrow_rect,
        }
    }

    /// Render the pair button in styled layout.
    ///
    /// Uses exact dimensions:
    /// - Outer rect: 52×38px
    /// - Inner hover rect: 9px horizontal margin, 2px vertical margin
    /// - Inner rounding: 6px
    /// - Icon: 28×28 centered
    pub fn show_styled(&self, ui: &mut Ui) -> PairButtonResponse {
        let tokens = &DESIGN_TOKENS.sizing.toolbar;
        let btn_width = tokens.button_width;
        let btn_height = tokens.button_height;
        let margin_h = tokens.hover_margin_h;
        let margin_v = tokens.hover_margin_v;
        let inner_rounding = tokens.inner_rounding;
        let icon_size = tokens.icon_size;

        let cursor_pos = ui.cursor().min;

        // Full button rect (52×38)
        let outer_rect = Rect::from_min_size(cursor_pos, Vec2::new(btn_width, btn_height));

        // Inner hover rect with margins
        let inner_rect = Rect::from_min_max(
            Pos2::new(outer_rect.min.x + margin_h, outer_rect.min.y + margin_v),
            Pos2::new(outer_rect.max.x - margin_h, outer_rect.max.y - margin_v),
        );

        // Arrow detection zone (right edge of outer rect)
        let arrow_width = margin_h + 2.0;
        let arrow_rect = Rect::from_min_max(
            Pos2::new(outer_rect.max.x - arrow_width, outer_rect.min.y),
            outer_rect.max,
        );

        // Icon zone (left part of outer rect)
        let icon_zone_rect = Rect::from_min_max(
            outer_rect.min,
            Pos2::new(outer_rect.max.x - arrow_width, outer_rect.max.y),
        );

        // Use Response-based interactions instead of manual pointer polling
        let icon_res = ui.allocate_rect(icon_zone_rect, Sense::click());
        let arrow_res = ui.allocate_rect(arrow_rect, Sense::click());

        let icon_hovered = icon_res.hovered();
        let arrow_hovered = arrow_res.hovered();
        let icon_clicked = icon_res.clicked();
        let arrow_clicked = arrow_res.clicked();

        // Combined hover state for background
        let any_hovered = icon_hovered || arrow_hovered;

        // Draw inner hover rect only on hover or active (with visibility check)
        let show_bg = any_hovered || self.is_icon_active;
        if show_bg && ui.is_rect_visible(inner_rect) {
            let bg = if self.is_icon_active && icon_hovered {
                theming::active_hover_color(ui)
            } else if self.is_icon_active {
                theming::sel_color(ui)
            } else if any_hovered {
                theming::hover_color(ui)
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(inner_rect, inner_rounding, bg);
        }

        // Draw icon centered in inner rect (with visibility check)
        let icon_rect = Rect::from_center_size(inner_rect.center(), Vec2::splat(icon_size));
        if ui.is_rect_visible(icon_rect) {
            render_svg_at_rect_themed(ui, self.icon, icon_rect, any_hovered, self.is_icon_active);
        }

        // Draw arrow at right edge (with visibility check)
        if ui.is_rect_visible(arrow_rect) {
            let arrow_stroke = Stroke::new(
                1.2,
                if arrow_hovered {
                    theming::active_icon_color(ui)
                } else {
                    theming::icon_color(ui)
                },
            );

            let arrow_center = Pos2::new(outer_rect.max.x - margin_h / 2.0, outer_rect.center().y);
            draw_arrow(
                ui,
                arrow_center,
                if self.is_expanded {
                    ArrowDirection::Up
                } else {
                    ArrowDirection::Down
                },
                arrow_stroke,
            );
        }

        // Tooltips using Response-based interactions
        icon_res.on_hover_text(self.tooltip);
        arrow_res.on_hover_text(self.arrow_tooltip);

        PairButtonResponse {
            icon_clicked,
            arrow_clicked,
            icon_hovered,
            arrow_hovered,
            icon_rect,
            arrow_rect,
        }
    }
}

/// Draw an arrow indicator at the given position
pub fn draw_arrow(ui: &mut Ui, center: Pos2, direction: ArrowDirection, stroke: Stroke) {
    let arrow_x = center.x;
    let arrow_y = center.y;

    match direction {
        ArrowDirection::Down => {
            // Arrow down: v
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x - 2.0, arrow_y - 1.0),
                    Pos2::new(arrow_x, arrow_y + 1.0),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x, arrow_y + 1.0),
                    Pos2::new(arrow_x + 2.0, arrow_y - 1.0),
                ],
                stroke,
            );
        }
        ArrowDirection::Up => {
            // Arrow up: ^
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x - 2.0, arrow_y + 1.0),
                    Pos2::new(arrow_x, arrow_y - 1.0),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x, arrow_y - 1.0),
                    Pos2::new(arrow_x + 2.0, arrow_y + 1.0),
                ],
                stroke,
            );
        }
        ArrowDirection::Right => {
            // Arrow right: >
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x - 1.0, arrow_y - 2.0),
                    Pos2::new(arrow_x + 1.0, arrow_y),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x + 1.0, arrow_y),
                    Pos2::new(arrow_x - 1.0, arrow_y + 2.0),
                ],
                stroke,
            );
        }
        ArrowDirection::Left => {
            // Arrow left: <
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x + 1.0, arrow_y - 2.0),
                    Pos2::new(arrow_x - 1.0, arrow_y),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    Pos2::new(arrow_x - 1.0, arrow_y),
                    Pos2::new(arrow_x + 1.0, arrow_y + 2.0),
                ],
                stroke,
            );
        }
    }
}
