//! Fibonacci & Gann category for left toolbar.
//!
//! Contains two sub-sections:
//! - FIBONACCI: Fib Retracement, Extension, Channel, Time Zones, Speed Fan, Pitchfan, etc.
//! - GANN: Gann Box, Gann Square, Gann Fixed, Gann Fan

use super::ToolCategory;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
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
}
