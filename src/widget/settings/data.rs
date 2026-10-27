//! Data structures for settings state - Canvas style

use super::types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
use crate::chart::PnlUnit;
use crate::model::LineStyle;
use egui::Color32;

// ============================================================================
// Settings State
// ============================================================================

/// Candle color configuration (body, border, wick for up/down candles) and which parts are drawn.
/// Saved settings without a field take its default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CandleColorConfig {
    /// Colour each candle by its close against the previous close instead of its open.
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
            show_border: true,
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
pub struct StatusLineOptions {
    /// Whether the legend shows the host's instrument logo before the title.
    pub show_logo: bool,
    /// Whether the legend shows the instrument title.
    pub show_symbol: bool,
    /// Display the description instead of the ticker.
    pub title_as_description: bool,
    /// Display current bid and ask controls.
    pub show_quote_boxes: bool,
    /// Display study names.
    pub show_indicator_titles: bool,
    /// Display host-supplied study inputs.
    pub show_indicator_inputs: bool,
    /// Display historical study output values.
    pub show_indicator_values: bool,
    /// Whether the legend shows open, high, low and close.
    pub show_ohlc: bool,
    /// Whether the legend shows the absolute price change.
    pub show_change: bool,
    /// Whether the legend shows the percentage price change.
    pub show_change_percent: bool,
    /// Whether the legend shows volume.
    pub show_volume: bool,
    /// Whether the legend shows the change of the last price from the previous session's close.
    pub show_last_day_change: bool,
    /// Whether study rows in the legend sit on a backdrop of the chart background.
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
            show_quote_boxes: true,
            show_indicator_titles: true,
            show_indicator_inputs: true,
            show_indicator_values: true,
            show_ohlc: true,
            show_change: true,
            show_change_percent: true,
            show_volume: true,
            show_last_day_change: false,
            show_indicator_background: true,
            indicator_background_opacity: 100,
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
pub struct ScalesAppearanceSettings {
    /// Text color.
    pub text_color: Color32,
    /// Font size in logical pixels.
    pub font_size: u8,
    /// Axis line color.
    pub lines_color: Color32,
}

impl Default for ScalesAppearanceSettings {
    fn default() -> Self {
        Self {
            text_color: crate::theme::Theme::default().ui.text_secondary,
            font_size: 12,
            lines_color: Color32::TRANSPARENT,
        }
    }
}

/// Canvas tab: Buttons visibility settings
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
            navigation: ButtonVisibility::Hidden,
            pane: ButtonVisibility::VisibleOnMouseOver,
        }
    }
}

/// Canvas tab: Margins settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MarginsSettings {
    /// Empty space above prices, as a percentage of pane height.
    pub top_percent: f32,
    /// Empty space below prices, as a percentage of pane height.
    pub bottom_percent: f32,
    /// Empty space after the latest bar, in bars.
    pub right_bars: u32,
}

impl Default for MarginsSettings {
    fn default() -> Self {
        Self {
            top_percent: 10.0,
            bottom_percent: 8.0,
            right_bars: 10,
        }
    }
}

/// Scales and lines tab settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScalesAndLinesSettings {
    /// Display host-supplied currency metadata.
    pub show_currency: bool,
    /// Display host-supplied price unit metadata.
    pub show_unit: bool,
    /// Display Auto, Log and Percent navigation buttons.
    pub show_scale_mode_buttons: bool,
    /// Display the price-axis host action button.
    pub show_plus_button: bool,
    /// Let value tags overlap instead of moving them apart.
    pub allow_label_overlap: bool,
    /// Lock scale units per bar to `price_to_bar_ratio`.
    pub lock_price_to_bar_ratio: bool,
    /// Positive scale units per bar while locked; log scales use natural logarithms of prices.
    pub price_to_bar_ratio: f64,
    /// Last price tag and line colour, or the candle direction colour when unset.
    pub symbol_price_color: Option<Color32>,
    /// Display the last values of studies on their price axes.
    pub show_study_values: bool,
    /// Study value tag colour, or each plot's colour when unset.
    pub study_value_color: Option<Color32>,
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
    /// Whether to show countdown.
    pub show_countdown: bool,
    /// Whether to show bid ask.
    pub show_bid_ask: bool,
    /// Whether to show high low.
    pub show_high_low: bool,
    /// Whether to show prev close.
    pub show_prev_close: bool,
}

impl Default for ScalesAndLinesSettings {
    fn default() -> Self {
        Self {
            show_currency: true,
            show_unit: true,
            show_scale_mode_buttons: true,
            show_plus_button: true,
            allow_label_overlap: false,
            lock_price_to_bar_ratio: false,
            price_to_bar_ratio: 1.0,
            symbol_price_color: None,
            show_study_values: true,
            study_value_color: None,
            show_extended_session_price: true,
            extended_session_price_color: Color32::from_rgb(251, 142, 1),
            auto_scale: true,
            log_scale: false,
            percentage_scale: false,
            invert_scale: false,
            price_scale_left: false,
            show_last_price_label: true,
            show_prev_close_label: false,
            show_price_line: true,
            show_countdown: true,
            show_bid_ask: false,
            show_high_low: false,
            show_prev_close: false,
        }
    }
}

/// Trading tab settings: the Buy and Sell buttons' behaviour, notifications and the layers
/// `ui::trading::Trading::show` draws through `Trading::display` with the `ui` feature.
/// The Buy and Sell buttons themselves are [`StatusLineOptions::show_quote_boxes`].
/// Saved settings without a field take its default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TradingSettings {
    /// Buy and Sell place a market order of the order quantity at once instead of opening the
    /// order ticket.
    pub instant_orders: bool,
    /// Show current Bid and Ask tags supplied by the quote source.
    pub bid_ask_labels: bool,
    /// Ask the host to play [`execution_sound`](Self::execution_sound) for every fill, through
    /// `ui::trading::Trading::take_execution_sound` with the `ui` feature.
    pub play_execution_sound: bool,
    /// The name of the sound the host plays for fills, one of the
    /// `SettingsDialog::execution_sounds` with the `ui` feature; empty for the host's first.
    pub execution_sound: String,
    /// Which trading events raise a notification.
    pub notifications: TradingNotifications,
    /// Draw the open positions.
    pub show_positions: bool,
    /// Give each position's profit or loss on its label.
    pub position_pnl: bool,
    /// The unit of the positions' profit or loss.
    pub position_pnl_unit: PnlUnit,
    /// Show a position's reverse button while its label is hovered.
    pub reverse_button: bool,
    /// Draw the working orders and their brackets.
    pub show_orders: bool,
    /// Give on each take profit and stop loss the profit or loss of reaching it.
    pub bracket_pnl: bool,
    /// The unit of the brackets' profit or loss.
    pub bracket_pnl_unit: PnlUnit,
    /// Draw the fills on their bars.
    pub show_executions: bool,
    /// Write the quantity and average price beside each fill's mark.
    pub show_execution_labels: bool,
}

impl Default for TradingSettings {
    fn default() -> Self {
        Self {
            instant_orders: false,
            bid_ask_labels: false,
            play_execution_sound: false,
            execution_sound: String::new(),
            notifications: TradingNotifications::All,
            show_positions: true,
            position_pnl: true,
            position_pnl_unit: PnlUnit::Money,
            reverse_button: true,
            show_orders: true,
            bracket_pnl: true,
            bracket_pnl_unit: PnlUnit::Money,
            show_executions: true,
            show_execution_labels: false,
        }
    }
}

/// Which trading events raise a notification: the levels of the toasts they use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradingNotifications {
    /// Fills, partial fills, rejections and refused orders.
    #[default]
    All,
    /// Rejections and refused orders only.
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

/// Alerts tab settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AlertsSettings {
    /// Whether to show alerts.
    pub show_alerts: bool,
    /// Whether to show alert labels.
    pub show_alert_labels: bool,
    /// Alert color.
    pub alert_color: Color32,
    /// Draw only active alerts; otherwise triggered, disabled and expired alerts are drawn dimmed.
    pub show_active_only: bool,
    /// Alert sound volume in percent; zero is mute. The host plays requested sounds.
    pub sound_volume: u8,
    /// Seconds an alert notification stays on screen.
    pub notification_seconds: u32,
}

impl Default for AlertsSettings {
    fn default() -> Self {
        Self {
            show_alerts: true,
            show_alert_labels: true,
            alert_color: crate::studies::palette::ORANGE, // Orange
            show_active_only: true,
            sound_volume: 100,
            notification_seconds: 5,
        }
    }
}

/// Serializable display settings shared by the chart and its settings dialog.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChartSettingsState {
    // Symbol tab
    /// Candle body, border and wick colors.
    pub candle_colors: CandleColorConfig,
    /// Price display precision.
    pub precision: PrecisionMode,
    /// UTC, Local, or a supported zone name.
    pub timezone: String,
    /// Show analyst estimates answered by the host feed.
    #[serde(default)]
    pub show_price_targets: bool,
    /// Ask the host for history adjusted for dividends. The host owns the data, so
    /// [`apply_to_chart`](Self::apply_to_chart) leaves it to the host, which reloads the
    /// adjusted history; [`from_chart`](Self::from_chart) leaves it off.
    #[serde(default)]
    pub adjust_for_dividends: bool,

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
    /// Crosshair line appearance.
    pub crosshair: CrosshairSettings,
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
}

impl Default for ChartSettingsState {
    fn default() -> Self {
        let chart_basic_styles = ChartBasicStylesSettings::default();
        let grid_lines = GridLinesSettings::default();

        Self {
            candle_colors: CandleColorConfig::default(),
            precision: PrecisionMode::Default,
            timezone: "UTC".to_string(),
            show_price_targets: false,
            adjust_for_dividends: false,

            status_line: StatusLineOptions::default(),

            scales_and_lines: ScalesAndLinesSettings::default(),

            chart_basic_styles: chart_basic_styles.clone(),
            grid_lines: grid_lines.clone(),
            crosshair: CrosshairSettings::default(),
            watermark: WatermarkSettings::default(),
            scales_appearance: ScalesAppearanceSettings::default(),
            btns: ButtonsSettings::default(),
            margins: MarginsSettings::default(),

            trading: TradingSettings::default(),

            alerts: AlertsSettings::default(),

            events: crate::model::EventVisibility::default(),
        }
    }
}
