use super::over_finite_bars;
use crate::model::Bar;
/// Close-to-Close Volatility
/// Standard deviation of log returns, annualized by sqrt(252)
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

#[derive(Clone)]
pub struct CloseToCloseVol {
    period: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl CloseToCloseVol {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(156, 39, 176)], // Purple
            visible: true,
        }
    }
}

impl Default for CloseToCloseVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl CloseToCloseVol {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < 2 {
            for _ in data {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate log returns
        let mut log_returns = Vec::with_capacity(data.len());
        log_returns.push(f64::NAN); // No return for first bar

        for i in 1..data.len() {
            if data[i - 1].close > 0.0 && data[i].close > 0.0 {
                log_returns.push((data[i].close / data[i - 1].close).ln());
            } else {
                log_returns.push(f64::NAN);
            }
        }

        // Rolling standard deviation of log returns
        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let window = &log_returns[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.len() < 2 {
                values.push(IndicatorValue::None);
                continue;
            }

            let mean = valid.iter().sum::<f64>() / valid.len() as f64;
            let variance =
                valid.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (valid.len() - 1) as f64;
            let std_dev = variance.sqrt();

            // Annualize: std_dev * sqrt(252) * 100 (as percentage)
            let annualized = std_dev * 252.0_f64.sqrt() * 100.0;
            values.push(IndicatorValue::Single(annualized));
        }

        values
    }
}

impl Indicator for CloseToCloseVol {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "C2C Vol"
    }

    fn desc(&self) -> &str {
        "Close-to-Close Volatility - Annualized standard deviation of log returns"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.colors = colors;
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
        vec![format!("C2C Vol({})", self.period)]
    }
}
