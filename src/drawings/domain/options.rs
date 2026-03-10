//! Drawing options and configuration types.
//!
//! Contains styling enums ([`LineStyle`], [`ArrowStyle`], [`FontWeight`]),
//! interaction configuration ([`DrawingOptions`], [`HandlePos`]),
//! per-timeframe visibility ([`TimeframeVisibility`]), and Fibonacci level
//! configuration ([`FibonacciConfig`], [`FibonacciLevel`]).

use std::collections::HashSet;

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

/// Line style for drawing strokes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    /// Get the dash pattern for rendering
    /// Returns (dash_len, gap_len) or None for solid
    #[inline]
    pub fn dash_pattern(&self) -> Option<(f32, f32)> {
        match self {
            LineStyle::Solid => None,
            LineStyle::Dashed => Some((8.0, 4.0)),
            LineStyle::Dotted => Some((2.0, 3.0)),
        }
    }

    /// All available line styles
    pub fn all() -> &'static [LineStyle] {
        &[LineStyle::Solid, LineStyle::Dashed, LineStyle::Dotted]
    }
}

/// Arrow/arrowhead style for line drawings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

/// Per-timeframe visibility configuration
///
/// Allows drawings to be visible only on specific timeframes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimeframeVisibility {
    /// If true, show on all timeframes (default behavior)
    pub show_on_all: bool,
    /// Set of timeframe strings where this drawing is visible
    pub visible_timeframes: HashSet<String>,
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
        }
    }

    /// Get visible levels only
    pub fn visible_levels(&self) -> impl Iterator<Item = &FibonacciLevel> {
        self.levels.iter().filter(|l| l.visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_style_dash_pattern() {
        assert!(LineStyle::Solid.dash_pattern().is_none());
        assert!(LineStyle::Dashed.dash_pattern().is_some());
        assert!(LineStyle::Dotted.dash_pattern().is_some());
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

    #[test]
    fn test_fibonacci_config_default() {
        let config = FibonacciConfig::default();
        assert_eq!(config.levels.len(), 7);
        assert!(!config.show_background);
    }
}
