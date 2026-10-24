//! Advance/Decline Ratio (Bars)
//!
//! The breadth ratio of one instrument's own bars: how many of the last `length` bars closed
//! up against how many closed down.
//!
//! # Formula
//! ```text
//! up bar:   close > open
//! down bar: close < open   (a bar with close == open counts as neither)
//! ratio = count(up bars, length) / count(down bars, length)
//! ```
//! with conventional length 9. A window without a down bar has no value.
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
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Up bars over down bars within a window.
#[derive(Clone)]
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
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite open and close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let direction = |b: &Bar| (usize::from(b.close > b.open), usize::from(b.close < b.open));
        let (mut up, mut down) = (0, 0);
        let mut values = IndicatorValues::new();
        for (i, bar) in data.iter().enumerate() {
            let (u, d) = direction(bar);
            (up, down) = (up + u, down + d);
            if let Some(gone) = i.checked_sub(self.period) {
                let (u, d) = direction(&data[gone]);
                (up, down) = (up - u, down - d);
            }
            let full = i + 1 >= self.period;
            values.push_lines(&[if full && down > 0 {
                up as f64 / down as f64
            } else {
                f64::NAN
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
        self.values = over_finite_bars(
            data,
            |b| b.open.is_finite() && b.close.is_finite(),
            |data| self.compute(data),
        );
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("A/D Ratio({})", self.period)]
    }
}
