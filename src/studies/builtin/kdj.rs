use crate::model::Bar;
/// KDJ Indicator
/// Popular in Chinese stock markets, derived from Stochastic
/// K%, D%, and J% lines with J being more volatile
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct KDJ {
    /// Period for %K calculation (typically 9)
    period: usize,
    /// Smoothing period for %K (typically 3)
    k_smooth: usize,
    /// Smoothing period for %D (typically 3)
    d_smooth: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [K, D, J]
    colors: Vec<Color32>,
    visible: bool,
}

impl KDJ {
    pub fn new(period: usize, k_smooth: usize, d_smooth: usize) -> Self {
        Self {
            period,
            k_smooth,
            d_smooth,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - K
                DESIGN_TOKENS.semantic.extended.warning, // Orange - D
                DESIGN_TOKENS.semantic.extended.purple,  // Purple - J
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

impl Default for KDJ {
    fn default() -> Self {
        Self::new(9, 3, 3)
    }
}

impl Indicator for KDJ {
    fn name(&self) -> &str {
        "KDJ"
    }

    fn desc(&self) -> &str {
        "KDJ - Stochastic-derived indicator with K, D, J lines"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate raw stochastic %K (RSV)
        let mut rsv = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < self.period - 1 {
                rsv.push(f64::NAN);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                let range = highest - lowest;
                if range == 0.0 {
                    rsv.push(50.0);
                } else {
                    let value = (data[i].close - lowest) / range * 100.0;
                    rsv.push(value);
                }
            }
        }

        // Smooth RSV to get %K
        let k_line = Self::calculate_sma(&rsv, self.k_smooth);

        // Smooth %K to get %D
        let d_line = Self::calculate_sma(&k_line, self.d_smooth);

        // Calculate J = 3*K - 2*D
        for i in 0..data.len() {
            if k_line[i].is_nan() || d_line[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                let k = k_line[i];
                let d = d_line[i];
                let j = 3.0 * k - 2.0 * d;
                self.values.push(IndicatorValue::Multiple(vec![k, d, j]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![
            format!("K({})", self.period),
            format!("D({})", self.d_smooth),
            "J".to_string(),
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
        }
    }

    #[test]
    fn test_kdj_calculation() {
        let mut kdj = KDJ::new(5, 2, 2);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(112.0, 107.0, 111.0),
            make_bar(114.0, 109.0, 113.0),
        ];

        kdj.calculate(&data);

        assert_eq!(kdj.values.len(), 7);

        // Should have K, D, J values
        if let IndicatorValue::Multiple(v) = kdj.values.last().unwrap() {
            assert_eq!(v.len(), 3);
            // In uptrend, K should be high
            assert!(v[0] > 50.0, "K should be > 50 in uptrend");
        }
    }
}
