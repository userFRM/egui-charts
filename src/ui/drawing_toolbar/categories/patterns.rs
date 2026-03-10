//! Patterns category for left toolbar.
//!
//! Contains three sub-sections:
//! - PATTERNS: XABCD, Cypher, Head and Shoulders, ABCD, Triangle, Three Drives
//! - ELLIOTT WAVES: Impulse, Correction, Triangle, Double Combo, Triple Combo
//! - CYCLES: Cyclic Lines, Time Cycles, Sine Line

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Rect, Ui};

use super::trend_lines::render_tool_submenu;
use super::{DrawingToolbarAction, ToolCategory};
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Patterns category implementation
pub struct PatternsCategory;

impl ToolCategory for PatternsCategory {
    fn name(&self) -> &'static str {
        "Patterns"
    }

    fn tooltip(&self) -> &'static str {
        "Patterns"
    }

    fn icon(&self) -> &'static Icon {
        &icons::XABCD_PATTERN
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::XABCD_PATTERN
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
                "PATTERNS",
                vec![
                    DrawingToolType::XABCDPattern,
                    DrawingToolType::CypherPattern,
                    DrawingToolType::HeadAndShoulders,
                    DrawingToolType::ABCDPattern,
                    DrawingToolType::TrianglePattern,
                    DrawingToolType::ThreeDrivesPattern,
                ],
            ),
            (
                "ELLIOTT WAVES",
                vec![
                    DrawingToolType::ElliottImpulse,
                    DrawingToolType::ElliottCorrection,
                    DrawingToolType::ElliottTriangle,
                    DrawingToolType::ElliottDoubleCombo,
                    DrawingToolType::ElliottTripleCombo,
                ],
            ),
            (
                "CYCLES",
                vec![
                    DrawingToolType::CyclicLines,
                    DrawingToolType::TimeCycles,
                    DrawingToolType::SineLine,
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
            "patterns_submenu",
        )
    }
}
