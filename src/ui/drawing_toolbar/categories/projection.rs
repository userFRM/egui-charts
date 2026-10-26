//! Projection & Measurement category for left toolbar.
//!
//! Contains three sub-sections:
//! - PROJECTION: Long Pos, Short Pos, Forecast, Bars Pattern, Ghost Feed, Projection
//! - VOLUME-BASED: Anchored VWAP, Fixed Range Volume Profile, Anchored Volume Profile
//! - MEASURER: Price Range, Date Range, Date and Price Range

use super::ToolCategory;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Projection & Measurement category implementation
pub struct ProjectionCategory;

impl ToolCategory for ProjectionCategory {
    fn name(&self) -> &'static str {
        "Projection"
    }

    fn tooltip(&self) -> &'static str {
        "Forecasting and measurement tools"
    }

    fn icon(&self) -> &'static Icon {
        &icons::LONG_POSITION
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::LONG_POSITION
    }
}
