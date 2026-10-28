//! Studies: the indicator dialog, chart and series settings, the context menus and symbol search.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn studies_init(&mut self) {}

    pub(super) fn studies_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn studies_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn studies_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn studies_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn studies_modal(&self) -> bool {
        false
    }
}
