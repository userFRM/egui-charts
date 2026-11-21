//! Core indicator trait and value types.
//!
//! This module defines the [`Indicator`] trait that every technical indicator
//! (built-in or custom) must implement, as well as the [`IndicatorValue`]
//! row format used to construct or export column-wise output.

use crate::model::Bar;
use crate::studies::IndicatorValues;
use egui::Color32;

// All existing Indicator implementations (including borrowed and unsized ones) receive
// this private supertrait. Only chart-owned 'static instances are inspected. Erasing the
// *conversion function*, rather than coercing an arbitrary ?Sized self into dyn Any, keeps
// that blanket implementation unconstrained. No study can override its concrete identity.
pub(crate) trait ConcreteStudy {
    #[cfg(all(feature = "widget", feature = "studies"))]
    fn concrete_study(&self, cast: &dyn std::any::Any) -> Option<&dyn std::any::Any>
    where
        Self: 'static;
}

impl<T: ?Sized> ConcreteStudy for T {
    #[cfg(all(feature = "widget", feature = "studies"))]
    fn concrete_study(&self, cast: &dyn std::any::Any) -> Option<&dyn std::any::Any>
    where
        Self: 'static,
    {
        cast.downcast_ref::<fn(&T) -> &dyn std::any::Any>()
            .map(|cast| cast(self))
    }
}

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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct StudyPane {
    /// Preferred initial height in logical pixels; `None` shares the available space.
    pub height: Option<f32>,
    /// Fixed initial range, or automatic fitting to visible values.
    pub range: Option<(f64, f64)>,
    /// Reference level, colour and caption: the defaults of
    /// [`StudySettings::levels`](super::StudySettings::levels).
    pub levels: Vec<(f64, Color32, &'static str)>,
    /// Fill between the levels selected by [`Indicator::level_fill_indices`], or the outer
    /// levels, using [`Indicator::level_fill_color`].
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
///
/// # Presentation hooks
///
/// The optional hooks return records a study builds from their defaults or variants:
///
/// ```
/// use ecolor::Color32;
/// use egui_charts::studies::{
///     FillBoundary, IndicatorLineStyle, StudyCell, StudyFill, StudyPane, StudySettings,
///     StudyTable,
/// };
///
/// // `line_style`
/// let mut style = IndicatorLineStyle::default();
/// style.width = 2.0;
/// // `pane`
/// let mut pane = StudyPane::default();
/// pane.range = Some((0.0, 100.0));
/// // `settings`
/// let mut settings = StudySettings::default();
/// settings.precision = Some(1);
/// // `table`
/// let mut table = StudyTable::default();
/// table.title = "Momentum".into();
/// table.rows = vec![vec![StudyCell::Text("RSI".into()), StudyCell::Percent(Some(0.42))]];
/// table.cell_colors = vec![vec![Color32::from_rgb(91, 156, 246)]];
/// // `fills`
/// let fill = StudyFill {
///     first: FillBoundary::Plot(0),
///     second: FillBoundary::Level(50.0),
///     color: Color32::from_rgba_unmultiplied(41, 98, 255, 40),
///     inverted_color: None,
/// };
/// ```
#[expect(
    private_bounds,
    reason = "blanket-implemented for every type; not a host extension point"
)]
pub trait Indicator: Send + ConcreteStudy {
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

    /// Whether users may edit this study. Host setters and restore operations remain available.
    fn user_edit_enabled(&self) -> bool {
        true
    }

    /// Optional study help article, opened by the host from the legend or context menu.
    /// Built-ins default to their API article; custom studies can supply a host URL.
    fn help_url(&self) -> Option<&str> {
        #[cfg(feature = "studies")]
        {
            super::factory::builtin_help_url(std::any::type_name::<Self>())
        }
        #[cfg(not(feature = "studies"))]
        {
            None
        }
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

    /// Stroke width of a pane reference level in logical pixels.
    fn level_width(&self, _value: f64) -> f32 {
        1.0
    }

    /// Colour of an enabled dotted value line at a pane guide. Off by default, independent of
    /// plot value tags and guide visibility. Hidden guides retain their value-line colour.
    fn level_price_line(&self, _index: usize) -> Option<Color32> {
        None
    }

    /// Pattern and width for a level by its position in the pane's levels. The value is the
    /// drawn value, including a host's zone override. Defaults to the value-based methods.
    fn level_stroke(&self, _index: usize, value: f64) -> (crate::model::LineStyle, f32) {
        (self.level_style(value), self.level_width(value))
    }

    /// Optional tint beneath the study's geometry over its plot area.
    fn background(&self) -> Option<Color32> {
        None
    }

    /// Indices into the pane's levels that bound its fill, independent of other guides.
    /// `None` uses the lowest and highest levels. Invalid indices contribute no boundary.
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        None
    }

    /// Colour of the band between the selected pane levels.
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

    /// Whether output `line` appears in the legend. Defaults to plot visibility; a study can
    /// retain a labelled readout for a quantity excluded from its plot and scale by default.
    fn legend_visible(&self, line: usize) -> bool {
        self.plot_style(line).visible
    }

    /// Whether output `line` is a readout that the legend lists and the chart never draws, such
    /// as a ratio far from the prices it would share a scale with. Its visibility in the style
    /// tab, which starts from [`plot_style`](Self::plot_style), then shows or hides the readout
    /// together with [`legend_visible`](Self::legend_visible). False by default.
    fn readout_only(&self, line: usize) -> bool {
        let _ = line;
        false
    }

    /// The output lines the legend lists, in the order it lists them: all of them in plot order
    /// by default. A study whose leading reading is not its first output lists that one first.
    fn legend_lines(&self) -> Vec<usize> {
        (0..self.line_count()).collect()
    }

    /// Whether plot `line` shows its last value in a tag on the price scale while the chart
    /// shows study values. By default every plot of a study with up to four plots does and no
    /// plot of a study with more, whose values stay in the legend, so that the tags leave the
    /// scale's tick labels readable. A study on a chart starts from this choice and keeps the
    /// user's in [`StudySettings::untagged_plots`](super::StudySettings::untagged_plots).
    fn plot_tagged(&self, line: usize) -> bool {
        let _ = line;
        self.line_count() <= 4
    }

    /// Whether a horizontal latest-value line is drawn for this output. Off by default,
    /// independent of scale tags and displacement. A missing latest output draws no line.
    fn plot_price_line(&self, _line: usize) -> bool {
        false
    }

    /// Whether plot `line` holds per-bar values. Return false for a plot that only styles the
    /// study's [`zones`](Self::zones): their edges are lines whatever the plot type, and a plot
    /// without values puts no tag on the price scale and draws no latest value line, so the
    /// settings dialog offers such a plot only its visibility, colour, width and line pattern.
    /// True by default.
    fn plot_holds_values(&self, line: usize) -> bool {
        let _ = line;
        true
    }

    /// Editable alternate colour and width states, separate from the output's main style.
    fn plot_variants(&self, _line: usize) -> Vec<super::PlotVariant> {
        Vec::new()
    }

    /// Alternate state at a source bar, or `None` for the main style.
    fn plot_variant(&self, _line: usize, _bar: usize) -> Option<usize> {
        None
    }

    /// Stroke width at a source bar. Column and background plots use bar pitch instead.
    fn plot_width(&self, line: usize, _bar: usize) -> f32 {
        self.plot_style(line).width
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

    /// Whether everything drawn under the current inputs is plots, their shapes, bands and
    /// fills, all of which an offset moves bar for bar. A study without a
    /// [`source`](Self::source) offers Offset while this holds. Return true only when it reads
    /// the chart's bars alone and draws no markers, labels, zones, projections, tables,
    /// candles, profiles or drawings. False by default.
    fn draws_only_plots(&self) -> bool {
        false
    }

    /// Whether everything the study draws can follow a calculation on another symbol or interval:
    /// plots, bands, fills, markers, zones, candles and profiles, which a chart places on its
    /// own bars that share their time. Return false for a study that can draw drawings,
    /// [`labels`](Self::labels), [`projections`](Self::projections) or
    /// [`price_segments`](Self::price_segments) under any of its inputs, or whose output the
    /// chart draws from its own bars or symbol, such as a [`tpo`](Self::tpo) profile. Such a
    /// study offers no Symbol, Interval or Wait for interval close input, and restoring it drops
    /// saved values of them. The answer must not depend on the inputs, so that saved inputs
    /// restore in any order. True by default; while a study that returns true runs on other
    /// bars, its drawings, labels, projections and price segments are not drawn.
    fn draws_on_other_bars(&self) -> bool {
        true
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

    /// Optional text shown while the pointer is over the shape a circle, cross or triangle plot
    /// draws at a source bar, or over that shape's [`plot_text`](Self::plot_text). `None`, the
    /// default, shows nothing.
    fn plot_tooltip(&self, line: usize, bar: usize) -> Option<String> {
        let _ = (line, bar);
        None
    }

    /// Filled bands, painted beneath this study's plots. Missing endpoints break a band.
    fn fills(&self) -> Vec<super::StudyFill> {
        Vec::new()
    }

    /// Vertical colour ramps beneath the plots. Missing values break the filled area.
    fn gradients(&self) -> Vec<super::StudyGradient> {
        Vec::new()
    }

    /// Value at a displayed bar, accounting for the plot's displacement.
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        let source = bar.checked_add_signed(self.plot_offset(line).checked_neg()?)?;
        self.values().value(line, source)
    }

    /// Names of the declared alert conditions, in the order
    /// [`alert_condition`](Self::alert_condition) numbers them, such as `"Trend change"`.
    /// The alert dialog lists them after the outputs as `"{study}: {condition}"`, translating
    /// the condition when shown. Keep indices stable across settings changes and releases.
    /// None by default.
    ///
    /// A host study can declare a condition and construct an alert on it:
    ///
    /// ```
    /// use egui_charts::model::{Alert, AlertCondition, AlertSource, Bar};
    /// use egui_charts::studies::{Indicator, IndicatorValues};
    ///
    /// #[derive(Clone, Default)]
    /// struct PositiveClose { values: IndicatorValues }
    /// impl Indicator for PositiveClose {
    ///     fn name(&self) -> &str { "Positive close" }
    ///     fn calculate(&mut self, bars: &[Bar]) {
    ///         self.values = IndicatorValues::new();
    ///         for bar in bars {
    ///             self.values.push_lines(&[bar.close]);
    ///         }
    ///     }
    ///     fn values(&self) -> &IndicatorValues { &self.values }
    ///     fn alert_conditions(&self) -> Vec<String> { vec!["Close above zero".into()] }
    ///     fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
    ///         match condition {
    ///             0 => self.values.value(0, bar).map(|v| v > 0.0),
    ///             _ => None,
    ///         }
    ///     }
    /// #   fn colors(&self) -> Vec<ecolor::Color32> { vec![ecolor::Color32::YELLOW] }
    /// #   fn set_colors(&mut self, _: Vec<ecolor::Color32>) {}
    /// #   fn set_visible(&mut self, _: bool) {}
    /// #   fn clone_box(&self) -> Box<dyn Indicator> { Box::new(self.clone()) }
    /// }
    /// let mut study = PositiveClose::default();
    /// study.calculate(&[Bar::new(chrono::DateTime::UNIX_EPOCH, 1.0, 1.0, 1.0, 1.0, 1.0)]);
    /// assert_eq!(study.alert_conditions(), ["Close above zero"]);
    /// assert_eq!(study.alert_condition(0, 0), Some(true));
    /// let mut alert = Alert::new("SAMPLE", AlertCondition::Holds);
    /// alert.source = AlertSource::StudyCondition { id: "3".into(), condition: 0 };
    /// assert!(alert.is_valid());
    /// ```
    fn alert_conditions(&self) -> Vec<String> {
        Vec::new()
    }

    /// Whether a declared condition holds at a source bar, independent of plot displacement.
    /// Returns `None` for an unknown condition, a missing bar or an undefined condition,
    /// including warm-up. An alert counts `None` as not holding.
    fn alert_condition(&self, _condition: usize, _bar: usize) -> Option<bool> {
        None
    }

    /// Current applied presentation, initialized from the study's own defaults.
    fn settings(&self) -> super::StudySettings {
        super::settings::declared_settings(self)
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
    ) -> Result<(), crate::studies::StudyError> {
        snapshot.restore(self)
    }

    /// Apply a complete edit atomically. Charts wrap studies in [`ConfiguredStudy`](super::ConfiguredStudy),
    /// which validates inputs on a clone before replacing the calculation and presentation.
    fn apply_settings(
        &mut self,
        inputs: &[(String, super::InputValue)],
        settings: super::StudySettings,
    ) -> Result<(), crate::studies::StudyError> {
        let _ = (inputs, settings);
        Err(super::StudyError::NotConfigured)
    }

    /// Visible source bars, end exclusive, supplied after pan, zoom and resize and before
    /// autoscaling. Returns whether values need recalculation. Direct callers may supply it too.
    fn set_visible_range(&mut self, range: std::ops::Range<usize>) -> bool {
        let _ = range;
        false
    }

    /// Visible logical slots, including the empty space beyond the last bar. Supplied before
    /// autoscaling, alongside `set_visible_range`, so projected levels can follow the right
    /// edge without calculating again. The default ignores the viewport.
    fn set_visible_logical_range(&mut self, range: crate::model::LogicalRange) {
        let _ = range;
    }

    /// The chart's current interval, supplied when added and when the interval changes.
    fn set_timeframe(&mut self, timeframe: crate::model::Timeframe) {
        let _ = timeframe;
    }

    /// Source bar calendar, including the zone that dates the chart's bars and its session anchor.
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

    /// Price mapping supplied by the chart, independent of pane bounds: for a shared scale, and
    /// for a study in the price pane on its own scale or no scale whose chart primitives add
    /// prices to its fit. Custom presentation implementations can retain it to support these.
    fn scale_mapping(&self) -> Option<crate::scales::PriceMap> {
        None
    }

    /// Updates the chart-supplied mapping. `None` restores the ordinary pane mapping.
    fn set_scale_mapping(&mut self, mapping: Option<crate::scales::PriceMap>) {
        let _ = mapping;
    }

    /// Replaces value formatting when supported by the presentation. `None` restores the
    /// declared units and precision. Returns false when this study owns its formatting. A chart
    /// passes each study the formatter from `Chart::set_indicator_formatter` or the chart's
    /// formatters on every frame.
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

    /// Formats one output, including readouts whose units differ from the study's scale.
    /// The default uses [`format_value`](Self::format_value).
    fn format_plot_value(&self, line: usize, value: f64) -> String {
        let _ = line;
        self.format_value(value)
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
    /// studies reading it or the feed reports its end, and loads more when that time moves
    /// earlier. The time may lie before the chart's first bar, however few bars the chart holds.
    /// `None`, the default, counts as the chart's first bar. Price-based chart types such as
    /// Renko load at least back to that bar, honoring a stated time only when it lies earlier,
    /// since the source study is not given the view of the rendered elements.
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

    /// Whether the histories this study requests still apply when it is calculated on another
    /// interval than the chart's. A [`ConfiguredStudy`](super::ConfiguredStudy) then offers it the
    /// Interval and Wait for interval close inputs, otherwise offered to source-aware studies,
    /// studies that request nothing, and studies without an interval input of their own that
    /// request only price bars of other symbols or of the chart's interval, which they read on the
    /// Interval as on a chart of it, but never to a study with an
    /// [`InputKind::AutoTimeframe`](super::InputKind::AutoTimeframe) input, whose interval
    /// follows the chart. On another interval it passes every request on with the
    /// start [`data_start`](Indicator::data_start) states. The default, `false`, keeps a study
    /// that requests other histories, such as the chart symbol's bars of a stated interval or
    /// metrics, on the chart's interval.
    fn requests_allow_other_intervals(&self) -> bool {
        false
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
    /// The chart adds inputs of its own after these under the ids `plot_offset`,
    /// `calculation_symbol`, `calculation_timeframe` and `wait_for_close`; where it adds one, an
    /// edit by that id reaches the chart's input, so give the study's inputs other ids.
    fn inputs(&self) -> Vec<super::StudyInput> {
        Vec::new()
    }

    /// Change a calculation input, rejecting unknown ids with [`super::StudyError::UnknownInput`]
    /// and wrong types and invalid values with [`super::StudyError::InvalidInput`].
    /// On success cached values are cleared; calculate again or let the chart refresh.
    fn set_input(
        &mut self,
        id: &str,
        value: super::InputValue,
    ) -> Result<(), crate::studies::StudyError> {
        let _ = value;
        Err(super::StudyError::UnknownInput(id.into()))
    }

    /// Return the short display name of this indicator (e.g. `"SMA"`, `"RSI"`, `"MACD"`).
    fn name(&self) -> &str;

    /// The factory key used to recreate this study in saved charts, templates, clipboard copies
    /// and alerts. `IndicatorFactory::create` retains the key even when it differs from the
    /// display [`name`](Self::name). A study whose display name changes with its inputs should
    /// return its stable factory key here.
    fn registered_name(&self) -> &str {
        self.name()
    }

    /// Set the factory key. Returns whether the study supports the change; the chart's
    /// presentation wrapper does.
    fn set_registered_name(&mut self, _name: &str) -> bool {
        false
    }

    /// Report `defaults`, values keyed by input id, as the [`default`](super::StudyInput::default)
    /// of the matching [`inputs`](Self::inputs) that accept them, so that resetting the inputs
    /// restores them. `IndicatorFactory` passes the inputs a study it creates under the host's
    /// defaults starts with. The chart's presentation wrapper reports them. A host study that
    /// keeps its own presentation implements this to report them too; otherwise its inputs keep
    /// their declared defaults.
    fn set_input_defaults(&mut self, defaults: &[(String, super::InputValue)]) {
        let _ = defaults;
    }

    /// The descriptor needed to restore a host metric without its catalogue registration.
    /// Ordinary studies return `None`; a scalar metric returns its own descriptor.
    fn metric(&self) -> Option<&crate::data::MetricSeries> {
        None
    }

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

    /// Order-flow sources this study reads: [`OrderFlowInput`](crate::data::OrderFlowInput)s of
    /// fixed aggregations, footprints or books. A chart with a feed connects them before its
    /// updates are coalesced, gives them its instrument's tick size, applies the trade store's
    /// retention and the replay cutoff, and updates the study from the first bar that changed.
    /// Hosts without a feed push events through the handles and recalculate. Several studies may
    /// return the same source; it receives each event once.
    fn order_flow_inputs(&self) -> &[crate::data::OrderFlowInput] {
        &[]
    }

    /// Whether the study has results only with its
    /// [`order_flow_inputs`](Self::order_flow_inputs), so that it cannot be calculated on the bars
    /// of another interval or symbol, whose order flow the chart does not hold. The default is
    /// whether it names any. A study that builds its results from the bars and refines them with
    /// order flow where the chart holds it returns `false`, and keeps its Interval input.
    fn needs_order_flow(&self) -> bool {
        !self.order_flow_inputs().is_empty()
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

    /// Called by the chart after it evicted the oldest `count` source bars, before it calculates
    /// the study again in full, which can be a few frames later. Returns whether the study
    /// dropped the outputs of those bars and keeps the others on the bars that remain until
    /// then; otherwise it has no values until then. The default keeps nothing. The chart's
    /// presentation wrapper, which holds every study whose
    /// [`has_settings`](Indicator::has_settings) is `false`, keeps the values of a study that
    /// draws no candles, zones, markers, labels, price segments, projections, provisional segments
    /// or drawings.
    fn remove_oldest_bars(&mut self, count: usize) -> bool {
        let _ = count;
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

    /// The symbol the chart shows, or the symbol a study calculated on another symbol uses.
    /// The instrument given to [`set_instrument`](Indicator::set_instrument) describes it even
    /// when its own `symbol` differs, such as `X:ABC` for `ABC`. Empty on a chart without a
    /// symbol. Returns `true` if the caller must recalculate from bar 0. The chart calls this
    /// wherever it calls `set_instrument`. Studies independent of the symbol keep the default.
    fn set_chart_symbol(&mut self, symbol: &str) -> bool {
        let _ = symbol;
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
    /// slice most recently passed to [`calculate`](Indicator::calculate), less the bars
    /// [`remove_oldest_bars`](Indicator::remove_oldest_bars) removed since.
    fn values(&self) -> &IndicatorValues;

    /// Rendering style of an overlay line. Defaults to a 1 px line in its configured colour.
    /// Any line is accepted, as for [`plot_style`](Self::plot_style).
    fn line_style(&self, line: usize) -> IndicatorLineStyle {
        let _ = line;
        IndicatorLineStyle::default()
    }

    /// Candle bodies and wicks, rendered in place of line plots when nonempty.
    /// Each candle takes its plot's colour, or [`candle_color`](Self::candle_color) when
    /// that returns one. Source ranges and OHLCV remain available to hosts for per-bar readouts.
    fn candles(&self) -> &[super::StudyCandle] {
        &[]
    }

    /// Optional colour of the candle at `index` in [`candles`](Self::candles), overriding its
    /// plot colour. Defaults to [`plot_color`](Self::plot_color) at its first source bar.
    /// A study mapping candle geometry to other bars can retain that source colour here.
    fn candle_color(&self, index: usize) -> Option<Color32> {
        let candle = self.candles().get(index)?;
        self.plot_color(candle.plot, candle.bars.start)
    }

    /// Whether each of [`candles`](Self::candles) is painted as a block filled in its plot
    /// colour from its open to its close, with no outline and no wick, rather than as a body
    /// tinted at 30% opacity inside an outline, with a wick from its high to its low. Off by
    /// default.
    fn solid_candles(&self) -> bool {
        false
    }

    /// Drawing geometry generated by this study, painted beneath its plots and styled as
    /// [`drawings_use_plot_style`](Self::drawings_use_plot_style) says. Empty by default.
    /// Calculations belong in `calculate`; use tools whose renderer needs only their anchors
    /// and settings, such as trend lines, parallel channels, Fibonacci levels and pitchforks.
    /// A drawing with a [`label`](crate::drawings::Drawing::label) shows it while the pointer is
    /// over the drawing.
    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        &[]
    }

    /// Whether plot 0 styles the [`drawings`](Self::drawings). When `true`, the default, every
    /// drawing takes plot 0's colour, width, pattern and visibility, and plot 0 is painted and
    /// selected as these drawings instead of as its values, which still scale the study and
    /// fill its readouts. When `false`, each drawing keeps its own style and shows while the
    /// study does, unless [`drawing_plot`](Self::drawing_plot) names a plot for it, and plot 0
    /// is painted like the other plots. Only a study with drawings is affected.
    #[cfg(feature = "drawings")]
    fn drawings_use_plot_style(&self) -> bool {
        true
    }

    /// The plot whose colour, width, pattern and visibility drawing `index` of
    /// [`drawings`](Self::drawings) takes while
    /// [`drawings_use_plot_style`](Self::drawings_use_plot_style) is `false`, so hiding that
    /// plot hides the drawing. `None`, the default, keeps the drawing's own style and shows it
    /// while the study does.
    #[cfg(feature = "drawings")]
    fn drawing_plot(&self, index: usize) -> Option<usize> {
        let _ = index;
        None
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

    /// Paint labels as tags at their price, with a plot-coloured background and white text.
    /// False keeps the ordinary text and its placement. Tag pointers face right.
    fn labels_use_plot_background(&self) -> bool {
        false
    }

    /// Draw [`labels`](Self::labels) on the main price chart, even when the study has its
    /// own pane. Label prices use the main series' scale. Off by default.
    fn labels_on_price(&self) -> bool {
        false
    }

    /// Keep the plots, their fills and the [`projections`](Self::projections) on the main price
    /// chart when the study has its own pane, using the main series' price scale. Its pane then
    /// shows the [`table`](Self::table) and legend alone, without a value axis. The price scale
    /// fits the plots as it fits an overlay's. They are selected from the study's pane. On the
    /// price pane the study uses its chosen scale as other overlays do. Off by default.
    fn plots_on_price(&self) -> bool {
        false
    }

    /// Straight lines on the main price chart, in the main series' price scale, also when the
    /// study has its own pane. Each uses the colour, width and pattern of its
    /// [`plot`](super::StudySegment::plot) while that output is visible. Empty by default. Keep
    /// lines from the last calculation: they are drawn every frame. A study calculated on
    /// another interval or symbol draws none, as its bars and prices are not the chart's.
    fn price_segments(&self) -> &[super::StudySegment] {
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

    /// Levels of finished price-row profiles carried right past their own bars, each to the
    /// first later bar that trades in its row of the profile, or on the line for the value area
    /// high, or to the last bar: those whose bars intersect the half-open visible source-bar
    /// range, including levels of profiles left of it. Empty by default.
    fn profile_levels(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::ProfileLevel> {
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

    /// The outputs an alert can test, with the names shown after the study's name. Read each
    /// output through [`plot_value`](Self::plot_value). Defaults to every line of
    /// [`line_names`](Self::line_names). Omit lines that cannot hold values with the current
    /// settings, such as lines that only style zones, candles or drawings. Values drawn in
    /// other forms, such as levels, may use outputs past
    /// [`line_count`](Self::line_count), which renderers do not draw. Keep an output's number
    /// while it means the same value, since saved alerts identify outputs by number.
    fn alert_outputs(&self) -> Vec<(usize, String)> {
        self.line_names().into_iter().enumerate().collect()
    }

    /// Whether this indicator should currently be rendered.
    ///
    /// Defaults to `true`. Toggle with [`set_visible`](Indicator::set_visible).
    fn is_visible(&self) -> bool {
        true
    }

    /// Show or hide this indicator.
    fn set_visible(&mut self, visible: bool);

    /// Whether the user has this indicator shown: the switch that
    /// [`set_visible`](Indicator::set_visible) sets, whatever its interval settings hide on the
    /// current interval. Study templates save this switch, so that a copy on another interval is
    /// drawn wherever its settings allow.
    ///
    /// Defaults to [`is_visible`](Indicator::is_visible); override it when `is_visible` also
    /// hides the indicator by another rule.
    fn user_visible(&self) -> bool {
        self.is_visible()
    }

    /// Clone this indicator into a new `Box<dyn Indicator>`.
    ///
    /// This enables cloning of heterogeneous indicator collections.
    fn clone_box(&self) -> Box<dyn Indicator>;

    /// Clone for an export that can be drawn after the live chart changes or is dropped.
    /// The result must have no live subscription and share nothing that later events can
    /// mutate. Immutable history owners may remain shared.
    ///
    /// The default calls [`clone_box`](Self::clone_box). Override when the study holds live
    /// state; [`OrderFlowInput::frozen`](crate::data::OrderFlowInput::frozen) detaches stream
    /// inputs while preserving their current data. Ordinary presentation and undo clones
    /// continue to use `clone_box`.
    fn clone_for_export(&self) -> Box<dyn Indicator> {
        self.clone_box()
    }
}

#[cfg(all(feature = "widget", feature = "studies"))]
impl dyn Indicator {
    pub(crate) fn concrete<T: 'static>(&self) -> Option<&T> {
        let cast: fn(&T) -> &dyn std::any::Any = |value| value;
        ConcreteStudy::concrete_study(self, &cast)?.downcast_ref()
    }
}

/// Blanket `Clone` implementation for boxed indicators, delegating to
/// [`Indicator::clone_box`].
impl Clone for Box<dyn Indicator> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
