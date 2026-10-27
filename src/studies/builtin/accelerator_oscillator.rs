//! Accelerator Oscillator (AC)
//!
//! Developed by Bill Williams, the Accelerator Oscillator measures the acceleration of
//! momentum: how far the Awesome Oscillator stands from its own recent average.
//!
//! # Formula
//! ```text
//! median = (high + low) / 2
//! AO = SMA(median, 5) - SMA(median, 34)
//! AC = AO - SMA(AO, 5)
//! ```
//! The first value is on bar 37, once the slow average and the AO average are complete.
//!
//! # Presentation
//! Columns, green when AC rises above its previous value and red when it falls or holds.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AcceleratorOscillator, Indicator};
//!
//! let mut ac = AcceleratorOscillator::new();
//! ac.calculate(&bars);
//! assert!(ac.values().value(0, 37).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::rolling_mean;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Accelerator Oscillator, with Bill Williams' fixed lengths 5, 34 and 5.
#[derive(Clone)]
pub struct AcceleratorOscillator {
    values: IndicatorValues,
    rising_color: Color32,
    falling_color: Color32,
    visible: bool,
}

impl AcceleratorOscillator {
    /// Create an Accelerator Oscillator.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            rising_color: palette::GREEN,
            falling_color: palette::RED,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(data: &[Bar]) -> IndicatorValues {
        let median: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();
        let ao: Vec<f64> = rolling_mean(&median, 5)
            .into_iter()
            .zip(rolling_mean(&median, 34))
            .map(|(fast, slow)| fast - slow)
            .collect();
        ao.iter()
            .zip(rolling_mean(&ao, 5))
            .map(|(ao, average)| IndicatorValue::Single(ao - average))
            .collect()
    }
}

impl Default for AcceleratorOscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AcceleratorOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HL2,
        ))
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.rising_color,
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
        Some(if previous.is_some_and(|previous| value <= previous) {
            self.falling_color
        } else {
            self.rising_color
        })
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "Accelerator Oscillator"
    }

    fn desc(&self) -> &str {
        "Accelerator Oscillator - Awesome Oscillator minus its 5-bar average"
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
            usable_behind(37, |b| b.high.is_finite() && b.low.is_finite()),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite(),
                    Self::compute,
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
        vec![self.rising_color, self.falling_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.rising_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.falling_color = color;
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
        vec!["AC".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A column is red when AC falls or holds, green when it rises or has no previous value;
    /// a bar without a value is passed over.
    #[test]
    fn an_unchanged_value_is_coloured_as_falling() {
        let mut study = AcceleratorOscillator::new();
        study.values = [1.0, 1.0, 2.0, 1.0, f64::NAN, 0.5]
            .into_iter()
            .map(IndicatorValue::Single)
            .collect();
        let (up, down) = (study.rising_color, study.falling_color);
        let colors: Vec<_> = (0..6).map(|bar| study.plot_color(0, bar)).collect();
        assert_eq!(
            colors,
            [Some(up), Some(down), Some(up), Some(down), None, Some(down)]
        );
    }
}
