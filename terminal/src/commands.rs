//! Commands: keyboard requests, quick search, export and the clipboard.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn commands_init(&mut self) {}

    pub(super) fn commands_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn commands_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn commands_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn commands_settings(
        &mut self,
        _settings: &mut UserSettings,
        _direction: Direction,
    ) {
    }

    pub(super) fn commands_modal(&self) -> bool {
        false
    }
}
