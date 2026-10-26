//! Remove objects dropdown menu.
//!
//! Provides options to permanently remove chart elements:
//! - All Drawings
//! - All Indicators

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Response, Ui};

/// Remove objects dropdown menu component.
pub struct RemoveMenu;

impl RemoveMenu {
    /// Render the remove button with trash icon. A click opens the dropdown.
    pub fn show(ui: &mut Ui) -> Response {
        draw_tool_button(ui, &icons::TRASH, "Remove objects", false)
    }

    /// Render the remove dropdown rows, in the error colour.
    pub fn show_submenu(ui: &mut Ui) -> Option<DrawingToolbarAction> {
        SubmenuBuilder::new(ui)
            .add_text_item_with_action(
                "Remove all drawings",
                "Permanently delete all drawing tools from chart",
                || Some(DrawingToolbarAction::RemoveAllDrawings),
            )
            .danger(&icons::TRASH)
            .add_text_item_with_action(
                "Remove all indicators",
                "Remove all indicators from chart",
                || Some(DrawingToolbarAction::RemoveAllIndicators),
            )
            .danger(&icons::TRASH)
            .show()
    }
}
