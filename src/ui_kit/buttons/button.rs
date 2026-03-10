//! Button - Variant-aware button widget
//!
//! A button that supports semantic variants (Primary, Secondary, Ghost, etc.)
//! providing clear visual hierarchy for different action types.

use egui::{Response, Ui, Vec2, Widget};

use super::{ButtonSize, ButtonVariant};
use crate::icons::Icon;
use crate::tokens::DESIGN_TOKENS;

/// A button widget with support for semantic variants
///
/// Button provides distinct visual variants that communicate different
/// levels of importance and action types.
///
/// # Example
///
/// ```ignore
/// use open_trading_charts::ui_kit::buttons::{Button, ButtonVariant};
/// use open_trading_charts::ui_kit::icons::icons;
///
/// // Primary button (high emphasis)
/// if Button::new("Save Changes").primary().show(ui).clicked() {
///     save();
/// }
///
/// // Ghost button with icon (low emphasis)
/// Button::icon_and_text(&icons::SETTINGS, "Settings")
///     .ghost()
///     .show(ui);
///
/// // Danger button (destructive action)
/// Button::new("Delete")
///     .danger()
///     .show(ui);
/// ```
pub struct Button<'a> {
    text: Option<String>,
    icon: Option<&'a Icon>,
    variant: ButtonVariant,
    size: ButtonSize,
    tooltip: Option<String>,
    enabled: bool,
    selected: bool,
    icon_scale: f32,
}

impl<'a> Button<'a> {
    /// Create a new button with text
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            icon: None,
            variant: ButtonVariant::Ghost,
            size: ButtonSize::MD,
            tooltip: None,
            enabled: true,
            selected: false,
            icon_scale: 0.6,
        }
    }

    /// Create a new button with only an icon
    pub fn icon(icon: &'a Icon) -> Self {
        Self {
            text: None,
            icon: Some(icon),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::MD,
            tooltip: None,
            enabled: true,
            selected: false,
            icon_scale: 0.6,
        }
    }

    /// Create a new button with both icon and text
    pub fn icon_and_text(icon: &'a Icon, text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            icon: Some(icon),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::MD,
            tooltip: None,
            enabled: true,
            selected: false,
            icon_scale: 0.6,
        }
    }

    /// Set the button variant
    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Make this a primary button (high emphasis)
    #[must_use]
    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }

    /// Make this a secondary button (medium emphasis)
    #[must_use]
    pub fn secondary(mut self) -> Self {
        self.variant = ButtonVariant::Secondary;
        self
    }

    /// Make this a ghost button (low emphasis, transparent)
    #[must_use]
    pub fn ghost(mut self) -> Self {
        self.variant = ButtonVariant::Ghost;
        self
    }

    /// Make this an outlined button (border stroke)
    #[must_use]
    pub fn outlined(mut self) -> Self {
        self.variant = ButtonVariant::Outlined;
        self
    }

    /// Make this a danger button (destructive action)
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.variant = ButtonVariant::Danger;
        self
    }

    /// Set the button size
    #[must_use]
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Make this a small button
    #[must_use]
    pub fn small(mut self) -> Self {
        self.size = ButtonSize::SM;
        self
    }

    /// Make this a large button
    #[must_use]
    pub fn large(mut self) -> Self {
        self.size = ButtonSize::LG;
        self
    }

    /// Set the tooltip text
    #[must_use]
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Set whether the button is enabled
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set whether the button appears selected/active
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Set the icon scale relative to button size (0.0-1.0)
    #[must_use]
    pub fn icon_scale(mut self, scale: f32) -> Self {
        self.icon_scale = scale.clamp(0.1, 1.0);
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        ui.add(self)
    }
}

impl<'a> Widget for Button<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let tokens = &DESIGN_TOKENS;

        // Calculate button size
        let button_height = self.size.pixels();
        let icon_size = button_height * self.icon_scale;

        // Calculate width based on content
        // Use approximate character width based on font size
        let font_id = egui::TextStyle::Button.resolve(ui.style());
        let char_width = font_id.size * 0.55; // Approximate character width

        let width = if self.text.is_some() && self.icon.is_some() {
            // Icon + gap + text + padding
            let text_width = self
                .text
                .as_ref()
                .map(|t| char_width * t.len() as f32)
                .unwrap_or(0.0);
            icon_size + tokens.spacing.sm + text_width + tokens.spacing.lg * 2.0
        } else if self.text.is_some() {
            // Text only
            let text_width = self
                .text
                .as_ref()
                .map(|t| char_width * t.len() as f32)
                .unwrap_or(0.0);
            text_width + tokens.spacing.lg * 2.0
        } else {
            // Icon only - square button
            button_height
        };

        let button_size = Vec2::new(width.max(button_height), button_height);

        let sense = if self.enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };

        let (rect, response) = ui.allocate_exact_size(button_size, sense);

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().visuals.clone();
            let hovered = response.hovered() && self.enabled;
            let active = response.is_pointer_button_down_on();

            // Determine colors based on variant and state
            let (bg_color, fg_color) = if !self.enabled {
                (
                    visuals.widgets.noninteractive.bg_fill,
                    visuals
                        .widgets
                        .noninteractive
                        .fg_stroke
                        .color
                        .gamma_multiply(0.5),
                )
            } else if self.selected {
                (visuals.selection.bg_fill, visuals.selection.stroke.color)
            } else {
                let bg = self.variant.bg_color(hovered, active);
                let fg = self.variant.fg_color();
                let fg = if fg == egui::Color32::PLACEHOLDER {
                    if hovered {
                        visuals.widgets.hovered.fg_stroke.color
                    } else {
                        visuals.widgets.inactive.fg_stroke.color
                    }
                } else {
                    fg
                };
                (bg, fg)
            };

            // Draw background
            let rounding = tokens.rounding.button;

            // For ghost variant with hover, show subtle background
            let actual_bg = if self.variant == ButtonVariant::Ghost && hovered && !self.selected {
                visuals.widgets.hovered.bg_fill
            } else if bg_color == egui::Color32::TRANSPARENT && self.selected {
                visuals.selection.bg_fill
            } else {
                bg_color
            };

            if actual_bg != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, rounding, actual_bg);
            }

            // Draw border for outlined variant
            if let Some(border_color) = self.variant.border_color(hovered) {
                ui.painter().rect_stroke(
                    rect,
                    rounding,
                    egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
                    egui::epaint::StrokeKind::Inside,
                );
            }

            // Draw content
            let content_rect = rect.shrink(tokens.spacing.sm);

            if let Some(icon) = self.icon {
                let icon_rect = if self.text.is_some() {
                    // Icon on the left
                    egui::Rect::from_min_size(
                        egui::pos2(
                            content_rect.min.x,
                            content_rect.center().y - icon_size / 2.0,
                        ),
                        Vec2::splat(icon_size),
                    )
                } else {
                    // Icon centered
                    egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size))
                };

                icon.as_image_tinted(Vec2::splat(icon_size), fg_color)
                    .paint_at(ui, icon_rect);
            }

            if let Some(text) = &self.text {
                let text_pos = if self.icon.is_some() {
                    // Text after icon
                    egui::pos2(
                        content_rect.min.x + icon_size + tokens.spacing.sm,
                        rect.center().y,
                    )
                } else {
                    // Text centered
                    rect.center()
                };

                let align = if self.icon.is_some() {
                    egui::Align2::LEFT_CENTER
                } else {
                    egui::Align2::CENTER_CENTER
                };

                ui.painter().text(
                    text_pos,
                    align,
                    text,
                    egui::TextStyle::Button.resolve(ui.style()),
                    fg_color,
                );
            }
        }

        // Add tooltip

        if let Some(tip) = self.tooltip {
            response.on_hover_text(tip)
        } else {
            response
        }
    }
}

/// Extension trait for quick button creation from strings
pub trait ButtonExt {
    /// Create a primary Button
    fn primary_button(self) -> Button<'static>;

    /// Create a secondary Button
    fn secondary_button(self) -> Button<'static>;

    /// Create an outlined Button
    fn outlined_button(self) -> Button<'static>;

    /// Create a danger Button
    fn danger_button(self) -> Button<'static>;
}

impl ButtonExt for &str {
    fn primary_button(self) -> Button<'static> {
        Button::new(self).primary()
    }

    fn secondary_button(self) -> Button<'static> {
        Button::new(self).secondary()
    }

    fn outlined_button(self) -> Button<'static> {
        Button::new(self).outlined()
    }

    fn danger_button(self) -> Button<'static> {
        Button::new(self).danger()
    }
}

impl ButtonExt for String {
    fn primary_button(self) -> Button<'static> {
        Button::new(self).primary()
    }

    fn secondary_button(self) -> Button<'static> {
        Button::new(self).secondary()
    }

    fn outlined_button(self) -> Button<'static> {
        Button::new(self).outlined()
    }

    fn danger_button(self) -> Button<'static> {
        Button::new(self).danger()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_creation() {
        let btn = Button::new("Test")
            .primary()
            .size(ButtonSize::LG)
            .tooltip("Test tooltip")
            .enabled(false);

        assert_eq!(btn.variant, ButtonVariant::Primary);
        assert_eq!(btn.size, ButtonSize::LG);
        assert_eq!(btn.tooltip, Some("Test tooltip".to_string()));
        assert!(!btn.enabled);
    }

    #[test]
    fn test_variant_shortcuts() {
        assert_eq!(
            Button::new("Test").primary().variant,
            ButtonVariant::Primary
        );
        assert_eq!(
            Button::new("Test").secondary().variant,
            ButtonVariant::Secondary
        );
        assert_eq!(Button::new("Test").ghost().variant, ButtonVariant::Ghost);
        assert_eq!(
            Button::new("Test").outlined().variant,
            ButtonVariant::Outlined
        );
        assert_eq!(Button::new("Test").danger().variant, ButtonVariant::Danger);
    }

    #[test]
    fn test_size_shortcuts() {
        assert_eq!(Button::new("Test").small().size, ButtonSize::SM);
        assert_eq!(Button::new("Test").large().size, ButtonSize::LG);
    }
}
