use crate::model::Bar;
/// OHLC Volatility (Garman-Klass)
/// Uses OHLC data for more efficient volatility estimation
/// GK = 0.5 * ln(H/L)^2 - (2*ln(2)-1) * ln(C/O)^2
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct OhlcVolatility {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl OhlcVolatility {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![Color32::from_rgb(233, 30, 99)], // Pink
            visible: true,
        }
    }
}

impl Default for OhlcVolatility {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for OhlcVolatility {
    fn name(&self) -> &str {
        "GK Vol"
    }

    fn desc(&self) -> &str {
        "Garman-Klass OHLC Volatility - Efficient volatility estimator using OHLC"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let ln2 = 2.0_f64.ln();
        let coeff = 2.0 * ln2 - 1.0;

        // Calculate per-bar Garman-Klass values
        let mut gk_values = Vec::with_capacity(data.len());

        for bar in data {
            if bar.low > 0.0 && bar.open > 0.0 {
                let hl = (bar.high / bar.low).ln();
                let co = (bar.close / bar.open).ln();
                let gk = 0.5 * hl * hl - coeff * co * co;
                gk_values.push(gk);
            } else {
                gk_values.push(f64::NAN);
            }
        }

        // Rolling average over period, then sqrt and annualize
        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let window = &gk_values[i + 1 - self.period..=i];
            let valid: Vec<f64> = window.iter().filter(|x| !x.is_nan()).copied().collect();

            if valid.is_empty() {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let avg = valid.iter().sum::<f64>() / valid.len() as f64;
            // sqrt of average GK, annualized, as percentage
            let vol = if avg >= 0.0 {
                avg.sqrt() * 252.0_f64.sqrt() * 100.0
            } else {
                0.0
            };

            self.values.push(IndicatorValue::Single(vol));
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
        vec![format!("GK Vol({})", self.period)]
    }
}
