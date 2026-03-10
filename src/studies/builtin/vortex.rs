use crate::model::Bar;
/// Vortex Indicator (VI)
/// Identifies trend direction and strength
/// Uses True Range and directional movement concepts
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VortexIndicator {
    /// Lookback period (typically 14)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [+VI (bullish), -VI (bearish)]
    colors: Vec<Color32>,
    visible: bool,
}

impl VortexIndicator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - +VI
                DESIGN_TOKENS.semantic.extended.error,   // Red - -VI
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Indicator for VortexIndicator {
    fn name(&self) -> &str {
        "Vortex"
    }

    fn desc(&self) -> &str {
        "Vortex Indicator - Identifies trend direction"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate True Range, +VM, -VM for each bar
        let mut tr_values = Vec::with_capacity(data.len());
        let mut vm_plus = Vec::with_capacity(data.len());
        let mut vm_minus = Vec::with_capacity(data.len());

        tr_values.push(0.0);
        vm_plus.push(0.0);
        vm_minus.push(0.0);

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            // True Range = max(High - Low, |High - Prev Close|, |Low - Prev Close|)
            let tr = (current.high - current.low)
                .max((current.high - prev.close).abs())
                .max((current.low - prev.close).abs());

            // +VM = |Current High - Previous Low|
            let vm_p = (current.high - prev.low).abs();

            // -VM = |Current Low - Previous High|
            let vm_m = (current.low - prev.high).abs();

            tr_values.push(tr);
            vm_plus.push(vm_p);
            vm_minus.push(vm_m);
        }

        // Calculate VI+ and VI-
        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                // Sum over period
                let tr_sum: f64 = tr_values[i + 1 - self.period..=i].iter().sum();
                let vm_plus_sum: f64 = vm_plus[i + 1 - self.period..=i].iter().sum();
                let vm_minus_sum: f64 = vm_minus[i + 1 - self.period..=i].iter().sum();

                if tr_sum == 0.0 {
                    self.values.push(IndicatorValue::None);
                } else {
                    let vi_plus = vm_plus_sum / tr_sum;
                    let vi_minus = vm_minus_sum / tr_sum;
                    self.values
                        .push(IndicatorValue::Multiple(vec![vi_plus, vi_minus]));
                }
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
        if colors.len() >= 2 {
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
            format!("+VI({})", self.period),
            format!("-VI({})", self.period),
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
    fn test_vortex_calculation() {
        let mut vi = VortexIndicator::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(112.0, 107.0, 111.0),
            make_bar(114.0, 109.0, 113.0),
        ];

        vi.calculate(&data);

        assert_eq!(vi.values.len(), 7);

        // Should have valid values after period
        if let IndicatorValue::Multiple(v) = &vi.values[6] {
            assert_eq!(v.len(), 2);
            // In uptrend, +VI should typically be > -VI
        }
    }

    #[test]
    fn test_vortex_uptrend() {
        let mut vi = VortexIndicator::new(3);

        // Clear uptrend
        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(114.0, 109.0, 113.0),
            make_bar(118.0, 113.0, 117.0),
        ];

        vi.calculate(&data);

        if let IndicatorValue::Multiple(v) = vi.values.last().unwrap() {
            // +VI should be greater than -VI in uptrend
            assert!(v[0] > v[1], "+VI should be > -VI in uptrend");
        }
    }
}
