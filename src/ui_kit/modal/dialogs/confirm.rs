//! Confirm Dialog
//!
//! A standard confirmation dialog with confirm/cancel buttons.

use egui::{Context, Id};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonVariant};

/// Result of showing a confirm dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmResult {
    /// Dialog is still open, no action taken
    None,
    /// User confirmed the action
    Confirmed,
    /// User cancelled the action
    Cancelled,
}

impl ConfirmResult {
    /// Check if the user confirmed
    pub fn is_confirmed(&self) -> bool {
        matches!(self, ConfirmResult::Confirmed)
    }

    /// Check if the user cancelled
    pub fn is_cancelled(&self) -> bool {
        matches!(self, ConfirmResult::Cancelled)
    }

    /// Check if the dialog is still open
    pub fn is_open(&self) -> bool {
        matches!(self, ConfirmResult::None)
    }
}

/// A confirmation dialog
pub struct ConfirmDialog {
    id: Id,
    title: String,
    message: String,
    confirm_text: String,
    cancel_text: String,
    danger: bool,
}

impl ConfirmDialog {
    /// Create a new confirm dialog
    pub fn new(
        id: impl std::hash::Hash,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Id::new(id),
            title: title.into(),
            message: message.into(),
            confirm_text: "Confirm".to_string(),
            cancel_text: "Cancel".to_string(),
            danger: false,
        }
    }

    /// Set the confirm button text
    pub fn confirm_text(mut self, text: impl Into<String>) -> Self {
        self.confirm_text = text.into();
        self
    }

    /// Set the cancel button text
    pub fn cancel_text(mut self, text: impl Into<String>) -> Self {
        self.cancel_text = text.into();
        self
    }

    /// Mark this as a dangerous action (red confirm button)
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Show the confirm dialog
    ///
    /// Returns the result of the dialog interaction.
    pub fn show(self, ctx: &Context, open: &mut bool) -> ConfirmResult {
        if !*open {
            return ConfirmResult::None;
        }

        let mut result = ConfirmResult::None;

        egui::Area::new(self.id)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();

                // Dim background
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(180));

                // Backdrop click - do nothing for confirm dialogs (force explicit choice)

                // Handle Escape key as cancel
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    result = ConfirmResult::Cancelled;
                    *open = false;
                    return;
                }

                // Modal window
                egui::Window::new(&self.title)
                    .id(self.id.with("_window"))
                    .fixed_pos(screen_rect.center() - egui::vec2(175.0, 75.0))
                    .fixed_size(egui::vec2(350.0, 0.0))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(true)
                    .show(ctx, |ui| {
                        ui.add_space(DESIGN_TOKENS.spacing.md);

                        // Message
                        ui.label(&self.message);

                        ui.add_space(DESIGN_TOKENS.spacing.xl);

                        // Buttons
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Confirm button
                                    let confirm_variant = if self.danger {
                                        ButtonVariant::Danger
                                    } else {
                                        ButtonVariant::Primary
                                    };

                                    if ui
                                        .add(
                                            Button::new(&self.confirm_text)
                                                .variant(confirm_variant),
                                        )
                                        .clicked()
                                    {
                                        result = ConfirmResult::Confirmed;
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
                                        result = ConfirmResult::Cancelled;
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

/// Show a simple confirm dialog
pub fn confirm(
    ctx: &Context,
    id: impl std::hash::Hash,
    title: impl Into<String>,
    message: impl Into<String>,
    open: &mut bool,
) -> ConfirmResult {
    ConfirmDialog::new(id, title, message).show(ctx, open)
}

/// Show a danger confirm dialog
pub fn confirm_danger(
    ctx: &Context,
    id: impl std::hash::Hash,
    title: impl Into<String>,
    message: impl Into<String>,
    open: &mut bool,
) -> ConfirmResult {
    ConfirmDialog::new(id, title, message)
        .danger()
        .show(ctx, open)
}
