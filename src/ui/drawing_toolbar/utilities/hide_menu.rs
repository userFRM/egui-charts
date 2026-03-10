//! Hide/Show objects dropdown menu.
//!
//! Controls visibility of various chart elements:
//! - Drawings
//! - Indicators
//! - Poss
//! - Orders

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Rect, Ui};

/// Hide/Show dropdown menu component.
pub struct HideMenu;

impl HideMenu {
    /// Render the hide button.
    ///
    /// Returns tuple of (icon_clicked, arrow_clicked, pair_rect)
    pub fn show(ui: &mut Ui) -> (bool, bool, Rect) {
        let response = draw_tool_button(ui, &icons::EYE_HIDE, "Hide all drawings", false);

        // Click opens dropdown to show hide options
        let arrow_clicked = response.clicked();

        (false, arrow_clicked, response.rect)
    }

    /// Render the hide dropdown submenu.
    pub fn show_submenu(
        ui: &mut Ui,
        sidebar_rect: Rect,
        category_rect: Option<Rect>,
    ) -> Option<DrawingToolbarAction> {
        let mut builder = SubmenuBuilder::new(ui, sidebar_rect)
            .with_width(DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg);

        if let Some(cat_rect) = category_rect {
            builder = builder.with_category_rect(cat_rect);
        }

        builder
            .add_text_item_with_action(
                "Hide All Drawings",
                "Hide all drawing tools from chart",
                || Some(DrawingToolbarAction::HideAllDrawings),
            )
            .add_text_item_with_action("Hide Indicators", "Hide all indicators from chart", || {
                Some(DrawingToolbarAction::HideAllIndicators)
            })
            .add_text_item_with_action("Hide Poss", "Hide all positions from chart", || {
                Some(DrawingToolbarAction::HideAllPoss)
            })
            .add_text_item_with_action("Hide Orders", "Hide all orders from chart", || {
                Some(DrawingToolbarAction::HideAllOrders)
            })
            .show()
    }
}
