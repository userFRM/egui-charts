//! Filter Widget
//!
//! A text input with filtering capabilities for lists.
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::filter::FilterWidget;
//!
//! // Create filter widget
//! let mut filter = FilterWidget::new();
//!
//! // Show the filter input
//! filter.show(ui);
//!
//! // Filter your items
//! for item in items.iter().filter(|i| filter.matches(&i.name)) {
//!     ui.label(&item.name);
//! }
//! ```

use egui::{Response, TextEdit, Ui, Vec2, Widget};

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;

/// Response from showing a FilterWidget
pub struct FilterResponse {
    /// The underlying egui Response
    pub response: Response,
    /// Whether the query changed this frame
    pub changed: bool,
}

/// A text input with filtering capabilities
#[derive(Default, Clone)]
pub struct FilterWidget {
    query: String,
    placeholder: String,
    show_clear_button: bool,
    show_icon: bool,
}

impl FilterWidget {
    /// Create a new filter widget
    pub fn new() -> Self {
        Self {
            query: String::new(),
            placeholder: "Filter...".to_string(),
            show_clear_button: true,
            show_icon: true,
        }
    }

    /// Create a filter widget with initial query
    pub fn with_query(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Default::default()
        }
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Set whether to show the clear button
    pub fn show_clear_button(mut self, show: bool) -> Self {
        self.show_clear_button = show;
        self
    }

    /// Set whether to show the search icon
    pub fn show_icon(mut self, show: bool) -> Self {
        self.show_icon = show;
        self
    }

    /// Get the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get mutable access to the query
    pub fn query_mut(&mut self) -> &mut String {
        &mut self.query
    }

    /// Clear the query
    pub fn clear(&mut self) {
        self.query.clear();
    }

    /// Check if an item matches the current filter (case-insensitive)
    pub fn matches(&self, text: &str) -> bool {
        if self.query.is_empty() {
            return true;
        }
        text.to_lowercase().contains(&self.query.to_lowercase())
    }

    /// Check if the filter is empty
    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }

    /// Show the filter widget
    pub fn show(&mut self, ui: &mut Ui) -> FilterResponse {
        let old_query = self.query.clone();

        let response = ui.horizontal(|ui| {
            // Search icon
            if self.show_icon {
                let icon_size = DESIGN_TOKENS.sizing.icon_sm;
                let tint = ui.style().visuals.widgets.noninteractive.fg_stroke.color;
                ui.add(icons::QUICK_SEARCH.as_image_tinted(Vec2::splat(icon_size), tint));
            }

            // Text input
            let text_response = TextEdit::singleline(&mut self.query)
                .hint_text(&self.placeholder)
                .desired_width(f32::INFINITY)
                .ui(ui);

            // Clear button
            if self.show_clear_button && !self.query.is_empty() {
                let clear_btn = ui.add(icons::CLOSE.as_image_tinted(
                    Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                    ui.style().visuals.widgets.noninteractive.fg_stroke.color,
                ));
                if clear_btn.clicked() {
                    self.query.clear();
                }
            }

            text_response
        });

        // Handle Escape key to clear
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !self.query.is_empty() {
            self.query.clear();
        }

        FilterResponse {
            response: response.inner,
            changed: self.query != old_query,
        }
    }
}

/// Stateless filter input that operates on an external string
pub fn filter_input(ui: &mut Ui, query: &mut String) -> FilterResponse {
    let old_query = query.clone();

    let response = ui.horizontal(|ui| {
        // Search icon
        let icon_size = DESIGN_TOKENS.sizing.icon_sm;
        let tint = ui.style().visuals.widgets.noninteractive.fg_stroke.color;
        ui.add(icons::QUICK_SEARCH.as_image_tinted(Vec2::splat(icon_size), tint));

        // Text input
        let text_response = TextEdit::singleline(query)
            .hint_text("Filter...")
            .desired_width(f32::INFINITY)
            .ui(ui);

        // Clear button
        if !query.is_empty() {
            let clear_btn = ui.add(icons::CLOSE.as_image_tinted(
                Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                ui.style().visuals.widgets.noninteractive.fg_stroke.color,
            ));
            if clear_btn.clicked() {
                query.clear();
            }
        }

        text_response
    });

    // Handle Escape key to clear
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !query.is_empty() {
        query.clear();
    }

    FilterResponse {
        response: response.inner,
        changed: *query != old_query,
    }
}
