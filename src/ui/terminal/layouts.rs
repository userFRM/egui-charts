//! Layouts: chart slots, layouts and the layout tabs.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn layouts_init(&mut self) {}

    pub(super) fn layouts_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn layouts_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn layouts_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn layouts_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn layouts_modal(&self) -> bool {
        false
    }
}
