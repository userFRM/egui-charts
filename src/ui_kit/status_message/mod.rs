//! Status message display helpers.
//!
//! Provides consistent error/success/info message rendering
//! used in dialogs and forms.
//!
//! # Example
//! ```ignore
//! if let Some(ref error) = state.error {
//!     status_message(ui, MessageKind::Error, error);
//! }
//! if let Some(ref msg) = state.success {
//!     status_message(ui, MessageKind::Success, msg);
//! }
//! ```

use crate::tokens::DESIGN_TOKENS;
use egui::Ui;

/// Kind of status message, determines color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Error,
    Success,
    Info,
}

/// Show a padded, colored status message.
pub fn status_message(ui: &mut Ui, kind: MessageKind, text: &str) {
    let color = match kind {
        MessageKind::Error => ui.style().visuals.error_fg_color,
        MessageKind::Success => ui.style().visuals.selection.bg_fill,
        MessageKind::Info => ui.style().visuals.text_color(),
    };

    ui.add_space(DESIGN_TOKENS.spacing.md);
    ui.horizontal(|ui| {
        ui.add_space(DESIGN_TOKENS.spacing.lg);
        ui.colored_label(color, text);
    });
}
