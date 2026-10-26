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
    source: Option<super::StudySource>,
    calculation_symbol: String,
    calculation_timeframe: Option<Timeframe>,
    wait_for_close: bool,
    offset: i32,
    clock: crate::model::BarClock,
    aligned: Option<IndicatorValues>,
    aligned_bars: Vec<Option<usize>>,
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
            source: None,
            calculation_symbol: String::new(),
            calculation_timeframe: None,
            wait_for_close: true,
            offset: 0,
            clock: crate::model::BarClock::new(Timeframe::Min1),
            aligned: None,
            aligned_bars: Vec::new(),
        }
    }
    fn update_price(&mut self, data: &[Bar], from: usize) {
        self.aligned = None;
        match self.source() {
            Some(super::StudySource::Price(source)) if self.source() != self.study.source() => {
                let values: Vec<_> = data
                    .iter()
                    .map(|bar| source.compute(bar.open, bar.high, bar.low, bar.close))
                    .collect();
                self.study.update_source(data, from, &values);
            }
            Some(super::StudySource::Study { .. }) => self.study.update_source(data, from, &[]),
            _ => self.study.update(data, from),
        }
    }
}

impl Indicator for ConfiguredStudy {
    fn projections(&self) -> &[super::StudyProjection] {
        self.study.projections()
    }

    fn table(&self) -> Option<&super::StudyTable> {
        self.study.table()
    }

    fn has_settings(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.study.name()
    }
    fn desc(&self) -> &str {
        self.study.desc()
    }
    fn source(&self) -> Option<super::StudySource> {
        self.source.or_else(|| self.study.source())
    }
    fn inputs(&self) -> Vec<super::StudyInput> {
        let mut inputs = self.study.inputs();
        if let Some(default) = self.study.source() {
            let initial = inputs
                .iter()
                .find(|input| input.id == "source")
                .and_then(|input| match input.default {
                    InputValue::Choice(index) => crate::model::PriceSource::all()
                        .get(index)
                        .copied()
                        .map(super::StudySource::Price),
                    InputValue::Source(value) => Some(value),
                    _ => None,
                })
                .unwrap_or(default);
            inputs.retain(|input| input.id != "source");
            inputs.push(
                super::StudyInput::new(
                    "source",
                    "Source",
                    InputValue::Source(self.source.unwrap_or(default)),
                    super::InputKind::Source,
                )
                .with_default(InputValue::Source(initial)),
            );
        }
        if self.study.source().is_some() {
            use super::{InputKind, StudyInput};
            inputs.extend([
                StudyInput::new(
                    "plot_offset",
                    "Offset",
                    InputValue::Integer(self.offset),
                    InputKind::Integer(i32::MIN, i32::MAX),
                )
                .with_default(InputValue::Integer(0)),
                StudyInput::new(
                    "calculation_symbol",
                    "Symbol",
                    InputValue::Symbol(self.calculation_symbol.clone()),
                    InputKind::Symbol,
                )
                .with_default(InputValue::Symbol(String::new())),
                StudyInput::new(
                    "calculation_timeframe",
                    "Interval",
                    InputValue::Timeframe(self.calculation_timeframe),
                    InputKind::Timeframe,
                )
                .with_default(InputValue::Timeframe(None)),
                StudyInput::new(
                    "wait_for_close",
                    "Wait for interval close",
                    InputValue::Toggle(self.wait_for_close),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(true)),
            ]);
        }
        inputs
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if id == "plot_offset" && self.study.source().is_some() {
            let InputValue::Integer(offset) = value else {
                return Err("invalid plot offset");
            };
            self.offset = offset;
            self.aligned = None;
            self.study.calculate(&[]);
            return Ok(());
        }
        if self.study.source().is_some()
            && matches!(
                id,
                "calculation_symbol" | "calculation_timeframe" | "wait_for_close"
            )
        {
            let schema = self
                .inputs()
                .into_iter()
                .find(|input| input.id == id)
                .ok_or("unknown study input")?;
            if !schema.accepts(&value) {
                return Err("invalid study data input");
            }
            if matches!(self.source(), Some(super::StudySource::Study { .. }))
                && !matches!(&value, InputValue::Symbol(s) if s.is_empty())
                && value != InputValue::Timeframe(None)
                && id != "wait_for_close"
            {
                return Err("a study output already follows the chart interval and symbol");
            }
            match value {
                InputValue::Symbol(value) => self.calculation_symbol = value,
                InputValue::Timeframe(value) => self.calculation_timeframe = value,
                InputValue::Toggle(value) => self.wait_for_close = value,
                _ => unreachable!(),
            }
            self.aligned = None;
            self.study.calculate(&[]);
            return Ok(());
        }
        if id == "source" && self.study.source().is_some() {
            if matches!(value, InputValue::Choice(_)) {
                self.study.set_input(id, value)?;
                self.source = None;
                self.aligned = None;
                return Ok(());
            }
            let InputValue::Source(source) = value else {
                return Err("invalid study source");
            };
            if matches!(source, super::StudySource::Study { .. })
                && (!self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some())
            {
                return Err("a study output already follows the chart interval and symbol");
            }
            self.source = Some(source);
            self.aligned = None;
            self.study.calculate(&[]);
            Ok(())
        } else {
            self.study.set_input(id, value)?;
            self.aligned = None;
            Ok(())
        }
    }
    fn data_requests(&self) -> Vec<super::StudyDataRequest> {
        if !self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some() {
            vec![super::StudyDataRequest {
                metric: None,
                symbol: self.calculation_symbol.clone(),
                timeframe: self.calculation_timeframe,
            }]
        } else {
            self.study.data_requests()
        }
    }
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[super::StudyData<'_>]) {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            if self.study.data_requests().is_empty() {
                self.update(data, from);
            } else {
                self.study.update_data(data, from, sources);
            }
            return;
        }
        let request = &self.data_requests()[0];
        let Some(source) = sources.iter().find(|source| &source.request == request) else {
            self.aligned = Some(IndicatorValues::missing(data.len()));
            self.aligned_bars.clear();
            return;
        };
        self.instrument_precision = source
            .instrument
            .and_then(crate::scales::instrument_price_precision);
        self.study.set_instrument(source.instrument);
        self.study.set_timeframe(source.timeframe);
        self.study.set_bar_clock(&source.clock);
        self.update_price(source.bars, 0);
        let mut aligned = IndicatorValues::new();
        self.aligned_bars.clear();
        for (at, bar) in data.iter().enumerate() {
            let forming_higher = at + 1 == data.len()
                && self.wait_for_close
                && self
                    .clock
                    .timeframe()
                    .is_some_and(|tf| source.timeframe.duration_ms() > tf.duration_ms());
            let close = if forming_higher {
                bar.time
            } else {
                self.clock
                    .next_open(self.clock.bar_open(bar.time))
                    .unwrap_or(bar.time)
            };
            let output = source.bar_at(bar.time).and_then(|_| {
                source.output_at(close, !self.wait_for_close && at + 1 == data.len())
            });
            self.aligned_bars.push(output);
            let values: Vec<_> = (0..self.study.line_cnt())
                .map(|line| {
                    output
                        .and_then(|at| self.study.values().value(line, at))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            aligned.push(super::IndicatorValue::Multiple(values));
        }
        self.aligned = Some(aligned);
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        if !self.data_requests().is_empty() {
            self.update_data(data, from, &[]);
        } else {
            self.update_price(data, from);
        }
    }
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        self.study.update_source(data, from, source);
    }
    fn is_incremental(&self) -> bool {
        self.data_requests().is_empty()
            && self.source() == self.study.source()
            && self.study.is_incremental()
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.instrument_precision =
                instrument.and_then(crate::scales::instrument_price_precision);
            self.study.set_instrument(instrument)
        } else {
            false
        }
    }
    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        self.clock = crate::model::BarClock::new(timeframe);
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.set_timeframe(timeframe);
        }
    }
    fn set_visible_range(&mut self, range: std::ops::Range<usize>) -> bool {
        self.study.set_visible_range(range)
    }
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        self.clock = clock.clone();
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.set_bar_clock(clock)
        } else {
            false
        }
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
        self.aligned.as_ref().unwrap_or_else(|| self.study.values())
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
        self.study
            .plot_offset(line)
            .saturating_add(self.offset as isize)
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        if let Some(values) = &self.aligned {
            values.value(
                line,
                bar.checked_add_signed(self.plot_offset(line).checked_neg()?)?,
            )
        } else {
            self.study.plot_value(
                line,
                bar.checked_add_signed((self.offset as isize).checked_neg()?)?,
            )
        }
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_color(line, bar)
    }
    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
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
        let mut edited = self.clone();
        let mut inputs: Vec<_> = inputs.iter().collect();
        if inputs.iter().any(|(id, value)| {
            id == "source" && matches!(value, InputValue::Source(super::StudySource::Study { .. }))
        }) {
            inputs.sort_by_key(|(id, _)| {
                !matches!(id.as_str(), "calculation_symbol" | "calculation_timeframe")
            });
        }
        for (id, value) in inputs {
            edited.set_input(id, value.clone())?;
        }
        if settings.plots.len() != edited.line_cnt() {
            return Err("one style is required per study plot");
        }
        edited.set_colors(settings.plots.iter().map(|plot| plot.color).collect());
        self.study = edited.study;
        self.source = edited.source;
        self.calculation_symbol = edited.calculation_symbol;
        self.calculation_timeframe = edited.calculation_timeframe;
        self.wait_for_close = edited.wait_for_close;
        self.offset = edited.offset;
        self.aligned = None;
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
