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

/// Colour and thickness for an alternate state of a plot, such as decreasing bars.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct PlotVariant {
    /// Caption in the style editor.
    pub name: String,
    /// Colour in this state.
    pub color: Color32,
    /// Stroke width in logical pixels, from 0.5 to 10.
    pub width: f32,
}

impl PlotVariant {
    /// An alternate state with a one-pixel stroke.
    pub fn new(name: impl Into<String>, color: Color32) -> Self {
        Self {
            name: name.into(),
            color,
            width: 1.0,
        }
    }
}

/// Presentation of one declared band, independent of its boundary plots.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FillStyle {
    /// Colour when the first boundary is at least the second.
    pub color: Color32,
    /// Optional colour when the first boundary is below the second.
    pub inverted_color: Option<Color32>,
    /// Whether the band is drawn.
    pub visible: bool,
}

impl FillStyle {
    /// A visible band with the same colour on either side of a crossing.
    pub fn new(color: Color32) -> Self {
        Self {
            color,
            inverted_color: None,
            visible: true,
        }
    }
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
    /// Stroke width in logical pixels, from 0.5 to 10. Older settings use one pixel.
    #[serde(default = "default_level_width")]
    pub width: f32,
    /// Whether the line is drawn. A hidden boundary still bounds the fill.
    pub visible: bool,
}

fn default_level_width() -> f32 {
    1.0
}

/// The levels and fill a study declares, as the defaults of its level settings.
pub(super) fn declared_levels<S: Indicator + ?Sized>(
    study: &S,
) -> (Vec<LevelStyle>, bool, Color32) {
    let pane = study.pane();
    let levels = pane
        .levels
        .iter()
        .enumerate()
        .map(|(index, &(value, color, _))| {
            let (pattern, width) = study.level_stroke(index, value);
            LevelStyle {
                value: None,
                color,
                pattern,
                width,
                visible: true,
            }
        })
        .collect();
    (levels, pane.fill_between_levels, study.level_fill_color())
}

/// Missing style lists inherit the study's declared presentation.
fn match_declared_styles(study: &dyn Indicator, settings: &mut StudySettings) {
    let (levels, fill, color) = declared_levels(study);
    if study.name() == "Stochastic" && settings.levels.len() == 2 && levels.len() == 3 {
        // Keep the two saved guides and the old appearance when adding the middle guide.
        let mut middle = levels[2];
        middle.visible = false;
        settings.levels.push(middle);
    } else if settings.levels.len() != levels.len() {
        settings.levels = levels;
        settings.fill_between_levels = fill;
        settings.level_fill_color = color;
    }
    settings
        .plot_variants
        .resize_with(study.line_count(), Vec::new);
    for (line, variants) in settings.plot_variants.iter_mut().enumerate() {
        let declared = study.plot_variants(line);
        if variants.len() != declared.len() {
            *variants = declared;
            // Saved before this plot had alternate states, all its bars used the plot's width.
            if let Some(plot) = settings.plots.get(line) {
                for variant in variants.iter_mut() {
                    variant.width = plot.width;
                }
            }
        }
    }
    // Saved band styles are kept whatever the study declares so far, since a study can declare
    // a band only once it has data. Bands beyond them start with the study's own style.
    let fills = study.fills();
    let saved = settings.fills.len().min(fills.len());
    settings
        .fills
        .extend(fills[saved..].iter().map(|fill| FillStyle {
            color: fill.color,
            inverted_color: fill.inverted_color,
            visible: true,
        }));
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
    /// Show values in the status line when the chart enables study values. Defaults to true.
    #[serde(default = "enabled_by_default")]
    pub show_legend_values: bool,
    /// Allow user edits through settings, legend and context controls. Host APIs remain writable.
    #[serde(default = "enabled_by_default")]
    pub user_edit_enabled: bool,
    /// Help article requested through the host. None uses the underlying study's metadata.
    #[serde(default)]
    pub help_url: Option<String>,
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
    /// Whether the band between the boundary levels is filled.
    /// [`Indicator::level_fill_indices`] selects them; otherwise the outer levels are used.
    #[serde(default)]
    pub fill_between_levels: bool,
    /// Colour of the band between the boundary levels.
    #[serde(default)]
    pub level_fill_color: Color32,
    /// Plots, as indices into [`plots`](Self::plots), whose last value has no tag on the price
    /// scale while the chart shows study values; the legend still shows it. A study starts
    /// with the plots its [`Indicator::plot_tagged`] leaves out, every plot of a study with more
    /// than four. An empty list, as in settings saved before this field, tags every plot.
    /// Applying settings drops indices without a plot and sorts the rest.
    #[serde(default)]
    pub untagged_plots: Vec<usize>,
    /// Outputs with a horizontal line at their latest value, independent of tags and displacement.
    /// Missing latest values remove the line. Empty, including older settings, disables all.
    #[serde(default)]
    pub price_line_plots: Vec<usize>,
    /// Alternate colour and width states per output. Missing states take declared defaults.
    #[serde(default)]
    pub plot_variants: Vec<Vec<PlotVariant>>,
    /// Optional tint across the study's plot area, independent of its plots and bands.
    #[serde(default)]
    pub background: Option<Color32>,
    /// Whether the optional pane tint is drawn. Hiding it retains its colour.
    #[serde(default = "enabled_by_default")]
    pub background_visible: bool,
    /// One independent style per band returned by [`Indicator::fills`].
    /// Missing styles take the study's declared colours and visibility. Styles of bands the study
    /// does not declare yet are kept for when it does.
    #[serde(default)]
    pub fills: Vec<FillStyle>,
}

fn enabled_by_default() -> bool {
    true
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
    ) -> Result<(), crate::studies::StudyError> {
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
        // A palette recolours the bands of a wrapped study, and the saved band styles win.
        if study.settings().fills != self.settings.fills {
            study.apply_settings(&[], self.settings.clone())?;
        }
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
///
/// Calculated on another interval or symbol, a study's volume profiles and zones span the chart
/// bars that share time with their bars, its markers sit on the last such chart bar once the
/// values on the chart reach their bar, and its labels are hidden.
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
    /// For each chart bar, the bars of another interval or symbol that share its time, by which
    /// the study's profiles, zones and markers are placed on chart bars.
    spans: Vec<std::ops::Range<usize>>,
    /// The study's markers and zones on chart bars while it runs on another interval or symbol.
    aligned_markers: Vec<super::IndicatorMarker>,
    aligned_zones: Vec<super::StudyZone>,
    /// The values kept on the bars that remain after the chart evicted the oldest bars, until
    /// the study is calculated again.
    kept: Option<Kept>,
}

/// The values a study keeps on the bars that remain after the chart evicted the oldest `shift`
/// bars since its last calculation.
#[derive(Clone)]
struct Kept {
    values: IndicatorValues,
    shift: usize,
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
        match_declared_styles(study.as_ref(), &mut settings);
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
            spans: Vec::new(),
            aligned_markers: Vec::new(),
            aligned_zones: Vec::new(),
            kept: None,
        }
    }
    /// Attach a host-chosen help article to this study, retained in saved presentation.
    pub fn with_help_url(mut self, url: impl Into<String>) -> Self {
        self.settings.help_url = Some(url.into());
        self
    }

    /// Whether the study can be calculated on another interval: it reads one source, or the bars
    /// alone, without data of its own to request or order flow it cannot do without.
    fn runs_on_other_bars(&self) -> bool {
        self.study.source().is_some()
            || (self.study.data_requests().is_empty() && !self.study.needs_order_flow())
    }
    /// The bars evicted since the study was last calculated, by which the bars of its kept values
    /// trail those of the study.
    fn shift(&self) -> usize {
        self.kept.as_ref().map_or(0, |kept| kept.shift)
    }
    fn update_price(&mut self, data: &[Bar], from: usize) {
        self.aligned = None;
        self.kept = None;
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

    fn user_edit_enabled(&self) -> bool {
        self.settings.user_edit_enabled
    }
    fn help_url(&self) -> Option<&str> {
        self.settings
            .help_url
            .as_deref()
            .or_else(|| self.study.help_url())
            .filter(|url| !url.trim().is_empty())
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
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        if id == "plot_offset" && self.study.source().is_some() {
            let InputValue::Integer(offset) = value else {
                return Err(crate::studies::StudyError::invalid_input(id));
            };
            self.offset = offset;
            self.aligned = None;
            self.kept = None;
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
                .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
            if !schema.accepts(&value) {
                return Err(crate::studies::StudyError::invalid_input(id));
            }
            if matches!(self.source(), Some(super::StudySource::Study { .. }))
                && !matches!(&value, InputValue::Symbol(s) if s.is_empty())
                && value != InputValue::Timeframe(None)
                && id != "wait_for_close"
            {
                return Err(super::StudyError::InvalidInput {
                    id: id.into(),
                    reason: "must follow the chart while the source is another study's output"
                        .into(),
                });
            }
            match value {
                InputValue::Symbol(value) => self.calculation_symbol = value,
                InputValue::Timeframe(value) => self.calculation_timeframe = value,
                InputValue::Toggle(value) => self.wait_for_close = value,
                _ => unreachable!(),
            }
            // The study missed the chart's interval and clock while it ran on other bars.
            if id != "wait_for_close"
                && self.calculation_symbol.is_empty()
                && self.calculation_timeframe.is_none()
                && let Some(timeframe) = self.timeframe
            {
                self.study.set_timeframe(timeframe);
                self.study.set_bar_clock(&self.clock);
            }
            self.aligned = None;
            self.kept = None;
            self.study.calculate(&[]);
            return Ok(());
        }
        if id == "source" && self.study.source().is_some() {
            if matches!(value, InputValue::Choice(_)) {
                self.study.set_input(id, value)?;
                self.source = None;
                self.aligned = None;
                self.kept = None;
                return Ok(());
            }
            let InputValue::Source(source) = value else {
                return Err(crate::studies::StudyError::invalid_input(id));
            };
            if matches!(source, super::StudySource::Study { .. })
                && (!self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some())
            {
                return Err(super::StudyError::InvalidInput {
                    id: id.into(),
                    reason: "cannot be another study's output while the study runs on another symbol or interval".into(),
                });
            }
            self.source = Some(source);
            self.aligned = None;
            self.kept = None;
            self.study.calculate(&[]);
            Ok(())
        } else {
            let format = self.study.value_format();
            self.study.set_input(id, value)?;
            if self.study.value_format() != format {
                self.formatter = None;
            }
            self.aligned = None;
            self.kept = None;
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
        self.kept = None;
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
            self.spans.clear();
            self.aligned_markers.clear();
            self.aligned_zones.clear();
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
        self.spans = spans_of(data, &self.clock, source);
        // A marker appears once the values on the chart reach its bar (Wait for interval close).
        let shown = self.aligned_bars.iter().flatten().max().copied();
        self.aligned_markers = self
            .study
            .markers()
            .iter()
            .filter(|marker| shown >= Some(marker.bar))
            .filter_map(|marker| {
                let bars = chart_bars(&self.spans, &(marker.bar..marker.bar.saturating_add(1)))?;
                Some(super::IndicatorMarker {
                    bar: bars.end - 1,
                    ..*marker
                })
            })
            .collect();
        self.aligned_zones = self
            .study
            .zones()
            .iter()
            .filter_map(|zone| {
                Some(super::StudyZone {
                    bars: chart_bars(&self.spans, &zone.bars)?,
                    ..zone.clone()
                })
            })
            .collect();
    }
    fn order_flow_inputs(&self) -> &[crate::data::OrderFlowInput] {
        self.study.order_flow_inputs()
    }
    fn needs_order_flow(&self) -> bool {
        self.study.needs_order_flow()
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
        self.kept = None;
        self.study.update_source(data, from, source);
    }
    fn is_incremental(&self) -> bool {
        self.data_requests().is_empty()
            && self.source() == self.study.source()
            && self.study.is_incremental()
    }
    fn remove_oldest_bars(&mut self, count: usize) -> bool {
        // These outputs index the evicted bars too.
        let indexed = !self.candles().is_empty()
            || !self.zones().is_empty()
            || !self.markers().is_empty()
            || !self.labels().is_empty()
            || !self.projections().is_empty()
            || (0..self.line_count()).any(|line| self.provisional_after(line).is_some());
        #[cfg(feature = "drawings")]
        let indexed = indexed || !self.drawings().is_empty();
        if indexed || self.values().len() <= count {
            return false;
        }
        let shift = self.shift() + count;
        let values = self.values().without_first(count);
        self.kept = Some(Kept { values, shift });
        true
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
        self.kept
            .as_ref()
            .map(|kept| &kept.values)
            .or(self.aligned.as_ref())
            .unwrap_or_else(|| self.study.values())
    }
    fn colors(&self) -> Vec<Color32> {
        self.settings.plots.iter().map(|plot| plot.color).collect()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        self.study.set_colors(colors);
        for (plot, color) in self.settings.plots.iter_mut().zip(self.study.colors()) {
            plot.color = color;
        }
        for (line, variants) in self.settings.plot_variants.iter_mut().enumerate() {
            for (variant, declared) in variants.iter_mut().zip(self.study.plot_variants(line)) {
                variant.color = declared.color;
            }
        }
        for (style, declared) in self.settings.fills.iter_mut().zip(self.study.fills()) {
            style.color = declared.color;
            style.inverted_color = declared.inverted_color;
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
    fn plot_tagged(&self, line: usize) -> bool {
        if line < self.settings.plots.len() {
            !self.settings.untagged_plots.contains(&line)
        } else {
            self.study.plot_tagged(line)
        }
    }
    fn plot_price_line(&self, line: usize) -> bool {
        self.settings.price_line_plots.contains(&line)
    }
    fn plot_variants(&self, line: usize) -> Vec<PlotVariant> {
        self.settings
            .plot_variants
            .get(line)
            .cloned()
            .unwrap_or_default()
    }
    fn plot_variant(&self, line: usize, bar: usize) -> Option<usize> {
        let bar = bar.checked_add(self.shift())?;
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_variant(line, bar)
    }
    fn plot_width(&self, line: usize, bar: usize) -> f32 {
        self.plot_variant(line, bar)
            .and_then(|variant| self.settings.plot_variants.get(line)?.get(variant))
            .map_or_else(|| self.plot_style(line).width, |variant| variant.width)
    }
    fn background(&self) -> Option<Color32> {
        self.settings
            .background
            .filter(|_| self.settings.background_visible)
    }
    fn plot_offset(&self, line: usize) -> isize {
        self.study
            .plot_offset(line)
            .saturating_add(self.offset as isize)
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        let kept = self.kept.as_ref().map(|kept| &kept.values);
        if let Some(values) = kept.or(self.aligned.as_ref()) {
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
        if let Some(variant) = self
            .plot_variant(line, bar)
            .and_then(|variant| self.settings.plot_variants.get(line)?.get(variant))
        {
            return Some(variant.color);
        }
        let bar = bar.checked_add(self.shift())?;
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_color(line, bar)
    }
    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let bar = bar.checked_add(self.shift())?;
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_text(line, bar)
    }
    fn fills(&self) -> Vec<super::StudyFill> {
        self.study
            .fills()
            .into_iter()
            .enumerate()
            .filter_map(|(index, mut fill)| {
                if let Some(style) = self.settings.fills.get(index) {
                    if !style.visible {
                        return None;
                    }
                    fill.color = style.color;
                    fill.inverted_color = style.inverted_color;
                }
                Some(fill)
            })
            .collect()
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
    fn restore_settings(
        &mut self,
        snapshot: &StudySettingsSnapshot,
    ) -> Result<(), crate::studies::StudyError> {
        let mut inputs: Vec<_> = snapshot.inputs.iter().collect();
        while !inputs.is_empty() {
            let remaining = inputs.len();
            let mut error = None;
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
                    error = Some(message);
                    true
                } else {
                    false
                }
            });
            if inputs.len() == remaining {
                return Err(error.expect("a retained input failed to apply"));
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
        self.kept = None;
        self.aligned_bars.clear();
        Ok(())
    }
    fn settings(&self) -> StudySettings {
        let mut settings = self.settings.clone();
        // A study can declare a band after its settings were last applied.
        match_declared_styles(self.study.as_ref(), &mut settings);
        settings
    }
    fn apply_settings(
        &mut self,
        inputs: &[(String, InputValue)],
        mut settings: StudySettings,
    ) -> Result<(), crate::studies::StudyError> {
        if let Some(precision) = settings.precision.filter(|precision| *precision > 12) {
            return Err(super::StudyError::InvalidPrecision(precision));
        }
        if let Some((plot, width)) = settings
            .plots
            .iter()
            .enumerate()
            .map(|(plot, style)| (plot, style.width))
            .chain(
                settings
                    .plot_variants
                    .iter()
                    .enumerate()
                    .flat_map(|(plot, variants)| {
                        variants.iter().map(move |variant| (plot, variant.width))
                    }),
            )
            .find(|(_, width)| !width.is_finite() || !(0.5..=10.0).contains(width))
        {
            return Err(super::StudyError::InvalidPlotWidth { plot, width });
        }
        if let Some((level, style)) = settings
            .levels
            .iter()
            .enumerate()
            .find(|(_, style)| !style.width.is_finite() || !(0.5..=10.0).contains(&style.width))
        {
            return Err(super::StudyError::InvalidLevelWidth {
                level,
                width: style.width,
            });
        }
        if let Some((level, value)) =
            settings
                .levels
                .iter()
                .enumerate()
                .find_map(|(level, style)| {
                    style
                        .value
                        .filter(|value| !value.is_finite())
                        .map(|value| (level, value))
                })
        {
            return Err(super::StudyError::InvalidLevelValue { level, value });
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
        // Styles and tags of the plots before the inputs changed their number keep their place,
        // and plots they add start with the study's own style and tag.
        if settings.plots.len() == self.line_count() && edited.line_count() != self.line_count() {
            let defaults = edited.study.settings();
            let kept = settings.plots.len().min(defaults.plots.len());
            settings.plots.truncate(kept);
            settings.plots.extend_from_slice(&defaults.plots[kept..]);
            settings.untagged_plots.retain(|&line| line < kept);
            settings
                .untagged_plots
                .extend(defaults.untagged_plots.iter().filter(|&&line| line >= kept));
        }
        if settings.plots.len() != edited.line_count() {
            return Err(super::StudyError::PlotCount {
                expected: edited.line_count(),
                found: settings.plots.len(),
            });
        }
        settings
            .untagged_plots
            .retain(|&line| line < settings.plots.len());
        settings.untagged_plots.sort_unstable();
        settings.untagged_plots.dedup();
        settings
            .price_line_plots
            .retain(|&line| line < settings.plots.len());
        settings.price_line_plots.sort_unstable();
        settings.price_line_plots.dedup();
        // Unchanged plot colours must retain any per-bar palette stored by the study.
        for (line, plot) in settings.plots.iter().enumerate() {
            if plot.color != edited.study.plot_style(line).color {
                edited.study.set_color(line, plot.color);
            }
        }
        // Bands these inputs remove take their styles with them. A study left with none keeps
        // them, since it may declare its bands only once it has data and these inputs can
        // recalculate it without bars.
        let bands = edited.study.fills().len();
        if bands > 0 && bands < self.study.fills().len() {
            settings.fills.truncate(bands);
        }
        match_declared_styles(edited.study.as_ref(), &mut settings);
        if self.study.value_format() != edited.study.value_format() {
            self.formatter = None;
        }
        self.study = edited.study;
        self.study.set_label_precision(settings.precision);
        self.source = edited.source;
        self.calculation_symbol = edited.calculation_symbol;
        self.calculation_timeframe = edited.calculation_timeframe;
        self.wait_for_close = edited.wait_for_close;
        self.offset = edited.offset;
        self.aligned = None;
        self.kept = None;
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
                || {
                    if self.precision() == self.study.precision() {
                        self.study.format_value(value)
                    } else {
                        self.value_format().format(value, self.precision())
                    }
                },
                |formatter| formatter.format(value),
            )
    }
    fn format_plot_value(&self, line: usize, value: f64) -> String {
        if self.formatter.is_some() || self.precision() != self.study.precision() {
            self.format_value(value)
        } else {
            self.study.format_plot_value(line, value)
        }
    }
    fn legend_lines(&self) -> Vec<usize> {
        self.study.legend_lines()
    }
    fn legend_visible(&self, line: usize) -> bool {
        let (shown, own) = (
            self.study.legend_visible(line),
            self.study.plot_style(line).visible,
        );
        // The study may add a readout for a hidden plot or withhold one for a drawn plot.
        self.settings.show_legend_values
            && (self.plot_style(line).visible || (shown && !own))
            && (shown || !own)
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
        if self.aligned.is_some() {
            &self.aligned_markers
        } else {
            self.study.markers()
        }
    }
    fn volume_profiles(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::StudyProfile> {
        // Kept profiles would index the evicted bars too.
        if self.kept.is_some() {
            return Vec::new();
        }
        if self.aligned.is_none() {
            return self.study.volume_profiles(visible);
        }
        // Source bars of a study on another interval or symbol are not chart bars.
        self.study
            .volume_profiles(source_bars(&self.spans, &visible))
            .into_iter()
            .filter_map(|mut plot| {
                plot.bars = chart_bars(&self.spans, &plot.bars)?;
                Some(plot)
            })
            .collect()
    }
    fn profile_levels(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::ProfileLevel> {
        // Kept levels would index the evicted bars too.
        if self.kept.is_some() {
            return Vec::new();
        }
        if self.aligned.is_none() {
            return self.study.profile_levels(visible);
        }
        // Source bars of a study on another interval or symbol are not chart bars.
        self.study
            .profile_levels(source_bars(&self.spans, &visible))
            .into_iter()
            .filter_map(|mut level| {
                level.bars = chart_bars(&self.spans, &level.bars)?;
                Some(level)
            })
            .collect()
    }
    fn zones(&self) -> &[super::StudyZone] {
        if self.aligned.is_some() {
            &self.aligned_zones
        } else {
            self.study.zones()
        }
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
    fn labels_on_price(&self) -> bool {
        self.study.labels_on_price()
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
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        self.study.level_fill_indices()
    }
    fn level_stroke(&self, index: usize, value: f64) -> (LineStyle, f32) {
        self.settings
            .levels
            .get(index)
            .filter(|style| {
                style
                    .value
                    .or_else(|| self.study.pane().levels.get(index).map(|level| level.0))
                    == Some(value)
            })
            .map_or_else(
                || (self.level_style(value), self.level_width(value)),
                |style| (style.pattern, style.width),
            )
    }
    fn level_fill_color(&self) -> Color32 {
        self.settings.level_fill_color
    }
    fn level_width(&self, value: f64) -> f32 {
        self.study
            .pane()
            .levels
            .iter()
            .zip(&self.settings.levels)
            .find(|((declared, ..), style)| {
                style.visible && style.value.unwrap_or(*declared) == value
            })
            .map_or_else(|| self.study.level_width(value), |(_, style)| style.width)
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

/// For each of the chart's bars `data`, opened by `clock`, the bars of `source` that share its
/// time: those that open before it closes and close after it opens. A bar of an interval not
/// measured in time lasts until the next one opens, and the last one without end.
fn spans_of(
    data: &[Bar],
    clock: &crate::model::BarClock,
    source: &super::StudyData<'_>,
) -> Vec<std::ops::Range<usize>> {
    let close = |by: &crate::model::BarClock, bars: &[Bar], at: usize| {
        by.next_open(by.bar_open(bars[at].time))
            .or_else(|| bars.get(at + 1).map(|bar| bar.time))
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC)
    };
    let (mut first, mut end) = (0, 0);
    data.iter()
        .enumerate()
        .map(|(at, bar)| {
            let (open, until) = (clock.bar_open(bar.time), close(clock, data, at));
            while first < source.bars.len() && close(&source.clock, source.bars, first) <= open {
                first += 1;
            }
            end = end.max(first);
            while end < source.bars.len() && source.clock.bar_open(source.bars[end].time) < until {
                end += 1;
            }
            first..end
        })
        .collect()
}

/// The chart bars that share time with the source bars `bars`, from the `spans` of each chart
/// bar; `None` when no chart bar does.
fn chart_bars(
    spans: &[std::ops::Range<usize>],
    bars: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let start = spans.partition_point(|span| span.end <= bars.start);
    let end = spans.partition_point(|span| span.start < bars.end);
    (bars.start < bars.end && start < end).then_some(start..end)
}

/// The source bars that share time with the chart bars `bars`: from the first source bar of the
/// first chart bar to the last of the last, from the `spans` of each chart bar.
fn source_bars(
    spans: &[std::ops::Range<usize>],
    bars: &std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let end = bars.end.min(spans.len());
    if bars.start >= end {
        return 0..0;
    }
    spans[bars.start].start..spans[end - 1].end
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::studies::BollingerBands;

    /// Guards #2112: clearing the calculation interval hands the study the chart's interval,
    /// whether the chart passed it before the input was set, as a template does, or moved on
    /// while it was set, and whether the edit comes directly or from the settings dialog.
    #[test]
    fn clearing_the_calculation_interval_restores_the_chart_interval() {
        let daily = super::super::StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(Timeframe::Day1),
            metric: None,
        };
        for (added_on, dialog) in [
            (None, false),
            (None, true),
            (Some(Timeframe::Day1), false),
            (Some(Timeframe::Day1), true),
        ] {
            let mut study = ConfiguredStudy::new(Box::new(super::super::Performance::new()));
            if let Some(timeframe) = added_on {
                study.set_timeframe(timeframe);
            }
            study
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Day1)),
                )
                .unwrap();
            study.set_timeframe(Timeframe::Min1);
            let cleared = (
                "calculation_timeframe".to_owned(),
                InputValue::Timeframe(None),
            );
            if dialog {
                let settings = study.settings();
                study.apply_settings(&[cleared], settings).unwrap();
            } else {
                study.set_input(&cleared.0, cleared.1).unwrap();
            }
            assert_eq!(
                study.data_requests(),
                std::slice::from_ref(&daily),
                "added on {added_on:?}, dialog {dialog}"
            );
        }
    }

    #[test]
    fn value_tags_default_by_plot_count_and_persist() {
        use crate::studies::{
            CustomIndicator, IndicatorValue, MARibbon, StudyTemplate, VolumeWeightedAvgPrice,
        };
        for (lines, untagged) in [(4, vec![]), (5, vec![0, 1, 2, 3, 4])] {
            let study = ConfiguredStudy::new(Box::new(
                CustomIndicator::new("Lines", |bars| vec![IndicatorValue::None; bars.len()])
                    .with_line_count(lines),
            ));
            assert_eq!(study.settings().untagged_plots, untagged, "{lines} plots");
        }

        let mut ribbon = ConfiguredStudy::new(Box::new(MARibbon::new()));
        assert_eq!(
            ribbon.settings().untagged_plots,
            (0..12).collect::<Vec<_>>()
        );
        let mut settings = ribbon.settings();
        settings.untagged_plots = (1..11).collect();
        ribbon.apply_settings(&[], settings).unwrap();
        let template = StudyTemplate::from_study(&ribbon);
        let json = serde_json::to_string(&template).unwrap();
        let restored = serde_json::from_str::<StudyTemplate>(&json)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            restored.settings().untagged_plots,
            (1..11).collect::<Vec<_>>()
        );
        assert!(restored.plot_tagged(0) && restored.plot_tagged(11) && !restored.plot_tagged(5));
        // Settings saved before the field existed tag every plot, as they did.
        let mut old = serde_json::to_value(&template).unwrap();
        old["settings"]
            .as_object_mut()
            .unwrap()
            .remove("untagged_plots");
        let restored = serde_json::from_value::<StudyTemplate>(old)
            .unwrap()
            .create()
            .unwrap();
        assert!((0..12).all(|line| restored.plot_tagged(line)));

        // Two bands take VWAP from one plot to five: the kept VWAP line stays tagged.
        let mut vwap = ConfiguredStudy::new(Box::new(VolumeWeightedAvgPrice::new()));
        let settings = vwap.settings();
        vwap.apply_settings(&[("band_count".into(), InputValue::Length(2))], settings)
            .unwrap();
        assert_eq!(vwap.settings().untagged_plots, [1, 2, 3, 4]);
        let mut settings = vwap.settings();
        settings.untagged_plots = vec![3, 1, 3, 7];
        vwap.apply_settings(&[], settings).unwrap();
        assert_eq!(vwap.settings().untagged_plots, [1, 3]);
    }

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

    /// Volume profiles, zones and markers of a study calculated on a finer or a coarser interval
    /// lie on the chart bars that share time with their bars, a marker once the values on the chart
    /// reach its bar, and none are left when the other history is missing.
    #[test]
    fn profiles_zones_and_markers_of_another_interval_lie_on_their_chart_bars() {
        use super::super::{
            CrossDirection, IndicatorMarker, StudyData, StudyZone, VolProfileSession,
            VolProfileVisible,
        };
        // Markers on the second and the last bar, and a zone from the one to the other.
        #[derive(Clone, Default)]
        struct Marks {
            values: IndicatorValues,
            markers: Vec<IndicatorMarker>,
            zones: Vec<StudyZone>,
        }
        impl Indicator for Marks {
            fn name(&self) -> &str {
                "Marks"
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.values = IndicatorValues::missing(data.len());
                let last = data.len().saturating_sub(1);
                let marker = |bar| IndicatorMarker {
                    bar,
                    value: 100.0,
                    direction: CrossDirection::Up,
                };
                let zone = StudyZone {
                    bars: 1..last + 1,
                    top: 101.0,
                    bottom: 100.0,
                    plot: 0,
                };
                (self.markers, self.zones) = if last > 1 {
                    (vec![marker(1), marker(last)], vec![zone])
                } else {
                    (Vec::new(), Vec::new())
                };
            }
            fn values(&self) -> &IndicatorValues {
                &self.values
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::WHITE]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn set_visible(&mut self, _: bool) {}
            fn markers(&self) -> &[IndicatorMarker] {
                &self.markers
            }
            fn zones(&self) -> &[StudyZone] {
                &self.zones
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
        }
        // Bar i opens i intervals after 23:50 UTC on 6 March, so midnight starts a second
        // session, and trades a volume of 1 from 100 + i to 101 + i.
        let start: chrono::DateTime<chrono::Utc> = "2026-03-06T23:50:00Z".parse().unwrap();
        let bars = |count: i32, interval: Timeframe| -> Vec<Bar> {
            (0..count)
                .map(|i| {
                    let low = 100.0 + f64::from(i);
                    let time = start + interval.duration() * i;
                    Bar::new(time, low, low + 1.0, low, low + 1.0, 1.0)
                })
                .collect()
        };
        let profiles = |study: &ConfiguredStudy, visible| -> Vec<_> {
            study
                .volume_profiles(visible)
                .into_iter()
                .map(|plot| {
                    let rows = &plot.profile;
                    (plot.bars, rows.low(), rows.high(), rows.volume().to_vec())
                })
                .collect()
        };
        for (name, [chart, other], sessions, (view, visible), markers, zones, levels) in [
            (
                "finer",
                [(4, Timeframe::Min5), (20, Timeframe::Min1)],
                [
                    (0..2, 100.0, 110.0, vec![10.0]),
                    (2..4, 110.0, 120.0, vec![10.0]),
                ],
                (1..3, (1..3, 105.0, 115.0, vec![10.0])),
                vec![0, 3],
                0..4,
                (2..4, 105.0),
            ),
            (
                "coarser",
                [(20, Timeframe::Min1), (4, Timeframe::Min5)],
                [
                    (0..10, 100.0, 102.0, vec![2.0]),
                    (10..20, 102.0, 104.0, vec![2.0]),
                ],
                (3..12, (0..15, 100.0, 103.0, vec![3.0])),
                vec![9],
                5..20,
                (10..20, 101.0),
            ),
        ] {
            let (interval, source_interval) = (chart.1, other.1);
            let (chart, source) = (bars(chart.0, interval), bars(other.0, source_interval));
            let calculated = |inner: Box<dyn Indicator>| {
                let mut study = ConfiguredStudy::new(inner);
                study.set_timeframe(interval);
                let other = InputValue::Timeframe(Some(source_interval));
                study.set_input("calculation_timeframe", other).unwrap();
                let data = StudyData {
                    request: study.data_requests().remove(0),
                    bars: &source,
                    clock: crate::model::BarClock::new(source_interval),
                    instrument: None,
                    timeframe: source_interval,
                };
                study.update_data(&chart, 0, &[data]);
                study
            };
            let mut session = VolProfileSession::new(1);
            session
                .set_input("extend_poc", InputValue::Toggle(true))
                .unwrap();
            let mut studies = [
                calculated(Box::new(session)),
                calculated(Box::new(VolProfileVisible::new(1))),
                calculated(Box::<Marks>::default()),
            ];
            assert_eq!(profiles(&studies[0], 0..chart.len()), sessions, "{name}");
            let extended: Vec<_> = studies[0]
                .profile_levels(0..chart.len())
                .into_iter()
                .map(|level| (level.bars, level.price))
                .collect();
            assert_eq!(extended, [levels], "{name}");
            assert_eq!(profiles(&studies[1], view), [visible], "{name}");
            let marked: Vec<_> = studies[2]
                .markers()
                .iter()
                .map(|marker| marker.bar)
                .collect();
            assert_eq!(marked, markers, "{name}");
            let zone = StudyZone {
                bars: zones,
                top: 101.0,
                bottom: 100.0,
                plot: 0,
            };
            assert_eq!(studies[2].zones(), [zone], "{name}");
            for study in &mut studies {
                study.update_data(&chart, 0, &[]);
            }
            for study in &studies {
                assert!(profiles(study, 0..chart.len()).is_empty(), "{name}");
                assert!(study.profile_levels(0..chart.len()).is_empty(), "{name}");
                assert!(
                    study.markers().is_empty() && study.zones().is_empty(),
                    "{name}"
                );
            }
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
            [(None, palette::MUTED, LineStyle::Dashed, true); 3]
        );
        assert!(declared.fill_between_levels);
        assert_eq!(
            declared.level_fill_color,
            Color32::from_rgba_unmultiplied(126, 87, 194, 26)
        );
        let before = study.snapshot_settings();
        let mut edited = declared.clone();
        edited.levels[0].value = Some(80.0);
        edited.levels[0].width = 3.0;
        edited.levels[1].visible = false;
        study.apply_settings(&[], edited.clone()).unwrap();
        assert_eq!(values(&study), [80.0, 30.0, 50.0]);
        // Templates, copies and layouts rebuild a fresh study from the saved JSON.
        let saved = serde_json::to_value(&edited).unwrap();
        let mut loaded = ConfiguredStudy::new(Box::new(RSI::default()));
        loaded
            .apply_settings(&[], serde_json::from_value(saved.clone()).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), edited);
        let mut older = saved;
        for level in older["levels"].as_array_mut().unwrap() {
            level.as_object_mut().unwrap().remove("width");
        }
        loaded
            .apply_settings(&[], serde_json::from_value(older.clone()).unwrap())
            .unwrap();
        assert_eq!(loaded.settings().levels[0].width, 1.0);
        for key in ["levels", "fill_between_levels", "level_fill_color"] {
            older.as_object_mut().unwrap().remove(key);
        }
        loaded
            .apply_settings(&[], serde_json::from_value(older).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), declared);
        study.restore_settings(&before).unwrap();
        assert_eq!(values(&study), [70.0, 30.0, 50.0]);
        let mut invalid = declared.clone();
        invalid.levels[0].value = Some(f64::INFINITY);
        assert!(matches!(
            study.apply_settings(&[], invalid),
            Err(super::super::StudyError::InvalidLevelValue { level: 0, value }) if value == f64::INFINITY
        ));
        assert_eq!(study.settings(), declared);
        for width in [f32::NAN, f32::INFINITY, 0.0, 11.0] {
            let mut invalid = declared.clone();
            invalid.levels[0].width = width;
            assert!(matches!(
                study.apply_settings(&[], invalid),
                Err(super::super::StudyError::InvalidLevelWidth { level: 0, .. })
            ));
            assert_eq!(study.settings(), declared);
        }
        // Adding a middle guide keeps the two saved Stochastic guides and their band.
        for name in ["Stochastic", "StochRSI"] {
            let mut study = ConfiguredStudy::new(
                crate::studies::IndicatorFactory::new()
                    .create(name)
                    .unwrap(),
            );
            let defaults = study.settings();
            assert_eq!(values(&study), [80.0, 20.0, 50.0]);
            assert!(defaults.fill_between_levels);
            assert_eq!(study.level_fill_indices(), Some([0, 1]));
            assert_eq!(
                defaults.level_fill_color,
                palette::LIGHT_BLUE.gamma_multiply_u8(26)
            );
            let mut older = defaults.clone();
            older.levels.truncate(2);
            older.levels[0].value = Some(85.0);
            older.levels[0].width = 4.0;
            older.levels[1].visible = false;
            older.fill_between_levels = false;
            study.apply_settings(&[], older.clone()).unwrap();
            if name == "Stochastic" {
                assert_eq!(&study.settings().levels[..2], older.levels);
                assert!(!study.settings().levels[2].visible);
                assert!(!study.pane().fill_between_levels);
            }
            let mut absent = defaults.clone();
            absent.levels.clear();
            study.apply_settings(&[], absent).unwrap();
            assert_eq!(study.settings(), defaults);
        }
        let mut adx = ConfiguredStudy::new(Box::new(AdxBreakout::default()));
        let settings = adx.settings();
        adx.apply_settings(&[("threshold".into(), InputValue::Length(25))], settings)
            .unwrap();
        assert_eq!(values(&adx), [25.0]);
    }

    /// A palette set on a chart study recolours its band, as it recolours its plots.
    #[test]
    fn set_colors_recolours_the_band() {
        let band = Color32::from_rgba_unmultiplied(1, 2, 3, 40);
        let palette = vec![Color32::RED, Color32::GREEN, Color32::BLUE, band];
        let mut own = BollingerBands::default();
        own.set_colors(palette.clone());
        // Bollinger Bands fill the band in the fourth palette colour.
        assert_eq!(own.fills()[0].color, band);
        let mut study = ConfiguredStudy::new(Box::new(BollingerBands::default()));
        study.set_colors(palette);
        assert_eq!(study.plot_style(0).color, Color32::RED);
        assert_eq!(study.fills()[0].color, band);

        // A hidden band stays hidden through a palette, and a template keeps its colour.
        let mut settings = study.settings();
        settings.fills[0].visible = false;
        study.apply_settings(&[], settings).unwrap();
        study.set_colors(vec![Color32::RED, Color32::GREEN, Color32::BLUE, band]);
        assert!(study.fills().is_empty());
        let restored = crate::studies::StudyTemplate::from_study(&study)
            .create()
            .unwrap();
        assert_eq!(restored.settings().fills[0].color, band);
        assert!(restored.fills().is_empty());

        // Restoring a snapshot through the trait reapplies the palette after the saved styles,
        // which keep a band colour of their own.
        let own = Color32::from_rgba_unmultiplied(9, 8, 7, 60);
        let mut settings = study.settings();
        settings.fills[0] = FillStyle::new(own);
        study.apply_settings(&[], settings).unwrap();
        study.snapshot_settings().restore(&mut study).unwrap();
        assert_eq!(study.fills()[0].color, own);
    }
}
