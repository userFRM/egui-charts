use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Acceleration Bands (Price Headley), a price envelope that widens on wide bars.
///
/// With `r = (high - low) / (high + low)` per bar (0 when `high + low` is 0):
/// upper = SMA(high * (1 + factor * r), period), middle = SMA(close, period),
/// lower = SMA(low * (1 - factor * r), period).
#[derive(Clone)]
pub struct AccelerationBands {
    /// Period for calculation
    period: usize,
    /// Band width factor (typically 4.0)
    factor: f64,
    values: IndicatorValues,
    /// Colors: [Upper, Middle, Lower]
    colors: Vec<Color32>,
    visible: bool,
}

impl AccelerationBands {
    /// `factor` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, factor: f64) -> Self {
        Self {
            period: period.max(1),
            factor: clamp_param(factor, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            colors: vec![
                palette::GREEN, // Green - Upper
                palette::GOLD,  // Amber - Middle
                palette::RED,   // Red - Lower
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for AccelerationBands {
    fn default() -> Self {
        Self::new(20, 4.0)
    }
}

impl Indicator for AccelerationBands {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        factor: number("Factor", 0.0, f64::MAX)
    ; {});

    fn name(&self) -> &str {
        "AccBands"
    }

    fn desc(&self) -> &str {
        "Acceleration Bands - Volatility-based price envelope"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                let n = self.period as f64;
                let (mut upper, mut middle, mut lower) = (0.0, 0.0, 0.0);
                for b in window {
                    let r = if b.high + b.low != 0.0 {
                        (b.high - b.low) / (b.high + b.low)
                    } else {
                        0.0
                    };
                    upper += b.high * (1.0 + self.factor * r);
                    middle += b.close;
                    lower += b.low * (1.0 - self.factor * r);
                }

                self.values.push_lines(&[upper / n, middle / n, lower / n]);
            }
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        3
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
            format!("Upper({})", self.period),
            "Middle".to_string(),
            format!("Lower({})", self.period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    /// Reference values from Price Headley's definition ("Big Trends in Trading", 2002), computed
    /// independently: upper = SMA(H * (1 + f * (H - L) / (H + L)), n), middle = SMA(C, n),
    /// lower = SMA(L * (1 - f * (H - L) / (H + L)), n). The bar ranges vary from 1 to 12, so
    /// averaging each bar's widened high differs from widening the average high.
    #[test]
    fn matches_reference_values() {
        let bars = [
            (101.0, 100.0, 100.5),
            (110.0, 98.0, 108.0),
            (104.0, 102.0, 103.0),
            (115.0, 105.0, 112.0),
            (111.0, 109.5, 110.0),
            (120.0, 108.0, 118.0),
        ];
        let expected = [
            [115.47780019494263, 103.83333333333333, 90.47780019494263],
            [126.44418041505419, 107.66666666666667, 86.44418041505419],
            [119.32277800793752, 108.33333333333333, 96.82277800793752],
            [131.73088565569768, 113.33333333333333, 92.56421898903102],
        ];

        let mut ab = AccelerationBands::new(3, 4.0);
        let data: Vec<Bar> = bars.iter().map(|&(h, l, c)| make_bar(h, l, c)).collect();
        ab.calculate(&data);

        let mut failures = Vec::new();
        if ab.line_cnt() != 3 {
            failures.push(format!("line_cnt {}, want 3", ab.line_cnt()));
        }
        assert_eq!(ab.values.len(), bars.len());
        assert!(matches!(
            ab.values.to_rows()[..2],
            [IndicatorValue::None, IndicatorValue::None]
        ));
        for (i, want) in expected.iter().enumerate() {
            let IndicatorValue::Multiple(got) = &ab.values.to_rows()[i + 2] else {
                panic!("bar {}: no value", i + 2);
            };
            assert_eq!(got.len(), 3, "bar {}", i + 2);
            for (line, (g, w)) in got.iter().zip(want).enumerate() {
                if ((g - w) / w).abs() >= 1e-12 {
                    failures.push(format!("bar {} line {line}: got {g}, want {w}", i + 2));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
