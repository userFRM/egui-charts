//! Data structures for settings state - Canvas style

use super::types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
use crate::chart::PnlUnit;
use crate::model::LineStyle;
use egui::Color32;

// ============================================================================
// Settings State
// ============================================================================

/// Candle color configuration (body, border, wick for up/down candles)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CandleColorConfig {
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

/// Status line display options
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusLineOptions {
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
}

impl Default for StatusLineOptions {
    fn default() -> Self {
        Self {
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
pub struct ScalesAndLinesSettings {
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
            bid_ask_labels: true,
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
pub struct AlertsSettings {
    /// Whether to show alerts.
    pub show_alerts: bool,
    /// Whether to show alert labels.
    pub show_alert_labels: bool,
    /// Alert color.
    pub alert_color: Color32,
}

impl Default for AlertsSettings {
    fn default() -> Self {
        Self {
            show_alerts: true,
            show_alert_labels: true,
            alert_color: crate::studies::palette::ORANGE, // Orange
        }
    }
}

/// Serializable display settings shared by the chart and its settings dialog.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChartSettingsState {
    // Symbol tab
    /// Active representation used to select the Symbol controls. Applying settings keeps the chart type.
    #[serde(default)]
    pub chart_type: crate::model::ChartType,
    /// Styles and construction inputs; absent in settings saved before these controls existed.
    #[serde(default)]
    pub chart_type_params: Option<crate::model::ChartTypeParams>,
    /// Source for line and area representations; absent in older saved settings.
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
            chart_type: crate::model::ChartType::default(),
            chart_type_params: None,
            price_source: None,
            candle_colors: CandleColorConfig::default(),
            precision: PrecisionMode::Default,
            timezone: "UTC".to_string(),
            show_price_targets: false,

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
