//! Command struct for the command palette

use crate::icons::Icon;
use egui::KeyboardShortcut;

/// A command that can be executed from the command palette
#[derive(Clone)]
pub struct Command {
    /// Unique identifier for this command
    pub id: String,
    /// Display label shown in the palette
    pub label: String,
    /// Optional longer description
    pub description: Option<String>,
    /// Optional keyboard shortcut
    pub shortcut: Option<KeyboardShortcut>,
    /// Optional icon
    pub icon: Option<&'static Icon>,
    /// Optional category for grouping
    pub category: Option<String>,
    /// Keywords for better search matching
    pub keywords: Vec<String>,
}

impl Command {
    /// Create a new command with the given ID and label
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            shortcut: None,
            icon: None,
            category: None,
            keywords: Vec::new(),
        }
    }

    /// Set the command description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the keyboard shortcut
    pub fn shortcut(mut self, shortcut: KeyboardShortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Set the icon
    pub fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the category
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Add keywords for better search matching
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords = keywords.into_iter().map(|k| k.into()).collect();
        self
    }

    /// Get all searchable text for this command
    pub fn searchable_text(&self) -> String {
        let mut text = self.label.clone();
        if let Some(desc) = &self.description {
            text.push(' ');
            text.push_str(desc);
        }
        if let Some(cat) = &self.category {
            text.push(' ');
            text.push_str(cat);
        }
        for keyword in &self.keywords {
            text.push(' ');
            text.push_str(keyword);
        }
        text
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("category", &self.category)
            .finish()
    }
}
