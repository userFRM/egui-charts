//! Drawing tool categories for left toolbar.
//!
//! Each category has its own module with:
//! - Tool definitions and data
//! - SVG icon mappings
//! - Submenu rendering logic
//! - Keyboard shortcuts

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
use egui::{Rect, Ui};

use super::DrawingToolbarAction;

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
    fn all_tools(&self) -> Vec<DrawingToolType>;

    /// Get sections for the submenu (section_name, tools)
    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)>;

    /// Check if a tool belongs to this category
    fn contains(&self, tool: DrawingToolType) -> bool {
        self.all_tools().contains(&tool)
    }

    /// Render the submenu for this category
    fn render_submenu(
        &self,
        ui: &mut Ui,
        anchor_rect: Rect,
        sel_tool: Option<DrawingToolType>,
        favorites: &[DrawingToolType],
    ) -> DrawingToolbarAction;
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
