use crate::model::Bar;
/// Choppiness Index
/// Measures market trendiness vs choppiness
/// Values close to 100 = choppy market, values close to 0 = trending market
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ChoppinessIndex {
    /// Lookback period (typically 14)
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ChoppinessIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for ChoppinessIndex {
    fn name(&self) -> &str {
        "CHOP"
    }

    fn desc(&self) -> &str {
        "Choppiness Index - Measures market trendiness"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate True Range for each bar
        let mut tr_values = Vec::with_capacity(data.len());
        tr_values.push(data[0].high - data[0].low); // First bar TR is just range

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            let tr = (current.high - current.low)
                .max((current.high - prev.close).abs())
                .max((current.low - prev.close).abs());

            tr_values.push(tr);
        }

        // Calculate Choppiness Index
        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                // Sum of TR over period
                let atr_sum: f64 = tr_values[i + 1 - self.period..=i].iter().sum();

                // Highest High and Lowest Low over period
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                let range = highest - lowest;

                if range == 0.0 {
                    self.values.push(IndicatorValue::None);
                } else {
                    // CHOP = 100 * LOG10(SUM(TR, n) / (Highest High - Lowest Low)) / LOG10(n)
                    let chop = 100.0 * (atr_sum / range).log10() / (self.period as f64).log10();
                    self.values.push(IndicatorValue::Single(chop));
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
        vec![format!("CHOP({})", self.period)]
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
    fn test_choppiness_calculation() {
        let mut chop = ChoppinessIndex::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(105.0, 101.0, 103.0),
            make_bar(106.0, 102.0, 104.0),
            make_bar(107.0, 103.0, 105.0),
            make_bar(108.0, 104.0, 106.0),
        ];

        chop.calculate(&data);

        assert_eq!(chop.values.len(), 7);

        // First 5 should be None
        for i in 0..5 {
            assert!(matches!(chop.values[i], IndicatorValue::None));
        }

        // Values should be between 0 and 100
        for value in &chop.values {
            if let IndicatorValue::Single(v) = value {
                assert!(
                    *v >= 0.0 && *v <= 100.0,
                    "CHOP should be between 0-100, got {}",
                    v
                );
            }
        }
    }

    #[test]
    fn test_trending_market() {
        let mut chop = ChoppinessIndex::new(5);

        // Strong trend - each bar higher than previous
        let data: Vec<Bar> = (0..10)
            .map(|i| {
                make_bar(
                    102.0 + i as f64 * 5.0,
                    98.0 + i as f64 * 5.0,
                    100.0 + i as f64 * 5.0,
                )
            })
            .collect();

        chop.calculate(&data);

        // In a strong trend, CHOP should be relatively low (< 50)
        if let IndicatorValue::Single(v) = chop.values.last().unwrap() {
            assert!(*v < 50.0, "CHOP should be low in strong trend, got {}", v);
        }
    }
}
