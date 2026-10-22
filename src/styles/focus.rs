//! Focus Ring Indicators
//!
//! Provides visible focus indicators for keyboard navigation accessibility.
//! Focus rings appear when UI elements receive keyboard focus.

use egui::{Color32, Response, Stroke, Ui, epaint::StrokeKind};

use crate::tokens::DESIGN_TOKENS;

/// The focus ring colour: the selection stroke, which egui also uses to outline a focused text input.
pub fn focus_ring_color(ui: &Ui) -> Color32 {
    ui.visuals().selection.stroke.color
}

/// Draw a focus ring around a response if it has keyboard focus.
///
/// Call this after creating a response to add an accessible focus indicator.
pub fn draw_focus_ring(ui: &Ui, response: &Response) {
    if response.has_focus() {
        let ring_color = focus_ring_color(ui);
        ui.painter().rect_stroke(
            response.rect.expand(DESIGN_TOKENS.spacing.xs),
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(DESIGN_TOKENS.stroke.thick, ring_color),
            StrokeKind::Outside,
        );
    }
}
