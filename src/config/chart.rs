//! Chart visual configuration.
//!
//! [`ChartConfig`] controls everything about the chart's appearance: candle
//! colors, grid lines, axis visibility, volume panel, session breaks,
//! watermark, and the "Go to Realtime" button.
//!
//! [`PriceAxisOptions`] sets the price axis border, tick marks and label clipping.
//!
//! [`SessionConfig`] describes trading session hours for a specific exchange,
//! with presets for NYSE, LSE, TSE, crypto 24/7, and forex.

use crate::model::{LineStyle, PriceSource};
use crate::scales::PriceScaleMode;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Background style for the chart.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
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

/// When the chart shows its navigation buttons in the main pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum NavigationButtons {
    /// Always show the buttons.
    Always,
    /// Show the buttons while the pointer is over the main pane, price and volume.
    OnHover,
    /// Hide the buttons (the default).
    #[default]
    Never,
}

/// Appearance of the price axis: its border, tick marks and which labels it shows. It applies to
/// the main and series price axes; study panes draw their own axes.
///
/// Tick spacing is a property of each scale, set through
/// [`PriceScaleOptions::tick_marks`](crate::scales::PriceScaleOptions::tick_marks).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceAxisOptions {
    /// Draw a line between the plot and the axis.
    /// Default: false
    pub border_visible: bool,
    /// Colour of the border; `None` uses the theme's border colour.
    /// Default: None
    pub border_color: Option<Color32>,
    /// Draw a short horizontal tick beside each label.
    /// Default: false
    pub ticks_visible: bool,
    /// Label the prices at the top and bottom of the pane, in place of the tick marks next to them.
    /// Default: false
    pub edge_ticks_visible: bool,
    /// Leave out labels that the top or bottom of the pane would cut; with `false` they show clipped.
    /// Default: true
    pub hide_clipped_labels: bool,
}

impl Default for PriceAxisOptions {
    fn default() -> Self {
        Self {
            border_visible: false,
            border_color: None,
            ticks_visible: false,
            edge_ticks_visible: false,
            hide_clipped_labels: true,
        }
    }
}

/// Trading session configuration.
///
/// Presets use exchange time zones with `timezones`, or fixed standard-time offsets without it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
            timezone: if cfg!(feature = "timezones") {
                "America/New_York"
            } else {
                "-05:00"
            }
            .to_string(),
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
            timezone: if cfg!(feature = "timezones") {
                "Europe/London"
            } else {
                "+00:00"
            }
            .to_string(),
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
            timezone: if cfg!(feature = "timezones") {
                "Asia/Tokyo"
            } else {
                "+09:00"
            }
            .to_string(),
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
            timezone: if cfg!(feature = "timezones") {
                "America/New_York"
            } else {
                "-05:00"
            }
            .to_string(),
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
/// Defaults follow a common dark-theme charting palette.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
    /// Vertical grid colour; `None` follows `grid_color`.
    pub vertical_grid_color: Option<Color32>,
    /// Grid line style, dotted by default.
    pub grid_line_style: LineStyle,
    /// Vertical grid line style; `None` follows `grid_line_style`.
    pub vertical_grid_line_style: Option<LineStyle>,
    /// Text color for labels
    pub text_color: Color32,
    /// Width of candlestick/bar bodies as a fraction of available space (0.0 to 1.0)
    pub candle_width: f32,
    /// Stroke width for candlestick wicks
    pub wick_width: f32,
    /// Stroke width for candle borders (0 = no border)
    pub candle_border_width: f32,
    /// Master visibility switch for horizontal and vertical grid lines
    pub show_grid: bool,
    /// Whether to show horizontal grid lines
    pub show_horizontal_grid: bool,
    /// Whether to show vertical grid lines
    pub show_vertical_grid: bool,
    /// Whether to show volume bars
    pub show_volume: bool,
    /// Volume height as a fraction of the price pane above `volume_baseline_inset`
    pub volume_height_fraction: f32,
    /// Distance in points from the volume baseline to the price pane bottom (12 by default).
    pub volume_baseline_inset: f32,
    /// Padding around the plot, excluding the axes and floating legend.
    pub padding: f32,
    /// Whether to show time labels on X-axis
    pub show_time_labels: bool,
    /// Whether to show OHLC info header
    pub show_ohlc_info: bool,
    /// Use the host description rather than the ticker in the legend title.
    pub title_as_description: bool,
    /// Show the current bid and ask controls in the legend.
    pub show_quote_boxes: bool,
    /// Show absolute bar change in the legend.
    pub show_ohlc_change: bool,
    /// Show percentage bar change in the legend.
    pub show_ohlc_change_percent: bool,
    /// Show volume in the legend independently of the histogram.
    pub show_legend_volume: bool,
    /// Show host-supplied study inputs in the legend.
    pub show_indicator_inputs: bool,
    /// Whether to show right price axis
    pub show_right_axis: bool,
    /// Whether to show left price axis
    pub show_left_axis: bool,
    /// Minimum right price axis width in pixels; current labels can grow the column.
    pub right_axis_width: f32,
    /// Minimum left price axis width in pixels; current labels can grow the column.
    pub left_axis_width: f32,
    /// Border, tick marks and label clipping of the price axis
    pub price_axis: PriceAxisOptions,
    /// Scale mode for right price axis (Normal, Logarithmic, Percentage, IndexedTo100)
    pub right_axis_scale_mode: PriceScaleMode,
    /// Scale mode for left price axis (Normal, Logarithmic, Percentage, IndexedTo100)
    pub left_axis_scale_mode: PriceScaleMode,
    /// Whether to show symbol name labels
    pub show_symbol_labels: bool,
    /// Whether to show last price value
    pub show_symbol_last_val: bool,
    /// Draw a line at the latest close independently of its tag.
    pub show_price_line: bool,
    /// Draw a line at the preceding session close independently of its tag.
    pub show_prev_close_line: bool,
    /// Whether to show previous close value line
    pub show_symbol_prev_close: bool,
    /// Whether to show indicator name labels
    pub show_indicator_labels: bool,
    /// Whether to show indicator last values
    pub show_indicator_last_val: bool,
    /// Preserve price units per bar when the time scale zooms or the pane resizes.
    pub lock_price_to_bar_ratio: bool,
    /// Draw the latest valid bid and ask lines and axis values supplied by the host.
    pub show_bid_ask: bool,
    /// Draw the highest high and lowest low of the visible bars on the price scale.
    pub show_high_low: bool,
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
    /// When to show the navigation buttons.
    pub navigation_buttons: NavigationButtons,
    /// Whether to show session break vertical lines
    pub show_session_breaks: bool,
    /// Color for session break lines
    pub session_break_color: Color32,
    /// Style for session break lines
    pub session_break_style: LineStyle,
    /// Session configuration (market hours)
    pub session_config: Option<SessionConfig>,
    /// Shade bars identified as extended hours by the session metadata.
    pub show_extended_hours: bool,
    /// Extended-hours fill. `PLACEHOLDER` uses the active theme accent at low opacity.
    pub extended_hours_color: Color32,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
        let chart = Theme::dark().chart;
        Self {
            bullish_color: chart.candle_up,
            bearish_color: chart.candle_down,
            // Border colors (None = same as body)
            bullish_border_color: None,
            bearish_border_color: None,
            // Wick colors (None = same as border/body)
            bullish_wick_color: None,
            bearish_wick_color: None,
            // Price source for line charts (default: Close)
            price_source: PriceSource::default(),
            // Dark background
            background_color: chart.bg,
            // Default to solid background
            background_style: BackgroundStyle::Solid,
            // Grid colour from the default theme
            grid_color: chart.grid_line,
            vertical_grid_color: None,
            grid_line_style: LineStyle::Dotted,
            vertical_grid_line_style: None,
            // Text color
            text_color: chart.axis_text,
            // Candle bodies leave room for the wider volume bars
            candle_width: 0.6,
            // Wick - thin crisp lines
            wick_width: 1.0,
            // Candle border width (0 = filled candles, >0 = hollow with border)
            candle_border_width: 0.0,
            show_grid: true,
            show_horizontal_grid: true,
            show_vertical_grid: true,
            show_volume: true,
            volume_height_fraction: 0.12,
            volume_baseline_inset: 12.0,
            padding: 0.0,
            show_time_labels: true,
            show_ohlc_info: true,
            title_as_description: true,
            show_quote_boxes: true,
            show_ohlc_change: true,
            show_ohlc_change_percent: true,
            show_legend_volume: true,
            show_indicator_inputs: true,
            show_right_axis: true,
            show_left_axis: false,
            right_axis_width: 70.0,
            left_axis_width: 70.0,
            price_axis: PriceAxisOptions::default(),
            right_axis_scale_mode: PriceScaleMode::Normal,
            left_axis_scale_mode: PriceScaleMode::Normal,
            show_symbol_labels: true,
            show_symbol_last_val: true,
            show_price_line: true,
            show_prev_close_line: false,
            show_symbol_prev_close: false,
            show_indicator_labels: true,
            show_indicator_last_val: true,
            lock_price_to_bar_ratio: false,
            show_bid_ask: false,
            show_high_low: false,
            show_countdown: true,
            show_realtime_btn: true,
            realtime_button_pos: RealtimeButtonPos::TopCenter,
            realtime_button_text: None,
            realtime_button_size: (110.0, 28.0),
            realtime_button_color: DESIGN_TOKENS.semantic.extended.chart_tooltip_bg,
            realtime_button_hover_color: DESIGN_TOKENS.semantic.extended.chart_axis_bg,
            realtime_button_text_color: chart.axis_text,
            navigation_buttons: NavigationButtons::default(),
            // Session breaks disabled by default
            show_session_breaks: false,
            session_break_color: chart.grid_line_major,
            session_break_style: LineStyle::Dashed,
            session_config: None,
            show_extended_hours: true,
            extended_hours_color: Color32::PLACEHOLDER,
            // Watermark disabled by default
            show_watermark: false,
            watermark_text: None,
            watermark_color: chart.grid_line,
            watermark_font_size: 48.0,
            watermark_pos: WatermarkPos::Center,
            // Background enabled by default (set false when inside a container)
            skip_background: false,
        }
    }
}

impl ChartConfig {
    /// Apply this theme's background, grid, axis text and candle colours to `config`.
    ///
    /// Only the configuration changes. A chart paints its legend, quote boxes, labels and
    /// overlays from its own theme, which [`ChartBuilder::with_theme`](crate::ChartBuilder::with_theme)
    /// keeps for that chart, so switch a chart's theme with
    /// [`Chart::set_theme`](crate::Chart::set_theme) or
    /// [`Chart::set_theme_override`](crate::Chart::set_theme_override), which apply these
    /// colours too.
    pub fn with_theme(mut self, theme: &Theme) -> Self {
        self.background_color = theme.chart.bg;
        self.grid_color = theme.chart.grid_line;
        self.text_color = theme.chart.axis_text;
        self.bullish_color = theme.chart.candle_up;
        self.bearish_color = theme.chart.candle_down;
        self.bullish_border_color = Some(theme.chart.candle_up);
        self.bearish_border_color = Some(theme.chart.candle_down);
        self.bullish_wick_color = Some(theme.chart.candle_up);
        self.bearish_wick_color = Some(theme.chart.candle_down);
        self
    }
}
