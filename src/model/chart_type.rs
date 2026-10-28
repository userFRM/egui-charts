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
//!
//! # Chart Types
//! - Standard: Bars, Candles, Hollow Candles, Volume Candles
//! - Line-based: Line, Line with Markers, Step Line
//! - Area-based: Area, HLC Area, Baseline, Columns
//! - Japanese: Renko, Kagi, Line Break, Heikin-Ashi, Point & Figure
//! - Range-based: High-Low, Range Bars
//! - Advanced: TPO (Market Profile), Volume Footprint, Session Volume Profile

use serde::{Deserialize, Serialize};
use std::fmt;

/// Chart visualization types
///
/// Each variant represents a distinct way to visualize OHLCV data.
/// The default is `Candles` (Japanese candlesticks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ChartType {
    // === Standard OHLC Types ===
    /// Traditional OHLC bars with tick marks
    Bars,
    /// Japanese candlesticks (default)
    #[default]
    Candles,
    /// Hollow when close > open, filled when close < open
    HollowCandles,
    /// Candle body width proportional to the bar's volume over the largest visible volume;
    /// a zero volume draws a body as thin as the wick
    VolumeCandles,

    // === Line-Based Types ===
    /// Price line, using the close by default.
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
    /// One column per bar, colored by close versus the previous close.
    /// The base is the pane bottom unless `ChartTypeParams::columns_base` is set.
    Columns,

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
    /// Line Break chart with a configurable reversal lookback.
    LineBreak,
    /// Heikin-Ashi candles (smoothed)
    Heikin,
    /// Point and Figure (X and O columns)
    PointAndFigure,

    // === Advanced/Professional Types ===
    /// Time Price Opportunity (Market Profile)
    TimePriceOpportunity,
    /// Bid and ask volume at each price of every bar, summed from the trades the feed sends on
    /// the instrument's tick size. Requires a time-based interval and an instrument tick size;
    /// otherwise draws candles, as do bars before the first held trade.
    VolumeFootprint,
    /// Volume at each price of every session of the visible bars, with its point of control and
    /// value area, summed like [`VolumeFootprint`](Self::VolumeFootprint). Draws candles without
    /// the `studies` feature.
    SessionVolumeProfile,
}

/// Categories for grouping chart types in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
    /// Market Profile and execution-based charts.
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
            ChartType::Columns,
            ChartType::HighLow,
            ChartType::VolumeFootprint,
            ChartType::TimePriceOpportunity,
            ChartType::SessionVolumeProfile,
            ChartType::Heikin,
            ChartType::Renko,
            ChartType::LineBreak,
            ChartType::Kagi,
            ChartType::PointAndFigure,
            ChartType::Range,
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
            ChartType::Columns => "Columns",
            ChartType::HighLow => "High-low",
            ChartType::TimePriceOpportunity => "Time Price Opportunity",
            ChartType::LineBreak => "Line break",
            ChartType::Kagi => "Kagi",
            ChartType::Range => "Range",
            ChartType::PointAndFigure => "Point & Figure",
            ChartType::Renko => "Renko",
            ChartType::Heikin => "Heikin Ashi",
            ChartType::VolumeFootprint => "Volume footprint",
            ChartType::SessionVolumeProfile => "Session volume profile",
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
            ChartType::Columns => "Columns up to the close",
            ChartType::HighLow => "High-Low range",
            ChartType::TimePriceOpportunity => "TPO / Market Profile",
            ChartType::LineBreak => "Three line break",
            ChartType::Kagi => "Kagi chart",
            ChartType::Range => "Range bars",
            ChartType::PointAndFigure => "X and O chart",
            ChartType::Renko => "Renko bricks",
            ChartType::Heikin => "Heikin-Ashi candles",
            ChartType::VolumeFootprint => "Executed volume per price in each bar",
            ChartType::SessionVolumeProfile => "Executed volume per price in each session",
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

            ChartType::Area | ChartType::HlcArea | ChartType::Baseline | ChartType::Columns => {
                ChartTypeCategory::AreaBased
            }

            ChartType::Renko
            | ChartType::Kagi
            | ChartType::LineBreak
            | ChartType::Heikin
            | ChartType::PointAndFigure => ChartTypeCategory::Japanese,

            ChartType::HighLow | ChartType::Range => ChartTypeCategory::RangeBased,

            ChartType::TimePriceOpportunity
            | ChartType::VolumeFootprint
            | ChartType::SessionVolumeProfile => ChartTypeCategory::Advanced,
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
            | ChartType::TimePriceOpportunity
            | ChartType::VolumeFootprint
            | ChartType::SessionVolumeProfile => true,

            ChartType::Line
            | ChartType::LineWithMarkers
            | ChartType::StepLine
            | ChartType::Area
            | ChartType::Baseline
            | ChartType::Columns => false,
        }
    }

    /// Whether this chart type supports volume display
    pub fn supports_volume(&self) -> bool {
        // Time-independent charts don't align with volume
        !self.is_time_independent()
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

    /// Whether this chart type is time-independent, with one time-scale slot per element.
    /// Studies, drawings and markers are kept but not drawn, hit or edited on these types.
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

/// Where the Baseline chart draws its base line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BaselineLevel {
    /// A price.
    Price(f64),
    /// A share of the price pane height from its bottom, 0 to 100.
    PercentOfPane(f32),
}

/// The parameters of the chart types that need them. Sizes are in price units and are used as
/// given; a size the chart cannot use is reported on the chart instead of being replaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartTypeParams {
    /// Independent line, area and baseline styles, retained across chart type changes.
    pub styles: super::ChartTypeStyles,
    /// Renko: method resolved once from loaded history, then frozen while bars stream.
    pub renko_size_method: super::BoxSizeMethod,
    /// Kagi: method for its reversal amount.
    pub kagi_size_method: super::BoxSizeMethod,
    /// Range bars: method for their range.
    pub range_size_method: super::BoxSizeMethod,
    /// Point and Figure: method for its box size.
    pub pnf_size_method: super::BoxSizeMethod,
    /// Renko: close or high/low input, also used for wick extremes.
    pub renko_source: super::PriceBasedSource,
    /// Kagi: close or high/low input.
    pub kagi_source: super::PriceBasedSource,
    /// Line Break: close or high/low input.
    pub line_break_source: super::PriceBasedSource,
    /// Point and Figure: high/low, the traditional method, or close input.
    pub pnf_source: super::PriceBasedSource,
    /// Draw Renko wicks from the brick's actual high and low.
    pub renko_wicks: bool,
    /// Renko: size of a brick.
    pub renko_brick_size: f64,
    /// Kagi: the move against the line that starts a new line.
    pub kagi_reversal: f64,
    /// Range bars: the range of a bar from its low to its high.
    pub range_size: f64,
    /// Point and Figure: the size of a box.
    pub pnf_box_size: f64,
    /// Point and Figure: the number of boxes a move against the column must cover to start a
    /// new one.
    pub pnf_reversal: u32,
    /// Line Break: the number of lines a close must break through to reverse, 1 to 10.
    pub line_break_count: usize,
    /// Baseline: where the base line is drawn.
    pub baseline: BaselineLevel,
    /// Columns: the price the columns rise above or fall below; `None` draws them up from the
    /// pane bottom.
    pub columns_base: Option<f64>,
    /// TPO: minutes per period, raised to the bar interval when shorter.
    pub tpo_period_minutes: u32,
    /// TPO: instrument ticks per price row. Zero, or an instrument without a tick size, fits
    /// about 40 rows to the visible price range.
    pub tpo_row_ticks: u32,
    /// TPO: share of the TPOs in the value area, in percent.
    pub tpo_value_area: f64,
    /// TPO: separate profiles for pre-market, regular and post-market hours.
    pub tpo_split_sessions: bool,
    /// TPO: sessions combined into one profile, as in
    /// [`TPOConfig::composites`](super::TPOConfig::composites).
    pub tpo_composites: Vec<std::ops::RangeInclusive<chrono::DateTime<chrono::Utc>>>,
}

impl Default for ChartTypeParams {
    fn default() -> Self {
        Self {
            styles: super::ChartTypeStyles::default(),
            renko_size_method: super::BoxSizeMethod::Fixed,
            kagi_size_method: super::BoxSizeMethod::Fixed,
            range_size_method: super::BoxSizeMethod::Fixed,
            pnf_size_method: super::BoxSizeMethod::Fixed,
            renko_source: super::PriceBasedSource::Close,
            kagi_source: super::PriceBasedSource::Close,
            line_break_source: super::PriceBasedSource::Close,
            pnf_source: super::PriceBasedSource::HighLow,
            renko_wicks: false,
            renko_brick_size: 1.0,
            kagi_reversal: 1.0,
            range_size: 1.0,
            pnf_box_size: 1.0,
            pnf_reversal: 3,
            line_break_count: 3,
            baseline: BaselineLevel::PercentOfPane(50.0),
            columns_base: None,
            tpo_period_minutes: 30,
            tpo_row_ticks: 0,
            tpo_value_area: 70.0,
            tpo_split_sessions: false,
            tpo_composites: Vec::new(),
        }
    }
}

impl ChartTypeParams {
    /// The TPO settings for an instrument with minimum price increment `tick_size`, as the TPO
    /// chart type builds its profiles with them.
    pub fn tpo_config(&self, tick_size: Option<f64>) -> super::TPOConfig {
        super::TPOConfig {
            // A zero row lets the renderer fit the rows to the visible range.
            tick_size: tick_size.map_or(0.0, |tick| tick * f64::from(self.tpo_row_ticks)),
            period_minutes: self.tpo_period_minutes,
            value_area_pct: self.tpo_value_area / 100.0,
            split_sessions: self.tpo_split_sessions,
            composites: self.tpo_composites.clone(),
            ..Default::default()
        }
    }

    /// Create with ATR-based Renko brick size
    pub fn with_atr_renko(atr: f64, multiplier: f64) -> Self {
        Self {
            renko_brick_size: atr * multiplier,
            ..Default::default()
        }
    }

    /// Size methods in Renko, Kagi, Range, Point and Figure order.
    #[cfg(feature = "widget")]
    pub(crate) fn size_methods(&self) -> [super::BoxSizeMethod; 4] {
        [
            self.renko_size_method,
            self.kagi_size_method,
            self.range_size_method,
            self.pnf_size_method,
        ]
    }

    /// Resolve pending sizes only when their history is ready. Each successful calculation is
    /// frozen until explicitly armed again; prepending history and live corrections keep it.
    #[cfg(feature = "widget")]
    pub(crate) fn resolve_sizes(&mut self, bars: &[super::Bar], pending: &mut [bool; 4]) {
        let methods = self.size_methods();
        let sizes = [
            &mut self.renko_brick_size,
            &mut self.kagi_reversal,
            &mut self.range_size,
            &mut self.pnf_box_size,
        ];
        for ((method, size), pending) in methods.into_iter().zip(sizes).zip(pending) {
            if !*pending {
                continue;
            }
            let waiting = match method {
                super::BoxSizeMethod::Atr { period, .. } => period > bars.len(),
                super::BoxSizeMethod::Percentage(_) => bars.is_empty(),
                super::BoxSizeMethod::Fixed => false,
            };
            *size = method.resolve(*size, bars).unwrap_or(0.0);
            *pending = waiting;
        }
    }

    /// Why `chart_type` cannot be drawn with these parameters, if it cannot.
    #[cfg(feature = "widget")]
    pub(crate) fn invalid(&self, chart_type: ChartType) -> Option<&'static str> {
        let usable = |size: f64| size > 0.0 && size.is_finite();
        match chart_type {
            ChartType::Renko if !usable(self.renko_brick_size) => {
                Some("Renko brick size must be a positive number")
            }
            ChartType::Kagi if !usable(self.kagi_reversal) => {
                Some("Kagi reversal amount must be a positive number")
            }
            ChartType::Range if !usable(self.range_size) => {
                Some("Range size must be a positive number")
            }
            ChartType::PointAndFigure if !usable(self.pnf_box_size) || self.pnf_reversal == 0 => {
                Some(
                    "Point and Figure box size must be a positive number and the reversal at least 1 box",
                )
            }
            ChartType::LineBreak if !(1..=10).contains(&self.line_break_count) => {
                Some("Line Break line count must be 1 to 10")
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
