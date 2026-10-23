use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
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
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl BalanceOfPower {
    pub fn new(smooth_period: usize) -> Self {
        Self {
            smooth_period: smooth_period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has finite prices.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
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
    fn name(&self) -> &str {
        "BoP"
    }

    fn desc(&self) -> &str {
        "Balance of Power - Measures buyer/seller strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| {
                [b.open, b.high, b.low, b.close]
                    .iter()
                    .all(|p| p.is_finite())
            },
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &[IndicatorValue] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_bop_bullish() {
        let mut bop = BalanceOfPower::new(0);

        // Bullish bar: close at high
        let data = vec![make_bar(100.0, 110.0, 95.0, 110.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            // (110 - 100) / (110 - 95) = 10/15 = 0.667
            assert!((v - 0.667).abs() < 0.01);
        }
    }

    #[test]
    fn test_bop_bearish() {
        let mut bop = BalanceOfPower::new(0);

        // Bearish bar: close at low
        let data = vec![make_bar(100.0, 105.0, 90.0, 90.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            // (90 - 100) / (105 - 90) = -10/15 = -0.667
            assert!((v + 0.667).abs() < 0.01);
        }
    }

    #[test]
    fn test_bop_neutral() {
        let mut bop = BalanceOfPower::new(0);

        // Neutral bar: close = open
        let data = vec![make_bar(100.0, 110.0, 90.0, 100.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            assert!((v - 0.0).abs() < 0.01);
        }
    }
}
