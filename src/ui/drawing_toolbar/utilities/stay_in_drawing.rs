//! Stay-in-drawing mode button.
//!
//! When enabled, the drawing tool remains active after completing
//! a drawing, allowing continuous drawing without reselecting the tool.

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Keep Drawing / Stay in Drawing Mode button.
pub struct StayInDrawingButton;

impl StayInDrawingButton {
    /// Render the stay-in-drawing button and return any action.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(ui, &icons::MAGNET, "Keep drawing", is_active);

        if response.clicked() {
            Some(DrawingToolbarAction::ToggleStayInDrawingMode)
        } else {
            None
        }
    }
}
