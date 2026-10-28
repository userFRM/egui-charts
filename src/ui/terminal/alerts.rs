//! Alerts: the alerts panel, the alert dialog and the alert levels on the chart.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn alerts_init(&mut self) {}

    pub(super) fn alerts_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn alerts_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn alerts_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn alerts_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {}

    pub(super) fn alerts_modal(&self) -> bool {
        false
    }
}
