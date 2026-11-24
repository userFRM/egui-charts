//! Study editor panel: an editing surface for host-owned study source.
//!
//! The panel holds a document title and its text, tells whether the text changed since it was
//! opened or saved, lists the host's diagnostics and sends open, save and apply requests. The
//! host owns the language, the files and the study: it compiles the text, answers with
//! [`set_diagnostics`](StudyEditor::set_diagnostics) and
//! [`apply_result`](StudyEditor::apply_result), and the edits stay in the panel whatever it
//! answers. A host that knows its language can also colour the text, offer completions at the
//! cursor, list example documents and note facts after lines. The panel needs no other part of
//! the library and can sit in any layout.

use crate::localization as l10n;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat, TextWrapping};
use egui::text_edit::{TextEditOutput, TextEditState};
use egui::{
    Align, Align2, AtomExt, Color32, Event, EventFilter, Frame, Galley, Id, Key, LayerId, Order,
    RichText, ScrollArea, Stroke, SurrenderFocusOn, TextBuffer, TextEdit, TextStyle, Ui, Visuals,
};
use std::ops::Range;
use std::sync::Arc;

/// Most completions the list shows.
const COMPLETIONS: usize = 12;
/// The width of the completion list.
const LIST_WIDTH: f32 = 420.0;

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

impl Severity {
    /// The colour the panel shows it in.
    fn color(self, visuals: &Visuals) -> Color32 {
        match self {
            Self::Error => visuals.error_fg_color,
            Self::Warning => visuals.warn_fg_color,
            Self::Info => visuals.weak_text_color(),
        }
    }
}

/// A host message about a span of the document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// How serious it is.
    pub severity: Severity,
    /// What the host found.
    pub message: String,
    /// The byte range of the text it concerns, underlined in the text. A range past the text or
    /// inside a character is clamped to the text and to character boundaries.
    pub span: Range<usize>,
}

impl Diagnostic {
    /// A `severity` message about the bytes `span` of the document.
    pub fn new(severity: Severity, message: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            severity,
            message: message.into(),
            span,
        }
    }
}

/// Colours for the text: byte ranges and their colours, for the current visuals. Ranges past the
/// text or inside a character are clamped; where two overlap, the one that starts first wins.
pub type Highlighter = Arc<dyn Fn(&str, &Visuals) -> Vec<(Range<usize>, Color32)> + Send + Sync>;

/// The completions for a text and the cursor's byte offset in it.
pub type Completer = Arc<dyn Fn(&str, usize) -> Vec<Completion> + Send + Sync>;

/// Byte ranges of a text with their colours.
type Colours = Vec<(Range<usize>, Color32)>;

/// One completion the panel offers at the cursor.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Completion {
    /// Shown in the list.
    pub label: String,
    /// Written at the cursor in place of the end of the text before it that begins `insert`.
    pub insert: String,
    /// Shown beside the label, such as a signature and a summary.
    pub detail: String,
}

impl Completion {
    /// A completion shown as `label` that writes `insert`, with `detail` beside it.
    pub fn new(
        label: impl Into<String>,
        insert: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            insert: insert.into(),
            detail: detail.into(),
        }
    }
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

/// The completion list while it is open.
struct Open {
    items: Vec<Completion>,
    selected: usize,
    /// The cursor's byte offset the items were asked for.
    cursor: usize,
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
    highlighter: Option<Highlighter>,
    completer: Option<Completer>,
    /// The titles and texts the Examples menu opens.
    examples: Vec<(String, String)>,
    /// Notes drawn after one-based lines, until the text changes.
    notes: Vec<(usize, String)>,
    /// The text and visuals the highlighter last ran for, with its ranges clamped and sorted.
    colours: Option<(String, Visuals, Colours)>,
    completion: Option<Open>,
}

impl std::fmt::Debug for StudyEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StudyEditor")
            .field("title", &self.title)
            .field("text", &self.text)
            .field("saved", &self.saved)
            .field("diagnostics", &self.diagnostics)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl StudyEditor {
    /// An empty, untitled document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the document, clearing diagnostics, line notes and status and the modified
    /// state.
    pub fn open_document(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.title = title.into();
        self.text = text.into();
        self.saved = self.text.clone();
        self.diagnostics.clear();
        self.status = None;
        self.reveal = None;
        self.notes.clear();
        self.completion = None;
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

    /// Colour the text with `highlighter`, called again when the text or the theme changes.
    pub fn set_highlighter(&mut self, highlighter: Highlighter) {
        self.highlighter = Some(highlighter);
        self.colours = None;
    }

    /// Offer completions from `completer` while the user types a name or a quote: the list opens
    /// below the cursor's row, or above it in the lower half of the window, the arrow keys choose,
    /// Tab or Enter write the chosen completion and Escape closes the list. While it is closed
    /// these keys edit the text as before.
    ///
    /// ```
    /// use egui_charts::ui::study_editor::{Completer, Completion, StudyEditor};
    /// use std::sync::Arc;
    ///
    /// let mut editor = StudyEditor::new();
    /// editor.set_highlighter(Arc::new(|text: &str, visuals: &egui::Visuals| {
    ///     let plots = text.match_indices("plot");
    ///     plots.map(|(at, word)| (at..at + word.len(), visuals.hyperlink_color)).collect()
    /// }));
    /// // The names that begin with the word before the cursor, a byte offset in the text.
    /// let completer: Completer = Arc::new(|text: &str, cursor: usize| {
    ///     let word = text[..cursor].rsplit(|c: char| !c.is_alphanumeric()).next();
    ///     let word = word.filter(|word| !word.is_empty());
    ///     ["plot", "close"]
    ///         .into_iter()
    ///         .filter(|name| word.is_some_and(|word| name.starts_with(word)))
    ///         .map(|name| Completion::new(name, name, "A name of the language"))
    ///         .collect()
    /// });
    /// let found = completer("x = cl", 6);
    /// assert_eq!(found, [Completion::new("close", "close", "A name of the language")]);
    /// editor.set_completer(completer);
    /// ```
    pub fn set_completer(&mut self, completer: Completer) {
        self.completer = Some(completer);
    }

    /// Titles and texts that the Examples menu opens as new documents; none hides the menu.
    pub fn set_examples(&mut self, examples: Vec<(String, String)>) {
        self.examples = examples;
    }

    /// Short notes drawn after the text of one-based lines, such as a profiler's calls and time,
    /// until the text changes or another document opens.
    pub fn set_line_notes(&mut self, notes: Vec<(usize, String)>) {
        self.notes = notes;
    }

    /// The span as character offsets, clamped to the text.
    fn char_range(&self, span: &Range<usize>) -> Range<usize> {
        let span = clamp(&self.text, span);
        let at = |byte: usize| self.text[..byte].chars().count();
        at(span.start)..at(span.end)
    }

    /// Shows the panel in `ui` and returns what the user asked of the host.
    pub fn show(&mut self, ui: &mut Ui) -> StudyEditorAction {
        let mut action = StudyEditorAction::None;
        let mut example = None;
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            let label = ui.label(RichText::new(l10n::text(ui.ctx(), "Title")).color(weak));
            ui.add(TextEdit::singleline(&mut self.title).desired_width(220.0))
                .labelled_by(label.id);
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
                if !self.examples.is_empty() {
                    ui.menu_button(l10n::text(ui.ctx(), "Examples"), |ui| {
                        for (title, text) in &self.examples {
                            if ui.button(title).clicked() {
                                example = Some((title.clone(), text.clone()));
                                ui.close();
                            }
                        }
                    });
                }
            });
        });
        if let Some((title, text)) = example {
            self.open_document(title, text);
        }
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
        let list = editor.with("completion");
        let height = (ui.available_height() - self.diagnostics_height(ui)).max(80.0);
        let reveal = self.reveal.take().map(|span| self.char_range(&span));
        let inserted = self.completion_keys(ui, editor);
        ScrollArea::vertical()
            .id_salt("study_editor_scroll")
            .max_height(height)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let underlined = (self.diagnostics.iter())
                    .any(|diagnostic| !clamp(&self.text, &diagnostic.span).is_empty());
                let styled = self.highlighter.is_some() || underlined;
                let (highlighter, colours) = (&self.highlighter, &mut self.colours);
                let diagnostics = &self.diagnostics;
                let mut layouter = |ui: &Ui, text: &dyn TextBuffer, wrap_width: f32| {
                    let text = text.as_str();
                    if let Some(highlighter) = highlighter
                        && colours.as_ref().is_none_or(|(seen, visuals, _)| {
                            seen != text || visuals != ui.visuals()
                        })
                    {
                        let mut ranges = highlighter(text, ui.visuals());
                        for (range, _) in &mut ranges {
                            *range = clamp(text, range);
                        }
                        ranges.sort_by_key(|(range, _)| range.start);
                        *colours = Some((text.to_owned(), ui.visuals().clone(), ranges));
                    }
                    let colours = colours.as_ref().map_or(&[][..], |(.., ranges)| ranges);
                    let mut underlines: Vec<_> = (diagnostics.iter())
                        .map(|diagnostic| {
                            let color = diagnostic.severity.color(ui.visuals());
                            (clamp(text, &diagnostic.span), color, diagnostic.severity)
                        })
                        .collect();
                    underlines.sort_by_key(|(.., severity)| std::cmp::Reverse(*severity));
                    layout(ui, text, colours, &underlines, wrap_width)
                };
                let mut text_edit = TextEdit::multiline(&mut self.text)
                    .id(editor)
                    .hint_text(l10n::text(ui.ctx(), "Study source"))
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(
                        (height / ui.text_style_height(&egui::TextStyle::Monospace)) as usize,
                    );
                let filter = text_edit.get_event_filter();
                if styled {
                    text_edit = text_edit.layouter(&mut layouter);
                }
                // A click on the list must not take the focus from the text first.
                let on_list = self.completion.is_some()
                    && (ui.ctx().pointer_interact_pos()).and_then(|pos| ui.ctx().layer_id_at(pos))
                        == Some(LayerId::new(Order::Foreground, list));
                let surrender = ui.options(|options| options.input_options.surrender_focus_on);
                if on_list {
                    ui.options_mut(|options| {
                        options.input_options.surrender_focus_on = SurrenderFocusOn::Never;
                    });
                }
                let mut output = text_edit.show(ui);
                if on_list {
                    ui.options_mut(|options| options.input_options.surrender_focus_on = surrender);
                }
                if let Some(range) = reveal {
                    let cursors =
                        CCursorRange::two(CCursor::new(range.start), CCursor::new(range.end));
                    output.state.cursor.set_char_range(Some(cursors));
                    output.state.clone().store(ui.ctx(), editor);
                    ui.memory_mut(|memory| memory.request_focus(editor));
                    let at = output.galley.pos_from_cursor(CCursor::new(range.start));
                    ui.scroll_to_rect(
                        at.translate(output.galley_pos.to_vec2()),
                        Some(Align::Center),
                    );
                }
                let typed = output.response.changed();
                if typed || inserted {
                    self.notes.clear();
                }
                self.paint_notes(ui, &output, weak);
                self.follow_cursor(&output, typed);
                if self.completion.is_some() {
                    // Escape closes the list instead of taking the focus from the text.
                    let filter = EventFilter {
                        escape: true,
                        ..filter
                    };
                    ui.memory_mut(|memory| memory.set_focus_lock_filter(editor, filter));
                    self.show_completion(ui, &output, list, editor);
                }
            });
        let mut reveal = None;
        for diagnostic in &self.diagnostics {
            let color = diagnostic.severity.color(ui.visuals());
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

    /// Takes the keys of the open completion list before the text sees them, while the text has
    /// the focus, and acts on them; whether a completion was written.
    fn completion_keys(&mut self, ui: &mut Ui, editor: Id) -> bool {
        let mut keys = Vec::new();
        if self.completion.is_some() && ui.memory(|memory| memory.has_focus(editor)) {
            ui.input_mut(|input| {
                input.events.retain(|event| match event {
                    Event::Key {
                        key:
                            key @ (Key::ArrowDown | Key::ArrowUp | Key::Tab | Key::Enter | Key::Escape),
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none() => {
                        keys.push(*key);
                        false
                    }
                    _ => true,
                });
            });
        }
        for key in keys {
            let Some(open) = &mut self.completion else {
                break;
            };
            match key {
                Key::ArrowDown => open.selected = (open.selected + 1).min(open.items.len() - 1),
                Key::ArrowUp => open.selected = open.selected.saturating_sub(1),
                Key::Escape => self.completion = None,
                _ => {
                    let (insert, cursor) = (open.items[open.selected].insert.clone(), open.cursor);
                    self.completion = None;
                    self.insert(ui, editor, &insert, cursor);
                    return true;
                }
            }
        }
        false
    }

    /// Writes `insert` at byte `cursor` in place of the end of the text before it that begins
    /// `insert`, so a name typed in part is completed, and puts the cursor after it.
    fn insert(&mut self, ui: &Ui, editor: Id, insert: &str, cursor: usize) {
        let cursor = clamp(&self.text, &(cursor..cursor)).start;
        let before = &self.text[..cursor];
        let start = (0..=cursor)
            .filter(|&at| before.is_char_boundary(at))
            .find(|&at| insert.starts_with(&before[at..]))
            .unwrap_or(cursor);
        self.text.replace_range(start..cursor, insert);
        let end = CCursor::new(self.text[..start + insert.len()].chars().count());
        let mut state = TextEditState::load(ui.ctx(), editor).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(end)));
        state.store(ui.ctx(), editor);
        self.notes.clear();
        ui.ctx().request_repaint();
    }

    /// Opens, refreshes or closes the completion list after the text edit ran: typing a name or a
    /// quote asks the completer, as does any edit while the list is open; moving the cursor
    /// otherwise or leaving the text closes it.
    fn follow_cursor(&mut self, output: &TextEditOutput, typed: bool) {
        let cursor = output.cursor_range.map(|range| {
            let index = range.primary.index.0;
            (self.text.char_indices().nth(index)).map_or(self.text.len(), |(at, _)| at)
        });
        let Some(cursor) = cursor.filter(|_| output.response.has_focus()) else {
            self.completion = None;
            return;
        };
        if !typed {
            if self
                .completion
                .as_ref()
                .is_some_and(|open| open.cursor != cursor)
            {
                self.completion = None;
            }
            return;
        }
        let word = (self.text[..cursor].chars().next_back())
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '"');
        if let Some(completer) = &self.completer
            && (word || self.completion.is_some())
        {
            let mut items = completer(&self.text, cursor);
            items.truncate(COMPLETIONS);
            self.completion = (!items.is_empty()).then_some(Open {
                items,
                selected: 0,
                cursor,
            });
        }
    }

    /// The completion list on the side of the cursor's row with more room, so that it never
    /// covers the row; a clicked row is written at the cursor.
    fn show_completion(&mut self, ui: &Ui, output: &TextEditOutput, list: Id, editor: Id) {
        let Some(open) = &self.completion else {
            return;
        };
        let cursor = CCursor::new(self.text[..open.cursor].chars().count());
        let row = (output.galley.pos_from_cursor(cursor)).translate(output.galley_pos.to_vec2());
        let (pivot, at) = if row.center().y > ui.ctx().content_rect().center().y {
            (Align2::LEFT_BOTTOM, row.left_top())
        } else {
            (Align2::LEFT_TOP, row.left_bottom())
        };
        let font = TextStyle::Monospace.resolve(ui.style());
        let mut clicked = None;
        egui::Area::new(list)
            .order(Order::Foreground)
            .pivot(pivot)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(LIST_WIDTH);
                    for (at, item) in open.items.iter().enumerate() {
                        let label = RichText::new(&item.label).font(font.clone());
                        let detail = RichText::new(&item.detail).weak().atom_shrink(true);
                        let button = egui::Button::selectable(at == open.selected, (label, detail))
                            .truncate()
                            .min_size(egui::vec2(ui.available_width(), 0.0));
                        if ui.add(button).clicked() {
                            clicked = Some(at);
                        }
                    }
                });
            });
        if let Some(at) = clicked
            && let Some(open) = self.completion.take()
        {
            self.insert(ui, editor, &open.items[at].insert, open.cursor);
        }
    }

    /// Paints each line note after the text of the row that ends its line.
    fn paint_notes(&self, ui: &Ui, output: &TextEditOutput, color: Color32) {
        if self.notes.is_empty() {
            return;
        }
        let font = TextStyle::Monospace.resolve(ui.style());
        let space = ui.fonts_mut(|fonts| fonts.glyph_width(&font, ' '));
        let rows = &output.galley.rows;
        let ends = (rows.iter().enumerate())
            .filter(|(at, row)| row.ends_with_newline || at + 1 == rows.len());
        for (line, (_, row)) in (1..).zip(ends) {
            if let Some((_, note)) = self.notes.iter().find(|(at, _)| *at == line) {
                let end = row.rect().right_top() + output.galley_pos.to_vec2();
                let at = end + egui::vec2(space, 0.0);
                ui.painter()
                    .text(at, Align2::LEFT_TOP, note, font.clone(), color);
            }
        }
    }

    /// Room kept below the text for the diagnostics list.
    fn diagnostics_height(&self, ui: &Ui) -> f32 {
        let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        row * self.diagnostics.len().min(6) as f32
    }
}

/// `span` clamped to `text` and to its character boundaries.
fn clamp(text: &str, span: &Range<usize>) -> Range<usize> {
    let at = |byte: usize| {
        let mut byte = byte.min(text.len());
        while !text.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    };
    at(span.start)..at(span.end.max(span.start))
}

/// `text` laid out as the text edit lays it out, in the colours of `colours` where they cover it
/// and underlined in the colour of the first of `underlines` that covers it.
fn layout(
    ui: &Ui,
    text: &str,
    colours: &[(Range<usize>, Color32)],
    underlines: &[(Range<usize>, Color32, Severity)],
    wrap_width: f32,
) -> Arc<Galley> {
    let visuals = ui.visuals();
    let base =
        (visuals.override_text_color).unwrap_or_else(|| visuals.widgets.inactive.text_color());
    let font = TextStyle::Monospace.resolve(ui.style());
    let line_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font)) + ui.spacing().extra_text_line_spacing;
    let format = |color, underline| TextFormat {
        font_id: font.clone(),
        color,
        underline,
        line_height: Some(line_height),
        ..Default::default()
    };
    let mut cuts: Vec<_> = (colours.iter().map(|(range, _)| range))
        .chain(underlines.iter().map(|(range, ..)| range))
        .flat_map(|range| [range.start, range.end])
        .chain([0, text.len()])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut job = LayoutJob {
        wrap: TextWrapping {
            max_width: wrap_width,
            ..Default::default()
        },
        keep_trailing_whitespace: true,
        ..Default::default()
    };
    for piece in cuts.windows(2) {
        let covers = |range: &Range<usize>| range.start <= piece[0] && piece[1] <= range.end;
        let color = (colours.iter().find(|(range, _)| covers(range))).map_or(base, |(_, c)| *c);
        let underline = (underlines.iter().find(|(range, ..)| covers(range)))
            .map_or(Stroke::NONE, |(_, color, _)| Stroke::new(1.0, *color));
        job.append(&text[piece[0]..piece[1]], 0.0, format(color, underline));
    }
    if job.sections.is_empty() {
        job.append("", 0.0, format(base, Stroke::NONE));
    }
    ui.fonts_mut(|fonts| fonts.layout_job(job))
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

    /// A harness showing `editor` in a 600 by 400 point window.
    fn harness(editor: StudyEditor) -> egui_kittest::Harness<'static, StudyEditor> {
        egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(
                |ui, editor: &mut StudyEditor| {
                    editor.show(ui);
                },
                editor,
            )
    }

    /// The text shapes of the last frame.
    fn texts(output: &egui::FullOutput) -> Vec<&egui::epaint::TextShape> {
        let shapes = output.shapes.iter();
        shapes
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hooks_colour_underline_and_annotate_the_text() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mut editor = StudyEditor::new();
        editor.open_document("t", "plot(x)\nbad");
        editor.set_highlighter(Arc::new(|_: &str, _: &Visuals| vec![(0..4, Color32::RED)]));
        editor.set_diagnostics(vec![Diagnostic::new(Severity::Error, "unknown", 8..11)]);
        editor.set_line_notes(vec![(1, "3 calls".into()), (9, "gone".into())]);
        let mut h = harness(editor);
        h.run();
        let error = h.ctx.global_style().visuals.error_fg_color;
        let shapes = texts(h.output());
        let text = (shapes.iter())
            .find(|shape| shape.galley.text() == "plot(x)\nbad")
            .expect("the document is drawn");
        let sections = (text.galley.job.sections.iter()).map(|section| {
            let bytes = section.byte_range.start.0..section.byte_range.end.0;
            (bytes, &section.format)
        });
        let sections: Vec<_> = sections.collect();
        assert!(
            (sections.iter())
                .any(|(bytes, format)| *bytes == (0..4) && format.color == Color32::RED)
        );
        let underlined: Vec<_> = (sections.iter())
            .filter(|(_, format)| format.underline != Stroke::NONE)
            .map(|(bytes, format)| (bytes.clone(), format.underline.color))
            .collect();
        assert_eq!(underlined, [(8..11, error)]);
        let notes: Vec<_> = (shapes.iter())
            .filter(|shape| shape.galley.text() == "3 calls")
            .collect();
        assert_eq!(notes.len(), 1);
        let first_row = text.galley.rows[0].rect().translate(text.pos.to_vec2());
        assert!(first_row.y_range().contains(notes[0].pos.y));
        assert!(!shapes.iter().any(|shape| shape.galley.text() == "gone"));

        h.get_by_role(Role::MultilineTextInput).focus();
        h.run();
        h.get_by_role(Role::MultilineTextInput).type_text("z");
        h.run();
        let shapes = texts(h.output());
        assert!(
            shapes
                .iter()
                .any(|shape| shape.galley.text() == "plot(x)\nbadz")
        );
        assert!(!shapes.iter().any(|shape| shape.galley.text() == "3 calls"));
    }

    #[test]
    fn completion_opens_moves_inserts_and_closes_with_keys() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mut editor = StudyEditor::new();
        editor.set_completer(Arc::new(|text: &str, cursor: usize| {
            let before = &text[..cursor];
            let names: &[&str] = if before.ends_with("pl") {
                &["plot", "plots"]
            } else if before.ends_with("study(\"RSI Di") {
                &["RSI Divergence Indicator"]
            } else {
                &[]
            };
            let names = names.iter();
            names
                .map(|name| Completion::new(*name, *name, "listed"))
                .collect()
        }));
        let mut h = harness(editor);
        h.get_by_role(Role::MultilineTextInput).focus();
        h.run();
        // Each row of the open list draws its detail.
        let rows = |h: &egui_kittest::Harness<'_, StudyEditor>| {
            let shapes = texts(h.output());
            (shapes.iter())
                .filter(|shape| shape.galley.text() == "listed")
                .count()
        };
        let type_text = |h: &mut egui_kittest::Harness<'_, StudyEditor>, text: &str| {
            h.event(Event::Text(text.into()));
            h.run();
        };
        let press = |h: &mut egui_kittest::Harness<'_, StudyEditor>, keys: &[Key]| {
            for key in keys {
                h.key_press(*key);
            }
            h.run();
        };

        type_text(&mut h, "pl");
        assert_eq!(rows(&h), 2);
        press(&mut h, &[Key::ArrowDown, Key::Tab]);
        assert_eq!((h.state().text(), rows(&h)), ("plots", 0));
        type_text(&mut h, " pl");
        assert_eq!(rows(&h), 2);
        press(&mut h, &[Key::ArrowDown, Key::ArrowUp, Key::Enter]);
        assert_eq!((h.state().text(), rows(&h)), ("plots plot", 0));
        // Escape closes the list and keeps the focus, and the keys edit the text again.
        type_text(&mut h, " pl");
        press(&mut h, &[Key::Escape]);
        assert_eq!((h.state().text(), rows(&h)), ("plots plot pl", 0));
        press(&mut h, &[Key::Enter]);
        assert_eq!(h.state().text(), "plots plot pl\n");
        type_text(&mut h, "pl");
        assert_eq!(rows(&h), 2);
        press(&mut h, &[Key::ArrowLeft]);
        assert_eq!((h.state().text(), rows(&h)), ("plots plot pl\npl", 0));
        // A name with spaces replaces what was typed of it, not only the last word.
        h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
        type_text(&mut h, "study(\"RSI Di");
        assert_eq!(rows(&h), 1);
        press(&mut h, &[Key::Enter]);
        assert_eq!(h.state().text(), "study(\"RSI Divergence Indicator");
    }
}
