//! Stochastic Oscillator Indicator
//!
//! The Stochastic Oscillator is a momentum indicator comparing the closing price
//! to the range of prices over a specified period. It generates overbought/oversold
//! signals when %K crosses above 80 or below 20.
//!
//! # Components
//! - %K (Fast): Main oscillator line
//! - %D (Slow): Signal line (SMA of %K)
//!
//! # Calculation
//! %K = 100 * (Close - Lowest Low) / (Highest High - Lowest Low)
//! %D = SMA(%K, d_period)
//!
//! A window without range, where the formula is 0 / 0, keeps the previous raw %K, and there is
//! no value before the first one; the %K smoothing and %D average those values.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Stochastic, Indicator};
//!
//! let mut stoch = Stochastic::new(14, 3, 3);
//! stoch.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::stepper::{rolling_max, rolling_min, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Stochastic Oscillator indicator
#[derive(Clone, Debug)]
pub struct Stochastic {
    k_period: usize,
    k_smooth: usize,
    d_period: usize,
    values: IndicatorValues,
    k_color: Color32,
    d_color: Color32,
    visible: bool,
}

impl Stochastic {
    /// Create a new Stochastic Oscillator
    ///
    /// # Arguments
    /// * `k_period` - Lookback period for %K (typically 14)
    /// * `k_smooth` - Smoothing period for %K (typically 3)
    /// * `d_period` - Period for %D signal line (typically 3)
    pub fn new(k_period: usize, k_smooth: usize, d_period: usize) -> Self {
        Self {
            k_period: k_period.max(1),
            k_smooth: k_smooth.max(1),
            d_period: d_period.max(1),
            values: IndicatorValues::new(),
            k_color: palette::BLUE,
            d_color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set colors for %K and %D lines
    pub fn with_colors(mut self, k_color: Color32, d_color: Color32) -> Self {
        self.k_color = k_color;
        self.d_color = d_color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for Stochastic {
    fn default() -> Self {
        Self::new(14, 3, 3)
    }
}

/// The raw %K of `close` in a window from `lowest` to `highest`, before clamping: NaN when the
/// window has no range and the close sits at it (0 / 0).
fn raw_k(close: f64, highest: f64, lowest: f64) -> f64 {
    100.0 * (close - lowest) / (highest - lowest)
}

impl Stochastic {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let min_period = self.k_period.saturating_add(self.k_smooth) - 1;
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate raw %K (Fast Stochastic)
        let highest = rolling_max(data.iter().map(|b| b.high), self.k_period);
        let lowest = rolling_min(data.iter().map(|b| b.low), self.k_period);
        let mut raw = Vec::with_capacity(data.len());
        // A window without range keeps the previous %K; there is none before the first.
        let mut carried = f64::NAN;
        for i in 0..data.len() {
            if i + 1 >= self.k_period {
                let k = raw_k(data[i].close, highest[i], lowest[i]);
                if !k.is_nan() {
                    carried = k.clamp(0.0, 100.0);
                }
            }
            raw.push(carried);
        }

        let smooth_k = helpers::sma(&raw, self.k_smooth);
        let d_values = helpers::sma(&smooth_k, self.d_period);

        // Build output values
        for i in 0..data.len() {
            if smooth_k[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[smooth_k[i], d_values[i]]);
            }
        }

        values
    }
}

impl Indicator for Stochastic {
    crate::studies::inputs::study_inputs!(self;
        k_period: length("K period", 1, usize::MAX),
        k_smooth: length("K smooth", 1, usize::MAX),
        d_period: length("D period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Stochastic"
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn desc(&self) -> &str {
        "Stochastic Oscillator - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        let behind = (self.k_period - 1)
            .saturating_add(self.k_smooth - 1)
            .saturating_add(self.d_period - 1);
        let usable = |b: &Bar| b.high.is_finite() && b.low.is_finite() && b.close.is_finite();
        update_window(
            &mut values,
            data,
            from,
            |data: &[Bar], from: usize| {
                let read = usable_behind(behind, usable)(data, from);
                // The first %K read from `read` carries an older one when its window has no range.
                // shortcut: such a window recomputes from bar 0; walk back to the last window with
                // a range if long flat histories show up in update profiles.
                let window: Vec<&Bar> = data[read..]
                    .iter()
                    .filter(|b| usable(b))
                    .take(self.k_period)
                    .collect();
                let carries = window.len() == self.k_period
                    && window.last().is_some_and(|last| {
                        let highest = window
                            .iter()
                            .map(|b| b.high)
                            .fold(f64::NEG_INFINITY, f64::max);
                        let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                        raw_k(last.close, highest, lowest).is_nan()
                    });
                if carries { 0 } else { read }
            },
            |data| over_finite_bars(data, usable, |data| self.compute(data)),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.k_color, self.d_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.k_color = colors[0];
        }
        if colors.len() > 1 {
            self.d_color = colors[1];
        }
    }

    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        Some([0, 1])
    }

    fn level_fill_color(&self) -> Color32 {
        palette::LIGHT_BLUE.gamma_multiply_u8(26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: Some((0.0, 100.0)),
            levels: vec![
                (80.0, palette::MUTED, "Overbought"),
                (20.0, palette::MUTED, "Oversold"),
                (50.0, palette::MUTED.gamma_multiply_u8(128), "Middle guide"),
            ],
            fill_between_levels: true,
        }
    }

    fn is_overlay(&self) -> bool {
        false // Stochastic is drawn in separate pane
    }

    fn line_count(&self) -> usize {
        2 // %K and %D
    }

    fn draws_only_plots(&self) -> bool {
        true
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
        vec![
            format!("%K({},{},{})", self.k_period, self.k_smooth, self.d_period),
            "%D".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stochastic(3, 2, 2) over a series that turns flat and one flat from the start. Raw %K is
    /// 100 (close - lowest low) / (highest high - lowest low): 20 on bar 2 (0 to 5, close 1), 40
    /// on bar 3 (0 to 5, close 2) and 200/3 on bar 4 (0 to 3, close 2); from bar 5 the window
    /// has no range and keeps 200/3. %K is the two-bar mean of raw %K and %D the two-bar mean of
    /// %K. Over flat bars only, no window has a range, so there is no value. Live updates must
    /// see the carried %K too, however far back it is.
    #[test]
    fn flat_windows_keep_the_last_k() {
        // (label, (high, low, close) bars, %K and %D by bar)
        type Row = (&'static str, Vec<(f64, f64, f64)>, [Vec<Option<f64>>; 2]);
        let flat = (2.0, 2.0, 2.0);
        let carried = Some(200.0 / 3.0);
        let mut turns_flat = vec![(4.0, 0.0, 2.0), (5.0, 1.0, 4.0), (3.0, 0.0, 1.0)];
        turns_flat.extend([flat; 9]);
        let k = [None, None, None, Some(30.0), Some(160.0 / 3.0)];
        let d = [None, None, None, None, Some(125.0 / 3.0), Some(60.0)];
        let rows: [Row; 2] = [
            (
                "turns flat",
                turns_flat,
                [
                    k.into_iter().chain([carried; 7]).collect(),
                    d.into_iter().chain([carried; 6]).collect(),
                ],
            ),
            (
                "flat from start",
                vec![flat; 8],
                [vec![None; 8], vec![None; 8]],
            ),
        ];
        for (label, hlc, lines) in rows {
            let bars = super::super::hlc_bars(&hlc);
            let mut stoch = Stochastic::new(3, 2, 2);
            stoch.calculate(&bars);
            super::super::assert_lines(label, &stoch, &lines);
            super::super::assert_replay_matches_batch(&Stochastic::new(3, 2, 2), &bars);
        }
    }
}
