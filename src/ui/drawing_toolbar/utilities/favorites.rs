//! Favorites toolbar toggle button.
//!
//! Shows/hides the favorites toolbar that displays
//! frequently used drawing tools for quick access.

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Favorites toolbar toggle button.
pub struct FavoritesButton;

impl FavoritesButton {
    /// Render the favorites button and return any action.
    pub fn show(ui: &mut Ui, is_visible: bool) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(
            ui,
            &icons::STAR_EMPTY,
            "Show Favorite Drawing Tools Toolbar",
            is_visible,
        );

        if response.clicked() {
            log::info!(
                "Favorites toolbar: {}",
                if !is_visible { "shown" } else { "hidden" }
            );
            Some(DrawingToolbarAction::ToggleFavoritesToolbar)
        } else {
            None
        }
    }
}
