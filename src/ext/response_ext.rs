//! ResponseExt - Extension trait for egui::Response
//!
//! Provides convenience methods for handling interaction responses.
//!
//! # Categories
//!
//! - **Click handlers**: on_click, on_double_click, on_triple_click, on_secondary_click, on_middle_click
//! - **Hover handlers**: on_hover
//! - **State queries**: is_double_clicked, is_middle_clicked
//! - **UI helpers**: highlight, show_tooltip_on_hover
//! - **Cursor**: set_cursor_icon

use crate::tokens::DESIGN_TOKENS;
use egui::{CursorIcon, Response, Ui};

/// Extension trait for `egui::Response` providing convenience methods
///
/// Note: Some egui Response methods consume self. For those methods,
/// use the egui Response methods directly rather than these extensions.
pub trait ResponseExt {
    // ==========================================================================
    // Click Handlers
    // ==========================================================================

    /// Handle single-click events (convenience method)
    fn on_click(&self, action: impl FnOnce()) -> &Self;

    /// Handle double-click events
    fn on_double_click(&self, action: impl FnOnce()) -> &Self;

    /// Handle triple-click events
    fn on_triple_click(&self, action: impl FnOnce()) -> &Self;

    /// Handle secondary click (right-click) events
    fn on_secondary_click(&self, action: impl FnOnce()) -> &Self;

    /// Handle middle click events
    fn on_middle_click(&self, action: impl FnOnce()) -> &Self;

    // ==========================================================================
    // Hover Handlers
    // ==========================================================================

    /// Handle hover events
    fn on_hover(&self, action: impl FnOnce()) -> &Self;

    /// Handle hover start (entering hover state)
    fn on_hover_start(&self, action: impl FnOnce()) -> &Self;

    // ==========================================================================
    // State Queries
    // ==========================================================================

    /// Check if the response was double-clicked (alias for double_clicked)
    fn is_double_clicked(&self) -> bool;

    /// Check if the response was middle-clicked (alias for middle_clicked)
    fn is_middle_clicked(&self) -> bool;

    /// Check if the response was secondary-clicked (alias for secondary_clicked)
    fn is_secondary_clicked(&self) -> bool;

    /// Check if drag started this frame
    fn drag_started(&self) -> bool;

    /// Check if drag ended this frame
    fn drag_stopped(&self) -> bool;

    // ==========================================================================
    // UI Helpers
    // ==========================================================================

    /// Highlight the response rect (for debugging or emphasis)
    fn highlight(&self, ui: &mut Ui) -> &Self;

    /// Show a tooltip on hover with the given text
    fn show_tooltip_on_hover(&self, text: impl Into<String>) -> &Self;

    // ==========================================================================
    // Cursor
    // ==========================================================================

    /// Set cursor icon when hovering over this response
    fn set_cursor_icon(&self, icon: CursorIcon) -> &Self;
}

impl ResponseExt for Response {
    // ==========================================================================
    // Click Handlers
    // ==========================================================================

    fn on_click(&self, action: impl FnOnce()) -> &Self {
        if self.clicked() {
            action();
        }
        self
    }

    fn on_double_click(&self, action: impl FnOnce()) -> &Self {
        if self.double_clicked() {
            action();
        }
        self
    }

    fn on_triple_click(&self, action: impl FnOnce()) -> &Self {
        if self.triple_clicked() {
            action();
        }
        self
    }

    fn on_secondary_click(&self, action: impl FnOnce()) -> &Self {
        if self.secondary_clicked() {
            action();
        }
        self
    }

    fn on_middle_click(&self, action: impl FnOnce()) -> &Self {
        if self.middle_clicked() {
            action();
        }
        self
    }

    // ==========================================================================
    // Hover Handlers
    // ==========================================================================

    fn on_hover(&self, action: impl FnOnce()) -> &Self {
        if self.hovered() {
            action();
        }
        self
    }

    fn on_hover_start(&self, action: impl FnOnce()) -> &Self {
        // Hovered this frame but not last frame
        if self.hovered() && !self.ctx.input(|i| i.pointer.any_pressed()) {
            // This is a simple approximation - egui doesn't have a built-in "just started hovering"
            // but we can use the response's "sense" timing for this
            action();
        }
        self
    }

    // ==========================================================================
    // State Queries
    // ==========================================================================

    fn is_double_clicked(&self) -> bool {
        self.double_clicked()
    }

    fn is_middle_clicked(&self) -> bool {
        self.middle_clicked()
    }

    fn is_secondary_clicked(&self) -> bool {
        self.secondary_clicked()
    }

    fn drag_started(&self) -> bool {
        self.drag_started_by(egui::PointerButton::Primary)
    }

    fn drag_stopped(&self) -> bool {
        self.drag_stopped_by(egui::PointerButton::Primary)
    }

    // ==========================================================================
    // UI Helpers
    // ==========================================================================

    fn highlight(&self, ui: &mut Ui) -> &Self {
        let rect = self.rect;
        let stroke = egui::Stroke::new(
            DESIGN_TOKENS.stroke.thick,
            DESIGN_TOKENS.semantic.status.caution,
        );
        ui.painter()
            .rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);
        self
    }

    fn show_tooltip_on_hover(&self, text: impl Into<String>) -> &Self {
        let _ = self.clone().on_hover_text(text.into());
        self
    }

    // ==========================================================================
    // Cursor
    // ==========================================================================

    fn set_cursor_icon(&self, icon: CursorIcon) -> &Self {
        if self.hovered() {
            self.ctx.set_cursor_icon(icon);
        }
        self
    }
}
