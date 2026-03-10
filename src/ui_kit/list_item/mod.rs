//! List Item Module - Composable list items following rerun patterns
//!
//! This module provides a flexible list item system with composable content:
//!
//! - `ListItem` - The main container widget
//! - `ListItemContent` - Trait for custom content types
//! - `LabelContent` - Simple text with optional icon
//! - `PropertyContent` - Key-value pair display
//! - `ButtonContent` - Clickable button content
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::list_item::{ListItem, LabelContent, PropertyContent, ButtonContent};
//!
//! // Simple label
//! let response = ListItem::new()
//!     .selected(is_selected)
//!     .show(ui, LabelContent::new("My Item"));
//!
//! // Label with icon
//! let response = ListItem::new()
//!     .show(ui, LabelContent::new("Settings").with_icon(&icons::SETTINGS));
//!
//! // Key-value property
//! let response = ListItem::new()
//!     .show(ui, PropertyContent::new("Price", "$1,234.56")
//!         .value_color(Color32::GREEN));
//!
//! // Action button
//! if ListItem::new()
//!     .show(ui, ButtonContent::new(&icons::ADD, "Add Item").primary())
//!     .clicked()
//! {
//!     self.add_item();
//! }
//! ```

mod button_content;
mod content;
mod label_content;
mod property_content;
mod visuals;

pub use button_content::{ButtonContent, ButtonStyle};
pub use content::{ContentContext, ListItem, ListItemContent};
pub use label_content::LabelContent;
pub use property_content::PropertyContent;
pub use visuals::ListVisuals;

use egui::{Color32, Response, Ui, Vec2};

use crate::tokens::DESIGN_TOKENS;

/// Default height for list items
fn default_height() -> f32 {
    DESIGN_TOKENS.sizing.list_item_height
}

impl ListItem {
    /// Show the list item with the given content
    pub fn show<C: ListItemContent>(self, ui: &mut Ui, content: C) -> Response {
        let height = self.height.unwrap_or_else(default_height);
        let width = ui.available_width();

        // Allocate space
        let (rect, mut response) = ui.allocate_exact_size(
            Vec2::new(width, height),
            if self.interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );

        if ui.is_rect_visible(rect) {
            let egui_visuals = ui.style().visuals.clone();

            // Build ListVisuals (rerun-inspired pattern)
            let list_visuals = ListVisuals::new()
                .with_hovered(response.hovered())
                .with_selected(self.selected)
                .with_interactive(self.interactive);

            // Determine background color using ListVisuals
            let bg_color = list_visuals
                .bg_color(&egui_visuals)
                .unwrap_or(Color32::TRANSPARENT);

            // Draw background
            if bg_color != Color32::TRANSPARENT {
                ui.painter()
                    .rect_filled(rect, DESIGN_TOKENS.rounding.sm, bg_color);
            }

            // Create content context
            let content_context = ContentContext {
                rect: rect.shrink2(Vec2::new(DESIGN_TOKENS.spacing.sm, 0.0)),
                bg_color,
                selected: self.selected,
                hovered: response.hovered(),
                list_item: &self,
                response: &response,
                visuals: list_visuals,
            };

            // Render content
            let _content_response = content.ui(ui, &content_context);

            // Draw separator if requested
            if self.show_separator {
                let sep_y = rect.bottom();
                let sep_color = egui_visuals.widgets.noninteractive.bg_stroke.color;
                ui.painter().hline(
                    rect.x_range(),
                    sep_y,
                    egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, sep_color),
                );
            }
        }

        // Handle selection on click
        if response.clicked() && self.interactive {
            response.mark_changed();
        }

        response
    }

    /// Show the list item with a simple text label
    pub fn show_text(self, ui: &mut Ui, text: impl Into<String>) -> Response {
        self.show(ui, LabelContent::new(text))
    }

    /// Show the list item with a text label and icon
    pub fn show_with_icon(
        self,
        ui: &mut Ui,
        text: impl Into<String>,
        icon: &crate::icons::Icon,
    ) -> Response {
        self.show(ui, LabelContent::new(text).with_icon(icon))
    }

    /// Show the list item as a key-value property
    pub fn show_property(
        self,
        ui: &mut Ui,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Response {
        self.show(ui, PropertyContent::new(label, value))
    }
}

/// Create a simple list of items
///
/// # Example
///
/// ```ignore
/// simple_list(ui, &items, &mut selected, |ui, item, selected| {
///     ListItem::new()
///         .selected(selected)
///         .show_text(ui, &item.name)
/// });
/// ```
pub fn simple_list<T, F>(ui: &mut Ui, items: &[T], selected: &mut Option<usize>, mut show_item: F)
where
    F: FnMut(&mut Ui, &T, bool) -> Response,
{
    for (idx, item) in items.iter().enumerate() {
        let is_selected = *selected == Some(idx);
        let response = show_item(ui, item, is_selected);
        if response.clicked() {
            *selected = Some(idx);
        }
    }
}
