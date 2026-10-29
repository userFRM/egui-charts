//! BBTrend
//!
//! John Bollinger's BBTrend compares a short and a long set of Bollinger Bands on the close to
//! measure the direction and strength of a trend: positive when the lower bands spread further
//! apart than the upper bands (an uptrend), negative in the opposite case.
//!
//! # Formula
//! ```text
//! lower, middle, upper = Bollinger Bands(close, length, multiplier), population deviation
//! BBTrend = (|lowerShort - lowerLong| - |upperShort - upperLong|) / middleShort * 100
//! ```
//! with conventional lengths 20 and 50 and multiplier 2. The first value is on the bar where the
//! longer band is complete.
//!
//! # Presentation
//! Columns: above zero, strong green while BBTrend rises and light green otherwise; at or
//! below zero, strong red while it falls and light red otherwise.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..80).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{BBTrend, Indicator};
//!
//! let mut trend = BBTrend::new(20, 50, 2.0);
//! trend.calculate(&bars);
//! assert!(trend.values().value(0, 49).is_some());
//! ```

use super::{BollingerBands, clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Trend strength from a short and a long Bollinger Band.
#[derive(Clone)]
pub struct BBTrend {
    short_length: usize,
    long_length: usize,
    multiplier: f64,
    values: IndicatorValues,
    /// Rising above zero, easing above zero, falling at or below zero, easing at or below zero.
    colors: [Color32; 4],
    visible: bool,
}

impl BBTrend {
    /// Create a BBTrend from the lengths of its two bands and their deviation multiplier.
    /// The multiplier is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(short_length: usize, long_length: usize, multiplier: f64) -> Self {
        let (up, down) = (palette::TEAL, palette::RED);
        Self {
            short_length: short_length.max(1),
            long_length: long_length.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            colors: [
                up.gamma_multiply_u8(191),
                up.gamma_multiply_u8(128),
                down.gamma_multiply_u8(191),
                down.gamma_multiply_u8(128),
            ],
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let bands = |length| {
            let mut bands = BollingerBands::new(length, self.multiplier);
            bands.calculate(data);
            bands
        };
        let (short, long) = (bands(self.short_length), bands(self.long_length));
        let (short, long) = (short.values(), long.values());
        (0..data.len())
            .map(|bar| {
                let line =
                    |values: &IndicatorValues, line| values.value(line, bar).unwrap_or(f64::NAN);
                IndicatorValue::Single(
                    ((line(short, 2) - line(long, 2)).abs()
                        - (line(short, 0) - line(long, 0)).abs())
                        / line(short, 1)
                        * 100.0,
                )
            })
            .collect()
    }
}

impl Default for BBTrend {
    fn default() -> Self {
        Self::new(20, 50, 2.0)
    }
}

impl Indicator for BBTrend {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        short_length: length("Short BB Length", 1, usize::MAX),
        long_length: length("Long BB Length", 1, usize::MAX),
        multiplier: number("StdDev", 0.0, f64::MAX)
    ; {});

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        let value = self.values.value(0, bar)?;
        // Bars without a value, skipped as non-finite, do not count as the previous one.
        let previous = (0..bar).rev().find_map(|bar| self.values.value(0, bar));
        Some(if value == 0.0 {
            self.colors[1]
        } else if value > 0.0 {
            self.colors[usize::from(!previous.is_some_and(|p| value >= p))]
        } else {
            self.colors[2 + usize::from(!previous.is_some_and(|p| value <= p))]
        })
    }

    fn name(&self) -> &str {
        "BBTrend"
    }

    fn desc(&self) -> &str {
        "BBTrend - Trend strength from a short and a long Bollinger Band"
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
            usable_behind(self.short_length.max(self.long_length) - 1, |b| {
                b.close.is_finite()
            }),
            |data| over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data)),
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
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn line_count(&self) -> usize {
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
        vec![format!(
            "BBTrend({}, {}, {})",
            self.short_length, self.long_length, self.multiplier
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Above zero the column is strong while BBTrend rises and light otherwise; at or below zero
    /// it is strong while BBTrend falls and light otherwise, and light without a previous value.
    #[test]
    fn columns_are_coloured_by_sign_and_direction() {
        let study = BBTrend {
            values: [1.0, 2.0, 1.5, -1.0, -2.0, -1.5, 0.0]
                .into_iter()
                .map(IndicatorValue::Single)
                .collect(),
            ..BBTrend::default()
        };
        let colors = study.colors();
        let slots: Vec<usize> = (0..7)
            .map(|bar| {
                let color = study.plot_color(0, bar).unwrap();
                colors.iter().position(|&c| c == color).unwrap()
            })
            .collect();
        assert_eq!(slots, [1, 0, 1, 2, 2, 3, 1]);
    }
}
