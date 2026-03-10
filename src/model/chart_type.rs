//! Chart Type Definitions
//!
//! Defines all chart visualization types supported by the charting library.
//! This module provides the canonical definition of chart types, their metadata,
//! categories, and configuration requirements.
//!
//! # Terminology
//!
//! | Context        | Term to use         | Notes                                      |
//! |----------------|---------------------|--------------------------------------------|
//! | Enum variant   | `Candles`           | Short form, matches Rust naming convention  |
//! | Doc comments   | "candlestick"       | Full technical name (Japanese candlesticks) |
//! | Heikin-Ashi    | Always hyphenated   | Enum variant stays `Heikin`                 |
//! | SeriesType     | `Candles`           | Matches `ChartType::Candles`                |
//!
//! # Chart Types
//! - Standard: Bars, Candles, Hollow Candles, Volume Candles
//! - Line-based: Line, Line with Markers, Step Line
//! - Area-based: Area, HLC Area, Baseline
//! - Japanese: Renko, Kagi, Line Break, Heikin-Ashi
//! - Range-based: Range Bars, Point & Figure
//! - Advanced: Volume Footprint, TPO, Session Volume, High-Low

use std::fmt;

/// Chart visualization types
///
/// Each variant represents a distinct way to visualize OHLCV data.
/// The default is `Candles` (Japanese candlesticks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChartType {
    // === Standard OHLC Types ===
    /// Traditional OHLC bars with tick marks
    Bars,
    /// Japanese candlesticks (default)
    #[default]
    Candles,
    /// Hollow when close > open, filled when close < open
    HollowCandles,
    /// Candle width proportional to volume
    VolumeCandles,

    // === Line-Based Types ===
    /// Simple close price line
    Line,
    /// Line with circular markers at each data point
    LineWithMarkers,
    /// Stepped line (horizontal then vertical)
    StepLine,

    // === Area-Based Types ===
    /// Filled area under close price line
    Area,
    /// High-Low-Close area chart
    HlcArea,
    /// Baseline comparison chart with colored fill
    Baseline,

    // === Range Types ===
    /// High-Low range visualization
    HighLow,
    /// Range bars (price-based, time-independent)
    Range,

    // === Japanese Chart Types ===
    /// Renko bricks (price movement only)
    Renko,
    /// Kagi chart (trend reversals)
    Kagi,
    /// Three line break chart
    LineBreak,
    /// Heikin-Ashi candles (smoothed)
    Heikin,
    /// Point and Figure (X and O columns)
    PointAndFigure,

    // === Advanced/Professional Types ===
    /// Volume footprint / order flow
    VolumeFootprint,
    /// Time Price Opportunity (Market Profile)
    TimePriceOpportunity,
    /// Volume aggregated by session
    SessionVolume,
}

/// Categories for grouping chart types in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartTypeCategory {
    /// Standard OHLC visualizations
    Standard,
    /// Line-based charts
    LineBased,
    /// Area/filled charts
    AreaBased,
    /// Japanese charting techniques
    Japanese,
    /// Range/price-based charts
    RangeBased,
    /// Advanced professional charts
    Advanced,
}

impl ChartTypeCategory {
    /// Display name for the category
    pub fn name(&self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::LineBased => "Line",
            Self::AreaBased => "Area",
            Self::Japanese => "Japanese",
            Self::RangeBased => "Range",
            Self::Advanced => "Advanced",
        }
    }

    /// All categories in display order
    pub fn all() -> &'static [ChartTypeCategory] {
        &[
            Self::Standard,
            Self::LineBased,
            Self::AreaBased,
            Self::Japanese,
            Self::RangeBased,
            Self::Advanced,
        ]
    }
}

impl ChartType {
    /// All chart types in display order
    pub fn all() -> &'static [ChartType] {
        &[
            ChartType::Bars,
            ChartType::Candles,
            ChartType::HollowCandles,
            ChartType::VolumeCandles,
            ChartType::Line,
            ChartType::LineWithMarkers,
            ChartType::StepLine,
            ChartType::Area,
            ChartType::HlcArea,
            ChartType::Baseline,
            ChartType::HighLow,
            ChartType::VolumeFootprint,
            ChartType::TimePriceOpportunity,
            ChartType::SessionVolume,
            ChartType::LineBreak,
            ChartType::Kagi,
            ChartType::Range,
            ChartType::PointAndFigure,
            ChartType::Renko,
            ChartType::Heikin,
        ]
    }

    /// User-facing display name
    pub fn name(&self) -> &'static str {
        match self {
            ChartType::Bars => "Bars",
            ChartType::Candles => "Candles",
            ChartType::HollowCandles => "Hollow candles",
            ChartType::VolumeCandles => "Volume candles",
            ChartType::Line => "Line",
            ChartType::LineWithMarkers => "Line with markers",
            ChartType::StepLine => "Step line",
            ChartType::Area => "Area",
            ChartType::HlcArea => "HLC area",
            ChartType::Baseline => "Baseline",
            ChartType::HighLow => "High-low",
            ChartType::VolumeFootprint => "Volume footprint",
            ChartType::TimePriceOpportunity => "Time Price Opportunity",
            ChartType::SessionVolume => "Session volume",
            ChartType::LineBreak => "Line break",
            ChartType::Kagi => "Kagi",
            ChartType::Range => "Range",
            ChartType::PointAndFigure => "Point & Figure",
            ChartType::Renko => "Renko",
            ChartType::Heikin => "Heikin Ashi",
        }
    }

    /// Alias for name() for string compatibility
    pub fn as_str(&self) -> &'static str {
        self.name()
    }

    /// Technical description for tooltips
    pub fn description(&self) -> &'static str {
        match self {
            ChartType::Bars => "OHLC bars with tick marks",
            ChartType::Candles => "Japanese candlesticks",
            ChartType::HollowCandles => "Hollow when close > open",
            ChartType::VolumeCandles => "Width based on volume",
            ChartType::Line => "Close price line",
            ChartType::LineWithMarkers => "Line with data points",
            ChartType::StepLine => "Stepped line chart",
            ChartType::Area => "Filled area chart",
            ChartType::HlcArea => "High-Low-Close area",
            ChartType::Baseline => "Baseline comparison",
            ChartType::HighLow => "High-Low range",
            ChartType::VolumeFootprint => "Order flow analysis",
            ChartType::TimePriceOpportunity => "TPO / Market Profile",
            ChartType::SessionVolume => "Volume by session",
            ChartType::LineBreak => "Three line break",
            ChartType::Kagi => "Kagi chart",
            ChartType::Range => "Range bars",
            ChartType::PointAndFigure => "X and O chart",
            ChartType::Renko => "Renko bricks",
            ChartType::Heikin => "Heikin-Ashi candles",
        }
    }

    /// Alias for description() for compatibility
    pub fn desc(&self) -> &'static str {
        self.description()
    }

    /// Category this chart type belongs to
    pub fn category(&self) -> ChartTypeCategory {
        match self {
            ChartType::Bars
            | ChartType::Candles
            | ChartType::HollowCandles
            | ChartType::VolumeCandles => ChartTypeCategory::Standard,

            ChartType::Line | ChartType::LineWithMarkers | ChartType::StepLine => {
                ChartTypeCategory::LineBased
            }

            ChartType::Area | ChartType::HlcArea | ChartType::Baseline => {
                ChartTypeCategory::AreaBased
            }

            ChartType::Renko
            | ChartType::Kagi
            | ChartType::LineBreak
            | ChartType::Heikin
            | ChartType::PointAndFigure => ChartTypeCategory::Japanese,

            ChartType::HighLow | ChartType::Range => ChartTypeCategory::RangeBased,

            ChartType::VolumeFootprint
            | ChartType::TimePriceOpportunity
            | ChartType::SessionVolume => ChartTypeCategory::Advanced,
        }
    }

    /// Get all chart types in a specific category
    pub fn in_category(category: ChartTypeCategory) -> Vec<ChartType> {
        Self::all()
            .iter()
            .copied()
            .filter(|ct| ct.category() == category)
            .collect()
    }

    /// Whether this chart type uses OHLC data (vs single value)
    pub fn uses_ohlc(&self) -> bool {
        match self {
            ChartType::Bars
            | ChartType::Candles
            | ChartType::HollowCandles
            | ChartType::VolumeCandles
            | ChartType::HlcArea
            | ChartType::HighLow
            | ChartType::Renko
            | ChartType::Kagi
            | ChartType::LineBreak
            | ChartType::Heikin
            | ChartType::Range
            | ChartType::PointAndFigure
            | ChartType::VolumeFootprint
            | ChartType::TimePriceOpportunity
            | ChartType::SessionVolume => true,

            ChartType::Line
            | ChartType::LineWithMarkers
            | ChartType::StepLine
            | ChartType::Area
            | ChartType::Baseline => false,
        }
    }

    /// Whether this chart type supports volume display
    pub fn supports_volume(&self) -> bool {
        match self {
            // Time-independent charts don't align with volume
            ChartType::Renko
            | ChartType::Kagi
            | ChartType::PointAndFigure
            | ChartType::Range
            | ChartType::LineBreak => false,

            // Volume footprint IS the volume display
            ChartType::VolumeFootprint | ChartType::SessionVolume => false,

            _ => true,
        }
    }

    /// Whether this chart type requires special parameters
    pub fn requires_parameters(&self) -> bool {
        matches!(
            self,
            ChartType::Renko
                | ChartType::Kagi
                | ChartType::Range
                | ChartType::PointAndFigure
                | ChartType::LineBreak
                | ChartType::Baseline
        )
    }

    /// Whether this chart type is time-independent
    pub fn is_time_independent(&self) -> bool {
        matches!(
            self,
            ChartType::Renko
                | ChartType::Kagi
                | ChartType::Range
                | ChartType::PointAndFigure
                | ChartType::LineBreak
        )
    }

    /// Whether this chart type transforms the input data
    pub fn transforms_data(&self) -> bool {
        matches!(
            self,
            ChartType::Renko
                | ChartType::Kagi
                | ChartType::Range
                | ChartType::PointAndFigure
                | ChartType::LineBreak
                | ChartType::Heikin
        )
    }
}

impl fmt::Display for ChartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

// === ChartStyle <-> ChartType Conversions ===

use crate::model::enums::ChartStyle;

/// Every TV `ChartStyle` maps to a domain `ChartType`.
impl From<ChartStyle> for ChartType {
    fn from(style: ChartStyle) -> Self {
        match style {
            ChartStyle::Bar => ChartType::Bars,
            ChartStyle::Candle => ChartType::Candles,
            ChartStyle::Line => ChartType::Line,
            ChartStyle::Area => ChartType::Area,
            ChartStyle::Renko => ChartType::Renko,
            ChartStyle::Kagi => ChartType::Kagi,
            ChartStyle::PnF => ChartType::PointAndFigure,
            ChartStyle::LineBreak => ChartType::LineBreak,
            ChartStyle::HeikinAshi => ChartType::Heikin,
            ChartStyle::HollowCandle => ChartType::HollowCandles,
            ChartStyle::Baseline => ChartType::Baseline,
            ChartStyle::HighLow => ChartType::HighLow,
            ChartStyle::Column => ChartType::Bars, // closest match
            ChartStyle::LineWithMarkers => ChartType::LineWithMarkers,
            ChartStyle::Stepline => ChartType::StepLine,
            ChartStyle::HLCArea => ChartType::HlcArea,
            ChartStyle::VolCandle => ChartType::VolumeCandles,
            ChartStyle::HLCBars => ChartType::HighLow, // closest match
        }
    }
}

impl ChartType {
    /// Convert to `ChartStyle`, if a mapping exists.
    ///
    /// Domain-only types (VolumeFootprint, TimePriceOpportunity, SessionVolume, Range)
    /// have no wire-format equivalent and return `None`.
    pub fn to_chart_style(&self) -> Option<ChartStyle> {
        match self {
            ChartType::Bars => Some(ChartStyle::Bar),
            ChartType::Candles => Some(ChartStyle::Candle),
            ChartType::HollowCandles => Some(ChartStyle::HollowCandle),
            ChartType::VolumeCandles => Some(ChartStyle::VolCandle),
            ChartType::Line => Some(ChartStyle::Line),
            ChartType::LineWithMarkers => Some(ChartStyle::LineWithMarkers),
            ChartType::StepLine => Some(ChartStyle::Stepline),
            ChartType::Area => Some(ChartStyle::Area),
            ChartType::HlcArea => Some(ChartStyle::HLCArea),
            ChartType::Baseline => Some(ChartStyle::Baseline),
            ChartType::HighLow => Some(ChartStyle::HighLow),
            ChartType::Renko => Some(ChartStyle::Renko),
            ChartType::Kagi => Some(ChartStyle::Kagi),
            ChartType::LineBreak => Some(ChartStyle::LineBreak),
            ChartType::Heikin => Some(ChartStyle::HeikinAshi),
            ChartType::PointAndFigure => Some(ChartStyle::PnF),
            // Domain-only types with no TV equivalent
            ChartType::Range
            | ChartType::VolumeFootprint
            | ChartType::TimePriceOpportunity
            | ChartType::SessionVolume => None,
        }
    }
}

/// Configuration parameters for chart types that require them
#[derive(Debug, Clone)]
pub struct ChartTypeParams {
    /// Renko brick size (price units)
    pub renko_brick_size: f64,
    /// Kagi reversal amount (price units or percentage)
    pub kagi_reversal: f64,
    /// Range bar size (price units)
    pub range_size: f64,
    /// Point & Figure box size
    pub pnf_box_size: f64,
    /// Point & Figure reversal count
    pub pnf_reversal: u32,
    /// Line break line count (typically 3)
    pub line_break_count: usize,
    /// Baseline price level
    pub baseline_price: f64,
}

impl Default for ChartTypeParams {
    fn default() -> Self {
        Self {
            renko_brick_size: 1.0,
            kagi_reversal: 4.0,
            range_size: 10.0,
            pnf_box_size: 1.0,
            pnf_reversal: 3,
            line_break_count: 3,
            baseline_price: 0.0, // Will be calculated from data
        }
    }
}

impl ChartTypeParams {
    /// Create with ATR-based Renko brick size
    pub fn with_atr_renko(atr: f64, multiplier: f64) -> Self {
        Self {
            renko_brick_size: atr * multiplier,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_chart_types_count() {
        assert_eq!(ChartType::all().len(), 20);
    }

    #[test]
    fn test_default_is_candles() {
        assert_eq!(ChartType::default(), ChartType::Candles);
    }

    #[test]
    fn test_category_grouping() {
        let standard = ChartType::in_category(ChartTypeCategory::Standard);
        assert!(standard.contains(&ChartType::Candles));
        assert!(standard.contains(&ChartType::Bars));
        assert!(!standard.contains(&ChartType::Line));
    }

    #[test]
    fn test_ohlc_vs_single_value() {
        assert!(ChartType::Candles.uses_ohlc());
        assert!(!ChartType::Line.uses_ohlc());
    }

    #[test]
    fn test_volume_support() {
        assert!(ChartType::Candles.supports_volume());
        assert!(!ChartType::Renko.supports_volume());
    }

    #[test]
    fn test_time_independence() {
        assert!(ChartType::Renko.is_time_independent());
        assert!(!ChartType::Candles.is_time_independent());
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", ChartType::Candles), "Candles");
        assert_eq!(format!("{}", ChartType::Heikin), "Heikin Ashi");
    }
}
