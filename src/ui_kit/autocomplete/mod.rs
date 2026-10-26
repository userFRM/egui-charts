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

use crate::localization as l10n;
use crate::theme::ThemeExt;
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
        let inset = ui.active_theme().spacing.lg;
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
            text_edit = text_edit.hint_text(l10n::text(ui.ctx(), placeholder));
        }

        let mut response = ui.add(text_edit);
        let owns_keyboard =
            response.has_focus() || ui.memory(|m| m.had_focus_last_frame(response.id));
        let text_changed = response.changed();

        // Handle keyboard navigation
        let (move_up, move_down, confirm, escape) = ui.input(|i| {
            (
                owns_keyboard && i.key_pressed(Key::ArrowUp),
                owns_keyboard && i.key_pressed(Key::ArrowDown),
                owns_keyboard && (i.key_pressed(Key::Enter) || i.key_pressed(Key::Tab)),
                owns_keyboard && i.key_pressed(Key::Escape),
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

        let pointer_in_popup = ui
            .memory(|m| m.area_rect(popup_id.with("area")))
            .is_some_and(|rect| ui.rect_contains_pointer(rect));
        if escape || !has_suggestions || (!owns_keyboard && !pointer_in_popup) {
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

        let mut selected = None;

        // Show the popup using Area + Frame pattern
        if popup_open && has_suggestions {
            Popup::open_id(ui.ctx(), popup_id);
            let visuals = ui.visuals().clone();
            let text_x = response.rect.left() + inset;
            let area_response = egui::Area::new(popup_id.with("area"))
                .order(egui::Order::Foreground)
                .fixed_pos(response.rect.left_bottom())
                .show(ui.ctx(), |ui| {
                    let frame = egui::Frame::popup(ui.style())
                        .inner_margin(Margin::symmetric(0, ui.active_theme().spacing.sm as i8));
                    frame.show(ui, |ui| {
                        // The popup, border included, is exactly as wide as the input.
                        ui.set_width(response.rect.width() - 2.0 * frame.stroke.width);
                        ui.set_max_height(200.0);
                        ui.spacing_mut().item_spacing.y = 0.0;

                        ScrollArea::vertical().show(ui, |ui| {
                            let rows: Vec<_> = filtered
                                .iter()
                                .map(|_| {
                                    ui.allocate_exact_size(
                                        Vec2::new(
                                            ui.available_width(),
                                            DESIGN_TOKENS.sizing.list_item_height,
                                        ),
                                        Sense::click(),
                                    )
                                })
                                .collect();
                            if let Some(idx) =
                                rows.iter().position(|(_, response)| response.hovered())
                            {
                                selected_idx = idx;
                            }
                            for (idx, ((suggestion, _score), (rect, item_response))) in
                                filtered.iter().zip(rows).enumerate()
                            {
                                let is_selected = idx == selected_idx;
                                let translated_label = l10n::text(ui.ctx(), suggestion);
                                item_response.widget_info(|| {
                                    WidgetInfo::selected(
                                        Role::Button,
                                        true,
                                        is_selected,
                                        &translated_label,
                                    )
                                });
                                crate::styles::focus::draw_focus_ring(
                                    ui,
                                    &item_response,
                                    ui.active_theme().radius.sm,
                                );

                                if ui.is_rect_visible(rect) {
                                    let painter = ui.painter();
                                    if is_selected {
                                        let fill = visuals.widgets.hovered.bg_fill;
                                        painter.rect_filled(
                                            rect.shrink2(Vec2::new(
                                                ui.active_theme().spacing.sm,
                                                0.0,
                                            )),
                                            ui.active_theme().radius.sm,
                                            fill,
                                        );
                                    }
                                    painter.text(
                                        egui::pos2(text_x, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        l10n::text(ui.ctx(), suggestion),
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

        ui.ctx()
            .memory_mut(|m| m.data.insert_temp(selected_idx_id, selected_idx));

        // If a suggestion was selected, update the text
        if let Some(ref sel) = selected {
            *self.text = sel.clone();
            response.mark_changed();
            Popup::close_id(ui.ctx(), popup_id);
            ui.ctx()
                .memory_mut(|m| m.data.insert_temp(selected_idx_id, 0usize));
        }

        AutocompleteResponse {
            response,
            selected,
            is_open: Popup::is_id_open(ui.ctx(), popup_id),
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
        // Enter takes the input's focus; the next confirmation explicitly focuses it again.
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

    #[test]
    fn hovering_a_suggestion_moves_the_only_highlight_and_enter_picks_it() {
        let ctx = Context::default();
        let mut text = "a".to_owned();
        let mut input_rect = egui::Rect::NOTHING;
        let mut time = 0.0;
        let mut input_id = Id::NULL;
        let mut frame = |events: Vec<Event>| {
            let mut picked = None;
            time += 0.1;
            let output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let shown = AutocompleteEdit::new(&mut text, ["Apple", "Avocado", "Apricot"])
                        .id_salt("hover")
                        .show(ui);
                    input_rect = shown.response.rect;
                    input_id = shown.response.id;
                    picked = shown.selected;
                },
            );
            (output, picked, input_rect, input_id)
        };
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let (output, _, rect, id) = frame(vec![]);
        output.drop_without_applying_deltas();
        ctx.memory_mut(|m| m.request_focus(id));
        for _ in 0..3 {
            frame(vec![]).0.drop_without_applying_deltas();
        }
        frame(vec![key(Key::ArrowDown)])
            .0
            .drop_without_applying_deltas();
        let pointer = egui::pos2(rect.left() + 20.0, rect.bottom() + 18.0);
        frame(vec![Event::PointerMoved(pointer)])
            .0
            .drop_without_applying_deltas();
        let (output, _, _, _) = frame(vec![]);
        let hover = ctx.global_style().visuals.widgets.hovered.bg_fill;
        let highlights: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == hover => Some(r.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(highlights.len(), 1);
        assert_eq!(
            highlights[0].corner_radius,
            egui::CornerRadius::from(ctx.active_theme().radius.sm)
        );
        assert!(highlights[0].rect.left() > rect.left() + 1.0);
        let (output, picked, _, _) = frame(vec![key(Key::Enter)]);
        output.drop_without_applying_deltas();
        assert_eq!(picked.as_deref(), Some("Apple"));
    }

    #[test]
    fn a_popup_does_not_handle_keys_after_focus_moves_to_another_field() {
        let ctx = Context::default();
        let mut text = "a".to_owned();
        let mut other = String::new();
        let mut frame = |events| {
            let mut shown = None;
            let mut other_id = Id::NULL;
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    shown = Some(AutocompleteEdit::new(&mut text, ["Apple", "Avocado"]).show(ui));
                    other_id = ui.text_edit_singleline(&mut other).id;
                },
            )
            .drop_without_applying_deltas();
            (shown.unwrap(), other_id, text.clone())
        };
        let (shown, other_id, _) = frame(vec![]);
        ctx.memory_mut(|m| m.request_focus(shown.response.id));
        assert!(frame(vec![]).0.is_open);
        ctx.memory_mut(|m| m.request_focus(other_id));
        let (shown, _, text) = frame(vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(shown.selected.is_none());
        assert!(!shown.is_open);
        assert_eq!(text, "a");
    }
}
