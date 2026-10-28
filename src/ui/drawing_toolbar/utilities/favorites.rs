//! Favourite tools strip button.
//!
//! Shows and hides the floating strip of starred drawing tools.

use crate::icons::icons;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Show/hide button for the favourite tools strip.
pub struct FavoritesButton;

impl FavoritesButton {
    /// Render the button, selected while the strip is shown, and return whether it was clicked.
    pub fn show(ui: &mut Ui, strip_shown: bool) -> bool {
        let tooltip = if strip_shown {
            "Hide favorite drawing tools"
        } else {
            "Show favorite drawing tools"
        };
        draw_tool_button(ui, &icons::STAR_EMPTY, tooltip, strip_shown).clicked()
    }
}
