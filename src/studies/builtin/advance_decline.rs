//! Advance/Decline
//!
//! The breadth ratio of one instrument's own bars: how many of the last `length` bars closed
//! above their open against how many closed below it.
//!
//! # Formula
//! ```text
//! advancing bar: close > open
//! declining bar: close < open   (a bar that closes at its open is neither)
//! ratio = count(advancing bars, length) / count(declining bars, length)
//!       = count(advancing bars, length) when there are no declining bars
//! ```
//! with conventional length 10. The pane marks equal counts with a level at 1.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + if i % 3 == 0 { -0.5 } else { 0.5 }, 1_000.0) }).collect();
//! use egui_charts::studies::{AdvanceDecline, Indicator};
//!
//! let mut ratio = AdvanceDecline::new(10);
//! ratio.calculate(&bars);
//! // Six advancing bars for every four declining bars.
//! assert_eq!(ratio.values().value(0, 9), Some(1.5));
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Advancing bars over declining bars within a window; bars that close at their open count as
/// neither.
#[derive(Clone, Debug)]
pub struct AdvanceDecline {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl AdvanceDecline {
    /// Create the ratio over the last `period` bars (conventionally 10), clamped into 1 to 2000.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.clamp(1, 2000),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite open and close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let counts = |b: &Bar| (usize::from(b.close > b.open), usize::from(b.close < b.open));
        let (mut advancing, mut declining) = (0, 0);
        let mut values = IndicatorValues::new();
        for (i, bar) in data.iter().enumerate() {
            let (up, down) = counts(bar);
            advancing += up;
            declining += down;
            if let Some(gone) = i.checked_sub(self.period) {
                let (up, down) = counts(&data[gone]);
                advancing -= up;
                declining -= down;
            }
            values.push_lines(&[if i + 1 < self.period {
                f64::NAN
            } else if declining == 0 {
                advancing as f64
            } else {
                advancing as f64 / declining as f64
            }]);
        }
        values
    }
}

impl Default for AdvanceDecline {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for AdvanceDecline {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, 2000)
    ; {});

    fn name(&self) -> &str {
        "Advance/Decline"
    }

    fn desc(&self) -> &str {
        "Advance/Decline - Advancing bars over declining bars in a window"
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
            usable_behind(self.period - 1, |b| {
                b.open.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.open.is_finite() && b.close.is_finite(),
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
        if let Some(&color) = colors.first() {
            self.color = color;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(1.0, palette::GRAY, "Equality")],
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
        vec![format!("A/D({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unchanged bars count as neither; a window without declining bars gives its advancing
    /// count, including zero, calculated at once and bar by bar.
    #[test]
    fn unchanged_bars_count_as_neither_and_a_window_without_declining_bars_gives_the_advancing_count()
     {
        type Row = (&'static [f64], &'static [Option<f64>]);
        let rows: [Row; 2] = [
            // Up, down, unchanged, up, up, up, unchanged, down.
            (
                &[1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 0.0, -1.0],
                &[
                    None,
                    None,
                    Some(1.0),
                    Some(1.0),
                    Some(2.0),
                    Some(3.0),
                    Some(2.0),
                    Some(1.0),
                ],
            ),
            // Unchanged only.
            (&[0.0, 0.0, 0.0], &[None, None, Some(0.0)]),
        ];
        for (changes, expected) in rows {
            let bars: Vec<_> = changes
                .iter()
                .enumerate()
                .map(|(i, change)| {
                    let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
                    Bar::new(time, 10.0, 12.0, 8.0, 10.0 + change, 1.0)
                })
                .collect();
            let mut ad = AdvanceDecline::new(3);
            ad.calculate(&bars);
            let values: Vec<_> = (0..bars.len()).map(|i| ad.values().value(0, i)).collect();
            assert_eq!(values, expected, "{changes:?}");
            super::super::assert_replay_matches_batch(&ad, &bars);
        }
    }
}
