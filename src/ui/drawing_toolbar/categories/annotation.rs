//! Annotation category for left toolbar.
//!
//! Contains text and annotation tools:
//! - TEXT & TEXT: Text, Anchored Text, Note, Price Note, Pin, Table,
//!   Callout, Comment, Price Label, Signpost, Flag Mark, Image, Tweet, Idea

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Rect, Ui};

use super::trend_lines::render_tool_submenu;
use super::{DrawingToolbarAction, ToolCategory};
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

    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        vec![(
            "TEXT & TEXT",
            vec![
                DrawingToolType::TextLabel,
                DrawingToolType::AnchoredText,
                DrawingToolType::Note,
                DrawingToolType::PriceNote,
                DrawingToolType::AnchoredNote,
                DrawingToolType::Table,
                DrawingToolType::Callout,
                DrawingToolType::Comment,
                DrawingToolType::PriceLabel,
                DrawingToolType::Signpost,
                DrawingToolType::FlagNote,
                DrawingToolType::Image,
                DrawingToolType::Tweet,
                DrawingToolType::Idea,
            ],
        )]
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
            "annotation_submenu",
        )
    }
}
