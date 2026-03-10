//! ListItemContent trait for composable list item content

use egui::{Color32, Rect, Response, Ui};

use super::visuals::ListVisuals;

/// Context provided to ListItemContent implementations when rendering
pub struct ContentContext<'a> {
    /// The rect allocated for the content (excluding any list item chrome)
    pub rect: Rect,
    /// Background color of the list item (computed from visuals)
    pub bg_color: Color32,
    /// Whether the list item is selected
    pub selected: bool,
    /// Whether the list item is hovered
    pub hovered: bool,
    /// Reference to the list item for accessing configuration
    pub list_item: &'a ListItem,
    /// Reference to the response for the list item
    pub response: &'a egui::Response,
    /// Visual state with color computation methods (rerun-inspired)
    pub visuals: ListVisuals,
}

/// Trait for types that can be rendered as list item content
///
/// Implement this trait to create custom list item content types.
/// The framework provides several built-in implementations:
/// - [`LabelContent`](super::LabelContent) - Simple text with optional icon
/// - [`PropertyContent`](super::PropertyContent) - Key-value pair display
/// - [`ButtonContent`](super::ButtonContent) - Clickable button content with styles
pub trait ListItemContent {
    /// Render the content and return the response
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> Response;
}

/// Reference to ListItem configuration (forward declaration)
pub struct ListItem {
    /// Whether this item is selected
    pub(crate) selected: bool,
    /// Whether this item is interactive (clickable)
    pub(crate) interactive: bool,
    /// Whether to show a drag handle
    pub(crate) draggable: bool,
    /// Optional fixed height
    pub(crate) height: Option<f32>,
    /// Whether to show separator below
    pub(crate) show_separator: bool,
}

impl Default for ListItem {
    fn default() -> Self {
        Self {
            selected: false,
            interactive: true,
            draggable: false,
            height: None,
            show_separator: false,
        }
    }
}

impl ListItem {
    /// Create a new list item
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether this item is selected
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Set whether this item is interactive (clickable)
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Set whether this item is draggable
    #[must_use]
    pub fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    /// Set a fixed height for the item
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Set whether to show a separator below this item
    #[must_use]
    pub fn show_separator(mut self, show: bool) -> Self {
        self.show_separator = show;
        self
    }

    /// Check if this item is selected
    pub fn is_selected(&self) -> bool {
        self.selected
    }

    /// Check if this item is interactive
    pub fn is_interactive(&self) -> bool {
        self.interactive
    }
}
