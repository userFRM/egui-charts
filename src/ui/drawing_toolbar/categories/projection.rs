//! Projection & Measurement category for left toolbar.
//!
//! Contains three sub-sections:
//! - PROJECTION: Long Pos, Short Pos, Forecast, Bars Pattern, Ghost Feed, Projection
//! - VOLUME-BASED: Anchored VWAP, Fixed Range Volume Profile, Anchored Volume Profile
//! - MEASURER: Price Range, Date Range, Date and Price Range

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use egui::{Rect, Ui};

use super::trend_lines::render_tool_submenu;
use super::{DrawingToolbarAction, ToolCategory};
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

    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        vec![
            (
                "PROJECTION",
                vec![
                    DrawingToolType::LongPos,
                    DrawingToolType::ShortPos,
                    DrawingToolType::Forecast,
                    DrawingToolType::BarsPattern,
                    DrawingToolType::GhostFeed,
                    DrawingToolType::ProjectionTool,
                ],
            ),
            (
                "VOLUME-BASED",
                vec![
                    DrawingToolType::AnchoredVWAP,
                    DrawingToolType::FixedRangeVolumeProfile,
                    DrawingToolType::AnchoredVolumeProfile,
                ],
            ),
            (
                "MEASURER",
                vec![
                    DrawingToolType::PriceRange,
                    DrawingToolType::DateRange,
                    DrawingToolType::DateAndPriceRange,
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
            "projection_submenu",
        )
    }
}
