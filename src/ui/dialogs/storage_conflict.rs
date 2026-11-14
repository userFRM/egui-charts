//! The dialogs that settle a write the host refused because the stored document changed, and
//! that offer the copies kept of a chart's drawings.

use crate::Chart;
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

/// Offers the copies of a chart's drawings that "Keep both" in a [`StorageConflictDialog`] kept
/// ([`ConflictChoice::KeepLocalCopy`]), for the symbol the chart shows: load one, which replaces
/// the drawings on the chart as one step that undo reverts, or discard one once the user
/// confirms. Open it with [`Self::open`], from a menu or after a copy was kept, and call
/// [`Self::show`] every frame. The chart needs storage, set with [`Chart::set_storage`].
///
/// ```no_run
/// # use egui_charts::Chart;
/// # use egui_charts::ui::dialogs::LocalCopiesDialog;
/// # fn frame(ctx: &egui::Context, chart: &mut Chart, copies: &mut LocalCopiesDialog, menu: bool) {
/// if menu {
///     copies.open(chart);
/// }
/// copies.show(ctx, chart);
/// # }
/// ```
#[derive(Debug, Default)]
pub struct LocalCopiesDialog {
    open: bool,
    /// The symbol whose copies were last asked for.
    symbol: String,
    /// The copy whose discard waits for the user's confirmation.
    discarding: Option<String>,
}

impl LocalCopiesDialog {
    /// Opens the dialog and asks `chart`'s storage for the kept copies of its symbol's drawings.
    pub fn open(&mut self, chart: &mut Chart) {
        self.open = true;
        self.discarding = None;
        self.symbol = chart.symbol().to_owned();
        chart.list_local_copies();
    }

    /// Whether the dialog is shown.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows the dialog while it is open; call it every frame. Loading a copy closes it, and
    /// failures are reported through [`Chart::take_storage_error`].
    pub fn show(&mut self, ctx: &egui::Context, chart: &mut Chart) {
        if !self.open {
            return;
        }
        let symbol = chart.symbol().to_owned();
        // Another symbol shown while open, for example by a linked chart: list its copies.
        if symbol != self.symbol {
            self.discarding = None;
            self.symbol.clone_from(&symbol);
            chart.list_local_copies();
        }
        let copies = chart.local_copies().map(<[String]>::to_vec);
        let (mut load, mut discard) = (None, None);
        let discarding = &mut self.discarding;
        Modal::new("local_copies", "Kept copies of drawings")
            .width(440.0)
            .show(ctx, &mut self.open, |ui| {
                let values = [("symbol", symbol.clone())];
                ui.label(l10n::format_text(
                    ui.ctx(),
                    "Copies of the {symbol} drawings kept when another window or device changed \
                     them. Loading one replaces the drawings on the chart; Undo brings them back.",
                    &values,
                ));
                ui.add_space(ui.active_theme().spacing.md);
                let Some(copies) = &copies else {
                    ui.label(l10n::text(ui.ctx(), "Looking for kept copies…"));
                    return;
                };
                if copies.is_empty() {
                    let text = "No copies of the {symbol} drawings are kept.";
                    ui.label(l10n::format_text(ui.ctx(), text, &values));
                }
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        for name in copies {
                            ui.add(egui::Label::new(name).truncate())
                                .on_hover_text(name);
                            if discarding.as_deref() == Some(name.as_str()) {
                                let text = "Discard this copy? This cannot be undone.";
                                ui.label(l10n::text(ui.ctx(), text));
                                ui.horizontal(|ui| {
                                    if Button::new("Discard").danger().show(ui).clicked() {
                                        discard = Some(name.clone());
                                    }
                                    if Button::new("Cancel").ghost().show(ui).clicked() {
                                        *discarding = None;
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    if Button::new("Load").secondary().show(ui).clicked() {
                                        load = Some(name.clone());
                                    }
                                    if Button::new("Discard").ghost().show(ui).clicked() {
                                        *discarding = Some(name.clone());
                                    }
                                });
                            }
                        }
                    });
            });
        if let Some(name) = discard {
            self.discarding = None;
            chart.discard_local_copy(&name);
        }
        if let Some(name) = load {
            self.open = false;
            chart.load_local_copy(&name);
        }
        if !self.open {
            self.discarding = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::persistence::DrawingStorage;
    use crate::model::{Bar, BarData};
    use crate::services::document::tests::Revised;
    use chrono::{TimeZone, Utc};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Arc;

    /// Guards #1604: opening lists the kept copies, a discard waits for the user's confirmation,
    /// and loading a copy closes the dialog.
    #[test]
    fn a_kept_copy_is_discarded_only_once_confirmed() {
        let host = Arc::new(Revised::default());
        let empty = DrawingStorage::new("A").to_json().unwrap();
        for name in ["A", "A (local copy)", "A (local copy 2)"] {
            host.put(name, &empty);
        }
        let bar = Bar::new(Utc.timestamp_opt(0, 0).unwrap(), 10.0, 11.0, 9.0, 10.0, 1.0);
        let mut chart = Chart::new(BarData::from_bars(vec![bar]));
        chart.set_symbol("A");
        chart.set_storage(host.clone());
        let mut dialog = LocalCopiesDialog::default();
        dialog.open(&mut chart);
        assert!(chart.flush_stored_drawings());
        let mut h = Harness::builder()
            .with_context_setup(|ctx| {
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                egui_extras::install_image_loaders(ctx);
            })
            .build_ui_state(
                |ui, state: &mut (LocalCopiesDialog, Chart)| state.0.show(ui.ctx(), &mut state.1),
                (dialog, chart),
            );
        h.run();
        // The first row's Discard, then the first Discard in the tree: its confirmation.
        for confirmed in [false, true] {
            h.query_all_by_label("Discard").next().unwrap().click();
            h.run();
            assert!(h.state_mut().1.flush_stored_drawings());
            assert_eq!(host.get("A (local copy)").is_none(), confirmed);
        }
        // The modal centres on its new height a frame after the row left.
        h.run_steps(2);
        h.get_by_label("Load").click();
        h.run();
        assert!(!h.state().0.is_open());
    }
}
