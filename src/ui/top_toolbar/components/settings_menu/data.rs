//! Data structures for settings state - Canvas style

use super::types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
use crate::chart::PnlUnit;
use egui::Color32;

// ============================================================================
// Settings State
// ============================================================================

/// Candle color configuration (body, border, wick for up/down candles)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CandleColorConfig {
    pub body_up: Color32,
    pub body_down: Color32,
    pub border_up: Color32,
    pub border_down: Color32,
    pub wick_up: Color32,
    pub wick_down: Color32,
}

impl Default for CandleColorConfig {
    fn default() -> Self {
        Self {
            body_up: crate::theme::Theme::default().chart.candle_up,
            body_down: crate::theme::Theme::default().chart.candle_down,
            border_up: crate::theme::Theme::default().chart.candle_up,
            border_down: crate::theme::Theme::default().chart.candle_down,
            wick_up: crate::theme::Theme::default().chart.candle_up,
            wick_down: crate::theme::Theme::default().chart.candle_down,
        }
    }
}

/// Status line display options
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusLineOptions {
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
    pub show_ohlc: bool,
    pub show_change: bool,
    pub show_change_percent: bool,
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
    pub background_type: BackgroundType,
    /// `Color32::PLACEHOLDER` follows the active chart background until a colour is chosen.
    pub background_color: Color32,
    pub background_gradient_top: Color32,
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
    pub mode: GridLinesMode,
    pub horizontal_color: Color32,
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
    pub color: Color32,
    pub line_width: f32,
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

/// Line style for crosshair and other elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LineStyle {
    Solid,
    #[default]
    Dashed,
    Dotted,
    LargeDashed,
    SparseDotted,
}

impl LineStyle {
    pub fn all() -> &'static [LineStyle] {
        &[
            LineStyle::Solid,
            LineStyle::Dashed,
            LineStyle::Dotted,
            LineStyle::LargeDashed,
            LineStyle::SparseDotted,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Dashed => "Dashed",
            LineStyle::Dotted => "Dotted",
            LineStyle::LargeDashed => "Large dashed",
            LineStyle::SparseDotted => "Sparse dotted",
        }
    }
}

/// Canvas tab: Watermark settings
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatermarkSettings {
    pub mode: WatermarkMode,
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
    pub text_color: Color32,
    pub font_size: u8,
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
    pub navigation: ButtonVisibility,
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
    pub top_percent: f32,
    pub bottom_percent: f32,
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
    pub auto_scale: bool,
    pub log_scale: bool,
    pub percentage_scale: bool,
    pub invert_scale: bool,
    /// Show the price scale on the left instead of the right.
    pub price_scale_left: bool,
    /// Display the current close tag independently of its line.
    pub show_last_price_label: bool,
    /// Display the previous session close tag independently of its line.
    pub show_prev_close_label: bool,
    pub show_price_line: bool,
    pub show_countdown: bool,
    pub show_bid_ask: bool,
    pub show_high_low: bool,
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
/// [`Trading::show`](crate::ui::trading::Trading::show) draws, through
/// [`Trading::display`](crate::ui::trading::Trading::display). The Buy and Sell buttons
/// themselves are [`StatusLineOptions::show_quote_boxes`]. Saved settings without a field take
/// its default.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TradingSettings {
    /// Buy and Sell place a market order of the order quantity at once instead of opening the
    /// order ticket.
    pub instant_orders: bool,
    /// Show current Bid and Ask tags supplied by the quote source.
    pub bid_ask_labels: bool,
    /// Ask the host to play [`execution_sound`](Self::execution_sound) for every fill, through
    /// [`Trading::take_execution_sound`](crate::ui::trading::Trading::take_execution_sound).
    pub play_execution_sound: bool,
    /// The name of the sound the host plays for fills, one of the
    /// [`SettingsDialog::execution_sounds`](super::SettingsDialog::execution_sounds); empty for
    /// the host's first.
    pub execution_sound: String,
    /// Which trading events raise a notification.
    pub notifications: TradingNotifications,
    /// Draw the open positions.
    pub show_poss: bool,
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
            show_poss: true,
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
    pub show_alerts: bool,
    pub show_alert_labels: bool,
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

/// Event category visibility shared with the chart.
pub type EventsSettings = crate::model::EventVisibility;

/// Comprehensive chart settings state
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChartSettingsState {
    // Symbol tab
    pub candle_colors: CandleColorConfig,
    pub precision: PrecisionMode,
    pub timezone: String,
    /// Show analyst estimates answered by the host feed.
    #[serde(default)]
    pub show_price_targets: bool,

    // Status line tab
    pub status_line: StatusLineOptions,

    // Scales and lines tab
    pub scales_and_lines: ScalesAndLinesSettings,

    // Canvas tab - grouped settings
    pub chart_basic_styles: ChartBasicStylesSettings,
    pub grid_lines: GridLinesSettings,
    pub crosshair: CrosshairSettings,
    pub watermark: WatermarkSettings,
    pub scales_appearance: ScalesAppearanceSettings,
    pub btns: ButtonsSettings,
    pub margins: MarginsSettings,

    // Trading tab
    pub trading: TradingSettings,

    // Alerts tab
    pub alerts: AlertsSettings,

    // Events tab
    pub events: EventsSettings,

    // Legacy compatibility fields (mapped to new structure)
    pub background_color: Color32,
    pub grid_color: Color32,
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

            events: EventsSettings::default(),

            // Legacy compatibility
            background_color: chart_basic_styles.background_color,
            grid_color: grid_lines.horizontal_color,
        }
    }
}

impl ChartSettingsState {
    /// Sync legacy fields with new structure
    pub fn sync_legacy_fields(&mut self) {
        self.background_color = self.chart_basic_styles.background_color;
        self.grid_color = self.grid_lines.horizontal_color;
    }
}
