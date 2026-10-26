//! Technical indicators, calculation helpers and study presentation.
//!
//! Implement [`Indicator`] or use [`CustomIndicator`] for a host-defined study;
//! [`IndicatorRegistry`] calculates a group of studies without a widget. Enable
//! `studies` for the built-in catalogue and `IndicatorFactory`. A chart owns the
//! calculation and rendering lifecycle of studies added through `Chart::add_indicator`.
//!
//! # Calculating a study
//!
//! Outputs contain a contiguous column per line. Missing and warm-up results are
//! NaN; [`IndicatorValues::value`] returns finite results as `Option<f64>`.
//!
//! ```
//! # #[cfg(feature = "studies")] {
//! use egui_charts::{model::Bar, studies::{Indicator, SMA}};
//!
//! let bars = vec![
//!     Bar::new("2024-01-01T00:00:00Z".parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 10.0),
//!     Bar::new("2024-01-01T00:01:00Z".parse().unwrap(), 101.0, 103.0, 100.0, 103.0, 20.0),
//! ];
//! let mut sma = SMA::new(2);
//! sma.calculate(&bars);
//! assert_eq!(sma.values().value(0, 1), Some(102.0));
//! # }
//! ```
//!
//! [`helpers`] provides scalar calculations for custom studies, and [`Stepper`]
//! provides incremental calculation state. [`StudyData`] carries auxiliary data
//! requested from the host's feed.

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
pub use format::StudyScale;
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
pub use settings::{ConfiguredStudy, PlotStyle, PlotType, StudySettings};
pub use table::{StudyCell, StudyTable};
mod values;
pub use values::IndicatorValues;
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

mod chart_state;
pub use chart_state::{ChartState, ZoomState};

#[cfg(feature = "studies")]
mod template;
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use template::StudyTemplate;

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
