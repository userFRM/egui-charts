//! Money Flow Index (MFI) Indicator
//!
//! MFI is a volume-weighted RSI, measuring buying and selling pressure.
//! It oscillates between 0 and 100, similar to RSI.
//!
//! # Interpretation
//! - MFI > 80: Overbought (potential reversal down)
//! - MFI < 20: Oversold (potential reversal up)
//! - Divergence between MFI and price can signal trend changes
//! - The pane opens on a 0 to 100 scale with guides at 80 and 20, a faint dashed middle guide
//!   at 50 and the band between 80 and 20 filled
//!
//! # Calculation
//! Raw Money Flow = |Typical Price| * Volume, assigned to positive or negative flow by
//! the change in typical price; an unchanged typical price contributes to neither.
//! MFI = 100 * Positive Money Flow / (Positive Money Flow + Negative Money Flow).
//! A window with no flow gives 50, only positive flow gives 100 and only negative flow gives 0.
//! The first bar has no earlier typical price, so its flow enters both sums. The first value
//! is on bar `length - 1`.
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
#[derive(Clone, Debug)]
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
    /// * `period` - Lookback period (typically 14), clamped into 1 to 2000
    pub fn new(period: usize) -> Self {
        Self {
            period: period.clamp(1, 2000),
            values: IndicatorValues::new(),
            color: palette::PURPLE,
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
        if data.len() < self.period {
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
            .map(|(bar, tp)| tp.abs() * bar.volume)
            .collect();

        // Determine positive/negative money flow
        let mut positive_flows = vec![raw_money_flows[0]];
        let mut negative_flows = vec![raw_money_flows[0]];

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
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let pos_sum: f64 = positive_flows[start..=i].iter().sum();
                let neg_sum: f64 = negative_flows[start..=i].iter().sum();

                let mfi = super::rsi::from_averages(pos_sum, neg_sum);

                values.push(IndicatorValue::Single(mfi.clamp(0.0, 100.0)));
            }
        }

        values
    }
}

impl Indicator for MoneyFlowIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, 2000)
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

    /// The band between the outer band guides, whatever the middle guide's value.
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        Some([0, 1])
    }

    fn level_fill_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), 26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            range: Some((0.0, 100.0)),
            levels: vec![
                (80.0, palette::MUTED, "Overbought"),
                (20.0, palette::MUTED, "Oversold"),
                (50.0, palette::MUTED.gamma_multiply_u8(128), "Middle Band"),
            ],
            fill_between_levels: true,
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false // MFI is drawn in separate pane
    }

    fn draws_only_plots(&self) -> bool {
        true
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

    #[test]
    fn endpoints_and_scale_invariance() {
        // Period 2 counts the first raw flow on both sides. With unit volume,
        // [1, 2, 1, 3] gives 100 * 3/4, 100 * 2/3, then 100 * 3/4.
        for (closes, volume, expected) in [
            ([1.0, 2.0, 3.0, 4.0], 0.0, [50.0, 50.0, 50.0]),
            ([5.0; 4], 700.0, [50.0, 50.0, 50.0]),
            ([1.0, 2.0, 3.0, 4.0], 1.0, [75.0, 100.0, 100.0]),
            ([4.0, 3.0, 2.0, 1.0], 1.0, [400.0 / 11.0, 0.0, 0.0]),
            ([1.0, 2.0, 1.0, 3.0], 1.0, [75.0, 200.0 / 3.0, 75.0]),
            ([-4.0, -2.0, -3.0, -1.0], 1.0, [60.0, 40.0, 25.0]),
        ] {
            for scale in [1.0, 1e-17, 1e12] {
                let bars: Vec<_> = closes
                    .iter()
                    .map(|p| {
                        let p = p * scale;
                        Bar::new(chrono::DateTime::UNIX_EPOCH, p, p, p, p, volume)
                    })
                    .collect();
                let mut study = MoneyFlowIndex::new(2);
                study.calculate(&bars);
                assert_eq!(study.values().value(0, 0), None);
                for (bar, want) in expected.into_iter().enumerate() {
                    let got = study.values().value(0, bar + 1).unwrap();
                    assert!(
                        (got - want).abs() < 1e-9,
                        "{closes:?}, {volume}, {scale}: {got}"
                    );
                }
            }
        }
    }
}
