//! Runtime-defined custom indicators.
//!
//! The [`CustomIndicator`] type lets users create indicators at runtime by
//! providing a closure that takes `&[Bar]` and returns `Vec<IndicatorValue>`.
//! This is useful for prototyping or embedding user-defined logic without
//! creating a new struct that implements [`Indicator`].
//!
//! # Example
//!
//! ```rust
//! use egui_charts::studies::{CustomIndicator, IndicatorValue};
//!
//! let momentum = CustomIndicator::new("Momentum(10)", |bars| {
//!     bars.iter()
//!         .enumerate()
//!         .map(|(i, bar)| {
//!             if i < 10 {
//!                 IndicatorValue::None
//!             } else {
//!                 IndicatorValue::Single(bar.close - bars[i - 10].close)
//!             }
//!         })
//!         .collect()
//! })
//! .with_overlay(false)
//! .with_description("Custom 10-bar price momentum");
//! ```
//!
//! The closure is shared behind an [`Arc`], so a clone computes exactly what
//! the original does. A study whose calculation resumes from its last bars, or runs away from
//! the UI thread, implements [`Resume`] instead and is set with
//! [`CustomIndicator::with_resume`].

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::sync::Arc;
use std::time::Duration;

/// Type alias for the custom calculation closure.
///
/// The closure receives the full bar series and must return exactly one
/// [`IndicatorValue`] per bar.
pub type CalculateFn = Arc<dyn Fn(&[Bar]) -> Vec<IndicatorValue> + Send + Sync>;

/// Calculation callback receiving current inputs, an aligned source and requested histories.
pub type InputCalculateFn = Arc<
    dyn Fn(&[Bar], &[super::StudyInput], &[f64], &[super::StudyData<'_>]) -> Vec<IndicatorValue>
        + Send
        + Sync,
>;

/// Callback declaring additional histories from the study's current inputs.
pub type DataRequestsFn =
    Arc<dyn Fn(&[super::StudyInput]) -> Vec<super::StudyDataRequest> + Send + Sync>;

/// Per-bar colour callback, receiving the output line, source bar and calculated columns.
pub type PlotColorFn = Arc<dyn Fn(usize, usize, &IndicatorValues) -> Option<Color32> + Send + Sync>;

/// The calculation of a custom study that resumes from its last bars instead of starting over,
/// and may run away from the UI thread. Set it with [`CustomIndicator::with_resume`].
///
/// A running sum of closes that resumes from its last bar, coloured by its value and labelled at
/// its last bar:
///
/// ```
/// use egui_charts::model::Bar;
/// use egui_charts::studies::{
///     Calculation, CustomIndicator, Indicator, IndicatorValues, LabelPlacement, Resume,
///     ResumeRun, StudyLabel,
/// };
/// use std::time::Duration;
///
/// #[derive(Clone, Default)]
/// struct RunningSum {
///     // The sum after each bar calculated so far.
///     sums: Vec<f64>,
///     labels: Vec<StudyLabel>,
/// }
/// impl Resume for RunningSum {
///     fn resume(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation {
///         let from = run.from.min(self.sums.len());
///         self.sums.truncate(from);
///         values.truncate(from);
///         for bar in &run.data[from..] {
///             let sum = self.sums.last().copied().unwrap_or(0.0) + bar.close;
///             self.sums.push(sum);
///             values.push_lines(&[sum]);
///         }
///         let last = self.sums.len().checked_sub(1);
///         self.labels = (last.zip(self.sums.last()))
///             .map(|(bar, sum)| StudyLabel {
///                 bar,
///                 price: *sum,
///                 text: format!("{sum}"),
///                 placement: LabelPlacement::Above,
///                 plot: 0,
///             })
///             .into_iter()
///             .collect();
///         Calculation::Done(Duration::ZERO)
///     }
///     fn alert_conditions(&self) -> Vec<String> {
///         vec!["Sum above 5".into()]
///     }
///     fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
///         (condition == 0).then(|| self.sums.get(bar).map(|sum| *sum > 5.0)).flatten()
///     }
///     fn plot_color(&self, _line: usize, bar: usize) -> Option<ecolor::Color32> {
///         let sum = self.sums.get(bar)?;
///         Some(if *sum > 5.0 { ecolor::Color32::GREEN } else { ecolor::Color32::RED })
///     }
///     fn labels(&self) -> &[StudyLabel] {
///         &self.labels
///     }
///     // Labels sit on the bars calculated on, which an offset or another interval leaves behind.
///     fn draws_only_plots(&self) -> bool {
///         false
///     }
///     fn draws_on_other_bars(&self) -> bool {
///         false
///     }
/// }
///
/// let bar = |close: f64| Bar::new(chrono::DateTime::UNIX_EPOCH, close, close, close, close, 1.0);
/// let mut bars = vec![bar(1.0), bar(2.0)];
/// let mut study = CustomIndicator::new("Sum", |_| Vec::new()).with_resume(RunningSum::default());
/// study.calculate(&bars);
/// bars.push(bar(4.0));
/// study.update(&bars, 2);
/// bars[2] = bar(5.0);
/// study.update(&bars, 2);
/// let mut fresh = study.clone();
/// fresh.calculate(&bars);
/// assert_eq!(format!("{:?}", study.values()), format!("{:?}", fresh.values()));
/// assert_eq!(study.values().value(0, 2), Some(8.0));
/// assert_eq!(study.calculation(), Some(Calculation::Done(Duration::ZERO)));
/// assert_eq!(study.alert_conditions(), ["Sum above 5"]);
/// assert_eq!(study.alert_condition(0, 1), Some(false));
/// assert_eq!(study.alert_condition(0, 2), Some(true));
/// assert_eq!(study.alert_condition(1, 2), None);
/// assert_eq!(study.plot_color(0, 1), Some(ecolor::Color32::RED));
/// assert_eq!(study.plot_color(0, 2), Some(ecolor::Color32::GREEN));
/// assert_eq!(study.labels()[0].text, "8");
/// assert!(!study.draws_only_plots() && !study.draws_on_other_bars());
/// ```
pub trait Resume: Send + Sync {
    /// Bring `values` up to date for `run.data`, given that `run.data[..run.from]` is unchanged
    /// since the previous call (the contract of [`Indicator::update`]), and say how far it got.
    /// [`Calculation::Running`] leaves `values` as an earlier call left them, and
    /// [`Calculation::Stopped`] as they were before the calculation that stopped: the chart
    /// keeps drawing them, and while the calculation runs it calls again on a later frame with
    /// the latest bars.
    fn resume(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation;

    /// Names of the study's alert conditions, in the order
    /// [`alert_condition`](Self::alert_condition) numbers them, which [`CustomIndicator`] gives as
    /// [`Indicator::alert_conditions`]. Keep the order stable: saved alerts know a condition by
    /// its number. None by default.
    fn alert_conditions(&self) -> Vec<String> {
        Vec::new()
    }

    /// Whether condition `condition` holds at source bar `bar` as of the values the last
    /// [`resume`](Self::resume) call left, which [`CustomIndicator`] gives as
    /// [`Indicator::alert_condition`]. `None` for an unknown condition or bar, or one not decided
    /// there, which an alert counts as not holding. `None` by default.
    fn alert_condition(&self, _condition: usize, _bar: usize) -> Option<bool> {
        None
    }

    /// The colour of plot `line` at source bar `bar` as of the values the last
    /// [`resume`](Self::resume) call left, which [`CustomIndicator`] gives as
    /// [`Indicator::plot_color`] ahead of [`with_plot_color`](CustomIndicator::with_plot_color).
    /// `None`, the default, keeps the plot's colour.
    fn plot_color(&self, _line: usize, _bar: usize) -> Option<Color32> {
        None
    }

    /// Text beside the study's values, in order of `bar`, which [`CustomIndicator`] gives as
    /// [`Indicator::labels`]. Empty by default.
    fn labels(&self) -> &[super::StudyLabel] {
        &[]
    }

    /// Straight lines on the price chart, which [`CustomIndicator`] gives as
    /// [`Indicator::price_segments`]. Empty by default.
    fn price_segments(&self) -> &[super::StudySegment] {
        &[]
    }

    /// Price bands, which [`CustomIndicator`] gives as [`Indicator::zones`]. Empty by default.
    fn zones(&self) -> &[super::StudyZone] {
        &[]
    }

    /// Shapes at study values, which [`CustomIndicator`] gives as [`Indicator::markers`]. Empty
    /// by default.
    fn markers(&self) -> &[super::IndicatorMarker] {
        &[]
    }

    /// Whether everything the study draws is plots, bands and fills, as
    /// [`Indicator::draws_only_plots`] asks: return false when it can draw labels, segments,
    /// zones or markers. True by default; a [`CustomIndicator`] with data requests answers false
    /// anyway.
    fn draws_only_plots(&self) -> bool {
        true
    }

    /// Whether everything the study draws can follow a calculation on another symbol or
    /// interval, as [`Indicator::draws_on_other_bars`] asks: return false when it can draw
    /// labels or price segments. The answer must not depend on the inputs. True by default.
    fn draws_on_other_bars(&self) -> bool {
        true
    }
}

/// What a [`Resume::resume`] call works on.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ResumeRun<'a> {
    /// The chart's bars.
    pub data: &'a [Bar],
    /// The first bar that may differ from the previous call.
    pub from: usize,
    /// The study's current inputs.
    pub inputs: &'a [super::StudyInput],
    /// The column the chart feeds into the `source` input, if any.
    pub source: Option<&'a [f64]>,
    /// The histories the study requested, in request order.
    pub histories: &'a [super::StudyData<'a>],
}

/// How far a study's calculation has come, for the chart and its legend.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Calculation {
    /// The values are up to date; the calculation took this long.
    Done(Duration),
    /// The calculation runs elsewhere; the fraction of its bars done.
    Running(f32),
    /// The calculation stopped before its end and the values are the previous ones; why.
    Stopped(String),
}

/// A [`Resume`] that [`CustomIndicator`] can clone.
trait ResumeBox: Send + Sync {
    fn resume_box(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation;
    fn alert_conditions_box(&self) -> Vec<String>;
    fn alert_condition_box(&self, condition: usize, bar: usize) -> Option<bool>;
    fn plot_color_box(&self, line: usize, bar: usize) -> Option<Color32>;
    fn labels_box(&self) -> &[super::StudyLabel];
    fn price_segments_box(&self) -> &[super::StudySegment];
    fn zones_box(&self) -> &[super::StudyZone];
    fn markers_box(&self) -> &[super::IndicatorMarker];
    fn draws_only_plots_box(&self) -> bool;
    fn draws_on_other_bars_box(&self) -> bool;
    fn clone_box(&self) -> Box<dyn ResumeBox>;
}

impl<R: Resume + Clone + 'static> ResumeBox for R {
    fn resume_box(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation {
        self.resume(run, values)
    }
    fn alert_conditions_box(&self) -> Vec<String> {
        self.alert_conditions()
    }
    fn alert_condition_box(&self, condition: usize, bar: usize) -> Option<bool> {
        self.alert_condition(condition, bar)
    }
    fn plot_color_box(&self, line: usize, bar: usize) -> Option<Color32> {
        self.plot_color(line, bar)
    }
    fn labels_box(&self) -> &[super::StudyLabel] {
        self.labels()
    }
    fn price_segments_box(&self) -> &[super::StudySegment] {
        self.price_segments()
    }
    fn zones_box(&self) -> &[super::StudyZone] {
        self.zones()
    }
    fn markers_box(&self) -> &[super::IndicatorMarker] {
        self.markers()
    }
    fn draws_only_plots_box(&self) -> bool {
        self.draws_only_plots()
    }
    fn draws_on_other_bars_box(&self) -> bool {
        self.draws_on_other_bars()
    }
    fn clone_box(&self) -> Box<dyn ResumeBox> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn ResumeBox> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// An indicator defined at runtime via a calculation closure.
///
/// Build one with [`new`](Self::new) and customise it with the builder
/// methods (`with_description`, `with_color`, `with_overlay`, etc.).
#[derive(Clone)]
pub struct CustomIndicator {
    name: String,
    desc: String,
    calculate_fn: CalculateFn,
    input_calculate_fn: Option<InputCalculateFn>,
    inputs: Vec<super::StudyInput>,
    data_requests: Option<DataRequestsFn>,
    wait_for_close: bool,
    values: IndicatorValues,
    colors: Vec<Color32>,
    is_overlay: bool,
    line_count: usize,
    line_names: Vec<String>,
    visible: bool,
    value_format: Option<crate::scales::StudyFormat>,
    scale: Option<super::StudyScale>,
    plot_types: Vec<super::PlotType>,
    offsets: Vec<isize>,
    fills: Vec<super::StudyFill>,
    plot_color: Option<PlotColorFn>,
    resume: Option<Box<dyn ResumeBox>>,
    calculation: Option<Calculation>,
}

impl std::fmt::Debug for CustomIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomIndicator")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl CustomIndicator {
    /// Create a new custom indicator.
    ///
    /// # Arguments
    /// * `name` -- Display name shown in legends and tooltips.
    /// * `calculate_fn` -- Closure that computes indicator values from bars.
    ///
    /// The indicator defaults to a single-line yellow overlay.
    pub fn new<F>(name: impl Into<String>, calculate_fn: F) -> Self
    where
        F: Fn(&[Bar]) -> Vec<IndicatorValue> + Send + Sync + 'static,
    {
        let name = name.into();
        Self {
            line_names: vec![name.clone()],
            name,
            desc: String::new(),
            calculate_fn: Arc::new(calculate_fn),
            input_calculate_fn: None,
            inputs: Vec::new(),
            data_requests: None,
            wait_for_close: false,
            values: IndicatorValues::new(),
            colors: vec![palette::YELLOW], // Yellow default
            is_overlay: true,
            line_count: 1,
            visible: true,
            value_format: None,
            scale: None,
            plot_types: Vec::new(),
            offsets: Vec::new(),
            fills: Vec::new(),
            plot_color: None,
            resume: None,
            calculation: None,
        }
    }

    /// Declare editable inputs and calculate from their current values. Ids must be unique, and
    /// must not be `plot_offset`, `calculation_symbol`, `calculation_timeframe` or
    /// `wait_for_close`: the chart adds its Offset, Symbol, Interval and Wait for interval close
    /// inputs to the study under these ids, and refuses them with
    /// [`StudyError::InvalidInput`](super::StudyError::InvalidInput). Defaults and initial values
    /// must satisfy the same schema used by the editor and setters.
    ///
    /// The calculation receives the bars, inputs, aligned source and available histories from
    /// [`with_data_requests`](Self::with_data_requests), and returns one value per bar.
    /// A [`Source`](super::InputKind::Source) input named `source` declares the study's
    /// [`Indicator::source`]. The source slice contains that price field or the series supplied
    /// by the chart, including another study's output. The chart calculates dependencies first
    /// and checks for cycles. Read the selected series from this slice: the input retains its
    /// own value and the bars retain their original prices. Without that input the slice is
    /// empty. Source inputs themselves accept price fields only; study outputs are supplied
    /// through the chart's source selection.
    pub fn with_inputs<F>(
        mut self,
        inputs: Vec<super::StudyInput>,
        calculate: F,
    ) -> Result<Self, super::StudyError>
    where
        F: Fn(&[Bar], &[super::StudyInput], &[f64], &[super::StudyData<'_>]) -> Vec<IndicatorValue>
            + Send
            + Sync
            + 'static,
    {
        for (at, input) in inputs.iter().enumerate() {
            if input.id.is_empty() {
                return Err(super::StudyError::EmptyInputId { index: at });
            }
            let reason = if super::settings::CHART_INPUT_IDS.contains(&input.id.as_str()) {
                "is reserved by the chart".into()
            } else if inputs[..at].iter().any(|other| other.id == input.id) {
                "is declared twice".into()
            } else if !input.accepts(&input.value) {
                format!("has value {:?}, expected {:?}", input.value, input.kind)
            } else if !input.accepts(&input.default) {
                format!("has default {:?}, expected {:?}", input.default, input.kind)
            } else if is_study(&input.value) {
                format!("has value {:?}, expected a price field", input.value)
            } else if is_study(&input.default) {
                format!("has default {:?}, expected a price field", input.default)
            } else {
                continue;
            };
            return Err(super::StudyError::InvalidInput {
                id: input.id.clone(),
                reason,
            });
        }
        self.inputs = inputs;
        self.input_calculate_fn = Some(Arc::new(calculate));
        self.values.clear();
        Ok(self)
    }

    /// Declare additional histories from the current inputs, such as the symbol named by a
    /// [`Symbol`](super::InputKind::Symbol) input. Empty symbols and absent intervals follow
    /// the chart. Input edits update the chart's requests; the calculation supplied to
    /// [`with_inputs`](Self::with_inputs) receives each available history with its declaration.
    /// Histories that have not arrived are absent. Read the bars of another interval as each
    /// chart bar knew them with [`StudyData::closed_at`](super::StudyData::closed_at), passing
    /// `developing` for the chart's latest bar, and offer Wait for interval close for them with
    /// [`with_wait_for_close`](Self::with_wait_for_close).
    /// [`StudyData::bar_at`](super::StudyData::bar_at) reads the bar containing a time, also
    /// while it is still open. A study declaring histories has no [`Indicator::source`]; its
    /// `source` input, if present, reads a chart-bar price field.
    ///
    /// ```
    /// use egui_charts::model::Timeframe;
    /// use egui_charts::studies::{
    ///     CustomIndicator, Indicator, IndicatorValue, InputKind, InputValue, StudyDataRequest,
    ///     StudyInput,
    /// };
    ///
    /// let hourly = |inputs: &[StudyInput]| match &inputs[0].value {
    ///     InputValue::Symbol(symbol) => vec![StudyDataRequest {
    ///         symbol: symbol.clone(),
    ///         timeframe: Some(Timeframe::Hour1),
    ///         metric: None,
    ///     }],
    ///     _ => Vec::new(),
    /// };
    /// let symbol = StudyInput::new(
    ///     "symbol", "Symbol", InputValue::Symbol("OTHER".into()), InputKind::Symbol,
    /// );
    /// let other_close = CustomIndicator::new("Other close", |_| Vec::new())
    ///     .with_inputs(vec![symbol], move |bars, inputs, _, histories| {
    ///         let requests = hourly(inputs);
    ///         let history = histories.iter().find(|history| requests.contains(&history.request));
    ///         bars.iter().enumerate().map(|(at, bar)| {
    ///             history
    ///                 .and_then(|history| {
    ///                     let at = history.closed_at(bar.time, at + 1 == bars.len())?;
    ///                     Some(history.bars[at].close)
    ///                 })
    ///                 .map_or(IndicatorValue::None, IndicatorValue::Single)
    ///         }).collect()
    ///     })
    ///     .unwrap()
    ///     .with_data_requests(hourly)
    ///     .with_wait_for_close();
    /// assert_eq!(other_close.data_requests()[0].symbol, "OTHER");
    /// ```
    pub fn with_data_requests<F>(mut self, requests: F) -> Self
    where
        F: Fn(&[super::StudyInput]) -> Vec<super::StudyDataRequest> + Send + Sync + 'static,
    {
        self.data_requests = Some(Arc::new(requests));
        self.values.clear();
        self
    }

    /// Offer Wait for interval close, on by default, for the histories this study reads with
    /// [`StudyData::closed_at`](super::StudyData::closed_at), also when an interval input of
    /// its own chooses them. While it is on, the chart's latest bar reads only bars of a longer
    /// interval that closed by its open; turned off, it reads the developing bar. The chart
    /// applies the setting through [`StudyData::chart`](super::StudyData::chart).
    pub fn with_wait_for_close(mut self) -> Self {
        self.wait_for_close = true;
        self
    }

    /// Calculate with `resume` instead of the per-bar closures, from the last bars on. The study
    /// then updates on every tick ([`Indicator::is_incremental`]), keeps drawing its values
    /// while a calculation runs or after one stopped, also after an input edit, and reports
    /// each call's result through [`Indicator::calculation`].
    pub fn with_resume(mut self, resume: impl Resume + Clone + 'static) -> Self {
        self.resume = Some(Box::new(resume));
        self.calculation = None;
        self.values.clear();
        self
    }

    /// Set a human-readable description (used in tooltips).
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.desc = desc.into();
        self
    }

    /// Set the line color (single-line indicators).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.colors = vec![color];
        self
    }

    /// Set colors for a multi-line indicator.
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// Set whether this indicator is drawn on the main price chart (`true`)
    /// or in a separate sub-pane (`false`).
    pub fn with_overlay(mut self, is_overlay: bool) -> Self {
        self.is_overlay = is_overlay;
        self
    }

    /// Set the number of output lines (must match the closure's output).
    pub fn with_line_count(mut self, count: usize) -> Self {
        self.line_count = count;
        self
    }

    /// Set the units used by every readout of this study.
    pub fn with_value_format(mut self, format: crate::scales::StudyFormat) -> Self {
        self.value_format = Some(format);
        self
    }

    /// Set the value scale independently of overlay or separate-pane placement.
    pub fn with_scale(mut self, scale: super::StudyScale) -> Self {
        self.scale = Some(scale);
        self
    }

    /// Set geometry per output; omitted outputs remain lines.
    pub fn with_plot_types(mut self, types: Vec<super::PlotType>) -> Self {
        self.plot_types = types;
        self
    }

    /// Set displacement per output; omitted outputs stay on their source bars.
    pub fn with_offsets(mut self, offsets: Vec<isize>) -> Self {
        self.offsets = offsets;
        self
    }

    /// Set filled bands between output lines or constant levels.
    pub fn with_fills(mut self, fills: Vec<super::StudyFill>) -> Self {
        self.fills = fills;
        self
    }

    /// Set a per-bar colour callback. Returning `None` uses the plot colour. A [`Resume`] that
    /// answers a colour comes first.
    pub fn with_plot_color<F>(mut self, color: F) -> Self
    where
        F: Fn(usize, usize, &IndicatorValues) -> Option<Color32> + Send + Sync + 'static,
    {
        self.plot_color = Some(Arc::new(color));
        self
    }

    /// Set display names for each line (used in legends).
    pub fn with_line_names(mut self, names: Vec<String>) -> Self {
        self.line_names = names;
        self
    }
}

/// Study outputs must be supplied by the chart, rather than stored as custom input values.
fn is_study(value: &super::InputValue) -> bool {
    matches!(
        value,
        super::InputValue::Source(super::StudySource::Study { .. })
    )
}

impl CustomIndicator {
    fn price_source(&self) -> Option<crate::model::PriceSource> {
        self.inputs
            .iter()
            .find(|input| input.id == "source" && matches!(input.kind, super::InputKind::Source))
            .and_then(|input| match input.value {
                super::InputValue::Source(super::StudySource::Price(price)) => Some(price),
                _ => None,
            })
    }

    /// Runs the [`Resume`], if set, and returns whether it ran.
    fn resume(
        &mut self,
        data: &[Bar],
        from: usize,
        source: Option<&[f64]>,
        histories: &[super::StudyData<'_>],
    ) -> bool {
        let Some(resume) = &mut self.resume else {
            return false;
        };
        let run = ResumeRun {
            data,
            from,
            inputs: &self.inputs,
            source,
            histories,
        };
        self.calculation = Some(resume.resume_box(run, &mut self.values));
        true
    }

    fn run(&mut self, data: &[Bar], fed: Option<&[f64]>, histories: &[super::StudyData<'_>]) {
        self.values = match &self.input_calculate_fn {
            Some(calculate) => {
                let source: Vec<f64> = match (fed, self.price_source()) {
                    (Some(fed), _) => (0..data.len())
                        .map(|at| fed.get(at).copied().unwrap_or(f64::NAN))
                        .collect(),
                    (None, Some(price)) => data
                        .iter()
                        .map(|bar| price.compute(bar.open, bar.high, bar.low, bar.close))
                        .collect(),
                    (None, None) => Vec::new(),
                };
                calculate(data, &self.inputs, &source, histories)
            }
            None => (self.calculate_fn)(data),
        }
        .into();
    }
}

impl Indicator for CustomIndicator {
    fn inputs(&self) -> Vec<super::StudyInput> {
        self.inputs.clone()
    }

    fn set_input(
        &mut self,
        id: &str,
        value: super::InputValue,
    ) -> Result<(), crate::studies::StudyError> {
        let input = self
            .inputs
            .iter_mut()
            .find(|input| input.id == id)
            .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
        if !input.accepts(&value) || is_study(&value) {
            return Err(crate::studies::StudyError::invalid_input(id));
        }
        input.value = value;
        // A resumed calculation keeps drawing its values until the next one replaces them.
        if self.resume.is_none() {
            self.values.clear();
        }
        Ok(())
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        self.value_format.unwrap_or(if self.is_overlay {
            crate::scales::StudyFormat::Price
        } else {
            crate::scales::StudyFormat::Fixed(2)
        })
    }

    fn scale(&self) -> super::StudyScale {
        self.scale.unwrap_or(if self.is_overlay {
            super::StudyScale::Main
        } else {
            super::StudyScale::Independent
        })
    }

    fn plot_style(&self, line: usize) -> super::PlotStyle {
        super::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: self.plot_types.get(line).copied().unwrap_or_default(),
            visible: true,
        }
    }

    fn plot_offset(&self, line: usize) -> isize {
        self.offsets.get(line).copied().unwrap_or(0)
    }

    fn draws_only_plots(&self) -> bool {
        // A study reading histories of its own keeps its inputs.
        self.data_requests.is_none()
            && (self.resume.as_ref()).is_none_or(|resume| resume.draws_only_plots_box())
    }

    fn draws_on_other_bars(&self) -> bool {
        (self.resume.as_ref()).is_none_or(|resume| resume.draws_on_other_bars_box())
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        (self.resume.as_ref())
            .and_then(|resume| resume.plot_color_box(line, bar))
            .or_else(|| (self.plot_color.as_ref()).and_then(|color| color(line, bar, &self.values)))
    }

    fn labels(&self) -> &[super::StudyLabel] {
        (self.resume.as_ref()).map_or(&[], |resume| resume.labels_box())
    }

    fn price_segments(&self) -> &[super::StudySegment] {
        (self.resume.as_ref()).map_or(&[], |resume| resume.price_segments_box())
    }

    fn zones(&self) -> &[super::StudyZone] {
        (self.resume.as_ref()).map_or(&[], |resume| resume.zones_box())
    }

    fn markers(&self) -> &[super::IndicatorMarker] {
        (self.resume.as_ref()).map_or(&[], |resume| resume.markers_box())
    }

    fn fills(&self) -> Vec<super::StudyFill> {
        self.fills.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn desc(&self) -> &str {
        &self.desc
    }

    fn calculate(&mut self, data: &[Bar]) {
        if !self.resume(data, 0, None, &[]) {
            self.run(data, None, &[]);
        }
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if !self.resume(data, from, None, &[]) {
            self.run(data, None, &[]);
        }
    }

    fn is_incremental(&self) -> bool {
        self.resume.is_some()
    }

    fn calculation(&self) -> Option<Calculation> {
        self.calculation.clone()
    }

    fn alert_conditions(&self) -> Vec<String> {
        (self.resume.as_ref()).map_or_else(Vec::new, |resume| resume.alert_conditions_box())
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        self.resume.as_ref()?.alert_condition_box(condition, bar)
    }

    fn source(&self) -> Option<super::StudySource> {
        // The chart supplies either histories or a source column to a study.
        if self.data_requests.is_some() {
            return None;
        }
        self.price_source().map(super::StudySource::Price)
    }

    fn data_requests(&self) -> Vec<super::StudyDataRequest> {
        self.data_requests
            .as_ref()
            .map_or_else(Vec::new, |requests| requests(&self.inputs))
    }

    fn offers_wait_for_close(&self) -> bool {
        self.wait_for_close
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[super::StudyData<'_>]) {
        if !self.resume(data, from, None, sources) {
            self.run(data, None, sources);
        }
    }

    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        if self.resume(data, from, Some(source), &[]) {
            return;
        }
        if self.input_calculate_fn.is_some() {
            self.run(data, Some(source), &[]);
        } else {
            // Preserve the trait's price substitution for closures that read bars alone.
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
            self.run(&bars, None, &[]);
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        self.is_overlay
    }

    fn line_count(&self) -> usize {
        self.line_count
    }

    fn line_names(&self) -> Vec<String> {
        self.line_names.clone()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_inputs_validate_edits_and_reach_cloned_calculations() {
        use crate::model::{PriceSource, Timeframe};
        use crate::studies::{InputKind, InputValue, StudyInput, StudySource};
        let schema = vec![
            StudyInput::new(
                "length",
                "Length",
                InputValue::Length(2),
                InputKind::Length(1, 10),
            ),
            StudyInput::new(
                "color",
                "Colour",
                InputValue::Color(Color32::RED),
                InputKind::Color,
            ),
            StudyInput::new(
                "source",
                "Source",
                InputValue::Source(StudySource::Price(PriceSource::HL2)),
                InputKind::Source,
            ),
            StudyInput::new(
                "interval",
                "Interval",
                InputValue::Timeframe(Some(Timeframe::Hour1)),
                InputKind::Timeframe,
            ),
            StudyInput::new(
                "symbol",
                "Symbol",
                InputValue::Symbol("TEST".into()),
                InputKind::Symbol,
            ),
        ];
        let mut study = CustomIndicator::new("Inputs", |_| Vec::new())
            .with_inputs(schema, |bars, inputs, source, _| {
                let InputValue::Length(length) = inputs[0].value else {
                    unreachable!()
                };
                assert!(
                    bars.iter()
                        .all(|bar| bar.payload.as_deref() == Some(&serde_json::json!(42))
                            && (bar.open, bar.high, bar.low, bar.close) == (3.0, 4.0, 2.0, 5.0))
                );
                source
                    .iter()
                    .map(|value| IndicatorValue::Single(value * length as f64))
                    .collect()
            })
            .unwrap();
        let mut bar = Bar::new(chrono::DateTime::UNIX_EPOCH, 3.0, 4.0, 2.0, 5.0, 1.0);
        bar.payload = Some(Arc::new(serde_json::json!(42)));
        study.calculate(std::slice::from_ref(&bar));
        for (id, bad) in [
            ("length", InputValue::Length(0)),
            ("length", InputValue::Number(2.0)),
            ("symbol", InputValue::Symbol("\n".into())),
            (
                "source",
                InputValue::Source(StudySource::Study { id: 999, plot: 0 }),
            ),
        ] {
            assert!(study.set_input(id, bad).is_err());
            assert_eq!(study.values().value(0, 0), Some(6.0));
        }
        study.set_input("length", InputValue::Length(4)).unwrap();
        assert!(study.values().is_empty());
        assert_eq!(study.inputs()[0].default, InputValue::Length(2));
        let saved: Vec<_> = study
            .inputs()
            .into_iter()
            .map(|input| (input.id, input.value))
            .collect();
        let restored: Vec<(String, InputValue)> =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let mut cloned = study.clone_box();
        for (id, value) in restored {
            cloned.set_input(&id, value).unwrap();
        }
        cloned.calculate(std::slice::from_ref(&bar));
        assert_eq!(cloned.values().value(0, 0), Some(12.0));
        cloned.update_source(std::slice::from_ref(&bar), 0, &[5.0]);
        assert_eq!(cloned.values().value(0, 0), Some(20.0));
        // A supplied column keeps the real bars and pads missing values without stretching it.
        for (source, expected) in [
            (&[5.0][..], [Some(20.0), None]),
            (&[5.0, 6.0, 7.0][..], [Some(20.0), Some(24.0)]),
            (&[f64::NAN, f64::INFINITY][..], [None, None]),
        ] {
            cloned.update_source(&[bar.clone(), bar.clone()], 0, source);
            assert_eq!(cloned.values().len(), 2);
            assert_eq!(
                [cloned.values().value(0, 0), cloned.values().value(0, 1)],
                expected
            );
        }
        let length = |value| StudyInput::new("length", "Length", value, InputKind::Length(1, 10));
        let reserved = |id: &str| {
            vec![StudyInput::new(
                id,
                "Length",
                InputValue::Length(2),
                InputKind::Length(1, 10),
            )]
        };
        for (schema, expected) in [
            (
                vec![StudyInput::new(
                    "",
                    "Length",
                    InputValue::Length(2),
                    InputKind::Length(1, 10),
                )],
                "study input 0 has an empty id",
            ),
            (
                vec![length(InputValue::Length(2)), length(InputValue::Length(3))],
                "study input \"length\" is declared twice",
            ),
            (
                reserved("plot_offset"),
                "study input \"plot_offset\" is reserved by the chart",
            ),
            (
                reserved("calculation_symbol"),
                "study input \"calculation_symbol\" is reserved by the chart",
            ),
            (
                reserved("calculation_timeframe"),
                "study input \"calculation_timeframe\" is reserved by the chart",
            ),
            (
                reserved("wait_for_close"),
                "study input \"wait_for_close\" is reserved by the chart",
            ),
            (
                vec![length(InputValue::Length(0))],
                "study input \"length\" has value Length(0), expected Length(1, 10)",
            ),
            (
                vec![length(InputValue::Length(2)).with_default(InputValue::Length(11))],
                "study input \"length\" has default Length(11), expected Length(1, 10)",
            ),
            (
                vec![StudyInput::new(
                    "source",
                    "Source",
                    InputValue::Source(StudySource::Study { id: 3, plot: 0 }),
                    InputKind::Source,
                )],
                "study input \"source\" has value Source(Study { id: 3, plot: 0 }), expected a price field",
            ),
            (
                vec![
                    StudyInput::new(
                        "source",
                        "Source",
                        InputValue::Source(StudySource::Price(PriceSource::Close)),
                        InputKind::Source,
                    )
                    .with_default(InputValue::Source(StudySource::Study { id: 3, plot: 0 })),
                ],
                "study input \"source\" has default Source(Study { id: 3, plot: 0 }), expected a price field",
            ),
        ] {
            let error = CustomIndicator::new("Inputs", |_| Vec::new())
                .with_inputs(schema, |_, _, _, _| Vec::new())
                .err()
                .unwrap();
            assert_eq!(error.to_string(), expected);
        }
    }
}
