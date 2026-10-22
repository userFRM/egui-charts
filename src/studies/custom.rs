//! Runtime-defined custom indicators.
//!
//! The [`CustomIndicator`] type lets users create indicators at runtime by
//! providing a closure that takes `&[Bar]` and returns `Vec<IndicatorValue>`.
//! This is useful for prototyping or embedding user-defined logic without
//! creating a new struct that implements [`Indicator`].
//!
//! # Example
//!
//! ```rust,ignore
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
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use std::sync::Arc;

/// Type alias for the custom calculation closure.
///
/// The closure receives the full bar series and must return exactly one
/// [`IndicatorValue`] per bar.
pub type CalculateFn = Arc<dyn Fn(&[Bar]) -> Vec<IndicatorValue> + Send + Sync>;

/// An indicator defined at runtime via a calculation closure.
///
/// Build one with [`new`](Self::new) and customise it with the builder
/// methods (`with_description`, `with_color`, `with_overlay`, etc.).
#[derive(Clone)]
pub struct CustomIndicator {
    name: String,
    desc: String,
    calculate_fn: CalculateFn,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    is_overlay: bool,
    line_cnt: usize,
    line_names: Vec<String>,
    visible: bool,
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
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.caution], // Yellow default
            is_overlay: true,
            line_cnt: 1,
            visible: true,
        }
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
    pub fn with_line_cnt(mut self, count: usize) -> Self {
        self.line_cnt = count;
        self
    }

    /// Set display names for each line (used in legends).
    pub fn with_line_names(mut self, names: Vec<String>) -> Self {
        self.line_names = names;
        self
    }
}

impl Indicator for CustomIndicator {
    fn name(&self) -> &str {
        &self.name
    }

    fn desc(&self) -> &str {
        &self.desc
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = (self.calculate_fn)(data);
    }

    fn values(&self) -> &[IndicatorValue] {
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

    fn line_cnt(&self) -> usize {
        self.line_cnt
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
    fn clone_keeps_the_calculation() {
        let indicator = CustomIndicator::new("Constant", |_| vec![IndicatorValue::Single(1.0); 3]);

        let mut cloned = indicator.clone_box();
        cloned.calculate(&[]);
        assert_eq!(cloned.values().len(), 3);
    }
}
