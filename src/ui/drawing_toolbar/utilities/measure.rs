//! Measure tool button.
//!
//! Activates the measurement tool for measuring price ranges,
//! time ranges, and percentage changes on the chart.

use crate::drawings::DrawingToolType;
use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::components::icon_button::draw_disabled_tool_button;
use egui::Ui;

/// Measure tool button component.
#[derive(Debug)]
pub struct MeasureButton;

impl MeasureButton {
    /// Render the measure button and return any action.
    pub fn show(ui: &mut Ui, is_active: bool) -> Option<DrawingToolbarAction> {
        Self::show_available(ui, is_active, true)
    }

    /// Render the measure button, drawn disabled unless `enabled`, and return any action. A
    /// click on a disabled button returns [`DrawingToolbarAction::ToolUnavailable`].
    pub(crate) fn show_available(
        ui: &mut Ui,
        is_active: bool,
        enabled: bool,
    ) -> Option<DrawingToolbarAction> {
        let response = if enabled {
            draw_tool_button(ui, &icons::MEASURE, "Measure", is_active)
        } else {
            draw_disabled_tool_button(ui, &icons::MEASURE, "Measure", is_active)
        };
        if !response.clicked() {
            None
        } else if enabled {
            log::info!("Measure tool clicked");
            Some(DrawingToolbarAction::SelectTool(DrawingToolType::Measure))
        } else {
            Some(DrawingToolbarAction::ToolUnavailable(
                DrawingToolType::Measure,
            ))
        }
    }
}
