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
//! let momentum = CustomIndicator::new("Momentum(10)", Box::new(|bars| {
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
//! }))
//! .with_overlay(false)
//! .with_description("Custom 10-bar price momentum");
//! ```
//!
//! # Limitations
//!
//! Because the calculation closure is a `Box<dyn Fn>`, it cannot be truly
//! cloned. [`Indicator::clone_box`] produces a copy that retains the cached
//! values but replaces the closure with a no-op placeholder.

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Type alias for the custom calculation closure.
///
/// The closure receives the full bar series and must return exactly one
/// [`IndicatorValue`] per bar.
pub type CalculateFn = Box<dyn Fn(&[Bar]) -> Vec<IndicatorValue> + Send>;

/// An indicator defined at runtime via a calculation closure.
///
/// Build one with [`new`](Self::new) and customise it with the builder
/// methods (`with_description`, `with_color`, `with_overlay`, etc.).
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
    pub fn new(name: impl Into<String>, calculate_fn: CalculateFn) -> Self {
        let name = name.into();
        Self {
            line_names: vec![name.clone()],
            name,
            desc: String::new(),
            calculate_fn,
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
        // Note: Custom indicators with closures can't be fully cloned
        // This is a limitation of the current design
        // For production use, consider using a different approach
        Box::new(Self {
            name: self.name.clone(),
            desc: self.desc.clone(),
            calculate_fn: Box::new(|_| Vec::new()), // Placeholder
            values: self.values.clone(),
            colors: self.colors.clone(),
            is_overlay: self.is_overlay,
            line_cnt: self.line_cnt,
            line_names: self.line_names.clone(),
            visible: self.visible,
        })
    }
}
