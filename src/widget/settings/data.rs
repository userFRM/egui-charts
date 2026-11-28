//! Data structures for settings state - Canvas style

use super::types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
use crate::chart::{PnlUnit, TradingLabelAlignment};
use crate::model::LineStyle;
use egui::Color32;

// ============================================================================
// Settings State
// ============================================================================

/// Backgrounds of supplied extended and overnight bars. Colours include opacity; absent phase
/// colours use the shared extended-session colour, whose placeholder follows the theme.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct SessionBackgroundSettings {
    /// Highlight extended and overnight sessions on intraday charts.
    pub show_extended_hours: bool,
    /// Shared extended-session fill; `PLACEHOLDER` follows the theme at 8% opacity.
    pub extended_hours_color: Color32,
    /// Pre-market fill, or the shared fill when absent.
    pub premarket_color: Option<Color32>,
    /// Post-market fill, or the shared fill when absent.
    pub postmarket_color: Option<Color32>,
    /// Highlight the pre-market while session highlighting is enabled.
    pub shade_premarket: bool,
    /// Highlight the post-market while session highlighting is enabled.
    pub shade_postmarket: bool,
    /// Highlight overnight windows while session highlighting is enabled.
    pub shade_overnight: bool,
    /// Overnight fill; new settings use #D500F9 at 8% opacity, and `PLACEHOLDER` follows the
    /// theme accent at 16% opacity.
    pub overnight_hours_color: Color32,
}

impl Default for SessionBackgroundSettings {
    fn default() -> Self {
        let config = crate::config::ChartConfig::default();
        Self {
            show_extended_hours: config.show_extended_hours,
            extended_hours_color: config.extended_hours_color,
            premarket_color: config.premarket_color,
            postmarket_color: config.postmarket_color,
            shade_premarket: config.shade_premarket,
            shade_postmarket: config.shade_postmarket,
            shade_overnight: config.shade_overnight,
            overnight_hours_color: config.overnight_hours_color,
        }
    }
}

/// Candle color configuration (body, border, wick for up/down candles) and which parts are drawn.
/// Saved settings without a field take its default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct CandleColorConfig {
    /// Colour plain candles by their close against the previous close instead of their open.
    /// Bars, volume candles and Heikin Ashi keep their rule in `ChartSettingsState::chart_type_params`.
    pub color_by_previous_close: bool,
    /// Whether candle bodies are filled.
    pub show_body: bool,
    /// Whether candle borders are drawn.
    pub show_border: bool,
    /// Whether candle wicks are drawn.
    pub show_wick: bool,
    /// Body color of rising candles.
    pub body_up: Color32,
    /// Body color of falling candles.
    pub body_down: Color32,
    /// Border color of rising candles.
    pub border_up: Color32,
    /// Border color of falling candles.
    pub border_down: Color32,
    /// Wick color of rising candles.
    pub wick_up: Color32,
    /// Wick color of falling candles.
    pub wick_down: Color32,
}

impl CandleColorConfig {
    /// Candle body, border and wick colours from `theme`.
    pub fn from_theme(theme: &crate::theme::Theme) -> Self {
        let chart = &theme.chart;
        Self {
            color_by_previous_close: false,
            show_body: true,
            show_border: false,
            show_wick: true,
            body_up: chart.candle_up,
            body_down: chart.candle_down,
            border_up: chart.candle_up,
            border_down: chart.candle_down,
            wick_up: chart.candle_up,
            wick_down: chart.candle_down,
        }
    }
}

impl Default for CandleColorConfig {
    fn default() -> Self {
        Self::from_theme(&crate::theme::Theme::default())
    }
}

/// Status line display options. Saved settings without a field take its default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct StatusLineOptions {
    /// Whether the legend shows the symbol's logo before the title, or its initial on a neutral
    /// disc without one.
    pub show_logo: bool,
    /// Whether the legend shows the instrument title.
    pub show_symbol: bool,
    /// Display the description instead of the ticker.
    pub title_as_description: bool,
    /// Whether the legend shows the market status after the title: the session's state and
    /// delayed or end-of-day data.
    pub show_market_status: bool,
    /// Display current bid and ask controls.
    pub show_quote_boxes: bool,
    /// Show Buy and Sell as a movable panel over the price pane instead of in the legend.
    pub quote_panel: bool,
    /// Display study names.
    pub show_indicator_titles: bool,
    /// Display study inputs after their names: the host's caption, else one built from the
    /// study's inputs.
    pub show_indicator_inputs: bool,
    /// Display historical study output values.
    pub show_indicator_values: bool,
    /// Whether the legend shows open, high, low and close.
    pub show_ohlc: bool,
    /// Whether the legend shows the absolute price change.
    pub show_change: bool,
    /// Whether the legend shows the percentage price change.
    pub show_change_percent: bool,
    /// Whether the legend shows volume. Off by default.
    pub show_volume: bool,
    /// Whether the legend shows the change of the last price from the previous session's close.
    pub show_last_day_change: bool,
    /// Whether the study rows' backdrop is shown; off keeps its opacity.
    pub show_indicator_background: bool,
    /// Opacity of the study rows' backdrop, from 0 to 100 percent.
    pub indicator_background_opacity: u8,
}

impl Default for StatusLineOptions {
    fn default() -> Self {
        Self {
            show_logo: true,
            show_symbol: true,
            title_as_description: true,
            show_market_status: true,
            show_quote_boxes: true,
            quote_panel: false,
            show_indicator_titles: true,
            show_indicator_inputs: true,
            show_indicator_values: true,
            show_ohlc: true,
            show_change: true,
            show_change_percent: true,
            show_volume: false,
            show_last_day_change: false,
            show_indicator_background: true,
            indicator_background_opacity: 60,
        }
    }
}

/// Canvas tab: Chart basic styles settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChartBasicStylesSettings {
    /// Solid or gradient chart background.
    pub background_type: BackgroundType,
    /// `Color32::PLACEHOLDER` follows the active chart background until a colour is chosen.
    pub background_color: Color32,
    /// First gradient endpoint color.
    pub background_gradient_top: Color32,
    /// Second gradient endpoint color.
    pub background_gradient_bottom: Color32,
}

impl Default for ChartBasicStylesSettings {
    fn default() -> Self {
        Self {
            background_type: BackgroundType::Solid,
            background_color: Color32::PLACEHOLDER,
            background_gradient_top: Color32::PLACEHOLDER,
            background_gradient_bottom: Color32::PLACEHOLDER,
        }
    }
}

/// Canvas tab: Grid lines settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GridLinesSettings {
    /// Display mode.
    pub mode: GridLinesMode,
    /// Color of horizontal grid lines.
    pub horizontal_color: Color32,
    /// Color of vertical grid lines.
    pub vertical_color: Color32,
}

impl Default for GridLinesSettings {
    fn default() -> Self {
        Self {
            mode: GridLinesMode::Both,
            horizontal_color: crate::theme::Theme::default().chart.grid_line,
            vertical_color: crate::theme::Theme::default().chart.grid_line,
        }
    }
}

/// Canvas tab: Crosshair settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CrosshairSettings {
    /// Element color.
    pub color: Color32,
    /// Stroke width in logical pixels.
    pub line_width: f32,
    /// Stroke pattern.
    pub line_style: LineStyle,
}

impl Default for CrosshairSettings {
    fn default() -> Self {
        Self {
            color: crate::theme::Theme::default().chart.axis_text_secondary,
            line_width: 1.0,
            line_style: LineStyle::Dashed,
        }
    }
}

/// Which crosshair lines and axis labels the Canvas tab shows. Lines show only in the full
/// crosshair style.
///
/// ```
/// use egui_charts::widget::settings::CrosshairVisibility;
///
/// let mut shown = CrosshairVisibility::default();
/// shown.horizontal_line = false;
/// shown.price_label = false;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct CrosshairVisibility {
    /// Draw the vertical line.
    pub vertical_line: bool,
    /// Draw the horizontal line.
    pub horizontal_line: bool,
    /// Draw the time label on the time axis.
    pub time_label: bool,
    /// Draw the price label on the price axis, and the values of volume and studies.
    pub price_label: bool,
}

impl Default for CrosshairVisibility {
    /// Everything shown, as on a new chart.
    fn default() -> Self {
        Self {
            vertical_line: true,
            horizontal_line: true,
            time_label: true,
            price_label: true,
        }
    }
}

/// Canvas tab: Watermark settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatermarkSettings {
    /// Display mode.
    pub mode: WatermarkMode,
    /// Element color.
    pub color: Color32,
}

impl Default for WatermarkSettings {
    fn default() -> Self {
        Self {
            mode: WatermarkMode::ReplayMode,
            color: crate::theme::Theme::default()
                .ui
                .text_muted
                .gamma_multiply(0.2), // ~20% opacity
        }
    }
}

/// Canvas tab: Scales appearance settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ScalesAppearanceSettings {
    /// Text color.
    pub text_color: Color32,
    /// Font size in logical pixels.
    pub font_size: u8,
    /// Font family of the axis labels and tags. `None` keeps the chart's family, as do settings
    /// saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<egui::FontFamily>,
    /// Axis line color.
    pub lines_color: Color32,
}

impl Default for ScalesAppearanceSettings {
    fn default() -> Self {
        Self {
            text_color: crate::theme::Theme::default().ui.text_secondary,
            font_size: 12,
            font_family: None,
            lines_color: Color32::TRANSPARENT,
        }
    }
}

/// Canvas tab: Buttons visibility settings. Both groups show on mouse over by default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ButtonsSettings {
    /// Visibility of chart navigation controls.
    pub navigation: ButtonVisibility,
    /// Visibility of pane controls.
    pub pane: ButtonVisibility,
}

impl Default for ButtonsSettings {
    fn default() -> Self {
        Self {
            navigation: ButtonVisibility::VisibleOnMouseOver,
            pane: ButtonVisibility::VisibleOnMouseOver,
        }
    }
}

/// Canvas tab: Margins settings
///
/// Built from [`Default`] or [`ChartSettingsState::from_chart`], then filled field by field:
///
/// ```
/// use egui_charts::widget::settings::{ChartSettingsState, MarginsSettings};
///
/// let mut margins = MarginsSettings::default();
/// margins.right_pixels = Some(60.0);
/// let mut settings = ChartSettingsState::default();
/// settings.margins = margins;
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct MarginsSettings {
    /// Empty space above prices, as a percentage of pane height.
    pub top_percent: f32,
    /// Empty space below prices, as a percentage of pane height.
    pub bottom_percent: f32,
    /// Empty space after the latest bar, in bars; a margin set in pixels reads as its width in
    /// bars at the current bar spacing. Settings without
    /// [`ScalesAppearanceSettings::font_family`], as saved when this held whole bars, keep the
    /// chart's margin when this equals its whole bars.
    pub right_bars: f32,
    /// Empty space after the latest bar in pixels, for a chart that keeps its margin in pixels.
    /// It has priority over `right_bars`, as
    /// [`TimeScaleOptions::right_offset_pixels`](crate::config::TimeScaleOptions::right_offset_pixels)
    /// has over `right_offset`, so that every chart the settings reach keeps the same pixels at
    /// its own bar spacing. `None` applies `right_bars`; the settings dialog clears it when the
    /// margin is edited, and settings saved without it hold `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_pixels: Option<f32>,
}

impl Default for MarginsSettings {
    /// The margins of a new chart, read as the settings read a chart's.
    fn default() -> Self {
        let price = crate::scales::PriceScaleMargins::CHART;
        Self {
            top_percent: (price.top * 100.0) as f32,
            bottom_percent: (price.bottom * 100.0) as f32,
            right_bars: crate::config::TimeScaleOptions::default().right_offset,
            right_pixels: None,
        }
    }
}

/// The character that groups the thousands of the built-in price labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ThousandsSeparator {
    /// Plain digits: `42541.77`.
    Off,
    /// A comma: `42,541.77`.
    Comma,
    /// A space: `42 541.77`.
    Space,
    /// An apostrophe: `42'541.77`.
    Apostrophe,
    /// A character the host set as `ChartConfig::thousands_sep`.
    Other(char),
}

/// The date of crosshair, tooltip and forecast labels, as
/// [`Chart::set_date_format`](crate::Chart::set_date_format) sets it.
///
/// Built from a pattern and set field by field:
///
/// ```
/// use egui_charts::scales::WeekdayNames;
/// use egui_charts::widget::settings::{ChartSettingsState, DateFormat};
///
/// let mut settings = ChartSettingsState::default();
/// settings.scales_and_lines.date_format = Some(DateFormat::Pattern("%d.%m.%Y".into()));
/// settings.scales_and_lines.use_24_hour = Some(false);
/// settings.scales_and_lines.weekday_names = Some(WeekdayNames::Always);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum DateFormat {
    /// Built-in dates: `Wed 13 Mar '24` on intraday bars and `Mar 13, 2024` on daily bars.
    Default,
    /// A `strftime` pattern, such as `"%d.%m.%Y"`; invalid or empty patterns use built-in dates.
    Pattern(String),
}

/// Scales and lines tab settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ScalesAndLinesSettings {
    /// Axis layout of the pane containing the main series.
    pub axis_layout: crate::model::PaneAxisOptions,
    /// Display host-supplied currency metadata.
    pub show_currency: bool,
    /// Display host-supplied price unit metadata.
    pub show_unit: bool,
    /// Show the currency and unit only while the pointer is over the price scale. Settings saved
    /// without this field keep them always visible.
    #[serde(default)]
    pub currency_unit_on_hover: bool,
    /// Display Auto, Log and Percent navigation buttons.
    pub show_scale_mode_buttons: bool,
    /// Display the price-axis host action button.
    pub show_plus_button: bool,
    /// Let value tags overlap instead of moving them apart.
    pub allow_label_overlap: bool,
    /// Lock scale units per bar to `price_to_bar_ratio`.
    pub lock_price_to_bar_ratio: bool,
    /// Positive scale units per bar while locked; log scales use natural logarithms of prices.
    /// Read from a chart that is not locked, it is the ratio of the view the chart shows, so ticking
    /// the lock keeps that view; 1 for a chart not drawn yet.
    pub price_to_bar_ratio: f64,
    /// Last price tag and line colour, or the candle direction colour when unset.
    pub symbol_price_color: Option<Color32>,
    /// Last price line width in logical points. `None` keeps the chart's width, as do settings
    /// saved without this field.
    pub price_line_width: Option<f32>,
    /// Last price line pattern. `None` keeps the chart's pattern, as do settings saved without
    /// this field.
    pub price_line_style: Option<crate::model::LineStyle>,
    /// Write the change of the last price from the previous session's close under the last
    /// price tag, absolute and in percent.
    pub show_last_price_change: bool,
    /// Display the last values of studies on their price axes.
    pub show_study_values: bool,
    /// Study value tag colour, or each plot's colour when unset.
    pub study_value_color: Option<Color32>,
    /// Write the symbol before its value tags on the price scale, independently of its last
    /// value and legend title.
    pub show_symbol_name: bool,
    /// Write each study's name before its value tags, independently of the values and legend
    /// titles.
    pub show_study_names: bool,
    /// Display the host-supplied extended-session price tag.
    pub show_extended_session_price: bool,
    /// Extended-session price tag colour.
    pub extended_session_price_color: Color32,
    /// Whether visible prices determine the scale range.
    pub auto_scale: bool,
    /// Whether prices use a logarithmic scale.
    pub log_scale: bool,
    /// Whether prices are shown as percentage changes.
    pub percentage_scale: bool,
    /// Whether prices are indexed to 100 when neither [`log_scale`](Self::log_scale) nor
    /// [`percentage_scale`](Self::percentage_scale) is on. `None` keeps a chart indexed to 100
    /// as it is and gives any other chart a regular scale, as do settings saved without this
    /// field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_to_100: Option<bool>,
    /// Whether increasing prices run downward.
    pub invert_scale: bool,
    /// Show the price scale on the left instead of the right.
    pub price_scale_left: bool,
    /// Display the current close tag independently of its line.
    pub show_last_price_label: bool,
    /// Display the previous session close tag independently of its line.
    pub show_prev_close_label: bool,
    /// Whether to show price line.
    pub show_price_line: bool,
    /// The bar whose close the last price line and label of the main series show. `None` keeps
    /// the chart's choice, as do settings saved without this field.
    pub price_line_source: Option<crate::model::PriceLineSource>,
    /// The thousands separator of price labels. `None` keeps the chart's separator, as do
    /// settings saved without this field.
    pub thousands_sep: Option<ThousandsSeparator>,
    /// The date of crosshair, tooltip and forecast labels. `None` keeps the chart's format,
    /// as do settings saved without this field.
    pub date_format: Option<DateFormat>,
    /// Whether time labels use a 24-hour clock or a 12-hour one with an AM/PM marker.
    /// `None` keeps the chart's clock, as do settings saved without this field.
    pub use_24_hour: Option<bool>,
    /// Whether crosshair, tooltip and forecast dates name the weekday. `None` keeps the chart's
    /// choice, as do settings saved without this field.
    pub weekday_names: Option<crate::scales::WeekdayNames>,
    /// Whether an interval change keeps the bar at the left edge in place, as
    /// [`TimeScaleOptions::keep_left_edge_on_interval_change`](crate::config::TimeScaleOptions::keep_left_edge_on_interval_change)
    /// does. `None` keeps the chart's choice, as do settings saved without this field.
    pub keep_left_edge_on_interval_change: Option<bool>,
    /// Whether to show countdown.
    pub show_countdown: bool,
    /// Whether to show the bid and ask tags.
    pub show_bid_ask: bool,
    /// Whether to draw the bid and ask lines, apart from their tags. `None` keeps the chart's
    /// choice, as do settings saved without this field.
    pub show_bid_ask_lines: Option<bool>,
    /// Whether to show the high and low tags.
    pub show_high_low: bool,
    /// Whether to draw the high and low lines, apart from their tags. `None` keeps the chart's
    /// choice, as do settings saved without this field.
    pub show_high_low_lines: Option<bool>,
    /// Whether to show prev close.
    pub show_prev_close: bool,
    /// Colour, width and pattern of the previous close line. `None` keeps the chart's, as do
    /// settings saved without this field.
    pub prev_close_line: Option<crate::config::ChartLineStyle>,
}

impl Default for ScalesAndLinesSettings {
    fn default() -> Self {
        Self {
            axis_layout: Default::default(),
            show_currency: true,
            show_unit: true,
            currency_unit_on_hover: true,
            show_scale_mode_buttons: true,
            show_plus_button: true,
            allow_label_overlap: false,
            lock_price_to_bar_ratio: false,
            price_to_bar_ratio: 1.0,
            symbol_price_color: None,
            price_line_width: None,
            price_line_style: None,
            show_last_price_change: false,
            show_study_values: true,
            study_value_color: None,
            show_symbol_name: false,
            show_study_names: false,
            show_extended_session_price: true,
            extended_session_price_color: Color32::from_rgb(251, 142, 1),
            auto_scale: true,
            log_scale: false,
            percentage_scale: false,
            indexed_to_100: None,
            invert_scale: false,
            price_scale_left: false,
            show_last_price_label: true,
            show_prev_close_label: false,
            show_price_line: true,
            price_line_source: None,
            thousands_sep: None,
            date_format: None,
            use_24_hour: None,
            weekday_names: None,
            keep_left_edge_on_interval_change: None,
            show_countdown: true,
            show_bid_ask: false,
            show_bid_ask_lines: None,
            show_high_low: false,
            show_high_low_lines: None,
            show_prev_close: false,
            prev_close_line: None,
        }
    }
}

/// Trading tab settings: the Buy and Sell buttons' behaviour, notifications and the layers
/// `ui::trading::Trading::show` draws through `Trading::display` with the `ui` feature.
/// The Buy and Sell buttons themselves are [`StatusLineOptions::show_quote_boxes`].
/// Saved settings without a field take its default, unless the field says otherwise.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TradingSettings {
    /// Buy and Sell place a market order of the order quantity at once instead of opening the
    /// order ticket.
    pub instant_orders: bool,
    /// Show only side, order type, prices and quantity above the send button, with the order's
    /// exits listed one per line. The ticket's More menu switches it.
    pub compact_ticket: bool,
    /// Show the order ticket as a movable window over the chart, leaving the chart its full
    /// width. The ticket header switches it. Defaults to false, including for settings saved
    /// without this field.
    pub undocked_ticket: bool,
    /// Show current Bid and Ask tags supplied by the quote source.
    pub bid_ask_labels: bool,
    /// Ask the host to play [`execution_sound`](Self::execution_sound) for every fill, through
    /// `ui::trading::Trading::take_execution_sound` with the `ui` feature.
    pub play_execution_sound: bool,
    /// The name of the sound the host plays for fills, one of the
    /// `SettingsDialog::execution_sounds` with the `ui` feature; empty for the host's first.
    pub execution_sound: String,
    /// Local execution sound gain, from zero (muted) to one.
    pub execution_sound_volume: f32,
    /// Which trading events raise a notification.
    pub notifications: TradingNotifications,
    /// Draw the open positions.
    pub show_positions: bool,
    /// Give each position's profit or loss on its label.
    pub position_pnl: bool,
    /// The unit of the positions' profit or loss, on their labels and in the depth ladder.
    pub position_pnl_unit: PnlUnit,
    /// Show a position's reverse button while its label is hovered.
    pub reverse_button: bool,
    /// Extend trading lines to the left edge of the pane instead of starting at their labels.
    pub extend_lines: bool,
    /// Horizontal placement of order and position labels.
    pub label_alignment: TradingLabelAlignment,
    /// Separate overlapping order and position labels vertically, keeping their lines, tags
    /// and buttons. The label menus' No overlap switch; off for new and older saved settings.
    pub separate_labels: bool,
    /// Include the visible trading overlays in images exported through `ui::trading::Trading`.
    pub include_in_image: bool,
    /// Draw the working orders and their brackets.
    pub show_orders: bool,
    /// Give on each take profit and stop loss the profit or loss of reaching it.
    pub bracket_pnl: bool,
    /// The unit of the brackets' profit or loss.
    pub bracket_pnl_unit: PnlUnit,
    /// Draw the fills on their bars.
    pub show_executions: bool,
    /// Write the side and total quantity beside each fill's mark. On for new settings; settings
    /// saved without this field keep labels off, as they were drawn when saved.
    #[serde(default)]
    pub show_execution_labels: bool,
    /// Add the quantity-weighted average fill price to each mark's label, as in "Buy 2 @ 101.25".
    /// Off by default, including for settings saved without this field.
    pub show_execution_prices: bool,
}

impl Default for TradingSettings {
    fn default() -> Self {
        Self {
            instant_orders: false,
            compact_ticket: false,
            undocked_ticket: false,
            bid_ask_labels: false,
            play_execution_sound: false,
            execution_sound: String::new(),
            execution_sound_volume: 1.0,
            notifications: TradingNotifications::All,
            show_positions: true,
            position_pnl: true,
            position_pnl_unit: PnlUnit::Money,
            reverse_button: true,
            extend_lines: true,
            label_alignment: TradingLabelAlignment::Right,
            separate_labels: false,
            include_in_image: false,
            show_orders: true,
            bracket_pnl: true,
            bracket_pnl_unit: PnlUnit::Money,
            show_executions: true,
            show_execution_labels: true,
            show_execution_prices: false,
        }
    }
}

/// Which trading events raise a notification: the levels of the toasts they use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum TradingNotifications {
    /// Everything Errors shows, and fills, partial fills, placed orders, orders the broker
    /// cancelled and a broker that is live again.
    #[default]
    All,
    /// Rejected orders, refused orders, cancellations and changes, and a broker that stops
    /// being live.
    Errors,
    /// None.
    Off,
}

impl TradingNotifications {
    /// Every level, in the order a menu lists them.
    pub const ALL: [Self; 3] = [Self::All, Self::Errors, Self::Off];

    /// The level's name in menus.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All events",
            Self::Errors => "Errors only",
            Self::Off => "Off",
        }
    }
}

/// Alerts tab settings. Missing saved fields keep their defaults: toasts close after 20 seconds
/// and alerts stay off the desktop.
///
/// ```
/// use egui_charts::widget::settings::AlertsSettings;
///
/// let mut settings = AlertsSettings::default();
/// settings.desktop_notifications = true;
/// settings.close_toasts = false;
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct AlertsSettings {
    /// Host sound name played when an alert whose delivery keeps Sound fires; empty selects its
    /// first available sound.
    pub sound: String,
    /// Local alert sound gain, from zero (muted) to one.
    pub sound_volume: f32,
    /// Whether to show alerts.
    pub show_alerts: bool,
    /// Whether to show alert labels.
    pub show_alert_labels: bool,
    /// Alert color.
    pub alert_color: Color32,
    /// Draw only active alerts; otherwise triggered, disabled and expired alerts are drawn dimmed.
    pub show_active_only: bool,
    /// Seconds an alert notification stays on screen while [`close_toasts`](Self::close_toasts)
    /// holds.
    pub notification_seconds: u32,
    /// Close each alert toast after [`notification_seconds`](Self::notification_seconds);
    /// otherwise it stays until the user dismisses it.
    pub close_toasts: bool,
    /// Also show alert popups as desktop notifications, through the host's
    /// [`Notifications`](crate::services::Notifications) service.
    pub desktop_notifications: bool,
}

impl Default for AlertsSettings {
    fn default() -> Self {
        Self {
            sound: String::new(),
            sound_volume: 1.0,
            show_alerts: true,
            show_alert_labels: true,
            alert_color: crate::studies::palette::WARM_ORANGE,
            show_active_only: true,
            notification_seconds: 20,
            close_toasts: true,
            desktop_notifications: false,
        }
    }
}

/// Independent order-flow layers and their appearance. Missing saved fields keep layers off.
///
/// ```
/// use egui_charts::widget::settings::OrderFlowSettings;
///
/// let mut settings = OrderFlowSettings::default();
/// settings.show_heatmap = true;
/// ```
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct OrderFlowSettings {
    /// Draw sampled resting liquidity behind the price series.
    pub show_heatmap: bool,
    /// Brightness and resting-size range, retained while the heatmap is off.
    pub heatmap: crate::chart::overlays::LiquidityHeatmapConfig,
    /// How long to keep depth columns, see
    /// [`DepthHistoryConfig::retention`](crate::data::DepthHistoryConfig::retention). `None`, the
    /// default, keeps the full history, including for settings saved without this field.
    pub heatmap_retention: Option<chrono::TimeDelta>,
    /// Draw the live book's side profile independently of the heatmap.
    pub show_order_book: bool,
    /// Live book measure, order filter and size highlights.
    pub order_book: crate::chart::overlays::OrderBookView,
    /// Draw the supplied trades as volume bubbles.
    pub show_volume_bubbles: bool,
    /// Bubble grouping, colours and sizing, retained while bubbles are off.
    pub volume_bubbles: crate::chart::overlays::VolumeBubblesConfig,
}

/// Serializable display settings shared by the chart and its settings dialog.
///
/// ```
/// use egui_charts::widget::settings::ChartSettingsState;
///
/// let mut settings = ChartSettingsState::default();
/// settings.candle_colors.show_wick = false;
/// settings.status_line.show_logo = false;
/// settings.trading = egui_charts::widget::settings::TradingSettings::default();
/// settings.trading.instant_orders = true;
/// settings.alerts.sound_volume = 0.5;
/// settings.session_background = Some(egui_charts::widget::settings::SessionBackgroundSettings::default());
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
// The trait impls below wrap the derived ones.
#[serde(remote = "Self")]
pub struct ChartSettingsState {
    // Symbol tab
    /// Active representation used to select the Symbol controls. Applying settings keeps the chart type.
    #[serde(default)]
    pub chart_type: crate::model::ChartType,
    /// Styles and construction inputs; absent in settings saved before these controls existed.
    #[serde(default)]
    pub chart_type_params: Option<crate::model::ChartTypeParams>,
    /// The price the Line, Step line, Line with markers, Area and Baseline charts draw while
    /// their own source in `chart_type_params` is `None`, as
    /// [`ChartConfig::price_source`](crate::config::ChartConfig::price_source) holds it; absent
    /// in older saved settings, which keep the chart's.
    #[serde(default)]
    pub price_source: Option<crate::model::PriceSource>,
    /// Candle body, border and wick colors.
    pub candle_colors: CandleColorConfig,
    /// Price display precision.
    pub precision: PrecisionMode,
    /// UTC, Local, or a supported zone name.
    pub timezone: String,
    /// Show analyst estimates answered by the host feed.
    #[serde(default)]
    pub show_price_targets: bool,
    /// Colour of the analyst targets above their publication close, as
    /// [`ChartConfig::price_target_positive_color`](crate::config::ChartConfig::price_target_positive_color).
    /// `None`, as in settings saved before it existed, keeps the chart's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_target_positive_color: Option<Color32>,
    /// Colour of the analyst targets below their publication close, as
    /// [`ChartConfig::price_target_negative_color`](crate::config::ChartConfig::price_target_negative_color).
    /// `None`, as in settings saved before it existed, keeps the chart's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_target_negative_color: Option<Color32>,
    /// Ask the host for history adjusted for dividends. The host owns the data, so
    /// [`apply_to_chart`](Self::apply_to_chart) leaves it to the host, which reloads the
    /// adjusted history; [`from_chart`](Self::from_chart) leaves it off.
    #[serde(default)]
    pub adjust_for_dividends: bool,
    /// Regular hours (`false`) or regular and extended hours (`true`), as
    /// [`Chart::set_extended_hours`](crate::widget::Chart::set_extended_hours) requests them.
    /// `None` keeps the chart's choice, as do settings saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extended_hours: Option<bool>,
    /// Include overnight hours, as [`Chart::set_overnight_hours`](crate::widget::Chart::set_overnight_hours)
    /// requests them. Applied before extended hours, so regular hours turn both off.
    /// `None` keeps the chart's choice, as do settings saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overnight_hours: Option<bool>,
    /// Session highlights, independent of the hours requested from the feed. Absent in older
    /// settings, which keep the chart's current backgrounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_background: Option<SessionBackgroundSettings>,
    /// Ask the host for continuous futures history back-adjusted at each roll.
    /// [`apply_to_chart`](Self::apply_to_chart) records the choice on the chart, and
    /// [`from_chart`](Self::from_chart) and layouts keep it; the host owns the data and reloads
    /// its history when the choice changes. Off by default, also in settings saved without it.
    #[serde(default)]
    pub back_adjust_futures: bool,
    /// Ask the host to use settlement prices as the close of daily and longer futures bars;
    /// intraday history is unchanged. [`apply_to_chart`](Self::apply_to_chart) records the
    /// choice on the chart, and [`from_chart`](Self::from_chart) and layouts keep it; the host
    /// owns the data and reloads its history when the choice changes. On by default, also in
    /// settings saved without it.
    #[serde(default = "settlement_closes")]
    pub settlement_as_close: bool,

    // Order flow tab
    /// Independent heatmap, live book and trade bubble settings; older documents keep them off.
    #[serde(default)]
    pub order_flow: OrderFlowSettings,

    // Status line tab
    /// Legend content and quote controls.
    pub status_line: StatusLineOptions,

    // Scales and lines tab
    /// Price scale and reference line settings.
    pub scales_and_lines: ScalesAndLinesSettings,

    // Canvas tab - grouped settings
    /// Chart background settings.
    pub chart_basic_styles: ChartBasicStylesSettings,
    /// Grid directions and colors.
    pub grid_lines: GridLinesSettings,
    /// Colour of the separators between panes, applied to
    /// [`PaneOptions::separator_color`](crate::widget::PaneOptions::separator_color). `None`
    /// follows the theme's grid line colour, as do settings saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_separator_color: Option<Color32>,
    /// Crosshair line appearance.
    pub crosshair: CrosshairSettings,
    /// Which crosshair lines and labels are drawn. `None` keeps the chart's, as do settings
    /// saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crosshair_lines: Option<CrosshairVisibility>,
    /// Watermark appearance.
    pub watermark: WatermarkSettings,
    /// Axis appearance.
    pub scales_appearance: ScalesAppearanceSettings,
    /// Chart and pane control visibility.
    pub btns: ButtonsSettings,
    /// Space around the visible prices.
    pub margins: MarginsSettings,

    // Trading tab
    /// Host trading controls and overlays.
    pub trading: TradingSettings,

    // Alerts tab
    /// Alert marks and labels.
    pub alerts: AlertsSettings,

    // Events tab
    /// Visible event categories.
    pub events: crate::model::EventVisibility,
    /// Whether session break lines are drawn. `None` keeps the chart's choice, as do settings
    /// saved without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_session_breaks: Option<bool>,
}

impl serde::Serialize for ChartSettingsState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ChartSettingsState {
    /// A saved HLC area entry of the former format takes its colours from the saved candle
    /// colours, which [`from_chart`](Self::from_chart) writes from the chart's.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::model::HlcAreaStyle::forget_former();
        let mut state = Self::deserialize(deserializer)?;
        if let Some(params) = &mut state.chart_type_params {
            params
                .styles
                .hlc_area
                .fill_former(state.candle_colors.body_up, state.candle_colors.body_down);
        }
        Ok(state)
    }
}

/// Settlement closes, the default also for settings saved before the choice existed.
fn settlement_closes() -> bool {
    true
}

impl Default for ChartSettingsState {
    fn default() -> Self {
        let chart_basic_styles = ChartBasicStylesSettings::default();
        let grid_lines = GridLinesSettings::default();

        Self {
            chart_type: crate::model::ChartType::default(),
            chart_type_params: None,
            price_source: None,
            candle_colors: CandleColorConfig::default(),
            precision: PrecisionMode::Default,
            timezone: "UTC".to_string(),
            show_price_targets: false,
            price_target_positive_color: None,
            price_target_negative_color: None,
            adjust_for_dividends: false,
            extended_hours: None,
            overnight_hours: None,
            session_background: None,
            back_adjust_futures: false,
            settlement_as_close: true,

            order_flow: OrderFlowSettings::default(),

            status_line: StatusLineOptions::default(),

            scales_and_lines: ScalesAndLinesSettings::default(),

            chart_basic_styles: chart_basic_styles.clone(),
            grid_lines: grid_lines.clone(),
            pane_separator_color: None,
            crosshair: CrosshairSettings::default(),
            crosshair_lines: None,
            watermark: WatermarkSettings::default(),
            scales_appearance: ScalesAppearanceSettings::default(),
            btns: ButtonsSettings::default(),
            margins: MarginsSettings::default(),

            trading: TradingSettings::default(),

            alerts: AlertsSettings::default(),

            events: crate::model::EventVisibility::default(),
            show_session_breaks: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AlertsSettings;

    /// Alerts settings saved before the toast and desktop choices keep their lifetime, close
    /// their toasts and stay off the desktop; only a save without a lifetime takes 20 seconds.
    #[test]
    fn alerts_saved_before_the_toast_choices_keep_closing_toasts_in_the_app() {
        for (saved, seconds) in [
            (
                r#"{"play_sound":false,"sound":"","sound_volume":1.0,"show_alerts":true,"show_alert_labels":true,"alert_color":[255,152,0,255],"show_active_only":true,"notification_seconds":5}"#,
                5,
            ),
            (r#"{"play_sound":true,"show_active_only":false}"#, 20),
        ] {
            let alerts: AlertsSettings = serde_json::from_str(saved).unwrap();
            assert_eq!(
                (
                    alerts.notification_seconds,
                    alerts.close_toasts,
                    alerts.desktop_notifications
                ),
                (seconds, true, false),
                "{saved}"
            );
        }
    }
}
