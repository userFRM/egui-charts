use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Pretty Good Oscillator (PGO)
/// Measures price distance from moving avg in ATR units
/// Values > 3 or < -3 indicate potential reversal
#[derive(Clone)]
pub struct PrettyGoodOscillator {
    /// Period for calculations (typically 14)
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl PrettyGoodOscillator {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
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

impl Default for PrettyGoodOscillator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl PrettyGoodOscillator {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate True Range
        let mut tr = Vec::with_capacity(data.len());
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            let high_low = data[i].high - data[i].low;
            let high_close = (data[i].high - data[i - 1].close).abs();
            let low_close = (data[i].low - data[i - 1].close).abs();
            tr.push(high_low.max(high_close).max(low_close));
        }

        // Calculate SMA of closes and EMA of TR (ATR)
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                // SMA of closes
                let sma: f64 = data[i + 1 - self.period..=i]
                    .iter()
                    .map(|b| b.close)
                    .sum::<f64>()
                    / self.period as f64;

                // SMA of TR (ATR)
                let atr: f64 = tr[i + 1 - self.period..=i].iter().sum::<f64>() / self.period as f64;

                if atr == 0.0 {
                    values.push(IndicatorValue::None);
                } else {
                    // PGO = (Close - SMA) / ATR
                    let pgo = (data[i].close - sma) / atr;
                    values.push(IndicatorValue::Single(pgo));
                }
            }
        }

        values
    }
}

impl Indicator for PrettyGoodOscillator {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "PGO"
    }

    fn desc(&self) -> &str {
        "Pretty Good Oscillator - Price distance in ATR units"
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
            usable_behind(self.period, |b| {
                b.high.is_finite() && b.low.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
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
        vec![format!("PGO({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_range_bar as make_bar;

    #[test]
    fn test_pgo_calculation() {
        let mut pgo = PrettyGoodOscillator::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(105.0, 101.0, 103.0),
            make_bar(106.0, 102.0, 104.0),
            make_bar(110.0, 105.0, 109.0), // Big move up
        ];

        pgo.calculate(&data);

        assert_eq!(pgo.values.len(), 6);

        // Last value should be positive (price above avg)
        let rows = pgo.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", pgo.values.to_rows().last().unwrap());
        };
        assert!(*v > 0.0, "PGO should be positive after upward move");
    }
}
