//! Icon buttons, split buttons and button styling helpers.
//!
//! Use [`IconButton`] for an individual action and [`SplitButton`] for an action
//! with a separate dropdown trigger.

use crate::localization as l10n;
mod button;
mod icon_button;
mod split_button;
mod variant;

pub use button::{Button, ButtonExt};
pub use icon_button::IconButton;
pub use split_button::SplitButton;
pub use variant::ButtonVariant;

use crate::tokens::DESIGN_TOKENS;
use egui::{Response, Role, Ui, WidgetInfo};

/// Button sizes
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ButtonSize {
    /// Small buttons.
    SM,
    /// Medium toolbar buttons.
    #[default]
    MD,
    /// Large buttons.
    LG,
    /// Extra-large buttons.
    XL,
}

impl ButtonSize {
    /// Side length in egui logical points.
    #[inline]
    pub fn pixels(&self) -> f32 {
        match self {
            ButtonSize::SM => DESIGN_TOKENS.sizing.button_sm,
            ButtonSize::MD => DESIGN_TOKENS.sizing.button_md,
            ButtonSize::LG => DESIGN_TOKENS.sizing.button_lg,
            ButtonSize::XL => DESIGN_TOKENS.sizing.button_xl,
        }
    }

    /// Icon size for this button size, or `scale` times the button size when one is set.
    fn icon_pixels(&self, scale: Option<f32>) -> f32 {
        scale.map_or(
            match self {
                ButtonSize::SM => DESIGN_TOKENS.sizing.icon_sm,
                ButtonSize::MD => DESIGN_TOKENS.sizing.icon_md,
                ButtonSize::LG | ButtonSize::XL => DESIGN_TOKENS.sizing.icon_lg,
            },
            |scale| self.pixels() * scale,
        )
    }
}

/// Report a button to accessibility tools and attach its tooltip, which also shows while disabled.
fn finish(
    response: Response,
    selected: Option<bool>,
    label: &str,
    tooltip: Option<&str>,
) -> Response {
    let translated_label = l10n::text(&response.ctx, label);
    response.widget_info(|| {
        if let Some(selected) = selected {
            WidgetInfo::selected(
                Role::Button,
                response.enabled(),
                selected,
                &translated_label,
            )
        } else {
            WidgetInfo::labeled(Role::Button, response.enabled(), &translated_label)
        }
    });
    match tooltip {
        Some(tip) => {
            let tip = l10n::text(&response.ctx, tip);
            response.on_hover_text(&tip).on_disabled_hover_text(&tip)
        }
        None => response,
    }
}

/// Render a dialog close button: an [`IconButton`] with the close icon.
///
/// Returns the click [`Response`].
///
/// # Example
///
/// ```
/// use egui_charts::tokens::DESIGN_TOKENS;
/// use egui_charts::ui_kit::buttons::dialog_close_button;
///
/// # egui::__run_test_ui(|ui| {
/// ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
///     ui.add_space(DESIGN_TOKENS.spacing.sm);
///     if dialog_close_button(ui).clicked() {
///         // close the dialog
///     }
/// });
/// # });
/// ```
pub fn dialog_close_button(ui: &mut Ui) -> Response {
    IconButton::new(&crate::icons::icons::CLOSE)
        .tooltip("Close")
        .show(ui)
}

/// The kit disclosure icon for egui combo boxes.
pub(crate) fn combo_chevron(
    ui: &Ui,
    rect: egui::Rect,
    visuals: &egui::style::WidgetVisuals,
    _open: bool,
) {
    use egui::emath::GuiRounding;
    let rect = egui::Rect::from_center_size(
        rect.center(),
        egui::Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
    )
    .round_to_pixels(ui.pixels_per_point());
    crate::icons::icons::CHEVRON_DOWN
        .as_image_tinted(rect.size(), visuals.fg_stroke.color)
        .paint_at(ui, rect);
}
