//! Confirm Dialog
//!
//! A standard confirmation dialog with confirm/cancel buttons.

use crate::localization as l10n;
use egui::{Context, Id};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;

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
        id: impl std::hash::Hash + std::fmt::Debug,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Id::unique(id),
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

    /// Mark this as a dangerous action (confirm button in the danger colour)
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Show the confirm dialog
    ///
    /// Returns the result of the dialog interaction. Escape, the close button and a click on
    /// the backdrop cancel.
    pub fn show(self, ctx: &Context, open: &mut bool) -> ConfirmResult {
        let footer = DialogFooter::new(&self.confirm_text).secondary(&self.cancel_text);
        let footer = if self.danger { footer.danger() } else { footer };

        let Some(response) = Modal::new(self.id, &self.title)
            .width(DESIGN_TOKENS.sizing.dialog.confirm_width)
            .show(ctx, open, |ui| {
                ui.label(l10n::text(ui.ctx(), &self.message));
                footer.show(ui)
            })
        else {
            return ConfirmResult::None;
        };

        let result = match response.inner {
            (true, _) => ConfirmResult::Confirmed,
            (_, true) => ConfirmResult::Cancelled,
            _ if response.close_requested => ConfirmResult::Cancelled,
            _ => ConfirmResult::None,
        };
        if result != ConfirmResult::None {
            *open = false;
        }
        result
    }
}

/// Show a simple confirm dialog
pub fn confirm(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: impl Into<String>,
    message: impl Into<String>,
    open: &mut bool,
) -> ConfirmResult {
    ConfirmDialog::new(id, title, message).show(ctx, open)
}

/// Show a danger confirm dialog
pub fn confirm_danger(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: impl Into<String>,
    message: impl Into<String>,
    open: &mut bool,
) -> ConfirmResult {
    ConfirmDialog::new(id, title, message)
        .danger()
        .show(ctx, open)
}
