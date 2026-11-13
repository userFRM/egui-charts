//! Hide/Show objects dropdown menu.
//!
//! Controls visibility of various chart elements:
//! - Drawings
//! - Indicators
//! - Positions
//! - Orders
//! - All of them

use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::state::ToolbarState;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use crate::ui_kit::list_item::ListItem;
use egui::{Response, Ui};

/// Hide/Show dropdown menu component.
pub struct HideMenu;

impl HideMenu {
    /// Render a checked toolbar visibility control in a host menu outside the left panel.
    /// Handle its action and omit that panel while `preferences.hidden` is set.
    pub fn show_toolbar_toggle(ui: &mut Ui, state: &ToolbarState) -> Option<DrawingToolbarAction> {
        let visible = !state.preferences.hidden;
        let response = ListItem::new().show_with_icon(ui, "Drawing tools", &icons::PENCIL);
        if visible {
            let rect = egui::Rect::from_center_size(
                response.rect.right_center() - egui::vec2(12.0, 0.0),
                egui::Vec2::splat(12.0),
            );
            icons::CHECK
                .as_image_tinted(rect.size(), ui.active_theme().ui.text)
                .paint_at(ui, rect);
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::CheckBox,
                response.enabled(),
                visible,
                "Drawing tools",
            )
        });
        response
            .clicked()
            .then_some(DrawingToolbarAction::SetToolbarVisible(!visible))
    }

    /// Render the visibility menu button. A click opens the dropdown.
    pub fn show(ui: &mut Ui) -> Response {
        draw_tool_button(ui, &icons::EYE, "Hide objects", false)
    }

    /// Render the hide dropdown rows, with the positions and orders rows when `trading`. A check
    /// marks each group `state` has hidden. "Hide all" toggles every supported group.
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
        let tooltip = if trading {
            "Hide drawings, indicators, positions and orders"
        } else {
            "Hide drawings and indicators"
        };
        builder
            .add_text_item_with_action("Hide all", tooltip, || Some(DrawingToolbarAction::HideAll))
            .checked(state.all_hidden(trading))
            .show()
    }
}
