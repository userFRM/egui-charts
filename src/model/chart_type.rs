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
    /// One column per bar up to the price of `ChartTypeStyles::columns`, colored by close versus
    /// the previous close.
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
            ChartType::Columns => "Columns up to the selected price",
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
    /// Studies, drawings and markers are drawn at the element that contains their time, and
    /// volume is hidden.
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
#[non_exhaustive]
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
#[non_exhaustive]
pub enum FootprintColor {
    /// Volume relative to the largest row of the [`FootprintIntensity`] scale, by default this bar's.
    Volume,
    /// Delta as a fraction of the row's classified volume.
    Delta,
    /// Dominant bid or ask side, using the configured comparison.
    Imbalance,
    /// The neutral colour for every cell, whatever was traded.
    Fixed,
    /// The buy colour for a positive delta and the sell colour for a negative one, with the
    /// opacity growing with the delta's absolute size relative to the largest of the
    /// [`FootprintIntensity`] scale, by default the bar's. A zero delta takes the neutral colour
    /// at full opacity.
    DeltaOpacity,
    /// Sell and buy volume shaded separately, each in one of four strengths of its own colour by
    /// the quarter it falls in between the smallest and the largest volume of its side in the
    /// bar, or in the bars in view, or between zero and an entered volume, as
    /// [`FootprintIntensity`] chooses. A total-volume column instead uses one shade mixed from
    /// the neutral colour, by the quarter of the total-volume range; chosen side shades do not
    /// apply to it. The POC row takes the text colour, the value area is two
    /// dotted lines, and a side that dominates under the imbalance settings gets a strip along its outer edge.
    VolumeSteps,
    /// The cell of the side that traded more in its row, in the buy or sell colour at half
    /// opacity; the other cell, and every cell of a row whose sell and buy volumes are equal,
    /// stays unfilled. Unknown-side volume takes no part. A column with one cell per row fills
    /// its left half for sells and its right half for buys. Both numbers stay.
    LargerSide,
    /// Every sell cell in the sell colour and every buy cell in the buy colour at half opacity,
    /// whatever was traded. A column with one cell per row fills its left half in the sell colour
    /// and its right half in the buy colour.
    Side,
}

/// What footprint cell shades and number sizes are measured against: the shades of
/// [`FootprintColor::Volume`], [`FootprintColor::DeltaOpacity`] and
/// [`FootprintColor::VolumeSteps`], and each row's size within the font size range.
///
/// ```
/// use egui_charts::model::{ChartTypeParams, FootprintIntensity};
///
/// let mut params = ChartTypeParams::default();
/// params.footprint_intensity = FootprintIntensity::Volume(500.0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FootprintIntensity {
    /// Each bar against its own rows; its busiest row takes the full shade.
    #[default]
    Bar,
    /// Complete bars whose cells are in view share a scale over all their rows. Without such a
    /// bar, each bar uses its own rows. Vertical panning leaves the scale unchanged.
    Visible,
    /// All bars against this volume after row grouping. A row's volume or absolute delta at or
    /// above it takes the full shade and largest font; smaller rows take their share. Volume
    /// steps divide zero to this volume into quarters. Non-finite or non-positive values use
    /// each bar's own rows instead.
    Volume(f64),
}

/// How `studies::TradeProfile::nodes` (requires `studies`) chooses volume nodes.
///
/// The default compares each price row with its immediate neighbours, which is the most
/// sensitive setting. Raising `smoothing` or `prominence` leaves only the broader nodes.
///
/// ```
/// use egui_charts::data::NodeSensitivity;
///
/// let mut sensitivity = NodeSensitivity::default();
/// sensitivity.smoothing = 3;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct CustomProfileSession {
    /// Wall-clock time each session opens, in `zone`.
    pub start: chrono::NaiveTime,
    /// Wall-clock time each session closes, in `zone`.
    pub end: chrono::NaiveTime,
    /// Zone of `start` and `end`. `None` uses the instrument's exchange zone, or UTC without one.
    pub zone: Option<super::TimezoneId>,
}

impl CustomProfileSession {
    /// Sessions from `start` to `end` on the instrument's exchange clock, or in UTC without one:
    /// assign [`zone`](Self::zone) to read them in another zone.
    ///
    /// ```
    /// use chrono::NaiveTime;
    /// use egui_charts::model::CustomProfileSession;
    ///
    /// let open = NaiveTime::from_hms_opt(9, 30, 0).unwrap();
    /// let close = NaiveTime::from_hms_opt(16, 0, 0).unwrap();
    /// let session = CustomProfileSession::new(open, close);
    /// assert_eq!(session.zone, None);
    /// ```
    pub fn new(start: chrono::NaiveTime, end: chrono::NaiveTime) -> Self {
        Self {
            start,
            end,
            zone: None,
        }
    }
}

/// How custom sessions share the hours where they overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SessionOverlap {
    /// Cut at every session boundary, giving the shared hours a separate profile.
    #[default]
    OwnProfile,
    /// Keep shared bars in each session's profile.
    InBoth,
}

/// Up to five daily windows, each with a profile and period clock of its own.
/// Saved single windows restore as the only session, without clipping.
///
/// ```
/// use chrono::NaiveTime;
/// use egui_charts::model::{CustomProfileSession, CustomProfileSessions, ProfileSession, SessionOverlap};
/// let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
/// let mut sessions = CustomProfileSessions::new([
///     CustomProfileSession::new(at(8), at(12)),
///     CustomProfileSession::new(at(12), at(16)),
/// ]);
/// sessions.overlap = SessionOverlap::InBoth;
/// sessions.trading_hours_only = true;
/// let session = ProfileSession::Custom(sessions);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "SavedCustomSessions")]
#[non_exhaustive]
pub struct CustomProfileSessions {
    /// Windows in input order. Only the first [`Self::MAX`] are used; an empty list profiles none.
    pub sessions: Vec<CustomProfileSession>,
    /// Whether shared hours get their own profile or stay in both sessions.
    pub overlap: SessionOverlap,
    /// Clip to the instrument's regular hours, including its nontrading days. Without instrument
    /// hours, keep every bar. Periods count from the later of the custom and regular opens.
    pub trading_hours_only: bool,
}

impl CustomProfileSessions {
    /// Maximum number of custom sessions used each day.
    pub const MAX: usize = 5;

    /// Daily windows whose shared hours get a separate profile, without trading-hours clipping.
    pub fn new(sessions: impl IntoIterator<Item = CustomProfileSession>) -> Self {
        Self {
            sessions: sessions.into_iter().collect(),
            overlap: SessionOverlap::OwnProfile,
            trading_hours_only: false,
        }
    }
}

impl Default for CustomProfileSessions {
    fn default() -> Self {
        Self::new([CustomProfileSession::new(
            chrono::NaiveTime::MIN,
            chrono::NaiveTime::MIN,
        )])
    }
}

impl From<CustomProfileSession> for CustomProfileSessions {
    fn from(session: CustomProfileSession) -> Self {
        Self::new([session])
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SavedCustomSessions {
    Sessions {
        sessions: Vec<CustomProfileSession>,
        #[serde(default)]
        overlap: SessionOverlap,
        #[serde(default)]
        trading_hours_only: bool,
    },
    One(CustomProfileSession),
}

impl From<SavedCustomSessions> for CustomProfileSessions {
    fn from(saved: SavedCustomSessions) -> Self {
        match saved {
            SavedCustomSessions::Sessions {
                sessions,
                overlap,
                trading_hours_only,
            } => Self {
                sessions,
                overlap,
                trading_hours_only,
            },
            SavedCustomSessions::One(session) => session.into(),
        }
    }
}

/// A custom profile's bar range, period origin and first owning session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CustomPart {
    pub(crate) bars: std::ops::Range<usize>,
    pub(crate) open: chrono::DateTime<chrono::Utc>,
    pub(crate) date: chrono::NaiveDate,
    pub(crate) session: usize,
}

impl CustomProfileSessions {
    /// Fill unset zones with the instrument's exchange zone.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub(crate) fn with_zone(&self, zone: &super::TimezoneId) -> Self {
        let mut resolved = self.clone();
        for session in &mut resolved.sessions {
            session.zone.get_or_insert_with(|| zone.clone());
        }
        resolved
    }

    /// Divide `range` on each window's clock, then share or cut overlaps. With `hours_only` off
    /// and no clipping, retain each whole trading date, as the other session choices do.
    pub(crate) fn parts(
        &self,
        bars: &[super::Bar],
        range: std::ops::Range<usize>,
        interval: super::Timeframe,
        regular: Option<&super::BarClock>,
        hours_only: bool,
    ) -> Vec<CustomPart> {
        use super::SessionProvider;
        use chrono::Weekday::*;
        if range.is_empty() {
            return Vec::new();
        }
        let regular = regular.filter(|_| self.trading_hours_only);
        let mut parts = Vec::new();
        for (session, custom) in self.sessions.iter().take(Self::MAX).enumerate() {
            let zone = custom.zone.clone().unwrap_or_else(super::TimezoneId::utc);
            let window = super::SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri, Sat, Sun],
                custom.start,
                custom.end,
            );
            let clock = super::BarClock::new(interval).with_session(
                zone,
                super::Session::new([window]),
                false,
            );
            let provider = clock.sessions();
            let mut cuts = vec![range.start];
            for (i, _) in provider.session_breaks(&bars[range.clone()]) {
                cuts.push(range.start + i);
            }
            cuts.push(range.end);
            for day in cuts.windows(2) {
                let mut inside = (day[0]..day[1]).filter(|&i| {
                    provider.is_open(bars[i].time, true)
                        && regular.is_none_or(|r| r.is_open(bars[i].time))
                });
                let kept = if !hours_only && regular.is_none() {
                    day[0]..day[1]
                } else {
                    let Some(first) = inside.next() else { continue };
                    first..inside.next_back().unwrap_or(first) + 1
                };
                if kept.is_empty() {
                    continue;
                }
                let time = bars[kept.start].time;
                let mut open = provider.session_open(time, false).unwrap_or(time);
                if let Some(regular) = regular.and_then(|r| r.sessions().session_open(time, false))
                {
                    open = open.max(regular);
                }
                parts.push(CustomPart {
                    bars: kept,
                    open,
                    date: provider.trading_date(time),
                    session,
                });
            }
        }
        // Each window's days are disjoint, so no two parts share this key.
        parts.sort_unstable_by_key(|p| (p.bars.start, p.session));
        if self.overlap == SessionOverlap::InBoth {
            return parts;
        }
        let mut cuts = Vec::with_capacity(parts.len() * 2);
        for p in &parts {
            cuts.extend([p.bars.start, p.bars.end]);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut shared = Vec::with_capacity(cuts.len());
        for pair in cuts.windows(2) {
            // The latest open sets the origin and date, the first session breaking a tie.
            let mut piece: Option<CustomPart> = None;
            let mut first = usize::MAX;
            for p in &parts {
                if pair[0] < p.bars.start || p.bars.end < pair[1] {
                    continue;
                }
                first = first.min(p.session);
                let later = |q: &CustomPart| {
                    (p.open, std::cmp::Reverse(p.session)) > (q.open, std::cmp::Reverse(q.session))
                };
                if piece.as_ref().is_none_or(later) {
                    piece = Some(CustomPart {
                        bars: pair[0]..pair[1],
                        ..p.clone()
                    });
                }
            }
            if let Some(mut piece) = piece {
                piece.session = first;
                shared.push(piece);
            }
        }
        shared
    }
}

/// Which hours divide a session volume profile chart into sessions.
///
/// Only intraday charts are divided this way: daily and longer bars profile by trading date.
/// Every choice but [`Self::Chart`] profiles only the bars of its hours, each bar in the hours it
/// opens in: a profile takes a day's bars from its first in those hours to its last, so the bars
/// of a midday break inside them stay in it, and the bars before and after, such as the
/// post-market of a regular-hours profile, join no profile. Custom windows use separate clocks,
/// sharing their common bars according to [`CustomProfileSessions::overlap`].
///
/// ```
/// use egui_charts::model::{ProfileSession, TradeProfileSettings};
///
/// let mut settings = TradeProfileSettings::default();
/// settings.session = ProfileSession::Segments;
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProfileSession {
    /// The chart's own calendar: its instrument's hours, with extended and overnight hours as
    /// the chart shows them, or calendar days in the instrument's zone without hours.
    #[default]
    Chart,
    /// The instrument's regular windows only. The chart's calendar without instrument hours.
    Regular,
    /// The instrument's regular, extended and overnight windows. The chart's calendar without
    /// instrument hours.
    Extended,
    /// Up to five daily windows, optionally clipped to the instrument's regular hours.
    Custom(CustomProfileSessions),
    /// The instrument's pre-market hours alone, one profile per trading date: its extended hours
    /// that the date's regular hours follow. The chart's calendar without extended hours.
    PreMarket,
    /// The instrument's post-market hours alone, one profile per trading date: its extended hours
    /// after the date's regular hours. The chart's calendar without extended hours.
    PostMarket,
    /// One profile for each of the overnight, pre-market, regular and post-market hours of every
    /// trading date. The chart's calendar without extended or overnight hours.
    Segments,
}

impl ProfileSession {
    /// `clock` dividing intraday bars at the opens of the hours this choice selects, for an
    /// instrument trading `hours` in `zone`: a window opening every day, in its own zone or else
    /// `zone`, or `hours` with or without their extended windows. `None` for [`Self::Chart`],
    /// for regular or extended hours without `hours`, for pre-market or post-market without
    /// extended windows, and for segments without extended or overnight windows. Custom sessions
    /// also return `None`: [`CustomProfileSessions::parts`] divides them on separate clocks.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub(crate) fn session_clock(
        &self,
        clock: &super::BarClock,
        zone: &super::TimezoneId,
        hours: Option<&super::Session>,
    ) -> Option<super::BarClock> {
        let (zone, session, extended) = match self {
            Self::Chart | Self::Custom(_) => return None,
            Self::Regular | Self::Extended => (zone, hours?.clone(), *self == Self::Extended),
            Self::PreMarket | Self::PostMarket => {
                let hours = hours.filter(|hours| !hours.extended_windows.is_empty())?;
                (zone, hours.clone(), true)
            }
            Self::Segments => {
                let hours = hours.filter(|hours| {
                    !hours.extended_windows.is_empty() || !hours.overnight_windows.is_empty()
                })?;
                (zone, hours.clone(), true)
            }
        };
        Some(clock.clone().with_session(zone.clone(), session, extended))
    }

    /// The parts of the bars `range`, one session of `sessions`, that this choice profiles, each a
    /// profile of its own: the runs of pre-market, of regular and of post-market bars for
    /// [`Self::Segments`], only the pre-market or post-market runs for [`Self::PreMarket`] and
    /// [`Self::PostMarket`], and for the other choices the whole range, or with `hours_only` its
    /// bars from the first inside the hours to the last. A bar belongs to the hours it opens in.
    /// Bars of no hours are left out before, after and between parts; those inside a part, such
    /// as bars of a midday break, stay in it.
    pub(crate) fn profiled(
        &self,
        bars: &[super::Bar],
        range: std::ops::Range<usize>,
        sessions: &super::ExchangeSessionProvider,
        hours_only: bool,
    ) -> Vec<std::ops::Range<usize>> {
        debug_assert!(!matches!(self, Self::Custom(_)));
        use super::session::MarketSession;
        let segments = matches!(self, Self::PreMarket | Self::PostMarket | Self::Segments);
        if !segments && !hours_only {
            return vec![range];
        }
        let mut parts: Vec<(MarketSession, std::ops::Range<usize>)> = Vec::new();
        for (i, bar) in range.clone().zip(&bars[range]) {
            let hours = if segments {
                sessions
                    .market_session(bar.time)
                    .filter(|hours| *hours != MarketSession::Closed)
            } else {
                // The clock keeps only the windows this choice selects.
                sessions
                    .is_open(bar.time, true)
                    .then_some(MarketSession::Open)
            };
            let Some(hours) = hours else { continue };
            match parts.last_mut() {
                Some((last, part)) if *last == hours => part.end = i + 1,
                _ => parts.push((hours, i..i + 1)),
            }
        }
        parts
            .into_iter()
            .filter(|(hours, _)| match self {
                Self::PreMarket => *hours == MarketSession::PreMarket,
                Self::PostMarket => *hours == MarketSession::PostMarket,
                _ => true,
            })
            .map(|(_, part)| part)
            .collect()
    }
}

/// How a session volume profile groups executions into price rows, in instrument ticks.
///
/// Rows are never finer than the feed's own rows, and align to multiples of their height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProfileRows {
    /// The rows the feed groups executions into.
    #[default]
    Feed,
    /// About this many rows between each session's lowest and highest traded rows. That span
    /// of feed rows over this count, rounded down and rounded up, gives two whole row heights;
    /// the one whose rows, laid on multiples of their height, come closer to this count is
    /// used, the taller on a tie. Zero counts as one. The volume profile studies lay Row count
    /// on whole ticks by the same rule.
    Count(u32),
    /// At least this many instrument ticks per row.
    Ticks(u32),
}

/// Display settings of the chart's execution-based session profiles.
/// Defaults retain the stacked session histogram, with optional annotations hidden.
///
/// ```
/// use egui_charts::model::TradeProfileSettings;
///
/// let mut settings = TradeProfileSettings::default();
/// settings.developing_poc = true;
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TradeProfileSettings {
    /// Hours that divide the chart into sessions.
    pub session: ProfileSession,
    /// Price row grouping; new charts start at about 24 rows a session.
    pub rows: ProfileRows,
    /// Bid/ask stacks, total volume or signed delta; `DeltaVolume` also draws signed delta.
    pub mode: FootprintMode,
    /// Trace the total-volume POC after each complete execution bar.
    pub developing_poc: bool,
    /// Trace the outer value area edges after each complete execution bar; hidden without
    /// `value_area`. Uses that share and the same row selection as the final profile.
    pub developing_value_area: bool,
    /// Extend a completed session's POC to the first later bar with volume whose low-to-high
    /// range reaches its drawn price. Untouched, it runs to the viewport's right edge.
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
    /// Draw guides at the outer edges of the value area: the edges `value_area_high` and
    /// `value_area_low` select.
    pub value_area_guides: bool,
    /// Draw the POC line across each session. `extend_poc` extends it either way.
    pub poc: bool,
    /// Colour of the POC line and its extension; `None` follows the chart theme.
    pub poc_color: Option<egui::Color32>,
    /// With `value_area_guides`, draw the guide at the value area high.
    pub value_area_high: bool,
    /// Colour of the value area high guide and its extension; `None` follows the chart theme.
    pub value_area_high_color: Option<egui::Color32>,
    /// With `value_area_guides`, draw the guide at the value area low.
    pub value_area_low: bool,
    /// Colour of the value area low guide and its extension; `None` follows the chart theme.
    pub value_area_low_color: Option<egui::Color32>,
    /// Colour of up volume outside the value area, painted with its own opacity; `None` follows
    /// the chart theme. In delta mode, rows of positive delta take the up colours.
    pub up_volume_color: Option<egui::Color32>,
    /// Colour of down volume outside the value area; `None` follows the chart theme.
    pub down_volume_color: Option<egui::Color32>,
    /// Colour of up volume inside the value area; `None` follows the chart theme.
    pub value_area_up_color: Option<egui::Color32>,
    /// Colour of down volume inside the value area; `None` follows the chart theme.
    pub value_area_down_color: Option<egui::Color32>,
    /// Grow each histogram from the right edge of its session instead of the left. The edge a
    /// histogram grows from stays in view while its session runs past the viewport.
    pub right_aligned: bool,
    /// Share of the session's width that the longest row spans, from 0.01 to 1. Values outside
    /// are clamped, and NaN spans the whole session.
    pub width: f64,
    /// Write each row's volume beside its bar when the row is at least as tall as the text:
    /// `bid × ask` for bid/ask stacks, the total for total volume and the signed delta for delta,
    /// abbreviated as footprint numbers are.
    pub values: bool,
    /// Colour of the row values; `None` follows the chart theme.
    pub values_color: Option<egui::Color32>,
    /// Draw the box behind each session.
    pub session_box: bool,
    /// Colour of the session box, painted with its own opacity; `None` follows the chart theme.
    pub session_box_color: Option<egui::Color32>,
}

impl Default for TradeProfileSettings {
    fn default() -> Self {
        Self {
            session: ProfileSession::Chart,
            rows: ProfileRows::Count(24),
            mode: FootprintMode::BidAsk,
            developing_poc: false,
            developing_value_area: false,
            extend_poc: false,
            extend_vah: false,
            extend_val: false,
            nodes: None,
            value_area: Some(0.7),
            value_area_guides: false,
            poc: true,
            poc_color: None,
            value_area_high: true,
            value_area_high_color: None,
            value_area_low: true,
            value_area_low_color: None,
            up_volume_color: None,
            down_volume_color: None,
            value_area_up_color: None,
            value_area_down_color: None,
            right_aligned: false,
            width: 1.0,
            values: false,
            values_color: None,
            session_box: true,
            session_box_color: None,
        }
    }
}

/// A row of the per-bar execution statistics table.
///
/// ```
/// use egui_charts::model::FootprintStatistic;
///
/// let rows = [
///     FootprintStatistic::Volume,
///     FootprintStatistic::OpenInterestHigh,
///     FootprintStatistic::OpenInterestLow,
/// ];
/// assert_eq!(rows.map(FootprintStatistic::label), ["Vol", "OI high", "OI low"]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// Sell volume divided by total volume, in percent.
    BidPercent,
    /// Buy volume divided by total volume, in percent.
    AskPercent,
    /// The bar's high minus its low.
    Range,
    /// The lowest price of the bar's highest-volume row, as the footprint groups its rows.
    PointOfControl,
    /// The change of total volume from the previous bar with executions, in percent; the first
    /// such bar has none.
    VolumeChange,
    /// The highest open interest of the bar: the value in effect at its open and every value
    /// published before its close, from the metric `open_interest` of the chart symbol.
    OpenInterestHigh,
    /// The lowest open interest of the bar, from the same values as
    /// [`OpenInterestHigh`](Self::OpenInterestHigh).
    OpenInterestLow,
}

impl FootprintStatistic {
    /// All available rows in their display order.
    pub const ALL: [Self; 18] = [
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
        Self::BidPercent,
        Self::AskPercent,
        Self::Range,
        Self::PointOfControl,
        Self::VolumeChange,
        Self::OpenInterestHigh,
        Self::OpenInterestLow,
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
            Self::BidPercent => "Bid %",
            Self::AskPercent => "Ask %",
            Self::Range => "Range",
            Self::PointOfControl => "POC",
            Self::VolumeChange => "Vol change %",
            Self::OpenInterestHigh => "OI high",
            Self::OpenInterestLow => "OI low",
        }
    }
}

/// Where the Baseline chart draws its base line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// Footprint: what cell shades and number sizes are measured against.
    pub footprint_intensity: FootprintIntensity,
    /// Footprint: buy, sell, text and neutral colours. None follows the chart theme.
    pub footprint_colors: [Option<egui::Color32>; 4],
    /// Footprint: buy and sell colours of imbalance strips, imbalance colouring and stacked
    /// imbalance zones. None follows the buy or sell colour.
    pub footprint_imbalance_colors: [Option<egui::Color32>; 2],
    /// Footprint: the four shades of the buy cells and of the sell cells under
    /// [`FootprintColor::VolumeSteps`], weakest first. None mixes the shade from the buy or sell
    /// colour and the chart background, as when the field is missing from a saved chart.
    /// Total-volume cells mix their shade from the neutral colour and ignore these side shades.
    pub footprint_step_colors: [[Option<egui::Color32>; 4]; 2],
    /// Footprint: instrument ticks per row; zero groups automatically with zoom. Rows sized by
    /// `footprint_row_atr` use it only until the chart holds a complete bar.
    pub footprint_row_ticks: u32,
    /// Footprint: size rows from the average true range of the last `footprint_row_atr_length`
    /// complete bars the chart holds, not the bars in view, so panning, zooming and loading
    /// older history keep them. A row is a fifth of the average, in whole instrument ticks and
    /// at least one. The newest bar, which may still be forming, is left out.
    pub footprint_row_atr: bool,
    /// Footprint: length of the average true range that sizes rows, 1 to 200; with fewer
    /// complete bars the average takes as many as there are.
    pub footprint_row_atr_length: u32,
    /// Footprint: minimum height of rows grouped by zoom, in points within 10 to 40.
    pub footprint_auto_row_height: f32,
    /// Footprint: rows shorter than this, in points within 10 to 40, lose their numbers;
    /// a column without numbers shows the overview.
    pub footprint_min_text_height: f32,
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
    /// Footprint: end stacked imbalance zones only at a bar closing beyond their far edge,
    /// counting and marking the bars that trade into them without such a close, rather than at
    /// their first revisit.
    pub footprint_zones_until_broken: bool,
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
    /// Footprint: later bars with complete executions an unrevisited POC, stacked imbalance
    /// zone or unfinished high or low extends over before expiry. `None` extends until a
    /// revisit or the last bar; `Some(0)` expires at its own bar.
    pub footprint_extend_bars: Option<usize>,
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
    /// Footprint: restart the cumulative delta row at the first bar of each session, rather than
    /// summing every complete bar.
    pub footprint_session_cumulative_delta: bool,
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
    /// Point and Figure: with a reversal of one box, draw a reversal that follows a column of one
    /// symbol in that column, which then holds both symbols. No effect at larger reversals.
    pub pnf_one_step_back: bool,
    /// Renko: label the last real price on the price scale; `false` labels the close of the
    /// newest brick.
    pub renko_real_price: bool,
    /// Kagi: label the last real price on the price scale; `false` labels the end of the newest
    /// line.
    pub kagi_real_price: bool,
    /// Line Break: label the last real price on the price scale; `false` labels the close of
    /// the newest line.
    pub line_break_real_price: bool,
    /// Point and Figure: label the last real price on the price scale; `false` labels the end
    /// of the newest column.
    pub pnf_real_price: bool,
    /// Baseline: where the base line is drawn.
    pub baseline: BaselineLevel,
    /// Columns: the price the columns rise above or fall below; `None` draws them up from the
    /// pane bottom.
    pub columns_base: Option<f64>,
    /// TPO: minutes per period, raised to the bar interval when shorter.
    pub tpo_period_minutes: u32,
    /// TPO: minutes from the session open included in the initial balance, 60 by default.
    pub tpo_initial_balance_minutes: u32,
    /// TPO: saved by earlier releases for poor highs and lows together; when on, both are marked.
    /// The settings write `tpo_poor_high` and `tpo_poor_low` and turn it off.
    pub tpo_poor_highs_lows: bool,
    /// TPO: mark poor highs. Off by default.
    pub tpo_poor_high: bool,
    /// TPO: mark poor lows. Off by default.
    pub tpo_poor_low: bool,
    /// TPO: extend poor highs and lows until a later bar trades through them. Off by default.
    pub tpo_extend_poor: bool,
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
    /// TPO: caption and mark the middle of each profile's range. Off for a new chart; settings
    /// saved without the field show it, as every profile did before the setting.
    #[serde(default = "tpo_midpoint_drawn")]
    pub tpo_midpoint: bool,
    /// TPO: paint each period's letter inside its block. On for a new chart; settings saved
    /// without the field keep blocks.
    // Settings saved before the field existed were drawn as blocks, so a missing field reads false.
    #[serde(default)]
    pub tpo_letters: bool,
    /// TPO: draw the session statistics table below each profile.
    pub tpo_statistics: bool,
    /// TPO: colour blocks along `tpo_block_colors`; off uses `tpo_block_color`. On by default.
    pub tpo_colored_blocks: bool,
    /// TPO: the four colours of [`TPOConfig::period_colors`](super::TPOConfig::period_colors).
    pub tpo_block_colors: [egui::Color32; 4],
    /// TPO: colour of every block with `tpo_colored_blocks` off.
    pub tpo_block_color: egui::Color32,
    /// TPO: opacity of blocks outside the value area, in percent, clamped to 100. Default 45.
    pub tpo_outside_value_area_opacity: u8,
    /// TPO: length of a profile, as in [`TPOConfig::profile_period`](super::TPOConfig::profile_period).
    pub tpo_profile_period: super::TPOProfilePeriod,
    /// TPO: each period in its own column, as in [`TPOConfig::split_periods`](super::TPOConfig::split_periods).
    pub tpo_split_periods: bool,
    /// TPO: draw the initial balance marker. On by default; its period and extensions apply either way.
    pub tpo_initial_balance: bool,
    /// TPO: captions, as in [`TPOConfig::labels`](super::TPOConfig::labels). Settings
    /// saved without this field keep the point of control, value area and midpoint captions.
    pub tpo_labels: super::TPOLabels,
    /// TPO: tag captioned levels of the profile holding the last bar on the price scale,
    /// in their caption colours, while the profile is drawn. Single prints take no tag.
    /// Off by default.
    pub tpo_level_tags: bool,
    /// TPO: colour, underline and caption the point of control. On by default.
    pub tpo_poc: bool,
    /// TPO: draw and caption the value area high and low and dim blocks outside it. On by default.
    pub tpo_value_area_levels: bool,
    /// TPO: mark each profile's opening price. On by default.
    pub tpo_open_price: bool,
    /// TPO: mark each profile's closing price. Off by default.
    pub tpo_close_price: bool,
    /// TPO: draw a volume profile beside each profile. Off by default.
    pub tpo_volume_profile: bool,
    /// TPO: sessions combined into one profile, as in
    /// [`TPOConfig::composites`](super::TPOConfig::composites).
    pub tpo_composites: Vec<std::ops::RangeInclusive<chrono::DateTime<chrono::Utc>>>,
    /// TPO: instants that divide a session's profile, as in
    /// [`TPOConfig::splits`](super::TPOConfig::splits).
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use egui_charts::model::ChartTypeParams;
    ///
    /// let split = Utc.with_ymd_and_hms(2024, 3, 5, 15, 0, 0).unwrap();
    /// let params = ChartTypeParams {
    ///     tpo_splits: vec![split],
    ///     ..Default::default()
    /// };
    /// assert_eq!(params.tpo_config(None).splits, [split]);
    /// ```
    pub tpo_splits: Vec<chrono::DateTime<chrono::Utc>>,
}

/// What a missing `tpo_midpoint` reads: every profile drew its midpoint before it had a setting.
fn tpo_midpoint_drawn() -> bool {
    true
}

impl Default for ChartTypeParams {
    fn default() -> Self {
        Self {
            footprint_modes: vec![FootprintMode::Ladder],
            footprint_profile: false,
            footprint_color_by: FootprintColor::VolumeSteps,
            footprint_intensity: FootprintIntensity::Bar,
            footprint_colors: [None; 4],
            footprint_imbalance_colors: [None; 2],
            footprint_step_colors: [[None; 4]; 2],
            footprint_row_ticks: 0,
            footprint_row_atr: false,
            footprint_row_atr_length: 14,
            footprint_auto_row_height: 16.0,
            footprint_min_text_height: 12.0,
            footprint_imbalance: true,
            footprint_poc: true,
            footprint_value_area: Some(0.7),
            footprint_imbalance_ratio: 3.0,
            footprint_imbalance_min_volume: 0.0,
            footprint_diagonal: true,
            footprint_stacked_imbalances: false,
            footprint_stack: 3,
            footprint_zones_until_broken: false,
            footprint_naked_poc: false,
            footprint_delta_poc: false,
            footprint_unfinished_auctions: false,
            footprint_cluster_share: None,
            footprint_single_print_max: None,
            footprint_delta_percent: None,
            footprint_ratio: None,
            footprint_extend_bars: None,
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
            footprint_session_cumulative_delta: false,
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
            pnf_one_step_back: false,
            renko_real_price: true,
            kagi_real_price: true,
            line_break_real_price: true,
            pnf_real_price: true,
            baseline: BaselineLevel::PercentOfPane(50.0),
            columns_base: None,
            tpo_period_minutes: 30,
            tpo_initial_balance_minutes: 60,
            tpo_poor_highs_lows: false,
            tpo_poor_high: false,
            tpo_poor_low: false,
            tpo_extend_poor: false,
            tpo_initial_balance_extensions: false,
            tpo_row_ticks: 0,
            tpo_value_area: 70.0,
            tpo_split_sessions: false,
            tpo_single_prints: false,
            tpo_midpoint: false,
            tpo_letters: true,
            tpo_statistics: false,
            tpo_colored_blocks: true,
            tpo_block_colors: [
                egui::Color32::from_rgb(214, 56, 100),
                egui::Color32::from_rgb(90, 197, 97),
                egui::Color32::from_rgb(84, 185, 209),
                egui::Color32::from_rgb(93, 36, 245),
            ],
            tpo_block_color: crate::tokens::DESIGN_TOKENS.semantic.tpo.letter_default,
            tpo_outside_value_area_opacity: 45,
            tpo_profile_period: super::TPOProfilePeriod::Session,
            tpo_split_periods: false,
            tpo_initial_balance: true,
            tpo_labels: super::TPOLabels::default(),
            tpo_level_tags: false,
            tpo_poc: true,
            tpo_value_area_levels: true,
            tpo_open_price: true,
            tpo_close_price: false,
            tpo_volume_profile: false,
            tpo_composites: Vec::new(),
            tpo_splits: Vec::new(),
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
            show_initial_balance: self.tpo_initial_balance,
            labels: self.tpo_labels,
            show_poc: self.tpo_poc,
            show_value_area: self.tpo_value_area_levels,
            show_opening_range: self.tpo_open_price,
            show_closing_price: self.tpo_close_price,
            show_volume_profile: self.tpo_volume_profile,
            show_poor_high: self.tpo_poor_high || self.tpo_poor_highs_lows,
            show_poor_low: self.tpo_poor_low || self.tpo_poor_highs_lows,
            extend_poor_extremes: self.tpo_extend_poor,
            show_initial_balance_extensions: self.tpo_initial_balance_extensions,
            value_area_pct: self.tpo_value_area / 100.0,
            split_sessions: self.tpo_split_sessions,
            profile_period: self.tpo_profile_period,
            split_periods: self.tpo_split_periods,
            show_single_prints: self.tpo_single_prints,
            show_midpoint: self.tpo_midpoint,
            show_statistics: self.tpo_statistics,
            color_mode: if self.tpo_colored_blocks {
                super::TPOColorMode::ByPeriod
            } else {
                super::TPOColorMode::Solid
            },
            period_colors: self.tpo_block_colors,
            solid_color: self.tpo_block_color,
            outside_value_area_opacity: f32::from(self.tpo_outside_value_area_opacity.min(100))
                / 100.0,
            display_mode: if self.tpo_letters {
                super::TPODisplayMode::Both
            } else {
                super::TPODisplayMode::Blocks
            },
            composites: self.tpo_composites.clone(),
            splits: self.tpo_splits.clone(),
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

    /// Whether the price scale of `chart_type` labels the close of its newest element instead
    /// of the last real price.
    #[cfg(feature = "widget")]
    pub(crate) fn labels_element_price(&self, chart_type: super::ChartType) -> bool {
        use super::ChartType::{Kagi, LineBreak, PointAndFigure, Renko};
        match chart_type {
            Renko => !self.renko_real_price,
            Kagi => !self.kagi_real_price,
            LineBreak => !self.line_break_real_price,
            PointAndFigure => !self.pnf_real_price,
            _ => false,
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

    /// Sizes in Renko, Kagi, Range, Point and Figure order.
    #[cfg(feature = "widget")]
    pub(crate) fn sizes_mut(&mut self) -> [&mut f64; 4] {
        [
            &mut self.renko_brick_size,
            &mut self.kagi_reversal,
            &mut self.range_size,
            &mut self.pnf_box_size,
        ]
    }

    /// Resolves the pending sizes from `bars`. A size stays pending while the bars give it no
    /// positive value, too few bars for its method included, and once they give one it is kept
    /// until armed again, through prepends and live corrections.
    #[cfg(feature = "widget")]
    pub(crate) fn resolve_sizes(&mut self, bars: &[super::Bar], pending: &mut [bool; 4]) {
        let methods = self.size_methods();
        for ((method, size), pending) in methods.into_iter().zip(self.sizes_mut()).zip(pending) {
            if *pending {
                let resolved = method.resolve(*size, bars);
                *size = resolved.unwrap_or(0.0);
                *pending = resolved.is_none();
            }
        }
    }

    /// Instrument ticks per footprint row on `bars` with instrument ticks of `tick`: from the
    /// average true range of their last complete bars when `footprint_row_atr` chooses it and
    /// there is one, otherwise `footprint_row_ticks`.
    #[cfg(feature = "widget")]
    pub(crate) fn footprint_row_size(&self, bars: &[super::Bar], tick: f64) -> u32 {
        if !self.footprint_row_atr {
            return self.footprint_row_ticks;
        }
        // The newest bar may still be forming. Of the bars before it only the last `length`
        // count, so older history that a pan loads leaves the rows as they are. Whitespace bars
        // have no range, and `BoxSizeMethod::resolve` skips them too.
        let length = self.footprint_row_atr_length.clamp(1, 200) as usize;
        let mut loaded = (0..bars.len())
            .rev()
            .filter(|&at| !bars[at].is_whitespace());
        let newest = loaded.next().unwrap_or(0);
        let complete = loaded.take(length);
        let first = complete.clone().last().unwrap_or(newest);
        let count = complete.count();
        let average = super::BoxSizeMethod::Atr {
            period: count,
            multiplier: 0.2,
        };
        match average
            .resolve(0.0, &bars[first..newest])
            .map(|size| size / tick)
            .filter(|ticks| ticks.is_finite())
        {
            Some(ticks) => ticks.round().clamp(1.0, 1_000_000.0) as u32,
            // The chart holds valid bars only, so complete bars without a size never moved.
            None if count > 0 => 1,
            None => self.footprint_row_ticks,
        }
    }

    /// Why `chart_type` cannot be drawn with these parameters, if it cannot. A size derived from
    /// history is judged by its method, since it is 0 while it waits for history.
    #[cfg(feature = "widget")]
    pub(crate) fn invalid(&self, chart_type: ChartType) -> Option<&'static str> {
        use super::BoxSizeMethod::{Atr, Fixed, Percentage};
        let positive = |n: f64| n > 0.0 && n.is_finite();
        let usable = |method, size| match method {
            Fixed => positive(size),
            Atr { period, multiplier } => period > 0 && positive(multiplier),
            Percentage(percent) => positive(percent),
        };
        match chart_type {
            ChartType::Renko if !usable(self.renko_size_method, self.renko_brick_size) => {
                Some("Renko brick size must be a positive number")
            }
            ChartType::Kagi if !usable(self.kagi_size_method, self.kagi_reversal) => {
                Some("Kagi reversal amount must be a positive number")
            }
            ChartType::Range if !usable(self.range_size_method, self.range_size) => {
                Some("Range size must be a positive number")
            }
            ChartType::PointAndFigure
                if !usable(self.pnf_size_method, self.pnf_box_size) || self.pnf_reversal == 0 =>
            {
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

    /// The bars of history the size of `chart_type` needs, while its method derives it from
    /// history and it is not yet a positive size.
    #[cfg(feature = "widget")]
    pub(crate) fn bars_needed(&self, chart_type: ChartType) -> Option<usize> {
        let (method, size) = match chart_type {
            ChartType::Renko => (self.renko_size_method, self.renko_brick_size),
            ChartType::Kagi => (self.kagi_size_method, self.kagi_reversal),
            ChartType::Range => (self.range_size_method, self.range_size),
            ChartType::PointAndFigure => (self.pnf_size_method, self.pnf_box_size),
            _ => return None,
        };
        match method {
            _ if size > 0.0 && size.is_finite() => None,
            super::BoxSizeMethod::Atr { period, .. } => Some(period),
            super::BoxSizeMethod::Percentage(_) => Some(1),
            super::BoxSizeMethod::Fixed => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_execution_settings_keep_choices_and_legacy_defaults() {
        let profile = serde_json::json!({"mode": "Volume", "value_area": 0.5, "developing_poc": true,
                "developing_value_area": true, "extend_poc": true, "extend_vah": true,
                "extend_val": true, "value_area_guides": true, "poc": false,
                "poc_color": [12, 34, 56, 255], "value_area_high": false,
                "value_area_high_color": [0, 200, 0, 255], "value_area_low": false,
                "value_area_low_color": [0, 0, 100, 128], "up_volume_color": [60, 0, 0, 60],
                "down_volume_color": [0, 60, 0, 60], "value_area_up_color": [0, 0, 120, 120],
                "value_area_down_color": [120, 120, 0, 120],
                "right_aligned": true, "width": 0.5, "values": true,
                "values_color": [1, 2, 3, 255], "session_box": false,
                "session_box_color": [4, 5, 6, 128],
                "nodes": {"smoothing": 2, "prominence": 0.1}});
        let mut settings = serde_json::json!({
            "footprint_modes": ["Delta", "BidAskTrades", "Trades", "AverageSize"], "footprint_row_ticks": 2,
            "footprint_color_by": "LargerSide",
            "footprint_step_colors": [[null, null, null, [1, 2, 3, 255]], [[4, 5, 6, 255], null, null, null]],
            "footprint_intensity": {"Volume": 2500.0},
            "footprint_extend_bars": 8,
            "footprint_auto_row_height": 24.0, "footprint_min_text_height": 10.0,
            "footprint_row_atr": true, "footprint_row_atr_length": 30,
            "footprint_value_area": 0.010003969988090037,
            "footprint_statistics": true,
            "footprint_zones_until_broken": true,
            "footprint_statistics_rows": ["Volume", "Delta", "Trades", "AverageSize", "PointOfControl", "VolumeChange"],
            "footprint_session_cumulative_delta": true,
            "footprint_colors": [[12, 34, 56, 255], null, null, null],
            "footprint_imbalance_colors": [[200, 30, 220, 255], null],
            "tpo_letters": false,
            "tpo_colored_blocks": false,
            "tpo_block_colors": [[255,0,0,255],[0,160,0,255],[0,160,0,255],[128,128,128,255]],
            "tpo_block_color": [12,34,56,255],
            "tpo_outside_value_area_opacity": 80,
            "tpo_profile_period": "Week",
            "tpo_split_periods": true,
            "tpo_initial_balance": false,
            "tpo_poc": false,
            "tpo_value_area_levels": false,
            "tpo_open_price": false,
            "tpo_close_price": true,
            "tpo_poor_high": true,
            "tpo_poor_low": false,
            "tpo_extend_poor": true,
            "tpo_volume_profile": true,
            "profile": profile
        });
        settings["tpo_labels"] = serde_json::json!({"poc": false, "value_area_high": true, "value_area_low": false,
                "midpoint": true, "initial_balance_high": true, "initial_balance_low": false,
                "opening_price": true, "closing_price": true, "poor_high": false, "poor_low": true,
                "single_prints": true, "prices": true});
        settings["tpo_level_tags"] = true.into();
        assert_eq!(TradeProfileSettings::default().rows, ProfileRows::Count(24));
        for (saved, expected) in [
            (r#"{}"#, ProfileRows::Count(24)),
            (r#"{"rows":"Feed"}"#, ProfileRows::Feed),
            (r#"{"rows":{"Ticks":4}}"#, ProfileRows::Ticks(4)),
        ] {
            let restored: TradeProfileSettings = serde_json::from_str(saved).unwrap();
            assert_eq!(restored.rows, expected);
        }
        let params: ChartTypeParams = serde_json::from_value(settings.clone()).unwrap();
        assert_eq!(params.profile.rows, ProfileRows::Count(24));
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
            for chart_type in [
                ChartType::VolumeFootprint,
                ChartType::SessionVolumeProfile,
                ChartType::TimePriceOpportunity,
            ] {
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
            let mut chart = crate::widget::Chart::new(crate::model::BarData::new());
            chart.set_chart_type(ChartType::TimePriceOpportunity);
            chart.user_action(|chart| {
                let mut series = chart.series_settings();
                series.chart_type_params = params.clone();
                chart.apply_series_settings(&series);
            });
            let applied = chart.chart_type_params().clone();
            let saved = serde_json::to_value(&applied).unwrap();
            for (name, expected) in settings.as_object().unwrap() {
                if name.starts_with("tpo_") {
                    assert_eq!(&saved[name], expected);
                }
            }
            assert!(chart.undo());
            assert_eq!(chart.chart_type_params(), &ChartTypeParams::default());
            assert!(chart.redo());
            assert_eq!(chart.chart_type_params(), &applied);
            let layout = crate::widget::SavedLayout::new(
                crate::widget::ChartLayoutMode::Single,
                vec![chart.snapshot().unwrap()],
            );
            let saved = crate::widget::SavedLayout::from_json(&layout.to_json().unwrap()).unwrap();
            let mut restored = crate::widget::Chart::new(crate::model::BarData::new());
            restored.restore(&saved.charts[0]).unwrap();
            assert_eq!(restored.chart_type_params(), &applied);
            let mut legacy: serde_json::Value =
                serde_json::from_str(&layout.to_json().unwrap()).unwrap();
            for path in [
                "/charts/0/chart_type_params",
                "/charts/0/settings/chart_type_params",
            ] {
                let fields = legacy.pointer_mut(path).unwrap().as_object_mut().unwrap();
                for name in [
                    "footprint_intensity",
                    "footprint_auto_row_height",
                    "footprint_min_text_height",
                    "footprint_extend_bars",
                    "footprint_step_colors",
                ] {
                    assert!(fields.remove(name).is_some(), "{path}: {name}");
                }
            }
            let mut expected = applied.clone();
            expected.footprint_intensity = FootprintIntensity::Bar;
            expected.footprint_auto_row_height = 16.0;
            expected.footprint_min_text_height = 12.0;
            expected.footprint_extend_bars = None;
            expected.footprint_step_colors = [[None; 4]; 2];
            // A sparse layout written and restored on main before these settings existed.
            let older = r#"{"charts":[{"bar_spacing":6.0,"chart_type":"VolumeFootprint","chart_type_params":{"footprint_color_by":"VolumeSteps","footprint_modes":["Ladder"],"footprint_naked_poc":true,"footprint_row_atr":false,"footprint_row_ticks":0},"drawings":{"drawings":[],"metadata":{"app_version":null,"created_at":1795116037269,"modified_at":1795116037269},"saved_by":17567378796323732970,"symbol":"","timeframe":null,"version":10},"extended_hours":false,"interval":null,"main_series_visible":true,"panes":[{"axes":{"hide_unused":true,"side":null},"collapsed":null,"main":true,"members":[0],"preserve":false,"scale":null,"scales":[],"stretch":3.0}],"right_offset":2.5,"settings":{"alerts":{"alert_color":[255,152,0,255],"close_toasts":true,"desktop_notifications":false,"notification_seconds":20,"play_sound":false,"show_active_only":true,"show_alert_labels":true,"show_alerts":true,"sound":"","sound_volume":1.0},"btns":{"navigation":"Hidden","pane":"VisibleOnMouseOver"},"candle_colors":{"body_down":[242,54,69,255],"body_up":[8,153,129,255],"border_down":[242,54,69,255],"border_up":[8,153,129,255],"color_by_previous_close":false,"show_body":true,"show_border":false,"show_wick":true,"wick_down":[242,54,69,255],"wick_up":[8,153,129,255]},"chart_basic_styles":{"background_color":[0,0,0,255],"background_gradient_bottom":[0,0,0,255],"background_gradient_top":[0,0,0,255],"background_type":"Solid"},"crosshair":{"color":[64,254,0,128],"line_style":"Dashed","line_width":1.0},"events":{"show_dividends":true,"show_earnings":false,"show_earnings_breaks":false,"show_economic_events":false,"show_ideas":true,"show_news":true,"show_news_notifications":false,"show_splits":true},"grid_lines":{"horizontal_color":[34,34,34,255],"mode":"Both","vertical_color":[34,34,34,255]},"margins":{"bottom_percent":10.0,"right_bars":2.5,"top_percent":10.0},"precision":"Default","scales_and_lines":{"allow_label_overlap":false,"auto_scale":true,"axis_layout":{"hide_unused":true,"side":null},"currency_unit_on_hover":false,"date_format":"Default","extended_session_price_color":[251,142,1,255],"indexed_to_100":false,"invert_scale":false,"lock_price_to_bar_ratio":false,"log_scale":false,"percentage_scale":false,"prev_close_line":{"color":null,"style":"Dotted","width":1.0},"price_line_source":"LastBar","price_line_style":"Dashed","price_line_width":1.0,"price_scale_left":false,"price_to_bar_ratio":1.0,"show_bid_ask":false,"show_bid_ask_lines":null,"show_countdown":true,"show_currency":true,"show_extended_session_price":true,"show_high_low":false,"show_high_low_lines":null,"show_last_price_change":false,"show_last_price_label":true,"show_plus_button":true,"show_prev_close":false,"show_prev_close_label":false,"show_price_line":true,"show_scale_mode_buttons":true,"show_study_names":false,"show_study_values":true,"show_symbol_name":false,"show_unit":true,"study_value_color":null,"symbol_price_color":null,"thousands_sep":"Comma","use_24_hour":true,"weekday_names":"Default"},"scales_appearance":{"font_family":"Proportional","font_size":12,"lines_color":[0,0,0,0],"text_color":[255,255,255,255]},"status_line":{"indicator_background_opacity":60,"show_change":true,"show_change_percent":true,"show_indicator_inputs":true,"show_indicator_titles":true,"show_indicator_values":true,"show_last_day_change":false,"show_logo":true,"show_market_status":true,"show_ohlc":true,"show_quote_boxes":true,"show_symbol":true,"show_volume":true,"title_as_description":true},"timezone":"UTC","trading":{"bid_ask_labels":false,"bracket_pnl":true,"bracket_pnl_unit":"Money","execution_sound":"","execution_sound_volume":1.0,"extend_lines":true,"include_in_image":false,"instant_orders":false,"label_alignment":"Right","notifications":"All","play_execution_sound":false,"position_pnl":true,"position_pnl_unit":"Money","reverse_button":true,"show_execution_labels":true,"show_executions":true,"show_orders":true,"show_positions":true},"watermark":{"color":[34,34,34,255],"mode":"ReplayMode"}},"studies":[],"symbol":""}],"mode":"Single","version":3}"#;
            let mut older_expected = ChartTypeParams {
                footprint_naked_poc: true,
                tpo_midpoint: true,
                tpo_letters: false,
                ..Default::default()
            };
            // Omitted style switches keep the defaults of documents saved before they existed.
            older_expected.styles.heikin.real_price_only = true;
            older_expected.styles.high_low.labels_visible = false;
            let placeholder = [egui::Color32::PLACEHOLDER; 2];
            [
                older_expected.styles.kagi.up_color,
                older_expected.styles.kagi.down_color,
            ] = placeholder;
            [
                older_expected.styles.point_figure.up_color,
                older_expected.styles.point_figure.down_color,
            ] = placeholder;
            for (json, expected) in [
                (legacy.to_string(), expected),
                (older.to_owned(), older_expected),
            ] {
                let saved = crate::widget::SavedLayout::from_json(&json).unwrap();
                restored.restore(&saved.charts[0]).unwrap();
                assert_eq!(restored.chart_type_params(), &expected);
            }
        }
        for (saved, count, overlap) in [
            (
                r#"{"Custom":{"start":"08:00:00","end":"12:30:00","zone":"America/New_York"}}"#,
                1,
                SessionOverlap::OwnProfile,
            ),
            (
                r#"{"Custom":{"sessions":[],"overlap":"InBoth"}}"#,
                0,
                SessionOverlap::InBoth,
            ),
        ] {
            let saved = if cfg!(feature = "timezones") {
                saved.to_owned()
            } else {
                saved.replace("America/New_York", "-05:00")
            };
            let session: ProfileSession = serde_json::from_str(&saved).unwrap();
            let ProfileSession::Custom(custom) = &session else {
                unreachable!()
            };
            assert_eq!(custom.sessions.len(), count);
            assert_eq!(custom.overlap, overlap);
            assert!(!custom.trading_hours_only);
            if count == 1 {
                assert_eq!(
                    custom.sessions[0].start,
                    chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap()
                );
                assert_eq!(
                    custom.sessions[0].end,
                    chrono::NaiveTime::from_hms_opt(12, 30, 0).unwrap()
                );
            }
            let new = serde_json::to_value(&session).unwrap();
            assert_eq!(new["Custom"]["sessions"].as_array().unwrap().len(), count);
            assert_eq!(
                serde_json::from_value::<ProfileSession>(new).unwrap(),
                session
            );
        }
        // A profile saved before level styles existed, including its enabled guides, and one
        // saved before histogram placement, width, values and box options existed.
        for (saved, guides) in [
            (r#"{"tpo_row_ticks":2}"#, false),
            (
                r#"{"tpo_row_ticks":2,"profile":{"session":"Chart","rows":"Feed","mode":"BidAsk","developing_poc":false,"developing_value_area":false,"extend_poc":false,"extend_vah":false,"extend_val":false,"nodes":null,"value_area":0.7,"value_area_guides":true}}"#,
                true,
            ),
            (
                r#"{"tpo_row_ticks":2,"profile":{"session":"Chart","rows":"Feed","mode":"BidAsk","developing_poc":false,"developing_value_area":false,"extend_poc":false,"extend_vah":false,"extend_val":false,"nodes":null,"value_area":0.7,"value_area_guides":true,"poc":true,"poc_color":null,"value_area_high":true,"value_area_high_color":null,"value_area_low":true,"value_area_low_color":null,"up_volume_color":null,"down_volume_color":null,"value_area_up_color":null,"value_area_down_color":null}}"#,
                true,
            ),
        ] {
            let legacy: ChartTypeParams = serde_json::from_str(saved).unwrap();
            assert_eq!(
                legacy.profile,
                TradeProfileSettings {
                    rows: if guides {
                        ProfileRows::Feed
                    } else {
                        ProfileRows::Count(24)
                    },
                    value_area_guides: guides,
                    ..Default::default()
                }
            );
            let legacy = serde_json::to_value(legacy).unwrap();
            assert_eq!(legacy["footprint_intensity"], "Bar");
            assert_eq!(legacy["footprint_extend_bars"], serde_json::Value::Null);
            assert_eq!(legacy["footprint_auto_row_height"], 16.0);
            assert_eq!(legacy["footprint_min_text_height"], 12.0);
            assert_eq!(
                legacy["footprint_step_colors"],
                serde_json::json!([[null, null, null, null], [null, null, null, null]])
            );
            assert_eq!(legacy["footprint_statistics"], false);
            assert_eq!(legacy["footprint_row_ticks"], 0);
            assert_eq!(legacy["footprint_row_atr"], false);
            assert_eq!(legacy["footprint_row_atr_length"], 14);
            assert_eq!(legacy["tpo_letters"], false);
            assert_eq!(legacy["tpo_colored_blocks"], true);
            assert_eq!(
                legacy["tpo_block_colors"],
                serde_json::json!([
                    [214, 56, 100, 255],
                    [90, 197, 97, 255],
                    [84, 185, 209, 255],
                    [93, 36, 245, 255]
                ])
            );
            assert_eq!(
                legacy["tpo_block_color"],
                serde_json::json!([189, 189, 189, 255])
            );
            assert_eq!(legacy["tpo_outside_value_area_opacity"], 45);
            assert_eq!(legacy["tpo_profile_period"], "Session");
            assert_eq!(legacy["tpo_split_periods"], false);
            assert_eq!(legacy["tpo_initial_balance"], true);
            assert_eq!(legacy["tpo_poc"], true);
            assert_eq!(legacy["tpo_value_area_levels"], true);
            assert_eq!(legacy["tpo_open_price"], true);
            assert_eq!(legacy["tpo_close_price"], false);
            assert_eq!(legacy["tpo_poor_high"], false);
            assert_eq!(legacy["tpo_poor_low"], false);
            assert_eq!(legacy["tpo_extend_poor"], false);
            assert_eq!(legacy["tpo_volume_profile"], false);
            assert_eq!(
                legacy["tpo_labels"],
                serde_json::json!({
                    "poc": true, "value_area_high": true, "value_area_low": true, "midpoint": true,
                    "initial_balance_high": false, "initial_balance_low": false, "opening_price": false, "closing_price": false,
                    "poor_high": false, "poor_low": false, "single_prints": false, "prices": false
                })
            );
            assert_eq!(legacy["tpo_level_tags"], false);
            assert_eq!(
                legacy["footprint_colors"],
                serde_json::json!([null, null, null, null])
            );
            assert_eq!(
                legacy["footprint_imbalance_colors"],
                serde_json::json!([null, null])
            );
            assert_eq!(legacy["footprint_session_cumulative_delta"], false);
            for field in [
                "footprint_stacked_imbalances",
                "footprint_naked_poc",
                "footprint_delta_poc",
                "footprint_unfinished_auctions",
                "footprint_extreme_ratio",
                "footprint_pullback",
                "footprint_zones_until_broken",
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
        let partial: ChartTypeParams =
            serde_json::from_str(r#"{"tpo_labels":{"prices":true}}"#).unwrap();
        assert!(partial.tpo_labels.prices);
        assert!(partial.tpo_labels.poc);
    }

    /// Nested windows retain the covering window's open after the inner one closes; identical
    /// windows collapse only under segmentation. Extra saved windows survive but only five run.
    #[test]
    fn custom_session_parts_keep_nested_origins_and_bound_the_session_count() {
        use chrono::{NaiveTime, TimeDelta};
        let start: chrono::DateTime<chrono::Utc> = "2024-03-11T08:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..10)
            .map(|i| crate::model::Bar::from_value(start + TimeDelta::hours(i), 100.0))
            .collect();
        let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let outer = CustomProfileSession::new(at(8), at(16));
        for (overlap, expected) in [
            (
                SessionOverlap::OwnProfile,
                vec![(0..2, 8), (2..4, 10), (4..8, 8)],
            ),
            (SessionOverlap::InBoth, vec![(0..8, 8), (2..4, 10)]),
        ] {
            let mut custom = CustomProfileSessions::new([
                outer.clone(),
                CustomProfileSession::new(at(10), at(12)),
            ]);
            custom.overlap = overlap;
            let observed: Vec<_> = custom
                .parts(
                    &bars,
                    0..bars.len(),
                    crate::model::Timeframe::Hour1,
                    None,
                    true,
                )
                .iter()
                .map(|p| (p.bars.clone(), 8 + (p.open - start).num_hours()))
                .collect();
            assert_eq!(observed, expected, "{overlap:?}");
            custom.sessions[1] = outer.clone();
            assert_eq!(
                custom
                    .parts(
                        &bars,
                        0..bars.len(),
                        crate::model::Timeframe::Hour1,
                        None,
                        true
                    )
                    .len(),
                if overlap == SessionOverlap::InBoth {
                    2
                } else {
                    1
                }
            );
        }
        let mut custom = CustomProfileSessions::new(vec![outer; 6]);
        custom.overlap = SessionOverlap::InBoth;
        let saved = serde_json::to_string(&custom).unwrap();
        let mut restored: CustomProfileSessions = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.sessions.len(), 6);
        assert_eq!(
            restored
                .parts(
                    &bars,
                    0..bars.len(),
                    crate::model::Timeframe::Hour1,
                    None,
                    true
                )
                .len(),
            5
        );
        restored.sessions.clear();
        assert!(
            restored
                .parts(
                    &bars,
                    0..bars.len(),
                    crate::model::Timeframe::Hour1,
                    None,
                    true
                )
                .is_empty()
        );
        restored
            .sessions
            .push(CustomProfileSession::new(at(8), at(8)));
        assert_eq!(
            restored.parts(
                &bars,
                0..bars.len(),
                crate::model::Timeframe::Hour1,
                None,
                true
            )[0]
            .bars,
            0..10
        );
    }

    /// Rows from the average true range take the last `footprint_row_atr_length` complete bars,
    /// so older bars that a pan loads leave them as they are. Every bar closes at 100 with its
    /// range centred on it, so its true range is its own range. At length 3 the three bars of
    /// 10 before the newest, whose 50 may still be forming, average 10, and a fifth of that is 2
    /// ticks, also with bars of 100 loaded before them. Bars that never moved have no positive
    /// average and get rows of one tick; with no complete bar the ticks per row apply.
    #[cfg(feature = "widget")]
    #[test]
    fn footprint_rows_average_the_last_complete_bars() {
        use crate::model::Bar;
        use chrono::{Duration, TimeZone, Utc};
        let start = Utc.with_ymd_and_hms(2024, 3, 5, 9, 30, 0).unwrap();
        let bar = |minute, range: f64| {
            let (time, half) = (start + Duration::minutes(minute), range / 2.0);
            Bar::new(time, 100.0, 100.0 + half, 100.0 - half, 100.0, 1.0)
        };
        let recent = [
            bar(3, 10.0),
            bar(4, 10.0),
            bar(5, 10.0),
            Bar::whitespace(start + Duration::minutes(6)),
            bar(7, 10.0),
            bar(8, 50.0),
        ];
        let paged: Vec<_> = (0..3)
            .map(|minute| bar(minute, 100.0))
            .chain(recent.clone())
            .collect();
        let flat: Vec<_> = (0..4).map(|minute| bar(minute, 0.0)).collect();
        let params = ChartTypeParams {
            footprint_row_ticks: 4,
            footprint_row_atr: true,
            footprint_row_atr_length: 3,
            ..Default::default()
        };
        let rows = [&recent[..], &paged[..], &flat[..], &recent[5..]]
            .map(|bars| params.footprint_row_size(bars, 1.0));
        assert_eq!(rows, [2, 2, 1, 4], "recent, paged, flat, newest only");
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
            series.chart_type_params.profile.session = ProfileSession::Custom(
                CustomProfileSession {
                    start: chrono::NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                    end: chrono::NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                    zone: Some(crate::model::TimezoneId::named(zone).unwrap()),
                }
                .into(),
            );
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
            assert_eq!(
                custom.sessions[0].zone.as_ref().map(|z| z.name()),
                Some(zone)
            );
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
        for (saved, minutes, high, enabled, single_prints, midpoint) in [
            (
                r#"{"tpo_initial_balance_minutes":30,"tpo_poor_highs_lows":true,"tpo_initial_balance_extensions":true}"#,
                30,
                102.0,
                true,
                false,
                true,
            ),
            (
                r#"{"tpo_period_minutes":30,"tpo_row_ticks":1}"#,
                60,
                104.0,
                false,
                false,
                true,
            ),
            (
                r#"{"tpo_single_prints":true,"tpo_midpoint":true}"#,
                60,
                104.0,
                false,
                true,
                true,
            ),
            (
                r#"{"tpo_single_prints":true,"tpo_midpoint":false}"#,
                60,
                104.0,
                false,
                true,
                false,
            ),
        ] {
            let params: ChartTypeParams = serde_json::from_str(saved).unwrap();
            assert!(
                params.tpo_splits.is_empty(),
                "older settings keep whole sessions"
            );
            let config = params.tpo_config(Some(1.0));
            assert!(config.splits.is_empty());
            assert_eq!(config.initial_balance_minutes, minutes);
            assert_eq!(config.show_poor_high, enabled);
            assert_eq!(config.show_poor_low, enabled);
            assert_eq!(config.show_single_prints, single_prints);
            assert_eq!(config.show_midpoint, midpoint);
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
            assert_eq!(saved["tpo_single_prints"], single_prints);
            assert_eq!(saved["tpo_midpoint"], midpoint);
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
