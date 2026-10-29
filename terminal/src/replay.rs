//! Replay: the replay start picker and the floating replay control across the chart slots.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn replay_init(&mut self) {}

    pub(super) fn replay_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn replay_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn replay_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn replay_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {}

    pub(super) fn replay_modal(&self) -> bool {
        false
    }
}
