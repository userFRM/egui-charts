//! Core indicator trait and value types.
//!
//! This module defines the [`Indicator`] trait that every technical indicator
//! (built-in or custom) must implement, as well as the [`IndicatorValue`]
//! row format used to construct or export column-wise output.

use crate::model::Bar;
use crate::studies::IndicatorValues;
use egui::Color32;

/// The output value of an indicator at a single bar position.
///
/// An owned row for constructing or exporting [`IndicatorValues`](super::IndicatorValues).
/// Studies store columns; this row format is also accepted by custom calculation closures.
/// The variant depends on the indicator:
///
/// - **`Single(f64)`** -- used by single-line indicators (SMA, EMA, RSI, ATR, ...).
/// - **`Multiple(Vec<f64>)`** -- used by multi-line indicators, one value per
///   line in the order of [`line_names`](Indicator::line_names). For example,
///   MACD emits `[macd_line, signal_line, histogram]` and Bollinger Bands
///   emits `[upper, middle, lower]`. A line without a value on the bar, such
///   as a signal line during its warmup, is NaN.
/// - **`None`** -- emitted for bars where no line has a value: within the
///   indicator's warmup period, or on a bar whose inputs are not finite.
///
/// An indicator with more than one line emits `Multiple` with
/// [`line_count`](Indicator::line_count) values or `None`, never `Single`.
#[derive(Debug, Clone)]
pub enum IndicatorValue {
    /// A single computed value (e.g. SMA output at one bar).
    Single(f64),
    /// One value per line for multi-line indicators (e.g. MACD producing
    /// MACD line, signal line, and histogram), NaN for a line without one.
    Multiple(Vec<f64>),
    /// No value on this bar: warmup, or inputs that are not finite.
    None,
}

/// Direction of a crossing marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossDirection {
    /// First line crosses above the second.
    Up,
    /// First line crosses below the second.
    Down,
}

/// A shape at a study value on a source bar.
#[derive(Clone, Copy, Debug)]
pub struct IndicatorMarker {
    /// Index in the study's input bars.
    pub bar: usize,
    /// Price at which the crossing is marked.
    pub value: f64,
    /// Crossing direction.
    pub direction: CrossDirection,
}

/// A vertical fade between an overlay line and the plot's bottom edge.
#[derive(Clone, Copy, Debug)]
pub struct IndicatorFill {
    /// Fill colour; alpha is supplied separately at each end of the gradient.
    pub color: Color32,
    /// Alpha just under the line, from 0 to 255.
    pub top_alpha: u8,
    /// Alpha at the plot's bottom edge, from 0 to 255.
    pub bottom_alpha: u8,
}

/// Width, optional direction colours and optional bottom fill of an overlay line.
#[derive(Clone, Copy, Debug)]
pub struct IndicatorLineStyle {
    /// Stroke width in logical pixels.
    pub width: f32,
    /// Colour on rising segments, or the study's line colour when absent.
    pub rising: Option<Color32>,
    /// Colour on falling segments, or the study's line colour when absent.
    pub falling: Option<Color32>,
    /// Fill from the line to the plot's bottom.
    pub fill: Option<IndicatorFill>,
}

impl Default for IndicatorLineStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            rising: None,
            falling: None,
            fill: None,
        }
    }
}

/// Default layout and reference levels supplied by a study for its separate pane.
#[derive(Clone, Debug, Default)]
pub struct StudyPane {
    /// Preferred initial height in logical pixels; `None` shares the available space.
    pub height: Option<f32>,
    /// Fixed initial range, or automatic fitting to visible values.
    pub range: Option<(f64, f64)>,
    /// Reference level, colour and caption: the defaults of
    /// [`StudySettings::levels`](super::StudySettings::levels).
    pub levels: Vec<(f64, Color32, &'static str)>,
    /// Fill between the outer levels using [`Indicator::level_fill_color`].
    pub fill_between_levels: bool,
}

/// The core trait that every technical indicator must implement.
///
/// The trait defines the full indicator lifecycle: naming, calculation,
/// value access, color management, and display properties. It is
/// object-safe so indicators can be stored as `Box<dyn Indicator>` in
/// the [`IndicatorRegistry`](super::IndicatorRegistry).
///
/// # Implementors
///
/// All built-in indicators implement this trait (e.g. `SMA`, `RSI`, `MACD` with the
/// `studies` feature), and users can implement it
/// for their own indicators or use [`CustomIndicator`](super::CustomIndicator)
/// to define one at runtime via a closure.
///
/// # Lifecycle
///
/// 1. Construct the indicator with desired parameters. Built-in indicators
///    raise a length (period, smoothing, level or row count) of 0 to 1, and
///    clamp a float parameter (multiplier, percentage, divisor, threshold or
///    tick size) into the range its constructor or setter documents, NaN to
///    the bottom of that range. No length, parameter or bar value makes
///    `calculate` panic or allocate without bound.
/// 2. Call [`calculate`](Indicator::calculate) with bar data.
/// 3. Read results via [`values`](Indicator::values).
/// 4. The rendering layer queries [`is_overlay`](Indicator::is_overlay),
///    [`colors`](Indicator::colors), [`line_names`](Indicator::line_names),
///    etc. for drawing.
///
/// # Non-finite bars
///
/// A bar whose inputs, the fields a study reads, are not finite has no value, and the
/// study computes every other bar as if that bar were not in the series, so running
/// state carries over it unchanged. A window study may also leave the windows that
/// contain it without a value. The chart's datafeed drops such bars before they reach a
/// study, so this matters to direct callers of [`calculate`](Indicator::calculate).
pub trait Indicator: Send {
    /// Sparse projected price bands, with fractional endpoints in future whitespace.
    fn projections(&self) -> &[super::StudyProjection] {
        &[]
    }

    /// A labelled table of the latest study statistics, drawn independently of the plots.
    fn table(&self) -> Option<&super::StudyTable> {
        None
    }

    /// Whether this study stores presentation edits itself. Charts wrap other studies once.
    fn has_settings(&self) -> bool {
        false
    }

    /// Colour inside a projected price band.
    fn projection_fill_color(&self, projection: &super::StudyProjection) -> Color32 {
        self.plot_color(projection.plot, projection.bars.start as usize)
            .unwrap_or_else(|| self.plot_style(projection.plot).color)
            .gamma_multiply(0.2)
    }

    /// Optional background and text colours for captions on plotted markers.
    fn plot_label_colors(&self, _line: usize) -> Option<(Color32, Color32)> {
        None
    }

    /// Optional presentation of crossing markers; absent uses directional triangles.
    fn marker_style(&self) -> Option<super::PlotStyle> {
        None
    }

    /// Stroke pattern for a pane level at the given value.
    fn level_style(&self, _value: f64) -> crate::model::LineStyle {
        crate::model::LineStyle::Dashed
    }

    /// Colour of the band between the outer pane levels.
    fn level_fill_color(&self) -> Color32 {
        let color = self.plot_style(0).color;
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 16)
    }

    /// Presentation defaults for a zero-based output line. Any line is accepted: one at or past
    /// [`line_count`](Self::line_count), which renderers do not draw, still gets a style, so an
    /// implementation must not panic on it. The default takes the line's colour from
    /// [`colors`](Self::colors), white for a line without one, and its width from
    /// [`line_style`](Self::line_style).
    fn plot_style(&self, line: usize) -> super::PlotStyle {
        super::PlotStyle {
            color: self.colors().get(line).copied().unwrap_or(Color32::WHITE),
            width: self.line_style(line).width,
            pattern: crate::model::LineStyle::Solid,
            plot_type: super::PlotType::Line,
            visible: true,
        }
    }

    /// The output whose visibility in the style tab also applies to plot `line`, for a plot past
    /// [`line_count`](Self::line_count) that draws zones or candles for one of the outputs. The
    /// default is `line` itself.
    fn plot_output(&self, line: usize) -> usize {
        line
    }

    /// Horizontal displacement in bars. Positive values draw into future whitespace.
    /// Values remain indexed by their source bar; missing bars still occupy a position.
    fn plot_offset(&self, line: usize) -> isize {
        let _ = line;
        0
    }

    /// Optional per-bar colour, overriding the plot colour at a source bar.
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let _ = (line, bar);
        None
    }

    /// The last source bar whose value on a zero-based line is final. The segments after it can
    /// still move with later bars and are drawn dashed when the line is solid. `None`, the
    /// default, draws the whole line as styled.
    fn provisional_after(&self, line: usize) -> Option<usize> {
        let _ = line;
        None
    }

    /// Optional text beside the shape a circle, cross or triangle plot draws at a source bar:
    /// under an upward triangle and over the other shapes, in the shape's colour.
    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let _ = (line, bar);
        None
    }

    /// Filled bands, painted beneath this study's plots. Missing endpoints break a band.
    fn fills(&self) -> Vec<super::StudyFill> {
        Vec::new()
    }

    /// Value at a displayed bar, accounting for the plot's displacement.
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        let source = bar.checked_add_signed(self.plot_offset(line).checked_neg()?)?;
        self.values().value(line, source)
    }

    /// Current applied presentation, initialized from the study's own defaults.
    fn settings(&self) -> super::StudySettings {
        let (levels, fill_between_levels, level_fill_color) =
            super::settings::declared_levels(self);
        super::StudySettings {
            plots: (0..self.line_count())
                .map(|line| self.plot_style(line))
                .collect(),
            precision: self.precision(),
            hidden_intervals: Vec::new(),
            interval_range: Default::default(),
            levels,
            fill_between_levels,
            level_fill_color,
        }
    }

    /// Capture editable inputs and presentation without copying calculated values.
    /// The chart presentation wrapper also retains visibility before interval filtering.
    /// Custom presentations can use [`super::StudySettingsSnapshot::capture`] with their
    /// global visibility toggle to do the same.
    fn snapshot_settings(&self) -> super::StudySettingsSnapshot {
        super::StudySettingsSnapshot::capture(self, self.is_visible())
    }

    /// Restore a snapshot from this study. Recalculate against current data afterwards.
    /// The default applies changed inputs and presentation through [`Self::apply_settings`].
    /// Returns an error if the snapshot is no longer accepted.
    fn restore_settings(
        &mut self,
        snapshot: &super::StudySettingsSnapshot,
    ) -> Result<(), &'static str> {
        snapshot.restore(self)
    }

    /// Apply a complete edit atomically. Charts wrap studies in [`ConfiguredStudy`](super::ConfiguredStudy),
    /// which validates inputs on a clone before replacing the calculation and presentation.
    fn apply_settings(
        &mut self,
        inputs: &[(String, super::InputValue)],
        settings: super::StudySettings,
    ) -> Result<(), &'static str> {
        let _ = (inputs, settings);
        Err("wrap the study in ConfiguredStudy to edit its presentation")
    }

    /// Visible source bars, end exclusive, supplied after pan, zoom and resize and before
    /// autoscaling. Returns whether values need recalculation. Direct callers may supply it too.
    fn set_visible_range(&mut self, range: std::ops::Range<usize>) -> bool {
        let _ = range;
        false
    }

    /// The chart's current interval, supplied when added and when the interval changes.
    fn set_timeframe(&mut self, timeframe: crate::model::Timeframe) {
        let _ = timeframe;
    }

    /// Source bar calendar, including the chart's selected timezone and session anchor.
    /// Returns whether cached values must be recalculated. Direct callers default to UTC.
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        let _ = clock;
        false
    }

    /// Units of this study's values. Placement defaults preserve price overlays and numeric panes.
    fn value_format(&self) -> crate::scales::StudyFormat {
        if self.is_volume() {
            crate::scales::StudyFormat::Volume
        } else if self.is_overlay() {
            crate::scales::StudyFormat::Price
        } else {
            crate::scales::StudyFormat::Fixed(2)
        }
    }

    /// Scale used in the pane selected by `is_overlay`.
    fn scale(&self) -> super::StudyScale {
        if self.is_overlay() && !self.is_volume() {
            super::StudyScale::Main
        } else {
            super::StudyScale::Independent
        }
    }

    /// Price mapping supplied by the chart for a shared scale, independent of pane bounds.
    /// Custom presentation implementations can retain it to support shared scales.
    fn scale_mapping(&self) -> Option<crate::scales::PriceMap> {
        None
    }

    /// Updates the chart's shared-scale mapping. `None` restores the ordinary pane mapping.
    fn set_scale_mapping(&mut self, mapping: Option<crate::scales::PriceMap>) {
        let _ = mapping;
    }

    /// Replaces value formatting when supported by the presentation. `None` restores the
    /// declared units and precision. Returns false when this study owns its formatting.
    fn set_value_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) -> bool {
        let _ = formatter;
        false
    }

    /// Format a value using the declared units and current precision override.
    fn format_value(&self, value: f64) -> String {
        self.value_format().format(value, self.precision())
    }

    /// Decimal places for study readouts, or the format's default precision.
    fn precision(&self) -> Option<usize> {
        match self.value_format() {
            crate::scales::StudyFormat::Fixed(decimals) => Some(decimals.min(12)),
            _ => None,
        }
    }

    /// Additional histories this study needs. The chart resolves defaults, requests history
    /// and live updates from its feed, and supplies them before recalculating the study.
    fn data_requests(&self) -> Vec<super::StudyDataRequest> {
        Vec::new()
    }

    /// The oldest time this study reads from `request`, one of its
    /// [`data_requests`](Indicator::data_requests). A connected feed stops loading older
    /// history of that series once it holds a bar at or before the earliest such time of the
    /// studies reading it, and loads more when that time moves earlier. `None`, the default,
    /// and a time before the chart's first bar load the series back to that bar. Price-based
    /// chart types such as Renko do not use the times and always load it back to that bar.
    fn data_start(
        &self,
        request: &super::StudyDataRequest,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let _ = request;
        None
    }

    /// Update with requested histories. Studies using only chart bars keep the default.
    /// Pass 0 when a requested history changes; nonzero `from` retains the same requested
    /// histories and follows [`update`](Indicator::update)'s unchanged-prefix contract.
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[super::StudyData<'_>]) {
        let _ = sources;
        self.update(data, from);
    }

    /// The single price series this study reads, or `None` for OHLC or multi-source studies.
    /// Configured studies expose this as a Source input. Study references use chart ids and
    /// undisplaced output columns; missing or cyclic references produce missing values.
    fn source(&self) -> Option<super::StudySource> {
        None
    }

    /// Update using an aligned source column. Missing source values remain gaps.
    /// The default substitutes the source for OHLC and retains the bar's other fields.
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        let bars: Vec<_> = data
            .iter()
            .enumerate()
            .map(|(at, bar)| {
                let value = source.get(at).copied().unwrap_or(f64::NAN);
                Bar {
                    open: value,
                    high: value,
                    low: value,
                    close: value,
                    ..bar.clone()
                }
            })
            .collect();
        self.update(&bars, from);
    }

    /// Current editable inputs. A host study can supply the same schema as built-ins.
    fn inputs(&self) -> Vec<super::StudyInput> {
        Vec::new()
    }

    /// Change a calculation input, rejecting unknown ids, wrong types and invalid values.
    /// On success cached values are cleared; calculate again or let the chart refresh.
    fn set_input(&mut self, id: &str, value: super::InputValue) -> Result<(), &'static str> {
        let _ = (id, value);
        Err("study has no editable inputs")
    }

    /// Return the short display name of this indicator (e.g. `"SMA"`, `"RSI"`, `"MACD"`).
    fn name(&self) -> &str;

    /// Return a human-readable description of what this indicator measures.
    ///
    /// Defaults to an empty string. Override to provide a tooltip-friendly
    /// summary such as `"Relative Strength Index - Momentum oscillator (0-100)"`.
    fn desc(&self) -> &str {
        ""
    }

    /// Compute the indicator's output from the given bar data.
    ///
    /// After this call, [`values`](Indicator::values) returns one
    /// [`IndicatorValue`] per input bar. Implementations should clear
    /// any previous state so the indicator can be recalculated on new data.
    fn calculate(&mut self, data: &[Bar]);

    /// Optional source for exact stream studies, connected before feed updates are coalesced.
    /// Hosts supplying their own events can push them through the returned handle.
    fn order_flow_input(&self) -> Option<crate::data::OrderFlowInput> {
        None
    }

    /// Bring [`values`](Indicator::values) up to date after `data[from..]` changed or grew,
    /// given that `data[..from]` is unchanged since the previous call. The values equal those of
    /// [`calculate`](Indicator::calculate) on the same data, bit for bit. After bars are
    /// prepended, or after a setting of the study changed, pass 0; after bars are removed from
    /// the end pass the new length.
    ///
    /// The default recomputes everything.
    fn update(&mut self, data: &[Bar], from: usize) {
        let _ = from;
        self.calculate(data);
    }

    /// Whether live updates avoid recomputing unaffected history. Causal studies do work
    /// independent of the series length; studies that redraw earlier legs or periods also
    /// revisit the bars whose output changes. Earlier corrections may recompute in full.
    /// Defaults to `false`.
    fn is_incremental(&self) -> bool {
        false
    }

    /// Apply the chart instrument's time zone and trading session. Returns `true`
    /// if calculation settings changed and the caller must recalculate from bar 0.
    /// The chart calls this on add and before calculation after its symbol or metadata changes.
    /// `None` clears chart metadata and restores the study's own settings.
    /// Studies independent of instrument metadata keep the default.
    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        let _ = instrument;
        false
    }

    /// The typed events the chart holds for its symbol, those given to
    /// `Chart::set_time_scale_events`, in the order supplied. Returns `true` if calculation
    /// settings changed and the caller must recalculate from bar 0. The chart calls this on add
    /// and before calculation. Studies that read no events keep the default.
    fn set_time_scale_events(&mut self, events: &[crate::model::TimeScaleEvent]) -> bool {
        let _ = events;
        false
    }

    /// Return the computed output columns, with NaN for missing values.
    ///
    /// The length of the returned output matches the length of the `data`
    /// slice most recently passed to [`calculate`](Indicator::calculate).
    fn values(&self) -> &IndicatorValues;

    /// Rendering style of an overlay line. Defaults to a 1 px line in its configured colour.
    /// Any line is accepted, as for [`plot_style`](Self::plot_style).
    fn line_style(&self, line: usize) -> IndicatorLineStyle {
        let _ = line;
        IndicatorLineStyle::default()
    }

    /// Candle bodies and wicks, rendered in place of line plots when nonempty.
    /// Source ranges and OHLCV remain available to hosts for per-bar readouts.
    fn candles(&self) -> &[super::StudyCandle] {
        &[]
    }

    /// Drawing geometry generated by this study, styled as plot 0, empty by default.
    /// Calculations belong in `calculate`; use tools whose renderer needs only their anchors
    /// and settings, such as trend lines, parallel channels, Fibonacci levels and pitchforks.
    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        &[]
    }

    /// Market Profile settings of a study whose session profiles the chart builds with
    /// [`to_tpo_profiles`](crate::model::to_tpo_profiles) and draws on the price pane, beside
    /// each session's bars, as it does for the TPO chart type. `None` by default.
    fn tpo(&self) -> Option<&crate::model::TPOConfig> {
        None
    }

    /// Price bands painted beneath this study's plots, stored per zone rather than per bar.
    /// Empty by default.
    fn zones(&self) -> &[super::StudyZone] {
        &[]
    }

    /// Whether each of [`zones`](Self::zones) spans its bars from the left edge of the first to
    /// the right edge of the last, as study candles do, rather than from the centre of the first
    /// to the centre of the last. Off by default.
    fn zones_cover_whole_bars(&self) -> bool {
        false
    }

    /// Text beside study values, in order of `bar`. The chart draws the labels near the visible
    /// bars, in the colour of their plot, inside the plot area, so they never run under the
    /// price axis. Empty by default. Keep the text from the last calculation: it is drawn every
    /// frame.
    fn labels(&self) -> &[super::StudyLabel] {
        &[]
    }

    /// The decimal places the study's presentation settings set for the prices in
    /// [`labels`](Self::labels), `None` when they set none and the instrument's decimals apply.
    /// The text is written when the study calculates, so calculate again afterwards, as a chart
    /// does after any settings edit. Studies without price labels keep the default.
    fn set_label_precision(&mut self, precision: Option<usize>) {
        let _ = precision;
    }

    /// Price-row volume profiles intersecting a half-open visible source-bar range.
    /// Fixed and calendar profiles include their whole selected span, even offscreen bars.
    /// Visible-range studies recompute for this range; ordinary studies return no profiles.
    fn volume_profiles(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::StudyProfile> {
        let _ = visible;
        Vec::new()
    }

    /// Shapes attached to study values, empty by default.
    fn markers(&self) -> &[IndicatorMarker] {
        &[]
    }

    /// Extend the last two values of each overlay line to the plot's right edge.
    fn extend_right(&self) -> bool {
        false
    }

    /// An annotation beside the latest value of a zero-based overlay line.
    fn plot_label(&self) -> Option<(usize, String)> {
        None
    }

    /// Return the plot colours and any extra palette entries used by fills or per-bar colouring.
    ///
    /// Extra palette entries are saved with the study and do not add output columns.
    /// Their layout is specific to the study.
    fn colors(&self) -> Vec<Color32>;

    /// Replace all line colors at once.
    ///
    /// For single-line indicators, pass a `Vec` with one color.
    /// For multi-line indicators, pass one color per line.
    fn set_colors(&mut self, colors: Vec<Color32>);

    /// Set the color of a single line by its zero-based index.
    ///
    /// This is a convenience wrapper around [`colors`](Indicator::colors)
    /// and [`set_colors`](Indicator::set_colors). If `line_idx` is out of
    /// range the call is silently ignored.
    fn set_color(&mut self, line_idx: usize, color: Color32) {
        let mut colors = self.colors();
        if line_idx < colors.len() {
            colors[line_idx] = color;
            self.set_colors(colors);
        }
    }

    /// Whether this indicator is drawn on the main price chart (overlay)
    /// or in a separate sub-pane.
    ///
    /// Returns `true` for overlays (e.g. SMA, Bollinger Bands, Ichimoku),
    /// `false` for oscillators and volume indicators that need their own
    /// Y-axis (e.g. RSI, MACD, OBV). Defaults to `true`.
    fn is_overlay(&self) -> bool {
        true
    }

    /// Default separate-pane layout and reference levels, independent of the study name.
    /// [`ConfiguredStudy`](super::ConfiguredStudy) returns its levels where its settings place
    /// them, in their colours, and a hidden level in `Color32::TRANSPARENT`, which still bounds
    /// the fill.
    fn pane(&self) -> StudyPane {
        StudyPane::default()
    }

    /// Changes placement between the main chart and a separate pane.
    /// Returns whether this study supports the change. The chart's presentation wrapper does.
    fn set_overlay(&mut self, _overlay: bool) -> bool {
        false
    }

    /// Changes the [`scale`](Self::scale) used in the study's pane.
    /// Returns whether this study supports the change. The chart's presentation wrapper does.
    fn set_scale(&mut self, _scale: super::StudyScale) -> bool {
        false
    }

    /// Volume plots use an independent zero-based scale: up columns, down columns and MA.
    /// Renderers size columns to 80% of the bar pitch and format values as volume.
    fn is_volume(&self) -> bool {
        false
    }

    /// The number of output columns this indicator plots. Renderers draw lines
    /// `0..line_count()`.
    ///
    /// For example, SMA returns `1`, MACD returns `3` (MACD line, signal,
    /// histogram), and Ichimoku returns `5`. Defaults to the number of
    /// [`line_names`](Indicator::line_names).
    fn line_count(&self) -> usize {
        self.line_names().len()
    }

    /// Return display names for each line, used in chart legends.
    ///
    /// The length of the returned `Vec` must match [`line_count`](Indicator::line_count).
    /// Defaults to a single entry containing [`name`](Indicator::name).
    fn line_names(&self) -> Vec<String> {
        vec![self.name().to_string()]
    }

    /// Whether this indicator should currently be rendered.
    ///
    /// Defaults to `true`. Toggle with [`set_visible`](Indicator::set_visible).
    fn is_visible(&self) -> bool {
        true
    }

    /// Show or hide this indicator.
    fn set_visible(&mut self, visible: bool);

    /// Clone this indicator into a new `Box<dyn Indicator>`.
    ///
    /// This enables cloning of heterogeneous indicator collections.
    fn clone_box(&self) -> Box<dyn Indicator>;
}

/// Blanket `Clone` implementation for boxed indicators, delegating to
/// [`Indicator::clone_box`].
impl Clone for Box<dyn Indicator> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
