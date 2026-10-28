//! Trading: the broker picker, the account panel, the bottom dock and the strategy report.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn trading_init(&mut self) {}

    pub(super) fn trading_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn trading_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn trading_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn trading_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn trading_modal(&self) -> bool {
        false
    }
}
