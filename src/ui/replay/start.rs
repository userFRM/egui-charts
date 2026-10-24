//! Start selection intents; the host supplies history, randomness and saved sessions.

use crate::ui_kit::{buttons::Button, modal::Modal};
use chrono::{DateTime, NaiveDate, Utc};

/// A replay start requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayStartAction {
    /// No selection, including cancellation.
    #[default]
    None,
    /// Select the start by clicking a chart bar.
    SelectBar,
    /// First available source bar.
    First,
    /// A random source bar selected by the host.
    Random,
    /// First source bar opening on or after this UTC instant.
    Date(DateTime<Utc>),
    /// Restore the host's saved session.
    Resume,
}

/// Start selection dialog shared by hosts of the replay controller.
#[derive(Default)]
pub struct ReplayStartPicker {
    /// Whether the dialog is open.
    pub is_open: bool,
    date: String,
}

impl ReplayStartPicker {
    /// Opens start selection with an optional initial UTC date.
    pub fn open(&mut self, date: Option<DateTime<Utc>>) {
        self.date = date.map_or_else(String::new, |time| time.format("%Y-%m-%d").to_string());
        self.is_open = true;
    }

    /// Shows start choices. Resume is enabled only when the host has a saved session.
    pub fn show(&mut self, ctx: &egui::Context, can_resume: bool) -> ReplayStartAction {
        if !self.is_open {
            return ReplayStartAction::None;
        }
        let mut action = ReplayStartAction::None;
        Modal::new("replay_start", "Start replay")
            .width(360.0)
            .show(ctx, &mut self.is_open, |ui| {
                ui.label("Choose where to begin. Playback starts paused.");
                ui.add_space(8.0);
                for (label, choice) in [
                    ("Select a bar on the chart", ReplayStartAction::SelectBar),
                    ("First available bar", ReplayStartAction::First),
                    ("Random bar", ReplayStartAction::Random),
                ] {
                    if Button::new(label).secondary().show(ui).clicked() {
                        action = choice;
                    }
                }
                ui.separator();
                ui.label("Date (UTC, YYYY-MM-DD)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.date).desired_width(ui.available_width()),
                );
                let date = NaiveDate::parse_from_str(self.date.trim(), "%Y-%m-%d")
                    .ok()
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
                    .map(|date| date.and_utc());
                if Button::new("Start at date")
                    .secondary()
                    .enabled(date.is_some())
                    .show(ui)
                    .clicked()
                    && let Some(date) = date
                {
                    action = ReplayStartAction::Date(date);
                }
                ui.separator();
                if Button::new("Resume saved session")
                    .secondary()
                    .enabled(can_resume)
                    .show(ui)
                    .clicked()
                {
                    action = ReplayStartAction::Resume;
                }
            });
        if action != ReplayStartAction::None {
            self.is_open = false;
        }
        action
    }
}
