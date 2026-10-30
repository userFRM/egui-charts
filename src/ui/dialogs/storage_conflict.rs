//! The dialog that settles a write the host refused because the stored document changed.

use crate::localization as l10n;
use crate::services::ConflictChoice;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;

/// Asks how to settle a write the host refused because another client changed the document:
/// keep both versions, reload the stored one or overwrite it. It has no close button and ignores
/// Escape, since the unsaved document waits for a choice; Enter keeps both.
///
/// ```no_run
/// # use egui_charts::services::SettingsStore;
/// # use egui_charts::ui::dialogs::StorageConflictDialog;
/// # fn frame(ctx: &egui::Context, settings: &mut SettingsStore) {
/// if settings.conflict()
///     && let Some(choice) = StorageConflictDialog::new(
///         "Settings changed elsewhere",
///         "Another window or device saved the settings while this window had unsaved changes.",
///     )
///     .show(ctx)
/// {
///     settings.resolve_conflict(choice);
/// }
/// # }
/// ```
pub struct StorageConflictDialog {
    title: String,
    message: String,
}

impl StorageConflictDialog {
    /// A dialog titled `title` that describes the conflict with `message`, both in English and
    /// translated when shown.
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }

    /// Shows the dialog; call it every frame while the conflict is unsettled. Returns the user's
    /// choice in the frame it is made.
    pub fn show(self, ctx: &egui::Context) -> Option<ConflictChoice> {
        let mut overwrite = false;
        let response = Modal::new(("storage_conflict", &self.title), &self.title)
            .closable(false)
            .show(ctx, &mut true, |ui| {
                ui.label(l10n::text(ui.ctx(), &self.message));
                ui.add_space(ui.active_theme().spacing.md);
                for choice in [
                    "Keep both saves your changes as a copy, then loads the other version.",
                    "Reload loads the other version and discards your changes.",
                    "Overwrite replaces the other version with your changes.",
                ] {
                    ui.label(l10n::text(ui.ctx(), choice));
                }
                DialogFooter::new("Keep both")
                    .secondary("Reload")
                    .show_with_left(ui, |ui| {
                        overwrite = Button::new("Overwrite").ghost().show(ui).clicked();
                    })
            })?;
        match response.inner {
            (true, _) => Some(ConflictChoice::KeepLocalCopy),
            (_, true) => Some(ConflictChoice::ReloadRemote),
            _ => overwrite.then_some(ConflictChoice::Retry),
        }
    }
}
