//! Drawing tool categories for left toolbar.
//!
//! Each category has its own module with its name, tooltip and icons. The tools of the tool
//! categories come from [`data::get_category_sections`], so
//! the bar and the flyout read one table.

pub mod annotation;
pub mod cursors;
pub mod fibonacci;
pub mod patterns;
pub mod projection;
pub mod shapes;
pub mod trend_lines;

// Re-export category traits and common types
pub use annotation::AnnotationCategory;
pub use cursors::CursorsCategory;
pub use fibonacci::FibonacciCategory;
pub use patterns::PatternsCategory;
pub use projection::ProjectionCategory;
pub use shapes::ShapesCategory;
pub use trend_lines::TrendLinesCategory;

use crate::drawings::DrawingToolType;
use crate::icons::Icon;
use egui::Ui;

use super::{DrawingToolbarAction, data};

/// Trait for all tool categories
pub trait ToolCategory {
    /// Category name displayed in UI
    fn name(&self) -> &'static str;

    /// Category tooltip for arrow button
    fn tooltip(&self) -> &'static str;

    /// Get the SVG icon for the category button
    fn icon(&self) -> &'static Icon;

    /// Get the currently selected tool icon (or default)
    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon;

    /// Get all tools in this category
    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    /// Get sections for the submenu (section_name, tools)
    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        data::get_category_sections(self.name())
    }

    /// Check if a tool belongs to this category
    fn contains(&self, tool: DrawingToolType) -> bool {
        self.all_tools().contains(&tool)
    }

    /// Draw the submenu rows into `ui`, which the toolbar shows in its flyout. A click on a
    /// star toggles that tool in `favorites`.
    fn render_submenu(
        &self,
        ui: &mut Ui,
        sel_tool: Option<DrawingToolType>,
        favorites: &mut Vec<DrawingToolType>,
    ) -> DrawingToolbarAction {
        trend_lines::render_tool_submenu(ui, self.sections(), sel_tool, favorites)
    }
}

/// Get all tool categories in display order
pub fn all_categories() -> Vec<Box<dyn ToolCategory>> {
    vec![
        Box::new(CursorsCategory),
        Box::new(TrendLinesCategory),
        Box::new(FibonacciCategory),
        Box::new(PatternsCategory),
        Box::new(ProjectionCategory),
        Box::new(ShapesCategory),
        Box::new(AnnotationCategory),
    ]
}
