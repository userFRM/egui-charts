//! Chart, drawing, and layout enums.
//!
//! Wire-format enum definitions for chart display modes, drawing tool
//! styles, timezones, resolutions, and UI layout configurations. These
//! types mirror the TradingView Charting Library wire protocol and are
//! used for serialization. Domain-level code should prefer the
//! higher-level types in [`crate::model`] where available.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Line style for drawing tools (trend lines, channels, etc.)
///
/// Discriminant values:
/// Solid=0, Dotted=1, Dashed=2, LargeDashed=3, SparseDotted=4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum LineStyle {
    /// Solid continuous line
    #[default]
    Solid = 0,
    /// Dotted line with small gaps
    Dotted = 1,
    /// Dashed line with regular gaps
    Dashed = 2,
    /// Large dashed line with wider segments
    LargeDashed = 3,
    /// Sparse dotted line with wider gaps
    SparseDotted = 4,
}

impl LineStyle {
    pub fn all() -> &'static [Self] {
        &[
            Self::Solid,
            Self::Dotted,
            Self::Dashed,
            Self::LargeDashed,
            Self::SparseDotted,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::LargeDashed => "large_dashed",
            Self::SparseDotted => "sparse_dotted",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dotted => "Dotted",
            Self::Dashed => "Dashed",
            Self::LargeDashed => "Large Dashed",
            Self::SparseDotted => "Sparse Dotted",
        }
    }

    /// Pattern definition for rendering (on/off segments)
    /// Returns vec of segment lengths (positive = draw, negative = gap)
    pub fn pattern(&self) -> Vec<f32> {
        match self {
            Self::Solid => vec![f32::MAX],
            Self::Dotted => vec![2.0, 4.0],
            Self::Dashed => vec![10.0, 5.0],
            Self::LargeDashed => vec![16.0, 6.0],
            Self::SparseDotted => vec![2.0, 8.0],
        }
    }
}

impl fmt::Display for LineStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Chart display mode (logarithmic vs linear)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChartMode {
    /// Linear/Arithmetic scale
    #[default]
    Linear,
    /// Logarithmic scale
    Logarithmic,
    /// Percentage scale (percentage change from start)
    Percentage,
    /// Indexed to 100 (normalized scale)
    IndexedTo100,
}

impl ChartMode {
    pub fn all() -> &'static [Self] {
        &[
            Self::Linear,
            Self::Logarithmic,
            Self::Percentage,
            Self::IndexedTo100,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Logarithmic => "log",
            Self::Percentage => "percentage",
            Self::IndexedTo100 => "indexed_to_100",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Logarithmic => "Logarithmic",
            Self::Percentage => "Percentage",
            Self::IndexedTo100 => "Indexed to 100",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Linear => "Lin",
            Self::Logarithmic => "Log",
            Self::Percentage => "%",
            Self::IndexedTo100 => "Idx",
        }
    }
}

impl fmt::Display for ChartMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Timezone options for chart display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Timezone {
    /// Use browser local timezone
    #[default]
    Local,
    /// Use exchange timezone
    Exchange,
    /// UTC/GMT timezone
    UTC,
    /// America/New_York (EST/EDT)
    AmericaNewYork,
    /// America/Chicago (CST/CDT)
    AmericaChicago,
    /// America/Denver (MST/MDT)
    AmericaDenver,
    /// America/Los_Angeles (PST/PDT)
    AmericaLosAngeles,
    /// Europe/London (GMT/BST)
    EuropeLondon,
    /// Europe/Paris (CET/CEST)
    EuropeParis,
    /// Europe/Berlin (CET/CEST)
    EuropeBerlin,
    /// Asia/Tokyo (JST)
    AsiaTokyo,
    /// Asia/Hong_Kong (HKT)
    AsiaHongKong,
    /// Asia/Singapore (SGT)
    AsiaSingapore,
    /// Australia/Sydney (AEST/AEDT)
    AustraliaSydney,
    /// Pacific/Auckland (NZST/NZDT)
    PacificAuckland,
}

impl Timezone {
    pub fn all() -> &'static [Self] {
        &[
            Self::Local,
            Self::Exchange,
            Self::UTC,
            Self::AmericaNewYork,
            Self::AmericaChicago,
            Self::AmericaDenver,
            Self::AmericaLosAngeles,
            Self::EuropeLondon,
            Self::EuropeParis,
            Self::EuropeBerlin,
            Self::AsiaTokyo,
            Self::AsiaHongKong,
            Self::AsiaSingapore,
            Self::AustraliaSydney,
            Self::PacificAuckland,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Exchange => "exchange",
            Self::UTC => "UTC",
            Self::AmericaNewYork => "America/New_York",
            Self::AmericaChicago => "America/Chicago",
            Self::AmericaDenver => "America/Denver",
            Self::AmericaLosAngeles => "America/Los_Angeles",
            Self::EuropeLondon => "Europe/London",
            Self::EuropeParis => "Europe/Paris",
            Self::EuropeBerlin => "Europe/Berlin",
            Self::AsiaTokyo => "Asia/Tokyo",
            Self::AsiaHongKong => "Asia/Hong_Kong",
            Self::AsiaSingapore => "Asia/Singapore",
            Self::AustraliaSydney => "Australia/Sydney",
            Self::PacificAuckland => "Pacific/Auckland",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Exchange => "Exchange",
            Self::UTC => "UTC",
            Self::AmericaNewYork => "New York (EST/EDT)",
            Self::AmericaChicago => "Chicago (CST/CDT)",
            Self::AmericaDenver => "Denver (MST/MDT)",
            Self::AmericaLosAngeles => "Los Angeles (PST/PDT)",
            Self::EuropeLondon => "London (GMT/BST)",
            Self::EuropeParis => "Paris (CET/CEST)",
            Self::EuropeBerlin => "Berlin (CET/CEST)",
            Self::AsiaTokyo => "Tokyo (JST)",
            Self::AsiaHongKong => "Hong Kong (HKT)",
            Self::AsiaSingapore => "Singapore (SGT)",
            Self::AustraliaSydney => "Sydney (AEST/AEDT)",
            Self::PacificAuckland => "Auckland (NZST/NZDT)",
        }
    }

    /// Offset from UTC in seconds (for reference, actual offset varies by DST)
    pub fn standard_offset_secs(&self) -> i32 {
        match self {
            Self::Local => 0,    // Varies by system
            Self::Exchange => 0, // Varies by exchange
            Self::UTC => 0,
            Self::AmericaNewYork => -5 * 3600,    // -5 hours
            Self::AmericaChicago => -6 * 3600,    // -6 hours
            Self::AmericaDenver => -7 * 3600,     // -7 hours
            Self::AmericaLosAngeles => -8 * 3600, // -8 hours
            Self::EuropeLondon => 0,              // GMT
            Self::EuropeParis => 3600,            // +1 hour
            Self::EuropeBerlin => 3600,           // +1 hour
            Self::AsiaTokyo => 9 * 3600,          // +9 hours
            Self::AsiaHongKong => 8 * 3600,       // +8 hours
            Self::AsiaSingapore => 8 * 3600,      // +8 hours
            Self::AustraliaSydney => 10 * 3600,   // +10 hours
            Self::PacificAuckland => 12 * 3600,   // +12 hours
        }
    }
}

impl fmt::Display for Timezone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Price scale mode (determines how price is displayed)
///
/// Discriminant values:
/// Normal=0, Log=1, Percentage=2, IndexedTo100=3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum PriceScaleMode {
    /// Normal price scale
    #[default]
    Normal = 0,
    /// Logarithmic scale
    Logarithmic = 1,
    /// Percentage scale
    Percentage = 2,
    /// Indexed to 100 scale
    IndexedTo100 = 3,
}

impl PriceScaleMode {
    pub fn all() -> &'static [Self] {
        &[
            Self::Normal,
            Self::Logarithmic,
            Self::Percentage,
            Self::IndexedTo100,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Logarithmic => "log",
            Self::Percentage => "percentage",
            Self::IndexedTo100 => "indexedTo100",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Logarithmic => "Logarithmic",
            Self::Percentage => "Percentage",
            Self::IndexedTo100 => "Indexed to 100",
        }
    }
}

impl fmt::Display for PriceScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Magnet mode for drawing tools (snapping to bars)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MagnetMode {
    /// No magnet/snapping
    #[default]
    Off,
    /// Weak magnet (close range)
    Weak,
    /// Strong magnet (wider range)
    Strong,
}

impl MagnetMode {
    pub fn all() -> &'static [Self] {
        &[Self::Off, Self::Weak, Self::Strong]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Weak => "weak",
            Self::Strong => "strong",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Weak => "Weak",
            Self::Strong => "Strong",
        }
    }

    /// Snap distance in pixels (for drawing tool magnet)
    pub fn snap_distance_px(&self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Weak => 20.0,
            Self::Strong => 50.0,
        }
    }
}

impl fmt::Display for MagnetMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Layout orientation for panels and widgets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    /// Horizontal layout
    #[default]
    Horizontal,
    /// Vertical layout
    Vertical,
    /// Auto-detect based on available space
    Auto,
}

impl Orientation {
    pub fn all() -> &'static [Self] {
        &[Self::Horizontal, Self::Vertical, Self::Auto]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Auto => "auto",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::Auto => "Auto",
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Size presets for UI elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Size {
    /// Extra small
    XSmall,
    /// Small
    Small,
    /// Medium (default)
    #[default]
    Medium,
    /// Large
    Large,
    /// Extra large
    XLarge,
    /// Automatic sizing
    Auto,
}

impl Size {
    pub fn all() -> &'static [Self] {
        &[
            Self::XSmall,
            Self::Small,
            Self::Medium,
            Self::Large,
            Self::XLarge,
            Self::Auto,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::XSmall => "xsmall",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::XLarge => "xlarge",
            Self::Auto => "auto",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::XSmall => "Extra Small",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
            Self::XLarge => "Extra Large",
            Self::Auto => "Auto",
        }
    }

    /// Size multiplier relative to base size
    pub fn multiplier(&self) -> f32 {
        match self {
            Self::XSmall => 0.75,
            Self::Small => 0.875,
            Self::Medium => 1.0,
            Self::Large => 1.25,
            Self::XLarge => 1.5,
            Self::Auto => 1.0,
        }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Resolution/Timeframe type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Resolution {
    /// 1 minute
    #[default]
    M1,
    /// 3 minutes
    M3,
    /// 5 minutes
    M5,
    /// 15 minutes
    M15,
    /// 30 minutes
    M30,
    /// 1 hour
    H1,
    /// 2 hours
    H2,
    /// 4 hours
    H4,
    /// 1 day
    D1,
    /// 1 week
    W1,
    /// 1 month
    MN1,
}

impl Resolution {
    pub fn all() -> &'static [Self] {
        &[
            Self::M1,
            Self::M3,
            Self::M5,
            Self::M15,
            Self::M30,
            Self::H1,
            Self::H2,
            Self::H4,
            Self::D1,
            Self::W1,
            Self::MN1,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::M1 => "1",
            Self::M3 => "3",
            Self::M5 => "5",
            Self::M15 => "15",
            Self::M30 => "30",
            Self::H1 => "60",
            Self::H2 => "120",
            Self::H4 => "240",
            Self::D1 => "1D",
            Self::W1 => "1W",
            Self::MN1 => "1M",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::M1 => "1 minute",
            Self::M3 => "3 minutes",
            Self::M5 => "5 minutes",
            Self::M15 => "15 minutes",
            Self::M30 => "30 minutes",
            Self::H1 => "1 hour",
            Self::H2 => "2 hours",
            Self::H4 => "4 hours",
            Self::D1 => "1 day",
            Self::W1 => "1 week",
            Self::MN1 => "1 month",
        }
    }

    /// Duration in seconds
    pub fn seconds(&self) -> u64 {
        match self {
            Self::M1 => 60,
            Self::M3 => 3 * 60,
            Self::M5 => 5 * 60,
            Self::M15 => 15 * 60,
            Self::M30 => 30 * 60,
            Self::H1 => 60 * 60,
            Self::H2 => 2 * 60 * 60,
            Self::H4 => 4 * 60 * 60,
            Self::D1 => 24 * 60 * 60,
            Self::W1 => 7 * 24 * 60 * 60,
            Self::MN1 => 30 * 24 * 60 * 60, // Approximate
        }
    }

    /// Minutes equivalent
    pub fn minutes(&self) -> u32 {
        match self {
            Self::M1 => 1,
            Self::M3 => 3,
            Self::M5 => 5,
            Self::M15 => 15,
            Self::M30 => 30,
            Self::H1 => 60,
            Self::H2 => 120,
            Self::H4 => 240,
            Self::D1 => 1440,
            Self::W1 => 10080,
            Self::MN1 => 43200,
        }
    }

    /// Is intraday timeframe (less than 1 day)
    pub fn is_intraday(&self) -> bool {
        matches!(
            self,
            Self::M1 | Self::M3 | Self::M5 | Self::M15 | Self::M30 | Self::H1 | Self::H2 | Self::H4
        )
    }

    /// Is daily or higher timeframe
    pub fn is_daily_or_higher(&self) -> bool {
        matches!(self, Self::D1 | Self::W1 | Self::MN1)
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl TryFrom<&str> for Resolution {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "1" => Ok(Self::M1),
            "3" => Ok(Self::M3),
            "5" => Ok(Self::M5),
            "15" => Ok(Self::M15),
            "30" => Ok(Self::M30),
            "60" => Ok(Self::H1),
            "120" => Ok(Self::H2),
            "240" => Ok(Self::H4),
            "1D" => Ok(Self::D1),
            "D" => Ok(Self::D1),
            "1W" => Ok(Self::W1),
            "W" => Ok(Self::W1),
            "1M" => Ok(Self::MN1),
            "M" => Ok(Self::MN1),
            _ => Err(format!("Unknown resolution: {}", value)),
        }
    }
}

/// Chart color theme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChartTheme {
    /// Light theme (white background)
    #[default]
    Light,
    /// Dark theme (dark background)
    Dark,
    /// Auto-detect based on system preference
    Auto,
}

impl ChartTheme {
    pub fn all() -> &'static [Self] {
        &[Self::Light, Self::Dark, Self::Auto]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Auto => "Auto",
        }
    }

    /// Is dark mode (for actual rendering)
    pub fn is_dark(&self, system_is_dark: bool) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::Auto => system_is_dark,
        }
    }
}

impl fmt::Display for ChartTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Crosshair mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CrosshairMode {
    /// Crosshair hidden
    #[default]
    Hidden,
    /// Vertical line only
    Vertical,
    /// Horizontal line only
    Horizontal,
    /// Both vertical and horizontal lines
    Both,
}

impl CrosshairMode {
    pub fn all() -> &'static [Self] {
        &[Self::Hidden, Self::Vertical, Self::Horizontal, Self::Both]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
            Self::Both => "both",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Hidden => "Hidden",
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
            Self::Both => "Both",
        }
    }
}

impl fmt::Display for CrosshairMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Price line source (which price point to use for line charts)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PriceLineSource {
    /// Use closing price
    #[default]
    Close,
    /// Use opening price
    Open,
    /// Use high price
    High,
    /// Use low price
    Low,
    /// Use typical price (H+L+C)/3
    Typical,
    /// Use median price (H+L)/2
    Median,
    /// Use OHLC average (O+H+L+C)/4
    OHLC,
    /// Use weighted close (H+L+C+C)/4
    WeightedClose,
}

impl PriceLineSource {
    pub fn all() -> &'static [Self] {
        &[
            Self::Close,
            Self::Open,
            Self::High,
            Self::Low,
            Self::Typical,
            Self::Median,
            Self::OHLC,
            Self::WeightedClose,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::Open => "open",
            Self::High => "high",
            Self::Low => "low",
            Self::Typical => "typical",
            Self::Median => "median",
            Self::OHLC => "ohlc",
            Self::WeightedClose => "weighted",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Close => "Close",
            Self::Open => "Open",
            Self::High => "High",
            Self::Low => "Low",
            Self::Typical => "Typical Price",
            Self::Median => "Median Price",
            Self::OHLC => "OHLC Average",
            Self::WeightedClose => "Weighted Close",
        }
    }
}

impl fmt::Display for PriceLineSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Scale location (where price/time scales appear)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScaleLocation {
    /// Scale on the right side
    #[default]
    Right,
    /// Scale on the left side
    Left,
    /// Scale on both sides
    Both,
    /// No scale shown
    None,
}

impl ScaleLocation {
    pub fn all() -> &'static [Self] {
        &[Self::Right, Self::Left, Self::Both, Self::None]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Both => "both",
            Self::None => "none",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Right => "Right",
            Self::Left => "Left",
            Self::Both => "Both",
            Self::None => "None",
        }
    }
}

impl fmt::Display for ScaleLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Background type for panels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BackgroundType {
    /// Solid color background
    #[default]
    Solid,
    /// Vertical gradient
    VerticalGradient,
    /// Horizontal gradient
    HorizontalGradient,
    /// Transparent background
    Transparent,
}

impl BackgroundType {
    pub fn all() -> &'static [Self] {
        &[
            Self::Solid,
            Self::VerticalGradient,
            Self::HorizontalGradient,
            Self::Transparent,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::VerticalGradient => "gradient_vertical",
            Self::HorizontalGradient => "gradient_horizontal",
            Self::Transparent => "transparent",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::VerticalGradient => "Vertical Gradient",
            Self::HorizontalGradient => "Horizontal Gradient",
            Self::Transparent => "Transparent",
        }
    }
}

impl fmt::Display for BackgroundType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Chart style -- defines how price data is visualized.
///
/// Discriminant values (`#[repr(u8)]`) match the TradingView wire
/// protocol for direct serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[derive(Default)]
pub enum ChartStyle {
    /// Traditional OHLC bars (vertical line with open/close ticks).
    Bar = 0,
    /// Japanese candlestick chart (default).
    #[default]
    Candle = 1,
    /// Simple line chart connecting close prices.
    Line = 2,
    /// Area chart (line with filled region below).
    Area = 3,
    /// Renko bricks -- requires [`ChartStyleParameters`].
    Renko = 4,
    /// Kagi lines -- requires [`ChartStyleParameters`].
    Kagi = 5,
    /// Point & Figure columns -- requires [`ChartStyleParameters`].
    PnF = 6,
    /// Three-Line Break chart -- requires [`ChartStyleParameters`].
    LineBreak = 7,
    /// Heikin Ashi (averaged) candles.
    HeikinAshi = 8,
    /// Hollow candlestick (body outlined for bullish, filled for bearish).
    HollowCandle = 9,
    /// Baseline chart with a configurable reference level.
    Baseline = 10,
    /// High-Low range bars.
    HighLow = 12,
    /// Vertical column bars (volume-style).
    Column = 13,
    /// Line chart with circular data-point markers.
    LineWithMarkers = 14,
    /// Step line (horizontal then vertical segments).
    Stepline = 15,
    /// HLC area chart.
    HLCArea = 16,
    /// Volume-weighted candles.
    VolCandle = 19,
    /// HLC bars (no open tick).
    HLCBars = 21,
}

impl ChartStyle {
    /// Wire-protocol discriminant value.
    pub fn tv_value(&self) -> u8 {
        *self as u8
    }

    /// Whether this chart style requires special parameters
    pub fn requires_parameters(&self) -> bool {
        matches!(
            self,
            Self::Renko | Self::Kagi | Self::PnF | Self::LineBreak | Self::Baseline
        )
    }

    /// TradingView string identifier for this style.
    pub fn tv_id(&self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Candle => "Candle",
            Self::Line => "Line",
            Self::Area => "Area",
            Self::Renko => "Renko",
            Self::Kagi => "Kagi",
            Self::PnF => "PnF",
            Self::LineBreak => "LineBreak",
            Self::HeikinAshi => "HeikinAshi",
            Self::HollowCandle => "HollowCandle",
            Self::Baseline => "Baseline",
            Self::HighLow => "HiLo",
            Self::Column => "Column",
            Self::LineWithMarkers => "LineWithMarkers",
            Self::Stepline => "Stepline",
            Self::HLCArea => "HLCArea",
            Self::VolCandle => "VolCandle",
            Self::HLCBars => "HLCBars",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Bar => "Bars",
            Self::Candle => "Candles",
            Self::Line => "Line",
            Self::Area => "Area",
            Self::Renko => "Renko",
            Self::Kagi => "Kagi",
            Self::PnF => "Point & Figure",
            Self::LineBreak => "Line Break",
            Self::HeikinAshi => "Heikin Ashi",
            Self::HollowCandle => "Hollow Candles",
            Self::Baseline => "Baseline",
            Self::HighLow => "High-Low",
            Self::Column => "Column",
            Self::LineWithMarkers => "Line with Markers",
            Self::Stepline => "Step Line",
            Self::HLCArea => "HLC Area",
            Self::VolCandle => "Volume Candles",
            Self::HLCBars => "HLC Bars",
        }
    }

    pub fn all() -> &'static [ChartStyle] {
        &[
            Self::Bar,
            Self::Candle,
            Self::Line,
            Self::Area,
            Self::Renko,
            Self::Kagi,
            Self::PnF,
            Self::LineBreak,
            Self::HeikinAshi,
            Self::HollowCandle,
            Self::Baseline,
            Self::HighLow,
            Self::Column,
            Self::LineWithMarkers,
            Self::Stepline,
            Self::HLCArea,
            Self::VolCandle,
            Self::HLCBars,
        ]
    }
}

impl fmt::Display for ChartStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Extra parameters for chart styles that need configuration.
///
/// Only the fields relevant to the active [`ChartStyle`] are used;
/// the rest are ignored. Call [`validate`](Self::validate) to check
/// that the required fields are present and positive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartStyleParameters {
    /// Renko brick size (price units or ATR multiplier)
    pub renko_brick_size: Option<f64>,
    /// Kagi reversal percentage
    pub kagi_reversal: Option<f64>,
    /// Range bar range size
    pub range_bar_size: Option<f64>,
    /// Point & Figure box size
    pub pnf_box_size: Option<f64>,
    /// Point & Figure reversal count
    pub pnf_reversal: Option<u32>,
    /// Line break line count
    pub line_break_count: Option<u32>,
    /// Baseline level (if fixed)
    pub baseline_level: Option<f64>,
}

impl Default for ChartStyleParameters {
    fn default() -> Self {
        Self {
            renko_brick_size: Some(1.0),
            kagi_reversal: Some(1.0),
            range_bar_size: Some(10.0),
            pnf_box_size: Some(1.0),
            pnf_reversal: Some(3),
            line_break_count: Some(3),
            baseline_level: None,
        }
    }
}

impl ChartStyleParameters {
    /// Validate parameters for a given style
    pub fn validate(&self, style: ChartStyle) -> Result<(), String> {
        match style {
            ChartStyle::Renko => {
                if self.renko_brick_size.is_none_or(|s| s <= 0.0) {
                    return Err("Renko brick size must be positive".to_string());
                }
            }
            ChartStyle::Kagi => {
                if self.kagi_reversal.is_none_or(|r| r <= 0.0) {
                    return Err("Kagi reversal must be positive".to_string());
                }
            }
            ChartStyle::PnF => {
                if self.pnf_box_size.is_none_or(|s| s <= 0.0) {
                    return Err("Point & Figure box size must be positive".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_style_pattern() {
        assert_eq!(LineStyle::Solid.pattern(), vec![f32::MAX]);
        assert_eq!(LineStyle::Dotted.pattern(), vec![2.0, 4.0]);
        assert_eq!(LineStyle::Dashed.pattern(), vec![10.0, 5.0]);
    }

    #[test]
    fn test_resolution_seconds() {
        assert_eq!(Resolution::M1.seconds(), 60);
        assert_eq!(Resolution::H1.seconds(), 3600);
        assert_eq!(Resolution::D1.seconds(), 86400);
    }

    #[test]
    fn test_resolution_parse() {
        assert_eq!(Resolution::try_from("5").unwrap(), Resolution::M5);
        assert_eq!(Resolution::try_from("1D").unwrap(), Resolution::D1);
        assert!(Resolution::try_from("invalid").is_err());
    }

    #[test]
    fn test_chart_theme_is_dark() {
        assert!(!ChartTheme::Light.is_dark(true));
        assert!(ChartTheme::Dark.is_dark(false));
        assert!(ChartTheme::Auto.is_dark(true));
    }

    #[test]
    fn test_price_line_source_all() {
        assert_eq!(PriceLineSource::all().len(), 8);
    }

    #[test]
    fn test_magnet_mode_distance() {
        assert_eq!(MagnetMode::Off.snap_distance_px(), 0.0);
        assert!(MagnetMode::Strong.snap_distance_px() > MagnetMode::Weak.snap_distance_px());
    }
}

/// Color Type - Defines color representation formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ColorType {
    /// Hex color string (#RRGGBB or #RRGGBBAA)
    #[default]
    Hex,
    /// RGB array [R, G, B]
    Rgb,
    /// RGBA array [R, G, B, A]
    Rgba,
    /// Color name (e.g., "red", "blue")
    Named,
}

impl ColorType {
    pub fn all() -> &'static [Self] {
        &[Self::Hex, Self::Rgb, Self::Rgba, Self::Named]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
            Self::Named => "named",
        }
    }
}

/// Override Line Style - Line styles for study/drawing overrides
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OverrideLineStyle {
    /// Solid line
    #[default]
    Solid,
    /// Dashed line
    Dashed,
    /// Dotted line
    Dotted,
    /// Large dashed
    LargeDashed,
    /// Sparse dotted
    SparseDotted,
}

impl OverrideLineStyle {
    pub fn all() -> &'static [Self] {
        &[
            Self::Solid,
            Self::Dashed,
            Self::Dotted,
            Self::LargeDashed,
            Self::SparseDotted,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Dashed => "dashed",
            Self::Dotted => "dotted",
            Self::LargeDashed => "large_dashed",
            Self::SparseDotted => "sparse_dotted",
        }
    }

    pub fn tv_value(&self) -> i32 {
        match self {
            Self::Solid => 0,
            Self::Dashed => 1,
            Self::Dotted => 2,
            Self::LargeDashed => 3,
            Self::SparseDotted => 4,
        }
    }
}

/// Override Price Axis Last Value Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OverridePriceAxisLastValueMode {
    /// Last value is shown
    #[default]
    LastValue,
    /// Last value is hidden
    LastValueHidden,
    /// Last value and previous close are shown
    LastValueAndPreviousClose,
}

impl OverridePriceAxisLastValueMode {
    pub fn all() -> &'static [Self] {
        &[
            Self::LastValue,
            Self::LastValueHidden,
            Self::LastValueAndPreviousClose,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::LastValue => "last_value",
            Self::LastValueHidden => "last_value_hidden",
            Self::LastValueAndPreviousClose => "last_value_and_previous_close",
        }
    }

    pub fn tv_value(&self) -> i32 {
        match self {
            Self::LastValue => 0,
            Self::LastValueHidden => 1,
            Self::LastValueAndPreviousClose => 2,
        }
    }
}
