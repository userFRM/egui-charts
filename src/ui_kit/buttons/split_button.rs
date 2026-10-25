//! Split Button - icon + dropdown arrow
//!
//! Used for category buttons that expand to show submenus.
//! Layout: the icon centred in the same square cell as an [`super::IconButton`], with a small
//! downward chevron in its bottom-right corner.

use egui::emath::GuiRounding;

use super::{ButtonSize, ButtonVariant, finish};
use crate::icons::{Icon, icons};
use crate::tokens::DESIGN_TOKENS;
use egui::{Rect, Response, Sense, Ui, Vec2, vec2};

/// Split button with icon and dropdown arrow
///
/// Used for tool categories that expand to show submenus.
///
/// # Example
///
/// ```
/// use egui_charts::icons::icons;
/// use egui_charts::ui_kit::buttons::SplitButton;
///
/// # egui::__run_test_ui(|ui| {
/// # let is_submenu_open = false;
/// let response = SplitButton::new(&icons::TREND_LINE)
///     .tooltip("Trend Lines")
///     .expanded(is_submenu_open)
///     .show(ui);
///
/// if response.clicked() {
///     // toggle the submenu
/// }
/// # });
/// ```
pub struct SplitButton {
    icon: &'static Icon,
    size: ButtonSize,
    tooltip: Option<String>,
    expanded: bool,
    selected: Option<bool>,
}

impl SplitButton {
    /// Create a new split button
    pub fn new(icon: &'static Icon) -> Self {
        Self {
            icon,
            size: ButtonSize::MD,
            tooltip: None,
            expanded: false,
            selected: None,
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
        self.selected = Some(selected);
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        let button_size = Vec2::splat(self.size.pixels());
        let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, DESIGN_TOKENS.rounding.button);

        let show_active = self.expanded || self.selected == Some(true);
        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let pressed = response.is_pointer_button_down_on();
            let variant = if show_active {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Ghost
            };
            ui.painter().rect_filled(
                rect,
                DESIGN_TOKENS.rounding.button,
                variant.bg_color(ui, hovered, pressed),
            );

            let color = if show_active {
                variant.fg_color(ui)
            } else {
                ui.style().interact(&response).fg_stroke.color
            };
            let icon_size = Vec2::splat(self.size.icon_pixels(None));
            self.icon
                .as_image_tinted(icon_size, color)
                .paint_at(ui, Rect::from_center_size(rect.center(), icon_size));

            let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_sm);
            let chevron = Rect::from_min_size(rect.right_bottom() - size + vec2(0.0, 4.0), size)
                .round_to_pixels(ui.pixels_per_point());
            icons::CHEVRON_DOWN
                .as_image_tinted(size, color)
                .paint_at(ui, chevron);
        }

        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_expanded(self.expanded));
        let tooltip = self.tooltip.as_deref();
        finish(
            response,
            self.selected,
            tooltip.unwrap_or_default(),
            tooltip,
        )
    }
}
