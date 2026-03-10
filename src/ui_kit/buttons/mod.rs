//! Unified button components for open-trading-charts-rs
//!
//! Consolidates scattered button implementations into a consistent API.
//!
//! # Components
//!
//! - [`IconButton`] - Simple icon button (most common)
//! - [`SplitButton`] - Icon + dropdown arrow (category buttons)
//! - Toggle button - On/off toggle with icon
//!
//! # Usage
//!
//! ```ignore
//! use egui_open_trading_charts_rs::ui_kit::buttons::IconButton;
//!
//! // Simple icon button
//! let response = IconButton::new(icon)
//!     .tooltip("Trend Line (Alt+T)")
//!     .selected(is_selected)
//!     .show(ui);
//!
//! if response.clicked() {
//!     // handle click
//! }
//! ```

mod button;
mod icon_button;
mod split_button;
mod variant;

pub use button::{Button, ButtonExt};
pub use icon_button::IconButton;
pub use split_button::SplitButton;
pub use variant::ButtonVariant;

use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Response, Sense, Ui, Vec2};

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
}

/// Get button background color based on state
pub fn button_bg(ui: &Ui, hovered: bool, selected: bool) -> Color32 {
    if selected && hovered {
        // Use theming helper for active+hover state
        theming::active_hover_color(ui)
    } else if selected {
        theming::sel_color(ui)
    } else if hovered {
        theming::hover_color(ui)
    } else {
        Color32::TRANSPARENT
    }
}

/// Get icon color based on state
pub fn icon_color(ui: &Ui, hovered: bool, selected: bool) -> Color32 {
    let visuals = &ui.style().visuals;

    if selected || hovered {
        visuals.widgets.active.fg_stroke.color
    } else {
        visuals.widgets.noninteractive.fg_stroke.color
    }
}

/// Render a styled dialog close button (X) with rounded rect border.
///
/// Used in dialog title bars. Draws a selection-colored outlined square
/// with an X glyph inside. Returns the click [`Response`].
///
/// # Example
///
/// ```ignore
/// use crate::ui_kit::buttons::dialog_close_button;
///
/// ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
///     ui.add_space(DESIGN_TOKENS.spacing.sm);
///     if dialog_close_button(ui).clicked() {
///         // close the dialog
///     }
/// });
/// ```
pub fn dialog_close_button(ui: &mut Ui) -> Response {
    let btn_size = Vec2::splat(DESIGN_TOKENS.sizing.icon_xl);
    let (rect, response) = ui.allocate_exact_size(btn_size, Sense::click());
    let is_hovered = response.hovered();

    // Stroke color: brighter on hover, slightly dimmed at rest
    let stroke_color = if is_hovered {
        ui.style().visuals.selection.bg_fill
    } else {
        ui.style().visuals.selection.bg_fill.gamma_multiply(0.7)
    };

    let stroke_width = DESIGN_TOKENS.spacing.hairline + 0.5;

    // Draw rounded rect border
    ui.painter().rect_stroke(
        rect,
        DESIGN_TOKENS.rounding.sm,
        egui::Stroke::new(stroke_width, stroke_color),
        egui::StrokeKind::Inside,
    );

    // Draw X glyph
    let center = rect.center();
    let half = DESIGN_TOKENS.spacing.sm + 1.0;
    ui.painter().line_segment(
        [
            Pos2::new(center.x - half, center.y - half),
            Pos2::new(center.x + half, center.y + half),
        ],
        egui::Stroke::new(stroke_width, stroke_color),
    );
    ui.painter().line_segment(
        [
            Pos2::new(center.x + half, center.y - half),
            Pos2::new(center.x - half, center.y + half),
        ],
        egui::Stroke::new(stroke_width, stroke_color),
    );

    response
}
