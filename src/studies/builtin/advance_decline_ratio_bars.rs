//! Advance/Decline Ratio (Bars)
//!
//! The breadth ratio of one instrument's own bars: how many of the last `length` bars closed
//! up against how many closed down.
//!
//! # Formula
//! ```text
//! up bar:   close >= open   (unchanged bars count as up)
//! down bar: close < open
//! ratio = count(up bars, length) / count(down bars, length)
//!       = count(up bars, length) when there are no down bars
//! ```
//! with conventional length 9. The pane marks equal up and down counts with a level at 1.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + if i % 3 == 0 { -0.5 } else { 0.5 }, 1_000.0) }).collect();
//! use egui_charts::studies::{AdvanceDeclineRatioBars, Indicator};
//!
//! let mut ratio = AdvanceDeclineRatioBars::new(9);
//! ratio.calculate(&bars);
//! // Six up bars for every three down bars.
//! assert_eq!(ratio.values().value(0, 8), Some(2.0));
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Up bars over down bars within a window.
#[derive(Clone, Debug)]
pub struct AdvanceDeclineRatioBars {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl AdvanceDeclineRatioBars {
    /// Create the ratio over the last `period` bars (conventionally 9).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite open and close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let down = |b: &Bar| usize::from(b.close < b.open);
        let mut downs = 0;
        let mut values = IndicatorValues::new();
        for (i, bar) in data.iter().enumerate() {
            downs += down(bar);
            if let Some(gone) = i.checked_sub(self.period) {
                downs -= down(&data[gone]);
            }
            let ups = (self.period - downs) as f64;
            values.push_lines(&[if i + 1 < self.period {
                f64::NAN
            } else if downs == 0 {
                ups
            } else {
                ups / downs as f64
            }]);
        }
        values
    }
}

impl Default for AdvanceDeclineRatioBars {
    fn default() -> Self {
        Self::new(9)
    }
}

impl Indicator for AdvanceDeclineRatioBars {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Advance/Decline Ratio (Bars)"
    }

    fn desc(&self) -> &str {
        "Advance/Decline Ratio (Bars) - Up bars over down bars in a window"
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
        vec![format!("A/D Ratio({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unchanged bars count as up; a window without down bars gives its up count, calculated
    /// at once and bar by bar.
    #[test]
    fn unchanged_bars_count_as_up_and_a_window_without_down_bars_gives_the_up_count() {
        // Up, down, unchanged, up, up, up, unchanged, down.
        let bars: Vec<_> = [1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 0.0, -1.0]
            .into_iter()
            .enumerate()
            .map(|(i, change)| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
                Bar::new(time, 10.0, 12.0, 8.0, 10.0 + change, 1.0)
            })
            .collect();
        let mut ratio = AdvanceDeclineRatioBars::new(3);
        ratio.calculate(&bars);
        let values: Vec<_> = (0..bars.len())
            .map(|i| ratio.values().value(0, i))
            .collect();
        assert_eq!(
            values,
            [
                None,
                None,
                Some(2.0),
                Some(2.0),
                Some(3.0),
                Some(3.0),
                Some(3.0),
                Some(2.0)
            ]
        );
        super::super::assert_replay_matches_batch(&ratio, &bars);
    }
}
