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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{MoneyFlowIndex, Indicator};
//!
//! let mut mfi = MoneyFlowIndex::new(14);
//! mfi.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Money Flow Index indicator
#[derive(Clone)]
pub struct MoneyFlowIndex {
    period: usize,
    values: IndicatorValues,
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
            values: IndicatorValues::new(),
            color: palette::UP,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for MoneyFlowIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl MoneyFlowIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate typical prices and raw money flow
        let typical_prices: Vec<f64> = data.iter().map(Bar::typical_price).collect();
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
                values.push(IndicatorValue::None);
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

                values.push(IndicatorValue::Single(mfi.clamp(0.0, 100.0)));
            }
        }

        values
    }
}

impl Indicator for MoneyFlowIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "MFI"
    }

    fn desc(&self) -> &str {
        "Money Flow Index - Volume-weighted RSI (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period, |b| {
                b.high.is_finite()
                    && b.low.is_finite()
                    && b.close.is_finite()
                    && b.volume.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| {
                        b.high.is_finite()
                            && b.low.is_finite()
                            && b.close.is_finite()
                            && b.volume.is_finite()
                    },
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
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
