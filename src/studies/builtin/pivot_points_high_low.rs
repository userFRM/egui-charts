//! Pivot Points High Low.
//!
//! Marks swing highs and lows: a pivot high is a bar whose high is the highest of a window of
//! `left` bars before it and `right` bars after it, a pivot low mirrors it with lows. Each
//! pivot is labelled with its price.
//!
//! # Rule
//!
//! ```text
//! Pivot high at bar p: high[p] >= high[p - left ..= p - 1] and high[p] > high[p + 1 ..= p + right]
//! Pivot low at bar p:  low[p]  <= low[p - left ..= p - 1]  and low[p]  < low[p + 1 ..= p + right]
//! ```
//!
//! Of equal extremes in one window the newest is the pivot. A pivot needs its full window, so
//! it is known `right` bars after its bar, and the first `left` bars have none. The defaults
//! are 10 bars on each side for both highs and lows.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, PivotPointsHighLow};
//!
//! let bars: Vec<Bar> = [1.0, 3.0, 3.0, 2.0, 1.0]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64 * 60, 0).unwrap(), p, p, p, p, 1.0))
//!     .collect();
//! let mut pivots = PivotPointsHighLow::new(2, 2, 2, 2);
//! pivots.calculate(&bars);
//! // The newer of the two equal highs is the pivot, labelled with its price.
//! assert_eq!(pivots.values().value(0, 2), Some(3.0));
//! assert_eq!(pivots.plot_text(0, 2).as_deref(), Some("3.00"));
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_max, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Whether each value is a pivot: at least each of the `left` values before it and above each
/// of the `right` values after it, every value of that window finite. `high` false finds
/// lows. Lengths must be at least 1.
pub(super) fn pivots(values: &[f64], left: usize, right: usize, high: bool) -> Vec<bool> {
    let x: Vec<f64> = values.iter().map(|&v| if high { v } else { -v }).collect();
    let before = rolling_max(x.iter().copied(), left);
    let after = rolling_max(x.iter().copied(), right);
    let mut finite = 0;
    // Finite values ending at each index.
    let runs: Vec<usize> = x
        .iter()
        .map(|v| {
            finite = if v.is_finite() { finite + 1 } else { 0 };
            finite
        })
        .collect();
    (0..x.len())
        .map(|p| {
            let (Some(start), Some(end)) = (p.checked_sub(left), p.checked_add(right)) else {
                return false;
            };
            end < x.len() && runs[end] > end - start && before[p - 1] <= x[p] && after[end] < x[p]
        })
        .collect()
}

/// [`pivots`] over a stream of bars: the window of the last `left + right + 1` bars with finite
/// highs and lows, from which each new bar confirms the pivot `right` bars before it.
#[derive(Clone, Default)]
pub(super) struct Swings {
    window: std::collections::VecDeque<(usize, f64, f64)>,
}

impl Swings {
    /// Adds the bar at `index` and returns the pivot high and the pivot low it confirms, each as
    /// its bar and price. A bar without a finite high and low is skipped, as if it were absent.
    pub(super) fn push(
        &mut self,
        index: usize,
        bar: &Bar,
        left: usize,
        right: usize,
    ) -> [Option<(usize, f64)>; 2] {
        if !(bar.high.is_finite() && bar.low.is_finite()) {
            return [None, None];
        }
        let size = left.saturating_add(right).saturating_add(1);
        self.window.push_back((index, bar.high, bar.low));
        if self.window.len() > size {
            self.window.pop_front();
        }
        if self.window.len() < size {
            return [None, None];
        }
        let (at, high, low) = self.window[left];
        let before = || self.window.range(..left);
        let after = || self.window.range(left + 1..);
        [
            (before().all(|w| w.1 <= high) && after().all(|w| w.1 < high)).then_some((at, high)),
            (before().all(|w| w.2 >= low) && after().all(|w| w.2 > low)).then_some((at, low)),
        ]
    }
}

/// Pivot Points High Low: swing highs and lows, each labelled with its price.
#[derive(Clone)]
pub struct PivotPointsHighLow {
    high_left: usize,
    high_right: usize,
    low_left: usize,
    low_right: usize,
    values: IndicatorValues,
    high_color: Color32,
    low_color: Color32,
    visible: bool,
}

impl PivotPointsHighLow {
    /// Pivot highs with `high_left` and `high_right` bars on each side and pivot lows with
    /// `low_left` and `low_right`, each raised to at least 1.
    pub fn new(high_left: usize, high_right: usize, low_left: usize, low_right: usize) -> Self {
        Self {
            high_left: high_left.max(1),
            high_right: high_right.max(1),
            low_left: low_left.max(1),
            low_right: low_right.max(1),
            values: IndicatorValues::new(),
            high_color: palette::MUTED,
            low_color: palette::MUTED,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Both columns exist, so a bar with one pivot still has a value per line.
            values.set_value(1, 0, f64::NAN);
        }
        let highs: Vec<f64> = data.iter().map(|b| b.high).collect();
        let lows: Vec<f64> = data.iter().map(|b| b.low).collect();
        for (line, prices, left, right) in [
            (0, highs, self.high_left, self.high_right),
            (1, lows, self.low_left, self.low_right),
        ] {
            for (bar, pivot) in pivots(&prices, left, right, line == 0)
                .into_iter()
                .enumerate()
            {
                if pivot {
                    values.set_value(line, bar, prices[bar]);
                }
            }
        }
        values
    }
}

/// Construct with the conventional default parameters.
impl Default for PivotPointsHighLow {
    fn default() -> Self {
        Self::new(10, 10, 10, 10)
    }
}

impl Indicator for PivotPointsHighLow {
    crate::studies::inputs::study_inputs!(self;
        high_left: length("Pivot high left bars", 1, usize::MAX),
        high_right: length("Pivot high right bars", 1, usize::MAX),
        low_left: length("Pivot low left bars", 1, usize::MAX),
        low_right: length("Pivot low right bars", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Pivot Points High Low"
    }

    fn desc(&self) -> &str {
        "Pivot Points High Low - Swing highs and lows labelled with their price"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        // Confirmation of a right-hand window can change earlier pivot bars.
        let from = usable_behind(self.high_right.max(self.low_right), |b| {
            b.high.is_finite() && b.low.is_finite()
        })(data, from.min(self.values.len()).min(data.len()));
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.high_left.max(self.low_left), |b| {
                b.high.is_finite() && b.low.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite(),
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: if line == 0 {
                self.high_color
            } else {
                self.low_color
            },
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            // The label sits over a downward triangle and under an upward one.
            plot_type: if line == 0 {
                crate::studies::PlotType::TriangleDown
            } else {
                crate::studies::PlotType::TriangleUp
            },
            visible: true,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        Some(self.format_value(self.values.value(line, bar)?))
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.high_color, self.low_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.high_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.low_color = color;
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Pivot High".to_string(), "Pivot Low".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// A stream of bars confirms exactly the pivots of the batch rule over its finite bars, each
    /// `right` finite bars later, skipping a bar without a price.
    #[test]
    fn swings_confirm_the_pivots_of_the_batch_rule() {
        let highs: Vec<f64> = (0..200u32)
            .map(|i| {
                if i == 90 {
                    f64::NAN
                } else {
                    f64::from(i * 7919 % 31) + 1.0
                }
            })
            .collect();
        let bar = |i: usize, p: f64| {
            Bar::new(
                Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                p,
                p,
                p,
                p,
                1.0,
            )
        };
        let finite: Vec<usize> = (0..highs.len()).filter(|&i| highs[i].is_finite()).collect();
        let prices: Vec<f64> = finite.iter().map(|&i| highs[i]).collect();
        for (left, right) in [(1, 1), (3, 2), (2, 5)] {
            let mut swings = Swings::default();
            let mut streamed = [vec![false; prices.len()], vec![false; prices.len()]];
            for (i, &p) in highs.iter().enumerate() {
                for (line, pivot) in swings
                    .push(i, &bar(i, p), left, right)
                    .into_iter()
                    .enumerate()
                {
                    if let Some((at, price)) = pivot {
                        streamed[line][finite.binary_search(&at).unwrap()] = true;
                        assert_eq!(price, highs[at]);
                    }
                }
            }
            assert_eq!(
                streamed[0],
                pivots(&prices, left, right, true),
                "{left} {right}"
            );
            assert_eq!(
                streamed[1],
                pivots(&prices, left, right, false),
                "{left} {right}"
            );
        }
    }
}
