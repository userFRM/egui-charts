//! Remove objects dropdown menu.
//!
//! Provides options to permanently remove chart elements:
//! - All Drawings
//! - All Indicators
//! - All Studies

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Rect, Ui};

/// Remove objects dropdown menu component.
pub struct RemoveMenu;

impl RemoveMenu {
    /// Render the remove button with trash icon.
    ///
    /// Returns tuple of (icon_clicked, arrow_clicked, pair_rect)
    pub fn show(ui: &mut Ui) -> (bool, bool, Rect) {
        let response = draw_tool_button(ui, &icons::TRASH, "Remove objects", false);

        // Click opens dropdown to show remove options
        let arrow_clicked = response.clicked();

        (false, arrow_clicked, response.rect)
    }

    /// Render the remove dropdown submenu.
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
                "Remove All Drawings",
                "Permanently delete all drawing tools from chart",
                || Some(DrawingToolbarAction::RemoveAllDrawings),
            )
            .add_text_item_with_action(
                "Remove Indicators",
                "Remove all indicators from chart",
                || Some(DrawingToolbarAction::RemoveAllIndicators),
            )
            .add_text_item_with_action(
                "Remove All Studies",
                "Remove all studies (indicators and overlays) from chart",
                || Some(DrawingToolbarAction::RemoveAllStudies),
            )
            .show()
    }
}
