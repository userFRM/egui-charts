//! Status message display helpers.
//!
//! Provides consistent error/success/warning/info message rendering
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

use crate::localization as l10n;
use crate::theme::ThemeExt;
use egui::Ui;

/// Kind of status message, determines color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    /// An operation failed.
    Error,
    /// Successful completion.
    Success,
    /// A warning requiring attention.
    Warning,
    /// Informational status.
    Info,
}

/// Show a padded, colored status message.
pub fn status_message(ui: &mut Ui, kind: MessageKind, text: &str) {
    let theme = ui.active_theme();
    let color = match kind {
        MessageKind::Error => ui.style().visuals.error_fg_color,
        MessageKind::Success => theme.ui.success,
        MessageKind::Warning => ui.style().visuals.warn_fg_color,
        MessageKind::Info => theme.ui.info,
    };

    ui.add_space(theme.spacing.sm);
    ui.colored_label(color, l10n::text(ui.ctx(), text));
}
