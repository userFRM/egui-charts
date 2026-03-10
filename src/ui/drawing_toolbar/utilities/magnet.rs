//! Magnet mode button with dropdown menu.
//!
//! Provides magnetic snapping functionality for drawing tools:
//! - Weak Magnet: Loose snapping tolerance
//! - Strong Magnet: Tight snapping tolerance
//! - OHLC Magnet: Snap to OHLC price values

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::state::MagnetType;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Rect, Ui};

/// Magnet button with dropdown component.
pub struct MagnetButton;

impl MagnetButton {
    /// Render the magnet button and return any action.
    ///
    /// Returns tuple of (action, icon_clicked, arrow_clicked, arrow_rect)
    pub fn show(ui: &mut Ui, is_active: bool) -> (Option<DrawingToolbarAction>, bool, bool, Rect) {
        let response = draw_tool_button(ui, &icons::MAGNET, "Magnet Mode", is_active);

        // Click opens dropdown to show magnet options
        let arrow_clicked = response.clicked();

        // No direct action on click - user selects from dropdown
        (None, false, arrow_clicked, response.rect)
    }

    /// Render the magnet dropdown submenu.
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
                "Weak Magnet",
                "Weak magnetic snapping with loose tolerance",
                || Some(DrawingToolbarAction::SetMagnetType(MagnetType::Weak)),
            )
            .add_text_item_with_action(
                "Strong Magnet",
                "Strong magnetic snapping with tight tolerance",
                || Some(DrawingToolbarAction::SetMagnetType(MagnetType::Strong)),
            )
            .add_text_item_with_action("OHLC Magnet", "Snap precisely to OHLC price values", || {
                Some(DrawingToolbarAction::SetMagnetType(MagnetType::OHLC))
            })
            .show()
    }
}
