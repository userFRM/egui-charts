//! Repository for drawing tools data access.
//!
//! This module implements the Repository pattern to abstract data access
//! for drawing tools, categories, and related metadata.
//!
//! ## Benefits
//!
//! - **Abstraction**: UI and services don't know where data comes from
//! - **Testability**: Easy to mock/stub for testing
//! - **Flexibility**: Can swap implementations (in-memory, JSON, database) without changing consumers
//! - **Single responsibility**: All data access logic in one place
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │  UI Layer   │
//! └──────┬──────┘
//!        │ uses
//! ┌──────▼─────────┐
//! │  Service Layer │
//! └──────┬─────────┘
//!        │ uses
//! ┌──────▼────────────────┐
//! │  Repository (trait)   │ ← Interface
//! └───────────────────────┘
//!          △
//!          │ implements
//! ┌────────┴───────────────┐
//! │  InMemoryRepository    │ ← Current implementation
//! └────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use egui_charts::drawings::repositories::{DrawingToolsRepository, InMemoryToolsRepository};
//!
//! let repo = InMemoryToolsRepository::new();
//! let categories = repo.get_all_categories();
//! let tools = repo.get_category_tools("Lines");
//! ```

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

/// In-memory implementation of DrawingToolsRepository.
///
/// This implementation uses static data from the `data` module.
/// It's fast, simple, and suitable for the current use case.
///
/// Future implementations could:
/// - Load from JSON configuration files
/// - Read from a database
/// - Fetch from a remote API
/// - Support user-defined custom tools
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

    #[test]
    fn test_get_all_categories() {
        let repo = InMemoryToolsRepository::new();
        let categories = repo.get_all_categories();

        assert!(!categories.is_empty(), "Should have categories");
        assert!(categories.contains(&"Lines".to_string()));
        assert!(categories.contains(&"Fibonacci".to_string()));
        assert!(
            !categories.contains(&"Favorites".to_string()),
            "Should not include dynamic categories"
        );
    }

    #[test]
    fn test_get_category_tools() {
        let repo = InMemoryToolsRepository::new();
        let tools = repo.get_category_tools("Lines");

        assert!(!tools.is_empty(), "Lines category should have tools");

        // Check that we get sections
        let all_tools: Vec<DrawingToolType> = tools
            .iter()
            .flat_map(|(_, section_tools)| section_tools.clone())
            .collect();

        assert!(
            all_tools.contains(&DrawingToolType::TrendLine),
            "Should contain TrendLine"
        );
    }

    #[test]
    fn test_get_tool_category() {
        let repo = InMemoryToolsRepository::new();

        assert_eq!(
            repo.get_tool_category(DrawingToolType::TrendLine),
            Some("Lines".to_string())
        );
        assert_eq!(
            repo.get_tool_category(DrawingToolType::FibonacciRetracement),
            Some("Fibonacci".to_string())
        );
    }

    #[test]
    fn test_search_tools() {
        let repo = InMemoryToolsRepository::new();

        // Search for "fib" which matches "Fib Retracement", "Fib Channel", etc.
        let results = repo.search_tools("fib");
        assert!(!results.is_empty(), "Should find Fib tools");
        assert!(results.contains(&DrawingToolType::FibonacciRetracement));
    }

    #[test]
    fn test_search_case_insensitive() {
        let repo = InMemoryToolsRepository::new();

        let results1 = repo.search_tools("FIB");
        let results2 = repo.search_tools("fib");

        assert_eq!(
            results1.len(),
            results2.len(),
            "Search should be case-insensitive"
        );
    }

    #[test]
    fn test_search_empty_returns_all() {
        let repo = InMemoryToolsRepository::new();

        let all = repo.get_all_tools();
        let search_results = repo.search_tools("");

        assert_eq!(
            all.len(),
            search_results.len(),
            "Empty search should return all tools"
        );
    }
}
