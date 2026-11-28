//! Volume columns with an optional moving average on an independent scale.

use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, PlotStyle, PlotType};
use egui::Color32;
use std::collections::VecDeque;

/// Comparison used to colour volume columns. An unchanged close is always up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeColorRule {
    /// Compare this bar's close with its open.
    Open,
    /// Compare with the previous usable close; the first bar uses its open.
    PreviousClose,
}

impl VolumeColorRule {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::PreviousClose => "Previous close",
        }
    }
}

/// Moving average formula for volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VolumeAverage {
    /// Arithmetic mean of a complete window.
    Simple,
    /// Alpha 2/(length+1), seeded by the first usable volume.
    Exponential,
    /// Weights 1 to length over a complete window, the newest volume heaviest.
    Weighted,
}

impl VolumeAverage {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Exponential => "Exponential",
            Self::Weighted => "Weighted",
        }
    }
}

/// Volume columns and a moving average, in the price pane or a separate pane.
///
/// Outputs are up volume, down volume, the average and an optional Smoothed MA. Exactly one volume
/// column is defined per usable bar. Equal prices count as up under both rules;
/// a bar with non-finite volume, open or close leaves all outputs missing.
#[derive(Clone, Debug)]
pub struct Volume {
    period: usize,
    average: VolumeAverage,
    show_average: bool,
    color_rule: VolumeColorRule,
    overlay: bool,
    smoothing: SmoothingType,
    smoothing_length: usize,
    colors: [Color32; 4],
    visible: bool,
    values: IndicatorValues,
    stepper: Stepper<State>,
}

#[derive(Clone, Debug, Default)]
struct State {
    previous: Option<f64>,
    window: VecDeque<f64>,
    ema: Option<f64>,
    smoothed: Smoothed,
}

const DEFAULT_PERIOD: usize = 20;

impl Default for Volume {
    fn default() -> Self {
        Self::new(DEFAULT_PERIOD)
    }
}

impl Volume {
    /// The chart's automatic volume columns are derived solely from each displayed bar.
    #[cfg(feature = "widget")]
    pub(crate) fn default_replay_display(&self) -> bool {
        !self.show_average
            && self.smoothing == SmoothingType::None
            && self.average == VolumeAverage::Simple
            && self.color_rule == VolumeColorRule::Open
    }

    /// Volume with an SMA of `period` bars (zero becomes one) that is off until enabled,
    /// overlaid by default.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            average: VolumeAverage::Simple,
            show_average: false,
            color_rule: VolumeColorRule::Open,
            overlay: true,
            smoothing: SmoothingType::None,
            smoothing_length: 9,
            colors: [
                palette::HISTOGRAM_TEAL.gamma_multiply_u8(128),
                Color32::from_rgba_unmultiplied(239, 83, 80, 128),
                palette::LIGHT_BLUE,
                SMOOTHING_COLORS[0],
            ],
            visible: true,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
        }
    }

    /// Choose a price comparison for column colours.
    pub fn with_color_rule(mut self, rule: VolumeColorRule) -> Self {
        self.color_rule = rule;
        self
    }

    /// Choose SMA, EMA, WMA, or no moving average.
    pub fn with_average(mut self, average: Option<VolumeAverage>) -> Self {
        self.show_average = average.is_some();
        if let Some(average) = average {
            self.average = average;
        }
        self
    }

    /// Draw in the price pane (`true`) or in a separate pane (`false`).
    pub fn with_overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }
}

impl Indicator for Volume {
    crate::studies::inputs::study_inputs!(self;
        period: length("MA length", 1, 2000),
        average: choice("MA type", [VolumeAverage::Simple, VolumeAverage::Exponential, VolumeAverage::Weighted]),
        show_average: toggle("Moving average"),
        color_rule: choice("Colour by", [VolumeColorRule::Open, VolumeColorRule::PreviousClose]),
        overlay: toggle("Overlay price pane"),
        smoothing: choice("Smoothing", [SmoothingType::None, SmoothingType::Sma, SmoothingType::Ema, SmoothingType::Wma]),
        smoothing_length: length("Smoothing length", 1, 10000) if smoothing in [1, 2, 3]
    ; {});

    fn name(&self) -> &str {
        "Volume"
    }
    fn desc(&self) -> &str {
        "Volume with optional SMA, EMA, WMA and smoothing; unchanged prices count as up"
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, average, show_average, rule) = (
            self.period,
            self.average,
            self.show_average,
            self.color_rule,
        );
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.volume.is_finite() || !bar.open.is_finite() || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let reference = match rule {
                    VolumeColorRule::Open => bar.open,
                    VolumeColorRule::PreviousClose => state.previous.unwrap_or(bar.open),
                };
                state.previous = Some(bar.close);
                state.window.push_back(bar.volume);
                if state.window.len() > period {
                    state.window.pop_front();
                }
                let ema = state.ema.map_or(bar.volume, |previous| {
                    previous + (bar.volume - previous) * (2.0 / (period as f64 + 1.0))
                });
                state.ema = Some(ema);
                let mean = if !show_average {
                    f64::NAN
                } else {
                    match average {
                        VolumeAverage::Simple if state.window.len() == period => {
                            state.window.iter().map(|v| v / period as f64).sum()
                        }
                        VolumeAverage::Simple => f64::NAN,
                        VolumeAverage::Exponential => ema,
                        VolumeAverage::Weighted if state.window.len() == period => {
                            let n = period as f64;
                            state
                                .window
                                .iter()
                                .enumerate()
                                .map(|(i, v)| (i + 1) as f64 * v)
                                .sum::<f64>()
                                / (n * (n + 1.0) / 2.0)
                        }
                        VolumeAverage::Weighted => f64::NAN,
                    }
                };
                let smoothed = match state.smoothed.next(
                    self.smoothing,
                    self.smoothing_length,
                    0.0,
                    bar.volume,
                    bar.volume,
                ) {
                    IndicatorValue::Multiple(values) => values[1],
                    _ => f64::NAN,
                };
                let mut outputs = vec![f64::NAN, f64::NAN, mean, smoothed];
                outputs[usize::from(bar.close < reference)] = bar.volume;
                IndicatorValue::Multiple(outputs)
            });
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
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line < 2 {
                PlotType::Columns
            } else {
                PlotType::Line
            },
            visible: match line {
                0 | 1 => true,
                2 => self.show_average,
                _ => self.smoothing != SmoothingType::None,
            },
        }
    }
    fn is_overlay(&self) -> bool {
        self.overlay
    }
    fn is_volume(&self) -> bool {
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
        vec![
            "Up volume".into(),
            "Down volume".into(),
            format!(
                "{} {}",
                match self.average {
                    VolumeAverage::Simple => "SMA",
                    VolumeAverage::Exponential => "EMA",
                    VolumeAverage::Weighted => "WMA",
                },
                self.period
            ),
            "Smoothed MA".into(),
        ]
    }

    // The optional averages hold values only while they are shown.
    fn alert_outputs(&self) -> Vec<(usize, String)> {
        self.line_names()
            .into_iter()
            .enumerate()
            .filter(|(line, _)| self.plot_style(*line).visible)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn comparisons_and_averages_match_hand_worked_bars() {
        let bars: Vec<_> = [
            (10.0, 12.0, 10.0),
            (13.0, 12.5, 20.0),
            (11.0, 11.5, 30.0),
            (11.5, 11.5, 40.0),
        ]
        .iter()
        .enumerate()
        .map(|(i, &(open, close, volume))| {
            Bar::new(
                Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                open,
                14.0,
                9.0,
                close,
                volume,
            )
        })
        .collect();
        for (rule, up) in [
            (0, [true, false, true, true]),
            (1, [true, true, false, true]),
        ] {
            for (average, expected) in [
                (Some(0), [None, Some(15.0), Some(25.0), Some(35.0)]),
                (
                    Some(1),
                    [
                        Some(10.0),
                        Some(50.0 / 3.0),
                        Some(230.0 / 9.0),
                        Some(950.0 / 27.0),
                    ],
                ),
                (
                    Some(2),
                    [None, Some(50.0 / 3.0), Some(80.0 / 3.0), Some(110.0 / 3.0)],
                ),
                (None, [None; 4]),
            ] {
                let mut study = crate::studies::IndicatorFactory::new()
                    .create("Volume")
                    .expect("registered Volume study");
                study
                    .set_input("period", crate::studies::InputValue::Length(2))
                    .unwrap();
                study
                    .set_input("color_rule", crate::studies::InputValue::Choice(rule))
                    .unwrap();
                study
                    .set_input(
                        "show_average",
                        crate::studies::InputValue::Toggle(average.is_some()),
                    )
                    .unwrap();
                if let Some(average) = average {
                    study
                        .set_input("average", crate::studies::InputValue::Choice(average))
                        .unwrap();
                }
                study.calculate(&bars);
                for (i, is_up) in up.iter().enumerate() {
                    let line = usize::from(!is_up);
                    assert_eq!(study.values().value(line, i), Some(bars[i].volume));
                    assert_eq!(study.values().value(1 - line, i), None);
                    assert_eq!(
                        study.colors()[line],
                        if *is_up {
                            Color32::from_rgba_unmultiplied(38, 166, 154, 128)
                        } else {
                            Color32::from_rgba_unmultiplied(239, 83, 80, 128)
                        }
                    );
                    match expected[i] {
                        Some(value) => {
                            assert!((study.values().value(2, i).unwrap() - value).abs() < 1e-10)
                        }
                        None => assert_eq!(study.values().value(2, i), None),
                    }
                }
                let mut changed = bars.clone();
                changed[3].volume = 60.0;
                study.update(&changed, 3);
                let mut fresh = study.clone_box();
                fresh.calculate(&changed);
                for line in 0..4 {
                    for i in 0..4 {
                        assert_eq!(study.values().value(line, i), fresh.values().value(line, i));
                    }
                }
            }
        }
        // SMA(3) averages the complete three-volume window; WMA(2) weighs the newest twice.
        // EMA(3) seeds at 20 then adds half the step to 40.
        for (kind, length, expected) in [
            (0, 2, [None; 4]),
            (1, 3, [None, None, Some(20.0), Some(30.0)]),
            (2, 3, [None, None, Some(20.0), Some(30.0)]),
            (
                3,
                2,
                [None, Some(50.0 / 3.0), Some(80.0 / 3.0), Some(110.0 / 3.0)],
            ),
        ] {
            let mut study = Volume::new(2);
            study
                .set_input("smoothing", crate::studies::InputValue::Choice(kind))
                .unwrap();
            study
                .set_input(
                    "smoothing_length",
                    crate::studies::InputValue::Length(length),
                )
                .unwrap();
            study.calculate(&bars[..3]);
            study.update(&bars, 3);
            for (at, expected) in expected.into_iter().enumerate() {
                match expected {
                    Some(value) => {
                        assert!((study.plot_value(3, at).unwrap() - value).abs() < 1e-10)
                    }
                    None => assert_eq!(study.plot_value(3, at), None),
                }
            }
            assert_eq!(
                study
                    .alert_outputs()
                    .iter()
                    .map(|(i, _)| *i)
                    .collect::<Vec<_>>(),
                if kind == 0 { vec![0, 1] } else { vec![0, 1, 3] }
            );
            let mut changed = bars.clone();
            changed[3].volume = 60.0;
            study.update(&changed, 3);
            let mut fresh = study.clone();
            fresh.calculate(&changed);
            assert_eq!(study.plot_value(3, 3), fresh.plot_value(3, 3));
        }
        assert!(
            !Volume::default().plot_style(2).visible,
            "the average starts off"
        );
        let mut study =
            crate::studies::ConfiguredStudy::new(Box::new(Volume::new(2).with_average(None)));
        for enabled in [false, true, false, true] {
            study
                .set_input("show_average", crate::studies::InputValue::Toggle(enabled))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.plot_style(2).visible, enabled);
            assert_eq!(study.values().value(2, 3), enabled.then_some(35.0));
        }
        study
            .set_input("smoothing_length", crate::studies::InputValue::Length(2))
            .unwrap();
        for kind in [0, 1, 0, 1] {
            study
                .set_input("smoothing", crate::studies::InputValue::Choice(kind))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.plot_style(3).visible, kind != 0);
            assert_eq!(study.plot_value(3, 3), (kind != 0).then_some(35.0));
        }
        let mut settings = study.settings();
        settings.plots[2].visible = false;
        study.apply_settings(&[], settings).unwrap();
        assert!(
            !study.plot_style(2).visible,
            "style visibility still applies"
        );
    }
}
