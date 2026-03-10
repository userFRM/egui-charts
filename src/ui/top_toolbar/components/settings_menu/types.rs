//! Core types and enums for chart settings.
//!
//! Defines scale modes, background types, grid line modes, button visibility,
//! watermark display, candle colors, status line options, and precision modes.

use crate::theme::Theme;
use egui::Color32;
use std::fmt;
use std::str::FromStr;

// ============================================================================
// Core Types (used throughout codebase)
// ============================================================================

/// Price axis scale mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScaleMode {
    /// Automatic scale based on visible data
    #[default]
    Auto,
    /// Display as percentage change from reference price
    Percentage,
    /// Logarithmic price scale
    Logarithmic,
}

impl fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaleMode::Auto => write!(f, "Auto"),
            ScaleMode::Percentage => write!(f, "Percentage"),
            ScaleMode::Logarithmic => write!(f, "Logarithmic"),
        }
    }
}

impl FromStr for ScaleMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ScaleMode::Auto),
            "percentage" | "percent" | "%" => Ok(ScaleMode::Percentage),
            "logarithmic" | "log" => Ok(ScaleMode::Logarithmic),
            _ => Err(format!("Invalid scale mode: {s}")),
        }
    }
}

// ChartType is re-exported from chart_type_selector - use that as the single source of truth

// ============================================================================
// Settings Types
// ============================================================================

/// Settings dialog tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// Symbol-specific settings (candle colors, precision, timezone)
    Symbol,
    /// Status line visibility options (OHLC, volume, change)
    StatusLine,
    /// Scale modes and price line settings
    ScalesAndLines,
    /// Canvas appearance (background, grid, crosshair, watermark, margins)
    Canvas,
    /// Trading display options (positions, orders, executions)
    Trading,
    /// Alert display settings
    Alerts,
    /// Event marker settings (dividends, splits, earnings)
    Events,
}

impl SettingsTab {
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Symbol,
            SettingsTab::StatusLine,
            SettingsTab::ScalesAndLines,
            SettingsTab::Canvas,
            SettingsTab::Trading,
            SettingsTab::Alerts,
            SettingsTab::Events,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            SettingsTab::Symbol => "Symbol",
            SettingsTab::StatusLine => "Status line",
            SettingsTab::ScalesAndLines => "Scales and lines",
            SettingsTab::Canvas => "Canvas",
            SettingsTab::Trading => "Trading",
            SettingsTab::Alerts => "Alerts",
            SettingsTab::Events => "Events",
        }
    }
}

/// Background type for chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundType {
    #[default]
    Solid,
    VerticalGradient,
    HorizontalGradient,
}

impl BackgroundType {
    pub fn all() -> &'static [BackgroundType] {
        &[
            BackgroundType::Solid,
            BackgroundType::VerticalGradient,
            BackgroundType::HorizontalGradient,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            BackgroundType::Solid => "Solid",
            BackgroundType::VerticalGradient => "Gradient vertical",
            BackgroundType::HorizontalGradient => "Gradient horizontal",
        }
    }
}

/// Grid lines display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridLinesMode {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

impl GridLinesMode {
    pub fn all() -> &'static [GridLinesMode] {
        &[
            GridLinesMode::None,
            GridLinesMode::Horizontal,
            GridLinesMode::Vertical,
            GridLinesMode::Both,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            GridLinesMode::None => "None",
            GridLinesMode::Horizontal => "Horizontal",
            GridLinesMode::Vertical => "Vertical",
            GridLinesMode::Both => "Both",
        }
    }
}

/// Button visibility mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVisibility {
    AlwaysVisible,
    #[default]
    VisibleOnMouseOver,
    Hidden,
}

impl ButtonVisibility {
    pub fn all() -> &'static [ButtonVisibility] {
        &[
            ButtonVisibility::AlwaysVisible,
            ButtonVisibility::VisibleOnMouseOver,
            ButtonVisibility::Hidden,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ButtonVisibility::AlwaysVisible => "Always visible",
            ButtonVisibility::VisibleOnMouseOver => "Visible on mouse over",
            ButtonVisibility::Hidden => "Hidden",
        }
    }
}

/// Watermark display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatermarkMode {
    #[default]
    ReplayMode,
    Symbol,
    SymbolAndDescription,
    Custom,
}

impl WatermarkMode {
    pub fn all() -> &'static [WatermarkMode] {
        &[
            WatermarkMode::ReplayMode,
            WatermarkMode::Symbol,
            WatermarkMode::SymbolAndDescription,
            WatermarkMode::Custom,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            WatermarkMode::ReplayMode => "Replay mode",
            WatermarkMode::Symbol => "Symbol",
            WatermarkMode::SymbolAndDescription => "Symbol and desc",
            WatermarkMode::Custom => "Custom",
        }
    }
}

/// Candle color configuration
#[derive(Debug, Clone, PartialEq)]
pub struct CandleColorConfig {
    pub body_up: Color32,
    pub body_down: Color32,
    pub border_up: Color32,
    pub border_down: Color32,
    pub wick_up: Color32,
    pub wick_down: Color32,
}

impl CandleColorConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let chart = &theme.semantic.chart;
        Self {
            body_up: chart.candle_up,
            body_down: chart.candle_down,
            border_up: chart.candle_up_border,
            border_down: chart.candle_down_border,
            wick_up: chart.candle_up_wick,
            wick_down: chart.candle_down_wick,
        }
    }
}

impl Default for CandleColorConfig {
    fn default() -> Self {
        // Default uses standard theme
        Self::from_theme(&Theme::dark())
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

/// Precision mode for price display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrecisionMode {
    #[default]
    Default,
    Decimals2,
    Decimals3,
    Decimals4,
    Decimals5,
    Decimals6,
    Decimals8,
}

impl PrecisionMode {
    pub fn all() -> &'static [PrecisionMode] {
        &[
            PrecisionMode::Default,
            PrecisionMode::Decimals2,
            PrecisionMode::Decimals3,
            PrecisionMode::Decimals4,
            PrecisionMode::Decimals5,
            PrecisionMode::Decimals6,
            PrecisionMode::Decimals8,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PrecisionMode::Default => "Default",
            PrecisionMode::Decimals2 => "2 decimals",
            PrecisionMode::Decimals3 => "3 decimals",
            PrecisionMode::Decimals4 => "4 decimals",
            PrecisionMode::Decimals5 => "5 decimals",
            PrecisionMode::Decimals6 => "6 decimals",
            PrecisionMode::Decimals8 => "8 decimals",
        }
    }

    pub fn decimals(&self) -> Option<usize> {
        match self {
            PrecisionMode::Default => None,
            PrecisionMode::Decimals2 => Some(2),
            PrecisionMode::Decimals3 => Some(3),
            PrecisionMode::Decimals4 => Some(4),
            PrecisionMode::Decimals5 => Some(5),
            PrecisionMode::Decimals6 => Some(6),
            PrecisionMode::Decimals8 => Some(8),
        }
    }
}
