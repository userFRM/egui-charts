//! Fibonacci & Gann category for left toolbar.
//!
//! Contains two sub-sections:
//! - FIBONACCI: Fib Retracement, Extension, Channel, Time Zones, Speed Fan, Pitchfan, etc.
//! - GANN: Gann Box, Gann Square, Gann Fixed, Gann Fan

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Rect, Ui};

use super::trend_lines::render_tool_submenu;
use super::{DrawingToolbarAction, ToolCategory};
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Fibonacci & Gann category implementation
pub struct FibonacciCategory;

impl ToolCategory for FibonacciCategory {
    fn name(&self) -> &'static str {
        "Fibonacci"
    }

    fn tooltip(&self) -> &'static str {
        "Gann and Fibonacci tools"
    }

    fn icon(&self) -> &'static Icon {
        &icons::FIB_RETRACEMENT
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::FIB_RETRACEMENT
    }

    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        vec![
            (
                "FIBONACCI",
                vec![
                    DrawingToolType::FibonacciRetracement,
                    DrawingToolType::FibonacciExtension,
                    DrawingToolType::FibonacciChannel,
                    DrawingToolType::FibonacciTimeZones,
                    DrawingToolType::FibonacciSpeedFan,
                    DrawingToolType::TrendBasedFibTime,
                    DrawingToolType::FibonacciCircles,
                    DrawingToolType::FibonacciSpiral,
                    DrawingToolType::FibonacciSpeedResistanceArcs,
                    DrawingToolType::FibonacciWedge,
                    DrawingToolType::Pitchfan,
                ],
            ),
            (
                "GANN",
                vec![
                    DrawingToolType::GannBox,
                    DrawingToolType::GannSquare,
                    DrawingToolType::GannFixed,
                    DrawingToolType::GannFan,
                ],
            ),
        ]
    }

    fn render_submenu(
        &self,
        ui: &mut Ui,
        anchor_rect: Rect,
        sel_tool: Option<DrawingToolType>,
        favorites: &[DrawingToolType],
    ) -> DrawingToolbarAction {
        render_tool_submenu(
            ui,
            anchor_rect,
            self.sections(),
            sel_tool,
            favorites,
            "fibonacci_submenu",
        )
    }
}

/// Keyboard shortcuts for Fibonacci tools
pub fn get_shortcut(tool: DrawingToolType) -> Option<&'static str> {
    match tool {
        DrawingToolType::FibonacciRetracement => Some("Alt F"),
        _ => None,
    }
}
