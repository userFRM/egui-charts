//! Woodies CCI, after Ken Wood's CCI trading method.
//!
//! Two Commodity Channel Index lines of the close, a fast "turbo" CCI and the trading CCI, with the
//! trading CCI also drawn as columns coloured by how long it has held one side of zero.
//!
//! # Formula
//!
//! ```text
//! CCI(n) = (close - SMA(close, n)) / (0.015 * mean absolute deviation of close over n)
//! ```
//!
//! A window with no deviation gives 0. Lines, in order: the CCI 14 columns, the CCI Turbo (6) and
//! the CCI 14 line. Levels at 100, 0 and -100.
//!
//! # Column stages
//!
//! A column's stage follows its run: the bar itself and the unbroken CCI 14 values just before it
//! on the same side of zero. Bars without a value are not part of the series, and a value of
//! exactly zero is on neither side, so it has no run and ends the run before it.
//!
//! | Run | Stage | Colour |
//! |-----|-------|--------|
//! | 6 or more | established | columns colour above zero, established down colour below |
//! | 2 to 5 | transition | transition colour |
//! | 1, or the value is zero | neutral | neutral colour |
//!
//! The columns colour is the first plot colour of the settings; the other three are study inputs.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, WoodiesCCI};
//!
//! let mut woodies = WoodiesCCI::default();
//! woodies.calculate(&bars);
//! assert_eq!(woodies.values().value(0, 30), woodies.values().value(2, 30));
//! assert!(woodies.values().value(1, 5).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Woodies CCI: CCI 14 columns, CCI Turbo and CCI 14 lines in a separate pane.
#[derive(Clone)]
pub struct WoodiesCCI {
    turbo_length: usize,
    cci_length: usize,
    values: IndicatorValues,
    /// Colors: [columns, CCI Turbo, CCI 14]; the columns colour is the established run above zero.
    colors: Vec<Color32>,
    /// Column colour of an established run below zero.
    down_color: Color32,
    /// Column colour of a run of two to five values.
    transition_color: Color32,
    transition_negative: Color32,
    /// Column colour of a first value on a side of zero, or of zero.
    neutral_color: Color32,
    neutral_negative: Color32,
    visible: bool,
}

impl WoodiesCCI {
    /// Create the study with the turbo and trading CCI lengths (6 and 14 by convention).
    pub fn new(turbo_length: usize, cci_length: usize) -> Self {
        Self {
            turbo_length: turbo_length.max(1),
            cci_length: cci_length.max(1),
            values: IndicatorValues::new(),
            colors: vec![palette::DEEP_TEAL, palette::DEEP_TEAL, palette::BRIGHT_RED],
            down_color: palette::BRIGHT_RED,
            transition_color: palette::BRIGHT_RED,
            transition_negative: palette::DEEP_TEAL,
            neutral_color: palette::BRIGHT_RED,
            neutral_negative: palette::DEEP_TEAL,
            visible: true,
        }
    }

    /// CCI of the close at bar `i`, or NaN before its window is full.
    fn cci(closes: &[f64], i: usize, length: usize) -> f64 {
        let Some(start) = (i + 1).checked_sub(length) else {
            return f64::NAN;
        };
        let window = &closes[start..=i];
        let n = length as f64;
        let mean = window.iter().sum::<f64>() / n;
        // Correct the rounding of the sum, so equal closes have no deviation.
        let mean = mean + window.iter().map(|c| c - mean).sum::<f64>() / n;
        let deviation = window.iter().map(|c| (c - mean).abs()).sum::<f64>() / n;
        if deviation == 0.0 {
            0.0
        } else {
            (closes[i] - mean) / (0.015 * deviation)
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
        let mut values = IndicatorValues::new();
        for i in 0..closes.len() {
            let cci = Self::cci(&closes, i, self.cci_length);
            values.push_lines(&[cci, Self::cci(&closes, i, self.turbo_length), cci]);
        }
        values
    }
}

impl Default for WoodiesCCI {
    fn default() -> Self {
        Self::new(6, 14)
    }
}

impl Indicator for WoodiesCCI {
    fn level_style(&self, value: f64) -> crate::model::LineStyle {
        if value == 0.0 {
            crate::model::LineStyle::Solid
        } else {
            crate::model::LineStyle::Dotted
        }
    }

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        turbo_length: length("CCI Turbo length", 1, usize::MAX),
        cci_length: length("CCI 14 length", 1, usize::MAX),
        down_color: color("Established down colour"),
        transition_color: color("Transition colour"),
        neutral_color: color("Neutral colour")
    ; changed => {
        if changed == "transition_color" { self.transition_negative = self.transition_color; }
        if changed == "neutral_color" { self.neutral_negative = self.neutral_color; }
    });

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::Histogram
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        if line != 0 {
            return None;
        }
        let value = self.values.value(0, bar)?;
        // Bars without a value, skipped as non-finite, are not in the run.
        let previous = (0..bar).rev().filter_map(|bar| self.values.value(0, bar));
        let run = 1 + previous.take_while(|p| p * value > 0.0).take(5).count();
        Some(match run {
            1 if value < 0.0 => self.neutral_negative,
            1 => self.neutral_color,
            2..=5 if value < 0.0 => self.transition_negative,
            2..=5 => self.transition_color,
            _ if value > 0.0 => self.colors[0],
            _ => self.down_color,
        })
    }

    fn name(&self) -> &str {
        "Woodies CCI"
    }

    fn desc(&self) -> &str {
        "Woodies CCI - Turbo and trading CCI with a trend-coloured histogram"
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
            usable_behind(self.turbo_length.max(self.cci_length) - 1, |b| {
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
        self.colors
            .iter()
            .copied()
            .chain([self.transition_negative, self.neutral_negative])
            .collect()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors[..3].to_vec();
            self.transition_negative = colors.get(3).copied().unwrap_or(self.transition_color);
            self.neutral_negative = colors.get(4).copied().unwrap_or(self.neutral_color);
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = palette::GRAY;
        crate::studies::StudyPane {
            height: None,
            range: None,
            levels: vec![
                (100.0, gray, "Hundred Line"),
                (0.0, gray, "Zero Line"),
                (-100.0, gray, "Minus Line"),
            ],
            fill_between_levels: false,
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

    fn line_count(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("Histogram({})", self.cci_length),
            format!("CCI Turbo({})", self.turbo_length),
            format!("CCI({})", self.cci_length),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// A CCI of length 2 has the sign of the close change, and zero for equal closes, so the closes
    /// set the runs. A run of one or a zero is neutral, two to five values are a transition and six
    /// or more are established, above zero in the columns colour and below it in the down colour. A
    /// non-finite bar is not part of the run, and a zero ends it.
    #[test]
    fn columns_are_staged_by_the_run_on_one_side_of_zero() {
        let closes = [
            0.0,
            1.0,
            2.0,
            f64::NAN,
            3.0,
            4.0,
            5.0,
            6.0,
            5.0,
            4.0,
            5.0,
            4.0,
            3.0,
            2.0,
            1.0,
            0.0,
            -1.0,
            -1.0,
            -2.0,
        ];
        let bars: Vec<Bar> = closes
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
        let mut study = WoodiesCCI::new(2, 2);
        study.calculate(&bars);
        let (g, r) = (study.colors[0], study.down_color);
        let expected = [
            None,
            Some(r),
            Some(r),
            None,
            Some(r),
            Some(r),
            Some(r),
            Some(g),
            Some(g),
            Some(g),
            Some(r),
            Some(g),
            Some(g),
            Some(g),
            Some(g),
            Some(g),
            Some(r),
            Some(r),
            Some(g),
        ];
        let colours = |study: &WoodiesCCI| -> Vec<_> {
            (0..bars.len())
                .map(|bar| study.plot_color(0, bar))
                .collect()
        };
        assert_eq!(colours(&study), expected);
        // The columns colour of the plot and the three stage inputs replace the defaults.
        study.set_color(0, Color32::BLUE);
        for id in ["down_color", "transition_color", "neutral_color"] {
            study
                .set_input(id, crate::studies::InputValue::Color(Color32::GOLD))
                .unwrap();
        }
        study.calculate(&bars);
        let mut recoloured = expected.map(|c| c.map(|_| Color32::GOLD));
        recoloured[7] = Some(Color32::BLUE);
        assert_eq!(colours(&study), recoloured);
    }
}
