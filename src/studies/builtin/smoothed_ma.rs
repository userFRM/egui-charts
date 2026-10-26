use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Smoothed Moving Average (Wilder's Smoothing)
#[derive(Clone)]
pub struct SmoothedMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl SmoothedMA {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.indicators.ma,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // First value is SMA
        let first_sma: f64 =
            data[..self.period].iter().map(|b| b.close).sum::<f64>() / self.period as f64;

        for _i in 0..self.period - 1 {
            values.push(IndicatorValue::None);
        }
        values.push(IndicatorValue::Single(first_sma));

        let mut prev = first_sma;
        for i in self.period..data.len() {
            let smoothed = (prev * (self.period as f64 - 1.0) + data[i].close) / self.period as f64;
            values.push(IndicatorValue::Single(smoothed));
            prev = smoothed;
        }

        values
    }
}

impl Default for SmoothedMA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for SmoothedMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "SMMA"
    }

    fn desc(&self) -> &str {
        "Smoothed Moving Average - Wilder's smoothing method"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
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
        vec![format!("SMMA({})", self.period)]
    }
}
