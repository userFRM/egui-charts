//! Brushes & Shapes category for left toolbar.
//!
//! Contains three sub-sections:
//! - BRUSHES: Brush, Highlighter
//! - ARROWS: Arrow Marker, Arrow, Arrow Mark Up, Arrow Mark Down
//! - SHAPES: Rectangle, Rotated Rectangle, Path, Circle, Ellipse, Polyline, Triangle, Arc, Curve, Double Curve

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Rect, Ui};

use super::trend_lines::render_tool_submenu;
use super::{DrawingToolbarAction, ToolCategory};
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

    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        vec![
            (
                "BRUSHES",
                vec![DrawingToolType::Brush, DrawingToolType::Highlighter],
            ),
            (
                "ARROWS",
                vec![
                    DrawingToolType::ArrowMarker,
                    DrawingToolType::Arrow,
                    DrawingToolType::ArrowMarkUp,
                    DrawingToolType::ArrowMarkDown,
                ],
            ),
            (
                "SHAPES",
                vec![
                    DrawingToolType::Rect,
                    DrawingToolType::RotatedRect,
                    DrawingToolType::Path,
                    DrawingToolType::Circle,
                    DrawingToolType::Ellipse,
                    DrawingToolType::Polyline,
                    DrawingToolType::Triangle,
                    DrawingToolType::Arc,
                    DrawingToolType::Curve,
                    DrawingToolType::DoubleCurve,
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
            "shapes_submenu",
        )
    }
}
