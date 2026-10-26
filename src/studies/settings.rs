//! Applied study presentation and transactional input edits.

use super::{Indicator, IndicatorLineStyle, IndicatorValues, InputValue};
use crate::model::{Bar, Instrument, Timeframe};
use egui::Color32;

/// Stroke pattern for a study plot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlotPattern {
    /// A continuous stroke.
    #[default]
    Solid,
    /// Equally spaced dashes.
    Dashed,
    /// Equally spaced dots.
    Dotted,
    /// Dashes twice as long as `Dashed`, with twice the gap.
    LargeDashed,
    /// Dots with twice the spacing of `Dotted`.
    SparseDotted,
}

/// Geometry for one output line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlotType {
    /// Connected values, broken at missing bars.
    #[default]
    Line,
    /// Bars from zero to the output value.
    Histogram,
    /// A circle at each defined value.
    Circles,
    /// Horizontal segments followed by a vertical change at the next bar.
    StepLine,
    /// Columns from zero, at 80% of the bar pitch.
    Columns,
    /// A plus at each defined value.
    Crosses,
    /// Filled area between the output and zero.
    Area,
    /// An upward triangle centred on each defined value.
    TriangleUp,
    /// A downward triangle centred on each defined value.
    TriangleDown,
    /// Repaint the source price bars using this plot's per-bar colour.
    BarColor,
    /// Colour the full pane height at each defined bar.
    BackgroundColor,
}

/// Editable presentation of one output line.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PlotStyle {
    /// Plot colour.
    pub color: Color32,
    /// Stroke width in logical pixels, from 0.5 to 10.
    pub width: f32,
    /// Stroke pattern for line plots.
    pub pattern: PlotPattern,
    /// Plot geometry.
    pub plot_type: PlotType,
    /// Whether the plot participates in rendering, hit testing and scaling.
    pub visible: bool,
}

/// Host-serializable presentation settings of an applied study.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StudySettings {
    /// One style per declared output line.
    pub plots: Vec<PlotStyle>,
    /// Decimal places, or the chart's default precision.
    pub precision: Option<usize>,
    /// Exact intervals on which the study is hidden, including custom intervals.
    pub hidden_intervals: Vec<Timeframe>,
}

/// A study with editable presentation, used automatically by the chart.
/// Hosts composing their own chart can wrap built-in or custom studies the same way.
#[derive(Clone)]
pub struct ConfiguredStudy {
    study: Box<dyn Indicator>,
    settings: StudySettings,
    timeframe: Option<Timeframe>,
    instrument_precision: Option<u8>,
    overlay: Option<bool>,
    scale: Option<super::StudyScale>,
}

impl ConfiguredStudy {
    /// Retain the study's current colours, widths and calculation parameters.
    pub fn new(study: Box<dyn Indicator>) -> Self {
        let mut settings = study.settings();
        // The optional average input must not become a permanent style override.
        if study.is_volume()
            && let Some(plot) = settings.plots.get_mut(2)
        {
            plot.visible = true;
        }
        Self {
            study,
            settings,
            timeframe: None,
            instrument_precision: None,
            overlay: None,
            scale: None,
        }
    }
}

impl Indicator for ConfiguredStudy {
    fn has_settings(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.study.name()
    }
    fn desc(&self) -> &str {
        self.study.desc()
    }
    fn inputs(&self) -> Vec<super::StudyInput> {
        self.study.inputs()
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        self.study.set_input(id, value)
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.study.calculate(data);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        self.study.update(data, from);
    }
    fn is_incremental(&self) -> bool {
        self.study.is_incremental()
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.instrument_precision = instrument.and_then(crate::scales::instrument_price_precision);
        self.study.set_instrument(instrument)
    }
    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        self.study.set_timeframe(timeframe);
    }
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        self.study.set_bar_clock(clock)
    }
    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        self.study.drawings()
    }
    fn candles(&self) -> &[super::StudyCandle] {
        self.study.candles()
    }
    fn tpo(&self) -> Option<&crate::model::TPOConfig> {
        self.study.tpo()
    }
    fn values(&self) -> &IndicatorValues {
        self.study.values()
    }
    fn colors(&self) -> Vec<Color32> {
        self.settings.plots.iter().map(|plot| plot.color).collect()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        self.study.set_colors(colors);
        for (plot, color) in self.settings.plots.iter_mut().zip(self.study.colors()) {
            plot.color = color;
        }
    }
    fn line_style(&self, line: usize) -> IndicatorLineStyle {
        let mut style = self.study.line_style(line);
        if let Some(plot) = self.settings.plots.get(line) {
            style.width = plot.width;
        }
        style
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        let mut plot = self
            .settings
            .plots
            .get(line)
            .copied()
            .unwrap_or_else(|| self.study.plot_style(line));
        if self.study.is_volume() {
            plot.visible &= self.study.plot_style(line).visible;
        }
        plot
    }
    fn plot_offset(&self, line: usize) -> isize {
        self.study.plot_offset(line)
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        self.study.plot_color(line, bar)
    }
    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        self.study.plot_text(line, bar)
    }
    fn fills(&self) -> Vec<super::StudyFill> {
        self.study.fills()
    }
    fn settings(&self) -> StudySettings {
        self.settings.clone()
    }
    fn apply_settings(
        &mut self,
        inputs: &[(String, InputValue)],
        settings: StudySettings,
    ) -> Result<(), &'static str> {
        if settings.precision.is_some_and(|precision| precision > 12)
            || settings
                .plots
                .iter()
                .any(|plot| !plot.width.is_finite() || !(0.5..=10.0).contains(&plot.width))
        {
            return Err("invalid study presentation settings");
        }
        let mut edited = self.study.clone_box();
        for (id, value) in inputs {
            edited.set_input(id, value.clone())?;
        }
        if settings.plots.len() != edited.line_cnt() {
            return Err("one style is required per study plot");
        }
        edited.set_colors(settings.plots.iter().map(|plot| plot.color).collect());
        self.study = edited;
        self.settings = settings;
        Ok(())
    }
    fn value_format(&self) -> super::StudyFormat {
        self.study.value_format()
    }
    fn scale(&self) -> super::StudyScale {
        self.scale.unwrap_or_else(|| self.study.scale())
    }
    fn precision(&self) -> Option<usize> {
        self.settings
            .precision
            .or_else(|| self.study.precision())
            .or_else(|| {
                (self.value_format() == super::StudyFormat::Price)
                    .then_some(self.instrument_precision.map(usize::from))
                    .flatten()
            })
    }
    fn line_cnt(&self) -> usize {
        self.study.line_cnt()
    }
    fn line_names(&self) -> Vec<String> {
        self.study.line_names()
    }
    fn markers(&self) -> &[super::IndicatorMarker] {
        self.study.markers()
    }
    fn volume_profiles(&self, visible: std::ops::Range<usize>) -> Vec<super::StudyProfile> {
        self.study.volume_profiles(visible)
    }
    fn zones(&self) -> &[super::StudyZone] {
        self.study.zones()
    }
    fn extend_right(&self) -> bool {
        self.study.extend_right()
    }
    fn plot_label(&self) -> Option<(usize, String)> {
        self.study.plot_label()
    }
    fn is_volume(&self) -> bool {
        self.study.is_volume()
    }
    fn pane(&self) -> super::StudyPane {
        self.study.pane()
    }
    fn is_overlay(&self) -> bool {
        self.overlay.unwrap_or_else(|| self.study.is_overlay())
    }
    fn set_overlay(&mut self, overlay: bool) -> bool {
        self.overlay = Some(overlay);
        true
    }
    fn set_scale(&mut self, scale: super::StudyScale) -> bool {
        self.scale = Some(scale);
        true
    }
    fn is_visible(&self) -> bool {
        self.study.is_visible()
            && self
                .timeframe
                .is_none_or(|tf| !self.settings.hidden_intervals.contains(&tf))
    }
    fn set_visible(&mut self, visible: bool) {
        self.study.set_visible(visible);
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::studies::BollingerBands;

    #[test]
    fn automatic_precision_tracks_metadata_and_preserves_study_overrides() {
        let mut study = ConfiguredStudy::new(Box::new(super::super::MACD::default()));
        let instrument = Instrument::new("FX").with_tick_size(0.00001);
        study.set_instrument(Some(&instrument));
        assert_eq!(study.precision(), Some(5));
        assert_eq!(study.settings().precision, None);
        let mut settings = study.settings();
        settings.precision = Some(3);
        study.apply_settings(&[], settings).unwrap();
        study.set_instrument(Some(&Instrument::new("INT").with_tick_size(1.0)));
        assert_eq!(study.precision(), Some(3));
        let mut settings = study.settings();
        settings.precision = None;
        study.apply_settings(&[], settings).unwrap();
        assert_eq!(study.precision(), Some(0));
        study.set_instrument(None);
        assert_eq!(study.precision(), None);
        for (bounded, precision) in [
            (
                Box::new(super::super::RSI::default()) as Box<dyn Indicator>,
                Some(2),
            ),
            (Box::new(super::super::Stochastic::default()), Some(2)),
            (Box::new(super::super::OnBalanceVolume::default()), None),
            (
                Box::new(
                    super::super::CustomIndicator::new("Small", |_| Vec::new())
                        .with_value_format(super::super::StudyFormat::Fixed(4)),
                ),
                Some(4),
            ),
        ] {
            let mut bounded = ConfiguredStudy::new(bounded);
            bounded.set_instrument(Some(&instrument));
            let mut settings = bounded.settings();
            settings.precision = None;
            bounded.apply_settings(&[], settings).unwrap();
            assert_eq!(bounded.precision(), precision);
        }
    }

    #[test]
    fn edits_are_atomic_and_interval_visibility_preserves_the_global_toggle() {
        let mut study = ConfiguredStudy::new(Box::new(BollingerBands::new(3, 2.0)));
        let mut settings = study.settings();
        settings.plots[0].visible = false;
        settings.plots[1].width = 3.0;
        settings.precision = Some(4);
        settings.hidden_intervals.push(Timeframe::Min1);
        assert!(
            study
                .apply_settings(
                    &[
                        ("period".into(), InputValue::Length(2)),
                        ("std_dev".into(), InputValue::Number(f64::NAN))
                    ],
                    settings.clone()
                )
                .is_err()
        );
        assert_eq!(study.inputs()[0].value, InputValue::Length(3));
        assert!(study.plot_style(0).visible);
        study
            .apply_settings(&[("period".into(), InputValue::Length(2))], settings)
            .unwrap();
        assert_eq!(study.inputs()[0].value, InputValue::Length(2));
        assert!(!study.plot_style(0).visible);
        assert_eq!(study.line_style(1).width, 3.0);
        assert_eq!(study.precision(), Some(4));
        study.set_timeframe(Timeframe::Min1);
        assert!(!study.is_visible());
        study.set_timeframe(Timeframe::Hour1);
        assert!(study.is_visible());
        study.set_visible(false);
        study.set_timeframe(Timeframe::Day1);
        assert!(!study.is_visible());
        let saved = serde_json::to_string(&study.settings()).unwrap();
        let restored: StudySettings = serde_json::from_str(&saved).unwrap();
        assert!(!restored.plots[0].visible);
        assert_eq!(restored.hidden_intervals, [Timeframe::Min1]);
    }
}
