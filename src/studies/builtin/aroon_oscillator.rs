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
//! counting the most recent bar. The value ranges from -100 to 100. Guides at 90 and -90 mark
//! a strong uptrend or downtrend. The line takes the plot colour at or above zero and the
//! Below zero colour under it, both editable in the Style tab, and is shaded to zero in the
//! same two colours at a tenth of full opacity.
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
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Aroon Up minus Aroon Down.
#[derive(Clone, Debug)]
pub struct AroonOscillator {
    period: usize,
    values: IndicatorValues,
    colors: [Color32; 2],
    visible: bool,
}

impl AroonOscillator {
    /// Create an Aroon Oscillator over `period` bars (conventionally 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: [palette::GREEN, palette::HISTOGRAM_RED],
            visible: true,
        }
    }
}

impl Default for AroonOscillator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl AroonOscillator {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut aroon = Aroon::new(self.period);
        aroon.calculate(data);
        let lines = aroon.values();
        (0..data.len())
            .map(|bar| match (lines.value(0, bar), lines.value(1, bar)) {
                (Some(up), Some(down)) => IndicatorValue::Single(up - down),
                _ => IndicatorValue::None,
            })
            .collect()
    }
}

impl Indicator for AroonOscillator {
    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        self.values
            .value(0, bar)
            .map(|value| self.colors[usize::from(value < 0.0)])
    }
    fn line_count(&self) -> usize {
        1
    }

    fn draws_only_plots(&self) -> bool {
        true
    }
    fn plot_variants(&self, _line: usize) -> Vec<crate::studies::PlotVariant> {
        vec![crate::studies::PlotVariant::new(
            "Below zero",
            self.colors[1],
        )]
    }

    fn plot_variant(&self, _line: usize, bar: usize) -> Option<usize> {
        (self.values.value(0, bar)? < 0.0).then_some(0)
    }

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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period, |bar| {
                bar.high.is_finite() && bar.low.is_finite()
            }),
            |data| self.compute(data),
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
        if let Some(&color) = colors.first() {
            self.colors = [color, colors.get(1).copied().unwrap_or(color)];
        }
    }

    /// Shade to zero with the same sign colours as the line.
    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Level(0.0),
            color: self.colors[0].gamma_multiply_u8(26),
            inverted_color: Some(self.colors[1].gamma_multiply_u8(26)),
        }]
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            range: Some((-100.0, 100.0)),
            levels: vec![
                (90.0, palette::GRAY, "Upper level"),
                (0.0, palette::GRAY, "Center"),
                (-90.0, palette::GRAY, "Lower level"),
            ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{ConfiguredStudy, StudyTemplate, test_support::make_bar};

    #[test]
    fn each_sign_colour_is_edited_alone_and_saved() {
        let data: Vec<_> = [
            (10.0, 9.0),
            (11.0, 10.0),
            (12.0, 11.0),
            (11.0, 10.0),
            (10.0, 9.0),
            (9.0, 8.0),
            (12.0, 7.0),
        ]
        .into_iter()
        .map(|(high, low)| Bar {
            high,
            low,
            ..make_bar((high + low) / 2.0)
        })
        .collect();
        let mut study = ConfiguredStudy::new(Box::new(AroonOscillator::new(2)));
        study.calculate(&data);
        // At bar 2 the high is current and the low two bars back; at bar 4, the reverse.
        assert_eq!(study.plot_value(0, 2), Some(100.0));
        assert_eq!(study.plot_value(0, 4), Some(-100.0));
        assert_eq!(study.plot_value(0, 6), Some(0.0));
        assert_eq!(study.plot_color(0, 0), None);
        assert_eq!(study.plot_variant(0, 0), None);

        let rising = Color32::from_rgb(1, 2, 3);
        let falling = Color32::from_rgb(4, 5, 6);
        let mut edited = study.settings();
        edited.plots[0].color = rising;
        study.apply_settings(&[], edited).unwrap();
        study.calculate(&data);
        assert_eq!(study.plot_color(0, 2), Some(rising));
        assert_eq!(study.plot_color(0, 4), Some(palette::HISTOGRAM_RED));
        assert_eq!(study.plot_color(0, 6), Some(rising));

        let mut edited = study.settings();
        edited.plot_variants[0][0].color = falling;
        edited.plot_variants[0][0].width = 2.5;
        study.apply_settings(&[], edited).unwrap();
        study.calculate(&data);
        assert_eq!(study.plot_color(0, 2), Some(rising));
        assert_eq!(study.plot_color(0, 4), Some(falling));
        assert_eq!(study.plot_color(0, 6), Some(rising));
        assert_eq!(study.plot_width(0, 4), 2.5);
        assert_eq!(study.plot_width(0, 2), 1.0);

        let text = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
        let mut restored = serde_json::from_str::<StudyTemplate>(&text)
            .unwrap()
            .create()
            .unwrap();
        restored.calculate(&data);
        assert_eq!(restored.plot_color(0, 2), Some(rising));
        assert_eq!(restored.plot_color(0, 4), Some(falling));
        assert_eq!(restored.plot_color(0, 6), Some(rising));
        assert_eq!(restored.plot_width(0, 4), 2.5);

        for (saved_color, below) in [(palette::GREEN, palette::HISTOGRAM_RED), (rising, rising)] {
            for variants in [vec![Vec::new()], Vec::new()] {
                let mut saved = StudyTemplate::from_study(&ConfiguredStudy::new(Box::new(
                    AroonOscillator::new(2),
                )));
                saved.settings.plots[0].color = saved_color;
                saved.settings.plots[0].width = 3.5;
                saved.settings.plot_variants = variants;
                let mut restored = saved.create().unwrap();
                restored.calculate(&data);
                assert_eq!(restored.plot_color(0, 2), Some(saved_color));
                assert_eq!(restored.plot_color(0, 4), Some(below));
                assert_eq!(restored.plot_color(0, 6), Some(saved_color));
                assert_eq!(restored.plot_variants(0)[0].width, 3.5);
            }
        }
    }
}
