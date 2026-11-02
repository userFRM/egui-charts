//! Trading: the broker picker, the account panel, the bottom dock and the strategy report.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::model::StrategyReport;
use egui_charts::services::UserSettings;

#[derive(Default)]
pub(super) struct State {
    strategy_report: Option<StrategyReport>,
}

impl Terminal {
    /// Shows `report` in the strategy report panel, or closes it with `None`.
    pub fn set_strategy_report(&mut self, report: Option<StrategyReport>) {
        self.trading.strategy_report = report;
    }

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
