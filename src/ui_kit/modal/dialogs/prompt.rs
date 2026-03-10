//! Prompt Dialog
//!
//! A dialog with a text input field.

use egui::{Context, Id, TextEdit, Widget};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonVariant};

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
    // Internal state
    input_value: String,
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
            input_value: String::new(),
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
        let value = value.into();
        self.initial_value = value.clone();
        self.input_value = value;
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
    pub fn show(mut self, ctx: &Context, open: &mut bool) -> PromptResult {
        if !*open {
            return PromptResult::None;
        }

        let mut result = PromptResult::None;

        egui::Area::new(self.id)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();

                // Dim background
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(180));

                // Handle Escape key as cancel
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    result = PromptResult::Cancelled;
                    *open = false;
                    return;
                }

                // Modal window
                let width = if self.multiline { 450.0 } else { 350.0 };

                egui::Window::new(&self.title)
                    .id(self.id.with("_window"))
                    .fixed_pos(screen_rect.center() - egui::vec2(width / 2.0, 100.0))
                    .fixed_size(egui::vec2(width, 0.0))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(true)
                    .show(ctx, |ui| {
                        ui.add_space(DESIGN_TOKENS.spacing.md);

                        // Message
                        if let Some(msg) = &self.message {
                            ui.label(msg);
                            ui.add_space(DESIGN_TOKENS.spacing.md);
                        }

                        // Input field
                        let text_edit = if self.multiline {
                            TextEdit::multiline(&mut self.input_value)
                                .hint_text(&self.placeholder)
                                .desired_width(f32::INFINITY)
                                .desired_rows(4)
                        } else {
                            TextEdit::singleline(&mut self.input_value)
                                .hint_text(&self.placeholder)
                                .desired_width(f32::INFINITY)
                        };

                        let response = text_edit.ui(ui);

                        // Focus the text field on first show
                        if !response.has_focus() {
                            response.request_focus();
                        }

                        // Handle Enter key in single-line mode
                        if !self.multiline
                            && response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            result = PromptResult::Submitted(self.input_value.clone());
                            *open = false;
                            return;
                        }

                        ui.add_space(DESIGN_TOKENS.spacing.xl);

                        // Buttons
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Submit button
                                    if ui
                                        .add(
                                            Button::new(&self.submit_text)
                                                .variant(ButtonVariant::Primary),
                                        )
                                        .clicked()
                                    {
                                        result = PromptResult::Submitted(self.input_value.clone());
                                        *open = false;
                                    }

                                    ui.add_space(DESIGN_TOKENS.spacing.md);

                                    // Cancel button
                                    if ui
                                        .add(
                                            Button::new(&self.cancel_text)
                                                .variant(ButtonVariant::Secondary),
                                        )
                                        .clicked()
                                    {
                                        result = PromptResult::Cancelled;
                                        *open = false;
                                    }
                                },
                            );
                        });

                        ui.add_space(DESIGN_TOKENS.spacing.md);
                    });
            });

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
