//! Chart state management (model layer, no rendering).
//!
//! [`ChartState`] is the back-end owner of bar data and coordinate systems.
//! It tracks which bars are visible, manages price auto-scaling, and maintains
//! a zoom-history stack for undo/redo.

/// ChartState Backend - Pure state management (no rendering).
/// Combines data with coordinate systems.
use super::{Bar, BarData, TimeScale};
use crate::scales::{PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::{Indicator, IndicatorValue};
use std::sync::atomic::{AtomicU64, Ordering};

/// Source of [`ChartState::revision`] values, so that no two chart states or
/// histories ever share one.
static REVISIONS: AtomicU64 = AtomicU64::new(0);

fn next_revision() -> u64 {
    REVISIONS.fetch_add(1, Ordering::Relaxed)
}

/// A snapshot of zoom parameters, used for undo/redo of zoom operations.
///
/// The chart maintains a stack of these snapshots so the user can press
/// "zoom out" to return to a previous view.
#[derive(Debug, Clone, Copy)]
pub struct ZoomState {
    /// Pixels per bar at the time of the snapshot.
    pub bar_spacing: f32,
    /// Right offset in bars from the chart edge.
    pub right_offset: f32,
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
/// use egui_charts::model::{BarData, ChartState};
///
/// let data = BarData::new();
/// let state = ChartState::new(data);
/// assert!(state.visible_data().is_empty());
/// ```
#[derive(Debug)]
pub struct ChartState {
    /// The OHLCV bar dataset.
    data: BarData,
    /// Time coordinate system (logical index <-> pixel mapping).
    time_scale: TimeScale,
    /// The main (right) price scale.
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
        self.time_scale.set_bar_cnt(self.data.len());
        first_changed
    }

    /// Get visible data based on current time scale
    pub fn visible_data(&self) -> &[crate::model::Bar] {
        if self.data.bars.is_empty() {
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
        let data_len = self.data.len();
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

        &self.data.bars[start..end]
    }

    /// Get visible data range indices
    pub fn visible_range(&self) -> (usize, usize) {
        if self.data.bars.is_empty() {
            return (0, 0);
        }

        let logical_range = self.time_scale.visible_logical_range();
        let (mut start, mut end) = logical_range.to_strict_range();

        log::debug!("[visible_range] BEFORE clamping: start={start}, end={end}");

        // Clamp to valid data range
        let data_len = self.data.len();
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

    /// Returns the main (right) price scale.
    pub fn price_scale(&self) -> &PriceScale {
        &self.price_scale
    }

    /// Returns the main (right) price scale for changing its options or range.
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
        let mode = self.price_scale.options().mode;
        let (start, end) = self.visible_range();
        let visible = &self.data.bars[start..end];
        let base = visible.first().map_or(f64::NAN, |bar| bar.close);
        let relative = matches!(
            mode,
            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
        );
        // Without a usable base there is no percentage, so such a frame shows prices. The other
        // modes ignore the base, so a scroll does not change their transform.
        let transform = if !relative {
            PriceTransform { mode, base: 0.0 }
        } else if base.is_finite() && base != 0.0 {
            PriceTransform { mode, base }
        } else {
            PriceTransform::default()
        };
        let study_values = indicators
            .into_iter()
            .filter(|study| study.is_visible() && study.is_overlay())
            .flat_map(|study| {
                let (values, lines) = (study.values(), study.line_cnt());
                values
                    .get(start..end.min(values.len()))
                    .unwrap_or_default()
                    .iter()
                    .flat_map(move |value| match value {
                        IndicatorValue::Single(price) => std::slice::from_ref(price),
                        IndicatorValue::Multiple(prices) => &prices[..prices.len().min(lines)],
                        IndicatorValue::None => &[],
                    })
                    .copied()
            });
        let (min, max) = visible
            .iter()
            .flat_map(|bar| [bar.low, bar.high])
            .chain(study_values)
            .map(|value| transform.to_scale(value))
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
            right_offset: self.time_scale.right_offset(),
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
