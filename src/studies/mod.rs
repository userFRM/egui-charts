//! Technical analysis indicators (studies) module.
//!
//! This module provides technical indicators commonly used in financial
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
//! 3. **Reading results** -- Call [`Indicator::values`] to retrieve [`IndicatorValues`],
//!    one contiguous `Vec<f64>` per output line. NaN represents missing values, including
//!    warm-up. [`IndicatorValues::value`] returns finite values as `Option<f64>`.
//! 4. **Rendering** -- The chart engine reads [`Indicator::is_overlay`] to
//!    decide whether the indicator is drawn on the main price pane or in a
//!    separate sub-pane, and uses [`Indicator::colors`] /
//!    [`Indicator::line_names`] for styling and legends.
//!
//! # Module layout
//!
//! | Sub-module | Purpose |
//! |---|---|
//! | `builtin` | *(feature `studies`)* All built-in indicators ([`SMA`], [`EMA`], [`RSI`], [`MACD`], [`BollingerBands`], ...) |
//! | [`factory`] | *(feature `studies`)* [`IndicatorFactory`] -- dynamic creation of indicators by name |
//! | `indicator_trait` | The core [`Indicator`] trait and [`IndicatorValue`] enum |
//! | `custom` | [`CustomIndicator`] -- define indicators at runtime with closures |
//!
//! # Quick start
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{BollingerBands, Indicator, IndicatorRegistry, RSI, SMA};
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

#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
mod builtin;
mod custom;
mod data;
pub use data::{StudyData, StudyDataRequest};
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub mod factory;
mod format;
pub mod helpers;
mod indicator_trait;
mod order_flow;
pub use format::{StudyFormat, StudyScale};
pub use order_flow::{OrderFlowInput, OrderFlowKind};
mod inputs;
mod metric;
pub use inputs::{InputKind, InputValue, StudyInput, StudySource};
pub use metric::MetricStudy;
pub(crate) mod plots;
pub(crate) mod projection;
mod settings;
pub(crate) mod table;
pub use plots::{FillBoundary, StudyCandle, StudyFill, StudyZone};
pub use projection::StudyProjection;
pub use settings::{ConfiguredStudy, PlotPattern, PlotStyle, PlotType, StudySettings};
pub use table::{StudyCell, StudyTable};
mod volume_profile;
pub use volume_profile::{ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig};
mod values;
pub use values::IndicatorValues;
#[cfg(any(feature = "studies", feature = "drawings"))]
pub(crate) mod math;
#[cfg(all(test, feature = "studies"))]
mod reference;
mod stepper;
pub use stepper::Stepper;

#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use builtin::*;
pub use custom::CustomIndicator;
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use factory::IndicatorFactory;
pub use indicator_trait::{
    CrossDirection, Indicator, IndicatorFill, IndicatorLineStyle, IndicatorMarker, IndicatorValue,
    StudyPane,
};

use crate::model::Bar;

/// A registry that owns a collection of [`Indicator`] trait objects and
/// provides batch operations (calculate all, clear, etc.).
///
/// Use the registry when you want to manage multiple indicators as a group
/// and recalculate them together whenever the underlying bar data changes.
///
/// # Example
///
/// ```rust
/// # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
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

    /// Bring every indicator up to date after `data[from..]` changed or grew,
    /// given that `data[..from]` is unchanged since the previous call.
    ///
    /// Calls [`Indicator::update`] on each one; studies that are not
    /// incremental recompute in full.
    pub fn update_all(&mut self, data: &[Bar], from: usize) {
        for indicator in &mut self.indicators {
            indicator.update(data, from);
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
