//! Drawing tool category lookup and search.
//!
//! [`DrawingToolsRepository`] supplies category sections and case-insensitive tool
//! search for host pickers. [`InMemoryToolsRepository`] reads the crate's static
//! category definitions.

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
        // Exclude special categories (Favorites/Recent) which are dynamic
        vec![
            "Lines".to_string(),
            "Fibonacci".to_string(),
            "Patterns".to_string(),
            "Projection".to_string(),
            "Brushes/Shapes".to_string(),
            "Text/Annotations".to_string(),
            "Icons/Emojis".to_string(),
        ]
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

    /// A tool reports the category it is listed under, and a case-insensitive
    /// search on part of its name finds it.
    #[test]
    fn tools_are_found_by_category_and_search() {
        let repo = InMemoryToolsRepository::new();
        let cases = [
            (DrawingToolType::TrendLine, "Lines", "trend"),
            (DrawingToolType::FibonacciRetracement, "Fibonacci", "FIB"),
        ];
        for (tool, category, query) in cases {
            assert_eq!(
                repo.get_tool_category(tool).as_deref(),
                Some(category),
                "{tool:?}"
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
