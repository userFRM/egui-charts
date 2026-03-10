use crate::model::Bar;
/// Close-to-Close Volatility
/// Standard deviation of log returns, annualized by sqrt(252)
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct CloseToCloseVol {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl CloseToCloseVol {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
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

impl Indicator for CloseToCloseVol {
    fn name(&self) -> &str {
        "C2C Vol"
    }

    fn desc(&self) -> &str {
        "Close-to-Close Volatility - Annualized standard deviation of log returns"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in data {
                self.values.push(IndicatorValue::None);
            }
            return;
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
                self.values.push(IndicatorValue::None);
                continue;
            }

            let window = &log_returns[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.len() < 2 {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let mean = valid.iter().sum::<f64>() / valid.len() as f64;
            let variance =
                valid.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (valid.len() - 1) as f64;
            let std_dev = variance.sqrt();

            // Annualize: std_dev * sqrt(252) * 100 (as percentage)
            let annualized = std_dev * 252.0_f64.sqrt() * 100.0;
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
        vec![format!("C2C Vol({})", self.period)]
    }
}
