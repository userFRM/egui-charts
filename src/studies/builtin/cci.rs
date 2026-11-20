//! Commodity Channel Index (CCI) Indicator
//!
//! CCI measures the current price level relative to an avg price level over
//! a given period. It oscillates above and below zero, helping identify cyclical
//! trends in a commodity or stock.
//!
//! # Interpretation
//! - Readings above +100: Overbought, potential reversal
//! - Readings below -100: Oversold, potential reversal
//! - Zero crossings can signal trend changes
//! - The pane opens with guides at 100, 0 and -100 and the band between 100 and -100 filled
//!   in its own `#2196F3` at a tenth of full opacity, independent of the line colour
//!
//! The smoothing inputs add a CCI-based moving average, a 14-value SMA by default, with
//! optional Bollinger Bands. [`CCI::new`] leaves the average off and emits single values.
//!
//! # Calculation
//! Typical Price (TP) = (High + Low + Close) / 3
//! SMA = Simple Moving Avg of TP
//! Mean Deviation = Avg of |TP - SMA|
//! CCI = (TP - SMA) / (0.015 * Mean Deviation)
//!
//! A window whose typical prices are all equal has no mean deviation, so its ratio divides zero
//! by zero and the bar has no value.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{CCI, Indicator};
//!
//! let mut cci = CCI::new(20);
//! cci.calculate(&bars);
//! ```

use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Commodity Channel Index indicator
#[derive(Clone, Debug)]
pub struct CCI {
    period: usize,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    values: IndicatorValues,
    stepper: Stepper<Rolling>,
    color: Color32,
    smoothing_colors: [Color32; 3],
    visible: bool,
}

/// The last `period` typical prices and the average of the index.
#[derive(Clone, Debug, Default)]
struct Rolling {
    window: VecDeque<f64>,
    smoothed: Smoothed,
}

impl CCI {
    /// Create a CCI without a CCI-based moving average.
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::BLUE,
            smoothing_colors: SMOOTHING_COLORS,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// A 20-bar CCI and its 14-value SMA, the CCI-based MA.
impl Default for CCI {
    fn default() -> Self {
        Self {
            smoothing: SmoothingType::Sma,
            ..Self::new(20)
        }
    }
}

impl Indicator for CCI {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HLC3,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.001, 50.0)
            if smoothing in SmoothingType::WITH_BANDS
    ; {});

    fn name(&self) -> &str {
        "CCI"
    }

    fn desc(&self) -> &str {
        "Commodity Channel Index - Cyclical trend indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let tp = bar.typical_price();
                state.window.push_back(tp);
                if state.window.len() > period {
                    state.window.pop_front();
                }
                if state.window.len() < period {
                    return IndicatorValue::None;
                }
                let window = state.window.make_contiguous();
                let n = period as f64;
                let sma = window.iter().sum::<f64>() / n;
                // Correct the sum's rounding so equal typical prices have no deviation.
                let sma = sma + window.iter().map(|p| p - sma).sum::<f64>() / n;
                let mean_dev = window.iter().map(|p| (p - sma).abs()).sum::<f64>() / n;
                if mean_dev == 0.0 {
                    return IndicatorValue::None;
                }
                let cci = (tp - sma) / (0.015 * mean_dev);
                if !cci.is_finite() {
                    return IndicatorValue::None;
                }
                state
                    .smoothed
                    .next(kind, length, deviation, cci, bar.volume)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        let [average, upper, lower] = self.smoothing_colors;
        vec![self.color, average, upper, lower]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            for (slot, color) in self.smoothing_colors.iter_mut().zip(&colors[1..]) {
                *slot = *color;
            }
        }
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    /// The band between the outer band guides, whatever the middle guide's value.
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        Some([0, 1])
    }

    fn level_fill_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(33, 150, 243, 26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![
                (100.0, palette::MUTED, "Upper Band"),
                (-100.0, palette::MUTED, "Lower Band"),
                (0.0, palette::MUTED.gamma_multiply_u8(128), "Middle Band"),
            ],
            fill_between_levels: true,
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false // CCI is drawn in separate pane
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
        let mut names = vec![format!("CCI({})", self.period)];
        names.extend(self.smoothing.line_names("CCI-based MA"));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// CCI(3) of 1, 2, 4, 3, 7, 6 is 100, 0, 100, 200/7 after warm-up.
    /// Its two-value SMA is 50, 50, 450/7; twice the population deviation gives
    /// bands at 150 and -50, then 950/7 and -50/7 on the last bar.
    #[test]
    fn smoothing_averages_the_index_and_the_band_keeps_its_colour() {
        use crate::studies::InputValue::{Choice, Length, Number};
        let bars: Vec<_> = [1.0, 2.0, 4.0, 3.0, 7.0, 6.0]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                Bar::new(
                    chrono::DateTime::from_timestamp(i as i64, 0).unwrap(),
                    p,
                    p,
                    p,
                    p,
                    1.0,
                )
            })
            .collect();
        for kind in [1, 2] {
            let mut study = CCI::new(3);
            study.set_input("smoothing", Choice(kind)).unwrap();
            study.set_input("smoothing_length", Length(2)).unwrap();
            study.calculate(&bars[..5]);
            study.update(&bars, 5);
            let mut fresh = study.clone();
            fresh.calculate(&bars);
            assert_eq!(
                format!("{:?}", study.values()),
                format!("{:?}", fresh.values())
            );
            assert_eq!(study.values().value(1, 2), None);
            let rows = [
                (2, vec![100.0]),
                (3, vec![0.0, 50.0, 150.0, -50.0]),
                (4, vec![100.0, 50.0, 150.0, -50.0]),
                (5, vec![200.0 / 7.0, 450.0 / 7.0, 950.0 / 7.0, -50.0 / 7.0]),
            ];
            for (bar, expected) in rows {
                for (line, want) in expected.into_iter().take(study.line_count()).enumerate() {
                    let got = study.values().value(line, bar).unwrap();
                    assert!(
                        (got - want).abs() < 1e-9,
                        "kind {kind}, bar {bar}, line {line}: {got}"
                    );
                }
            }
        }
        let mut study = CCI::default();
        for (id, want) in [
            ("smoothing", Choice(1)),
            ("smoothing_length", Length(14)),
            ("smoothing_deviation", Number(2.0)),
        ] {
            assert_eq!(
                study
                    .inputs()
                    .iter()
                    .find(|input| input.id == id)
                    .unwrap()
                    .value,
                want
            );
        }
        assert_eq!(study.line_names(), ["CCI(20)", "CCI-based MA"]);
        assert_eq!(study.colors()[1], palette::YELLOW);
        assert_eq!(CCI::new(20).line_names(), ["CCI(20)"]);
        assert!(study.set_input("smoothing_deviation", Number(0.0)).is_err());
        assert_eq!(
            CCI::default().with_color(Color32::RED).level_fill_color(),
            Color32::from_rgba_unmultiplied(33, 150, 243, 26)
        );
    }

    #[test]
    fn test_cci_flat_prices() {
        // A window without deviation divides zero by zero and has no value.
        let start = Utc::now();
        let mut bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
                colors: None,
                payload: None,
            })
            .collect();

        for price in [100.0, 0.1, 1e-17] {
            for bar in &mut bars {
                bar.open = price;
                bar.high = price;
                bar.low = price;
                bar.close = price;
            }
            let mut cci = CCI::new(20);
            cci.calculate(&bars);
            for value in cci.values().to_rows() {
                assert!(
                    matches!(value, IndicatorValue::None),
                    "a window without deviation has no value, got {value:?} at {price}"
                );
            }
        }
    }
}
