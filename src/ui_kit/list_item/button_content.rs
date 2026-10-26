//! ButtonContent - Clickable button content for list items

use crate::localization as l10n;
use egui::{Color32, Response, Role, Ui, Vec2, WidgetInfo};

use crate::icons::Icon;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::ButtonVariant;

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
/// ```
/// use egui_charts::icons::icons;
/// use egui_charts::ui_kit::list_item::{ButtonContent, ListItem};
///
/// # egui::__run_test_ui(|ui| {
/// let response = ListItem::new()
///     .show(ui, ButtonContent::new(&icons::PLUS, "Add Item"));
///
/// if response.clicked() {
///     // add the item
/// }
/// # });
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

        if self.disabled || !ui.is_enabled() {
            let text = ui.active_theme().ui.text_disabled;
            return (Color32::TRANSPARENT, text, text);
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
            ButtonStyle::Primary | ButtonStyle::Danger => {
                let variant = if self.style == ButtonStyle::Primary {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Danger
                };
                if active || hovered {
                    let text = variant.fg_color(ui);
                    (variant.bg_color(ui, hovered, active), text, text)
                } else {
                    let text = if self.style == ButtonStyle::Danger {
                        ui.active_theme().ui.error
                    } else {
                        visuals.text_color()
                    };
                    (Color32::TRANSPARENT, text, text)
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
        let response = context.response;
        let translated_label = l10n::text(ui.ctx(), self.label.as_deref().unwrap_or_default());
        response.widget_info(|| {
            WidgetInfo::labeled(
                Role::Button,
                !self.disabled && ui.is_enabled(),
                &translated_label,
            )
        });

        if ui.is_rect_visible(response.rect) {
            let (bg_color, icon_color, text_color) =
                self.get_colors(ui, response.hovered(), response.is_pointer_button_down_on());

            // The fill covers the whole row, like the list item's own hover fill.
            if bg_color != Color32::TRANSPARENT {
                ui.painter()
                    .rect_filled(response.rect, DESIGN_TOKENS.rounding.sm, bg_color);
            }

            let content_rect = context.rect;

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
                    l10n::text(ui.ctx(), label),
                    egui::TextStyle::Body.resolve(ui.style()),
                    text_color,
                );
            }
        }

        response.clone()
    }

    fn interactive(&self) -> bool {
        !self.disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::icons;
    use crate::theme::{apply_to_egui, contrast, presets};

    #[test]
    fn action_rows_are_transparent_at_rest_and_use_readable_state_colours() {
        for (name, theme) in presets() {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                for style in [
                    ButtonStyle::Default,
                    ButtonStyle::Primary,
                    ButtonStyle::Danger,
                    ButtonStyle::Subtle,
                ] {
                    let row = ButtonContent::new(&icons::PLUS, "Action").style(style);
                    let (fill, icon, text) = row.get_colors(ui, false, false);
                    assert_eq!(fill, Color32::TRANSPARENT, "{name}: {style:?}");
                    assert_eq!(icon, text);
                    assert!(
                        contrast(text, theme.ui.panel_bg) >= 4.5,
                        "{name}: {style:?}"
                    );
                    let disabled = row.disabled(true).get_colors(ui, true, true);
                    assert_eq!(
                        disabled,
                        (
                            Color32::TRANSPARENT,
                            theme.ui.text_disabled,
                            theme.ui.text_disabled
                        )
                    );
                }
                for (row, hover, active, text) in [
                    (
                        ButtonContent::new(&icons::PLUS, "Add").primary(),
                        theme.ui.accent_hover,
                        theme.ui.accent_active,
                        theme.ui.accent_text,
                    ),
                    (
                        ButtonContent::new(&icons::CLOSE, "Delete").danger(),
                        theme.ui.danger_bg_hover,
                        theme.ui.danger_bg_active,
                        theme.ui.danger_text,
                    ),
                ] {
                    assert_eq!(row.get_colors(ui, true, false), (hover, text, text));
                    assert_eq!(row.get_colors(ui, true, true), (active, text, text));
                }
            })
            .drop_without_applying_deltas();
        }
    }
}
