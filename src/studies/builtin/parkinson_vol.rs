use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Parkinson high-low range volatility.
///
/// `sqrt(mean(ln(high / low)^2) / (4 * ln(2))) * sqrt(252) * 100`.
/// Annualisation assumes daily bars and 252 trading days per year (see #672).
#[derive(Clone)]
pub struct ParkinsonVol {
    period: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl ParkinsonVol {
    /// Create a Parkinson study with a lookback of `period` bars (at least 1).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(103, 58, 183)], // Deep purple
            visible: true,
        }
    }
}

impl Default for ParkinsonVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl ParkinsonVol {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        let ln2 = 2.0_f64.ln();

        // Calculate per-bar ln(H/L)^2
        let mut hl_sq = Vec::with_capacity(data.len());

        for bar in data {
            if bar.low > 0.0 {
                let ln_hl = (bar.high / bar.low).ln();
                hl_sq.push(ln_hl * ln_hl);
            } else {
                hl_sq.push(f64::NAN);
            }
        }

        // Rolling Parkinson estimator
        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let window = &hl_sq[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.is_empty() {
                values.push(IndicatorValue::None);
                continue;
            }

            let n = valid.len() as f64;
            let sum = valid.iter().sum::<f64>();

            // Parkinson = sqrt(1 / (4 * n * ln(2)) * sum(ln(H/L)^2))
            let parkinson = (sum / (4.0 * n * ln2)).sqrt();

            // Annualize and convert to percentage
            let annualized = parkinson * 252.0_f64.sqrt() * 100.0;
            values.push(IndicatorValue::Single(annualized));
        }

        values
    }
}

impl Indicator for ParkinsonVol {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "Parkinson Vol"
    }

    fn desc(&self) -> &str {
        "Parkinson High-Low Range Volatility - Annualised percentage assuming daily bars"
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
        vec![format!("Parkinson Vol({})", self.period)]
    }
}
