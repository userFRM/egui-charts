//! Detrended Price Oscillator (DPO)
//!
//! The DPO removes the trend from the close by comparing it with a moving average shifted by
//! half its length plus one bar, so that cycles shorter than the length stand out.
//!
//! # Formula
//! ```text
//! shift = length / 2 + 1   (integer division)
//! non-centred: DPO = close - SMA(close, length)[shift]
//! centred:     DPO = close[shift] - SMA(close, length), drawn `shift` bars back
//! ```
//! with conventional length 21, non-centred. The non-centred form is aligned with the current
//! bar. The centred form compares each close with the average centred on it; its value is
//! stored at the bar that completes the average and [`plot_offset`](Indicator::plot_offset)
//! draws it `shift` bars back, so its last `shift` bars are blank. A bar skipped for a non-finite
//! close still counts toward that shift, so each value is drawn under the close it uses.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{DetrendedPriceOscillator, Indicator};
//!
//! let mut dpo = DetrendedPriceOscillator::new(21);
//! dpo.calculate(&bars);
//! assert!(dpo.values().value(0, 31).is_some());
//!
//! let mut centred = DetrendedPriceOscillator::new(21).with_centered(true);
//! centred.calculate(&bars);
//! assert_eq!(centred.plot_offset(0), -11);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::rolling_mean;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Detrended Price Oscillator, aligned with the current bar or centred on the average.
#[derive(Clone)]
pub struct DetrendedPriceOscillator {
    period: usize,
    centered: bool,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl DetrendedPriceOscillator {
    /// Create a non-centred DPO over `period` bars (conventionally 21).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            centered: false,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple,
            visible: true,
        }
    }

    /// Compare each close with the average centred on it, drawn back by the shift.
    pub fn with_centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn shift(&self) -> usize {
        self.period / 2 + 1
    }

    /// The non-centred values over `data`, every bar of which has a finite close, or the average
    /// itself for the centred form.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
        let average = rolling_mean(&closes, self.period);
        (0..data.len())
            .map(|i| {
                IndicatorValue::Single(match i.checked_sub(self.shift()) {
                    _ if self.centered => average[i],
                    Some(back) => closes[i] - average[back],
                    None => f64::NAN,
                })
            })
            .collect()
    }
}

/// Construct with the conventional default parameters.
impl Default for DetrendedPriceOscillator {
    fn default() -> Self {
        Self::new(21)
    }
}

impl Indicator for DetrendedPriceOscillator {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX),
        centered: toggle("Centered")
    ; {});

    fn plot_offset(&self, _line: usize) -> isize {
        if self.centered {
            -(self.shift().min(isize::MAX as usize) as isize)
        } else {
            0
        }
    }

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "DPO"
    }

    fn desc(&self) -> &str {
        "Detrended Price Oscillator - Removes trend to identify cycles"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
        if self.centered {
            let average = std::mem::take(&mut self.values);
            // The plot offset counts every chart bar, so the close drawn under a value is the one
            // `shift` bars back on the chart, skipped bars included.
            self.values = (0..data.len())
                .map(|i| {
                    IndicatorValue::Single(
                        match (i.checked_sub(self.shift()), average.value(0, i)) {
                            (Some(back), Some(mean)) => data[back].close - mean,
                            _ => f64::NAN,
                        },
                    )
                })
                .collect();
        }
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

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(0.0, Color32::from_gray(128), "Zero")],
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false // Separate pane
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
        vec![format!("DPO({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// With length 1 the shift is 1 and the average is the close, so a centred value is drawn
    /// under the close one bar before the one completing it. A skipped bar draws nothing, and
    /// neither does the bar whose average it would complete.
    #[test]
    fn centred_values_stay_under_their_close_past_a_skipped_bar() {
        let bars: Vec<Bar> = [0.0, 1.0, 4.0, f64::NAN, 9.0, 16.0]
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect();
        let mut dpo = DetrendedPriceOscillator::new(1).with_centered(true);
        dpo.calculate(&bars);
        let drawn: Vec<_> = (0..5).map(|bar| dpo.plot_value(0, bar)).collect();
        assert_eq!(drawn, [Some(-1.0), Some(-3.0), None, None, Some(-7.0)]);
    }
}
