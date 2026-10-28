//! Crosshair snapping and study legend actions shared by the chart panes.

use super::{Chart, ChartElementId, PaneId, RightClickTarget, SeriesId};
use crate::chart::renderers::{ChartMapping, SeriesAction, StudyAction, study_mapping};
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
        let mode = self.chart_options.crosshair.mode;
        if !mode.snaps() {
            return pos;
        }
        if mode == CrosshairMode::MagnetClose {
            if let Some(bar) = self
                .state
                .shown()
                .get(index)
                .filter(|_| self.main_series_visible)
            {
                pos.y = map.price_to_y(bar.close);
            }
            return pos;
        }
        // Candidates are pointer-space heights, so each value goes through its own scale.
        let ohlc = self
            .state
            .shown()
            .get(index)
            .filter(|_| self.main_series_visible)
            .into_iter()
            .flat_map(|bar| [bar.open, bar.high, bar.low, bar.close])
            .map(|value| map.price_to_y(value));
        let time_based = !self.chart_type.is_time_independent();
        let coords = ChartMapping {
            price: *map,
            ..self.get_chart_mapping()
        };
        let studies = self
            .study_ids
            .iter()
            .zip(self.studies.indicators())
            .filter(|(id, study)| {
                time_based
                    && study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
                    && self.series_pane(**id) == Some(PaneId::MAIN)
            })
            .flat_map(|(_, study)| {
                let scale = study_mapping(study.as_ref(), &coords).price;
                (0..study.line_count())
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
                    .map(move |value| scale.price_to_y(value))
            });
        let rect = self.last_rendered_price_rect;
        let series = self
            .main_pane_series()
            .filter(|_| time_based)
            .filter_map(|item| {
                let scale = self.series_map(item, rect);
                self.series_close_at(item, index)
                    .map(|value| scale.price_to_y(value))
            });
        // Equal distances keep the first candidate: the bar, then studies, then series.
        if let Some(y) = ohlc
            .chain(studies)
            .chain(series)
            .filter(|y| y.is_finite())
            .min_by(|a, b| (a - pos.y).abs().total_cmp(&(b - pos.y).abs()))
        {
            pos.y = y;
        }
        pos
    }

    /// Applies a legend control of the main series to the chart's typed operations. Settings are
    /// reported to the host, as for a study, with [`SeriesId::MAIN`].
    pub(super) fn apply_main_series_action(&mut self, action: SeriesAction) {
        self.user_action(|chart| match action {
            SeriesAction::Settings => chart.indicator_settings_request = Some(SeriesId::MAIN),
            SeriesAction::ToggleVisibility => {
                chart.set_main_series_visible(!chart.main_series_visible);
            }
            SeriesAction::Remove => {
                chart.remove_series(SeriesId::MAIN);
            }
        });
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
                price: self
                    .indicator_value_at(id, bar_idx)
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
    (0..study.line_count())
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
