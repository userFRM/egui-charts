//! Button - Variant-aware button widget
//!
//! A button that supports semantic variants (Primary, Secondary, Ghost, etc.)
//! providing clear visual hierarchy for different action types.

use crate::localization as l10n;
use egui::{Color32, Rect, Response, Sense, TextStyle, Ui, Vec2, Widget, pos2};

use super::{ButtonSize, ButtonVariant, finish};
use crate::icons::Icon;
use crate::tokens::DESIGN_TOKENS;

/// A button widget with support for semantic variants
///
/// Button provides distinct visual variants that communicate different
/// levels of importance and action types.
///
/// # Example
///
/// ```
/// use egui_charts::icons::icons;
/// use egui_charts::ui_kit::buttons::Button;
///
/// # egui::__run_test_ui(|ui| {
/// // Primary button (high emphasis)
/// if Button::new("Save Changes").primary().show(ui).clicked() {
///     // save
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
/// # });
/// ```
pub struct Button<'a> {
    text: Option<String>,
    icon: Option<&'a Icon>,
    variant: ButtonVariant,
    size: ButtonSize,
    tooltip: Option<String>,
    enabled: bool,
    selected: Option<bool>,
    icon_scale: Option<f32>,
    text_color: Option<Color32>,
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
            selected: None,
            icon_scale: None,
            text_color: None,
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
            selected: None,
            icon_scale: None,
            text_color: None,
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
            selected: None,
            icon_scale: None,
            text_color: None,
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

    /// Override only the label colour, retaining the variant's fill and icon colour.
    #[must_use]
    pub fn text_color(mut self, color: Color32) -> Self {
        self.text_color = Some(color);
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
        self.selected = Some(selected);
        self
    }

    /// Set the icon size as a fraction of the button height (0.1-1.0).
    ///
    /// By default the icon is 16, 20 or 24 px for small, medium and larger buttons.
    #[must_use]
    pub fn icon_scale(mut self, scale: f32) -> Self {
        self.icon_scale = Some(scale.clamp(0.1, 1.0));
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        ui.add(self)
    }
}

impl<'a> Widget for Button<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        if !self.enabled {
            // `enabled(false)` looks exactly like a button inside `add_enabled_ui(false)`.
            return ui.add_enabled(
                false,
                Self {
                    enabled: true,
                    ..self
                },
            );
        }

        let tokens = &DESIGN_TOKENS;
        let height = self.size.pixels();
        let icon_size = self.size.icon_pixels(self.icon_scale);
        let galley = self.text.as_ref().map(|text| {
            let font = TextStyle::Button.resolve(ui.style());
            ui.painter().layout_no_wrap(
                l10n::text(ui.ctx(), text.clone()),
                font,
                Color32::PLACEHOLDER,
            )
        });

        // Content is laid out text, so the padding is the same for every label.
        let (padding, gap) = match (self.icon.is_some(), galley.is_some()) {
            (true, true) => (tokens.spacing.xl, tokens.spacing.sm),
            (false, true) => (tokens.spacing.xl, 0.0),
            _ => (0.0, 0.0),
        };
        let content_width = self.icon.map_or(0.0, |_| icon_size)
            + gap
            + galley.as_ref().map_or(0.0, |galley| galley.size().x);
        let width = (content_width + 2.0 * padding).max(height);

        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, DESIGN_TOKENS.rounding.button);

        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let pressed = response.is_pointer_button_down_on();
            let variant = if self.selected == Some(true) {
                ButtonVariant::Primary
            } else {
                self.variant
            };
            let fg = if variant.is_inverse() {
                variant.fg_color(ui)
            } else {
                ui.style().interact(&response).fg_stroke.color
            };
            let rounding = tokens.rounding.button;

            ui.painter()
                .rect_filled(rect, rounding, variant.bg_color(ui, hovered, pressed));
            if let Some(border) = variant.border_color(ui) {
                ui.painter().rect_stroke(
                    rect,
                    rounding,
                    egui::Stroke::new(tokens.stroke.hairline, border),
                    egui::epaint::StrokeKind::Inside,
                );
            }

            // Centred, so a label narrower than a square button still sits in the middle.
            let mut x = rect.center().x - content_width / 2.0;
            if let Some(icon) = self.icon {
                let icon_rect = Rect::from_min_size(
                    pos2(x, rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                );
                icon.as_image_tinted(Vec2::splat(icon_size), fg)
                    .paint_at(ui, icon_rect);
                x += icon_size + gap;
            }
            if let Some(galley) = galley {
                let pos = pos2(x, rect.center().y - galley.size().y / 2.0);
                ui.painter()
                    .galley(pos, galley, self.text_color.unwrap_or(fg));
            }
        }

        let label = self.text.as_deref().or(self.tooltip.as_deref());
        finish(
            response,
            self.selected,
            label.unwrap_or_default(),
            self.tooltip.as_deref(),
        )
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
    fn width_is_the_laid_out_label_plus_padding() {
        // Multi-byte characters: the width must follow the rendered text, not the byte count.
        let label = "Größe ändern";
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let font = egui::TextStyle::Button.resolve(ui.style());
                let text_width = ui
                    .painter()
                    .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
                    .size()
                    .x;
                let width = Button::new(label).show(ui).rect.width();
                assert_eq!(width, text_width + 2.0 * DESIGN_TOKENS.spacing.xl);
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn focus_ring_follows_the_button_corners_and_stays_within_two_pixels() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut id = egui::Id::NULL;
            ctx.run_ui(Default::default(), |ui| {
                id = Button::new("Focus").show(ui).id;
            })
            .drop_without_applying_deltas();
            ctx.memory_mut(|m| m.request_focus(id));
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run_ui(Default::default(), |ui| {
                rect = Button::new("Focus").show(ui).rect;
            });
            let rings: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.stroke.width == DESIGN_TOKENS.stroke.thick => {
                        Some(r.clone())
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(rings.len(), 1);
            assert_eq!(rings[0].rect, rect.expand(2.0));
            assert_eq!(rings[0].stroke_kind, egui::epaint::StrokeKind::Inside);
            assert_eq!(
                rings[0].corner_radius,
                egui::CornerRadius::from(DESIGN_TOKENS.rounding.button + 2.0)
            );
        }
    }
}
