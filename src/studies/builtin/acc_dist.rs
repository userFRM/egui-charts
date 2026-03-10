//! Accumulation/Distribution Line (A/D Line) Indicator
//!
//! The A/D Line measures the cumulative flow of money into and out of a security
//! based on the close's position within the bar's high-low range.
//!
//! # Formula
//! Money Flow Multiplier = ((Close - Low) - (High - Close)) / (High - Low)
//! Money Flow Volume = Money Flow Multiplier * Volume
//! A/D Line = Previous A/D + Money Flow Volume
//!
//! # Interpretation
//! - Rising A/D Line: Accumulation (buying pressure)
//! - Falling A/D Line: Distribution (selling pressure)
//! - Divergence with price: Potential reversal signal
//!
//! # Example
//! ```ignore
//! use egui_charts::AccumulationDistribution;
//!
//! let mut ad = AccumulationDistribution::new();
//! ad.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Accumulation/Distribution Line indicator
#[derive(Clone)]
pub struct AccumulationDistribution {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl AccumulationDistribution {
    /// Create a new A/D Line indicator
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for AccumulationDistribution {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AccumulationDistribution {
    fn name(&self) -> &str {
        "A/D"
    }

    fn desc(&self) -> &str {
        "Accumulation/Distribution Line - Measures buying/selling pressure"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let mut ad = 0.0;

        for bar in data {
            let high_low = bar.high - bar.low;

            if high_low > 0.0 {
                // Money Flow Multiplier: ((Close - Low) - (High - Close)) / (High - Low)
                // Simplifies to: (2 * Close - High - Low) / (High - Low)
                let mfm = (2.0 * bar.close - bar.high - bar.low) / high_low;

                // Money Flow Volume
                let mfv = mfm * bar.volume;

                // Cumulative A/D
                ad += mfv;
            }
            // If high == low (doji), no change to A/D

            self.values.push(IndicatorValue::Single(ad));
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
        false // A/D is a separate pane
    }

    fn line_cnt(&self) -> usize {
        1
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
        vec!["A/D Line".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_accumulation_bars() -> Vec<Bar> {
        // Bars with closes near highs (accumulation)
        let start = Utc::now();
        (0..30)
            .map(|i| {
                let base = 100.0 + i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: base,
                    high: base + 2.0,
                    low: base - 1.0,
                    close: base + 1.8, // Close near high
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn create_distribution_bars() -> Vec<Bar> {
        // Bars with closes near lows (distribution)
        let start = Utc::now();
        (0..30)
            .map(|i| {
                let base = 150.0 - i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: base,
                    high: base + 1.0,
                    low: base - 2.0,
                    close: base - 1.8, // Close near low
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_ad_accumulation() {
        let bars = create_accumulation_bars();
        let mut ad = AccumulationDistribution::new();
        ad.calculate(&bars);

        // A/D should be rising in accumulation
        if let (Some(IndicatorValue::Single(first)), Some(IndicatorValue::Single(last))) =
            (ad.values().first(), ad.values().last())
        {
            assert!(*last > *first, "A/D should rise during accumulation");
        }
    }

    #[test]
    fn test_ad_distribution() {
        let bars = create_distribution_bars();
        let mut ad = AccumulationDistribution::new();
        ad.calculate(&bars);

        // A/D should be falling in distribution
        if let (Some(IndicatorValue::Single(first)), Some(IndicatorValue::Single(last))) =
            (ad.values().first(), ad.values().last())
        {
            assert!(*last < *first, "A/D should fall during distribution");
        }
    }

    #[test]
    fn test_ad_is_not_overlay() {
        let ad = AccumulationDistribution::new();
        assert!(!ad.is_overlay());
    }

    #[test]
    fn test_ad_empty_data() {
        let mut ad = AccumulationDistribution::new();
        ad.calculate(&[]);
        assert!(ad.values().is_empty());
    }

    #[test]
    fn test_ad_doji_no_change() {
        // When high == low, A/D shouldn't change
        let bars = vec![
            Bar {
                time: Utc::now(),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc::now() + Duration::minutes(5),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            },
        ];
        let mut ad = AccumulationDistribution::new();
        ad.calculate(&bars);

        if let (Some(IndicatorValue::Single(first)), Some(IndicatorValue::Single(second))) =
            (ad.values().first(), ad.values().get(1))
        {
            assert_eq!(*first, *second, "A/D shouldn't change on doji bars");
        }
    }
}
