//! Time: intervals, date ranges, go to date, the time zone and the session.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn time_init(&mut self) {}

    pub(super) fn time_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn time_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn time_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn time_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {}

    pub(super) fn time_modal(&self) -> bool {
        false
    }
}
