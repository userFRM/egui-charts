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
    /// the instrument's tick size, in the same bars as the chart on tick, volume, range and delta
    /// intervals. Requires an instrument tick size; otherwise draws candles, as do bars before the
    /// first held trade.
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

/// Numbers shown in a footprint column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintMode {
    /// Sell volume on the left, buy volume on the right.
    BidAsk,
    /// All executed volume, including unknown sides.
    Volume,
    /// Buy minus sell volume.
    Delta,
    /// Delta followed by total volume.
    DeltaVolume,
    /// Only the larger of the sell and buy volume of the row, at the left of the cell when sells
    /// dominate and at the right when buys do, centred on a tie. The other side stays out.
    DominantSide,
    /// Sell execution count on the left, buy execution count on the right.
    BidAskTrades,
    /// Total execution count, including unknown sides.
    Trades,
    /// Total volume divided by execution count; absent for an empty row.
    AverageSize,
    /// A narrow candle body in the middle of the column, with the sell volume of each row in a
    /// cell on its left and the buy volume in a cell on its right.
    Ladder,
}

/// What determines a footprint cell's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintColor {
    /// Volume relative to the largest row of this bar.
    Volume,
    /// Delta as a fraction of the row's classified volume.
    Delta,
    /// Dominant bid or ask side, using the configured comparison.
    Imbalance,
    /// The neutral colour for every cell, whatever was traded.
    Fixed,
    /// The buy colour for a positive delta and the sell colour for a negative one, with the
    /// opacity growing with the delta's absolute size relative to the bar's largest. A zero delta
    /// takes the neutral colour at full opacity.
    DeltaOpacity,
    /// Sell and buy volume shaded separately, each in one of four strengths of its own colour by
    /// the quarter it falls in between the smallest and the largest volume of its side in the
    /// bar. The POC row takes the text colour, the value area is two dotted lines, and a side
    /// that dominates under the imbalance settings gets a strip along its outer edge.
    VolumeSteps,
}

/// How `studies::TradeProfile::nodes` (requires `studies`) chooses volume nodes.
///
/// The default compares each price row with its immediate neighbours, which is the most
/// sensitive setting. Raising `smoothing` or `prominence` leaves only the broader nodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeSensitivity {
    /// Rows on each side of a row that are averaged with it before comparing; zero compares
    /// the rows as traded. Untraded rows count as zero volume.
    pub smoothing: u32,
    /// Smallest rise of a peak above, or drop of a valley below, the higher respectively
    /// lower of its neighbouring plateaus, as a share of the largest smoothed volume. NaN
    /// counts as zero and values are clamped to 0..=1.
    pub prominence: f64,
}

/// The daily hours of a [`ProfileSession::Custom`] session profile.
///
/// A window that ends at or before its start spans midnight and belongs to the day it ends on.
/// Clock changes in `zone` move the window with them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomProfileSession {
    /// Wall-clock time each session opens, in `zone`.
    pub start: chrono::NaiveTime,
    /// Wall-clock time each session closes, in `zone`.
    pub end: chrono::NaiveTime,
    /// Zone of `start` and `end`. `None` uses the instrument's exchange zone, or UTC without one.
    pub zone: Option<super::TimezoneId>,
}

/// Which hours divide a session volume profile chart into sessions.
///
/// Only intraday charts are divided this way: daily and longer bars profile by trading date.
/// Bars outside the chosen hours still join the session before them, so a host that wants
/// them left out should not supply them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ProfileSession {
    /// The chart's own calendar: its instrument's hours, with extended hours as the chart shows
    /// them, or calendar days in the instrument's zone without hours.
    #[default]
    Chart,
    /// The instrument's regular windows only. The chart's calendar without instrument hours.
    Regular,
    /// The instrument's regular and extended windows. The chart's calendar without instrument
    /// hours.
    Extended,
    /// The same hours every day, whatever the instrument trades.
    Custom(CustomProfileSession),
}

/// How a session volume profile groups executions into price rows, in instrument ticks.
///
/// Rows are never finer than the feed's own rows, and align to multiples of their height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileRows {
    /// The rows the feed groups executions into.
    #[default]
    Feed,
    /// About this many rows between each session's lowest and highest traded rows.
    Count(u32),
    /// At least this many instrument ticks per row.
    Ticks(u32),
}

/// Display settings of the chart's execution-based session profiles.
/// Defaults retain the stacked session histogram, with optional annotations hidden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TradeProfileSettings {
    /// Hours that divide the chart into sessions.
    pub session: ProfileSession,
    /// Price row grouping.
    pub rows: ProfileRows,
    /// Bid/ask stacks, total volume or signed delta; `DeltaVolume` also draws signed delta.
    pub mode: FootprintMode,
    /// Trace the total-volume POC after each complete execution bar.
    pub developing_poc: bool,
    /// Trace the outer value area edges after each complete execution bar; hidden without
    /// `value_area`. Uses that share and the same row selection as the final profile.
    pub developing_value_area: bool,
    /// Extend a completed session's POC to the first later bar that trades in its row: a bar
    /// with volume whose low-to-high range reaches the row. Untouched, it runs to the viewport's
    /// right edge.
    pub extend_poc: bool,
    /// Extend a completed session's value area high, as for `extend_poc`; hidden without
    /// `value_area`.
    pub extend_vah: bool,
    /// Extend a completed session's value area low, as for `extend_poc`; hidden without
    /// `value_area`.
    pub extend_val: bool,
    /// Volume-node smoothing and prominence; `None` hides the node labels.
    pub nodes: Option<NodeSensitivity>,
    /// Share of total volume shaded as the contiguous value area; `None` hides it.
    pub value_area: Option<f64>,
    /// Draw guides at the outer edges of the value area.
    pub value_area_guides: bool,
}

impl Default for TradeProfileSettings {
    fn default() -> Self {
        Self {
            session: ProfileSession::Chart,
            rows: ProfileRows::Feed,
            mode: FootprintMode::BidAsk,
            developing_poc: false,
            developing_value_area: false,
            extend_poc: false,
            extend_vah: false,
            extend_val: false,
            nodes: None,
            value_area: Some(0.7),
            value_area_guides: false,
        }
    }
}

/// A row of the per-bar execution statistics table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintStatistic {
    /// Total executed volume, including unknown sides.
    Volume,
    /// Sell volume.
    Bid,
    /// Buy volume.
    Ask,
    /// Buy volume minus sell volume.
    Delta,
    /// Minimum running delta within the bar.
    MinDelta,
    /// Maximum running delta within the bar.
    MaxDelta,
    /// Running delta across complete bars.
    CumulativeDelta,
    /// Delta divided by total volume, in percent.
    DeltaPercent,
    /// Buy volume divided by sell volume.
    AskBidRatio,
    /// Total execution count, including unknown sides.
    Trades,
    /// Total volume divided by execution count; undefined for an empty bar.
    AverageSize,
}

impl FootprintStatistic {
    /// All available rows in their display order.
    pub const ALL: [Self; 11] = [
        Self::Volume,
        Self::Bid,
        Self::Ask,
        Self::Delta,
        Self::MinDelta,
        Self::MaxDelta,
        Self::CumulativeDelta,
        Self::DeltaPercent,
        Self::AskBidRatio,
        Self::Trades,
        Self::AverageSize,
    ];

    /// Shared table label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "Vol",
            Self::Bid => "Bid",
            Self::Ask => "Ask",
            Self::Delta => "Delta",
            Self::MinDelta => "Min",
            Self::MaxDelta => "Max",
            Self::CumulativeDelta => "CVD",
            Self::DeltaPercent => "Delta %",
            Self::AskBidRatio => "Ask/Bid",
            Self::Trades => "Trades",
            Self::AverageSize => "Average size",
        }
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

/// Parameters and display settings retained when changing chart types. Price-based sizes are
/// in price units and used as given; an unusable size is reported instead of being replaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartTypeParams {
    /// Footprint: up to four content columns.
    pub footprint_modes: Vec<FootprintMode>,
    /// Footprint: proportional volume profiles behind the numbers.
    pub footprint_profile: bool,
    /// Footprint: cell colouring rule.
    pub footprint_color_by: FootprintColor,
    /// Footprint: buy, sell, text and neutral colours. None follows the chart theme.
    pub footprint_colors: [Option<egui::Color32>; 4],
    /// Footprint: instrument ticks per row; zero groups automatically with zoom.
    pub footprint_row_ticks: u32,
    /// Footprint: put a strip along the outer edge of a side that dominates under the imbalance
    /// settings, under [`FootprintColor::VolumeSteps`].
    pub footprint_imbalance: bool,
    /// Footprint: outline the highest-volume row, or fill it under
    /// [`FootprintColor::VolumeSteps`].
    pub footprint_poc: bool,
    /// Footprint: share of volume in the value area; None hides it.
    pub footprint_value_area: Option<f64>,
    /// Footprint: dominance multiple for imbalances.
    pub footprint_imbalance_ratio: f64,
    /// Footprint: minimum dominant-side volume after grouping.
    pub footprint_imbalance_min_volume: f64,
    /// Footprint: compare opposite sides one price row apart.
    pub footprint_diagonal: bool,
    /// Footprint: mark stacked imbalance zones.
    pub footprint_stacked_imbalances: bool,
    /// Footprint: adjacent imbalances required for a zone.
    pub footprint_stack: usize,
    /// Footprint: extend volume POCs until revisited.
    pub footprint_naked_poc: bool,
    /// Footprint: mark the largest absolute delta row.
    pub footprint_delta_poc: bool,
    /// Footprint: extend unfinished highs and lows until revisited.
    pub footprint_unfinished_auctions: bool,
    /// Footprint: minimum share of bar volume for a cluster; None hides clusters.
    pub footprint_cluster_share: Option<f64>,
    /// Footprint: maximum extreme-row volume for a single print; None hides them.
    pub footprint_single_print_max: Option<f64>,
    /// Footprint: minimum absolute delta percentage for a bar marker; None hides it.
    pub footprint_delta_percent: Option<f64>,
    /// Footprint: minimum buy/sell or sell/buy multiple for a bar marker; None hides it.
    pub footprint_ratio: Option<f64>,
    /// Footprint: write the ask/bid ratios at the bar extremes.
    pub footprint_extreme_ratio: bool,
    /// Footprint: outline rows between the close and the extreme beyond it.
    pub footprint_pullback: bool,
    /// Footprint: prefer abbreviated numbers.
    pub footprint_abbreviate: bool,
    /// Footprint: write zero values rather than leaving them blank.
    pub footprint_show_zero: bool,
    /// Footprint: preferred minimum and maximum font sizes in points, within 8 to 24.
    pub footprint_font_size: [f32; 2],
    /// Footprint: write the sell and buy totals and a delta and total volume box below each bar
    /// of a [`FootprintMode::Ladder`] column.
    pub footprint_info_box: bool,
    /// Footprint: show the selected statistics rows below the price pane.
    pub footprint_statistics: bool,
    /// Footprint: statistics rows in display order; an empty list hides the table.
    pub footprint_statistics_rows: Vec<FootprintStatistic>,
    /// Execution-based session profile display settings.
    pub profile: TradeProfileSettings,
    /// Independent line, area and baseline styles, retained across chart type changes.
    #[serde(default = "super::ChartTypeStyles::saved_without_styles")]
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
    /// TPO: minutes from the session open included in the initial balance, 60 by default.
    pub tpo_initial_balance_minutes: u32,
    /// TPO: mark the highest and lowest rows when they hold more than one TPO.
    pub tpo_poor_highs_lows: bool,
    /// TPO: draw one and two initial balance ranges above its high and below its low.
    pub tpo_initial_balance_extensions: bool,
    /// TPO: instrument ticks per price row. Zero, or an instrument without a tick size, fits
    /// about 40 rows to the visible price range, each a whole number of the instrument's ticks
    /// when it has a tick size.
    pub tpo_row_ticks: u32,
    /// TPO: share of the TPOs in the value area, in percent.
    pub tpo_value_area: f64,
    /// TPO: separate profiles for pre-market, regular and post-market hours.
    pub tpo_split_sessions: bool,
    /// TPO: highlight the rows touched by a single period.
    pub tpo_single_prints: bool,
    /// TPO: caption and mark the middle of each profile's range.
    pub tpo_midpoint: bool,
    /// TPO: paint each period's letter inside its block.
    pub tpo_letters: bool,
    /// TPO: sessions combined into one profile, as in
    /// [`TPOConfig::composites`](super::TPOConfig::composites).
    pub tpo_composites: Vec<std::ops::RangeInclusive<chrono::DateTime<chrono::Utc>>>,
}

impl Default for ChartTypeParams {
    fn default() -> Self {
        Self {
            footprint_modes: vec![FootprintMode::Ladder],
            footprint_profile: false,
            footprint_color_by: FootprintColor::VolumeSteps,
            footprint_colors: [None; 4],
            footprint_row_ticks: 0,
            footprint_imbalance: true,
            footprint_poc: true,
            footprint_value_area: Some(0.7),
            footprint_imbalance_ratio: 3.0,
            footprint_imbalance_min_volume: 0.0,
            footprint_diagonal: true,
            footprint_stacked_imbalances: false,
            footprint_stack: 3,
            footprint_naked_poc: false,
            footprint_delta_poc: false,
            footprint_unfinished_auctions: false,
            footprint_cluster_share: None,
            footprint_single_print_max: None,
            footprint_delta_percent: None,
            footprint_ratio: None,
            footprint_extreme_ratio: false,
            footprint_pullback: false,
            footprint_abbreviate: true,
            footprint_show_zero: true,
            footprint_font_size: [10.0, 10.0],
            footprint_info_box: true,
            footprint_statistics: false,
            footprint_statistics_rows: vec![
                FootprintStatistic::Volume,
                FootprintStatistic::Delta,
                FootprintStatistic::DeltaPercent,
            ],
            profile: TradeProfileSettings::default(),
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
            tpo_initial_balance_minutes: 60,
            tpo_poor_highs_lows: false,
            tpo_initial_balance_extensions: false,
            tpo_row_ticks: 0,
            tpo_value_area: 70.0,
            tpo_split_sessions: false,
            tpo_single_prints: false,
            tpo_midpoint: false,
            tpo_letters: false,
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
            initial_balance_minutes: self.tpo_initial_balance_minutes,
            show_poor_highs_lows: self.tpo_poor_highs_lows,
            show_initial_balance_extensions: self.tpo_initial_balance_extensions,
            value_area_pct: self.tpo_value_area / 100.0,
            split_sessions: self.tpo_split_sessions,
            show_single_prints: self.tpo_single_prints,
            show_midpoint: self.tpo_midpoint,
            display_mode: if self.tpo_letters {
                super::TPODisplayMode::Both
            } else {
                super::TPODisplayMode::Blocks
            },
            composites: self.tpo_composites.clone(),
            instrument_tick: tick_size,
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
    fn saved_execution_settings_keep_choices_and_legacy_defaults() {
        let settings = serde_json::json!({
            "footprint_modes": ["Delta", "BidAskTrades", "Trades", "AverageSize"], "footprint_row_ticks": 2,
            "footprint_value_area": 0.010003969988090037,
            "footprint_statistics": true, "footprint_statistics_rows": ["Volume", "Delta", "Trades", "AverageSize"],
            "footprint_colors": [[12, 34, 56, 255], null, null, null],
            "profile": {"mode": "Volume", "value_area": 0.5, "developing_poc": true,
                "developing_value_area": true, "extend_poc": true, "extend_vah": true,
                "extend_val": true,
                "nodes": {"smoothing": 2, "prominence": 0.1}}
        });
        let params: ChartTypeParams = serde_json::from_value(settings.clone()).unwrap();
        let saved = serde_json::to_value(&params).unwrap();
        for (name, expected) in settings.as_object().unwrap() {
            if name == "profile" {
                for (field, expected) in expected.as_object().unwrap() {
                    assert_eq!(&saved[name][field], expected);
                }
            } else {
                assert_eq!(&saved[name], expected);
            }
        }
        assert_eq!(
            serde_json::from_value::<ChartTypeParams>(saved).unwrap(),
            params
        );
        #[cfg(feature = "ui")]
        {
            for chart_type in [ChartType::VolumeFootprint, ChartType::SessionVolumeProfile] {
                let mut series = crate::chart::series::SeriesSettings {
                    chart_type,
                    chart_type_params: params.clone(),
                    ..Default::default()
                };
                egui::Context::default()
                    .run_ui(egui::RawInput::default(), |ui| {
                        crate::ui::dialogs::series_settings::SymbolTab::show(ui, &mut series);
                    })
                    .drop_without_applying_deltas();
                assert_eq!(
                    series.chart_type_params, params,
                    "opening settings keeps saved values"
                );
            }
        }
        let legacy: ChartTypeParams = serde_json::from_str(r#"{"tpo_row_ticks":2}"#).unwrap();
        let legacy = serde_json::to_value(legacy).unwrap();
        assert_eq!(legacy["footprint_statistics"], false);
        assert_eq!(legacy["footprint_row_ticks"], 0);
        assert_eq!(
            legacy["footprint_colors"],
            serde_json::json!([null, null, null, null])
        );
        for field in [
            "footprint_stacked_imbalances",
            "footprint_naked_poc",
            "footprint_delta_poc",
            "footprint_unfinished_auctions",
            "footprint_extreme_ratio",
            "footprint_pullback",
        ] {
            assert_eq!(legacy[field], false);
        }
        assert_eq!(legacy["profile"]["value_area"], 0.7);
        for field in [
            "developing_value_area",
            "extend_poc",
            "extend_vah",
            "extend_val",
        ] {
            assert_eq!(legacy["profile"][field], false);
        }
    }

    /// A zone chosen from the session zone list is listed once when the list is opened again,
    /// whether it is one of the listed zones or a saved zone outside them.
    #[cfg(all(feature = "ui", feature = "timezones"))]
    #[test]
    fn session_zone_choices_list_the_chosen_zone_once() {
        for zone in ["America/New_York", "Europe/Berlin"] {
            let mut series = crate::chart::series::SeriesSettings {
                chart_type: ChartType::SessionVolumeProfile,
                ..Default::default()
            };
            series.chart_type_params.profile.session =
                ProfileSession::Custom(CustomProfileSession {
                    start: chrono::NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                    end: chrono::NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                    zone: Some(crate::model::TimezoneId::named(zone).unwrap()),
                });
            let ctx = egui::Context::default();
            // Room for every entry: rows scrolled out of a shorter list are not painted.
            ctx.all_styles_mut(|style| style.spacing.combo_height = 4000.0);
            let mut frame = |events: Vec<egui::Event>| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 2400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| crate::ui::dialogs::series_settings::SymbolTab::show(ui, &mut series),
                );
                let found: Vec<egui::Pos2> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == zone => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                found
            };
            frame(vec![]);
            let pos = frame(vec![])[0];
            frame(vec![egui::Event::PointerMoved(pos)]);
            let button = |pressed| egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: Default::default(),
            };
            frame(vec![button(true)]);
            frame(vec![button(false)]);
            // The combo's own text and one entry in the opened list.
            let shown = frame(vec![]).len();
            assert_eq!(shown, 2, "{zone} is drawn {shown} times");
            let ProfileSession::Custom(custom) = &series.chart_type_params.profile.session else {
                panic!("the session stays custom");
            };
            assert_eq!(custom.zone.as_ref().map(|z| z.name()), Some(zone));
        }
    }

    #[test]
    fn saved_tpo_settings_apply_the_balance_window_and_keep_legacy_defaults() {
        use crate::model::{Bar, Timeframe, to_tpo_profiles};
        use chrono::{Duration, TimeZone, Utc};
        let start = Utc.with_ymd_and_hms(2024, 3, 5, 9, 30, 0).unwrap();
        let bars = [
            Bar::new(start, 100.0, 102.0, 100.0, 102.0, 1.0),
            Bar::new(
                start + Duration::minutes(30),
                101.0,
                104.0,
                101.0,
                104.0,
                1.0,
            ),
        ];
        for (saved, minutes, high, enabled, marked) in [
            (
                r#"{"tpo_initial_balance_minutes":30,"tpo_poor_highs_lows":true,"tpo_initial_balance_extensions":true}"#,
                30,
                102.0,
                true,
                false,
            ),
            (
                r#"{"tpo_period_minutes":30,"tpo_row_ticks":1}"#,
                60,
                104.0,
                false,
                false,
            ),
            (
                r#"{"tpo_single_prints":true,"tpo_midpoint":true}"#,
                60,
                104.0,
                false,
                true,
            ),
        ] {
            let params: ChartTypeParams = serde_json::from_str(saved).unwrap();
            let config = params.tpo_config(Some(1.0));
            assert_eq!(config.initial_balance_minutes, minutes);
            assert_eq!(config.show_single_prints, marked);
            assert_eq!(config.show_midpoint, marked);
            let profiles = to_tpo_profiles(&bars, &config, &Timeframe::Min30.into());
            assert_eq!(
                (
                    profiles[0].initial_balance_low,
                    profiles[0].initial_balance_high
                ),
                (100.0, high)
            );
            let saved = serde_json::to_value(&params).unwrap();
            assert_eq!(saved["tpo_initial_balance_minutes"], minutes);
            assert_eq!(saved["tpo_poor_highs_lows"], enabled);
            assert_eq!(saved["tpo_initial_balance_extensions"], enabled);
            assert_eq!(saved["tpo_single_prints"], marked);
            assert_eq!(saved["tpo_midpoint"], marked);
            assert_eq!(
                serde_json::from_value::<ChartTypeParams>(saved).unwrap(),
                params
            );
        }
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
