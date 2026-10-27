//! Comparisons: compared symbols, spreads, dividend adjustment and templates.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn compare_init(&mut self) {}

    pub(super) fn compare_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn compare_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn compare_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn compare_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn compare_modal(&self) -> bool {
        false
    }
}
