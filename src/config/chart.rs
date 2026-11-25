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
        /// Color at the top edge of the gradient.
        top_color: Color32,
        /// Color at the bottom edge of the gradient.
        bottom_color: Color32,
    },
    /// Horizontal gradient from left to right
    HorizontalGradient {
        /// Color at the left edge of the gradient.
        left_color: Color32,
        /// Color at the right edge of the gradient.
        right_color: Color32,
    },
}

/// When the chart shows a group of buttons: its navigation buttons in the main pane, or the
/// maximize and menu buttons of its panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum NavigationButtons {
    /// Always show the buttons.
    Always,
    /// Show the buttons while the pointer is over the main pane, price and volume (the default).
    #[default]
    OnHover,
    /// Hide the buttons.
    Never,
}

/// Older saved configurations show the pane buttons while the pointer is over the pane.
fn pane_buttons_on_hover() -> NavigationButtons {
    NavigationButtons::OnHover
}

/// Older saved configurations show the market status, as they showed delayed and end-of-day
/// data marks.
fn market_status_shown() -> bool {
    true
}

/// Layouts saved before `thousands_sep` existed group thousands like a new chart.
fn group_thousands_with_comma() -> Option<char> {
    Some(',')
}

/// Older saved configurations keep the one-physical-pixel grouping threshold.
fn pixel_conflation_threshold() -> Option<f32> {
    Some(1.0)
}

/// Older saved configurations use the theme for analyst targets and overnight shading.
fn theme_color() -> Color32 {
    Color32::PLACEHOLDER
}

/// Older saved configurations draw 1 point session breaks.
fn session_break_width() -> f32 {
    1.0
}

/// Older saved configurations shade the pre-market, the post-market and overnight windows.
fn shaded() -> bool {
    true
}

/// Appearance of the price axis: its border, tick marks and which labels it shows. It applies to
/// the main and series price axes; value tag settings also apply to study panes.
///
/// Tick spacing is a property of each scale, set through
/// [`PriceScaleOptions::tick_marks`](crate::scales::PriceScaleOptions::tick_marks).
///
/// ```
/// use egui_charts::config::PriceAxisOptions;
///
/// let mut options = PriceAxisOptions::default();
/// options.show_symbol_name = true;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PriceAxisOptions {
    /// Allow value tags to overlap instead of moving them to free vertical slots.
    pub allow_label_overlap: bool,
    /// Display study value tags on price axes, independently of the status line values. Each tag
    /// reads its plot at the last bar in view, or at its last value when the view reaches past
    /// it, so it stays where the plot leaves the pane while the chart scrolls; the main series'
    /// last price tag follows its own price line source.
    pub show_study_values: bool,
    /// Study value tag colour; `None` follows each plot's colour.
    pub study_value_color: Option<Color32>,
    /// Write the symbol before its last value tag on its price axis, and before the value tags
    /// of added series on theirs. Independent of the legend title.
    /// Default: false
    pub show_symbol_name: bool,
    /// Write each study's name before its value tags. Independent of the legend title.
    /// Default: false
    pub show_study_names: bool,
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
            allow_label_overlap: false,
            show_study_values: true,
            study_value_color: None,
            show_symbol_name: false,
            show_study_names: false,
            border_visible: false,
            border_color: None,
            ticks_visible: false,
            edge_ticks_visible: false,
            hide_clipped_labels: true,
        }
    }
}

/// Colour, width and pattern of one of the chart's own price lines, such as the high and low or
/// the previous close line. Its colour also fills the line's axis tag.
///
/// ```
/// use egui_charts::config::{ChartConfig, ChartLineStyle};
/// use egui_charts::model::LineStyle;
///
/// let mut line = ChartLineStyle::default();
/// line.color = Some(egui::Color32::GOLD);
/// line.width = 2.0;
/// line.style = LineStyle::Solid;
/// let mut config = ChartConfig::default();
/// config.show_prev_close_line = true;
/// config.prev_close_line = line;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ChartLineStyle {
    /// Colour of the line and its axis tag; `None` keeps the colour each line documents.
    /// Default: None
    pub color: Option<Color32>,
    /// Width in points. A width that is not positive and finite draws no line; the tag stays.
    /// Default: 1
    pub width: f32,
    /// Pattern of the line.
    /// Default: [`LineStyle::Dotted`]
    pub style: LineStyle,
}

impl Default for ChartLineStyle {
    fn default() -> Self {
        Self {
            color: None,
            width: 1.0,
            style: LineStyle::Dotted,
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
    /// The price the Line, Step line, Line with markers, Area and Baseline charts draw while
    /// their own source in [`ChartTypeStyles`](crate::model::ChartTypeStyles) is `None`, as in
    /// settings, templates and layouts saved before each had one. Close by default.
    pub price_source: PriceSource,
    /// Minimum spacing in physical pixels for unmerged main-series samples; `None` disables
    /// conflation. The default, `Some(1.0)`, preserves the one-pixel grouping. Below the
    /// threshold, consecutive bars form groups of the smallest power of two whose spacing
    /// reaches it, capped at 2^30 bars. For example, 0.25 points at two pixels per point gives
    /// groups of two at a threshold of 1, and four at a threshold of 2. Equality stays unmerged.
    /// Non-positive and non-finite thresholds use 1. Price-based chart types stay unmerged;
    /// gaps and per-bar colours keep their source samples even within a group.
    /// [`Chart::prepare_conflation`](crate::Chart::prepare_conflation) prepares these groups
    /// before rendering. Saved configurations without this field keep the default.
    #[serde(default = "pixel_conflation_threshold")]
    pub conflation_threshold: Option<f32>,
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
    /// Font size of this chart's price and time scale text in logical pixels: the axis labels
    /// and tags, including the crosshair's and those of study panes. `None` follows the theme's
    /// label size; other chart text keeps the theme's sizes.
    /// Default: None
    pub axis_font_size: Option<f32>,
    /// Font family of this chart's price and time axis labels and tags, including study panes.
    /// Absent named families use proportional fonts until the host installs them in
    /// [`egui::FontDefinitions`], without changing the saved choice.
    /// Defaults to proportional, including for saved configurations without this field.
    #[serde(default)]
    pub axis_font_family: egui::FontFamily,
    /// Character that groups the thousands of the built-in price labels: the ticks and tags of
    /// price scales, the crosshair label, the prices of the legend, the data window, the chart
    /// data table and the hover tooltip, the tooltip's change, their rows of studies in price
    /// units, and drawing prices and price changes in panes with a price series write
    /// `42,541.77` with the default `','`.
    /// `None` writes plain digits (`42541.77`), and so do `'.'` and the ASCII digits, since
    /// built-in labels always write `'.'` as the decimal point. Labels written by a host formatter
    /// ([`Chart::set_price_formatter`](crate::Chart::set_price_formatter)) are never grouped;
    /// study panes and volumes keep their own formats. Layouts saved without this field restore
    /// with the default.
    /// Default: `Some(',')`
    #[serde(default = "group_thousands_with_comma")]
    pub thousands_sep: Option<char>,
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
    /// Whether to show volume bars. With the `studies` feature it adds a Volume study on the next
    /// frame unless one is on the chart, and a user action that turns it on at the chart's study
    /// cap without one leaves it off.
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
    /// Show Buy and Sell in a panel over the price pane, moved by its grip, instead of in the
    /// legend. Off by default and in configurations saved without it.
    #[serde(default)]
    pub quote_panel: bool,
    /// Show absolute bar change in the legend.
    pub show_ohlc_change: bool,
    /// Show percentage bar change in the legend.
    pub show_ohlc_change_percent: bool,
    /// Show volume in the legend independently of the histogram.
    pub show_legend_volume: bool,
    /// Show study inputs after their names in the legend: the host's caption, else one built
    /// from the study's inputs.
    pub show_indicator_inputs: bool,
    /// Show the symbol's logo before the legend title: the legend badge, else the instrument's
    /// logo, else the symbol's initial on a neutral disc.
    pub show_legend_badge: bool,
    /// Show the market status after the legend title: the session's state and delayed or
    /// end-of-day data.
    #[serde(default = "market_status_shown")]
    pub show_market_status: bool,
    /// Show the change of the last price from the previous session's close in the legend.
    pub show_last_day_change: bool,
    /// Opacity of the study rows' backdrop, from 0 to 1.
    pub indicator_backdrop_opacity: f32,
    /// Show the host-supplied currency above the price ticks.
    pub show_currency: bool,
    /// Show the host-supplied price unit above the price ticks.
    pub show_unit: bool,
    /// Show the currency and unit only while the pointer is over the price scale or their menu
    /// is open. On by default; layouts saved without this field keep them always visible.
    #[serde(default)]
    pub currency_unit_on_hover: bool,
    /// Show Auto, Log and Percent buttons in chart navigation.
    pub show_scale_mode_buttons: bool,
    /// Enable the crosshair price tag action that opens the host's price menu.
    pub show_plus_button: bool,
    /// Show the host-supplied extended-session price tag independently of the last price.
    pub show_extended_session_price: bool,
    /// Colour of the extended-session price tag.
    pub extended_session_price_color: Color32,
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
    /// Write the main series' change from the previous session's close, absolute and in
    /// percent, under its last value tag, as the legend writes its day change. Shown only with
    /// the tag and when an earlier session has a close. Off by default.
    #[serde(default)]
    pub show_symbol_last_change: bool,
    /// Draw a line at the latest close independently of its tag.
    pub show_price_line: bool,
    /// Draw a line at the preceding session close independently of its tag.
    pub show_prev_close_line: bool,
    /// Colour, width and pattern of the previous close line and its tag. Without a colour it is
    /// mid grey, #555555, in a light theme and follows the theme's secondary text colour in a
    /// dark one.
    #[serde(default)]
    pub prev_close_line: ChartLineStyle,
    /// Whether to show previous close value line
    pub show_symbol_prev_close: bool,
    /// Whether to show indicator name labels
    pub show_indicator_labels: bool,
    /// Whether to show indicator last values
    pub show_indicator_last_val: bool,
    /// Preserve price units per bar when the time scale zooms or the pane resizes.
    pub lock_price_to_bar_ratio: bool,
    /// Exclude overlay studies and their primitives from the main price scale's autoscale
    /// range, without hiding them or changing their individual autoscale settings.
    #[serde(default)]
    pub scale_price_chart_only: bool,
    /// Draw the latest valid bid and ask tags supplied by the host on the price axis, and their
    /// lines while [`show_bid_ask_lines`](Self::show_bid_ask_lines) is `None`.
    pub show_bid_ask: bool,
    /// Draw the latest valid bid and ask lines across the pane. `None` draws them with
    /// [`show_bid_ask`](Self::show_bid_ask), as configurations saved before this field do.
    /// Default: None
    #[serde(default)]
    pub show_bid_ask_lines: Option<bool>,
    /// Colour, width and pattern of the bid line and tag. Without a colour it follows the
    /// theme's accent.
    #[serde(default)]
    pub bid_line: ChartLineStyle,
    /// Colour, width and pattern of the ask line and tag. Without a colour it follows the
    /// theme's falling candle colour.
    #[serde(default)]
    pub ask_line: ChartLineStyle,
    /// Tag the highest high and lowest low of the visible bars on the price scale, and draw
    /// their lines while [`show_high_low_lines`](Self::show_high_low_lines) is `None`.
    pub show_high_low: bool,
    /// Draw the high and low lines apart from their tags. `None` follows
    /// [`show_high_low`](Self::show_high_low), preserving older saved charts.
    /// Default: None
    #[serde(default)]
    pub show_high_low_lines: Option<bool>,
    /// Colour, width and pattern of the high line and tag. Without a colour it follows the
    /// rising candle colour.
    #[serde(default)]
    pub high_line: ChartLineStyle,
    /// Colour, width and pattern of the low line and tag. Without a colour it follows the
    /// falling candle colour.
    #[serde(default)]
    pub low_line: ChartLineStyle,
    /// Draw a line at the mean close of the visible bars, which moves with the view as it scrolls
    /// and zooms. Bars without a close are left out.
    /// Default: false
    #[serde(default)]
    pub show_average_close_line: bool,
    /// Tag the mean close of the visible bars on the price scale, independently of its line.
    /// Default: false
    #[serde(default)]
    pub show_average_close_label: bool,
    /// Colour, width and pattern of the average close line and tag. Without a colour it follows
    /// the theme's information colour.
    #[serde(default)]
    pub average_close_line: ChartLineStyle,
    /// Whether to show countdown to next candle
    pub show_countdown: bool,
    /// Whether to show the scroll-to-latest button when scrolled away from the live edge.
    pub show_realtime_btn: bool,
    /// Position of the realtime button; bottom-right by default.
    pub realtime_button_pos: RealtimeButtonPos,
    /// Custom text for the realtime button (`None` draws a double right chevron).
    pub realtime_button_text: Option<String>,
    /// Size of the realtime button (width, height), 28 by 28 points by default.
    pub realtime_button_size: (f32, f32),
    /// Background color for the realtime button; `PLACEHOLDER` follows the theme.
    pub realtime_button_color: Color32,
    /// Hover background color for the realtime button; `PLACEHOLDER` follows the theme.
    pub realtime_button_hover_color: Color32,
    /// Text or icon color for the realtime button; `PLACEHOLDER` follows the theme.
    pub realtime_button_text_color: Color32,
    /// When to show the navigation buttons. Default: [`NavigationButtons::OnHover`].
    pub navigation_buttons: NavigationButtons,
    /// When to show the maximize and menu buttons of the panes.
    #[serde(default = "pane_buttons_on_hover")]
    pub pane_buttons: NavigationButtons,
    /// Draw a vertical line at the first bar of each trading session, or of each day without
    /// trading hours, on intraday intervals. Daily and longer intervals draw none while this is
    /// on.
    pub show_session_breaks: bool,
    /// Colour of session break lines. Default: the theme's
    /// [`ChartColors::session_break`](crate::theme::ChartColors::session_break), the same blue in
    /// every built-in theme.
    pub session_break_color: Color32,
    /// Pattern of session break lines.
    pub session_break_style: LineStyle,
    /// Width of session break lines in points; a width that is not positive and finite draws
    /// none. Configurations saved without this field draw 1 point lines.
    /// Default: 1
    #[serde(default = "session_break_width")]
    pub session_break_width: f32,
    /// Session configuration (market hours)
    pub session_config: Option<SessionConfig>,
    /// Shade bars identified as extended or overnight hours by the session metadata.
    pub show_extended_hours: bool,
    /// Fill of the pre-market and the post-market where they set no colour of their own.
    /// `PLACEHOLDER` uses the active theme accent at low opacity.
    pub extended_hours_color: Color32,
    /// Analyst targets above the close on their publication date: their lines and labels, drawn
    /// opaque, and the area between those lines and that close, filled at this colour's
    /// opacity. `PLACEHOLDER` uses the active theme's up candle colour, filled at 20% opacity. A
    /// fully transparent colour fills nothing and draws the lines in the theme's colour.
    #[serde(default = "theme_color")]
    pub price_target_positive_color: Color32,
    /// Analyst targets below the close on their publication date, as
    /// [`price_target_positive_color`](Self::price_target_positive_color) with the theme's down
    /// candle colour.
    #[serde(default = "theme_color")]
    pub price_target_negative_color: Color32,
    /// Fill of the pre-market: extended hours that regular hours of the same trading date
    /// follow. `None` uses [`extended_hours_color`](Self::extended_hours_color).
    /// New charts use orange (#FF9800) at 8% opacity; older saved configurations keep the fallback.
    #[serde(default)]
    pub premarket_color: Option<Color32>,
    /// Fill of the post-market: extended hours that no regular hours of the same trading date
    /// follow. `None` uses [`extended_hours_color`](Self::extended_hours_color).
    /// New charts use blue (#2962FF) at 8% opacity; older saved configurations keep the fallback.
    #[serde(default)]
    pub postmarket_color: Option<Color32>,
    /// Shade the pre-market while [`show_extended_hours`](Self::show_extended_hours) is on.
    /// Configurations saved without this field shade it.
    /// Default: true
    #[serde(default = "shaded")]
    pub shade_premarket: bool,
    /// Shade the post-market while [`show_extended_hours`](Self::show_extended_hours) is on.
    /// Configurations saved without this field shade it.
    /// Default: true
    #[serde(default = "shaded")]
    pub shade_postmarket: bool,
    /// Fill for overnight windows. New charts use purple (#D500F9) at 8% opacity. `PLACEHOLDER`,
    /// which configurations saved before that default or without this field hold, follows the
    /// active theme accent at twice the extended-hours opacity.
    #[serde(default = "theme_color")]
    pub overnight_hours_color: Color32,
    /// Shade overnight windows while [`show_extended_hours`](Self::show_extended_hours) is on.
    /// Configurations saved without this field shade them.
    /// Default: true
    #[serde(default = "shaded")]
    pub shade_overnight: bool,
    /// Whether to show the symbol watermark.
    pub show_watermark: bool,
    /// Text of the symbol watermark; `\n` starts another line. `None` shows the symbol and the
    /// interval, such as `AAPL, 1D`, which follow symbol and interval changes.
    pub watermark_text: Option<String>,
    /// Without `watermark_text`, adds the instrument's description on a second line. Absent in
    /// configurations saved before it existed, where it reads as `false`.
    #[serde(default)]
    pub watermark_description: bool,
    /// Watermark color (typically semi-transparent)
    pub watermark_color: Color32,
    /// Font size of `watermark_text`. The symbol and interval line is twice this size and the
    /// description line 11/12 of it.
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
    /// Top-center of the chart
    TopCenter,
    /// Top-right of the chart
    TopRight,
    /// Bottom-left of the chart
    BottomLeft,
    /// Bottom-center of the chart
    BottomCenter,
    /// Bottom-right of the chart (default)
    #[default]
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
            conflation_threshold: pixel_conflation_threshold(),
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
            axis_font_size: None,
            axis_font_family: egui::FontFamily::Proportional,
            thousands_sep: Some(','),
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
            quote_panel: false,
            show_ohlc_change: true,
            show_ohlc_change_percent: true,
            show_legend_volume: true,
            show_indicator_inputs: true,
            show_legend_badge: true,
            show_market_status: true,
            show_last_day_change: false,
            indicator_backdrop_opacity: 0.6,
            show_currency: true,
            show_unit: true,
            currency_unit_on_hover: true,
            show_scale_mode_buttons: true,
            show_plus_button: true,
            show_extended_session_price: true,
            extended_session_price_color: Color32::from_rgb(251, 142, 1),
            show_right_axis: true,
            show_left_axis: false,
            right_axis_width: 70.0,
            left_axis_width: 70.0,
            price_axis: PriceAxisOptions::default(),
            right_axis_scale_mode: PriceScaleMode::Normal,
            left_axis_scale_mode: PriceScaleMode::Normal,
            show_symbol_labels: true,
            show_symbol_last_val: true,
            show_symbol_last_change: false,
            show_price_line: true,
            show_prev_close_line: false,
            prev_close_line: ChartLineStyle::default(),
            show_symbol_prev_close: false,
            show_indicator_labels: true,
            show_indicator_last_val: true,
            lock_price_to_bar_ratio: false,
            scale_price_chart_only: false,
            show_bid_ask: false,
            show_bid_ask_lines: None,
            bid_line: ChartLineStyle::default(),
            ask_line: ChartLineStyle::default(),
            show_high_low: false,
            show_high_low_lines: None,
            high_line: ChartLineStyle::default(),
            low_line: ChartLineStyle::default(),
            show_average_close_line: false,
            show_average_close_label: false,
            average_close_line: ChartLineStyle::default(),
            show_countdown: true,
            show_realtime_btn: true,
            realtime_button_pos: RealtimeButtonPos::BottomRight,
            realtime_button_text: None,
            realtime_button_size: (
                DESIGN_TOKENS.sizing.charts_ext.realtime_button_width,
                DESIGN_TOKENS.sizing.button_md,
            ),
            realtime_button_color: Color32::PLACEHOLDER,
            realtime_button_hover_color: Color32::PLACEHOLDER,
            realtime_button_text_color: Color32::PLACEHOLDER,
            navigation_buttons: NavigationButtons::default(),
            pane_buttons: pane_buttons_on_hover(),
            // Session breaks disabled by default
            show_session_breaks: false,
            session_break_color: chart.session_break,
            session_break_style: LineStyle::Dashed,
            session_break_width: session_break_width(),
            session_config: None,
            show_extended_hours: true,
            extended_hours_color: Color32::PLACEHOLDER,
            price_target_positive_color: Color32::PLACEHOLDER,
            price_target_negative_color: Color32::PLACEHOLDER,
            premarket_color: Some(Color32::from_rgba_unmultiplied(255, 152, 0, 20)),
            postmarket_color: Some(Color32::from_rgba_unmultiplied(41, 98, 255, 20)),
            shade_premarket: true,
            shade_postmarket: true,
            // #D500F9 at 8% opacity: round(0.08 * 255) = 20.
            overnight_hours_color: Color32::from_rgba_unmultiplied(213, 0, 249, 20),
            shade_overnight: true,
            // Watermark disabled by default
            show_watermark: false,
            watermark_text: None,
            watermark_description: false,
            watermark_color: chart.grid_line,
            watermark_font_size: 48.0,
            watermark_pos: WatermarkPos::Center,
            // Background enabled by default (set false when inside a container)
            skip_background: false,
        }
    }
}

impl ChartConfig {
    /// The line and fill colours on one side of the publication close, using that side's theme
    /// candle colour until a colour is chosen.
    pub(crate) fn price_target_colors(color: Color32, themed: Color32) -> (Color32, Color32) {
        if color == Color32::PLACEHOLDER {
            (themed, crate::model::HlcAreaStyle::fill_tint(themed, 51))
        } else if color.a() == 0 {
            // A fully transparent colour holds no RGB, so it has no opaque form.
            (themed, Color32::TRANSPARENT)
        } else {
            (color.to_opaque(), color)
        }
    }

    /// The colour an Analyst price forecast study shows for the choice `color`: the fill
    /// `PLACEHOLDER` stands for, made from the side's theme candle colour `themed`, or the
    /// choice itself.
    #[cfg(feature = "studies")]
    pub(crate) fn price_target_study_color(color: Color32, themed: Color32) -> Color32 {
        if color == Color32::PLACEHOLDER {
            Self::price_target_colors(color, themed).1
        } else {
            color
        }
    }

    /// The choice a study colour stands for: `PLACEHOLDER` when it is the theme's own fill,
    /// within the rounding of a colour picker, otherwise the colour itself.
    #[cfg(feature = "studies")]
    pub(crate) fn price_target_choice(color: Color32, themed: Color32) -> Color32 {
        let theme = Self::price_target_study_color(Color32::PLACEHOLDER, themed);
        let near = color
            .to_array()
            .iter()
            .zip(theme.to_array())
            .all(|(a, b)| a.abs_diff(b) <= 1);
        if near { Color32::PLACEHOLDER } else { color }
    }

    /// Apply the theme's background, grid, axis text, candle, session break and watermark
    /// colours to this configuration.
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
        self.session_break_color = theme.chart.session_break;
        self.watermark_color = if theme.dark {
            theme.chart.grid_line
        } else {
            // A cool ink: the interface text would give a neutral grey on the white plot.
            theme
                .chart
                .bg
                .lerp_to_gamma(Color32::from_rgb(10, 16, 33), 0.18)
        };
        self
    }

    /// The colour of the previous close line and its tag while `prev_close_line` sets none:
    /// mid grey, #555555, in a light theme, apart from the black scale text, and the theme's
    /// secondary text colour in a dark one.
    pub(crate) fn prev_close_fallback(theme: &Theme) -> Color32 {
        if theme.dark {
            theme.ui.text_secondary
        } else {
            Color32::from_gray(85)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configurations saved before the price line styles, the bid and ask line switch, the
    /// average close line, the session break width and the pre-market and post-market choices
    /// restore with today's lines: 1 point dotted price lines in their usual colours, bid and
    /// ask lines that follow their tags, 1 point breaks and both extended sessions shaded.
    #[test]
    fn configs_saved_before_line_styles_restore_todays_lines() {
        // A literal configuration saved by the base revision, before these fields existed.
        let saved = r#"{"bullish_color":[8,153,129,255],"bearish_color":[242,54,69,255],"bullish_border_color":[8,153,129,255],"bearish_border_color":[242,54,69,255],"bullish_wick_color":[8,153,129,255],"bearish_wick_color":[242,54,69,255],"price_source":"Close","conflation_threshold":1.0,"background_color":[0,0,0,255],"background_style":"Solid","grid_color":[34,34,34,255],"vertical_grid_color":null,"grid_line_style":"Dotted","vertical_grid_line_style":null,"text_color":[255,255,255,255],"axis_font_size":null,"axis_font_family":"Proportional","thousands_sep":",","candle_width":0.6,"wick_width":1.0,"candle_border_width":0.0,"show_grid":true,"show_horizontal_grid":true,"show_vertical_grid":true,"show_volume":false,"volume_height_fraction":0.12,"volume_baseline_inset":12.0,"padding":0.0,"show_time_labels":true,"show_ohlc_info":true,"title_as_description":true,"show_quote_boxes":false,"show_ohlc_change":true,"show_ohlc_change_percent":true,"show_legend_volume":true,"show_indicator_inputs":true,"show_legend_badge":true,"show_market_status":true,"show_last_day_change":false,"indicator_backdrop_opacity":0.6,"show_currency":true,"show_unit":true,"currency_unit_on_hover":false,"show_scale_mode_buttons":true,"show_plus_button":true,"show_extended_session_price":true,"extended_session_price_color":[251,142,1,255],"show_right_axis":true,"show_left_axis":false,"right_axis_width":70.0,"left_axis_width":70.0,"price_axis":{"allow_label_overlap":false,"show_study_values":true,"study_value_color":null,"show_symbol_name":false,"show_study_names":false,"border_visible":false,"border_color":null,"ticks_visible":false,"edge_ticks_visible":false,"hide_clipped_labels":true},"right_axis_scale_mode":"Normal","left_axis_scale_mode":"Normal","show_symbol_labels":true,"show_symbol_last_val":true,"show_symbol_last_change":false,"show_price_line":true,"show_prev_close_line":true,"show_symbol_prev_close":true,"show_indicator_labels":true,"show_indicator_last_val":true,"lock_price_to_bar_ratio":false,"scale_price_chart_only":false,"show_bid_ask":true,"show_high_low":true,"show_countdown":false,"show_realtime_btn":true,"realtime_button_pos":"BottomRight","realtime_button_text":null,"realtime_button_size":[28.0,28.0],"realtime_button_color":[64,254,0,128],"realtime_button_hover_color":[64,254,0,128],"realtime_button_text_color":[64,254,0,128],"navigation_buttons":"Never","pane_buttons":"OnHover","show_session_breaks":false,"session_break_color":[18,18,18,18],"session_break_style":"Solid","session_config":null,"show_extended_hours":true,"extended_hours_color":[64,254,0,128],"overnight_hours_color":[64,254,0,128],"show_watermark":false,"watermark_text":null,"watermark_description":false,"watermark_color":[34,34,34,255],"watermark_font_size":48.0,"watermark_pos":"Center","skip_background":false}"#;
        let restored: ChartConfig = serde_json::from_str(saved).unwrap();
        let original: serde_json::Value = serde_json::from_str(saved).unwrap();
        let current: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&restored).unwrap()).unwrap();
        for (field, value) in original.as_object().unwrap() {
            assert_eq!(&current[field], value, "{field}");
        }
        for line in [
            restored.high_line,
            restored.low_line,
            restored.bid_line,
            restored.ask_line,
            restored.prev_close_line,
            restored.average_close_line,
        ] {
            assert_eq!(line, ChartLineStyle::default());
        }
        assert_eq!(restored.show_high_low_lines, None);
        assert!(!restored.show_average_close_line && !restored.show_average_close_label);
        assert_eq!(
            (restored.premarket_color, restored.postmarket_color),
            (None, None)
        );
        let line = restored.prev_close_line;
        assert_eq!(
            (
                line.color,
                line.width,
                line.style,
                restored.show_bid_ask_lines,
                restored.session_break_width,
                restored.shade_premarket && restored.shade_postmarket && restored.shade_overnight,
            ),
            (None, 1.0, LineStyle::Dotted, None, 1.0, true)
        );
    }

    #[test]
    fn saved_axis_family_defaults_and_round_trips_host_names() {
        let mut saved = serde_json::to_value(ChartConfig::default()).unwrap();
        saved.as_object_mut().unwrap().remove("axis_font_family");
        let restored: ChartConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.axis_font_family, egui::FontFamily::Proportional);
        for family in [
            egui::FontFamily::Monospace,
            egui::FontFamily::Name("Host Sans".into()),
        ] {
            let config = ChartConfig {
                axis_font_family: family.clone(),
                ..Default::default()
            };
            let restored: ChartConfig =
                serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
            assert_eq!(restored.axis_font_family, family);
        }
    }

    #[test]
    fn with_theme_recolours_session_breaks_and_watermark() {
        assert_eq!(
            ChartConfig::default().session_break_color,
            Color32::from_rgb(73, 133, 231)
        );
        for (name, theme) in crate::theme::presets() {
            let config = ChartConfig::default().with_theme(&theme);
            assert_eq!(
                config.session_break_color,
                Color32::from_rgb(73, 133, 231),
                "{name}"
            );
        }
        let mut theme = Theme::light();
        theme.chart.session_break = Color32::RED;
        let config = ChartConfig::default().with_theme(&theme);
        assert_eq!(config.session_break_color, Color32::RED);
        assert_eq!(config.watermark_color, Color32::from_rgb(211, 212, 215));
    }

    #[test]
    fn saved_conflation_threshold_keeps_disabled_custom_and_legacy_modes() {
        let mut saved = serde_json::to_value(ChartConfig::default()).unwrap();
        saved
            .as_object_mut()
            .unwrap()
            .remove("conflation_threshold");
        let restored: ChartConfig = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(restored.conflation_threshold, Some(1.0));
        for threshold in [None, Some(0.5), Some(2.0)] {
            saved["conflation_threshold"] = serde_json::to_value(threshold).unwrap();
            let restored: ChartConfig = serde_json::from_value(saved.clone()).unwrap();
            assert_eq!(restored.conflation_threshold, threshold);
        }
    }
}
