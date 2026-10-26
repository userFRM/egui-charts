//! Aroon Oscillator
//!
//! Tushar Chande's Aroon Oscillator is the difference between Aroon Up and Aroon Down: positive
//! when the latest high is more recent than the latest low, negative otherwise.
//!
//! # Formula
//! ```text
//! Aroon Up   = 100 * (length - bars since the highest high) / length
//! Aroon Down = 100 * (length - bars since the lowest low) / length
//! Oscillator = Aroon Up - Aroon Down
//! ```
//! Both use the window of [`Aroon`]: the current bar and the `length` bars before it, a tie
//! counting the most recent bar. The value ranges from -100 to 100.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AroonOscillator, Indicator};
//!
//! let mut oscillator = AroonOscillator::new(14);
//! oscillator.calculate(&bars);
//! assert!(oscillator.values().value(0, 14).is_some());
//! ```

use super::Aroon;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Aroon Up minus Aroon Down.
#[derive(Clone)]
pub struct AroonOscillator {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl AroonOscillator {
    /// Create an Aroon Oscillator over `period` bars (conventionally 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }
}

impl Default for AroonOscillator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for AroonOscillator {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Aroon Oscillator"
    }

    fn desc(&self) -> &str {
        "Aroon Oscillator - Aroon Up minus Aroon Down"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let mut aroon = Aroon::new(self.period);
        aroon.calculate(data);
        let lines = aroon.values();
        self.values = (0..data.len())
            .map(|bar| match (lines.value(0, bar), lines.value(1, bar)) {
                (Some(up), Some(down)) => IndicatorValue::Single(up - down),
                _ => IndicatorValue::None,
            })
            .collect();
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
            range: Some((-100.0, 100.0)),
            levels: vec![(0.0, Color32::from_gray(128), "Zero")],
            ..Default::default()
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
        vec![format!("Aroon Osc({})", self.period)]
    }
}
