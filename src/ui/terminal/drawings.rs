//! Drawings: the drawing toolbar, the object tree, the data window and drawing properties.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn drawings_init(&mut self) {}

    pub(super) fn drawings_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn drawings_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn drawings_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn drawings_settings(
        &mut self,
        _settings: &mut UserSettings,
        _direction: Direction,
    ) {
    }

    pub(super) fn drawings_modal(&self) -> bool {
        false
    }
}
