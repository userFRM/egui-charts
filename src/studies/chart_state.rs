//! Chart state management (model layer, no rendering).
//!
//! [`ChartState`] is the back-end owner of bar data and coordinate systems.
//! It tracks which bars are visible, manages price auto-scaling, and maintains
//! a zoom-history stack for undo/redo.

use crate::model::{Bar, BarData, TimeScale, next_revision};
use crate::scales::{PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::Indicator;
use std::sync::Arc;
/// A snapshot of zoom parameters, used for undo/redo of zoom operations.
///
/// The chart maintains a stack of these snapshots so the user can press
/// "zoom out" to return to a previous view.
#[derive(Debug, Clone, Copy)]
struct ZoomState {
    /// Pixels per bar at the time of the snapshot.
    pub bar_spacing: f32,
    /// Right offset in bars from the chart edge.
    pub right_offset: f64,
    /// Manual price range in prices, or `None` when auto-scaled.
    pub price_range: Option<(f64, f64)>,
}

/// Backend chart state -- owns bar data, time-scale coordinate system,
/// price auto-scaling logic, and zoom history.
///
/// This is the pure-logic layer with no UI or rendering dependencies.  The UI
/// layer reads from `ChartState` to determine what to draw and writes back
/// interaction results (scroll, zoom, data updates).
///
/// # Example
///
/// ```
/// use egui_charts::model::BarData;
/// use egui_charts::studies::ChartState;
///
/// let data = BarData::new();
/// let state = ChartState::new(data);
/// assert!(state.visible_data().is_empty());
/// ```
#[derive(Debug)]
pub struct ChartState {
    /// The OHLCV bar dataset.
    data: BarData,
    /// The price-based elements currently shown, one bar per element.
    elements: Option<Arc<[Bar]>>,
    /// Time coordinate system (logical index <-> pixel mapping).
    time_scale: TimeScale,
    /// The main series' price scale.
    price_scale: PriceScale,
    /// Stack of previous zoom snapshots for the "zoom out" button.
    zoom_history: Vec<ZoomState>,
    /// See [`revision`](Self::revision).
    revision: u64,
}

impl Clone for ChartState {
    /// A copy with a revision of its own, since its bars change independently.
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            elements: self.elements.clone(),
            time_scale: self.time_scale.clone(),
            price_scale: self.price_scale.clone(),
            zoom_history: self.zoom_history.clone(),
            revision: next_revision(),
        }
    }
}

impl ChartState {
    /// Create new chart state with data
    pub fn new(data: BarData) -> Self {
        let mut time_scale = TimeScale::new();
        time_scale.set_bar_cnt(data.len());

        Self {
            data,
            elements: None,
            time_scale,
            price_scale: PriceScale::new(PriceScaleOptions::default()),
            zoom_history: Vec::new(),
            revision: next_revision(),
        }
    }

    /// Get reference to bar data
    pub fn data(&self) -> &BarData {
        &self.data
    }

    /// Get mutable reference to time scale
    pub fn time_scale_mut(&mut self) -> &mut TimeScale {
        &mut self.time_scale
    }

    /// Get reference to time scale
    pub fn time_scale(&self) -> &TimeScale {
        &self.time_scale
    }

    /// Identifies the bars before the last one. It changes when any of them
    /// changes or bars are removed, and stays the same when bars are appended
    /// or the last bar is updated, so results computed from the bars can be
    /// cached under it and extended as the series streams.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Replaces the bars (also updates the bar count of the time scale). Returns the index of the
    /// first bar that differs from the previous bars, counting a change of length: the shorter
    /// length when one is a prefix of the other, `None` when they are identical.
    pub fn set_data(&mut self, data: BarData) -> Option<usize> {
        self.edit_bars(|bars| {
            // Bitwise, so that a NaN compares equal to itself.
            let bits = |bar: &Bar| {
                (
                    bar.time,
                    [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits),
                )
            };
            let first = bars
                .iter()
                .zip(&data.bars)
                .position(|(old, new)| bits(old) != bits(new))
                .or_else(|| (bars.len() != data.len()).then(|| bars.len().min(data.len())));
            *bars = data.bars;
            first
        })
    }

    /// Runs `edit` on the bars and keeps the time scale and [`revision`](Self::revision) in step.
    /// `edit` returns the index of the first bar it inserted, changed or removed; bars after it
    /// may have moved.
    pub(crate) fn edit_bars(
        &mut self,
        edit: impl FnOnce(&mut Vec<Bar>) -> Option<usize>,
    ) -> Option<usize> {
        let old_len = self.data.len();
        let first_changed = edit(&mut self.data.bars);
        if first_changed.is_some_and(|first| first < old_len.saturating_sub(1))
            || self.data.len() < old_len
        {
            self.revision = next_revision();
        }
        self.time_scale.set_bar_cnt(self.shown().len());
        first_changed
    }

    /// The series drawn on the time scale, one OHLC bar per element on price-based charts.
    /// Indices returned by the chart hover and selection APIs refer to this series.
    pub fn shown(&self) -> &[Bar] {
        self.elements.as_deref().unwrap_or(&self.data.bars)
    }

    #[cfg(feature = "widget")]
    pub(crate) fn shows_elements(&self) -> bool {
        self.elements.is_some()
    }

    /// Keeps the right-edge item by time and rank when the shown series changes.
    #[cfg(any(feature = "widget", test))]
    pub(crate) fn set_elements(&mut self, elements: Option<Arc<[Bar]>>, follow: bool) {
        match (&self.elements, &elements) {
            (Some(old), Some(new)) if Arc::ptr_eq(old, new) => return,
            (None, None) => return,
            _ => {}
        }
        let old = self.shown();
        let new = elements.as_deref().unwrap_or(&self.data.bars);
        let ts = &self.time_scale;
        let near_live = ts.is_near_live_edge();
        let kept = (!old.is_empty() && !new.is_empty()).then(|| {
            // Several elements can share the completion time of one source bar.
            let right = ts.base_idx() as f64 + ts.right_offset_precise();
            let at = (right.floor().max(0.0) as usize).min(old.len() - 1);
            let time = old[at].time;
            let rank = at - old.partition_point(|bar| bar.time < time);
            let first = new.partition_point(|bar| bar.time < time);
            let same = new[first..].partition_point(|bar| bar.time == time);
            let to = (first + rank.min(same.saturating_sub(1))).min(new.len() - 1);
            let newer = new[new.len() - 1].time > old[old.len() - 1].time;
            (
                to as f64 + (right - at as f64) - (new.len() - 1) as f64,
                newer,
            )
        });
        self.elements = elements;
        self.time_scale.set_bar_cnt(self.shown().len());
        match kept {
            Some((_, true)) if follow && near_live => self.time_scale.scroll_to_realtime(),
            Some(_) if follow && near_live => {}
            Some((offset, _)) => self.time_scale.set_right_offset(offset),
            None => {}
        }
    }

    /// Visible source bars, or one OHLC bar per element on a price-based chart type.
    pub fn visible_data(&self) -> &[crate::model::Bar] {
        if self.shown().is_empty() {
            return &[];
        }

        let logical_range = self.time_scale.visible_logical_range();
        log::debug!(
            "[visible_data] logical_range: left={}, right={}",
            logical_range.left,
            logical_range.right
        );

        let (mut start, mut end) = logical_range.to_strict_range();
        log::debug!("[visible_data] BEFORE clamping: start={start}, end={end}");

        // Clamp to valid data range
        let data_len = self.shown().len();
        end = end.min(data_len);
        start = start.min(end);

        log::debug!("[visible_data] AFTER clamping: start={start}, end={end}, data_len={data_len}");

        // Calculate how many bars should be visible
        let expected_visible = self.time_scale.visible_candles();

        // If we have too few bars visible (scrolled past beginning), extend the range
        let actual_visible = end.saturating_sub(start);
        log::debug!(
            "[visible_data] actual_visible={actual_visible}, expected_visible={expected_visible}"
        );

        if actual_visible < expected_visible && start == 0 && data_len > 1 {
            // We're at the beginning but don't have enough bars visible
            // Extend end to show the expected number of bars
            // Note: Only extend if we have more than 1 bar to avoid log spam during startup
            let old_end = end;
            end = expected_visible.min(data_len);
            if end > old_end {
                log::debug!(
                    "[visible_data] EXTENDING RANGE: old_end={}, new_end={} (gained {} bars)",
                    old_end,
                    end,
                    end - old_end
                );
            }
        }

        log::debug!(
            "[visible_data] FINAL RESULT: returning bars[{}..{}] ({} bars)",
            start,
            end,
            end - start
        );

        &self.shown()[start..end]
    }

    /// Visible range indices, counting elements on price-based chart types and bars otherwise.
    pub fn visible_range(&self) -> (usize, usize) {
        if self.shown().is_empty() {
            return (0, 0);
        }

        let logical_range = self.time_scale.visible_logical_range();
        let (mut start, mut end) = logical_range.to_strict_range();

        log::debug!("[visible_range] BEFORE clamping: start={start}, end={end}");

        // Clamp to valid data range
        let data_len = self.shown().len();
        end = end.min(data_len);
        start = start.min(end);

        // Calculate how many bars should be visible
        let expected_visible = self.time_scale.visible_candles();

        // If we have too few bars visible (scrolled past beginning), extend the range
        let actual_visible = end.saturating_sub(start);

        if actual_visible < expected_visible && start == 0 && data_len > 1 {
            // We're at the beginning but don't have enough bars visible
            // Extend end to show the expected number of bars
            // Note: Only extend if we have more than 1 bar to avoid log spam during startup
            let old_end = end;
            end = expected_visible.min(data_len);
            if end > old_end {
                log::debug!(
                    "[visible_range] EXTENDING RANGE: old_end={}, new_end={} (gained {} bars)",
                    old_end,
                    end,
                    end - old_end
                );
            }
        }

        log::debug!(
            "[visible_range] FINAL RESULT: ({}, {}) - {} bars",
            start,
            end,
            end - start
        );

        (start, end)
    }

    /// Returns the main series' price scale.
    pub fn price_scale(&self) -> &PriceScale {
        &self.price_scale
    }

    /// Returns the main series' price scale for changing its options or range.
    pub fn price_scale_mut(&mut self) -> &mut PriceScale {
        &mut self.price_scale
    }

    /// Fits the price scale to the visible bars and the visible values of the visible overlay
    /// studies in `indicators`, unless the user scaled by hand, and returns this frame's
    /// transform: percentage and indexed modes are relative to the first visible close.
    pub fn fit_price_scale<'a>(
        &mut self,
        indicators: impl IntoIterator<Item = &'a dyn Indicator>,
    ) -> PriceTransform {
        self.fit_price_scale_with_series(
            indicators,
            None,
            true,
            None,
            None,
            std::iter::empty(),
            std::iter::empty(),
        )
    }

    /// Fits visible series and studies together. `autoscale` replaces the main series' range of
    /// lows and highs; `prices` are further prices to fit, such as those of markers;
    /// `series_values` are already in scale units. `main_bars` supplies transformed visible
    /// bars; `main_range` reuses a precomputed exact range before calling the autoscale provider.
    pub(crate) fn fit_price_scale_with_series<'a>(
        &mut self,
        indicators: impl IntoIterator<Item = &'a dyn Indicator>,
        main_bars: Option<&[Bar]>,
        main_visible: bool,
        autoscale: Option<&crate::model::AutoscaleProvider>,
        main_range: Option<(f64, f64)>,
        prices: impl IntoIterator<Item = f64>,
        series_values: impl IntoIterator<Item = f64>,
    ) -> PriceTransform {
        let mode = self.price_scale.options().mode;
        let (start, end) = self.visible_range();
        let visible = main_bars.unwrap_or(&self.shown()[start..end]);
        // In whitespace retain the nearest reference bar so relative axis units stay valid: the
        // first bar with a value from the left edge on, or else the last one before it.
        let at = start.min(self.shown().len().saturating_sub(1));
        let base = self.shown()[at..]
            .iter()
            .find(|bar| !bar.is_whitespace())
            .or_else(|| crate::model::last_with_value(&self.shown()[..at]))
            .map_or(f64::NAN, |bar| bar.close);
        let transform = PriceTransform::new(mode, base);
        let (study_start, study_end) = self.time_scale.visible_logical_range().to_strict_range();
        let study_values = indicators
            .into_iter()
            .filter(|study| {
                study.is_visible()
                    && study.is_overlay()
                    && study.scale() == crate::studies::StudyScale::Main
            })
            .flat_map(|study| {
                #[cfg(not(feature = "drawings"))]
                let drawing_prices = std::iter::empty();
                #[cfg(feature = "drawings")]
                let drawing_prices = crate::studies::plots::drawing_prices(
                    study,
                    study_start.max(start)..study_end.max(end),
                );
                (0..study.line_count())
                    .filter(|&index| {
                        let plot = study.plot_style(index);
                        plot.visible
                            && !matches!(
                                plot.plot_type,
                                crate::studies::PlotType::BarColor
                                    | crate::studies::PlotType::BackgroundColor
                            )
                    })
                    .flat_map(move |index| {
                        let available_end = study
                            .values()
                            .len()
                            .checked_add_signed(study.plot_offset(index))
                            .unwrap_or(0);
                        (study_start.max(start)..study_end.max(end).min(available_end))
                            .filter_map(move |bar| study.plot_value(index, bar))
                            .chain(std::iter::once(0.0).filter(move |_| {
                                matches!(
                                    study.plot_style(index).plot_type,
                                    crate::studies::PlotType::Histogram
                                        | crate::studies::PlotType::Columns
                                        | crate::studies::PlotType::Area
                                )
                            }))
                    })
                    .chain(
                        study
                            .candles()
                            .iter()
                            .filter(|candle| {
                                candle.bars.end > study_start.max(start)
                                    && candle.bars.start < study_end.max(end)
                                    && study.plot_style(candle.plot).visible
                            })
                            .flat_map(|candle| [candle.bar.low, candle.bar.high]),
                    )
                    .chain(
                        study
                            .zones()
                            .iter()
                            .filter(|zone| {
                                zone.bars.end > study_start.max(start)
                                    && zone.bars.start < study_end.max(end)
                                    && study.plot_style(zone.plot).visible
                            })
                            .flat_map(|zone| [zone.bottom, zone.top]),
                    )
                    .chain(
                        study
                            .projections()
                            .iter()
                            .filter(|projection| {
                                projection.bars.end > study_start.max(start) as f64
                                    && projection.bars.start < study_end.max(end) as f64
                                    && study.plot_style(projection.plot).visible
                            })
                            .flat_map(|projection| [projection.bottom, projection.top]),
                    )
                    .chain(
                        study
                            .volume_profiles(study_start.max(start)..study_end.max(end))
                            .into_iter()
                            .flat_map(|plot| [plot.profile.low(), plot.profile.high()]),
                    )
                    .chain(drawing_prices)
            });
        let main = main_visible
            .then(|| {
                let range = main_range.or_else(|| {
                    visible
                        .iter()
                        .flat_map(|bar| [bar.low, bar.high])
                        .chain(
                            visible
                                .iter()
                                .flat_map(|bar| [bar.open, bar.close])
                                .filter(|_| mode == PriceScaleMode::Logarithmic),
                        )
                        .filter(|value| transform.to_scale(*value).is_finite())
                        .fold(None, |range: Option<(f64, f64)>, value| {
                            Some(range.map_or((value, value), |(min, max)| {
                                (min.min(value), max.max(value))
                            }))
                        })
                });
                match autoscale {
                    Some(provider) => provider.range(visible, range),
                    None => range,
                }
            })
            .flatten();
        let (min, max) = main
            .into_iter()
            .flat_map(|(min, max)| [min, max])
            .chain(study_values)
            .chain(prices)
            .map(|value| transform.to_scale(value))
            .chain(series_values)
            .filter(|value| value.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
                (min.min(value), max.max(value))
            });
        self.price_scale.rebase(transform);
        self.price_scale.autoscale(min, max);
        transform
    }

    /// Save current zoom state to history
    pub fn push_zoom_state(&mut self) {
        let (range, t) = (self.price_scale.range(), self.price_scale.transform());
        let curr_state = ZoomState {
            bar_spacing: self.time_scale.bar_spacing(),
            right_offset: self.time_scale.right_offset_precise(),
            price_range: self
                .price_scale
                .is_manual()
                .then(|| (t.to_price(range.min), t.to_price(range.max))),
        };
        self.zoom_history.push(curr_state);
    }

    /// Restore previous zoom state (zoom out button)
    pub fn pop_zoom_state(&mut self) -> bool {
        if let Some(state) = self.zoom_history.pop() {
            self.time_scale.set_bar_spacing(state.bar_spacing);
            self.time_scale.set_right_offset(state.right_offset);
            // In the units of the scale's transform; the next fit rebases them. A price that
            // has no finite value there (0 on a logarithmic scale) resets the range instead.
            let t = self.price_scale.transform();
            let range = state
                .price_range
                .map(|(min, max)| (t.to_scale(min), t.to_scale(max)));
            match range {
                Some((min, max)) if min.is_finite() && max.is_finite() => {
                    self.price_scale.set_range(min, max)
                }
                _ => self.price_scale.reset(),
            }
            true
        } else {
            false
        }
    }

    /// Clear zoom history
    pub fn clear_zoom_history(&mut self) {
        self.zoom_history.clear();
    }

    /// Check if there are zoom states to restore
    pub fn has_zoom_history(&self) -> bool {
        !self.zoom_history.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;
    use chrono::{TimeZone, Utc};

    /// Time and rank identify the right-edge item across type changes and trimmed builds.
    #[test]
    fn a_new_series_keeps_the_item_at_the_right_edge() {
        let bars = |minutes: &[i64]| -> Vec<Bar> {
            minutes
                .iter()
                .map(|&minute| {
                    Bar::new(
                        Utc.timestamp_opt(1_700_000_000 + minute * 60, 0).unwrap(),
                        100.0,
                        100.0,
                        100.0,
                        100.0,
                        1.0,
                    )
                })
                .collect()
        };
        let e: &[i64] = &[1, 3, 3, 3, 6, 9];
        // Edge = last index + offset; duplicate times retain their rank.
        for (name, old, offset, new, follow, expected) in [
            ("bars to elements", None, -4.0, Some(e), true, -1.0),
            ("elements to bars", Some(e), -2.0, None, true, -6.0),
            (
                "oldest element dropped",
                Some(e),
                -2.0,
                Some(&[3, 3, 3, 6, 9, 12][..]),
                true,
                -3.0,
            ),
            (
                "following off",
                Some(e),
                0.5,
                Some(&[1, 3, 3, 3, 6, 9, 12][..]),
                false,
                -0.5,
            ),
            (
                "type change at the live edge",
                None,
                0.5,
                Some(&[1, 3, 6, 9, 9, 9][..]),
                true,
                0.5,
            ),
        ] {
            let mut state = ChartState::new(BarData::from_bars(bars(&(0..10).collect::<Vec<_>>())));
            state.set_elements(old.map(|times| Arc::from(bars(times))), true);
            state.time_scale_mut().set_right_offset(offset);
            state.set_elements(new.map(|times| Arc::from(bars(times))), follow);
            assert_eq!(state.time_scale().right_offset(), expected, "{name}");
        }
    }

    /// Flat visible data must not collapse the auto range to a near-zero span,
    /// which would put any other price (a drawing anchor) ~1e14 px away.
    #[test]
    fn flat_data_auto_scales_relative_to_price_level() {
        for (mode, price) in [
            (PriceScaleMode::Normal, 100.0),
            (PriceScaleMode::Logarithmic, 1.0001),
        ] {
            let bars = (0..50)
                .map(|i| {
                    let t = Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap();
                    Bar::new(t, price, price, price, price, 1.0)
                })
                .collect();
            let mut state = ChartState::new(BarData::from_bars(bars));
            state.time_scale_mut().set_width(800.0);
            state.price_scale_mut().options_mut().mode = mode;
            let transform = state.fit_price_scale(std::iter::empty());
            let range = state.price_scale().range();
            let (min, max) = (transform.to_price(range.min), transform.to_price(range.max));
            assert!(min < price && price < max, "{mode:?}: ({min}, {max})");
            // At least a ten-thousandth of the price (one cent at 100), at most the price itself.
            assert!(
                (price * 1e-4..=price).contains(&(max - min)),
                "{mode:?}: ({min}, {max})"
            );
        }
    }

    /// A range scaled by hand keeps every price at its pixel while the chart scrolls,
    /// and the zoom history restores the prices it saved, although the first visible
    /// close the relative modes are measured from changes with the scroll.
    #[test]
    fn manual_range_keeps_its_prices_while_the_base_moves() {
        for mode in [PriceScaleMode::Percentage, PriceScaleMode::IndexedTo100] {
            let bars = (0..300)
                .map(|i| {
                    let t = Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap();
                    let close = 100.0 + i as f64;
                    Bar::new(t, close, close, close, close, 1.0)
                })
                .collect();
            let mut state = ChartState::new(BarData::from_bars(bars));
            state.time_scale_mut().set_width(800.0);
            state.price_scale_mut().options_mut().mode = mode;
            let y_of_380 = |state: &mut ChartState| {
                let transform = state.fit_price_scale(std::iter::empty());
                let map = state.price_scale().map(transform, 0.0, 500.0);
                map.price_to_y(380.0)
            };
            let zoom_in = |state: &mut ChartState| {
                let range = state.price_scale().range();
                let centre = (range.min + range.max) / 2.0;
                state.price_scale_mut().zoom(0.9, centre);
            };
            y_of_380(&mut state);
            zoom_in(&mut state);
            let y = y_of_380(&mut state);
            state.push_zoom_state();

            let offset = state.time_scale().right_offset();
            state.time_scale_mut().set_right_offset(offset - 10.0);
            let panned = y_of_380(&mut state);
            assert!(
                (panned - y).abs() < 0.5,
                "{mode:?} panned: {panned}, was {y}"
            );

            zoom_in(&mut state);
            state.pop_zoom_state();
            let restored = y_of_380(&mut state);
            assert!(
                (restored - y).abs() < 0.5,
                "{mode:?} restored: {restored}, was {y}"
            );
        }
    }
}
