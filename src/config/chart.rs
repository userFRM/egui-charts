//! Chart visual configuration.
//!
//! [`ChartConfig`] controls everything about the chart's appearance: candle
//! colors, grid lines, axis visibility, volume panel, session breaks,
//! watermark, and the "Go to Realtime" button.
//!
//! [`SessionConfig`] describes trading session hours for a specific exchange,
//! with presets for NYSE, LSE, TSE, crypto 24/7, and forex.

use crate::model::PriceSource;
use crate::scales::PriceScaleMode;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Background style for the chart.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BackgroundStyle {
    /// Solid single-color background
    #[default]
    Solid,
    /// Vertical gradient from top to bottom
    VerticalGradient {
        top_color: Color32,
        bottom_color: Color32,
    },
    /// Horizontal gradient from left to right
    HorizontalGradient {
        left_color: Color32,
        right_color: Color32,
    },
}

/// Style for session break lines
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionBreakStyle {
    /// Solid line
    #[default]
    Solid,
    /// Dashed line
    Dashed,
    /// Dotted line
    Dotted,
}

/// Trading session configuration
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// Session start time in HH:MM format (24-hour)
    pub session_start: String,
    /// Session end time in HH:MM format (24-hour)
    pub session_end: String,
    /// Timezone for the session (e.g., "America/New_York")
    pub timezone: String,
    /// Whether to show pre-market session
    pub show_premarket: bool,
    /// Whether to show post-market session
    pub show_postmarket: bool,
    /// Pre-market start time (e.g., "04:00")
    pub premarket_start: Option<String>,
    /// Post-market end time (e.g., "20:00")
    pub postmarket_end: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        // Default to NYSE trading hours
        Self {
            session_start: "09:30".to_string(),
            session_end: "16:00".to_string(),
            timezone: "America/New_York".to_string(),
            show_premarket: false,
            show_postmarket: false,
            premarket_start: Some("04:00".to_string()),
            postmarket_end: Some("20:00".to_string()),
        }
    }
}

impl SessionConfig {
    /// Create NYSE session configuration
    pub fn nyse() -> Self {
        Self::default()
    }

    /// Create London Stock Exchange session configuration
    pub fn lse() -> Self {
        Self {
            session_start: "08:00".to_string(),
            session_end: "16:30".to_string(),
            timezone: "Europe/London".to_string(),
            show_premarket: false,
            show_postmarket: false,
            premarket_start: None,
            postmarket_end: None,
        }
    }

    /// Create Tokyo Stock Exchange session configuration
    pub fn tse() -> Self {
        Self {
            session_start: "09:00".to_string(),
            session_end: "15:00".to_string(),
            timezone: "Asia/Tokyo".to_string(),
            show_premarket: false,
            show_postmarket: false,
            premarket_start: None,
            postmarket_end: None,
        }
    }

    /// Create 24/7 crypto session (no breaks)
    pub fn crypto_24_7() -> Self {
        Self {
            session_start: "00:00".to_string(),
            session_end: "23:59".to_string(),
            timezone: "UTC".to_string(),
            show_premarket: false,
            show_postmarket: false,
            premarket_start: None,
            postmarket_end: None,
        }
    }

    /// Create forex session configuration (Sydney -> Tokyo -> London -> New York)
    pub fn forex() -> Self {
        Self {
            session_start: "17:00".to_string(), // Sunday 5PM EST
            session_end: "17:00".to_string(),   // Friday 5PM EST
            timezone: "America/New_York".to_string(),
            show_premarket: false,
            show_postmarket: false,
            premarket_start: None,
            postmarket_end: None,
        }
    }
}

/// Configuration for chart visual appearance.
///
/// Controls every aspect of chart styling: candle/bar colors, grid lines,
/// axis visibility, volume panel sizing, session break rendering, watermark
/// overlay, and the real-time navigation button.
///
/// Defaults follow a dark-theme TradingView-style palette.
#[derive(Debug, Clone)]
pub struct ChartConfig {
    /// Color for bullish (up) bars/candles body
    pub bullish_color: Color32,
    /// Color for bearish (down) bars/candles body
    pub bearish_color: Color32,
    /// Border color for bullish candles (None = same as body)
    pub bullish_border_color: Option<Color32>,
    /// Border color for bearish candles (None = same as body)
    pub bearish_border_color: Option<Color32>,
    /// Wick color for bullish candles (None = same as border or body)
    pub bullish_wick_color: Option<Color32>,
    /// Wick color for bearish candles (None = same as border or body)
    pub bearish_wick_color: Option<Color32>,
    /// Price source for line-based charts (Line, Area, StepLine, etc.)
    pub price_source: PriceSource,
    /// Background color for the chart (used when background_style is Solid)
    pub background_color: Color32,
    /// Background style (Solid, VerticalGradient, HorizontalGradient)
    pub background_style: BackgroundStyle,
    /// Grid line color
    pub grid_color: Color32,
    /// Text color for labels
    pub text_color: Color32,
    /// Width of candlestick/bar bodies as a fraction of available space (0.0 to 1.0)
    pub candle_width: f32,
    /// Stroke width for candlestick wicks
    pub wick_width: f32,
    /// Stroke width for candle borders (0 = no border)
    pub candle_border_width: f32,
    /// Whether to show the price grid (horizontal lines)
    pub show_grid: bool,
    /// Whether to show horizontal grid lines
    pub show_horizontal_grid: bool,
    /// Whether to show vertical grid lines
    pub show_vertical_grid: bool,
    /// Whether to show volume bars
    pub show_volume: bool,
    /// Height of volume panel as fraction of total height
    pub volume_height_fraction: f32,
    /// Padding around the chart
    pub padding: f32,
    /// Whether to show time labels on X-axis
    pub show_time_labels: bool,
    /// Whether to show OHLC info header
    pub show_ohlc_info: bool,
    /// Whether to show right price axis
    pub show_right_axis: bool,
    /// Whether to show left price axis
    pub show_left_axis: bool,
    /// Width of right price axis in pixels
    pub right_axis_width: f32,
    /// Width of left price axis in pixels
    pub left_axis_width: f32,
    /// Scale mode for right price axis (Normal, Logarithmic, Percentage, IndexedTo100)
    pub right_axis_scale_mode: PriceScaleMode,
    /// Scale mode for left price axis (Normal, Logarithmic, Percentage, IndexedTo100)
    pub left_axis_scale_mode: PriceScaleMode,
    /// Whether to show symbol name labels
    pub show_symbol_labels: bool,
    /// Whether to show last price value
    pub show_symbol_last_val: bool,
    /// Whether to show previous close value line
    pub show_symbol_prev_close: bool,
    /// Whether to show indicator name labels
    pub show_indicator_labels: bool,
    /// Whether to show indicator last values
    pub show_indicator_last_val: bool,
    /// Whether to show countdown to next candle
    pub show_countdown: bool,
    /// Whether to show "Go to Realtime" button when scrolled away from live edge
    pub show_realtime_btn: bool,
    /// Pos of the realtime button
    pub realtime_button_pos: RealtimeButtonPos,
    /// Custom text for the realtime button (None = "Go to Realtime")
    pub realtime_button_text: Option<String>,
    /// Size of the realtime button (width, height)
    pub realtime_button_size: (f32, f32),
    /// Background color for the realtime button
    pub realtime_button_color: Color32,
    /// Hover background color for the realtime button
    pub realtime_button_hover_color: Color32,
    /// Text color for the realtime button
    pub realtime_button_text_color: Color32,
    /// Whether to show session break vertical lines
    pub show_session_breaks: bool,
    /// Color for session break lines
    pub session_break_color: Color32,
    /// Style for session break lines
    pub session_break_style: SessionBreakStyle,
    /// Session configuration (market hours)
    pub session_config: Option<SessionConfig>,
    /// Whether to show watermark (large symbol name overlay)
    pub show_watermark: bool,
    /// Watermark text (None = use symbol name)
    pub watermark_text: Option<String>,
    /// Watermark color (typically semi-transparent)
    pub watermark_color: Color32,
    /// Watermark font size
    pub watermark_font_size: f32,
    /// Watermark position
    pub watermark_pos: WatermarkPos,
    /// Skip background rendering (use when chart is inside a container with its own background)
    pub skip_background: bool,
}

/// Pos options for the "Go to Realtime" button
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RealtimeButtonPos {
    /// Top-left of the chart
    TopLeft,
    /// Top-center of the chart (default)
    #[default]
    TopCenter,
    /// Top-right of the chart
    TopRight,
    /// Bottom-left of the chart
    BottomLeft,
    /// Bottom-center of the chart
    BottomCenter,
    /// Bottom-right of the chart
    BottomRight,
}

/// Pos options for the watermark overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatermarkPos {
    /// Center of the chart (default)
    #[default]
    Center,
    /// Top-left corner
    TopLeft,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-right corner
    BottomRight,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            // Bullish color: #26a69a
            bullish_color: DESIGN_TOKENS.semantic.extended.bullish,
            // Bearish color: #F23645
            bearish_color: DESIGN_TOKENS.semantic.extended.bearish,
            // Border colors (None = same as body)
            bullish_border_color: None,
            bearish_border_color: None,
            // Wick colors (None = same as border/body)
            bullish_wick_color: None,
            bearish_wick_color: None,
            // Price source for line charts (default: Close)
            price_source: PriceSource::default(),
            // Dark background
            background_color: DESIGN_TOKENS.semantic.chart.bg,
            // Default to solid background
            background_style: BackgroundStyle::Solid,
            // Grid: ~5% opacity
            grid_color: DESIGN_TOKENS.semantic.chart.grid_line,
            // Text color
            text_color: DESIGN_TOKENS.semantic.extended.chart_text,
            // Candle style - wider bodies
            candle_width: 0.8,
            // Wick - thin crisp lines
            wick_width: 1.0,
            // Candle border width (0 = filled candles, >0 = hollow with border)
            candle_border_width: 0.0,
            show_grid: true,
            show_horizontal_grid: true,
            show_vertical_grid: true,
            show_volume: true,
            volume_height_fraction: 0.2,
            padding: 40.0,
            show_time_labels: true,
            show_ohlc_info: true,
            show_right_axis: true,
            show_left_axis: false,
            right_axis_width: 70.0,
            left_axis_width: 70.0,
            right_axis_scale_mode: PriceScaleMode::Normal,
            left_axis_scale_mode: PriceScaleMode::Normal,
            show_symbol_labels: true,
            show_symbol_last_val: true,
            show_symbol_prev_close: false,
            show_indicator_labels: true,
            show_indicator_last_val: true,
            show_countdown: false,
            show_realtime_btn: true,
            realtime_button_pos: RealtimeButtonPos::TopCenter,
            realtime_button_text: None,
            realtime_button_size: (110.0, 28.0),
            realtime_button_color: DESIGN_TOKENS.semantic.extended.chart_tooltip_bg,
            realtime_button_hover_color: DESIGN_TOKENS.semantic.extended.chart_axis_bg,
            realtime_button_text_color: DESIGN_TOKENS.semantic.extended.chart_text,
            // Session breaks disabled by default
            show_session_breaks: false,
            session_break_color: DESIGN_TOKENS.semantic.chart.grid_line_major,
            session_break_style: SessionBreakStyle::Dashed,
            session_config: None,
            // Watermark disabled by default
            show_watermark: false,
            watermark_text: None,
            watermark_color: DESIGN_TOKENS.semantic.chart.grid_line,
            watermark_font_size: 48.0,
            watermark_pos: WatermarkPos::Center,
            // Background enabled by default (set false when inside a container)
            skip_background: false,
        }
    }
}
