//! Applied study presentation and transactional input edits.

use super::{Indicator, IndicatorLineStyle, IndicatorValues, InputValue};
use crate::model::LineStyle;
use crate::model::{Bar, Instrument, Timeframe};
use egui::Color32;

/// Geometry for one output line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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

impl PlotType {
    /// Every plot type, in the order a style editor lists them.
    pub const ALL: [Self; 11] = [
        Self::Line,
        Self::StepLine,
        Self::Histogram,
        Self::Columns,
        Self::Area,
        Self::Circles,
        Self::Crosses,
        Self::TriangleUp,
        Self::TriangleDown,
        Self::BarColor,
        Self::BackgroundColor,
    ];

    /// User-facing display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::StepLine => "Step line",
            Self::Histogram => "Histogram",
            Self::Columns => "Columns",
            Self::Area => "Area",
            Self::Circles => "Circles",
            Self::Crosses => "Crosses",
            Self::TriangleUp => "Triangle up",
            Self::TriangleDown => "Triangle down",
            Self::BarColor => "Bar colour",
            Self::BackgroundColor => "Background colour",
        }
    }

    /// Why a plot of this type cannot be drawn for a study, or `None` when it can.
    /// `overlay` is whether the study is drawn on the price chart, and `price_scale` whether it
    /// shares the price scale there, which a volume study does not.
    pub fn incompatibility(self, overlay: bool, price_scale: bool) -> Option<&'static str> {
        match self {
            Self::BarColor if !overlay => {
                Some("Bar colour repaints the price bars, so the study must be on the price chart.")
            }
            Self::Histogram | Self::Columns | Self::Area if overlay && price_scale => Some(
                "This type draws from zero, which is far from the prices on the price scale. \
                 Give the study its own scale first.",
            ),
            _ => None,
        }
    }
}

/// Editable presentation of one output line.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PlotStyle {
    /// Plot colour.
    pub color: Color32,
    /// Stroke width in logical pixels, from 0.5 to 10.
    pub width: f32,
    /// Stroke pattern for line plots.
    pub pattern: LineStyle,
    /// Plot geometry.
    pub plot_type: PlotType,
    /// Whether the plot participates in rendering, hit testing and scaling.
    pub visible: bool,
}

/// Editable presentation of one reference level in a study's pane, such as 70 for RSI.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct LevelStyle {
    /// Value the level is drawn at, or `None` for the value the study declares, which can
    /// follow its inputs.
    pub value: Option<f64>,
    /// Line colour.
    pub color: Color32,
    /// Stroke pattern.
    pub pattern: LineStyle,
    /// Whether the line is drawn. A hidden level still bounds the fill between the outer levels.
    pub visible: bool,
}

/// The levels and fill a study declares, as the defaults of its level settings.
pub(super) fn declared_levels<S: Indicator + ?Sized>(
    study: &S,
) -> (Vec<LevelStyle>, bool, Color32) {
    let pane = study.pane();
    let levels = pane
        .levels
        .iter()
        .map(|&(value, color, _)| LevelStyle {
            value: None,
            color,
            pattern: study.level_style(value),
            visible: true,
        })
        .collect();
    (levels, pane.fill_between_levels, study.level_fill_color())
}

/// Settings without one style per declared level take the study's own levels and fill.
fn match_declared_levels(study: &dyn Indicator, settings: &mut StudySettings) {
    let (levels, fill, color) = declared_levels(study);
    if settings.levels.len() != levels.len() {
        settings.levels = levels;
        settings.fill_between_levels = fill;
        settings.level_fill_color = color;
    }
}

/// Interval visibility evaluated for every interval, including custom and trade-based ones.
/// Ordered ranges compare nominal duration for time bars and thresholds within the same
/// trade-based family. Other families are outside the range. `Only` matches the exact interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StudyIntervalRange {
    /// Every interval.
    #[default]
    All,
    /// The chosen interval and longer durations or larger thresholds in its family.
    AndAbove(Timeframe),
    /// The chosen interval and shorter durations or smaller thresholds in its family.
    AndBelow(Timeframe),
    /// The exact chosen interval.
    Only(Timeframe),
}

impl StudyIntervalRange {
    /// Whether `interval` belongs to this visibility range.
    pub fn contains(self, interval: Timeframe) -> bool {
        let current = match self {
            Self::All => return true,
            Self::Only(current) => return interval == current,
            Self::AndAbove(current) | Self::AndBelow(current) => current,
        };
        let order = match (interval, current) {
            (Timeframe::Ticks(a), Timeframe::Ticks(b))
            | (Timeframe::Volume(a), Timeframe::Volume(b))
            | (Timeframe::Range(a), Timeframe::Range(b))
            | (Timeframe::Delta(a), Timeframe::Delta(b)) => Some(a.cmp(&b)),
            (a, b) if !a.is_trade_based() && !b.is_trade_based() => {
                Some(a.duration_ms().cmp(&b.duration_ms()))
            }
            _ => None,
        };
        order.is_some_and(|order| match self {
            Self::AndAbove(_) => !order.is_lt(),
            Self::AndBelow(_) => !order.is_gt(),
            _ => false,
        })
    }
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
    /// A range applied in addition to the exact hidden intervals. Older saved settings show
    /// every interval except those in `hidden_intervals`.
    #[serde(default)]
    pub interval_range: StudyIntervalRange,
    /// One style per level of the study's pane, in the order of
    /// [`StudyPane::levels`](super::StudyPane::levels). Settings saved before levels were
    /// editable, and settings with another number of levels, take the study's own levels and fill.
    #[serde(default)]
    pub levels: Vec<LevelStyle>,
    /// Whether the band between the lowest and the highest level is filled.
    #[serde(default)]
    pub fill_between_levels: bool,
    /// Colour of the band between the lowest and the highest level.
    #[serde(default)]
    pub level_fill_color: Color32,
}

/// An owned settings and input snapshot without calculated study data.
/// Used by chart history; restoring it requires recalculation on the current bars.
#[derive(Clone)]
pub struct StudySettingsSnapshot {
    inputs: Vec<(String, InputValue)>,
    settings: StudySettings,
    timeframe: Option<Timeframe>,
    colors: Vec<Color32>,
    visible: bool,
    overlay: Option<bool>,
    scale: Option<super::StudyScale>,
    source: Option<super::StudySource>,
    inherited_overlay: bool,
    inherited_scale: super::StudyScale,
    inherited_source: Option<super::StudySource>,
    calculation_symbol: String,
    calculation_timeframe: Option<Timeframe>,
    wait_for_close: bool,
    offset: i32,
}

impl PartialEq for StudySettingsSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.inputs == other.inputs
            && self.settings == other.settings
            && self.colors == other.colors
            && self.visible == other.visible
            && self.overlay.unwrap_or(self.inherited_overlay)
                == other.overlay.unwrap_or(other.inherited_overlay)
            && self.scale.unwrap_or(self.inherited_scale)
                == other.scale.unwrap_or(other.inherited_scale)
            && self.source.or(self.inherited_source) == other.source.or(other.inherited_source)
            && self.calculation_symbol == other.calculation_symbol
            && self.calculation_timeframe == other.calculation_timeframe
            && self.wait_for_close == other.wait_for_close
            && self.offset == other.offset
    }
}

impl StudySettingsSnapshot {
    /// Capture declared inputs and presentation with the global visibility toggle.
    /// `visible` precedes any interval filtering performed by [`Indicator::is_visible`].
    pub fn capture(study: &(impl Indicator + ?Sized), visible: bool) -> Self {
        Self {
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            timeframe: None,
            colors: study.colors(),
            visible,
            overlay: Some(study.is_overlay()),
            scale: Some(study.scale()),
            source: None,
            inherited_overlay: study.is_overlay(),
            inherited_scale: study.scale(),
            inherited_source: study.source(),
            calculation_symbol: String::new(),
            calculation_timeframe: None,
            wait_for_close: true,
            offset: 0,
        }
    }

    pub(super) fn restore(
        &self,
        study: &mut (impl Indicator + ?Sized),
    ) -> Result<(), &'static str> {
        let current = study.inputs();
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .filter(|(id, value)| {
                !current
                    .iter()
                    .any(|input| input.id == *id && input.value == *value)
            })
            .cloned()
            .collect();
        if !inputs.is_empty() || study.settings() != self.settings {
            study.apply_settings(&inputs, self.settings.clone())?;
        }
        study.set_colors(self.colors.clone());
        study.set_visible(self.visible);
        if let Some(overlay) = self.overlay {
            study.set_overlay(overlay);
        }
        if let Some(scale) = self.scale {
            study.set_scale(scale);
        }
        Ok(())
    }
}

/// A study with editable presentation, used automatically by the chart.
/// Hosts composing their own chart can wrap built-in or custom studies the same way.
#[derive(Clone)]
pub struct ConfiguredStudy {
    study: Box<dyn Indicator>,
    settings: StudySettings,
    timeframe: Option<Timeframe>,
    instrument_precision: Option<u8>,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    overlay: Option<bool>,
    scale: Option<super::StudyScale>,
    scale_mapping: Option<crate::scales::PriceMap>,
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
        match_declared_levels(study.as_ref(), &mut settings);
        Self {
            study,
            settings,
            timeframe: None,
            instrument_precision: None,
            formatter: None,
            overlay: None,
            scale: None,
            scale_mapping: None,
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
    /// Whether the study can be calculated on another interval: it reads one source, or the bars
    /// alone, without data of its own to request or the chart's trades.
    fn runs_on_other_bars(&self) -> bool {
        self.study.source().is_some()
            || (self.study.data_requests().is_empty() && self.study.order_flow_inputs().is_empty())
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
        use super::{InputKind, StudyInput};
        if self.study.source().is_some() {
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
            ]);
        }
        if self.runs_on_other_bars() {
            inputs.extend([
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
        if match id {
            "calculation_symbol" => self.study.source().is_some(),
            "calculation_timeframe" | "wait_for_close" => self.runs_on_other_bars(),
            _ => false,
        } {
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
    fn data_start(
        &self,
        request: &super::StudyDataRequest,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.data_start(request)
        } else {
            None
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
            let values: Vec<_> = (0..self.study.line_count())
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
    fn order_flow_inputs(&self) -> &[crate::data::OrderFlowInput] {
        self.study.order_flow_inputs()
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
    fn set_time_scale_events(&mut self, events: &[crate::model::TimeScaleEvent]) -> bool {
        // A study calculated on another symbol has none of the chart's events.
        self.study
            .set_time_scale_events(if self.calculation_symbol.is_empty() {
                events
            } else {
                &[]
            })
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
    fn set_color(&mut self, line: usize, color: Color32) {
        if let Some(plot) = self.settings.plots.get_mut(line) {
            self.study.set_color(line, color);
            plot.color = self.study.plot_style(line).color;
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
        let mut plot = self.settings.plots.get(line).copied().unwrap_or_else(|| {
            let mut plot = self.study.plot_style(line);
            let output = self.settings.plots.get(self.study.plot_output(line));
            plot.visible &= output.is_none_or(|output| output.visible);
            plot
        });
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
    fn snapshot_settings(&self) -> StudySettingsSnapshot {
        let mut snapshot =
            StudySettingsSnapshot::capture(self.study.as_ref(), self.study.is_visible());
        snapshot.settings = self.settings.clone();
        snapshot.timeframe = self.timeframe;
        snapshot.overlay = self.overlay;
        snapshot.scale = self.scale;
        snapshot.source = self.source;
        snapshot.calculation_symbol = self.calculation_symbol.clone();
        snapshot.calculation_timeframe = self.calculation_timeframe;
        snapshot.wait_for_close = self.wait_for_close;
        snapshot.offset = self.offset;
        snapshot
    }
    fn restore_settings(&mut self, snapshot: &StudySettingsSnapshot) -> Result<(), &'static str> {
        let mut inputs: Vec<_> = snapshot.inputs.iter().collect();
        while !inputs.is_empty() {
            let remaining = inputs.len();
            let mut error = "invalid study input";
            // An input can change another input's accepted range.
            inputs.retain(|(id, value)| {
                if self
                    .study
                    .inputs()
                    .iter()
                    .any(|input| input.id == *id && input.value == *value)
                {
                    return false;
                }
                if let Err(message) = self.study.set_input(id, value.clone()) {
                    error = message;
                    true
                } else {
                    false
                }
            });
            if inputs.len() == remaining {
                return Err(error);
            }
        }
        self.study.set_colors(snapshot.colors.clone());
        self.study.set_visible(snapshot.visible);
        self.study.set_label_precision(snapshot.settings.precision);
        self.settings = snapshot.settings.clone();
        self.timeframe = snapshot.timeframe;
        self.overlay = snapshot.overlay;
        self.scale = snapshot.scale;
        self.scale_mapping = None;
        self.source = snapshot.source;
        self.calculation_symbol = snapshot.calculation_symbol.clone();
        self.calculation_timeframe = snapshot.calculation_timeframe;
        self.wait_for_close = snapshot.wait_for_close;
        self.offset = snapshot.offset;
        self.aligned = None;
        self.aligned_bars.clear();
        Ok(())
    }
    fn settings(&self) -> StudySettings {
        self.settings.clone()
    }
    fn apply_settings(
        &mut self,
        inputs: &[(String, InputValue)],
        mut settings: StudySettings,
    ) -> Result<(), &'static str> {
        if settings.precision.is_some_and(|precision| precision > 12)
            || settings
                .plots
                .iter()
                .any(|plot| !plot.width.is_finite() || !(0.5..=10.0).contains(&plot.width))
            || settings
                .levels
                .iter()
                .any(|level| level.value.is_some_and(|value| !value.is_finite()))
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
        // Styles of the plots before the inputs changed their number keep their place, and
        // plots they add start with the study's own style.
        if settings.plots.len() == self.line_count() && edited.line_count() != self.line_count() {
            let defaults = edited.study.settings().plots;
            let kept = settings.plots.len().min(defaults.len());
            settings.plots.truncate(kept);
            settings.plots.extend_from_slice(&defaults[kept..]);
        }
        if settings.plots.len() != edited.line_count() {
            return Err("one style is required per study plot");
        }
        match_declared_levels(edited.study.as_ref(), &mut settings);
        // Unchanged plot colours must retain any per-bar palette stored by the study.
        for (line, plot) in settings.plots.iter().enumerate() {
            if plot.color != edited.study.plot_style(line).color {
                edited.study.set_color(line, plot.color);
            }
        }
        self.study = edited.study;
        self.study.set_label_precision(settings.precision);
        self.source = edited.source;
        self.calculation_symbol = edited.calculation_symbol;
        self.calculation_timeframe = edited.calculation_timeframe;
        self.wait_for_close = edited.wait_for_close;
        self.offset = edited.offset;
        self.aligned = None;
        self.settings = settings;
        Ok(())
    }
    fn scale_mapping(&self) -> Option<crate::scales::PriceMap> {
        self.scale_mapping
    }
    fn set_scale_mapping(&mut self, mapping: Option<crate::scales::PriceMap>) {
        self.scale_mapping = mapping;
    }
    fn set_value_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) -> bool {
        self.formatter = formatter;
        true
    }
    fn format_value(&self, value: f64) -> String {
        self.formatter
            .as_ref()
            .filter(|_| value.is_finite())
            .map_or_else(
                || self.value_format().format(value, self.precision()),
                |formatter| formatter.format(value),
            )
    }
    fn value_format(&self) -> crate::scales::StudyFormat {
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
                (self.value_format() == crate::scales::StudyFormat::Price)
                    .then_some(self.instrument_precision.map(usize::from))
                    .flatten()
            })
    }
    fn line_count(&self) -> usize {
        self.study.line_count()
    }
    fn line_names(&self) -> Vec<String> {
        self.study.line_names()
    }
    fn projection_fill_color(&self, projection: &super::StudyProjection) -> Color32 {
        self.study.projection_fill_color(projection)
    }
    fn plot_label_colors(&self, line: usize) -> Option<(Color32, Color32)> {
        self.study
            .plot_label_colors(line)
            .map(|(_, text)| (self.plot_style(line).color, text))
    }
    fn marker_style(&self) -> Option<PlotStyle> {
        self.study.marker_style()
    }
    fn markers(&self) -> &[super::IndicatorMarker] {
        self.study.markers()
    }
    fn volume_profiles(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::StudyProfile> {
        self.study.volume_profiles(visible)
    }
    fn zones(&self) -> &[super::StudyZone] {
        self.study.zones()
    }
    fn zones_cover_whole_bars(&self) -> bool {
        self.study.zones_cover_whole_bars()
    }
    fn labels(&self) -> &[super::StudyLabel] {
        // Source bars of a study on another interval or symbol are not chart bars.
        if self.aligned.is_some() {
            &[]
        } else {
            self.study.labels()
        }
    }
    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.study.set_label_precision(precision);
    }
    fn provisional_after(&self, line: usize) -> Option<usize> {
        self.aligned
            .is_none()
            .then(|| self.study.provisional_after(line))
            .flatten()
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
    fn level_style(&self, value: f64) -> LineStyle {
        // Levels moved onto one value draw with the pattern of the first visible one.
        self.study
            .pane()
            .levels
            .iter()
            .zip(&self.settings.levels)
            .find(|((declared, ..), style)| {
                style.visible && style.value.unwrap_or(*declared) == value
            })
            .map_or_else(|| self.study.level_style(value), |(_, style)| style.pattern)
    }
    fn level_fill_color(&self) -> Color32 {
        self.settings.level_fill_color
    }
    fn pane(&self) -> super::StudyPane {
        let mut pane = self.study.pane();
        for ((value, color, _), style) in pane.levels.iter_mut().zip(&self.settings.levels) {
            *value = style.value.unwrap_or(*value);
            // A hidden level still bounds the fill; its transparent line is not drawn.
            *color = if style.visible {
                style.color
            } else {
                Color32::TRANSPARENT
            };
        }
        pane.fill_between_levels = self.settings.fill_between_levels;
        pane
    }
    fn is_overlay(&self) -> bool {
        self.overlay.unwrap_or_else(|| self.study.is_overlay())
    }
    fn set_overlay(&mut self, overlay: bool) -> bool {
        self.overlay = Some(overlay);
        true
    }
    fn set_scale(&mut self, scale: super::StudyScale) -> bool {
        self.scale_mapping = None;
        self.scale = Some(scale);
        true
    }
    fn is_visible(&self) -> bool {
        self.study.is_visible()
            && self.timeframe.is_none_or(|tf| {
                !self.settings.hidden_intervals.contains(&tf)
                    && self.settings.interval_range.contains(tf)
            })
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
                        .with_value_format(crate::scales::StudyFormat::Fixed(4)),
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

    /// Net Volume starts as a line; PPO and PVO keep columns for their differences.
    /// A saved plot type replaces each default.
    #[test]
    fn histogram_defaults_yield_to_a_saved_plot_type() {
        use super::super::{NetVolume, PercentageVolumeOscillator, PriceOscillator};
        let (line, columns) = (PlotType::Line, PlotType::Columns);
        let studies: [(Box<dyn Indicator>, &[PlotType]); 3] = [
            (Box::new(NetVolume::default()), &[line]),
            (Box::new(PriceOscillator::default()), &[line, line, columns]),
            (
                Box::new(PercentageVolumeOscillator::default()),
                &[line, line, columns],
            ),
        ];
        for (study, expected) in studies {
            let name = study.name().to_owned();
            let mut study = ConfiguredStudy::new(study);
            let types = |study: &ConfiguredStudy| -> Vec<_> {
                study.settings().plots.iter().map(|p| p.plot_type).collect()
            };
            assert_eq!(types(&study), expected, "{name}");
            let mut settings = study.settings();
            settings.plots[0].plot_type = PlotType::Area;
            study.apply_settings(&[], settings).unwrap();
            assert_eq!(study.plot_style(0).plot_type, PlotType::Area, "{name}");
        }
    }

    /// Studies that read the bars alone offer the interval and close-wait inputs as well as the
    /// single-source studies, keep their own inputs, and calculate on the interval they are given.
    /// A study that requests data of its own, or reads trades, does not.
    #[test]
    fn ohlc_studies_offer_a_calculation_interval() {
        use super::super::{ADX, ATR};
        let ids = |study: &ConfiguredStudy| -> Vec<String> {
            study.inputs().into_iter().map(|input| input.id).collect()
        };
        for inner in [
            Box::new(ATR::new(3)) as Box<dyn Indicator>,
            Box::new(ADX::new(3)),
        ] {
            let mut study = ConfiguredStudy::new(inner);
            let own = study.study.inputs().len();
            let ids = ids(&study);
            assert_eq!(
                &ids[own..],
                ["calculation_timeframe", "wait_for_close"],
                "{}",
                study.name()
            );
            assert!(study.data_requests().is_empty());
            study
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Hour1)),
                )
                .unwrap();
            assert_eq!(study.data_requests().len(), 1);
            assert!(
                study
                    .set_input("calculation_symbol", InputValue::Symbol("X".into()))
                    .is_err()
            );
        }
        let breadth = ConfiguredStudy::new(Box::new(super::super::AdvanceDeclineRatio::default()));
        assert!(!ids(&breadth).contains(&"calculation_timeframe".to_owned()));
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
        for (range, interval, visible) in [
            (
                StudyIntervalRange::AndAbove(Timeframe::Min5),
                Timeframe::Custom(std::num::NonZeroU64::new(120).unwrap()),
                false,
            ),
            (
                StudyIntervalRange::AndAbove(Timeframe::Min5),
                Timeframe::Custom(std::num::NonZeroU64::new(420).unwrap()),
                true,
            ),
            (
                StudyIntervalRange::AndBelow(Timeframe::Ticks(
                    std::num::NonZeroU64::new(100).unwrap(),
                )),
                Timeframe::Ticks(std::num::NonZeroU64::new(50).unwrap()),
                true,
            ),
            (
                StudyIntervalRange::AndBelow(Timeframe::Ticks(
                    std::num::NonZeroU64::new(100).unwrap(),
                )),
                Timeframe::Volume(std::num::NonZeroU64::new(50).unwrap()),
                false,
            ),
        ] {
            let mut settings = study.settings();
            settings.interval_range = range;
            study.apply_settings(&[], settings).unwrap();
            study.set_visible(true);
            study.set_timeframe(interval);
            assert_eq!(study.is_visible(), visible);
            study.set_visible(false);
            assert!(!study.is_visible());
        }
        let saved = serde_json::to_string(&study.settings()).unwrap();
        let restored: StudySettings = serde_json::from_str(&saved).unwrap();
        assert!(!restored.plots[0].visible);
        assert_eq!(restored.hidden_intervals, [Timeframe::Min1]);
    }

    #[test]
    fn rsi_levels_round_trip_and_older_settings_take_the_declared_levels() {
        use crate::studies::{AdxBreakout, RSI, palette};
        let values = |study: &ConfiguredStudy| -> Vec<f64> {
            study.pane().levels.iter().map(|level| level.0).collect()
        };
        let mut study = ConfiguredStudy::new(Box::new(RSI::default()));
        let declared = study.settings();
        assert_eq!(
            declared
                .levels
                .iter()
                .map(|l| (l.value, l.color, l.pattern, l.visible))
                .collect::<Vec<_>>(),
            [(None, palette::MUTED, LineStyle::Dashed, true); 2]
        );
        assert!(declared.fill_between_levels);
        assert_eq!(
            declared.level_fill_color,
            Color32::from_rgba_unmultiplied(126, 87, 194, 26)
        );
        let before = study.snapshot_settings();
        let mut edited = declared.clone();
        edited.levels[0].value = Some(80.0);
        edited.levels[1].visible = false;
        study.apply_settings(&[], edited.clone()).unwrap();
        assert_eq!(values(&study), [80.0, 30.0]);
        // Templates, copies and layouts rebuild a fresh study from the saved JSON.
        let saved = serde_json::to_value(&edited).unwrap();
        let mut loaded = ConfiguredStudy::new(Box::new(RSI::default()));
        loaded
            .apply_settings(&[], serde_json::from_value(saved.clone()).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), edited);
        let mut older = saved;
        for key in ["levels", "fill_between_levels", "level_fill_color"] {
            older.as_object_mut().unwrap().remove(key);
        }
        loaded
            .apply_settings(&[], serde_json::from_value(older).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), declared);
        study.restore_settings(&before).unwrap();
        assert_eq!(values(&study), [70.0, 30.0]);
        let mut invalid = declared.clone();
        invalid.levels[0].value = Some(f64::INFINITY);
        assert_eq!(
            study.apply_settings(&[], invalid),
            Err("invalid study presentation settings")
        );
        assert_eq!(study.settings(), declared);
        let mut adx = ConfiguredStudy::new(Box::new(AdxBreakout::default()));
        let settings = adx.settings();
        adx.apply_settings(&[("threshold".into(), InputValue::Length(25))], settings)
            .unwrap();
        assert_eq!(values(&adx), [25.0]);
    }
}
