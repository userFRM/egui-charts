//! Lock all drawings button.
//!
//! Locks all drawings on the chart to prevent accidental
//! modification or deletion.

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Lock All Drawings button.
pub struct LockButton;

impl LockButton {
    /// Render the lock button and return any action.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(ui, &icons::LOCK, "Lock all drawings", is_active);

        if response.clicked() {
            log::info!("Lock all drawings clicked");
            Some(DrawingToolbarAction::LockAllDrawings)
        } else {
            None
        }
    }
}
