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
//! the CCI 14 line. A column is green when the five previous CCI 14 values are all above zero and
//! red when they are all below. Otherwise it is coloured against its own sign, green below zero and
//! red at or above it, the colouring of the conventional built-in. Fewer than five previous values
//! count as mixed. Levels at 100, 0 and -100.
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
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Woodies CCI: CCI 14 columns, CCI Turbo and CCI 14 lines in a separate pane.
#[derive(Clone)]
pub struct WoodiesCCI {
    turbo_length: usize,
    cci_length: usize,
    values: IndicatorValues,
    /// Colors: [columns, CCI Turbo, CCI 14]; the columns colour is used after five values above
    /// zero or a mixed run below zero.
    colors: Vec<Color32>,
    /// Column colour otherwise.
    down_color: Color32,
    visible: bool,
}

impl WoodiesCCI {
    /// Create the study with the turbo and trading CCI lengths (6 and 14 by convention).
    pub fn new(turbo_length: usize, cci_length: usize) -> Self {
        let extended = &DESIGN_TOKENS.semantic.extended;
        Self {
            turbo_length: turbo_length.max(1),
            cci_length: cci_length.max(1),
            values: IndicatorValues::new(),
            colors: vec![extended.success, extended.success, extended.error],
            down_color: extended.error,
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
    crate::studies::inputs::study_inputs!(self;
        turbo_length: length("CCI Turbo length", 1, usize::MAX),
        cci_length: length("CCI 14 length", 1, usize::MAX)
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: if line == 0 { 1.0 } else { 2.0 },
            pattern: crate::studies::PlotPattern::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::Columns
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
        let (mut above, mut below) = (0, 0);
        for value in previous.take(5) {
            above += usize::from(value > 0.0);
            below += usize::from(value < 0.0);
        }
        Some(if above == 5 {
            self.colors[0]
        } else if below == 5 || value >= 0.0 {
            self.down_color
        } else {
            self.colors[0]
        })
    }

    fn name(&self) -> &str {
        "Woodies CCI"
    }

    fn desc(&self) -> &str {
        "Woodies CCI - Turbo and trading CCI with a trend-coloured histogram"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = DESIGN_TOKENS.semantic.extended.gray;
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

    fn line_cnt(&self) -> usize {
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

    /// A CCI of length 2 has the sign of the close change, so the closes set the run of signs. A
    /// column is green after five values above zero, red after five below, and otherwise green below
    /// zero and red at or above it; a non-finite bar is not part of the run. The green is the
    /// columns colour.
    #[test]
    fn columns_are_coloured_by_the_five_previous_values() {
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
        let (g, r) = (
            DESIGN_TOKENS.semantic.extended.success,
            DESIGN_TOKENS.semantic.extended.error,
        );
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
        ];
        let colours = |study: &WoodiesCCI| -> Vec<_> {
            (0..bars.len())
                .map(|bar| study.plot_color(0, bar))
                .collect()
        };
        assert_eq!(colours(&study), expected);
        // The columns colour set in the settings replaces the green.
        study.set_color(0, Color32::BLUE);
        let recoloured = expected.map(|c| c.map(|c| if c == g { Color32::BLUE } else { c }));
        assert_eq!(colours(&study), recoloured);
    }
}
