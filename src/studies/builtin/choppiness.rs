use super::over_finite_bars;
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
            period: period.max(1),
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

/// Construct with the conventional default parameters.
impl Default for ChoppinessIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl ChoppinessIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();
        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
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
                values.push(IndicatorValue::None);
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
                    values.push(IndicatorValue::None);
                } else {
                    // CHOP = 100 * LOG10(SUM(TR, n) / (Highest High - Lowest Low)) / LOG10(n)
                    let chop = 100.0 * (atr_sum / range).log10() / (self.period as f64).log10();
                    values.push(IndicatorValue::Single(chop));
                }
            }
        }

        values
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
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
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
        vec![format!("CHOP({})", self.period)]
    }
}
