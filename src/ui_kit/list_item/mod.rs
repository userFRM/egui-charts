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
//! ```
//! use egui::Color32;
//! use egui_charts::icons::icons;
//! use egui_charts::ui_kit::list_item::{ButtonContent, LabelContent, ListItem, PropertyContent};
//!
//! # egui::__run_test_ui(|ui| {
//! # let is_selected = false;
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
//!     .show(ui, ButtonContent::new(&icons::PLUS, "Add Item").primary())
//!     .clicked()
//! {
//!     // add the item
//! }
//! # });
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
        let interactive = self.interactive && content.interactive();

        // Rows sit flush: allocate without the trailing item spacing.
        let spacing = std::mem::take(&mut ui.spacing_mut().item_spacing.y);
        let (rect, mut response) = ui.allocate_exact_size(
            Vec2::new(width, height),
            if interactive {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        ui.spacing_mut().item_spacing.y = spacing;

        if ui.is_rect_visible(rect) {
            let egui_visuals = ui.style().visuals.clone();

            // Build ListVisuals (rerun-inspired pattern)
            let list_visuals = ListVisuals::new()
                .with_hovered(response.hovered())
                .with_active(interactive && response.is_pointer_button_down_on())
                .with_selected(self.selected)
                .with_interactive(interactive);

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
                rect: rect.shrink2(Vec2::new(DESIGN_TOKENS.spacing.lg, 0.0)),
                bg_color,
                selected: self.selected,
                hovered: response.hovered(),
                list_item: &self,
                response: &response,
                visuals: list_visuals,
            };

            content.ui(ui, &content_context);

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
        if response.clicked() && interactive {
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
/// ```
/// # use egui_charts::ui_kit::list_item::{ListItem, simple_list};
/// # egui::__run_test_ui(|ui| {
/// let items = ["AAPL", "MSFT", "NVDA"];
/// let mut selected = None;
/// simple_list(ui, &items, &mut selected, |ui, item, selected| {
///     ListItem::new()
///         .selected(selected)
///         .show_text(ui, *item)
/// });
/// # });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_advances_by_exactly_one_row() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let top = ui.cursor().top();
                let row = ListItem::new().show_text(ui, "Row");
                assert_eq!(row.rect.height(), default_height());
                assert_eq!(ui.cursor().top() - top, default_height());
            });
        });
    }
}
