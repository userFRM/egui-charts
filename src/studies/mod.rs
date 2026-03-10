//! Technical analysis indicators (studies) module.
//!
//! This module provides 150+ technical indicators commonly used in financial
//! charting applications, covering moving averages, momentum oscillators,
//! volatility measures, volume analysis, trend detection, and more.
//!
//! # Architecture
//!
//! Every indicator implements the [`Indicator`] trait, which defines a
//! uniform lifecycle:
//!
//! 1. **Construction** -- Create an indicator with its parameters
//!    (e.g. `SMA::new(20)`, `RSI::new(14)`, `MACD::new(12, 26, 9)`).
//! 2. **Calculation** -- Call [`Indicator::calculate`] with a slice of [`Bar`]
//!    data. The indicator computes its output values in one pass.
//! 3. **Reading results** -- Call [`Indicator::values`] to retrieve computed
//!    [`IndicatorValue`]s (one per input bar). Single-line indicators emit
//!    `IndicatorValue::Single(f64)`, multi-line indicators emit
//!    `IndicatorValue::Multiple(Vec<f64>)`, and bars before the warmup period
//!    are `IndicatorValue::None`.
//! 4. **Rendering** -- The chart engine reads [`Indicator::is_overlay`] to
//!    decide whether the indicator is drawn on the main price pane or in a
//!    separate sub-pane, and uses [`Indicator::colors`] /
//!    [`Indicator::line_names`] for styling and legends.
//!
//! # Module layout
//!
//! | Sub-module | Purpose |
//! |---|---|
//! | `builtin` | All built-in indicators ([`SMA`], [`EMA`], [`RSI`], [`MACD`], [`BollingerBands`], ...) |
//! | [`factory`] | [`IndicatorFactory`] -- dynamic creation of indicators by name |
//! | `indicator_trait` | The core [`Indicator`] trait and [`IndicatorValue`] enum |
//! | `palette` | [`IndicatorPalette`], [`IndicatorColorSchemes`], [`ColorCategory`] |
//! | `custom` | [`CustomIndicator`] -- define indicators at runtime with closures |
//!
//! # Quick start
//!
//! ```rust,ignore
//! use egui_charts::studies::{SMA, RSI, BollingerBands, Indicator};
//!
//! // Create indicators
//! let mut sma = SMA::new(20);
//! let mut rsi = RSI::new(14);
//! let mut bb  = BollingerBands::new(20, 2.0);
//!
//! // Calculate on bar data
//! sma.calculate(&bars);
//! rsi.calculate(&bars);
//! bb.calculate(&bars);
//!
//! // Or use the registry for batch calculation
//! let mut registry = IndicatorRegistry::new();
//! registry.add(Box::new(SMA::new(50)));
//! registry.add(Box::new(RSI::new(14)));
//! registry.calculate_all(&bars);
//! ```

mod builtin;
mod custom;
pub mod factory;
mod indicator_trait;
mod palette;

pub use builtin::*;
pub use custom::CustomIndicator;
pub use factory::IndicatorFactory;
pub use indicator_trait::{Indicator, IndicatorValue};
pub use palette::{ColorCategory, IndicatorColorSchemes, IndicatorPalette};

use crate::model::Bar;

/// A registry that owns a collection of [`Indicator`] trait objects and
/// provides batch operations (calculate all, clear, etc.).
///
/// Use the registry when you want to manage multiple indicators as a group
/// and recalculate them together whenever the underlying bar data changes.
///
/// # Example
///
/// ```rust,ignore
/// use egui_charts::studies::{IndicatorRegistry, SMA, EMA};
///
/// let mut registry = IndicatorRegistry::new();
/// registry.add(Box::new(SMA::new(20)));
/// registry.add(Box::new(EMA::new(50)));
/// registry.calculate_all(&bars);
///
/// for indicator in registry.indicators() {
///     println!("{}: {} values", indicator.name(), indicator.values().len());
/// }
/// ```
pub struct IndicatorRegistry {
    indicators: Vec<Box<dyn Indicator>>,
}

impl IndicatorRegistry {
    /// Create an empty registry with no indicators.
    pub fn new() -> Self {
        Self {
            indicators: Vec::new(),
        }
    }

    /// Append a boxed indicator to the registry.
    pub fn add_indicator(&mut self, indicator: Box<dyn Indicator>) {
        self.indicators.push(indicator);
    }

    /// Alias for [`add_indicator`](Self::add_indicator) for brevity.
    pub fn add(&mut self, indicator: Box<dyn Indicator>) {
        self.add_indicator(indicator);
    }

    /// Remove and return the indicator at `index`, or `None` if out of bounds.
    pub fn remove_indicator(&mut self, index: usize) -> Option<Box<dyn Indicator>> {
        if index < self.indicators.len() {
            Some(self.indicators.remove(index))
        } else {
            None
        }
    }

    /// Return a shared slice of all registered indicators.
    pub fn indicators(&self) -> &[Box<dyn Indicator>] {
        &self.indicators
    }

    /// Return a mutable slice of all registered indicators.
    pub fn indicators_mut(&mut self) -> &mut [Box<dyn Indicator>] {
        &mut self.indicators
    }

    /// Recalculate every indicator against the provided bar data.
    ///
    /// This iterates through all registered indicators and calls
    /// [`Indicator::calculate`] on each one.
    pub fn calculate_all(&mut self, data: &[Bar]) {
        for indicator in &mut self.indicators {
            indicator.calculate(data);
        }
    }

    /// Remove all indicators from the registry.
    pub fn clear(&mut self) {
        self.indicators.clear();
    }
}

impl Default for IndicatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
