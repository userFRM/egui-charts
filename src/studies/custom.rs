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
//! the original does.

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::sync::Arc;

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
    /// Histories that have not arrived are absent. A study declaring histories has no
    /// [`Indicator::source`]; its `source` input, if present, reads a chart-bar price field.
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
    ///         bars.iter().map(|bar| {
    ///             history
    ///                 .and_then(|history| Some(history.bars[history.bar_at(bar.time)?].close))
    ///                 .map_or(IndicatorValue::None, IndicatorValue::Single)
    ///         }).collect()
    ///     })
    ///     .unwrap()
    ///     .with_data_requests(hourly);
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

    /// Set a per-bar colour callback. Returning `None` uses the plot colour.
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
        self.values.clear();
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
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        self.plot_color
            .as_ref()
            .and_then(|color| color(line, bar, &self.values))
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
        self.run(data, None, &[]);
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

    fn update_data(&mut self, data: &[Bar], _from: usize, sources: &[super::StudyData<'_>]) {
        self.run(data, None, sources);
    }

    fn update_source(&mut self, data: &[Bar], _from: usize, source: &[f64]) {
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
