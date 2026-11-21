use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Hamming Weighted Moving Average
#[derive(Clone, Debug)]
pub struct HammingMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl HammingMA {
    /// Create with a lookback of `period` bars, clamped to at least 1. A period of 1
    /// draws the source itself.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::GREEN,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Hamming window weights for `period` bars. A one-bar window is the single
    /// weight 1, where the general formula would divide by zero.
    fn hamming_weights(period: usize) -> Vec<f64> {
        if period == 1 {
            return vec![1.0];
        }
        let n = period as f64;
        (0..period)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1.0)).cos())
            .collect()
    }
}

impl Default for HammingMA {
    fn default() -> Self {
        Self::new(10)
    }
}

impl HammingMA {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        let weights = Self::hamming_weights(self.period);
        let weight_sum: f64 = weights.iter().sum();

        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let weighted_sum: f64 = data[start..=i]
                    .iter()
                    .zip(weights.iter())
                    .map(|(bar, w)| bar.close * w)
                    .sum();
                let hma = weighted_sum / weight_sum;
                values.push(IndicatorValue::Single(hma));
            }
        }
        values
    }
}

impl Indicator for HammingMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Hamming MA"
    }

    fn desc(&self) -> &str {
        "Hamming Weighted Moving Average - Hamming window function weighting"
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
            |_data, from| from.saturating_sub(self.period - 1),
            |data| self.compute(data),
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
        vec![format!("Hamming({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;
    use chrono::{TimeZone, Utc};

    /// The shortest accepted period draws the source, from the constructor and from the
    /// Period input alike, and longer periods keep the Hamming weights.
    #[test]
    fn shortest_period_draws_the_source() {
        let closes = [100.5, 101.25, 99.75, 102.0];
        let bars: Vec<Bar> = closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect();
        let mut by_input = HammingMA::default();
        by_input.set_input("period", InputValue::Length(1)).unwrap();
        // Period 3 weighs 0.08, 1 and 0.08: (100.5 * 0.08 + 101.25 + 99.75 * 0.08) / 1.16
        // = 11727 / 116, then (101.25 * 0.08 + 99.75 + 102 * 0.08) / 1.16 = 11601 / 116.
        let close = |g: &Option<f64>, w: &Option<f64>| match (g, w) {
            (Some(g), Some(w)) => (g - w).abs() <= 1e-12 * w.abs(),
            (g, w) => g == w,
        };
        let mut failures = Vec::new();
        for (label, mut study, want) in [
            ("new(1)", HammingMA::new(1), closes.map(Some)),
            ("Period input 1", by_input, closes.map(Some)),
            (
                "new(3)",
                HammingMA::new(3),
                [None, None, Some(11727.0 / 116.0), Some(11601.0 / 116.0)],
            ),
        ] {
            study.calculate(&bars);
            let got: Vec<Option<f64>> = (0..bars.len())
                .map(|i| study.values().value(0, i))
                .collect();
            if !got.iter().zip(&want).all(|(g, w)| close(g, w)) {
                failures.push(format!("{label}: {got:?}, want {want:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
