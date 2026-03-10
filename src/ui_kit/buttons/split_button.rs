//! Split Button - icon + dropdown arrow
//!
//! Used for category buttons that expand to show submenus.
//! Layout: [icon] [arrow]

use super::{ButtonSize, button_bg};
use crate::icons::Icon;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Split button with icon and dropdown arrow
///
/// Used for tool categories that expand to show submenus.
///
/// # Example
///
/// ```ignore
/// use crate::icons::icons;
/// let response = SplitButton::new(&icons::LINES)
///     .tooltip("Trend Lines")
///     .expanded(is_submenu_open)
///     .show(ui);
///
/// if response.clicked() {
///     toggle_submenu();
/// }
/// ```
pub struct SplitButton {
    icon: &'static Icon,
    size: ButtonSize,
    tooltip: Option<String>,
    expanded: bool,
    selected: bool,
}

impl SplitButton {
    /// Create a new split button
    pub fn new(icon: &'static Icon) -> Self {
        Self {
            icon,
            size: ButtonSize::MD,
            tooltip: None,
            expanded: false,
            selected: false,
        }
    }

    /// Set the button size
    #[must_use]
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Set the tooltip text
    #[must_use]
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Set whether the dropdown is expanded
    #[must_use]
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Set whether a tool in this category is selected
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        let button_height = self.size.pixels();
        let arrow_width = 10.0;
        let button_width = button_height + arrow_width;
        let button_size = Vec2::new(button_width, button_height);

        let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());

        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let show_active = self.expanded || self.selected;

            // Background
            let bg = button_bg(ui, hovered, show_active);
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.button, bg);

            // Get theme-aware icon color
            let icon_col = if show_active {
                theming::icon_active(ui)
            } else if hovered {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };

            // Icon (left side)
            let icon_size = button_height * 0.6;
            let icon_center = Pos2::new(rect.left() + button_height / 2.0, rect.center().y);
            let icon_rect = Rect::from_center_size(icon_center, Vec2::splat(icon_size));
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_col)
                .paint_at(ui, icon_rect);

            // Arrow (right side)
            let arrow_x = rect.right() - arrow_width / 2.0;
            let arrow_y = rect.center().y;
            self.draw_arrow(ui, Pos2::new(arrow_x, arrow_y), icon_col);
        }

        // Tooltip

        if let Some(tip) = self.tooltip {
            response.on_hover_text(tip)
        } else {
            response
        }
    }

    /// Draw the dropdown arrow
    fn draw_arrow(&self, ui: &Ui, center: Pos2, color: Color32) {
        let arrow_size = 3.0;
        let painter = ui.painter();

        // Triangle pointing down (or right if expanded)
        if self.expanded {
            // Right-pointing arrow when expanded
            let points = [
                Pos2::new(center.x - arrow_size / 2.0, center.y - arrow_size),
                Pos2::new(center.x - arrow_size / 2.0, center.y + arrow_size),
                Pos2::new(center.x + arrow_size / 2.0, center.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                Stroke::NONE,
            ));
        } else {
            // Down-pointing arrow
            let points = [
                Pos2::new(center.x - arrow_size, center.y - arrow_size / 2.0),
                Pos2::new(center.x + arrow_size, center.y - arrow_size / 2.0),
                Pos2::new(center.x, center.y + arrow_size / 2.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                Stroke::NONE,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::icons;

    #[test]
    fn test_split_button_creation() {
        let btn = SplitButton::new(&icons::TREND_LINE)
            .size(ButtonSize::LG)
            .tooltip("Category")
            .expanded(true)
            .selected(true);

        assert!(btn.expanded);
        assert!(btn.selected);
    }
}
