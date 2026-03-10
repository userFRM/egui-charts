//! Tooltip configuration for data display on hover.
//!
//! Provides three tooltip variants:
//! - **Floating**: Classic tooltip popup that follows the cursor
//! - **Tracking**: Fixed horizontal bar at the top of the chart showing OHLC data
//! - **Magnifier**: Circular zoom lens for detailed data inspection on dense charts

/// Tooltip Configuration for data tooltips.
///
/// Provides three tooltip variants:
/// - Floating: Classic tooltip that follows cursor position
/// - Tracking: Fixed horizontal bar showing OHLC data at top of chart
/// - Magnifier: Circular zoom lens for detailed data inspection.
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Tooltip display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TooltipMode {
    /// Floating tooltip that follows cursor position
    /// Shows OHLC, volume, and change info in a popup near the mouse
    #[default]
    Floating,

    /// Tracking tooltip - fixed position bar at top of chart
    /// Shows: O: xxx  H: xxx  L: xxx  C: xxx  Vol: xxx  (+x.xx%)
    /// Updates as cursor moves horizontally across the chart
    Tracking,

    /// Magnifier tooltip - circular zoom lens following cursor
    /// Shows magnified view of candles/data under the cursor
    /// Useful for detailed analysis on dense charts
    Magnifier,
}

impl std::fmt::Display for TooltipMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TooltipMode::Floating => write!(f, "Floating"),
            TooltipMode::Tracking => write!(f, "Tracking"),
            TooltipMode::Magnifier => write!(f, "Magnifier"),
        }
    }
}

/// Tooltip configuration options
#[derive(Debug, Clone)]
pub struct TooltipOptions {
    /// Tooltip display mode
    pub mode: TooltipMode,

    /// Show OHLC values (Open, High, Low, Close)
    pub show_ohlc: bool,

    /// Show volume
    pub show_volume: bool,

    /// Show price change (absolute and percentage)
    pub show_change: bool,

    /// Show ts
    pub show_time: bool,

    /// Show indicator values (if indicators are present)
    pub show_indicators: bool,

    /// Magnifier zoom level (2.0 = 2x zoom, 3.0 = 3x zoom)
    /// Only applies when mode is Magnifier
    pub magnifier_zoom: f32,

    /// Magnifier lens diameter in pixels
    /// Only applies when mode is Magnifier
    pub magnifier_size: f32,

    /// Background color for floating tooltip
    pub background_color: Color32,

    /// Text color for tooltip content
    pub text_color: Color32,

    /// Border color for tooltip (bullish)
    pub border_color_bullish: Color32,

    /// Border color for tooltip (bearish)
    pub border_color_bearish: Color32,

    /// Font size for tooltip text
    pub font_size: f32,

    /// Decimal precision for price values
    pub price_precision: usize,

    /// Tracking bar height (for Tracking mode)
    pub tracking_bar_height: f32,

    /// Tracking bar background color
    pub tracking_bar_background: Color32,
}

impl Default for TooltipOptions {
    fn default() -> Self {
        Self {
            mode: TooltipMode::Floating,
            show_ohlc: true,
            show_volume: true,
            show_change: true,
            show_time: true,
            show_indicators: true,
            magnifier_zoom: 2.0,
            magnifier_size: 120.0,
            background_color: DESIGN_TOKENS.semantic.extended.chart_tooltip_bg,
            text_color: DESIGN_TOKENS.semantic.extended.chart_text,
            border_color_bullish: DESIGN_TOKENS.semantic.extended.bullish,
            border_color_bearish: DESIGN_TOKENS.semantic.extended.bearish,
            font_size: 11.0,
            price_precision: 8,
            tracking_bar_height: 24.0,
            tracking_bar_background: DESIGN_TOKENS.semantic.chart.bg,
        }
    }
}

impl TooltipOptions {
    /// Create tooltip options with floating mode
    pub fn floating() -> Self {
        Self {
            mode: TooltipMode::Floating,
            ..Default::default()
        }
    }

    /// Create tooltip options with tracking mode
    pub fn tracking() -> Self {
        Self {
            mode: TooltipMode::Tracking,
            ..Default::default()
        }
    }

    /// Create tooltip options with magnifier mode
    pub fn magnifier() -> Self {
        Self {
            mode: TooltipMode::Magnifier,
            ..Default::default()
        }
    }

    /// Create magnifier with custom zoom level
    pub fn magnifier_with_zoom(zoom: f32) -> Self {
        Self {
            mode: TooltipMode::Magnifier,
            magnifier_zoom: zoom,
            ..Default::default()
        }
    }

    /// Set tooltip mode
    pub fn with_mode(mut self, mode: TooltipMode) -> Self {
        self.mode = mode;
        self
    }

    /// Enable/disable OHLC display
    pub fn with_ohlc(mut self, show: bool) -> Self {
        self.show_ohlc = show;
        self
    }

    /// Enable/disable volume display
    pub fn with_volume(mut self, show: bool) -> Self {
        self.show_volume = show;
        self
    }

    /// Enable/disable change display
    pub fn with_change(mut self, show: bool) -> Self {
        self.show_change = show;
        self
    }

    /// Set magnifier zoom level
    pub fn with_magnifier_zoom(mut self, zoom: f32) -> Self {
        self.magnifier_zoom = zoom;
        self
    }

    /// Set magnifier size
    pub fn with_magnifier_size(mut self, size: f32) -> Self {
        self.magnifier_size = size;
        self
    }

    /// Set price precision
    pub fn with_precision(mut self, precision: usize) -> Self {
        self.price_precision = precision;
        self
    }
}
