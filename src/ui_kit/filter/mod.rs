//! Filter Widget
//!
//! A text input with filtering capabilities for lists.
//!
//! # Usage
//!
//! ```
//! use egui_charts::ui_kit::filter::FilterWidget;
//!
//! // Create filter widget
//! let mut filter = FilterWidget::new();
//! # let items = ["AAPL", "MSFT", "NVDA"];
//!
//! # egui::__run_test_ui(|ui| {
//! // Show the filter input
//! filter.show(ui);
//!
//! // Filter your items
//! for item in items.iter().filter(|name| filter.matches(name)) {
//!     ui.label(*item);
//! }
//! # });
//! ```

use egui::{Margin, Rect, Response, TextEdit, Ui, UiBuilder, Vec2, Widget, pos2};

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};

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
        let response = search_field(
            ui,
            &mut self.query,
            &self.placeholder,
            self.show_icon,
            self.show_clear_button,
        );
        FilterResponse {
            response,
            changed: self.query != old_query,
        }
    }
}

/// Stateless filter input that operates on an external string
pub fn filter_input(ui: &mut Ui, query: &mut String) -> FilterResponse {
    let old_query = query.clone();
    let response = search_field(ui, query, "Filter...", true, true);
    FilterResponse {
        response,
        changed: *query != old_query,
    }
}

/// A full-width text field with the search icon and the clear button inside its frame.
/// Escape clears the query while the field has focus.
fn search_field(
    ui: &mut Ui,
    query: &mut String,
    placeholder: &str,
    show_icon: bool,
    show_clear: bool,
) -> Response {
    let icon_size = DESIGN_TOKENS.sizing.icon_sm;
    let inset = DESIGN_TOKENS.spacing.lg;
    let icon_slot = inset + icon_size + DESIGN_TOKENS.spacing.sm;
    let tint = ui.visuals().widgets.noninteractive.fg_stroke.color;

    let response = ui
        .scope(|ui| {
            // The field keeps its size when hovered or focused.
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.hovered.expansion = 0.0;
            widgets.active.expansion = 0.0;

            let margin = |icon: bool| (if icon { icon_slot } else { inset }) as i8;
            TextEdit::singleline(query)
                .hint_text(placeholder)
                .desired_width(f32::INFINITY)
                .min_size(Vec2::new(0.0, ui.spacing().interact_size.y))
                .margin(Margin {
                    left: margin(show_icon),
                    right: margin(show_clear),
                    ..Margin::symmetric(0, 2)
                })
                .ui(ui)
        })
        .inner
        .accessible_name(placeholder);

    let icon_rect = |center_x: f32| {
        Rect::from_center_size(
            pos2(center_x, response.rect.center().y),
            Vec2::splat(icon_size),
        )
    };
    if show_icon {
        icons::QUICK_SEARCH
            .as_image_tinted(Vec2::splat(icon_size), tint)
            .paint_at(
                ui,
                icon_rect(response.rect.left() + inset + icon_size / 2.0),
            );
    }
    if show_clear && !query.is_empty() {
        let rect = icon_rect(response.rect.right() - inset - icon_size / 2.0);
        let clear = ui
            .scope_builder(UiBuilder::new().max_rect(rect.expand(4.0)), |ui| {
                IconButton::new(&icons::CLOSE)
                    .size(ButtonSize::SM)
                    .tooltip("Clear filter")
                    .show(ui)
            })
            .inner;
        if clear.clicked() {
            query.clear();
        }
    }

    if (response.has_focus() || ui.memory(|m| m.had_focus_last_frame(response.id)))
        && !query.is_empty()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        query.clear();
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Key, Modifiers, RawInput};

    #[test]
    fn escape_clears_only_the_focused_filter() {
        for (focused, moved) in [(false, false), (true, false), (false, true)] {
            let ctx = Context::default();
            let mut query = "RSI".to_owned();
            let mut id = egui::Id::NULL;
            ctx.run_ui(RawInput::default(), |ui| {
                id = filter_input(ui, &mut query).response.id;
            })
            .drop_without_applying_deltas();
            if focused || moved {
                ctx.memory_mut(|m| m.request_focus(id));
                ctx.run_ui(RawInput::default(), |ui| {
                    filter_input(ui, &mut query);
                })
                .drop_without_applying_deltas();
            }
            if moved {
                ctx.memory_mut(|m| m.request_focus(egui::Id::unique("another field")));
            }
            let input = RawInput {
                events: vec![Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                filter_input(ui, &mut query);
            })
            .drop_without_applying_deltas();
            assert_eq!(
                query,
                if focused { "" } else { "RSI" },
                "focused: {focused}, moved: {moved}"
            );
        }
    }
}
