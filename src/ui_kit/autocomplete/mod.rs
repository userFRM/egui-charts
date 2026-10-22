//! Autocomplete Widget
//!
//! A text input with autocomplete suggestions.
//!
//! # Usage
//!
//! ```
//! use egui_charts::ui_kit::autocomplete::AutocompleteEdit;
//!
//! # egui::__run_test_ui(|ui| {
//! let suggestions = vec!["Apple", "Banana", "Cherry", "Date"];
//! let mut text = String::new();
//!
//! let response = AutocompleteEdit::new(&mut text, suggestions)
//!     .placeholder("Search fruits...")
//!     .max_suggestions(5)
//!     .show(ui);
//!
//! if let Some(selected) = response.selected {
//!     println!("Selected: {}", selected);
//! }
//! # });
//! ```

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::command_palette::fuzzy_match;
use egui::{
    Id, Key, Margin, Popup, Response, Role, ScrollArea, Sense, TextEdit, TextStyle, Ui, Vec2,
    WidgetInfo,
};

/// Result of showing the autocomplete widget
pub struct AutocompleteResponse {
    /// The underlying text edit response
    pub response: Response,
    /// The selected suggestion if the user picked one
    pub selected: Option<String>,
    /// Whether the dropdown is currently open
    pub is_open: bool,
}

/// A text input with autocomplete suggestions
pub struct AutocompleteEdit<'a> {
    text: &'a mut String,
    suggestions: Vec<String>,
    placeholder: Option<String>,
    max_suggestions: usize,
    case_sensitive: bool,
    id_salt: Option<Id>,
}

impl<'a> AutocompleteEdit<'a> {
    /// Create a new autocomplete edit with the given text and suggestions
    pub fn new(
        text: &'a mut String,
        suggestions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            text,
            suggestions: suggestions.into_iter().map(|s| s.into()).collect(),
            placeholder: None,
            max_suggestions: 8,
            case_sensitive: false,
            id_salt: None,
        }
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set the maximum number of suggestions to show
    pub fn max_suggestions(mut self, max: usize) -> Self {
        self.max_suggestions = max;
        self
    }

    /// Set whether matching should be case-sensitive
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }

    /// Set a unique ID salt for this widget
    pub fn id_salt(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = Some(Id::unique(id));
        self
    }

    /// Filter and sort suggestions based on current text
    fn filter_suggestions(&self) -> Vec<(String, i32)> {
        if self.text.is_empty() {
            // Return all suggestions with score 0 when empty
            return self
                .suggestions
                .iter()
                .take(self.max_suggestions)
                .map(|s| (s.clone(), 0))
                .collect();
        }

        let mut matches: Vec<_> = self
            .suggestions
            .iter()
            .filter_map(|s| fuzzy_match(self.text, s).map(|m| (s.clone(), m.score)))
            .collect();

        // Sort by score (descending)
        matches.sort_by_key(|m| std::cmp::Reverse(m.1));

        matches.into_iter().take(self.max_suggestions).collect()
    }

    /// Show the autocomplete widget
    pub fn show(self, ui: &mut Ui) -> AutocompleteResponse {
        // Horizontal text inset shared by the input and its suggestions.
        let inset = DESIGN_TOKENS.spacing.lg;
        let base_id = self
            .id_salt
            .unwrap_or_else(|| ui.scope_id().with("autocomplete"));
        let popup_id = base_id.with("popup");
        let selected_idx_id = base_id.with("selected_idx");

        // Get the selected index from memory
        let mut selected_idx: usize = ui
            .ctx()
            .memory(|m| m.data.get_temp(selected_idx_id).unwrap_or(0));

        // Create the text edit
        let mut text_edit = TextEdit::singleline(self.text)
            .desired_width(ui.available_width())
            .margin(Margin {
                left: inset as i8,
                right: inset as i8,
                ..Margin::symmetric(0, 2)
            });

        if let Some(ref placeholder) = self.placeholder {
            text_edit = text_edit.hint_text(placeholder);
        }

        let response = ui.add(text_edit);
        let text_changed = response.changed();

        // Handle keyboard navigation
        let (move_up, move_down, confirm, escape) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter) || i.key_pressed(Key::Tab),
                i.key_pressed(Key::Escape),
            )
        });

        // Filter suggestions
        let filtered = self.filter_suggestions();
        let has_suggestions = !filtered.is_empty();
        // The list can shrink without a keystroke, so a stored row may be past its end.
        selected_idx = selected_idx.min(filtered.len().saturating_sub(1));

        // Determine if popup should be open
        let mut popup_open = Popup::is_id_open(ui.ctx(), popup_id);
        let should_open = (response.has_focus() || response.gained_focus())
            && has_suggestions
            && !self.text.is_empty();

        if should_open && !popup_open {
            Popup::open_id(ui.ctx(), popup_id);
            popup_open = true;
        }

        if escape || (!response.has_focus() && !popup_open) {
            Popup::close_id(ui.ctx(), popup_id);
            popup_open = false;
        }

        // Handle keyboard navigation
        if popup_open {
            if move_up && selected_idx > 0 {
                selected_idx -= 1;
            }
            if move_down && selected_idx + 1 < filtered.len() {
                selected_idx += 1;
            }
        }

        // Reset selection when text changes
        if text_changed {
            selected_idx = 0;
        }

        // Store selected index
        ui.ctx()
            .memory_mut(|m| m.data.insert_temp(selected_idx_id, selected_idx));

        let mut selected = None;

        // Show the popup using Area + Frame pattern
        if popup_open && has_suggestions {
            let visuals = ui.visuals().clone();
            let text_x = response.rect.left() + inset;
            let area_response = egui::Area::new(popup_id.with("area"))
                .order(egui::Order::Foreground)
                .fixed_pos(response.rect.left_bottom())
                .show(ui.ctx(), |ui| {
                    let frame = egui::Frame::popup(ui.style())
                        .inner_margin(Margin::symmetric(0, DESIGN_TOKENS.spacing.sm as i8));
                    frame.show(ui, |ui| {
                        // The popup, border included, is exactly as wide as the input.
                        ui.set_width(response.rect.width() - 2.0 * frame.stroke.width);
                        ui.set_max_height(200.0);
                        ui.spacing_mut().item_spacing.y = 0.0;

                        ScrollArea::vertical().show(ui, |ui| {
                            for (idx, (suggestion, _score)) in filtered.iter().enumerate() {
                                let is_selected = idx == selected_idx;
                                let (rect, item_response) = ui.allocate_exact_size(
                                    Vec2::new(
                                        ui.available_width(),
                                        DESIGN_TOKENS.sizing.list_item_height,
                                    ),
                                    Sense::click(),
                                );
                                item_response.widget_info(|| {
                                    WidgetInfo::selected(
                                        Role::Button,
                                        true,
                                        is_selected,
                                        suggestion,
                                    )
                                });

                                if ui.is_rect_visible(rect) {
                                    let painter = ui.painter();
                                    if is_selected || item_response.hovered() {
                                        let fill = visuals.widgets.hovered.bg_fill;
                                        painter.rect_filled(rect, 0.0, fill);
                                    }
                                    painter.text(
                                        egui::pos2(text_x, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        suggestion,
                                        TextStyle::Body.resolve(ui.style()),
                                        visuals.widgets.hovered.fg_stroke.color,
                                    );
                                }

                                if item_response.clicked() {
                                    selected = Some(suggestion.clone());
                                }
                            }
                        });

                        // Handle Enter/Tab to select
                        if confirm && selected.is_none() {
                            selected = filtered.get(selected_idx).map(|(s, _)| s.clone());
                        }
                    });
                });

            // Close popup if clicked outside
            if ui.input(|i| i.pointer.any_pressed()) {
                let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                if let Some(pos) = pointer_pos
                    && !area_response.response.rect.contains(pos)
                    && !response.rect.contains(pos)
                {
                    Popup::close_id(ui.ctx(), popup_id);
                }
            }
        }

        // If a suggestion was selected, update the text
        if let Some(ref sel) = selected {
            *self.text = sel.clone();
            Popup::close_id(ui.ctx(), popup_id);
            ui.ctx()
                .memory_mut(|m| m.data.insert_temp(selected_idx_id, 0usize));
        }

        AutocompleteResponse {
            response,
            selected,
            is_open: popup_open,
        }
    }
}

/// Helper function to create an autocomplete text input
pub fn autocomplete(
    ui: &mut Ui,
    text: &mut String,
    suggestions: impl IntoIterator<Item = impl Into<String>>,
) -> AutocompleteResponse {
    AutocompleteEdit::new(text, suggestions).show(ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Modifiers, RawInput};

    #[test]
    fn enter_after_a_pick_from_further_down_keeps_the_picked_text() {
        let suggestions = [
            "Apple", "Avocado", "Banana", "Grape", "Papaya", "Guava", "Mango", "Pear",
        ];
        let ctx = Context::default();
        let mut text = String::new();
        let mut frame = |events: Vec<Event>| {
            let mut shown = None;
            let input = RawInput {
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                let response = AutocompleteEdit::new(&mut text, suggestions).show(ui);
                shown = Some((response.response.id, response.selected));
            })
            .drop_without_applying_deltas();
            let (id, selected) = shown.unwrap();
            (id, selected, text.clone())
        };
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };

        // Every name contains an `a`, so typing it lists all eight and five arrows reach row 5.
        // The popup stays in memory only every other frame, and Enter takes the input's focus,
        // so each Enter follows an idle frame that reopens the popup.
        let (id, _, _) = frame(vec![]);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(vec![Event::Text("a".into())]);
        for _ in 0..5 {
            frame(vec![key(Key::ArrowDown)]);
        }
        frame(vec![]);
        let (_, picked, text_after_pick) = frame(vec![key(Key::Enter)]);
        assert!(picked.is_some());

        // The picked name matches only itself, so the list now has one row.
        ctx.memory_mut(|m| m.request_focus(id));
        frame(vec![]);
        let (_, _, text_after_enter) = frame(vec![key(Key::Enter)]);
        assert_eq!(text_after_enter, text_after_pick);
    }
}
