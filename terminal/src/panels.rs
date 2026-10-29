//! Panels: the widget bar, the side panels, the market panels and the notification log.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::services::UserSettings;

#[derive(Default)]
pub(super) struct State {}

impl Terminal {
    pub(super) fn panels_init(&mut self) {}

    pub(super) fn panels_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn panels_show(&mut self, _ui: &mut egui::Ui, _region: Region) {}

    pub(super) fn panels_perform(&mut self, action: Action) -> Option<Action> {
        Some(action)
    }

    pub(super) fn panels_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {}

    pub(super) fn panels_modal(&self) -> bool {
        false
    }
}
