//! Hide/Show objects dropdown menu.
//!
//! Controls visibility of various chart elements:
//! - Drawings
//! - Indicators
//! - Positions
//! - Orders

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::state::ToolbarState;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Response, Ui};

/// Hide/Show dropdown menu component.
pub struct HideMenu;

impl HideMenu {
    /// Render the hide button. A click opens the dropdown.
    pub fn show(ui: &mut Ui) -> Response {
        draw_tool_button(ui, &icons::EYE_HIDE, "Hide objects", false)
    }

    /// Render the hide dropdown rows, with the positions and orders rows when `trading`. A check
    /// marks each group `state` has hidden.
    pub fn show_submenu(
        ui: &mut Ui,
        trading: bool,
        state: &ToolbarState,
    ) -> Option<DrawingToolbarAction> {
        let mut builder = SubmenuBuilder::new(ui)
            .add_text_item_with_action("Hide drawings", "Hide all drawing tools from chart", || {
                Some(DrawingToolbarAction::HideAllDrawings)
            })
            .checked(state.hide_drawings)
            .add_text_item_with_action("Hide indicators", "Hide all indicators from chart", || {
                Some(DrawingToolbarAction::HideAllIndicators)
            })
            .checked(state.hide_indicators);
        if trading {
            builder = builder
                .add_text_item_with_action(
                    "Hide positions",
                    "Hide all positions from chart",
                    || Some(DrawingToolbarAction::HideAllPoss),
                )
                .checked(state.hide_positions)
                .add_text_item_with_action("Hide orders", "Hide all orders from chart", || {
                    Some(DrawingToolbarAction::HideAllOrders)
                })
                .checked(state.hide_orders);
        }
        builder.show()
    }
}
