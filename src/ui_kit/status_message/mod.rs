//! Status message display helpers.
//!
//! Provides consistent error/success/info message rendering
//! used in dialogs and forms.
//!
//! # Example
//! ```
//! # use egui_charts::ui_kit::status_message::{MessageKind, status_message};
//! # egui::__run_test_ui(|ui| {
//! # let error: Option<String> = Some("Symbol not found".into());
//! # let success: Option<String> = None;
//! if let Some(ref error) = error {
//!     status_message(ui, MessageKind::Error, error);
//! }
//! if let Some(ref msg) = success {
//!     status_message(ui, MessageKind::Success, msg);
//! }
//! # });
//! ```

use crate::theme::ThemeUiExt;
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
        MessageKind::Success => ui.theme_success(),
        MessageKind::Info => ui.style().visuals.text_color(),
    };

    ui.add_space(DESIGN_TOKENS.spacing.md);
    ui.horizontal(|ui| {
        ui.add_space(DESIGN_TOKENS.spacing.lg);
        ui.colored_label(color, text);
    });
}
