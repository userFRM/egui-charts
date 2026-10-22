//! Unified button components for open-trading-charts-rs
//!
//! Consolidates scattered button implementations into a consistent API.
//!
//! # Components
//!
//! - [`IconButton`] - Simple icon button (most common)
//! - [`SplitButton`] - Icon + dropdown arrow (category buttons)
//! - [`Button`] - Text, icon or both, in a [`ButtonVariant`]
//!
//! # Usage
//!
//! ```
//! use egui_charts::icons::icons;
//! use egui_charts::ui_kit::buttons::IconButton;
//!
//! # egui::__run_test_ui(|ui| {
//! # let is_selected = false;
//! // Simple icon button
//! let response = IconButton::new(&icons::TREND_LINE)
//!     .tooltip("Trend Line (Alt+T)")
//!     .selected(is_selected)
//!     .show(ui);
//!
//! if response.clicked() {
//!     // handle click
//! }
//! # });
//! ```

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
    /// 24x24 - small buttons
    SM,
    /// 28x28 - standard toolbar buttons
    #[default]
    MD,
    /// 36x36 - large buttons
    LG,
    /// 40x40 - extra large
    XL,
}

impl ButtonSize {
    /// Get the pixel size
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
fn finish(response: Response, selected: bool, label: &str, tooltip: Option<&str>) -> Response {
    response.widget_info(|| {
        if selected {
            WidgetInfo::selected(Role::Button, response.enabled(), true, label)
        } else {
            WidgetInfo::labeled(Role::Button, response.enabled(), label)
        }
    });
    match tooltip {
        Some(tip) => response.on_hover_text(tip).on_disabled_hover_text(tip),
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
