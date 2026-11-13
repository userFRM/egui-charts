//! Crosshair snapping and study legend actions shared by the chart panes.

use super::{Chart, ChartElementId, RightClickTarget, SeriesId};
use crate::chart::rendering::{ChartMapping, SeriesAction, StudyAction, study_mapping};
use crate::config::CrosshairMode;
use crate::scales::PriceMap;
use crate::studies::Indicator;
use egui::Pos2;

impl Chart {
    /// Sets whether the crosshair follows the pointer value or snaps to visible data.
    pub fn set_crosshair_mode(&mut self, mode: CrosshairMode) {
        self.chart_options.crosshair.mode = mode;
    }

    /// Resolves the shared crosshair position without changing the stored time slots.
    pub(super) fn crosshair_position(&self, mut pos: Pos2, coords: &ChartMapping) -> Option<Pos2> {
        let (time, index) = self.pointer_time(pos)?;
        if time.is_some() {
            pos.x = coords.index_to_x(index);
        }
        Some(pos)
    }

    /// Finds a main-slot candidate before comparing independently keyed series points.
    pub(super) fn main_crosshair_position(
        &self,
        mut pos: Pos2,
        coords: &ChartMapping,
    ) -> Option<Pos2> {
        let options = &self.chart_options.time_scale;
        if !options.ignore_whitespace_indices && !options.ignore_hidden_indices {
            return Some(pos);
        }
        let bars = self.state.shown();
        let index = coords.x_to_idx_f64(pos.x);
        if bars.is_empty() || !index.is_finite() {
            return None;
        }
        let first = coords.x_to_idx_f64(coords.rect.left()).ceil().max(0.0) as isize;
        let last = coords
            .x_to_idx_f64(coords.rect.right())
            .floor()
            .min(bars.len() as f64 - 1.0) as isize;
        if first > last {
            return None;
        }
        // Pixel rounding must not bias an equal-distance tie towards an earlier slot.
        let max_coordinate = coords.rect.left().abs().max(coords.rect.right().abs());
        let precision = f32::EPSILON * max_coordinate.max(1.0);
        let mut left = index.floor().clamp(first as f64 - 1.0, last as f64) as isize;
        let mut right = left + 1;
        while left >= first || right <= last {
            let candidate = if right > last
                || (left >= first
                    && (pos.x - coords.idx_to_x(left as usize)).abs() + precision
                        < (coords.idx_to_x(right as usize) - pos.x).abs())
            {
                let candidate = left as usize;
                left -= 1;
                candidate
            } else {
                let candidate = right as usize;
                right += 1;
                candidate
            };
            let x = coords.idx_to_x(candidate);
            if self.crosshair_time_eligible(bars[candidate].time, Some(candidate)) {
                pos.x = x;
                return Some(pos);
            }
        }
        None
    }

    pub(super) fn crosshair_time_eligible(
        &self,
        time: chrono::DateTime<chrono::Utc>,
        index: Option<usize>,
    ) -> bool {
        let options = &self.chart_options.time_scale;
        let series = self.price_series.iter().filter_map(|item| {
            let at = item.bars.binary_search_by_key(&time, |bar| bar.time).ok()?;
            Some((item.visible, !item.bars[at].is_whitespace()))
        });
        let custom = self.custom_values_at(time);
        let studies = self
            .rendered_studies()
            .indicators()
            .iter()
            .flat_map(|study| {
                (0..study.line_count()).map(move |line| {
                    let plot = study.plot_style(line);
                    let valued = !matches!(
                        plot.plot_type,
                        crate::studies::PlotType::BarColor
                            | crate::studies::PlotType::BackgroundColor
                    ) && index
                        .and_then(|index| study.plot_value(line, index))
                        .is_some_and(f64::is_finite);
                    (study.is_visible() && plot.visible, valued)
                })
            });
        let mut valued = false;
        let main_value = self.main_series_present
            && index.is_some_and(|index| !self.state.shown()[index].is_whitespace());
        for (visible, has_value) in std::iter::once((self.main_series_visible, main_value))
            .chain(series)
            .chain(custom)
            .chain(studies)
        {
            if has_value {
                if visible || !options.ignore_hidden_indices {
                    return true;
                }
                valued = true;
            }
        }
        !valued && !options.ignore_whitespace_indices
    }

    pub(super) fn snap_crosshair(&self, mut pos: Pos2, map: &PriceMap, index: usize) -> Pos2 {
        let mode = self.chart_options.crosshair.mode;
        if !mode.snaps() {
            return pos;
        }
        let close_only = mode == CrosshairMode::MagnetClose;
        // Candidates are pointer-space heights, so each value goes through its own scale.
        let ohlc = self
            .state
            .shown()
            .get(index)
            .filter(|bar| self.main_series_visible && !bar.is_whitespace())
            .into_iter()
            .flat_map(|bar| {
                [bar.open, bar.high, bar.low, bar.close]
                    .into_iter()
                    .skip(if close_only { 3 } else { 0 })
            })
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
                !close_only
                    && time_based
                    && study.is_visible()
                    && !study.is_volume()
                    && self.series_pane(**id) == Some(self.main_series_pane())
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
            .filter(|_| time_based || close_only)
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
        if !matches!(action, StudyAction::ToggleVisibility(_))
            && !self.indicator_user_edit_enabled(id)
        {
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
