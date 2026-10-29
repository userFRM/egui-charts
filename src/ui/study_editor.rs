//! Study editor panel: an editing surface for host-owned study source.
//!
//! The panel holds a document title and its text, tells whether the text changed since it was
//! opened or saved, lists the host's diagnostics and sends open, save and apply requests. The
//! host owns the language, the files and the study: it compiles the text, answers with
//! [`set_diagnostics`](StudyEditor::set_diagnostics) and
//! [`apply_result`](StudyEditor::apply_result), and the edits stay in the panel whatever it
//! answers. The panel needs no other part of the library and can sit in any layout.

use crate::localization as l10n;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use egui::text::{CCursor, CCursorRange};
use egui::{Align, RichText, ScrollArea, TextEdit, Ui};
use std::ops::Range;

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Severity {
    /// A hint.
    Info,
    /// The text works but may not do what was meant.
    Warning,
    /// The text cannot be applied.
    Error,
}

/// A host message about a span of the document.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// How serious it is.
    pub severity: Severity,
    /// What the host found.
    pub message: String,
    /// The byte range of the text it concerns. A range past the text or inside a character is
    /// clamped to the text and to character boundaries.
    pub span: Range<usize>,
}

/// What the user asked of the host.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StudyEditorAction {
    /// Nothing was asked.
    None,
    /// The user wants to open another document: the host picks one and calls
    /// [`StudyEditor::open_document`]. Unsaved edits are still in the panel until it does.
    Open,
    /// The user wants the document stored: the host saves it and calls
    /// [`StudyEditor::mark_saved`].
    Save {
        /// The document title.
        title: String,
        /// The document text.
        text: String,
    },
    /// The user wants the text applied as a study: the host compiles it and calls
    /// [`StudyEditor::apply_result`].
    Apply {
        /// The document title.
        title: String,
        /// The document text.
        text: String,
    },
}

/// The host's answer to an apply request.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Applied,
    Rejected(String),
}

/// The study editor panel.
#[derive(Default)]
pub struct StudyEditor {
    title: String,
    text: String,
    /// The text as last opened or saved, to tell whether it was modified.
    saved: String,
    diagnostics: Vec<Diagnostic>,
    status: Option<Status>,
    /// A span to select and scroll to in the next frame.
    reveal: Option<Range<usize>>,
}

impl StudyEditor {
    /// An empty, untitled document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the document, clearing diagnostics and status and the modified state.
    pub fn open_document(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.title = title.into();
        self.text = text.into();
        self.saved = self.text.clone();
        self.diagnostics.clear();
        self.status = None;
        self.reveal = None;
    }

    /// The document title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The document text, with the user's edits.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text differs from the text last opened or saved.
    pub fn is_modified(&self) -> bool {
        self.text != self.saved
    }

    /// Records that the host stored `text`, which clears the modified state if the user has not
    /// edited the document since the save request.
    pub fn mark_saved(&mut self, text: &str) {
        self.saved = text.to_owned();
    }

    /// Replaces the diagnostics shown, most serious first as the host sends them.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }

    /// The diagnostics shown.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Records the host's answer to [`StudyEditorAction::Apply`]: `Ok` shows the text as applied,
    /// `Err` shows the reason. Neither changes the text.
    pub fn apply_result(&mut self, result: Result<(), String>) {
        self.status = Some(match result {
            Ok(()) => Status::Applied,
            Err(reason) => Status::Rejected(reason),
        });
    }

    /// Selects the byte `span` in the text and scrolls to it in the next frame.
    pub fn reveal(&mut self, span: Range<usize>) {
        self.reveal = Some(span);
    }

    /// The span as character offsets, clamped to the text.
    fn char_range(&self, span: &Range<usize>) -> Range<usize> {
        let at = |byte: usize| {
            let mut byte = byte.min(self.text.len());
            while !self.text.is_char_boundary(byte) {
                byte -= 1;
            }
            self.text[..byte].chars().count()
        };
        at(span.start)..at(span.end.max(span.start))
    }

    /// Shows the panel in `ui` and returns what the user asked of the host.
    pub fn show(&mut self, ui: &mut Ui) -> StudyEditorAction {
        let mut action = StudyEditorAction::None;
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            ui.label(RichText::new(l10n::text(ui.ctx(), "Title")).color(weak));
            ui.add(TextEdit::singleline(&mut self.title).desired_width(220.0));
            if self.is_modified() {
                ui.label(RichText::new(l10n::text(ui.ctx(), "Unsaved changes")).color(weak));
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if Button::new("Apply").show(ui).clicked() {
                    action = StudyEditorAction::Apply {
                        title: self.title.clone(),
                        text: self.text.clone(),
                    };
                }
                if Button::new("Save").show(ui).clicked() {
                    action = StudyEditorAction::Save {
                        title: self.title.clone(),
                        text: self.text.clone(),
                    };
                }
                if Button::new("Open").show(ui).clicked() {
                    action = StudyEditorAction::Open;
                }
            });
        });
        match &self.status {
            Some(Status::Applied) => {
                status_message(ui, MessageKind::Success, &l10n::text(ui.ctx(), "Applied"));
            }
            Some(Status::Rejected(reason)) => {
                status_message(ui, MessageKind::Error, reason);
            }
            None => {}
        }
        let editor = ui.make_persistent_id("study_editor_text");
        let height = (ui.available_height() - self.diagnostics_height(ui)).max(80.0);
        let reveal = self.reveal.take().map(|span| self.char_range(&span));
        ScrollArea::vertical()
            .id_salt("study_editor_scroll")
            .max_height(height)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut output = TextEdit::multiline(&mut self.text)
                    .id(editor)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(
                        (height / ui.text_style_height(&egui::TextStyle::Monospace)) as usize,
                    )
                    .show(ui);
                if let Some(range) = reveal {
                    let cursors =
                        CCursorRange::two(CCursor::new(range.start), CCursor::new(range.end));
                    output.state.cursor.set_char_range(Some(cursors));
                    output.state.store(ui.ctx(), editor);
                    ui.memory_mut(|memory| memory.request_focus(editor));
                    let at = output.galley.pos_from_cursor(CCursor::new(range.start));
                    ui.scroll_to_rect(
                        at.translate(output.galley_pos.to_vec2()),
                        Some(Align::Center),
                    );
                }
            });
        let mut reveal = None;
        for diagnostic in &self.diagnostics {
            let color = match diagnostic.severity {
                Severity::Error => ui.visuals().error_fg_color,
                Severity::Warning => ui.visuals().warn_fg_color,
                Severity::Info => weak,
            };
            let label = match diagnostic.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Info => "Info",
            };
            let line = ui.horizontal(|ui| {
                ui.label(RichText::new(l10n::text(ui.ctx(), label)).color(color));
                ui.add(
                    egui::Label::new(&diagnostic.message)
                        .sense(egui::Sense::click())
                        .truncate(),
                )
                .clicked()
            });
            if line.inner {
                reveal = Some(diagnostic.span.clone());
            }
        }
        if reveal.is_some() {
            self.reveal = reveal;
            ui.ctx().request_repaint();
        }
        action
    }

    /// Room kept below the text for the diagnostics list.
    fn diagnostics_height(&self, ui: &Ui) -> f32 {
        let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        row * self.diagnostics.len().min(6) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_apply_keeps_the_edits_and_a_save_clears_the_modified_state() {
        let mut editor = StudyEditor::new();
        editor.open_document("RSI cross", "plot(rsi)");
        assert!(!editor.is_modified());
        editor.text.push_str("\nplot(sma)");
        assert!(editor.is_modified());
        editor.apply_result(Err("line 2: unknown name".into()));
        assert_eq!(editor.text(), "plot(rsi)\nplot(sma)");
        assert!(editor.is_modified());
        let saved = editor.text().to_owned();
        editor.mark_saved(&saved);
        assert!(!editor.is_modified());
    }

    #[test]
    fn spans_are_clamped_to_characters_of_the_text() {
        let mut editor = StudyEditor::new();
        editor.open_document("t", "aé=1");
        // Bytes: a=0, é=1..3, '='=3, '1'=4.
        assert_eq!(editor.char_range(&(1..3)), 1..2);
        assert_eq!(editor.char_range(&(2..99)), 1..4);
    }
}
