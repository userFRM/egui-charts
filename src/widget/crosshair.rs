//! Crosshair snapping and study legend actions shared by the chart panes.

use super::Chart;
use crate::chart::renderers::{StudyAction, snap_to_values};
use crate::config::CrosshairMode;
use crate::scales::PriceMap;
use egui::Pos2;

impl Chart {
    /// Sets whether the crosshair follows the pointer value or snaps to visible data.
    pub fn set_crosshair_mode(&mut self, mode: CrosshairMode) {
        self.chart_options.crosshair.mode = mode;
    }

    pub(super) fn snap_crosshair(&self, mut pos: Pos2, map: &PriceMap, index: usize) -> Pos2 {
        if self.chart_options.crosshair.mode != CrosshairMode::Magnet {
            return pos;
        }
        let ohlc = self
            .state
            .shown()
            .get(index)
            .filter(|_| self.main_series_visible)
            .into_iter()
            .flat_map(|bar| [bar.open, bar.high, bar.low, bar.close]);
        let studies = self
            .studies
            .indicators()
            .iter()
            .filter(|study| {
                !self.chart_type.is_time_independent()
                    && study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
            })
            .flat_map(|study| {
                (0..study.line_cnt())
                    .filter(|line| study.plot_style(*line).visible)
                    .filter_map(move |line| study.values().value(line, index))
            });
        if let Some(value) = snap_to_values(pos.y, map, ohlc.chain(studies)) {
            pos.y = map.price_to_y(value);
        }
        pos
    }

    pub(super) fn apply_study_action(&mut self, action: StudyAction) {
        let index = match action {
            StudyAction::Settings(index)
            | StudyAction::ToggleVisibility(index)
            | StudyAction::Remove(index) => index,
        };
        let Some(id) = self.study_ids.get(index).copied() else {
            return;
        };
        match action {
            StudyAction::Settings(_) => self.indicator_settings_request = Some(id),
            StudyAction::ToggleVisibility(_) => {
                self.set_indicator_visible(id, !self.studies.indicators()[index].is_visible());
            }
            StudyAction::Remove(_) => {
                self.remove_series(id);
            }
        }
    }
}
