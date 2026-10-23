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
    /// Render the lock button and return any action. While `is_active` (all drawings are
    /// locked) it is filled, shows the closed padlock and offers to unlock.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        let (icon, tooltip) = if is_active {
            (&icons::LOCK, "Unlock all drawings")
        } else {
            (&icons::UNLOCK, "Lock all drawings")
        };
        let response = draw_tool_button(ui, icon, tooltip, is_active);

        if response.clicked() {
            log::info!("Lock all drawings clicked");
            Some(DrawingToolbarAction::LockAllDrawings)
        } else {
            None
        }
    }
}
