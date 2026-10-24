//! Crosshair snapping and study legend actions shared by the chart panes.

use super::{Chart, ChartElementId, RightClickTarget};
use crate::chart::renderers::{StudyAction, snap_to_values};
use crate::config::CrosshairMode;
use crate::scales::PriceMap;
use crate::studies::Indicator;
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
                    && study.scale() == crate::studies::StudyScale::Main
            })
            .flat_map(|study| {
                (0..study.line_cnt())
                    .filter(|line| {
                        let plot = study.plot_style(*line);
                        plot.visible
                            && !matches!(
                                plot.plot_type,
                                crate::studies::PlotType::BarColor
                                    | crate::studies::PlotType::BackgroundColor
                            )
                    })
                    .filter_map(move |line| study.plot_value(line, index))
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
            | StudyAction::Remove(index)
            | StudyAction::Menu { index, .. } => index,
        };
        let Some(id) = self.study_ids.get(index).copied() else {
            return;
        };
        if let StudyAction::Menu { pos, .. } = action {
            // The bar the legend shows: the hovered one, or the latest.
            let bars = self.state.shown().len();
            let bar_idx = self
                .last_hover_bar_idx
                .map(|index| index.round() as usize)
                .filter(|index| *index < bars)
                .unwrap_or(bars.saturating_sub(1));
            let element = ChartElementId::Indicator(id);
            self.selection.select(element, Some(bar_idx));
            self.right_click = Some(RightClickTarget::Element {
                id: element,
                bar_idx,
                pos,
                price: menu_value(self.studies.indicators()[index].as_ref(), bar_idx)
                    .map_or(f64::NAN, |(_, value)| value),
            });
            return;
        }
        self.user_action(|chart| match action {
            StudyAction::Settings(_) => chart.indicator_settings_request = Some(id),
            StudyAction::ToggleVisibility(_) => {
                chart.set_indicator_visible(id, !chart.studies.indicators()[index].is_visible());
            }
            StudyAction::Remove(_) => {
                chart.remove_series(id);
            }
            StudyAction::Menu { .. } => {}
        });
    }
}

/// The output and value a study's menu refers to at `bar`: its first visible plot with a value
/// there, which for volume is the bar's column.
pub(crate) fn menu_value(study: &dyn Indicator, bar: usize) -> Option<(usize, f64)> {
    (0..study.line_cnt())
        .filter(|line| {
            let plot = study.plot_style(*line);
            plot.visible
                && !matches!(
                    plot.plot_type,
                    crate::studies::PlotType::BarColor | crate::studies::PlotType::BackgroundColor
                )
        })
        .find_map(|line| Some((line, study.plot_value(line, bar)?)))
}
