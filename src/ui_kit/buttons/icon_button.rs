//! Icon Button - simple icon-only button
//!
//! The most common button type, used for tool selection, toggles, and actions.

use super::{ButtonSize, button_bg};
use crate::icons::Icon;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Rect, Response, Sense, Ui, Vec2};

/// Icon button with theme-aware styling
///
/// A square button containing a single icon. Supports hover, selected,
/// and disabled states with proper theme colors.
///
/// # Example
///
/// ```ignore
/// use crate::icons::icons;
/// let response = IconButton::new(&icons::TREND_LINE)
///     .size(ButtonSize::MD)
///     .tooltip("Trend Line (Alt+T)")
///     .selected(is_active)
///     .show(ui);
///
/// if response.clicked() {
///     select_tool(DrawingToolType::TrendLine);
/// }
/// ```
pub struct IconButton<'a> {
    icon: &'a Icon,
    size: ButtonSize,
    tooltip: Option<String>,
    selected: bool,
    enabled: bool,
    icon_scale: f32,
}

impl<'a> IconButton<'a> {
    /// Create a new icon button
    pub fn new(icon: &'a Icon) -> Self {
        Self {
            icon,
            size: ButtonSize::MD,
            tooltip: None,
            selected: false,
            enabled: true,
            icon_scale: 0.6, // Icon takes 60% of button size
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

    /// Set whether the button is in selected/active state
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Set whether the button is enabled
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the icon scale (0.0-1.0, default 0.6)
    #[must_use]
    pub fn icon_scale(mut self, scale: f32) -> Self {
        self.icon_scale = scale.clamp(0.1, 1.0);
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        let button_size = Vec2::splat(self.size.pixels());
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };

        let (rect, response) = ui.allocate_exact_size(button_size, sense);

        if ui.is_rect_visible(rect) {
            let hovered = response.hovered() && self.enabled;

            // Background
            let bg = button_bg(ui, hovered, self.selected);
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.button, bg);

            // Icon with theme-aware color
            let icon_color = if !self.enabled {
                ui.style()
                    .visuals
                    .widgets
                    .noninteractive
                    .fg_stroke
                    .color
                    .gamma_multiply(0.5)
            } else if self.selected {
                theming::icon_active(ui)
            } else if hovered {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };

            // Render icon centered in button
            let icon_size = self.size.pixels() * self.icon_scale;
            let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
        }

        // Tooltip

        if let Some(tip) = self.tooltip {
            response.on_hover_text(tip)
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::icons;

    #[test]
    fn test_button_creation() {
        let btn = IconButton::new(&icons::TREND_LINE)
            .size(ButtonSize::LG)
            .tooltip("Test tooltip")
            .selected(true)
            .enabled(false);

        assert!(btn.selected);
        assert!(!btn.enabled);
        assert_eq!(btn.tooltip, Some("Test tooltip".to_string()));
    }

    #[test]
    fn test_icon_scale_clamping() {
        let btn = IconButton::new(&icons::TREND_LINE).icon_scale(2.0); // Should clamp to 1.0
        assert!(btn.icon_scale <= 1.0);

        let btn = IconButton::new(&icons::TREND_LINE).icon_scale(-0.5); // Should clamp to 0.1
        assert!(btn.icon_scale >= 0.1);
    }
}
