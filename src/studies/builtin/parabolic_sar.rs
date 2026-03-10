//! Parabolic SAR (Stop and Reverse) Indicator
//!
//! Developed by J. Welles Wilder Jr., the Parabolic SAR provides potential
//! entry and exit points by plotting a trailing stop-and-reverse point.
//!
//! # Formula
//! SARn+1 = SARn + AF * (EP - SARn)
//! Where:
//! - AF = Acceleration Factor (starts at initial_af, increases by af_step up to max_af)
//! - EP = Extreme Point (highest high in uptrend, lowest low in downtrend)
//!
//! # Interpretation
//! - SAR below price: Uptrend (SAR acts as support/trailing stop)
//! - SAR above price: Downtrend (SAR acts as resistance/trailing stop)
//! - When price crosses SAR: Trend reversal signal
//!
//! # Example
//! ```ignore
//! use egui_charts::ParabolicSAR;
//!
//! let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
//! psar.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Parabolic SAR indicator
#[derive(Clone)]
pub struct ParabolicSAR {
    initial_af: f64,
    af_step: f64,
    max_af: f64,
    values: Vec<IndicatorValue>,
    bullish_color: Color32,
    bearish_color: Color32,
    visible: bool,
}

impl ParabolicSAR {
    /// Create a new Parabolic SAR indicator
    ///
    /// # Arguments
    /// * `initial_af` - Starting acceleration factor (typically 0.02)
    /// * `af_step` - AF increment (typically 0.02)
    /// * `max_af` - Max AF (typically 0.2)
    pub fn new(initial_af: f64, af_step: f64, max_af: f64) -> Self {
        Self {
            initial_af: initial_af.max(0.001),
            af_step: af_step.max(0.001),
            max_af: max_af.max(initial_af),
            values: Vec::new(),
            bullish_color: DESIGN_TOKENS.semantic.extended.bullish,
            bearish_color: DESIGN_TOKENS.semantic.extended.bearish,
            visible: true,
        }
    }

    /// Create with default params (0.02, 0.02, 0.2)
    pub fn default_params() -> Self {
        Self::new(0.02, 0.02, 0.2)
    }

    /// Set colors for bullish and bearish SAR points
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }
}

impl Indicator for ParabolicSAR {
    fn name(&self) -> &str {
        "PSAR"
    }

    fn desc(&self) -> &str {
        "Parabolic SAR - Trend following stop-and-reverse"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let mut sar = vec![0.0; data.len()];
        let mut trend = vec![1i32; data.len()]; // 1 = uptrend, -1 = downtrend
        let mut ep = vec![0.0; data.len()]; // Extreme point
        let mut af = vec![self.initial_af; data.len()];

        // Initialize - determine initial trend from first two bars
        if data[1].close >= data[0].close {
            // Start in uptrend
            trend[0] = 1;
            trend[1] = 1;
            sar[0] = data[0].low;
            sar[1] = data[0].low;
            ep[0] = data[0].high;
            ep[1] = data[1].high.max(ep[0]);
        } else {
            // Start in downtrend
            trend[0] = -1;
            trend[1] = -1;
            sar[0] = data[0].high;
            sar[1] = data[0].high;
            ep[0] = data[0].low;
            ep[1] = data[1].low.min(ep[0]);
        }

        for i in 2..data.len() {
            let prev_trend = trend[i - 1];
            let prev_sar = sar[i - 1];
            let prev_ep = ep[i - 1];
            let prev_af = af[i - 1];

            // Calculate potential next SAR
            let mut next_sar = prev_sar + prev_af * (prev_ep - prev_sar);

            // Check for reversal
            let reversal = if prev_trend == 1 {
                // In uptrend, reversal if price falls below SAR
                // Also ensure SAR doesn't go above prior two lows
                next_sar = next_sar.min(data[i - 1].low).min(data[i - 2].low);
                data[i].low < next_sar
            } else {
                // In downtrend, reversal if price rises above SAR
                // Also ensure SAR doesn't go below prior two highs
                next_sar = next_sar.max(data[i - 1].high).max(data[i - 2].high);
                data[i].high > next_sar
            };

            if reversal {
                // Trend reversal
                trend[i] = -prev_trend;
                sar[i] = prev_ep;
                ep[i] = if trend[i] == 1 {
                    data[i].high
                } else {
                    data[i].low
                };
                af[i] = self.initial_af;
            } else {
                // Continue trend
                trend[i] = prev_trend;
                sar[i] = next_sar;

                // Update EP and AF
                if trend[i] == 1 {
                    if data[i].high > prev_ep {
                        ep[i] = data[i].high;
                        af[i] = (prev_af + self.af_step).min(self.max_af);
                    } else {
                        ep[i] = prev_ep;
                        af[i] = prev_af;
                    }
                } else if data[i].low < prev_ep {
                    ep[i] = data[i].low;
                    af[i] = (prev_af + self.af_step).min(self.max_af);
                } else {
                    ep[i] = prev_ep;
                    af[i] = prev_af;
                }
            }
        }

        // Build output: [sar, trend]
        for i in 0..data.len() {
            if i == 0 {
                self.values.push(IndicatorValue::None);
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![sar[i], trend[i] as f64]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // PSAR is drawn on main chart
    }

    fn line_cnt(&self) -> usize {
        1 // Single SAR line (dots, color changes with trend)
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
        vec![format!(
            "PSAR({}, {}, {})",
            self.initial_af, self.af_step, self.max_af
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_uptrending_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 100.0 + i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price - 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn create_downtrending_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 150.0 - i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price + 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_psar_uptrend_below_price() {
        let bars = create_uptrending_bars();
        let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        psar.calculate(&bars);

        // In uptrend, SAR should be below price
        for (i, value) in psar.values().iter().enumerate().skip(1) {
            if let IndicatorValue::Multiple(vals) = value {
                let sar = vals[0];
                let trend = vals[1];
                if trend > 0.0 {
                    assert!(
                        sar <= bars[i].low,
                        "In uptrend, SAR ({}) should be <= low ({})",
                        sar,
                        bars[i].low
                    );
                }
            }
        }
    }

    #[test]
    fn test_psar_downtrend_above_price() {
        let bars = create_downtrending_bars();
        let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        psar.calculate(&bars);

        // Check last few values for downtrend
        if let Some(IndicatorValue::Multiple(vals)) = psar.values().last() {
            let sar = vals[0];
            let trend = vals[1];
            if trend < 0.0 {
                assert!(
                    sar >= bars.last().unwrap().high,
                    "In downtrend, SAR should be >= high"
                );
            }
        }
    }

    #[test]
    fn test_psar_trend_values() {
        let bars = create_uptrending_bars();
        let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        psar.calculate(&bars);

        for value in psar.values().iter().skip(1) {
            if let IndicatorValue::Multiple(vals) = value {
                let trend = vals[1];
                assert!(trend == 1.0 || trend == -1.0, "Trend should be 1 or -1");
            }
        }
    }

    #[test]
    fn test_psar_is_overlay() {
        let psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        assert!(psar.is_overlay());
    }

    #[test]
    fn test_psar_empty_data() {
        let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        psar.calculate(&[]);
        assert!(psar.values().is_empty());
    }

    #[test]
    fn test_psar_single_bar() {
        let bars = vec![Bar {
            time: Utc::now(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 1000.0,
        }];
        let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
        psar.calculate(&bars);
        assert_eq!(psar.values().len(), 1);
        assert!(matches!(psar.values()[0], IndicatorValue::None));
    }
}
