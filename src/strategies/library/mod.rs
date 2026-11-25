//! A name-based catalogue of strategies with editable study inputs.

use std::collections::HashMap;

use super::Strategy;
#[cfg(feature = "studies")]
use crate::model::Timeframe;
use crate::studies::{InputKind, InputValue, StudyInput};

mod price_action;
#[cfg(feature = "studies")]
mod studies;
#[cfg(test)]
mod tests;

type StrategyConstructor = Box<dyn Fn() -> Box<dyn Strategy> + Send + Sync>;

/// Creates built-in and host strategies by name.
///
/// Ten price-action strategies need only `strategies`; twelve more use the public study
/// calculations when `studies` is enabled. Inputs are available through
/// [`Strategy::study_inputs`] and edited through [`Strategy::set_input`]. Entry size and
/// pyramiding come from [`super::BacktestSettings`]. SuperTrend suggests 15% of equity,
/// Technical Ratings 5% and Bar Up/Down 10% through [`Strategy::default_order_size`], which the
/// strategy tester applies on selection. Technical Ratings protects positions at 3 ATR(14) from their average
/// price, or with a stop activated at 5 ATR(14) in profit that trails the best price by 2 ATR(14).
/// Its Indicator timeframe input uses the chart interval by default, or the last longer-interval
/// bar closed by each chart bar's end. Higher bars use UTC boundaries and are built from chart
/// bars, so input validation refuses another interval that is shorter, trade-based or not made
/// of whole chart bars, such as 5 minutes on a 3-minute chart or a month on a weekly one.
/// Bar Up/Down's Max Intraday Loss (%) input, 1 by default, cancels its entries and closes its
/// position once the loss since the start of the UTC day, open profit and fees included, exceeds
/// that share of the day's highest equity, and places no entry until the next UTC day.
/// Bollinger Bands Directed trades both sides by default, as Bollinger Bands does, or Long only
/// or Short only, where a signal on the other side closes the position without reversing it.
/// Built-ins ignore fill callbacks and decide
/// at bar close, or on forming-bar updates when tick recalculation is enabled.
///
/// ```
/// use egui_charts::strategies::StrategyFactory;
/// use egui_charts::studies::InputValue;
///
/// let factory = StrategyFactory::new();
/// let mut strategy = factory.create("Consecutive Up/Down").unwrap();
/// strategy.set_input("length", InputValue::Length(3)).unwrap();
/// assert!(!strategy.study_inputs().is_empty());
/// ```
pub struct StrategyFactory {
    constructors: HashMap<String, StrategyConstructor>,
}

impl std::fmt::Debug for StrategyFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrategyFactory")
            .field("constructors", &self.constructors.keys())
            .finish_non_exhaustive()
    }
}

impl StrategyFactory {
    /// Creates a catalogue with every built-in enabled by the current features.
    pub fn new() -> Self {
        let mut factory = Self {
            constructors: HashMap::new(),
        };
        price_action::register(&mut factory);
        #[cfg(feature = "studies")]
        studies::register(&mut factory);
        factory
    }

    /// Registers a host strategy, replacing an entry with the same name.
    pub fn register<F>(&mut self, name: impl Into<String>, constructor: F)
    where
        F: Fn() -> Box<dyn Strategy> + Send + Sync + 'static,
    {
        self.constructors.insert(name.into(), Box::new(constructor));
    }

    /// Creates an independent instance, or `None` for an unknown name.
    pub fn create(&self, name: &str) -> Option<Box<dyn Strategy>> {
        self.constructors.get(name).map(|constructor| constructor())
    }

    /// Lists names in alphabetical order, including host registrations.
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.constructors.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

impl Default for StrategyFactory {
    fn default() -> Self {
        Self::new()
    }
}

fn length_input(id: &str, label: &str, value: usize) -> StudyInput {
    StudyInput::new(
        id,
        label,
        InputValue::Length(value),
        InputKind::Length(1, usize::MAX),
    )
}

fn number_input(id: &str, label: &str, value: f64, min: f64, max: f64) -> StudyInput {
    StudyInput::new(
        id,
        label,
        InputValue::Number(value),
        InputKind::Number(min, max),
    )
}

fn set_input(inputs: &mut [StudyInput], id: &str, value: InputValue) -> Result<(), &'static str> {
    let input = inputs
        .iter_mut()
        .find(|input| input.id == id)
        .ok_or("unknown strategy input")?;
    if !input.accepts(&value) {
        return Err("strategy input is outside its accepted values");
    }
    input.value = value;
    Ok(())
}

fn length(inputs: &[StudyInput], id: &str) -> usize {
    match inputs
        .iter()
        .find(|input| input.id == id)
        .map(|input| &input.value)
    {
        Some(InputValue::Length(value)) => *value,
        _ => unreachable!("declared length input"),
    }
}

fn number(inputs: &[StudyInput], id: &str) -> f64 {
    match inputs
        .iter()
        .find(|input| input.id == id)
        .map(|input| &input.value)
    {
        Some(InputValue::Number(value)) => *value,
        _ => unreachable!("declared numeric input"),
    }
}

#[cfg(feature = "studies")]
fn timeframe(inputs: &[StudyInput], id: &str) -> Option<Timeframe> {
    match inputs
        .iter()
        .find(|input| input.id == id)
        .map(|input| &input.value)
    {
        Some(InputValue::Timeframe(value)) => *value,
        _ => unreachable!("declared timeframe input"),
    }
}
