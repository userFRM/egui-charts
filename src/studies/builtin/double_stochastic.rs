use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Double Stochastic
/// Applies the stochastic formula twice for smoother signals
/// More responsive to overbought/oversold conditions
/// A window without range keeps the previous value of its stage, and there is no value before the
/// first window with one; the smoothing averages those values.
#[derive(Clone, Debug)]
pub struct DoubleStochastic {
    /// Period for stochastic calculation
    period: usize,
    /// Smoothing period
    smooth: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl DoubleStochastic {
    /// Create with `period`, `smooth` in bars, each clamped to at least one.
    pub fn new(period: usize, smooth: usize) -> Self {
        Self {
            period: period.max(1),
            smooth: smooth.max(1),
            values: IndicatorValues::new(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for DoubleStochastic {
    fn default() -> Self {
        Self::new(10, 3)
    }
}

impl DoubleStochastic {
    /// The values over `data`, every bar of which has finite inputs, and whether a window of
    /// either stage has no range before that stage's first window with one, so that a calculation
    /// that starts at an earlier bar can give it a value carried from there.
    fn compute(&self, data: &[Bar]) -> (IndicatorValues, bool) {
        let mut values = IndicatorValues::new();
        let min_period = self.period.saturating_add(self.smooth.saturating_mul(2));
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return (values, false);
        }

        // A window without range keeps the previous value of its stage; there is none before the
        // first.
        let mut opens_flat = false;

        // First stochastic
        let mut stoch1: Vec<f64> = Vec::with_capacity(data.len());
        let mut carried = f64::NAN;

        for i in 0..data.len() {
            if i < self.period - 1 {
                stoch1.push(f64::NAN);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let high = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                let k = (data[i].close - low) / (high - low) * 100.0;
                if k.is_finite() {
                    carried = k;
                } else {
                    opens_flat |= carried.is_nan();
                }
                stoch1.push(carried);
            }
        }

        // Smooth the first stochastic
        let smooth_stoch1 = helpers::sma(&stoch1, self.smooth);

        // Second stochastic on the smoothed values
        let mut stoch2: Vec<f64> = Vec::with_capacity(data.len());
        let mut carried = f64::NAN;

        for i in 0..data.len() {
            let start = if i >= self.period - 1 {
                i + 1 - self.period
            } else {
                0
            };

            if i < self.period + self.smooth - 1 {
                stoch2.push(f64::NAN);
            } else {
                let window = &smooth_stoch1[start..=i];
                let valid_values: Vec<f64> =
                    window.iter().filter(|v| v.is_finite()).cloned().collect();

                if valid_values.len() >= self.period {
                    let high = valid_values
                        .iter()
                        .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                    let low = valid_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));

                    let k = (smooth_stoch1[i] - low) / (high - low) * 100.0;
                    if k.is_finite() {
                        carried = k;
                    } else {
                        opens_flat |= carried.is_nan();
                    }
                    stoch2.push(carried);
                } else {
                    stoch2.push(f64::NAN);
                }
            }
        }

        // Final smoothing
        let ds = helpers::sma(&stoch2, self.smooth);

        for i in 0..data.len() {
            if i < min_period - 1 || ds[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push(IndicatorValue::Single(ds[i]));
            }
        }

        (values, opens_flat)
    }
}

impl Indicator for DoubleStochastic {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smooth: length("Smooth", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "DS"
    }

    fn desc(&self) -> &str {
        "Double Stochastic - Double-smoothed stochastic oscillator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        let usable = |b: &Bar| b.high.is_finite() && b.low.is_finite() && b.close.is_finite();
        let behind = self.period.saturating_add(self.smooth).saturating_mul(2);
        update_window(
            &mut values,
            data,
            from,
            |data: &[Bar], from: usize| {
                let read = usable_behind(behind, usable)(data, from);
                // A stage whose first window read from `read` has no range carries an older value.
                // shortcut: such a window recomputes from bar 0; walk back to the last window with
                // a range in each stage if long flat histories show up in update profiles.
                let opens_flat = || {
                    let kept: Vec<Bar> =
                        data[read..].iter().filter(|b| usable(b)).cloned().collect();
                    self.compute(&kept).1
                };
                if read > 0 && opens_flat() { 0 } else { read }
            },
            |data| over_finite_bars(data, usable, |data| self.compute(data).0),
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
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
        vec![format!("DS({},{})", self.period, self.smooth)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DS(3, 2) over a series that turns flat, one that keeps closing at a new high and one flat
    /// from the start. Stage 1, 100 (close - lowest low) / (highest high - lowest low) over three
    /// bars, is 20, 250/3, 50, 20, 250/3, 50 on bars 2 to 7 of the swing, then 20 and 25 as the
    /// bars turn flat at 3, which it keeps from bar 10. Its two-bar mean is 155/3, 200/3, 35,
    /// 155/3, 200/3, 35, 45/2, then 25. Stage 2 places that mean among its last three: 0, 1000/19,
    /// 100, 0, 0, 20 and 100 on bars 5 to 11, and from bar 12 the window has no range and keeps
    /// 100. DS, the two-bar mean of stage 2 shown from bar 6, is 500/19, 1450/19, 50, 0, 10, 60,
    /// then 100. Closing at each new high holds stage 1 at 100 from bar 8, so stage 2 is 100 and
    /// keeps it from bar 11, where its window has no range. Over flat bars only, no window has a
    /// range, so there is no value. Live updates must see the carried value of either stage,
    /// however far back it is.
    #[test]
    fn flat_windows_keep_the_last_value() {
        // (label, (high, low, close) bars, values by bar)
        type Row = (&'static str, Vec<(f64, f64, f64)>, Vec<Option<f64>>);
        let swing = [
            (4.0, 0.0, 2.0),
            (5.0, 1.0, 4.0),
            (3.0, 0.0, 1.0),
            (6.0, 2.0, 5.0),
            (5.0, 3.0, 3.0),
            (4.0, 1.0, 2.0),
            (7.0, 3.0, 6.0),
            (6.0, 2.0, 4.0),
        ];
        let rises: Vec<_> = (0..14)
            .map(|i| (10.0 + f64::from(i), 9.0 + f64::from(i), 10.0 + f64::from(i)))
            .collect();
        let opening = || {
            [None; 6]
                .into_iter()
                .chain([Some(500.0 / 19.0), Some(1450.0 / 19.0)])
        };
        let rows: [Row; 3] = [
            (
                "turns flat",
                [&swing[..], &[(3.0, 3.0, 3.0); 14][..]].concat(),
                opening()
                    .chain([Some(50.0), Some(0.0), Some(10.0), Some(60.0)])
                    .chain([Some(100.0); 10])
                    .collect(),
            ),
            (
                "rises at the high",
                [&swing[..], &rises[..]].concat(),
                opening().chain([Some(100.0); 14]).collect(),
            ),
            ("flat from start", vec![(2.0, 2.0, 2.0); 10], vec![None; 10]),
        ];
        for (label, hlc, want) in rows {
            let bars = super::super::hlc_bars(&hlc);
            let mut study = DoubleStochastic::new(3, 2);
            study.calculate(&bars);
            super::super::assert_lines(label, &study, &[want]);
            super::super::assert_replay_matches_batch(&DoubleStochastic::new(3, 2), &bars);
        }
    }
}
