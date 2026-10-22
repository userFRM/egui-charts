//! Icon Button - simple icon-only button
//!
//! The most common button type, used for tool selection, toggles, and actions.

use super::{ButtonSize, ButtonVariant, finish};
use crate::icons::Icon;
use crate::tokens::DESIGN_TOKENS;
use egui::{Rect, Response, Sense, Ui, Vec2};

/// Icon button with theme-aware styling
///
/// A square button containing a single icon. Supports hover, pressed, selected
/// and disabled states with proper theme colors; a selected button is filled with the accent.
///
/// # Example
///
/// ```
/// use egui_charts::icons::icons;
/// use egui_charts::ui_kit::buttons::{ButtonSize, IconButton};
///
/// # egui::__run_test_ui(|ui| {
/// # let is_active = false;
/// let response = IconButton::new(&icons::TREND_LINE)
///     .size(ButtonSize::MD)
///     .tooltip("Trend Line (Alt+T)")
///     .selected(is_active)
///     .show(ui);
///
/// if response.clicked() {
///     // select the trend line tool
/// }
/// # });
/// ```
pub struct IconButton<'a> {
    icon: &'a Icon,
    size: ButtonSize,
    tooltip: Option<String>,
    selected: bool,
    enabled: bool,
    icon_scale: Option<f32>,
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
            icon_scale: None,
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

    /// Set the icon size as a fraction of the button size (0.1-1.0).
    ///
    /// By default the icon is 16, 20 or 24 px for small, medium and larger buttons.
    #[must_use]
    pub fn icon_scale(mut self, scale: f32) -> Self {
        self.icon_scale = Some(scale.clamp(0.1, 1.0));
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        if !self.enabled {
            // `enabled(false)` looks exactly like a button inside `add_enabled_ui(false)`.
            return ui.add_enabled(false, |ui: &mut Ui| {
                Self {
                    enabled: true,
                    ..self
                }
                .show(ui)
            });
        }

        let button_size = Vec2::splat(self.size.pixels());
        let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response);

        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let pressed = response.is_pointer_button_down_on();
            let variant = if self.selected {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Ghost
            };
            ui.painter().rect_filled(
                rect,
                DESIGN_TOKENS.rounding.button,
                variant.bg_color(ui, hovered, pressed),
            );

            let icon_color = if self.selected {
                variant.fg_color(ui)
            } else if hovered || pressed {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().widgets.noninteractive.fg_stroke.color
            };
            let icon_size = Vec2::splat(self.size.icon_pixels(self.icon_scale));
            self.icon
                .as_image_tinted(icon_size, icon_color)
                .paint_at(ui, Rect::from_center_size(rect.center(), icon_size));
        }

        let tooltip = self.tooltip.as_deref();
        finish(
            response,
            self.selected,
            tooltip.unwrap_or_default(),
            tooltip,
        )
    }
}
