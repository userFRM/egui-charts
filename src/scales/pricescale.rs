//! Price scale engine -- maps price values to Y-axis pixel coordinates.
//!
//! Supports four scaling modes:
//!
//! | Mode            | Axis labels             | Use case                         |
//! |-----------------|-------------------------|----------------------------------|
//! | Normal          | Absolute prices         | Default for most instruments     |
//! | Logarithmic     | Log-spaced prices       | Long-term trends, large ranges   |
//! | Percentage      | `+5.2%`, `-3.1%`        | Comparing relative performance   |
//! | IndexedTo100    | Rebased to 100          | Portfolio benchmarking           |

/// Price Scale Engine.
///
/// Reference: lightweight-charts/src/model/price-scale.ts.
/// Provides price scaling modes and auto-scale behavior.
use std::f64::consts::E;
use std::fmt;
use std::str::FromStr;

/// Scaling mode for the price (Y) axis.
///
/// Determines how raw price values are transformed before being mapped to
/// pixel coordinates.  The default is [`Normal`](PriceScaleMode::Normal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceScaleMode {
    /// Price scale shows prices. Price range changes linearly.
    #[default]
    Normal,
    /// Price scale shows prices. Price range changes logarithmically.
    Logarithmic,
    /// Price scale shows percentage values according to the first visible value.
    /// The first visible value is 0% in this mode.
    Percentage,
    /// The same as percentage mode, but the first value is moved to 100.
    IndexedTo100,
}

impl fmt::Display for PriceScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PriceScaleMode::Normal => write!(f, "Normal"),
            PriceScaleMode::Logarithmic => write!(f, "Logarithmic"),
            PriceScaleMode::Percentage => write!(f, "Percentage"),
            PriceScaleMode::IndexedTo100 => write!(f, "Indexed to 100"),
        }
    }
}

impl FromStr for PriceScaleMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" => Ok(PriceScaleMode::Normal),
            "logarithmic" | "log" => Ok(PriceScaleMode::Logarithmic),
            "percentage" | "percent" | "%" => Ok(PriceScaleMode::Percentage),
            "indexedto100" | "indexed to 100" | "indexed" => Ok(PriceScaleMode::IndexedTo100),
            _ => Err(format!("Invalid price scale mode: {s}")),
        }
    }
}

/// A min/max price range, used throughout the scale engine.
#[derive(Debug, Clone, Copy)]
pub struct PriceRange {
    /// Minimum price in the range.
    pub min: f64,
    /// Maximum price in the range.
    pub max: f64,
}

impl PriceRange {
    /// Creates a new price range.
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// Returns the size of the range (`max - min`), clamped to a minimum of `1e-12`.
    pub fn length(&self) -> f64 {
        (self.max - self.min).max(1e-12)
    }

    /// Returns `true` if `price` is within `[min, max]` inclusive.
    pub fn contains(&self, price: f64) -> bool {
        price >= self.min && price <= self.max
    }
}

/// Top and bottom margins for the price scale, expressed as fractions of the
/// visible data range.
#[derive(Debug, Clone, Copy)]
pub struct PriceScaleMargins {
    /// Top margin in percentages (0.0 to 1.0)
    pub top: f32,
    /// Bottom margin in percentages (0.0 to 1.0)
    pub bottom: f32,
}

impl Default for PriceScaleMargins {
    fn default() -> Self {
        Self {
            top: 0.2,
            bottom: 0.1,
        }
    }
}

/// Configuration options for a [`PriceScale`].
#[derive(Debug, Clone, Copy)]
pub struct PriceScaleOptions {
    /// Auto-scale to fit visible data
    pub auto_scale: bool,

    /// Price scale mode
    pub mode: PriceScaleMode,

    /// Invert the price scale (uptrends become downtrends)
    pub invert_scale: bool,

    /// Margins (top and bottom)
    pub scale_margins: PriceScaleMargins,

    /// Align labels to prevent overlapping
    pub align_labels: bool,

    /// Draw border between price scale and chart
    pub border_visible: bool,

    /// Show price scale
    pub visible: bool,

    /// Draw small horizontal lines on price axis labels
    pub ticks_visible: bool,

    /// Show only entire text labels
    pub entire_text_only: bool,
}

impl Default for PriceScaleOptions {
    fn default() -> Self {
        Self {
            auto_scale: true,
            mode: PriceScaleMode::Normal,
            invert_scale: false,
            scale_margins: PriceScaleMargins::default(),
            align_labels: true,
            border_visible: true,
            visible: true,
            ticks_visible: false,
            entire_text_only: false,
        }
    }
}

/// Price Scale Engine
///
/// Handles price-to-coord transformations with multiple modes:
/// - Normal: Linear price scale
/// - Logarithmic: Logarithmic price scale
/// - Percentage: Percentage change from first visible value (0% baseline)
/// - IndexedTo100: Percentage change from first visible value (100 baseline)
pub struct PriceScale {
    options: PriceScaleOptions,

    /// Current price range (in original price units)
    price_range: PriceRange,

    /// Height in pixels
    height: f32,

    /// First visible value (for Percentage/IndexedTo100 modes)
    first_val: Option<f64>,

    /// Manual price range override (when auto_scale is false)
    manual_range: Option<PriceRange>,
}

impl PriceScale {
    /// Creates a new price scale with default options and the given pixel height.
    pub fn new(height: f32) -> Self {
        Self {
            options: PriceScaleOptions::default(),
            price_range: PriceRange::new(0.0, 100.0),
            height,
            first_val: None,
            manual_range: None,
        }
    }

    /// Creates a new price scale with explicit options and the given pixel height.
    pub fn with_options(height: f32, options: PriceScaleOptions) -> Self {
        Self {
            options,
            price_range: PriceRange::new(0.0, 100.0),
            height,
            first_val: None,
            manual_range: None,
        }
    }

    /// Set height (call when chart is resized)
    pub fn set_height(&mut self, height: f32) {
        self.height = height;
    }

    /// Set options
    pub fn set_options(&mut self, options: PriceScaleOptions) {
        self.options = options;
    }

    /// Set first visible value (for Percentage/IndexedTo100 modes)
    pub fn set_first_val(&mut self, value: f64) {
        self.first_val = Some(value);
    }

    /// Auto-scale to fit the given price range
    pub fn auto_scale(&mut self, data_min: f64, data_max: f64) {
        if !self.options.auto_scale {
            // Use manual range if set
            if let Some(manual) = self.manual_range {
                self.price_range = manual;
                return;
            }
        }

        let range = (data_max - data_min).max(1e-12);
        let top_margin = range * self.options.scale_margins.top as f64;
        let bottom_margin = range * self.options.scale_margins.bottom as f64;

        self.price_range = PriceRange::new(data_min - bottom_margin, data_max + top_margin);
    }

    /// Set manual price range (disables auto-scale)
    pub fn set_manual_range(&mut self, min: f64, max: f64) {
        self.manual_range = Some(PriceRange::new(min, max));
        self.options.auto_scale = false;
    }

    /// Reset to auto-scale
    pub fn reset_auto_scale(&mut self) {
        self.manual_range = None;
        self.options.auto_scale = true;
    }

    /// Convert price to Y coord
    pub fn price_to_coord(&self, price: f64) -> f32 {
        let normalized = self.normalize_price(price);
        let ratio = self.price_to_ratio(normalized);

        // Invert Y axis (top = 0, bottom = height)
        let y = if self.options.invert_scale {
            ratio as f32 * self.height
        } else {
            (1.0 - ratio as f32) * self.height
        };

        y.clamp(0.0, self.height)
    }

    /// Convert Y coord to price
    pub fn coord_to_price(&self, y: f32) -> f64 {
        let ratio = if self.options.invert_scale {
            (y / self.height) as f64
        } else {
            (1.0 - y / self.height) as f64
        };

        let normalized = self.ratio_to_price(ratio);
        self.denormalize_price(normalized)
    }

    /// Get current price range
    pub fn price_range(&self) -> PriceRange {
        self.price_range
    }

    /// Get visible price range (accounting for mode transformations)
    pub fn visible_price_range(&self) -> PriceRange {
        match self.options.mode {
            PriceScaleMode::Normal => self.price_range,
            PriceScaleMode::Logarithmic => {
                // Return log-transformed range
                PriceRange::new(
                    self.price_to_log(self.price_range.min),
                    self.price_to_log(self.price_range.max),
                )
            }
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    PriceRange::new(
                        self.price_to_percent(self.price_range.min, first),
                        self.price_to_percent(self.price_range.max, first),
                    )
                } else {
                    self.price_range
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    PriceRange::new(
                        self.price_to_idxed(self.price_range.min, first),
                        self.price_to_idxed(self.price_range.max, first),
                    )
                } else {
                    self.price_range
                }
            }
        }
    }

    // ============= Internal transformation methods =============

    /// Normalize price according to current mode (price -> normalized)
    pub(crate) fn normalize_price(&self, price: f64) -> f64 {
        match self.options.mode {
            PriceScaleMode::Normal => price,
            PriceScaleMode::Logarithmic => self.price_to_log(price),
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    self.price_to_percent(price, first)
                } else {
                    price
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    self.price_to_idxed(price, first)
                } else {
                    price
                }
            }
        }
    }

    /// Denormalize price according to current mode (normalized -> price)
    fn denormalize_price(&self, normalized: f64) -> f64 {
        match self.options.mode {
            PriceScaleMode::Normal => normalized,
            PriceScaleMode::Logarithmic => self.log_to_price(normalized),
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    self.percent_to_price(normalized, first)
                } else {
                    normalized
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    self.indexed_to_price(normalized, first)
                } else {
                    normalized
                }
            }
        }
    }

    /// Convert normalized price to ratio in range [0, 1]
    fn price_to_ratio(&self, normalized_price: f64) -> f64 {
        let normalized_range = match self.options.mode {
            PriceScaleMode::Normal => self.price_range.length(),
            PriceScaleMode::Logarithmic => {
                self.price_to_log(self.price_range.max) - self.price_to_log(self.price_range.min)
            }
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    self.price_to_percent(self.price_range.max, first)
                        - self.price_to_percent(self.price_range.min, first)
                } else {
                    self.price_range.length()
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    self.price_to_idxed(self.price_range.max, first)
                        - self.price_to_idxed(self.price_range.min, first)
                } else {
                    self.price_range.length()
                }
            }
        };

        let normalized_min = match self.options.mode {
            PriceScaleMode::Normal => self.price_range.min,
            PriceScaleMode::Logarithmic => self.price_to_log(self.price_range.min),
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    self.price_to_percent(self.price_range.min, first)
                } else {
                    self.price_range.min
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    self.price_to_idxed(self.price_range.min, first)
                } else {
                    self.price_range.min
                }
            }
        };

        ((normalized_price - normalized_min) / normalized_range.max(1e-12)).clamp(0.0, 1.0)
    }

    /// Convert ratio [0, 1] to normalized price
    fn ratio_to_price(&self, ratio: f64) -> f64 {
        let ratio_clamped = ratio.clamp(0.0, 1.0);

        match self.options.mode {
            PriceScaleMode::Normal => {
                self.price_range.min + ratio_clamped * self.price_range.length()
            }
            PriceScaleMode::Logarithmic => {
                let log_min = self.price_to_log(self.price_range.min);
                let log_max = self.price_to_log(self.price_range.max);
                log_min + ratio_clamped * (log_max - log_min)
            }
            PriceScaleMode::Percentage => {
                if let Some(first) = self.first_val {
                    let percent_min = self.price_to_percent(self.price_range.min, first);
                    let percent_max = self.price_to_percent(self.price_range.max, first);
                    percent_min + ratio_clamped * (percent_max - percent_min)
                } else {
                    self.price_range.min + ratio_clamped * self.price_range.length()
                }
            }
            PriceScaleMode::IndexedTo100 => {
                if let Some(first) = self.first_val {
                    let indexed_min = self.price_to_idxed(self.price_range.min, first);
                    let indexed_max = self.price_to_idxed(self.price_range.max, first);
                    indexed_min + ratio_clamped * (indexed_max - indexed_min)
                } else {
                    self.price_range.min + ratio_clamped * self.price_range.length()
                }
            }
        }
    }

    // ============= Mode-specific transformations =============

    /// Logarithmic: price -> log(price)
    fn price_to_log(&self, price: f64) -> f64 {
        if price <= 0.0 {
            return 0.0;
        }
        price.ln() / E.ln()
    }

    /// Logarithmic: log(price) -> price
    fn log_to_price(&self, log_price: f64) -> f64 {
        E.powf(log_price)
    }

    /// Percentage: price -> percentage change from first value
    fn price_to_percent(&self, price: f64, first_val: f64) -> f64 {
        if first_val == 0.0 {
            return 0.0;
        }
        ((price - first_val) / first_val.abs()) * 100.0
    }

    /// Percentage: percentage -> price
    fn percent_to_price(&self, percent: f64, first_val: f64) -> f64 {
        first_val + (first_val * percent / 100.0)
    }

    /// IndexedTo100: price -> indexed to 100
    fn price_to_idxed(&self, price: f64, first_val: f64) -> f64 {
        if first_val == 0.0 {
            return 100.0;
        }
        (price / first_val) * 100.0
    }

    /// IndexedTo100: indexed -> price
    fn indexed_to_price(&self, indexed: f64, first_val: f64) -> f64 {
        (indexed / 100.0) * first_val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_mode() {
        let mut scale = PriceScale::new(100.0);
        scale.auto_scale(0.0, 100.0);

        // Price 0 should be at bottom (y = height)
        // Price 100 should be at top (y = 0)
        let y_min = scale.price_to_coord(0.0);
        let y_max = scale.price_to_coord(100.0);

        assert!(y_min > y_max, "Lower price should have higher Y coord");

        // Test reverse
        let price_from_y = scale.coord_to_price(y_min);
        assert!(
            (price_from_y - 0.0).abs() < 1.0,
            "Should convert back to ~0"
        );
    }

    #[test]
    fn test_logarithmic_mode() {
        let mut scale = PriceScale::new(100.0);
        scale.set_options(PriceScaleOptions {
            mode: PriceScaleMode::Logarithmic,
            ..Default::default()
        });
        scale.auto_scale(1.0, 100.0);

        // Logarithmic scale should compress higher values
        let y_10 = scale.price_to_coord(10.0);
        let y_100 = scale.price_to_coord(100.0);

        assert!(y_10 > y_100);
    }

    #[test]
    fn test_percentage_mode() {
        let mut scale = PriceScale::new(100.0);
        scale.set_options(PriceScaleOptions {
            mode: PriceScaleMode::Percentage,
            ..Default::default()
        });
        scale.set_first_val(100.0);
        scale.auto_scale(90.0, 110.0);

        // Test price transformation
        // 90 is -10%, 110 is +10%, 100 is 0%
        let percent_90 = scale.normalize_price(90.0);
        let percent_100 = scale.normalize_price(100.0);
        let percent_110 = scale.normalize_price(110.0);

        assert!((percent_90 - (-10.0)).abs() < 0.1, "90 should be -10%");
        assert!((percent_100 - 0.0).abs() < 0.1, "100 should be 0%");
        assert!((percent_110 - 10.0).abs() < 0.1, "110 should be +10%");
    }

    #[test]
    fn test_idxed_to_100_mode() {
        let mut scale = PriceScale::new(100.0);
        scale.set_options(PriceScaleOptions {
            mode: PriceScaleMode::IndexedTo100,
            ..Default::default()
        });
        scale.set_first_val(100.0);
        scale.auto_scale(90.0, 110.0);

        // Test price transformation
        // 100 -> 100, 90 -> 90, 110 -> 110
        let indexed_90 = scale.normalize_price(90.0);
        let indexed_100 = scale.normalize_price(100.0);
        let indexed_110 = scale.normalize_price(110.0);

        assert!((indexed_90 - 90.0).abs() < 0.1, "90 should map to 90");
        assert!((indexed_100 - 100.0).abs() < 0.1, "100 should map to 100");
        assert!((indexed_110 - 110.0).abs() < 0.1, "110 should map to 110");
    }

    #[test]
    fn test_invert_scale() {
        let mut scale = PriceScale::new(100.0);
        scale.set_options(PriceScaleOptions {
            invert_scale: true,
            ..Default::default()
        });
        scale.auto_scale(0.0, 100.0);

        // With inverted scale, higher prices should have higher Y coords
        let y_0 = scale.price_to_coord(0.0);
        let y_100 = scale.price_to_coord(100.0);

        assert!(
            y_100 > y_0,
            "With inverted scale, higher price should have higher Y"
        );
    }
}
