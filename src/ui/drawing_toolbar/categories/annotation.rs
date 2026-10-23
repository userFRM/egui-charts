//! Annotation category for left toolbar.
//!
//! Contains text and annotation tools:
//! - TEXT & NOTES: Text, Anchored Text, Note, Price Note, Pin, Table,
//!   Callout, Comment, Price Label, Signpost, Flag Mark
//! - CONTENT: Image, Tweet, Idea

use super::ToolCategory;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Annotation category implementation
pub struct AnnotationCategory;

impl ToolCategory for AnnotationCategory {
    fn name(&self) -> &'static str {
        "Text/Annotations"
    }

    fn tooltip(&self) -> &'static str {
        "Annotation tools"
    }

    fn icon(&self) -> &'static Icon {
        &icons::TEXT
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::TEXT
    }
}
