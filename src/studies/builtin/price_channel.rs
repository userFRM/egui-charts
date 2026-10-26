use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Price Channel
/// Simple highest high and lowest low channel
/// Also known as "N-bar high/low"
#[derive(Clone)]
pub struct PriceChannel {
    /// Lookback period
    period: usize,
    values: IndicatorValues,
    /// Colors: [Upper, Lower, Middle]
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceChannel {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Upper
                DESIGN_TOKENS.semantic.extended.error,   // Red - Lower
                DESIGN_TOKENS.semantic.extended.gray,    // Gray - Middle (neutral)
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

impl Default for PriceChannel {
    fn default() -> Self {
        Self::new(20)
    }
}

impl PriceChannel {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                let middle = (highest + lowest) / 2.0;

                values.push_lines(&[highest, lowest, middle]);
            }
        }

        values
    }
}

impl Indicator for PriceChannel {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Price Channel"
    }

    fn desc(&self) -> &str {
        "Price Channel - Highest high / Lowest low bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
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
        true // Overlay on price chart
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
            format!("Lower({})", self.period),
            "Middle".to_string(),
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

    #[test]
    fn test_price_channel() {
        let mut pc = PriceChannel::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
        ];

        pc.calculate(&data);

        assert_eq!(pc.values.len(), 5);

        // Last value should be highest=110, lowest=98, middle=104
        let rows = pc.values.to_rows();
        let IndicatorValue::Multiple(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", pc.values.to_rows().last().unwrap());
        };
        assert_eq!(v.len(), 3);
        assert!((v[0] - 110.0).abs() < 0.01); // Upper
        assert!((v[1] - 98.0).abs() < 0.01); // Lower
        assert!((v[2] - 104.0).abs() < 0.01); // Middle
    }
}
