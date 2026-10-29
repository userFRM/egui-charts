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

/// Calculation callback receiving the validated current inputs.
pub type InputCalculateFn =
    Arc<dyn Fn(&[Bar], &[super::StudyInput]) -> Vec<IndicatorValue> + Send + Sync>;

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

    /// Declare editable inputs and calculate from their current values. Ids must be unique;
    /// defaults and initial values must satisfy the same schema used by the editor and setters.
    pub fn with_inputs<F>(
        mut self,
        inputs: Vec<super::StudyInput>,
        calculate: F,
    ) -> Result<Self, &'static str>
    where
        F: Fn(&[Bar], &[super::StudyInput]) -> Vec<IndicatorValue> + Send + Sync + 'static,
    {
        for (at, input) in inputs.iter().enumerate() {
            if input.id.is_empty()
                || inputs[..at].iter().any(|other| other.id == input.id)
                || !input.accepts(&input.value)
                || !input.accepts(&input.default)
            {
                return Err("invalid custom study input schema");
            }
        }
        self.inputs = inputs;
        self.input_calculate_fn = Some(Arc::new(calculate));
        self.values.clear();
        Ok(self)
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

impl Indicator for CustomIndicator {
    fn inputs(&self) -> Vec<super::StudyInput> {
        self.inputs.clone()
    }

    fn set_input(&mut self, id: &str, value: super::InputValue) -> Result<(), &'static str> {
        let input = self
            .inputs
            .iter_mut()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !input.accepts(&value) {
            return Err("study input is outside its accepted values");
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
        self.values = match &self.input_calculate_fn {
            Some(calculate) => calculate(data, &self.inputs),
            None => (self.calculate_fn)(data),
        }
        .into();
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
            .with_inputs(schema, |bars, inputs| {
                let InputValue::Length(length) = inputs[0].value else {
                    unreachable!()
                };
                assert!(
                    bars.iter()
                        .all(|bar| bar.payload.as_deref() == Some(&serde_json::json!(42)))
                );
                bars.iter()
                    .map(|bar| IndicatorValue::Single(bar.close * length as f64))
                    .collect()
            })
            .unwrap();
        let mut bar = Bar::new(chrono::DateTime::UNIX_EPOCH, 3.0, 3.0, 3.0, 3.0, 1.0);
        bar.payload = Some(Arc::new(serde_json::json!(42)));
        study.calculate(std::slice::from_ref(&bar));
        for (id, bad) in [
            ("length", InputValue::Length(0)),
            ("length", InputValue::Number(2.0)),
            ("symbol", InputValue::Symbol("\n".into())),
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
        cloned.update_source(&[bar], 0, &[5.0]);
        assert_eq!(cloned.values().value(0, 0), Some(20.0));
    }

    #[test]
    fn clone_keeps_the_calculation() {
        let indicator = CustomIndicator::new("Constant", |_| vec![IndicatorValue::Single(1.0); 3]);

        let mut cloned = indicator.clone_box();
        cloned.calculate(&[]);
        assert_eq!(cloned.values().len(), 3);
    }
}
