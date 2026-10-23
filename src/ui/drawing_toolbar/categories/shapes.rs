//! Brushes & Shapes category for left toolbar.
//!
//! Contains three sub-sections:
//! - BRUSHES: Brush, Highlighter
//! - ARROWS: Arrow Marker, Arrow, Arrow Mark Up, Arrow Mark Down
//! - SHAPES: Rectangle, Rotated Rectangle, Path, Circle, Ellipse, Polyline, Triangle, Arc, Curve, Double Curve

use super::ToolCategory;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Brushes & Shapes category implementation
pub struct ShapesCategory;

impl ToolCategory for ShapesCategory {
    fn name(&self) -> &'static str {
        "Brushes/Shapes"
    }

    fn tooltip(&self) -> &'static str {
        "Geometric shapes"
    }

    fn icon(&self) -> &'static Icon {
        &icons::BRUSH
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::BRUSH
    }
}
