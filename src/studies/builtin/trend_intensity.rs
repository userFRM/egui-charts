use crate::model::Bar;
/// Trend Intensity Index (TII)
/// Measures the strength of a trend by comparing price to a moving avg
/// Values above 50 indicate uptrend, below 50 indicate downtrend
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TrendIntensityIndex {
    /// Period for SMA calculation
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl TrendIntensityIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Indigo/Blue
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TrendIntensityIndex {
    fn default() -> Self {
        Self::new(30)
    }
}

impl Indicator for TrendIntensityIndex {
    fn name(&self) -> &str {
        "TII"
    }

    fn desc(&self) -> &str {
        "Trend Intensity Index - Measures trend strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // Calculate SMA
                let sma: f64 = window.iter().map(|b| b.close).sum::<f64>() / self.period as f64;

                // Count bars above and below SMA
                let mut up_cnt = 0;
                let mut down_cnt = 0;

                for bar in window {
                    if bar.close > sma {
                        up_cnt += 1;
                    } else if bar.close < sma {
                        down_cnt += 1;
                    }
                }

                // Calculate TII
                let total = up_cnt + down_cnt;
                let tii = if total > 0 {
                    (up_cnt as f64 / total as f64) * 100.0
                } else {
                    50.0
                };

                self.values.push(IndicatorValue::Single(tii));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!("TII({})", self.period)]
    }
}

/// Qstick (Quantitative Candlestick)
/// Moving avg of the open-close difference
/// Positive = bullish, Negative = bearish
#[derive(Clone)]
pub struct Qstick {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl Qstick {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for Qstick {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Indicator for Qstick {
    fn name(&self) -> &str {
        "Qstick"
    }

    fn desc(&self) -> &str {
        "Qstick - Moving avg of candlestick body"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // Calculate avg of (close - open)
                let avg: f64 =
                    window.iter().map(|b| b.close - b.open).sum::<f64>() / self.period as f64;

                self.values.push(IndicatorValue::Single(avg));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!("Qstick({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, close: f64) -> Bar {
        let high = open.max(close) + 1.0;
        let low = open.min(close) - 1.0;
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_tii() {
        let mut tii = TrendIntensityIndex::new(10);

        // Uptrending data
        let data: Vec<Bar> = (0..20)
            .map(|i| make_bar(100.0 + i as f64, 101.0 + i as f64))
            .collect();

        tii.calculate(&data);

        assert_eq!(tii.values.len(), 20);

        // In uptrend, TII should be >= 50 (linear uptrend gives exactly 50)
        if let IndicatorValue::Single(v) = tii.values.last().unwrap() {
            assert!(*v >= 50.0);
        }
    }

    #[test]
    fn test_qstick() {
        let mut qs = Qstick::new(5);

        // Bullish candles (close > open)
        let data: Vec<Bar> = (0..10)
            .map(|i| make_bar(100.0 + i as f64, 102.0 + i as f64))
            .collect();

        qs.calculate(&data);

        assert_eq!(qs.values.len(), 10);

        // Should be positive for bullish candles
        if let IndicatorValue::Single(v) = qs.values.last().unwrap() {
            assert!(*v > 0.0);
        }
    }
}
