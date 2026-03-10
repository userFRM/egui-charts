//! Dual price scale manager for left/right Y-axis support.
//!
//! Professional charting platforms allow two independent price axes -- one on
//! each side of the chart -- so that series with different value ranges
//! (e.g. price on the right, RSI on the left) can share the same pane.
//!
//! [`DualPriceScaleManager`] owns both scales, tracks which series is assigned
//! to which axis, and delegates coordinate transformations accordingly.

/// Dual Price Scale Manager for left/right price axes.
///
/// Provides support for multiple price scales positioned on left and right
/// sides of the chart, with independent scale modes and series assignments.
use std::collections::HashMap;

use super::{PriceRange, PriceScale, PriceScaleMode, PriceScaleOptions};

/// Price scale position identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PriceScaleId {
    /// Left price scale (typically for indicators, percentage mode)
    Left,
    /// Right price scale (primary, typically for main series)
    #[default]
    Right,
}

impl PriceScaleId {
    /// Returns the opposite scale
    pub fn opposite(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

impl std::fmt::Display for PriceScaleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Left => write!(f, "left"),
            Self::Right => write!(f, "right"),
        }
    }
}

/// Configuration for a positioned price scale
#[derive(Debug, Clone)]
pub struct PosedPriceScaleOptions {
    /// Base price scale options
    pub options: PriceScaleOptions,
    /// Width of this scale axis in pixels
    pub width: f32,
    /// Whether to show border line between scale and chart
    pub show_border: bool,
}

impl Default for PosedPriceScaleOptions {
    fn default() -> Self {
        Self {
            options: PriceScaleOptions::default(),
            width: 70.0,
            show_border: true,
        }
    }
}

/// Series-to-scale assignment
#[derive(Debug, Clone)]
pub struct SeriesScaleAssignment {
    /// Series identifier (e.g., "BTCUSDT", "RSI", "volume")
    pub series_id: String,
    /// Which price scale this series uses
    pub scale_id: PriceScaleId,
}

/// Dual Price Scale Manager
///
/// Manages left and right price scales with:
/// - Independent scale options and modes
/// - Series-to-scale assignments
/// - Auto-scaling per scale based on assigned series
pub struct DualPriceScaleManager {
    /// Left price scale
    left_scale: PriceScale,
    /// Right price scale
    right_scale: PriceScale,
    /// Left scale options
    left_options: PosedPriceScaleOptions,
    /// Right scale options
    right_options: PosedPriceScaleOptions,
    /// Series assignments (series_id -> scale_id)
    assignments: HashMap<String, PriceScaleId>,
    /// Whether left scale is visible
    left_visible: bool,
    /// Whether right scale is visible
    right_visible: bool,
}

impl Default for DualPriceScaleManager {
    fn default() -> Self {
        Self::new(100.0)
    }
}

impl DualPriceScaleManager {
    /// Create a new dual price scale manager
    pub fn new(height: f32) -> Self {
        Self {
            left_scale: PriceScale::new(height),
            right_scale: PriceScale::new(height),
            left_options: PosedPriceScaleOptions {
                options: PriceScaleOptions {
                    visible: false, // Left hidden by default
                    ..Default::default()
                },
                ..Default::default()
            },
            right_options: PosedPriceScaleOptions::default(),
            assignments: HashMap::new(),
            left_visible: false,
            right_visible: true,
        }
    }

    /// Create with specific options for each scale
    pub fn with_options(
        height: f32,
        left_options: PosedPriceScaleOptions,
        right_options: PosedPriceScaleOptions,
    ) -> Self {
        let left_visible = left_options.options.visible;
        let right_visible = right_options.options.visible;

        Self {
            left_scale: PriceScale::with_options(height, left_options.options),
            right_scale: PriceScale::with_options(height, right_options.options),
            left_options,
            right_options,
            assignments: HashMap::new(),
            left_visible,
            right_visible,
        }
    }

    /// Set height for both scales (call on resize)
    pub fn set_height(&mut self, height: f32) {
        self.left_scale.set_height(height);
        self.right_scale.set_height(height);
    }

    /// Get the scale for a given position
    pub fn get_scale(&self, id: PriceScaleId) -> &PriceScale {
        match id {
            PriceScaleId::Left => &self.left_scale,
            PriceScaleId::Right => &self.right_scale,
        }
    }

    /// Get mutable reference to a scale
    pub fn get_scale_mut(&mut self, id: PriceScaleId) -> &mut PriceScale {
        match id {
            PriceScaleId::Left => &mut self.left_scale,
            PriceScaleId::Right => &mut self.right_scale,
        }
    }

    /// Get options for a scale position
    pub fn get_options(&self, id: PriceScaleId) -> &PosedPriceScaleOptions {
        match id {
            PriceScaleId::Left => &self.left_options,
            PriceScaleId::Right => &self.right_options,
        }
    }

    /// Set options for a scale position
    pub fn set_options(&mut self, id: PriceScaleId, options: PosedPriceScaleOptions) {
        let visible = options.options.visible;
        match id {
            PriceScaleId::Left => {
                self.left_scale.set_options(options.options);
                self.left_options = options;
                self.left_visible = visible;
            }
            PriceScaleId::Right => {
                self.right_scale.set_options(options.options);
                self.right_options = options;
                self.right_visible = visible;
            }
        }
    }

    /// Check if a scale is visible
    pub fn is_visible(&self, id: PriceScaleId) -> bool {
        match id {
            PriceScaleId::Left => self.left_visible,
            PriceScaleId::Right => self.right_visible,
        }
    }

    /// Set visibility for a scale
    pub fn set_visible(&mut self, id: PriceScaleId, visible: bool) {
        match id {
            PriceScaleId::Left => self.left_visible = visible,
            PriceScaleId::Right => self.right_visible = visible,
        }
    }

    /// Get width of a scale axis
    pub fn get_width(&self, id: PriceScaleId) -> f32 {
        if !self.is_visible(id) {
            return 0.0;
        }
        match id {
            PriceScaleId::Left => self.left_options.width,
            PriceScaleId::Right => self.right_options.width,
        }
    }

    /// Get total left padding (for chart layout)
    pub fn left_padding(&self) -> f32 {
        if self.left_visible {
            self.left_options.width
        } else {
            0.0
        }
    }

    /// Get total right padding (for chart layout)
    pub fn right_padding(&self) -> f32 {
        if self.right_visible {
            self.right_options.width
        } else {
            0.0
        }
    }

    /// Assign a series to a specific scale
    pub fn assign_series(&mut self, series_id: impl Into<String>, scale_id: PriceScaleId) {
        self.assignments.insert(series_id.into(), scale_id);
    }

    /// Remove a series assignment
    pub fn unassign_series(&mut self, series_id: &str) {
        self.assignments.remove(series_id);
    }

    /// Get which scale a series is assigned to (defaults to Right)
    pub fn get_series_scale(&self, series_id: &str) -> PriceScaleId {
        self.assignments
            .get(series_id)
            .copied()
            .unwrap_or(PriceScaleId::Right)
    }

    /// Get all series assigned to a specific scale
    pub fn get_assigned_series(&self, scale_id: PriceScaleId) -> Vec<&str> {
        self.assignments
            .iter()
            .filter_map(|(id, &assigned)| {
                if assigned == scale_id {
                    Some(id.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Auto-scale a specific scale based on provided data range
    pub fn auto_scale(&mut self, id: PriceScaleId, data_min: f64, data_max: f64) {
        self.get_scale_mut(id).auto_scale(data_min, data_max);
    }

    /// Set first value for percentage/indexed modes on a scale
    pub fn set_first_val(&mut self, id: PriceScaleId, value: f64) {
        self.get_scale_mut(id).set_first_val(value);
    }

    /// Get price-to-coord transformation for a series
    pub fn price_to_coord(&self, series_id: &str, price: f64) -> f32 {
        let scale_id = self.get_series_scale(series_id);
        self.get_scale(scale_id).price_to_coord(price)
    }

    /// Get coord-to-price transformation for a series
    pub fn coord_to_price(&self, series_id: &str, y: f32) -> f64 {
        let scale_id = self.get_series_scale(series_id);
        self.get_scale(scale_id).coord_to_price(y)
    }

    /// Get the price range for a scale
    pub fn price_range(&self, id: PriceScaleId) -> PriceRange {
        self.get_scale(id).price_range()
    }

    /// Get scale mode for a position
    pub fn get_mode(&self, id: PriceScaleId) -> PriceScaleMode {
        match id {
            PriceScaleId::Left => self.left_options.options.mode,
            PriceScaleId::Right => self.right_options.options.mode,
        }
    }

    /// Set scale mode for a position
    pub fn set_mode(&mut self, id: PriceScaleId, mode: PriceScaleMode) {
        match id {
            PriceScaleId::Left => {
                self.left_options.options.mode = mode;
                self.left_scale.set_options(self.left_options.options);
            }
            PriceScaleId::Right => {
                self.right_options.options.mode = mode;
                self.right_scale.set_options(self.right_options.options);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configuration() {
        let manager = DualPriceScaleManager::new(400.0);

        // Right visible by default, left hidden
        assert!(manager.is_visible(PriceScaleId::Right));
        assert!(!manager.is_visible(PriceScaleId::Left));

        // Padding reflects visibility
        assert!(manager.right_padding() > 0.0);
        assert_eq!(manager.left_padding(), 0.0);
    }

    #[test]
    fn test_series_assignment() {
        let mut manager = DualPriceScaleManager::new(400.0);

        // Default assignment is right
        assert_eq!(manager.get_series_scale("BTCUSDT"), PriceScaleId::Right);

        // Assign to left
        manager.assign_series("RSI", PriceScaleId::Left);
        assert_eq!(manager.get_series_scale("RSI"), PriceScaleId::Left);

        // Get assigned series
        let left_series = manager.get_assigned_series(PriceScaleId::Left);
        assert!(left_series.contains(&"RSI"));
    }

    #[test]
    fn test_dual_scale_modes() {
        let mut manager = DualPriceScaleManager::new(400.0);

        // Set different modes for each scale
        manager.set_mode(PriceScaleId::Right, PriceScaleMode::Normal);
        manager.set_mode(PriceScaleId::Left, PriceScaleMode::Percentage);

        assert_eq!(
            manager.get_mode(PriceScaleId::Right),
            PriceScaleMode::Normal
        );
        assert_eq!(
            manager.get_mode(PriceScaleId::Left),
            PriceScaleMode::Percentage
        );
    }

    #[test]
    fn test_independent_auto_scale() {
        let mut manager = DualPriceScaleManager::new(400.0);

        // Auto-scale each independently
        manager.auto_scale(PriceScaleId::Right, 100.0, 200.0);
        manager.auto_scale(PriceScaleId::Left, 0.0, 100.0);

        let right_range = manager.price_range(PriceScaleId::Right);
        let left_range = manager.price_range(PriceScaleId::Left);

        // Ranges should be different
        assert!(right_range.min > left_range.min);
    }
}
