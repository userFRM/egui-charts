use crate::model::Bar;
/// Directional Movement Index (DMI / DI+ / DI-)
/// Shows the positive and negative directional indicators
/// Part of the ADX family but shows the components
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct DirectionalMovement {
    /// Period for smoothing (typically 14)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [+DI, -DI]
    colors: Vec<Color32>,
    visible: bool,
}

impl DirectionalMovement {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - +DI
                DESIGN_TOKENS.semantic.extended.error,   // Red - -DI
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for DirectionalMovement {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for DirectionalMovement {
    fn name(&self) -> &str {
        "DMI"
    }

    fn desc(&self) -> &str {
        "Directional Movement Index - +DI and -DI lines"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate +DM, -DM, and TR
        let mut plus_dm = Vec::with_capacity(data.len());
        let mut minus_dm = Vec::with_capacity(data.len());
        let mut tr = Vec::with_capacity(data.len());

        plus_dm.push(0.0);
        minus_dm.push(0.0);
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            // True Range
            let high_low = current.high - current.low;
            let high_close = (current.high - prev.close).abs();
            let low_close = (current.low - prev.close).abs();
            tr.push(high_low.max(high_close).max(low_close));

            // Directional Movement
            let up_move = current.high - prev.high;
            let down_move = prev.low - current.low;

            let pdm = if up_move > down_move && up_move > 0.0 {
                up_move
            } else {
                0.0
            };
            let mdm = if down_move > up_move && down_move > 0.0 {
                down_move
            } else {
                0.0
            };

            plus_dm.push(pdm);
            minus_dm.push(mdm);
        }

        // Calculate smoothed values using Wilder's smoothing (like RMA/SMMA)
        let mut smoothed_plus_dm = Vec::with_capacity(data.len());
        let mut smoothed_minus_dm = Vec::with_capacity(data.len());
        let mut smoothed_tr = Vec::with_capacity(data.len());

        // First sum
        let mut sum_plus_dm: f64 = plus_dm[1..=self.period].iter().sum();
        let mut sum_minus_dm: f64 = minus_dm[1..=self.period].iter().sum();
        let mut sum_tr: f64 = tr[1..=self.period].iter().sum();

        for i in 0..data.len() {
            if i < self.period {
                smoothed_plus_dm.push(f64::NAN);
                smoothed_minus_dm.push(f64::NAN);
                smoothed_tr.push(f64::NAN);
            } else if i == self.period {
                smoothed_plus_dm.push(sum_plus_dm);
                smoothed_minus_dm.push(sum_minus_dm);
                smoothed_tr.push(sum_tr);
            } else {
                // Wilder smoothing: prev - (prev/period) + current
                sum_plus_dm = sum_plus_dm - sum_plus_dm / self.period as f64 + plus_dm[i];
                sum_minus_dm = sum_minus_dm - sum_minus_dm / self.period as f64 + minus_dm[i];
                sum_tr = sum_tr - sum_tr / self.period as f64 + tr[i];

                smoothed_plus_dm.push(sum_plus_dm);
                smoothed_minus_dm.push(sum_minus_dm);
                smoothed_tr.push(sum_tr);
            }
        }

        // Calculate +DI and -DI
        for i in 0..data.len() {
            if smoothed_tr[i].is_nan() || smoothed_tr[i] == 0.0 {
                self.values.push(IndicatorValue::None);
            } else {
                let plus_di = 100.0 * smoothed_plus_dm[i] / smoothed_tr[i];
                let minus_di = 100.0 * smoothed_minus_dm[i] / smoothed_tr[i];
                self.values
                    .push(IndicatorValue::Multiple(vec![plus_di, minus_di]));
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
            format!("+DI({})", self.period),
            format!("-DI({})", self.period),
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
    fn test_dmi_calculation() {
        let mut dmi = DirectionalMovement::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(112.0, 107.0, 111.0),
            make_bar(114.0, 109.0, 113.0),
        ];

        dmi.calculate(&data);

        assert_eq!(dmi.values.len(), 7);

        // Should have +DI and -DI values
        if let IndicatorValue::Multiple(v) = dmi.values.last().unwrap() {
            assert_eq!(v.len(), 2);
            // In uptrend, +DI should be higher
            assert!(v[0] > v[1], "+DI should be > -DI in uptrend");
        }
    }
}
