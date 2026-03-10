//! Autocomplete Widget
//!
//! A text input with autocomplete suggestions.
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::autocomplete::AutocompleteEdit;
//!
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
//! ```

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::command_palette::fuzzy_match;
use egui::{Color32, Id, Key, Popup, Response, ScrollArea, Sense, TextEdit, Ui, Vec2};

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
    pub fn id_salt(mut self, id: impl std::hash::Hash) -> Self {
        self.id_salt = Some(Id::new(id));
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
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        matches.into_iter().take(self.max_suggestions).collect()
    }

    /// Show the autocomplete widget
    pub fn show(self, ui: &mut Ui) -> AutocompleteResponse {
        let base_id = self.id_salt.unwrap_or_else(|| ui.id().with("autocomplete"));
        let popup_id = base_id.with("popup");
        let selected_idx_id = base_id.with("selected_idx");

        // Get the selected index from memory
        let mut selected_idx: usize = ui
            .ctx()
            .memory(|m| m.data.get_temp(selected_idx_id).unwrap_or(0));

        // Create the text edit
        let mut text_edit = TextEdit::singleline(self.text).desired_width(ui.available_width());

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
            let area_response = egui::Area::new(popup_id.with("area"))
                .order(egui::Order::Foreground)
                .fixed_pos(response.rect.left_bottom())
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(response.rect.width());
                        ui.set_max_height(200.0);

                        ScrollArea::vertical().show(ui, |ui| {
                            for (idx, (suggestion, _score)) in filtered.iter().enumerate() {
                                let is_selected = idx == selected_idx;

                                let bg_color = if is_selected {
                                    DESIGN_TOKENS.semantic.list_item.hovered_bg
                                } else {
                                    Color32::TRANSPARENT
                                };

                                let (rect, item_response) = ui.allocate_exact_size(
                                    Vec2::new(
                                        ui.available_width(),
                                        DESIGN_TOKENS.sizing.list_item_height,
                                    ),
                                    Sense::click(),
                                );

                                if ui.is_rect_visible(rect) {
                                    let painter = ui.painter();

                                    // Background
                                    let final_bg = if item_response.hovered() && !is_selected {
                                        DESIGN_TOKENS.semantic.list_item.active_bg
                                    } else {
                                        bg_color
                                    };
                                    painter.rect_filled(rect, 0.0, final_bg);

                                    // Text
                                    let text_color = if is_selected {
                                        DESIGN_TOKENS.semantic.ui.text_dark
                                    } else {
                                        DESIGN_TOKENS.semantic.ui.text_secondary_dark
                                    };
                                    painter.text(
                                        egui::pos2(
                                            rect.min.x + DESIGN_TOKENS.spacing.md,
                                            rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        suggestion,
                                        egui::FontId::proportional(typography::MD),
                                        text_color,
                                    );
                                }

                                if item_response.clicked() {
                                    selected = Some(suggestion.clone());
                                }
                            }
                        });

                        // Handle Enter/Tab to select
                        if confirm && !filtered.is_empty() && selected.is_none() {
                            selected = Some(filtered[selected_idx].0.clone());
                        }
                    });
                });

            // Close popup if clicked outside
            if ui.input(|i| i.pointer.any_pressed()) {
                let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                if let Some(pos) = pointer_pos {
                    if !area_response.response.rect.contains(pos) && !response.rect.contains(pos) {
                        Popup::close_id(ui.ctx(), popup_id);
                    }
                }
            }
        }

        // If a suggestion was selected, update the text
        if let Some(ref sel) = selected {
            *self.text = sel.clone();
            Popup::close_id(ui.ctx(), popup_id);
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
