//! Prompt Dialog
//!
//! A dialog with a text input field.

use egui::{Context, Id, TextEdit, Widget};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;

/// Result of showing a prompt dialog
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptResult {
    /// Dialog is still open, no action taken
    None,
    /// User submitted the input
    Submitted(String),
    /// User cancelled the dialog
    Cancelled,
}

impl PromptResult {
    /// Get the submitted value if any
    pub fn submitted(&self) -> Option<&str> {
        match self {
            PromptResult::Submitted(s) => Some(s),
            _ => None,
        }
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        matches!(self, PromptResult::Cancelled)
    }

    /// Check if the dialog is still open
    pub fn is_open(&self) -> bool {
        matches!(self, PromptResult::None)
    }
}

/// A prompt dialog with text input
pub struct PromptDialog {
    id: Id,
    title: String,
    message: Option<String>,
    placeholder: String,
    initial_value: String,
    submit_text: String,
    cancel_text: String,
    multiline: bool,
}

impl PromptDialog {
    /// Create a new prompt dialog
    pub fn new(id: impl std::hash::Hash, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id),
            title: title.into(),
            message: None,
            placeholder: String::new(),
            initial_value: String::new(),
            submit_text: "OK".to_string(),
            cancel_text: "Cancel".to_string(),
            multiline: false,
        }
    }

    /// Set a message to display above the input
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set the initial value
    pub fn initial_value(mut self, value: impl Into<String>) -> Self {
        self.initial_value = value.into();
        self
    }

    /// Set the submit button text
    pub fn submit_text(mut self, text: impl Into<String>) -> Self {
        self.submit_text = text.into();
        self
    }

    /// Set the cancel button text
    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel_text = text.into();
        self
    }

    /// Make the input multiline
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    /// Show the prompt dialog
    ///
    /// The typed text is kept between frames while the dialog is open. Enter in a single-line
    /// input submits; Escape, the close button and a click on the backdrop cancel.
    pub fn show(self, ctx: &Context, open: &mut bool) -> PromptResult {
        let value_id = self.id.with("value");
        if !*open {
            ctx.data_mut(|d| d.remove::<String>(value_id));
            return PromptResult::None;
        }
        let mut value = ctx
            .data_mut(|d| d.remove_temp::<String>(value_id))
            .unwrap_or(self.initial_value);

        let width = if self.multiline {
            DESIGN_TOKENS.sizing.dialog.prompt_width_lg
        } else {
            DESIGN_TOKENS.sizing.dialog.prompt_width_sm
        };

        let Some(response) = Modal::new(self.id, &self.title)
            .width(width)
            .show(ctx, open, |ui| {
                if let Some(msg) = &self.message {
                    ui.label(msg);
                    ui.add_space(DESIGN_TOKENS.spacing.sm);
                }

                let text_edit = if self.multiline {
                    TextEdit::multiline(&mut value).desired_rows(4)
                } else {
                    TextEdit::singleline(&mut value)
                };
                let input = text_edit
                    .hint_text(&self.placeholder)
                    .desired_width(f32::INFINITY)
                    .ui(ui);
                // Enter in a single-line input surrenders focus; read that before refocusing.
                let entered = !self.multiline
                    && input.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if !entered && !input.has_focus() {
                    input.request_focus();
                }

                let (submit, cancel) = DialogFooter::new(&self.submit_text)
                    .secondary(&self.cancel_text)
                    .show(ui);
                (submit || entered, cancel)
            })
        else {
            return PromptResult::None;
        };

        let result = match response.inner {
            (true, _) => PromptResult::Submitted(value),
            (_, true) => PromptResult::Cancelled,
            _ if response.close_requested => PromptResult::Cancelled,
            _ => {
                ctx.data_mut(|d| d.insert_temp(value_id, value));
                PromptResult::None
            }
        };
        if result != PromptResult::None {
            *open = false;
        }
        result
    }
}

/// Show a simple prompt dialog
pub fn prompt(
    ctx: &Context,
    id: impl std::hash::Hash,
    title: impl Into<String>,
    open: &mut bool,
) -> PromptResult {
    PromptDialog::new(id, title).show(ctx, open)
}

/// Show a prompt dialog with a message
pub fn prompt_with_message(
    ctx: &Context,
    id: impl std::hash::Hash,
    title: impl Into<String>,
    message: impl Into<String>,
    open: &mut bool,
) -> PromptResult {
    PromptDialog::new(id, title)
        .message(message)
        .show(ctx, open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    #[test]
    fn enter_submits_the_typed_text() {
        let ctx = Context::default();
        let mut open = true;
        let mut frame = |events: Vec<Event>| {
            let mut result = PromptResult::None;
            let input = RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                result = PromptDialog::new("p", "Name").show(ui.ctx(), &mut open)
            });
            result
        };
        frame(vec![]); // the input takes focus
        frame(vec![Event::Text("x".into())]);
        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        assert_eq!(frame(vec![enter]), PromptResult::Submitted("x".into()));
        assert!(!open);
    }
}
