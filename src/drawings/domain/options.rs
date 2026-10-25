//! Drawing options and configuration types.
//!
//! Contains styling enums ([`LineStyle`], [`ArrowStyle`], [`FontWeight`]),
//! interaction configuration ([`DrawingOptions`], [`HandlePos`]),
//! per-timeframe visibility ([`TimeframeVisibility`]), Fibonacci level
//! configuration ([`FibonacciConfig`], [`FibonacciLevel`]), and the settings of
//! the volume profile, regression trend and position drawings
//! ([`VolumeProfileConfig`], [`RegressionTrendConfig`], [`PositionSize`]).

pub use crate::model::LineStyle;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ops::RangeInclusive;

/// Identifies a selection handle position on a drawing.
///
/// Different tool types expose different handle layouts. Two-point tools use
/// `Start`, `End`, and `Middle`; rectangles use the four corner handles;
/// multi-point tools use indexed `Point(i)` handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlePos {
    /// First anchor point of the drawing.
    Start,
    /// Last anchor point of the drawing.
    End,
    /// Centroid/midpoint handle -- dragging this moves the entire drawing.
    Middle,
    /// Top-left corner (used by rectangle and similar axis-aligned shapes).
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
    /// Indexed anchor point for multi-point drawings (e.g., `Point(2)` is the
    /// third point in an XABCD pattern).
    Point(usize),
}

/// Arrow/arrowhead style for line drawings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub enum ArrowStyle {
    /// No arrowhead
    None,
    /// Standard filled arrow (default)
    #[default]
    Arrow,
    /// Open/outline arrow
    OpenArrow,
    /// Circle marker
    Circle,
    /// Square marker
    Square,
    /// Diamond marker
    Diamond,
}

impl ArrowStyle {
    /// Check if this style requires any rendering
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, ArrowStyle::None)
    }

    /// Get the marker size multiplier relative to stroke width
    #[inline]
    pub fn size_multiplier(&self) -> f32 {
        match self {
            ArrowStyle::None => 0.0,
            ArrowStyle::Arrow | ArrowStyle::OpenArrow => 4.0,
            ArrowStyle::Circle => 3.0,
            ArrowStyle::Square => 3.0,
            ArrowStyle::Diamond => 3.5,
        }
    }

    /// All available arrow styles
    pub fn all() -> &'static [ArrowStyle] {
        &[
            ArrowStyle::None,
            ArrowStyle::Arrow,
            ArrowStyle::OpenArrow,
            ArrowStyle::Circle,
            ArrowStyle::Square,
            ArrowStyle::Diamond,
        ]
    }
}

/// Font weight for text annotations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

/// Per-timeframe visibility configuration
///
/// Allows drawings to be visible only on specific timeframes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimeframeVisibility {
    /// If true, show on all timeframes (default behavior)
    pub show_on_all: bool,
    /// Set of timeframe strings where this drawing is visible
    #[serde(serialize_with = "serialize_sorted")]
    pub visible_timeframes: HashSet<String>,
}

/// Writes the set sorted, so saving the same drawings gives the same file.
fn serialize_sorted<S: serde::Serializer>(
    set: &HashSet<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(set.iter().collect::<std::collections::BTreeSet<_>>())
}

impl TimeframeVisibility {
    /// Create visibility for all timeframes (default)
    pub fn all() -> Self {
        Self {
            show_on_all: true,
            visible_timeframes: HashSet::new(),
        }
    }

    /// Create visibility for specific timeframes only
    pub fn specific(timeframes: &[&str]) -> Self {
        Self {
            show_on_all: false,
            visible_timeframes: timeframes.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// Check if visible on the given timeframe
    #[inline]
    pub fn is_visible_on(&self, timeframe: &str) -> bool {
        self.show_on_all || self.visible_timeframes.contains(timeframe)
    }

    /// Add a timeframe to visibility
    pub fn add_timeframe(&mut self, timeframe: &str) {
        self.show_on_all = false;
        self.visible_timeframes.insert(timeframe.to_string());
    }

    /// Remove a timeframe from visibility
    pub fn remove_timeframe(&mut self, timeframe: &str) {
        self.visible_timeframes.remove(timeframe);
        if self.visible_timeframes.is_empty() {
            self.show_on_all = true;
        }
    }
}

/// Drawing interaction options
#[derive(Debug, Clone)]
pub struct DrawingOptions {
    /// Enable snap to OHLC prices
    pub snap_to_price: bool,
    /// Enable snap to candle open/close times
    pub snap_to_time: bool,
    /// Snap distance in pixels
    pub snap_distance: f32,
    /// Enable magnet mode (snap to nearby points)
    pub magnet_mode: bool,
    /// Magnet distance in pixels
    pub magnet_distance: f32,
    /// Show selection handles
    pub show_handles: bool,
    /// Handle size in pixels
    pub handle_size: f32,
    /// Handle color (RGBA)
    pub handle_color: [u8; 4],
    /// Selected handle color (RGBA)
    pub handle_sel_color: [u8; 4],
    /// Default drawing color (RGBA)
    pub default_color: [u8; 4],
}

impl Default for DrawingOptions {
    fn default() -> Self {
        Self {
            snap_to_price: true,
            snap_to_time: true,
            snap_distance: 10.0,
            magnet_mode: false,
            magnet_distance: 15.0,
            show_handles: true,
            handle_size: 6.0,
            handle_color: [255, 255, 255, 200],
            handle_sel_color: [242, 54, 69, 255], // Default red
            default_color: [242, 54, 69, 255],    // Default red
        }
    }
}

/// Configuration for a single Fibonacci level
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FibonacciLevel {
    /// The ratio value (e.g., 0.236, 0.382, 0.618)
    pub value: f32,
    /// Display label (e.g., "23.6%", "61.8%")
    pub label: String,
    /// Level color as RGBA
    pub color: [u8; 4],
    /// Whether this level is visible
    pub visible: bool,
    /// Show price at this level
    pub show_price: bool,
}

impl FibonacciLevel {
    /// Creates a new Fibonacci level with the given ratio, display label, and color.
    ///
    /// The level is visible by default with price display enabled.
    pub fn new(value: f32, label: &str, color: [u8; 4]) -> Self {
        Self {
            value,
            label: label.to_string(),
            color,
            visible: true,
            show_price: true,
        }
    }
}

/// Configuration for all Fibonacci levels in a drawing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FibonacciConfig {
    /// Individual level configurations
    pub levels: Vec<FibonacciLevel>,
    /// Show background fill between levels
    pub show_background: bool,
    /// Background opacity (0-255)
    pub background_opacity: u8,
    /// Extend lines to the left
    pub extend_left: bool,
    /// Extend lines to the right
    pub extend_right: bool,
    /// Reverse the direction (flip vertically)
    pub reverse: bool,
    /// Measure retracement and extension levels in logarithmic price when all anchors are positive.
    #[serde(default)]
    pub logarithmic: bool,
}

impl Default for FibonacciConfig {
    /// Default Fibonacci retracement levels
    fn default() -> Self {
        Self {
            levels: vec![
                FibonacciLevel::new(0.0, "0%", [120, 123, 134, 255]),
                FibonacciLevel::new(0.236, "23.6%", [242, 54, 69, 255]),
                FibonacciLevel::new(0.382, "38.2%", [255, 152, 0, 255]),
                FibonacciLevel::new(0.5, "50%", [76, 175, 80, 255]),
                FibonacciLevel::new(0.618, "61.8%", [33, 150, 243, 255]),
                FibonacciLevel::new(0.786, "78.6%", [156, 39, 176, 255]),
                FibonacciLevel::new(1.0, "100%", [120, 123, 134, 255]),
            ],
            show_background: false,
            background_opacity: 30,
            extend_left: false,
            extend_right: false,
            reverse: false,
            logarithmic: false,
        }
    }
}

impl FibonacciConfig {
    /// Extension levels (beyond 100%)
    pub fn extension_default() -> Self {
        Self {
            levels: vec![
                FibonacciLevel::new(0.0, "0%", [120, 123, 134, 255]),
                FibonacciLevel::new(0.618, "61.8%", [33, 150, 243, 255]),
                FibonacciLevel::new(1.0, "100%", [120, 123, 134, 255]),
                FibonacciLevel::new(1.272, "127.2%", [242, 54, 69, 255]),
                FibonacciLevel::new(1.618, "161.8%", [255, 152, 0, 255]),
                FibonacciLevel::new(2.0, "200%", [76, 175, 80, 255]),
                FibonacciLevel::new(2.618, "261.8%", [156, 39, 176, 255]),
            ],
            show_background: false,
            background_opacity: 30,
            extend_left: false,
            extend_right: true,
            reverse: false,
            logarithmic: false,
        }
    }

    /// Get visible levels only
    pub fn visible_levels(&self) -> impl Iterator<Item = &FibonacciLevel> {
        self.levels.iter().filter(|l| l.visible)
    }

    /// Position of retracement level `value` between the first anchor `start`
    /// and the second anchor `end`, in price units.
    ///
    /// Level 0 sits on the second anchor and level 1 on the first, so levels
    /// measure back from the end of the move; `reverse` swaps the two anchors.
    pub(crate) fn retracement_position(&self, start: f64, end: f64, value: f64) -> f64 {
        let (from, to) = if self.reverse {
            (start, end)
        } else {
            (end, start)
        };
        if self.logarithmic && from > 0.0 && to > 0.0 {
            (from.ln() + (to.ln() - from.ln()) * value).exp()
        } else {
            from + (to - from) * value
        }
    }

    /// Price of each visible level, in config order, between the anchor prices `start` and `end`:
    /// a retracement's as [`Self::retracement_position`] places them, an extension's projected
    /// from `origin` by `end - start`, or by the log swing when all three prices are positive.
    pub(crate) fn level_prices(
        &self,
        start: f64,
        end: f64,
        extension: bool,
        origin: f64,
    ) -> impl Iterator<Item = f64> + '_ {
        self.visible_levels().map(move |level| {
            let value = f64::from(level.value);
            if extension {
                if self.logarithmic && start > 0.0 && end > 0.0 && origin > 0.0 {
                    (origin.ln() + (end.ln() - start.ln()) * value).exp()
                } else {
                    origin + (end - start) * value
                }
            } else {
                self.retracement_position(start, end, value)
            }
        })
    }
}

/// Settings of the fixed range and anchored volume profile drawings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VolumeProfileFields")]
pub struct VolumeProfileConfig {
    rows: usize,
    value_area: f64,
}

/// Stored settings of a volume profile, checked by [`VolumeProfileConfig::new`] on load.
#[derive(Deserialize)]
struct VolumeProfileFields {
    rows: usize,
    value_area: f64,
}

impl TryFrom<VolumeProfileFields> for VolumeProfileConfig {
    type Error = &'static str;

    fn try_from(fields: VolumeProfileFields) -> Result<Self, Self::Error> {
        Self::new(fields.rows, fields.value_area).ok_or("volume profile settings out of bounds")
    }
}

impl VolumeProfileConfig {
    /// Price rows a profile may have.
    pub const ROWS: RangeInclusive<usize> = 1..=1000;

    /// A profile of `rows` equal price rows whose value area holds the
    /// `value_area` share of its volume (above 0, at most 1); `None` when
    /// either is out of bounds.
    pub fn new(rows: usize, value_area: f64) -> Option<Self> {
        (Self::ROWS.contains(&rows) && value_area > 0.0 && value_area <= 1.0)
            .then_some(Self { rows, value_area })
    }

    /// Number of price rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Share of the volume inside the value area.
    pub fn value_area(&self) -> f64 {
        self.value_area
    }
}

impl Default for VolumeProfileConfig {
    /// 24 rows and a value area of 70% of the volume.
    fn default() -> Self {
        Self {
            rows: 24,
            value_area: 0.7,
        }
    }
}

/// Settings of the regression trend drawing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RegressionTrendFields")]
pub struct RegressionTrendConfig {
    deviations: f64,
}

/// Stored settings of a regression trend, checked by [`RegressionTrendConfig::new`] on load.
#[derive(Deserialize)]
struct RegressionTrendFields {
    deviations: f64,
}

impl TryFrom<RegressionTrendFields> for RegressionTrendConfig {
    type Error = &'static str;

    fn try_from(fields: RegressionTrendFields) -> Result<Self, Self::Error> {
        Self::new(fields.deviations).ok_or("regression trend settings out of bounds")
    }
}

impl RegressionTrendConfig {
    /// Bands `deviations` standard deviations either side of the fitted line
    /// (above 0, at most 10); `None` when out of bounds.
    pub fn new(deviations: f64) -> Option<Self> {
        (deviations > 0.0 && deviations <= 10.0).then_some(Self { deviations })
    }

    /// Distance of the bands from the fitted line, in standard deviations.
    pub fn deviations(&self) -> f64 {
        self.deviations
    }
}

impl Default for RegressionTrendConfig {
    /// Bands two standard deviations away.
    fn default() -> Self {
        Self { deviations: 2.0 }
    }
}

/// How a long or short position drawing sizes its quantity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PositionSize {
    /// A fixed quantity.
    Quantity(f64),
    /// The quantity that loses this amount if the stop is hit.
    RiskAmount(f64),
    /// The quantity that loses `percent` of `account_size` if the stop is hit.
    RiskPercent {
        /// Account size, in the currency of the instrument's prices.
        account_size: f64,
        /// Share of the account risked, in percent (above 0, at most 100).
        percent: f64,
    },
}

impl PositionSize {
    /// The quantity of a position entered at `entry` with its stop at `stop`;
    /// `None` when a size from the risk has its stop at the entry, or a
    /// setting is out of bounds (not positive and finite, or a percent above 100).
    pub fn quantity(&self, entry: f64, stop: f64) -> Option<f64> {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        let risk = match *self {
            Self::Quantity(quantity) => return positive(quantity).then_some(quantity),
            Self::RiskAmount(amount) => amount,
            Self::RiskPercent {
                account_size,
                percent,
            } if positive(account_size) && positive(percent) && percent <= 100.0 => {
                account_size * percent / 100.0
            }
            Self::RiskPercent { .. } => return None,
        };
        let quantity = risk / (entry - stop).abs();
        positive(quantity).then_some(quantity)
    }
}

impl Default for PositionSize {
    /// One unit, so the profit and loss read per unit.
    fn default() -> Self {
        Self::Quantity(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: quantity = risked amount / |entry - stop| (reference.py);
    /// none when a size from the risk has its stop at the entry, or a setting
    /// is out of bounds.
    #[test]
    fn position_size_quantity() {
        let percent = |account_size, percent| PositionSize::RiskPercent {
            account_size,
            percent,
        };
        for (size, entry, stop, want) in [
            (PositionSize::Quantity(3.0), 50.0, 48.0, Some(3.0)),
            (PositionSize::RiskAmount(100.0), 50.0, 48.0, Some(50.0)),
            (percent(10_000.0, 1.0), 50.0, 52.5, Some(40.0)),
            (percent(2_000.0, 2.5), 1.1, 1.0975, Some(19999.99999999865)),
            (PositionSize::RiskAmount(100.0), 50.0, 50.0, None),
            (PositionSize::RiskAmount(-100.0), 50.0, 48.0, None),
            (PositionSize::Quantity(f64::NAN), 50.0, 48.0, None),
            (percent(10_000.0, 150.0), 50.0, 48.0, None),
            (percent(-10_000.0, -1.0), 50.0, 48.0, None),
        ] {
            let got = size.quantity(entry, stop);
            assert!(
                got.zip(want)
                    .map_or(got == want, |(got, want)| (got - want).abs() < 1e-6),
                "{size:?}: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn drawing_settings_reject_out_of_bounds_values() {
        assert!(VolumeProfileConfig::new(1, 1.0).is_some());
        assert!(VolumeProfileConfig::new(1000, 0.01).is_some());
        for (rows, value_area) in [(0, 0.7), (1001, 0.7), (24, 0.0), (24, 1.01), (24, f64::NAN)] {
            assert!(VolumeProfileConfig::new(rows, value_area).is_none());
        }
        assert!(RegressionTrendConfig::new(10.0).is_some());
        for deviations in [0.0, -1.0, 10.5, f64::INFINITY, f64::NAN] {
            assert!(RegressionTrendConfig::new(deviations).is_none());
        }
    }

    #[test]
    fn test_arrow_style_visibility() {
        assert!(!ArrowStyle::None.is_visible());
        assert!(ArrowStyle::Arrow.is_visible());
        assert!(ArrowStyle::Circle.is_visible());
    }

    #[test]
    fn test_timeframe_visibility() {
        let mut vis = TimeframeVisibility::all();
        assert!(vis.is_visible_on("1h"));
        assert!(vis.is_visible_on("1D"));

        vis.add_timeframe("1h");
        assert!(vis.is_visible_on("1h"));
        assert!(!vis.is_visible_on("1D"));
    }

    /// Anchors 100 then 200: level v sits at 200 - 100 * v, or at 100 + 100 * v when reversed.
    #[test]
    fn fibonacci_retracement_position() {
        for (reverse, value, want) in [
            (false, 0.0, 200.0),
            (false, 0.236, 176.4),
            (false, 0.618, 138.2),
            (false, 1.0, 100.0),
            (true, 0.236, 123.6),
            (true, 0.786, 178.6),
        ] {
            let config = FibonacciConfig {
                reverse,
                ..Default::default()
            };
            let got = config.retracement_position(100.0, 200.0, value);
            assert!(
                (got - want).abs() < 1e-9,
                "reverse {reverse}, level {value}: got {got}, want {want}"
            );
        }
        // With 0 at 10 and 1 at 1000, log10(price) = 1 + 2*r.
        // Rounded independently: 10^1.472, 10^2, 10^2.236.
        for (logarithmic, low, ratio, want) in [
            (true, 10.0, 0.236, 29.64831389524342),
            (true, 10.0, 0.5, 100.0),
            (true, 10.0, 0.618, 172.1868574986006),
            (false, 10.0, 0.236, 243.64),
            (false, 10.0, 0.5, 505.0),
            (false, 10.0, 0.618, 621.82),
            (true, 0.0, 0.5, 500.0),
            (true, -10.0, 0.5, 495.0),
        ] {
            for reverse in [false, true] {
                let config = FibonacciConfig {
                    logarithmic,
                    reverse,
                    ..Default::default()
                };
                let (start, end) = if reverse {
                    (low, 1000.0)
                } else {
                    (1000.0, low)
                };
                let got = config.retracement_position(start, end, ratio);
                assert!(
                    (got - want).abs() < 1e-9,
                    "log {logarithmic}, reverse {reverse}, ratio {ratio}: {got} != {want}"
                );
            }
        }
    }
}
