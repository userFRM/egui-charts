use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Balance of Power (BoP): the strength of buyers against sellers,
/// `(close - open) / (high - low)` per bar, optionally smoothed with a simple average of
/// `smooth_period` bars. The default does not smooth.
///
/// - A bar without range (`high == low`) has no value: the ratio divides zero by zero there.
/// - With smoothing of length `n`, bars 0 to `n - 2` have no value, and a series shorter
///   than `n` has none on any bar. Length 1 (or 0) keeps the unsmoothed ratio on every bar.
/// - A bar with a non-finite price has no value either. The average skips every bar without a
///   value, as if it were not in the series, so it is the mean of the last `n` defined ratios.
#[derive(Clone, Debug)]
pub struct BalanceOfPower {
    /// Length of the simple average (0 or 1 = no smoothing)
    smooth_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl BalanceOfPower {
    /// Create with `smooth_period` in bars, each clamped to at least one.
    pub fn new(smooth_period: usize) -> Self {
        Self {
            smooth_period: smooth_period.max(1),
            values: IndicatorValues::new(),
            color: palette::RED,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Whether `bar` has a ratio: finite prices and a range.
    fn defined(bar: &Bar) -> bool {
        [bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|p| p.is_finite())
            && bar.high != bar.low
    }

    /// The values over `data`, every bar of which is [`defined`](Self::defined).
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let raw: Vec<f64> = data
            .iter()
            .map(|bar| (bar.close - bar.open) / (bar.high - bar.low))
            .collect();
        (0..raw.len())
            .map(|i| match i + 1 {
                n if n < self.smooth_period => IndicatorValue::None,
                n => IndicatorValue::Single(
                    raw[n - self.smooth_period..n].iter().sum::<f64>() / self.smooth_period as f64,
                ),
            })
            .collect()
    }
}

/// The unsmoothed ratio of every bar, as the study is conventionally drawn.
impl Default for BalanceOfPower {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Indicator for BalanceOfPower {
    crate::studies::inputs::study_inputs!(self;
        smooth_period: length("Smooth period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "BoP"
    }

    fn desc(&self) -> &str {
        "Balance of Power - Measures buyer/seller strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.smooth_period - 1, Self::defined),
            |data| over_finite_bars(data, Self::defined, |data| self.compute(data)),
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
        if self.smooth_period > 1 {
            vec![format!("BoP({})", self.smooth_period)]
        } else {
            vec!["BoP".to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat live bar has no value, and smoothing averages the last defined ratios across it.
    #[test]
    fn a_bar_without_range_has_no_value_bar_by_bar() {
        // Ratios 1/2, none, -1/4 and 1; two-bar averages 1/8 and 3/8.
        let bars: Vec<Bar> = [
            (0.0, 2.0, 0.0, 1.0),
            (5.0, 5.0, 5.0, 5.0),
            (2.0, 4.0, 0.0, 1.0),
            (1.0, 3.0, 1.0, 3.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close))| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i as i64);
            Bar::new(time, open, high, low, close, 1.0)
        })
        .collect();
        for (smooth, expected) in [
            (1, [Some(0.5), None, Some(-0.25), Some(1.0)]),
            (2, [None, None, Some(0.125), Some(0.375)]),
        ] {
            let mut study = BalanceOfPower::new(smooth);
            for end in 1..=bars.len() {
                study.update(&bars[..end], end - 1);
            }
            let values: Vec<_> = (0..bars.len())
                .map(|bar| study.values().value(0, bar))
                .collect();
            assert_eq!(values, expected, "smoothing {smooth}");
        }
    }
}
