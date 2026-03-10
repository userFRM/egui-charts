//! Zoom in/out btns.
//!
//! Zoom tools for chart navigation:
//! - Zoom In: Click and drag to zoom to a specific area
//! - Zoom Out: Restore previous zoom level from history

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Zoom In button component.
pub struct ZoomInButton;

impl ZoomInButton {
    /// Render the zoom in button and return any action.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(ui, &icons::ZOOM_IN, "Zoom In", is_active);

        if response.clicked() {
            Some(DrawingToolbarAction::ZoomIn)
        } else {
            None
        }
    }
}

/// Zoom Out button component.
pub struct ZoomOutButton;

impl ZoomOutButton {
    /// Render the zoom out button and return any action.
    pub fn show(ui: &mut Ui) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(ui, &icons::ZOOM_OUT, "Zoom Out", false);

        if response.clicked() {
            Some(DrawingToolbarAction::ZoomOut)
        } else {
            None
        }
    }
}
