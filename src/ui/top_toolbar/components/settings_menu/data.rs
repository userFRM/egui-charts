//! Data structures for settings state - Canvas style

use super::types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

// ============================================================================
// Settings State
// ============================================================================

/// Candle color configuration (body, border, wick for up/down candles)
#[derive(Debug, Clone, PartialEq)]
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
            body_up: DESIGN_TOKENS.semantic.extended.bullish,
            body_down: DESIGN_TOKENS.semantic.extended.bearish,
            border_up: DESIGN_TOKENS.semantic.extended.bullish,
            border_down: DESIGN_TOKENS.semantic.extended.bearish,
            wick_up: DESIGN_TOKENS.semantic.extended.bullish,
            wick_down: DESIGN_TOKENS.semantic.extended.bearish,
        }
    }
}

/// Status line display options
#[derive(Debug, Clone, PartialEq)]
pub struct StatusLineOptions {
    pub show_symbol: bool,
    pub show_ohlc: bool,
    pub show_change: bool,
    pub show_change_percent: bool,
    pub show_volume: bool,
}

impl Default for StatusLineOptions {
    fn default() -> Self {
        Self {
            show_symbol: true,
            show_ohlc: true,
            show_change: true,
            show_change_percent: true,
            show_volume: true,
        }
    }
}

/// Canvas tab: Chart basic styles settings
#[derive(Debug, Clone, PartialEq)]
pub struct ChartBasicStylesSettings {
    pub background_type: BackgroundType,
    pub background_color: Color32,
    pub background_gradient_top: Color32,
    pub background_gradient_bottom: Color32,
}

impl Default for ChartBasicStylesSettings {
    fn default() -> Self {
        Self {
            background_type: BackgroundType::Solid,
            background_color: DESIGN_TOKENS.semantic.extended.chart_bg,
            background_gradient_top: DESIGN_TOKENS.semantic.extended.chart_bg,
            background_gradient_bottom: DESIGN_TOKENS.semantic.extended.chart_axis_bg,
        }
    }
}

/// Canvas tab: Grid lines settings
#[derive(Debug, Clone, PartialEq)]
pub struct GridLinesSettings {
    pub mode: GridLinesMode,
    pub horizontal_color: Color32,
    pub vertical_color: Color32,
}

impl Default for GridLinesSettings {
    fn default() -> Self {
        Self {
            mode: GridLinesMode::Both,
            horizontal_color: DESIGN_TOKENS.semantic.chart.grid_line,
            vertical_color: DESIGN_TOKENS.semantic.chart.grid_line,
        }
    }
}

/// Canvas tab: Crosshair settings
#[derive(Debug, Clone, PartialEq)]
pub struct CrosshairSettings {
    pub color: Color32,
    pub line_width: f32,
    pub line_style: LineStyle,
}

impl Default for CrosshairSettings {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.extended.chart_text_secondary,
            line_width: 1.0,
            line_style: LineStyle::Dashed,
        }
    }
}

/// Line style for crosshair and other elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    Solid,
    #[default]
    Dashed,
    Dotted,
}

impl LineStyle {
    pub fn all() -> &'static [LineStyle] {
        &[LineStyle::Solid, LineStyle::Dashed, LineStyle::Dotted]
    }

    pub fn name(&self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Dashed => "Dashed",
            LineStyle::Dotted => "Dotted",
        }
    }
}

/// Canvas tab: Watermark settings
#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkSettings {
    pub mode: WatermarkMode,
    pub color: Color32,
}

impl Default for WatermarkSettings {
    fn default() -> Self {
        Self {
            mode: WatermarkMode::ReplayMode,
            color: DESIGN_TOKENS
                .semantic
                .extended
                .chart_text_muted
                .gamma_multiply(0.2), // ~20% opacity
        }
    }
}

/// Canvas tab: Scales appearance settings
#[derive(Debug, Clone, PartialEq)]
pub struct ScalesAppearanceSettings {
    pub text_color: Color32,
    pub font_size: u8,
    pub lines_color: Color32,
}

impl Default for ScalesAppearanceSettings {
    fn default() -> Self {
        Self {
            text_color: DESIGN_TOKENS.semantic.extended.text_secondary,
            font_size: 12,
            lines_color: Color32::TRANSPARENT,
        }
    }
}

/// Canvas tab: Buttons visibility settings
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonsSettings {
    pub navigation: ButtonVisibility,
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct ScalesAndLinesSettings {
    pub auto_scale: bool,
    pub log_scale: bool,
    pub percentage_scale: bool,
    pub invert_scale: bool,
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
            show_price_line: true,
            show_countdown: false,
            show_bid_ask: false,
            show_high_low: false,
            show_prev_close: false,
        }
    }
}

/// Trading tab settings
#[derive(Debug, Clone, PartialEq)]
pub struct TradingSettings {
    pub show_poss: bool,
    pub show_orders: bool,
    pub show_executions: bool,
    pub show_buy_sell_btns: bool,
}

impl Default for TradingSettings {
    fn default() -> Self {
        Self {
            show_poss: true,
            show_orders: true,
            show_executions: true,
            show_buy_sell_btns: false,
        }
    }
}

/// Alerts tab settings
#[derive(Debug, Clone, PartialEq)]
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
            alert_color: DESIGN_TOKENS.semantic.indicators.ma, // Orange
        }
    }
}

/// Events tab settings
#[derive(Debug, Clone, PartialEq)]
pub struct EventsSettings {
    pub show_dividends: bool,
    pub show_splits: bool,
    pub show_earnings: bool,
    pub show_economic_events: bool,
}

impl Default for EventsSettings {
    fn default() -> Self {
        Self {
            show_dividends: true,
            show_splits: true,
            show_earnings: false,
            show_economic_events: false,
        }
    }
}

/// Comprehensive chart settings state
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSettingsState {
    // Symbol tab
    pub candle_colors: CandleColorConfig,
    pub color_based_on_prev_close: bool,
    pub precision: PrecisionMode,
    pub timezone: String,

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
            color_based_on_prev_close: false,
            precision: PrecisionMode::Default,
            timezone: "UTC".to_string(),

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
