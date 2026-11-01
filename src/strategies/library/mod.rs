//! A name-based catalogue of strategies with editable study inputs.

use std::collections::HashMap;

use super::Strategy;
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
/// pyramiding come from [`super::BacktestSettings`]. Built-ins decide only at bar close,
/// including when the engine also recalculates after fills.
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
