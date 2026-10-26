//! Buttons, separators and SVG helpers for composing drawing toolbars.
//!
//! Use [`PairButton`] for a tool button with a separate flyout trigger, or the
//! button functions for individual tools and toggles.

pub mod icon_button;
pub mod pair_button;
pub mod separator;
pub mod svg_helpers;

// Re-export all public components
pub use icon_button::{draw_icon_btn, draw_icon_button_padded, draw_toggle_btn, draw_tool_button};
pub use pair_button::{ArrowDirection, PairButton, PairButtonResponse, draw_arrow};
pub use separator::{draw_separator, draw_separator_styled, draw_separator_with_margin};
pub use svg_helpers::{render_svg_at_rect_themed, render_svg_centered};

/// Fill of a selected button under the pointer: the pressed fill, lifted slightly.
fn active_hover_color(ui: &egui::Ui) -> egui::Color32 {
    let active = ui.visuals().widgets.active.bg_fill;
    egui::Color32::from_rgb(
        active.r().saturating_add(5),
        active.g().saturating_add(5),
        active.b().saturating_add(5),
    )
}
