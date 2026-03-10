use crate::model::Bar;
/// Pretty Good Oscillator (PGO)
/// Measures price distance from moving avg in ATR units
/// Values > 3 or < -3 indicate potential reversal
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct PrettyGoodOscillator {
    /// Period for calculations (typically 14)
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl PrettyGoodOscillator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan,
            visible: true,
        }
    }

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

impl Indicator for PrettyGoodOscillator {
    fn name(&self) -> &str {
        "PGO"
    }

    fn desc(&self) -> &str {
        "Pretty Good Oscillator - Price distance in ATR units"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
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
                self.values.push(IndicatorValue::None);
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
                    self.values.push(IndicatorValue::None);
                } else {
                    // PGO = (Close - SMA) / ATR
                    let pgo = (data[i].close - sma) / atr;
                    self.values.push(IndicatorValue::Single(pgo));
                }
            }
        }
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
        vec![format!("PGO({})", self.period)]
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
        }
    }

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
        if let IndicatorValue::Single(v) = pgo.values.last().unwrap() {
            assert!(*v > 0.0, "PGO should be positive after upward move");
        }
    }
}
