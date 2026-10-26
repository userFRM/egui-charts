//! Patterns category for left toolbar.
//!
//! Contains three sub-sections:
//! - PATTERNS: XABCD, Cypher, Head and Shoulders, ABCD, Triangle, Three Drives
//! - ELLIOTT WAVES: Impulse, Correction, Triangle, Double Combo, Triple Combo
//! - CYCLES: Cyclic Lines, Time Cycles, Sine Line

use super::ToolCategory;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
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
}
