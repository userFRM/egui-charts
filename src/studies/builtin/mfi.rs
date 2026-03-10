//! Money Flow Index (MFI) Indicator
//!
//! MFI is a volume-weighted RSI, measuring buying and selling pressure.
//! It oscillates between 0 and 100, similar to RSI.
//!
//! # Interpretation
//! - MFI > 80: Overbought (potential reversal down)
//! - MFI < 20: Oversold (potential reversal up)
//! - Divergence between MFI and price can signal trend changes
//!
//! # Calculation
//! Raw Money Flow = Typical Price * Volume
//! Money Flow Ratio = Positive Money Flow / Negative Money Flow
//! MFI = 100 - (100 / (1 + Money Flow Ratio))
//!
//! # Example
//! ```ignore
//! use egui_charts::MoneyFlowIndex;
//!
//! let mut mfi = MoneyFlowIndex::new(14);
//! mfi.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Money Flow Index indicator
#[derive(Clone)]
pub struct MoneyFlowIndex {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl MoneyFlowIndex {
    /// Create a new MFI indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate typical price
    #[inline]
    fn typical_price(bar: &Bar) -> f64 {
        (bar.high + bar.low + bar.close) / 3.0
    }
}

impl Indicator for MoneyFlowIndex {
    fn name(&self) -> &str {
        "MFI"
    }

    fn desc(&self) -> &str {
        "Money Flow Index - Volume-weighted RSI (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate typical prices and raw money flow
        let typical_prices: Vec<f64> = data.iter().map(Self::typical_price).collect();
        let raw_money_flows: Vec<f64> = data
            .iter()
            .zip(typical_prices.iter())
            .map(|(bar, tp)| tp * bar.volume)
            .collect();

        // Determine positive/negative money flow
        let mut positive_flows = vec![0.0];
        let mut negative_flows = vec![0.0];

        for i in 1..data.len() {
            if typical_prices[i] > typical_prices[i - 1] {
                positive_flows.push(raw_money_flows[i]);
                negative_flows.push(0.0);
            } else if typical_prices[i] < typical_prices[i - 1] {
                positive_flows.push(0.0);
                negative_flows.push(raw_money_flows[i]);
            } else {
                positive_flows.push(0.0);
                negative_flows.push(0.0);
            }
        }

        // Calculate MFI for each bar
        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let pos_sum: f64 = positive_flows[start..=i].iter().sum();
                let neg_sum: f64 = negative_flows[start..=i].iter().sum();

                let mfi = if neg_sum.abs() < 1e-10 {
                    100.0 // All positive flow
                } else if pos_sum.abs() < 1e-10 {
                    0.0 // All negative flow
                } else {
                    let ratio = pos_sum / neg_sum;
                    100.0 - (100.0 / (1.0 + ratio))
                };

                self.values
                    .push(IndicatorValue::Single(mfi.clamp(0.0, 100.0)));
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
        false // MFI is drawn in separate pane
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
        vec![format!("MFI({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..30)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.2).sin() * 10.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price + 0.5,
                    volume: 1000.0 + (i as f64 * 100.0),
                }
            })
            .collect()
    }

    #[test]
    fn test_mfi_range() {
        let bars = create_test_bars();
        let mut mfi = MoneyFlowIndex::new(14);
        mfi.calculate(&bars);

        for value in mfi.values() {
            if let IndicatorValue::Single(v) = value {
                assert!(*v >= 0.0 && *v <= 100.0, "MFI should be 0-100, got {}", v);
            }
        }
    }

    #[test]
    fn test_mfi_calculation() {
        let bars = create_test_bars();
        let mut mfi = MoneyFlowIndex::new(14);
        mfi.calculate(&bars);

        assert_eq!(mfi.values().len(), bars.len());

        // First period values should be None
        for i in 0..14 {
            assert!(matches!(mfi.values()[i], IndicatorValue::None));
        }

        // Remaining values should be valid
        for value in mfi.values().iter().skip(14) {
            assert!(matches!(value, IndicatorValue::Single(_)));
        }
    }

    #[test]
    fn test_mfi_empty_data() {
        let mut mfi = MoneyFlowIndex::new(14);
        mfi.calculate(&[]);
        assert!(mfi.values().is_empty());
    }
}
