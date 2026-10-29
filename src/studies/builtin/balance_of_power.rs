use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Balance of Power (BoP): the strength of buyers against sellers,
/// `(close - open) / (high - low)` per bar, smoothed with a simple average of
/// `smooth_period` bars.
///
/// - A bar without range (`high == low`) counts as 0.
/// - With smoothing of length `n`, bars 0 to `n - 2` have no value, and a series shorter
///   than `n` has none on any bar. Length 1 (or 0) keeps the unsmoothed ratio on every bar.
/// - A bar with a non-finite price has no value, and the average is taken as if it were not
///   in the series.
#[derive(Clone)]
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
            color: palette::RED, // Teal
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has finite prices.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let raw: Vec<f64> = data
            .iter()
            .map(|bar| {
                let range = bar.high - bar.low;
                if range == 0.0 {
                    0.0
                } else {
                    (bar.close - bar.open) / range
                }
            })
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

/// Construct with the conventional default parameters.
impl Default for BalanceOfPower {
    fn default() -> Self {
        Self::new(14)
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
            usable_behind(self.smooth_period - 1, |b| {
                [b.open, b.high, b.low, b.close]
                    .iter()
                    .all(|p| p.is_finite())
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| {
                        [b.open, b.high, b.low, b.close]
                            .iter()
                            .all(|p| p.is_finite())
                    },
                    |data| self.compute(data),
                )
            },
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
