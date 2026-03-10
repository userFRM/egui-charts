//! ButtonContent - Clickable button content for list items

use egui::{Color32, Response, Sense, Ui, Vec2};

use crate::icons::Icon;
use crate::tokens::DESIGN_TOKENS;

use super::content::{ContentContext, ListItemContent};

/// Button style variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    /// Standard button appearance
    #[default]
    Default,
    /// Primary/accent color button
    Primary,
    /// Danger/destructive action button (red)
    Danger,
    /// Subtle/ghost button (minimal background)
    Subtle,
}

/// Clickable button content for list items
///
/// Use this when you need an actionable button row in a list,
/// such as "Add new item", "Clear all", or action buttons.
///
/// # Example
///
/// ```ignore
/// use crate::icons::icons;
/// use crate::ui_kit::list_item::{ListItem, ButtonContent};
///
/// let response = ListItem::new()
///     .show(ui, ButtonContent::new(&icons::ADD, "Add Item"));
///
/// if response.clicked() {
///     self.add_item();
/// }
/// ```
pub struct ButtonContent<'a> {
    icon: &'a Icon,
    label: Option<String>,
    style: ButtonStyle,
    icon_size: f32,
    disabled: bool,
}

impl<'a> ButtonContent<'a> {
    /// Create new button content with an icon
    pub fn new(icon: &'a Icon, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: Some(label.into()),
            style: ButtonStyle::Default,
            icon_size: DESIGN_TOKENS.sizing.icon_sm,
            disabled: false,
        }
    }

    /// Create button content with only an icon (no label)
    pub fn icon_only(icon: &'a Icon) -> Self {
        Self {
            icon,
            label: None,
            style: ButtonStyle::Default,
            icon_size: DESIGN_TOKENS.sizing.icon_sm,
            disabled: false,
        }
    }

    /// Set the button style
    #[must_use]
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Make this a primary/accent button
    #[must_use]
    pub fn primary(mut self) -> Self {
        self.style = ButtonStyle::Primary;
        self
    }

    /// Make this a danger/destructive button
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.style = ButtonStyle::Danger;
        self
    }

    /// Make this a subtle/ghost button
    #[must_use]
    pub fn subtle(mut self) -> Self {
        self.style = ButtonStyle::Subtle;
        self
    }

    /// Set the icon size
    #[must_use]
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }

    /// Disable the button
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Get colors based on style and state
    fn get_colors(&self, ui: &Ui, hovered: bool, active: bool) -> (Color32, Color32, Color32) {
        let visuals = &ui.style().visuals;

        if self.disabled {
            return (
                visuals.widgets.noninteractive.bg_fill,
                visuals
                    .widgets
                    .noninteractive
                    .fg_stroke
                    .color
                    .gamma_multiply(0.5),
                visuals
                    .widgets
                    .noninteractive
                    .fg_stroke
                    .color
                    .gamma_multiply(0.5),
            );
        }

        match self.style {
            ButtonStyle::Default => {
                if active {
                    (
                        visuals.widgets.active.bg_fill,
                        visuals.widgets.active.fg_stroke.color,
                        visuals.widgets.active.fg_stroke.color,
                    )
                } else if hovered {
                    (
                        visuals.widgets.hovered.bg_fill,
                        visuals.widgets.hovered.fg_stroke.color,
                        visuals.widgets.hovered.fg_stroke.color,
                    )
                } else {
                    (
                        Color32::TRANSPARENT,
                        visuals.widgets.inactive.fg_stroke.color,
                        visuals.widgets.inactive.fg_stroke.color,
                    )
                }
            }
            ButtonStyle::Primary => {
                let accent = DESIGN_TOKENS.semantic.brand.accent;
                let text = DESIGN_TOKENS.semantic.ui.text_light;
                if active {
                    (accent.gamma_multiply(0.8), text, text)
                } else if hovered {
                    (accent.gamma_multiply(0.9), text, text)
                } else {
                    (accent, text, text)
                }
            }
            ButtonStyle::Danger => {
                let danger = DESIGN_TOKENS.semantic.chart.bearish;
                let text = DESIGN_TOKENS.semantic.ui.text_light;
                if active {
                    (danger.gamma_multiply(0.8), text, text)
                } else if hovered {
                    (danger.gamma_multiply(0.9), text, text)
                } else {
                    (danger, text, text)
                }
            }
            ButtonStyle::Subtle => {
                if active {
                    (
                        visuals.widgets.active.bg_fill.gamma_multiply(0.5),
                        visuals.widgets.active.fg_stroke.color,
                        visuals.widgets.active.fg_stroke.color,
                    )
                } else if hovered {
                    (
                        visuals.widgets.hovered.bg_fill.gamma_multiply(0.5),
                        visuals.widgets.hovered.fg_stroke.color,
                        visuals.widgets.hovered.fg_stroke.color,
                    )
                } else {
                    (
                        Color32::TRANSPARENT,
                        visuals.widgets.noninteractive.fg_stroke.color,
                        visuals.widgets.noninteractive.fg_stroke.color,
                    )
                }
            }
        }
    }
}

impl<'a> ListItemContent for ButtonContent<'a> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> Response {
        let sense = if self.disabled {
            Sense::hover()
        } else {
            Sense::click()
        };

        let (rect, response) = ui.allocate_exact_size(context.rect.size(), sense);

        if ui.is_rect_visible(rect) {
            let (bg_color, icon_color, text_color) =
                self.get_colors(ui, response.hovered(), response.is_pointer_button_down_on());

            // Draw background with rounded corners for Primary/Danger styles
            let rounding = match self.style {
                ButtonStyle::Primary | ButtonStyle::Danger => DESIGN_TOKENS.rounding.sm,
                _ => 0.0,
            };

            let button_rect = rect.shrink2(Vec2::new(
                DESIGN_TOKENS.spacing.xs,
                DESIGN_TOKENS.spacing.xs,
            ));

            if bg_color != Color32::TRANSPARENT {
                ui.painter().rect_filled(button_rect, rounding, bg_color);
            }

            // Calculate content layout
            let content_rect = button_rect.shrink2(Vec2::new(DESIGN_TOKENS.spacing.sm, 0.0));

            // Draw icon
            let icon_rect = egui::Rect::from_min_size(
                content_rect.min + Vec2::new(0.0, (content_rect.height() - self.icon_size) / 2.0),
                Vec2::splat(self.icon_size),
            );
            self.icon
                .as_image_tinted(Vec2::splat(self.icon_size), icon_color)
                .paint_at(ui, icon_rect);

            // Draw label if present
            if let Some(label) = &self.label {
                let text_pos = egui::pos2(
                    content_rect.min.x + self.icon_size + DESIGN_TOKENS.spacing.sm,
                    content_rect.center().y,
                );

                ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::TextStyle::Body.resolve(ui.style()),
                    text_color,
                );
            }
        }

        response
    }
}
