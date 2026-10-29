//! Drawing tool category lookup and search.
//!
//! [`DrawingToolsRepository`] supplies category sections and case-insensitive tool
//! search for host pickers. [`InMemoryToolsRepository`] reads the crate's static
//! toolbar category definitions in [`categories`], rather than the finer
//! command-list groups of [`DrawingToolType::categories`]. Measure is a standalone
//! toolbar button, so it is not included in this repository's category search.

use crate::drawings::DrawingToolType;
use crate::drawings::categories;

/// Repository trait for drawing tools data access.
///
/// This trait defines the contract for data access. Different implementations
/// can provide data from different sources (static data, JSON files, databases, etc.)
pub trait DrawingToolsRepository {
    /// Get all available category names in display order
    fn get_all_categories(&self) -> Vec<String>;

    /// Get all tools for a specific category, organized by sections
    ///
    /// Returns: Vec<(section_name, tools_in_section)>
    fn get_category_tools(&self, category: &str) -> Vec<(String, Vec<DrawingToolType>)>;

    /// Get the category that a tool belongs to
    fn get_tool_category(&self, tool: DrawingToolType) -> Option<String>;

    /// Search tools by query string (case-insensitive)
    fn search_tools(&self, query: &str) -> Vec<DrawingToolType>;
}

/// Drawing tool lookup backed by [`categories`].
pub struct InMemoryToolsRepository;

impl InMemoryToolsRepository {
    /// Create a new in-memory repository
    pub fn new() -> Self {
        Self
    }
}

impl Default for InMemoryToolsRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingToolsRepository for InMemoryToolsRepository {
    fn get_all_categories(&self) -> Vec<String> {
        categories::CATEGORY_ORDER
            .iter()
            .map(|category| category.to_string())
            .collect()
    }

    fn get_category_tools(&self, category: &str) -> Vec<(String, Vec<DrawingToolType>)> {
        categories::get_category_sections(category)
            .into_iter()
            .map(|(section, tools)| (section.to_string(), tools))
            .collect()
    }

    fn get_tool_category(&self, tool: DrawingToolType) -> Option<String> {
        categories::get_tool_category(tool).map(|s| s.to_string())
    }

    fn search_tools(&self, query: &str) -> Vec<DrawingToolType> {
        if query.is_empty() {
            return self.get_all_tools();
        }

        let query_lower = query.to_lowercase();
        self.get_all_tools()
            .into_iter()
            .filter(|tool| tool.as_str().to_lowercase().contains(&query_lower))
            .collect()
    }
}

impl InMemoryToolsRepository {
    /// Get all available tools from all categories
    fn get_all_tools(&self) -> Vec<DrawingToolType> {
        let mut all_tools = Vec::new();
        for category in self.get_all_categories() {
            for (_, tools) in self.get_category_tools(&category) {
                all_tools.extend(tools);
            }
        }
        all_tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toolbar categories include cursors, unknown names have no fallback, and
    /// a case-insensitive search finds tools in their listed category.
    #[test]
    fn tools_are_found_by_category_and_search() {
        let repo = InMemoryToolsRepository::new();
        assert_eq!(repo.get_all_categories(), categories::CATEGORY_ORDER);
        assert_eq!(categories::get_category_default_tool("Nonsense"), None);
        assert!(DrawingToolType::by_category("Nonsense").is_empty());
        let cases = [
            (DrawingToolType::CrossCursor, "Cursors", "Cursors", "cross"),
            (DrawingToolType::TrendLine, "Lines", "Lines", "trend"),
            (
                DrawingToolType::FibonacciRetracement,
                "Fibonacci",
                "Fibonacci",
                "FIB",
            ),
            (
                DrawingToolType::Table,
                "Text/Annotations",
                "Annotations",
                "table",
            ),
            (
                DrawingToolType::Image,
                "Text/Annotations",
                "Content",
                "image",
            ),
            (
                DrawingToolType::Tweet,
                "Text/Annotations",
                "Content",
                "tweet",
            ),
            (DrawingToolType::Idea, "Text/Annotations", "Content", "idea"),
            (
                DrawingToolType::GhostFeed,
                "Projection",
                "Projection",
                "ghost",
            ),
        ];
        for (tool, category, command_category, query) in cases {
            assert_eq!(
                repo.get_tool_category(tool).as_deref(),
                Some(category),
                "{tool:?}"
            );
            assert!(
                DrawingToolType::by_category(command_category).contains(&tool),
                "{tool:?} missing from command search"
            );
            let listed = repo
                .get_category_tools(category)
                .iter()
                .any(|(_, tools)| tools.contains(&tool));
            assert!(listed, "{tool:?} is not listed under {category}");
            assert!(
                repo.search_tools(query).contains(&tool),
                "search {query:?} misses {tool:?}"
            );
        }
    }
}
