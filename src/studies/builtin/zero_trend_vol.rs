use crate::model::Bar;
/// Zero-Trend Volatility (Parkinson)
/// Uses high-low range for volatility estimation assuming zero drift
/// Parkinson = sqrt(1/(4*n*ln(2)) * sum(ln(H/L)^2))
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct ZeroTrendVol {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl ZeroTrendVol {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![Color32::from_rgb(103, 58, 183)], // Deep purple
            visible: true,
        }
    }
}

impl Default for ZeroTrendVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for ZeroTrendVol {
    fn name(&self) -> &str {
        "Parkinson Vol"
    }

    fn desc(&self) -> &str {
        "Zero-Trend Volatility (Parkinson) - High-low range volatility estimator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
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
                self.values.push(IndicatorValue::None);
                continue;
            }

            let window = &hl_sq[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.is_empty() {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let n = valid.len() as f64;
            let sum = valid.iter().sum::<f64>();

            // Parkinson = sqrt(1 / (4 * n * ln(2)) * sum(ln(H/L)^2))
            let parkinson = (sum / (4.0 * n * ln2)).sqrt();

            // Annualize and convert to percentage
            let annualized = parkinson * 252.0_f64.sqrt() * 100.0;
            self.values.push(IndicatorValue::Single(annualized));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
