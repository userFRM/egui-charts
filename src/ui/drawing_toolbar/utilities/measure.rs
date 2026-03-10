//! Measure tool button.
//!
//! Activates the measurement tool for measuring price ranges,
//! time ranges, and percentage changes on the chart.

use crate::drawings::DrawingToolType;
use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use egui::Ui;

/// Measure tool button component.
pub struct MeasureButton;

impl MeasureButton {
    /// Render the measure button and return any action.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        let response = draw_tool_button(ui, &icons::MEASURE, "Measure", is_active);

        if response.clicked() {
            log::info!("Measure tool clicked");
            Some(DrawingToolbarAction::SelectTool(DrawingToolType::Measure))
        } else {
            None
        }
    }
}
