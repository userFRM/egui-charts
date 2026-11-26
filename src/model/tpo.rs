//! TPO (Time Price Opportunity) / Market Profile chart model.
//!
//! Market Profile charts show price distribution over time by placing a
//! letter (A, B, C, ...) at each price level touched during successive
//! time periods (typically 30-minute brackets). The resulting profile
//! reveals key structural levels:
//!
//! - **POC** (Point of Control) -- price with the most TPOs.
//! - **Value Area** -- price range containing the configured share of TPO activity.
//! - **Initial Balance** -- range established during the configured opening period.
//! - **Single Prints** -- levels touched during only one TPO period.
//!
//! Use [`to_tpo_profiles`] to transform OHLCV bars into per-session
//! [`TPOProfile`]s.

use crate::model::bar::{bar_level_keys, level_grid, level_key};
use crate::model::{Bar, BarClock, LineStyle, ProfileSession, SessionProvider};
use chrono::{DateTime, Datelike, Utc};
use std::collections::HashMap;
use std::ops::{Range, RangeInclusive};

/// Letter sequence for TPO periods (A, B, C... Z, a, b...)
const TPO_LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Default opacity of TPO blocks outside the value area.
pub(crate) const OUTSIDE_VALUE_AREA_OPACITY: f32 = 0.45;

/// A single TPO letter placed at a price level during one time period.
#[derive(Debug, Clone)]
pub struct TPOLetter {
    /// Price of the letter's row: its lowest tick (see [`TPOProfile::row_price`]).
    pub price: f64,
    /// The letter assigned to this time period (A, B, C, ...).
    pub letter: char,
    /// Zero-based index of the time period within the session.
    pub period_idx: usize,
    /// Open time of the first bar in this letter's period.
    pub ts: DateTime<Utc>,
}

/// A complete TPO profile for one trading session.
///
/// Contains all TPO letters, statistical levels (POC, value area,
/// initial balance), and structural flags (poor high/low, single prints).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TPOProfile {
    /// Session timestamp
    pub session_date: DateTime<Utc>,
    /// Row height the profile's prices are keyed on: `row_ticks` ticks of `instrument_tick`. It is
    /// the configured tick size, rounded up to whole instrument ticks when one is configured and
    /// widened to a whole number of rows when the bars span more than 1,000 of them, or the span
    /// divided into 1,000 rows without a usable tick size.
    pub tick_size: f64,
    /// Tick the rows are counted in: [`TPOConfig::instrument_tick`], or `tick_size` itself, making
    /// every row one tick, without one or when prices are more than 2^50 ticks from zero.
    pub instrument_tick: f64,
    /// Ticks of `instrument_tick` in each row. A row holds the ticks from a multiple of
    /// `row_ticks` up to the next and spans from half a tick below its first tick to half a tick
    /// below the next row's, as footprint rows do.
    pub row_ticks: u64,
    /// All TPO letters in this profile
    pub letters: Vec<TPOLetter>,
    /// TPO count at each price level
    pub price_tpo_count: HashMap<i64, usize>,
    /// Volume at each row; empty unless built with [`TPOConfig::show_volume_profile`].
    pub(crate) row_volume: HashMap<i64, f64>,
    /// Point of control: the price (lowest tick) of the row with the most TPOs. A tie takes the
    /// lower price.
    pub poc_price: f64,
    /// Price of the highest row of the configured TPO-count value area. The area grows from the
    /// point of control toward whichever of the next two rows above or the next two rows below
    /// holds more TPOs; equal counts add one row on each side.
    pub value_area_high: f64,
    /// Price of its lowest row.
    pub value_area_low: f64,
    /// Highest price of the bars that open within the configured initial balance period, or
    /// `f64::MIN` when none does, so the profile has no initial balance.
    pub initial_balance_high: f64,
    /// Lowest price of those bars, or `f64::MAX` when none does.
    pub initial_balance_low: f64,
    /// Profile high
    pub profile_high: f64,
    /// Profile low
    pub profile_low: f64,
    /// Opening price
    pub opening_price: f64,
    /// Closing price: the close of the profile's last bar.
    pub closing_price: f64,
    /// Volume of the profile's bars: the sum of the volumes that are positive and finite.
    pub total_volume: f64,
    /// Single prints: the prices of the rows with exactly one TPO, in ascending order
    pub single_prints: Vec<f64>,
    /// Poor highs (multiple TPOs at high)
    pub has_poor_high: bool,
    /// Poor lows (multiple TPOs at low)
    pub has_poor_low: bool,
}

/// Configuration for TPO chart generation and rendering.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TPOConfig {
    /// Row height for price grouping, such as 0.25 for one ES tick or 1.0 for four. With
    /// `instrument_tick` it rounds up to whole ticks. [`to_tpo_profiles`] widens it to bound the
    /// rows; see [`TPOProfile::tick_size`].
    pub tick_size: f64,
    /// Duration of each period in minutes (typically 30), raised to the bar interval when
    /// shorter so that every bar takes a period of its own
    pub period_minutes: u32,
    /// Duration of initial balance in minutes (typically 60)
    pub initial_balance_minutes: u32,
    /// Value area percentage (typically 0.70 for 70%)
    pub value_area_pct: f64,
    /// Show letters or blocks
    pub display_mode: TPODisplayMode,
    /// Draw periods side by side in time order, each in its own column, instead of packing each
    /// row from the left edge. The value area, point of control and statistics stay the same.
    pub split_periods: bool,
    /// Color mode for TPO letters
    pub color_mode: TPOColorMode,
    /// Colours of the entries under [`TPOColorMode::ByPeriod`]: the first pair runs through the periods
    /// lettered A to Z, one step per period, and the second pair through those lettered a to z, so a
    /// profile of 26 periods or fewer shows only the first pair. Each session of a composite, week or
    /// month profile counts its periods from its own open, and from the 53rd period the colours repeat
    /// as the letters do. With custom letters the colours follow the period's position the same way.
    /// Entries keep 3:1 contrast on the background, or 4.5:1 for letters without blocks, moving
    /// toward the chart text colour when needed.
    pub period_colors: [egui::Color32; 4],
    /// Colour of every entry under [`TPOColorMode::Solid`], kept readable in the same way.
    pub solid_color: egui::Color32,
    /// Colour of every letter, letters in cells included. `None`, the default, keeps the entry
    /// colours for letters alone and white or black, whichever reads, in cells. Letters alone
    /// stay readable as entry colours do.
    pub letter_color: Option<egui::Color32>,
    /// Opacity of blocks outside the value area over the chart background, clamped to 0..=1.
    /// Zero paints the background colour; one leaves blocks undimmed. NaN reads 0.45.
    pub outside_value_area_opacity: f32,
    /// Colour the point of control row, underline it under letters and caption it.
    pub show_poc: bool,
    /// Colour of the point of control row, underline and caption, kept readable on the
    /// background as entries are. `None`, the default, keeps the theme's text colours.
    pub poc_color: Option<egui::Color32>,
    /// Dim blocks outside the value area and allow its edges, which `show_value_area_high` and
    /// `show_value_area_low` draw and caption.
    pub show_value_area: bool,
    /// Draw the value area high and caption it, while `show_value_area` is on. On by default.
    pub show_value_area_high: bool,
    /// Draw the value area low and caption it, while `show_value_area` is on. On by default.
    pub show_value_area_low: bool,
    /// Show the caption and marker at the middle of the profile's range, with
    /// [`TPOLabels::midpoint`].
    pub show_midpoint: bool,
    /// Show Initial Balance
    pub show_initial_balance: bool,
    /// Draw one and two initial balance ranges above its high and below its low.
    pub show_initial_balance_extensions: bool,
    /// Mark the highest row when it holds more than one TPO.
    pub show_poor_high: bool,
    /// Mark the lowest row when it holds more than one TPO.
    pub show_poor_low: bool,
    /// Extend marked extremes of completed profiles until a later bar trades strictly through
    /// their price, or to the plot's right edge while no shown bar has.
    pub extend_poor_extremes: bool,
    /// Show single prints
    pub show_single_prints: bool,
    /// Show the session statistics table below the profile, with the rows of `statistics_rows`.
    pub show_statistics: bool,
    /// Rows of the statistics table; see [`TPOStatisticsRows`].
    pub statistics_rows: TPOStatisticsRows,
    /// Mark each profile's opening price with a triangle left of the profile.
    pub show_opening_range: bool,
    /// Level captions and their prices; see [`TPOLabels`].
    pub labels: TPOLabels,
    /// Mark the closing price right of its row, in the bullish colour when it closes at or above
    /// its open, otherwise in the bearish colour.
    pub show_closing_price: bool,
    /// Draw row volumes beside each profile, right of it unless [`TPOVolumeProfile::left`], up
    /// to 40 px for the busiest row. Each bar shares its volume equally among the rows its range
    /// touches; nonpositive or nonfinite volume adds none.
    pub show_volume_profile: bool,
    /// Levels, values, colours and side of the volume profile; see [`TPOVolumeProfile`].
    pub volume_profile: TPOVolumeProfile,
    /// Split each session where it moves between regular, extended and overnight hours, giving
    /// separate overnight, pre-market, regular and post-market profiles.
    /// One-day profiles only; see [`TPOProfilePeriod`].
    pub split_sessions: bool,
    /// Which bars of each trading day of the clock the profiles keep: every bar for
    /// `ProfileSession::Chart`, otherwise those of the hours the choice picks, read from a clock
    /// of those hours. The Time Price Opportunity study sets it; `split_sessions` splits the
    /// trading day only.
    pub(crate) session: ProfileSession,
    /// Regular instrument hours used only when custom sessions request clipping.
    pub(crate) regular_hours: Option<BarClock>,
    /// Unit of each profile's length: the default `Session`, a trading day, or a week or month
    /// of sessions; `profile_periods` sets how many.
    pub profile_period: TPOProfilePeriod,
    /// How many of `profile_period` each profile covers: consecutive trading days of the clock,
    /// ISO weeks or calendar months, counted on a grid fixed from 1970 so that a profile keeps
    /// its days while history loads (see [`TPOProfilePeriod`]). One by default; zero reads one.
    pub profile_periods: u32,
    /// Custom letters sequence; an empty string uses the default letters
    pub custom_letters: Option<String>,
    /// Sessions to combine: the profiles whose sessions open within one range form a single
    /// profile. Adding a range over adjacent sessions merges them, removing it splits them back,
    /// and a range over many sessions builds a composite profile.
    pub composites: Vec<RangeInclusive<DateTime<Utc>>>,
    /// Instants that start another profile at the first bar at or after them. Session profiles
    /// last until the next session or split. Periods still count from the session's open: splitting
    /// at its third period leaves A and B in the first profile and C and D in the second. Each part holds
    /// the initial balance of its own bars, with none after the balance window.
    /// An instant at or before the first usable bar changes nothing; one beyond the last
    /// bar takes effect when later bars reach it. A part joins a composite if its first bar
    /// lies in the range. A split inside a period, for example after changing the period
    /// length, leaves that period's letter in both parts.
    pub splits: Vec<DateTime<Utc>>,
    /// Draw only the newest session's profile instead of every visible one
    pub latest_session_only: bool,
    /// The instrument's minimum price increment. When set, rows are whole ticks counted as
    /// footprint rows are: a row of n ticks holds the ticks from a multiple of n up to the next.
    /// `None`, or a tick that is not positive and finite, counts each row as one tick.
    pub instrument_tick: Option<f64>,
}

impl Default for TPOConfig {
    fn default() -> Self {
        Self {
            tick_size: 1.0,
            period_minutes: 30,
            initial_balance_minutes: 60,
            value_area_pct: 0.70,
            display_mode: TPODisplayMode::Letters,
            split_periods: false,
            color_mode: TPOColorMode::ByPeriod,
            period_colors: [
                egui::Color32::from_rgb(214, 56, 100),
                egui::Color32::from_rgb(90, 197, 97),
                egui::Color32::from_rgb(84, 185, 209),
                egui::Color32::from_rgb(93, 36, 245),
            ],
            solid_color: crate::tokens::DESIGN_TOKENS.semantic.tpo.letter_default,
            letter_color: None,
            outside_value_area_opacity: OUTSIDE_VALUE_AREA_OPACITY,
            show_poc: true,
            poc_color: None,
            show_value_area: true,
            show_value_area_high: true,
            show_value_area_low: true,
            show_midpoint: false,
            show_initial_balance: true,
            show_initial_balance_extensions: false,
            show_poor_high: false,
            show_poor_low: false,
            extend_poor_extremes: false,
            show_single_prints: false,
            show_statistics: false,
            statistics_rows: TPOStatisticsRows::default(),
            show_opening_range: true,
            labels: TPOLabels::default(),
            show_closing_price: false,
            show_volume_profile: false,
            volume_profile: TPOVolumeProfile::default(),
            split_sessions: false,
            session: ProfileSession::Chart,
            regular_hours: None,
            profile_period: TPOProfilePeriod::Session,
            profile_periods: 1,
            custom_letters: None,
            composites: Vec::new(),
            splits: Vec::new(),
            latest_session_only: false,
            instrument_tick: None,
        }
    }
}

/// Level captions beside a TPO profile, with optional prices.
///
/// Each switch requires its level to be drawn by [`TPOConfig`]: `show_poc`, `show_value_area`
/// with `show_value_area_high` or `show_value_area_low`, `show_midpoint`,
/// `show_initial_balance`, `show_opening_range`, `show_closing_price`, `show_poor_high`,
/// `show_poor_low` or `show_single_prints`. Initial balance and poor extremes
/// also require that level to exist. The midpoint has no mark apart from its caption,
/// so hiding its caption hides the midpoint. Coinciding captions occupy separate rows,
/// with earlier fields keeping the level's row. Defaults keep the point of control,
/// value area edges and midpoint captions, without prices.
///
/// ```
/// use egui_charts::model::{ChartTypeParams, TPOLabels};
///
/// let mut labels = TPOLabels::default();
/// labels.initial_balance_high = true;
/// labels.prices = true;
/// let params = ChartTypeParams { tpo_labels: labels, ..Default::default() };
/// assert!(params.tpo_config(Some(0.25)).labels.initial_balance_high);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TPOLabels {
    /// Caption the point of control `POC`. On by default.
    pub poc: bool,
    /// Caption the value area high `VAH`. On by default.
    pub value_area_high: bool,
    /// Caption the value area low `VAL`. On by default.
    pub value_area_low: bool,
    /// Caption the profile's midpoint `Mid`. On by default.
    pub midpoint: bool,
    /// Caption the initial balance high `IBH`.
    pub initial_balance_high: bool,
    /// Caption the initial balance low `IBL`.
    pub initial_balance_low: bool,
    /// Caption the session's opening price `Open`.
    pub opening_price: bool,
    /// Caption the profile's closing price `Close`. Off by default.
    pub closing_price: bool,
    /// Caption a poor high `PH`.
    pub poor_high: bool,
    /// Caption a poor low `PL`.
    pub poor_low: bool,
    /// Caption each run of adjacent single-print rows `SP`.
    pub single_prints: bool,
    /// Append prices at [`TPOProfile::instrument_tick`] precision, or half that tick
    /// for the midpoint: `IBH 101.25` on a 0.25 tick. Single-print runs show their
    /// lowest and highest row, such as `SP 104 / 105`. Widens the caption gutter.
    pub prices: bool,
}

impl Default for TPOLabels {
    fn default() -> Self {
        Self {
            poc: true,
            value_area_high: true,
            value_area_low: true,
            midpoint: true,
            initial_balance_high: false,
            initial_balance_low: false,
            opening_price: false,
            closing_price: false,
            poor_high: false,
            poor_low: false,
            single_prints: false,
            prices: false,
        }
    }
}

/// Rows of the session statistics table below a TPO profile, in display order. The table is
/// drawn with [`TPOConfig::show_statistics`]. A row that is off is left out, the initial
/// balance rows are left out of a profile without one, and no table is drawn when no row is
/// left. Defaults show the six rows drawn before the rows could be chosen.
///
/// ```
/// use egui_charts::model::{ChartTypeParams, TPOStatisticsRows};
///
/// let mut rows = TPOStatisticsRows::default();
/// rows.total_volume = true;
/// rows.rotation_factor = false;
/// let params = ChartTypeParams {
///     tpo_statistics: true,
///     tpo_statistics_rows: rows,
///     ..Default::default()
/// };
/// assert!(params.tpo_config(None).statistics_rows.total_volume);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TPOStatisticsRows {
    /// `High-low range`: the profile's high minus its low. Off by default.
    pub high_low_range: bool,
    /// `POC`: the point of control. On by default.
    pub poc: bool,
    /// `VAH`: the value area high. On by default. With [`Self::value_area_low`] on too, the two
    /// share one `VAL / VAH` row.
    pub value_area_high: bool,
    /// `VAL`: the value area low. On by default.
    pub value_area_low: bool,
    /// `VA range`: the value area high minus its low. Off by default.
    pub value_area_range: bool,
    /// `IB high`: the initial balance high. Off by default.
    pub initial_balance_high: bool,
    /// `IB low`: the initial balance low. Off by default.
    pub initial_balance_low: bool,
    /// `IB range`: the initial balance high minus its low. On by default.
    pub initial_balance_range: bool,
    /// `TPO above`: the TPOs above the point of control's row. On by default. With
    /// [`Self::tpos_below`] on too, the two share one `TPO below / above` row.
    pub tpos_above: bool,
    /// `TPO below`: the TPOs below the point of control's row. On by default.
    pub tpos_below: bool,
    /// `Total TPOs`: every TPO of the profile. Off by default.
    pub total_tpos: bool,
    /// `Total volume`: [`TPOProfile::total_volume`], with K, M and B suffixes. Off by default.
    pub total_volume: bool,
    /// `Rotation`: [`TPOProfile::rotation_factor`]. On by default.
    pub rotation_factor: bool,
    /// `IB ext. below / above`: [`TPOProfile::range_extension`]. On by default.
    pub range_extension: bool,
}

impl Default for TPOStatisticsRows {
    fn default() -> Self {
        Self {
            high_low_range: false,
            poc: true,
            value_area_high: true,
            value_area_low: true,
            value_area_range: false,
            initial_balance_high: false,
            initial_balance_low: false,
            initial_balance_range: true,
            tpos_above: true,
            tpos_below: true,
            total_tpos: false,
            total_volume: false,
            rotation_factor: true,
            range_extension: true,
        }
    }
}

impl TPOStatisticsRows {
    /// Each row's saved name, table label and switch, in display order.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub(crate) fn switches(&mut self) -> [(&'static str, &'static str, &mut bool); 14] {
        [
            ("high_low_range", "High-low range", &mut self.high_low_range),
            ("poc", "POC", &mut self.poc),
            ("value_area_high", "VAH", &mut self.value_area_high),
            ("value_area_low", "VAL", &mut self.value_area_low),
            ("value_area_range", "VA range", &mut self.value_area_range),
            (
                "initial_balance_high",
                "IB high",
                &mut self.initial_balance_high,
            ),
            (
                "initial_balance_low",
                "IB low",
                &mut self.initial_balance_low,
            ),
            (
                "initial_balance_range",
                "IB range",
                &mut self.initial_balance_range,
            ),
            ("tpos_above", "TPO above", &mut self.tpos_above),
            ("tpos_below", "TPO below", &mut self.tpos_below),
            ("total_tpos", "Total TPOs", &mut self.total_tpos),
            ("total_volume", "Total volume", &mut self.total_volume),
            ("rotation_factor", "Rotation", &mut self.rotation_factor),
            (
                "range_extension",
                "IB ext. below / above",
                &mut self.range_extension,
            ),
        ]
    }
}

/// Levels, values, colours and side of the volume profile drawn beside each TPO profile with
/// [`TPOConfig::show_volume_profile`]. Its point of control is the row with the most volume, the
/// lower one on a tie; its value area grows from there over the row volumes, two rows at a time
/// toward the heavier side as [`TPOProfile::value_area_high`] describes, until it holds
/// [`TPOConfig::value_area_pct`] of the profile's volume.
///
/// ```
/// use egui_charts::model::{ChartTypeParams, TPOVolumeProfile};
///
/// let mut volume = TPOVolumeProfile::default();
/// volume.poc = true;
/// volume.left = true;
/// volume.value_area_color = Some(egui::Color32::BLUE);
/// let params = ChartTypeParams { tpo_volume: volume, ..Default::default() };
/// assert!(params.tpo_config(None).volume_profile.left);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TPOVolumeProfile {
    /// Draw a line across the volume profile at its point of control. Off by default.
    pub poc: bool,
    /// Colour of that line. `None`, the default, takes the TPO point of control's colour.
    pub poc_color: Option<egui::Color32>,
    /// Pattern of that line, solid by default.
    pub poc_style: LineStyle,
    /// Draw a line across it at its value area high. Off by default.
    pub value_area_high: bool,
    /// Colour of that line. `None`, the default, takes the theme's text colour.
    pub value_area_high_color: Option<egui::Color32>,
    /// Pattern of that line, solid by default.
    pub value_area_high_style: LineStyle,
    /// Draw a line across it at its value area low. Off by default.
    pub value_area_low: bool,
    /// Colour of that line. `None`, the default, takes the theme's text colour.
    pub value_area_low_color: Option<egui::Color32>,
    /// Pattern of that line, solid by default.
    pub value_area_low_style: LineStyle,
    /// Write each row's volume inside its bar, with K, M and B suffixes, where the text fits.
    /// Off by default.
    pub values: bool,
    /// Colour of those values. `None`, the default, writes white or black, whichever reads on
    /// the bar.
    pub values_color: Option<egui::Color32>,
    /// Colour of the rows outside the volume value area, including opacity. `None`, the
    /// default, fades `value_area_color` toward the background by
    /// [`TPOConfig::outside_value_area_opacity`].
    pub color: Option<egui::Color32>,
    /// Colour of the rows inside the volume value area, including opacity. `None`, the
    /// default, uses the chart's secondary axis text colour.
    pub value_area_color: Option<egui::Color32>,
    /// Draw the volume profile left of the TPO profile instead of right of it, the default.
    pub left: bool,
    /// Rows fade outside the TPO value area instead, and only while blocks and the value area
    /// are drawn, while both colours are `None`: how profiles saved before these settings drew.
    pub(crate) tpo_value_area_rows: bool,
}

impl TPOVolumeProfile {
    /// The settings of a document saved before the volume profile had settings of its own.
    pub(crate) fn saved_without_settings() -> Self {
        Self {
            tpo_value_area_rows: true,
            ..Self::default()
        }
    }
}

/// How to render TPO entries on the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TPODisplayMode {
    /// Show letters (A, B, C...)
    Letters,
    /// Show blocks
    Blocks,
    /// Show both letters and blocks
    Both,
}

/// Color-mapping strategy for TPO letters/blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TPOColorMode {
    /// Color by time period
    ByPeriod,
    /// Every entry in [`TPOConfig::solid_color`].
    Solid,
    /// Color by value area
    ByValueArea,
    /// Color by initial balance
    ByInitialBalance,
}

/// Unit of a TPO profile's length; [`TPOConfig::profile_periods`] counts how many each profile
/// covers.
///
/// Profiles longer than one session (a week, a month, or more than one day with
/// [`TPOConfig::profile_periods`]) combine sessions by the trading date of each session's first
/// bar. Their letters, point of control, value area and statistics cover all those sessions; the
/// open and initial balance remain those of the first session. They hold whole sessions, ignoring
/// [`TPOConfig::split_sessions`]. Explicit [`TPOConfig::splits`] still divide them, also at a
/// session's open, and subsequent sessions of the period join the last part.
/// [`TPOConfig::composites`] still join profiles whose first sessions open in a range. On daily
/// and longer bars, a period no longer than the session changes nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum TPOProfilePeriod {
    /// Trading days: one profile per session, or per run of `profile_periods` trading dates.
    #[default]
    Session,
    /// ISO weeks, Monday to Sunday, of the sessions' trading dates.
    Week,
    /// Calendar months of the sessions' trading dates.
    Month,
}

impl TPOConfig {
    /// Whether profiles join sessions: a week or month period, or more than one day.
    pub(crate) fn joins_sessions(&self) -> bool {
        self.profile_period != TPOProfilePeriod::Session || self.profile_periods > 1
    }

    /// The profile period holding the bar at `time`, by its trading date.
    pub(crate) fn period_key(
        &self,
        sessions: &crate::model::ExchangeSessionProvider,
        time: DateTime<Utc>,
    ) -> Option<i64> {
        self.joins_sessions()
            .then(|| sessions.trading_date(time))
            .and_then(|date| self.date_period_key(sessions, date))
    }

    /// The profile period holding trading date `date`, or `None` for one-day profiles, which
    /// never join by period. Days: the first date, in days after 1970-01-01, of the run of
    /// `profile_periods` trading dates of `sessions` that holds it, or of the run of as many
    /// calendar days when nothing trades on or before it. Weeks: the run of ISO weeks counted
    /// from Monday 1970-01-05. Months: the run of calendar months counted from January of year 0.
    pub(crate) fn date_period_key(
        &self,
        sessions: &crate::model::ExchangeSessionProvider,
        date: chrono::NaiveDate,
    ) -> Option<i64> {
        if !self.joins_sessions() {
            return None;
        }
        let count = i64::from(self.profile_periods.max(1));
        let epoch = DateTime::<Utc>::UNIX_EPOCH.date_naive();
        Some(match self.profile_period {
            TPOProfilePeriod::Session => sessions.trading_run(date, count, 0).map_or_else(
                || {
                    let day = (date - epoch).num_days();
                    day - day.rem_euclid(count)
                },
                |first| (first - epoch).num_days(),
            ),
            TPOProfilePeriod::Week => {
                let monday = chrono::NaiveDate::from_ymd_opt(1970, 1, 5).unwrap();
                (date - monday).num_days().div_euclid(7).div_euclid(count)
            }
            TPOProfilePeriod::Month => {
                (i64::from(date.year()) * 12 + i64::from(date.month0())).div_euclid(count)
            }
        })
    }

    /// Whether a caption is enabled for a level the profiles draw.
    #[cfg(feature = "widget")]
    pub(crate) fn shows_captions(&self) -> bool {
        let labels = &self.labels;
        (labels.poc && self.show_poc)
            || (labels.value_area_high && self.show_value_area && self.show_value_area_high)
            || (labels.value_area_low && self.show_value_area && self.show_value_area_low)
            || (labels.midpoint && self.show_midpoint)
            || ((labels.initial_balance_high || labels.initial_balance_low)
                && self.show_initial_balance)
            || (labels.opening_price && self.show_opening_range)
            || (labels.closing_price && self.show_closing_price)
            || (labels.poor_high && self.show_poor_high)
            || (labels.poor_low && self.show_poor_low)
            || (labels.single_prints && self.show_single_prints)
    }

    /// Only one-day profiles split at transitions between regular, extended and overnight hours.
    pub(crate) fn split_hours(&self) -> bool {
        self.split_sessions && !self.joins_sessions()
    }

    /// The length of the periods on bars of `interval`: `period_minutes`, at least one minute, or
    /// the interval when that is longer.
    pub(crate) fn period_length(
        &self,
        interval: Option<crate::model::Timeframe>,
    ) -> crate::model::Timeframe {
        let interval = interval.map_or(0, |t| t.duration_ms() / 60_000);
        let minutes = i64::from(self.period_minutes.max(1)).max(interval);
        crate::model::Timeframe::Custom(
            std::num::NonZeroU64::new(minutes.unsigned_abs() * 60)
                .unwrap_or(std::num::NonZeroU64::MIN),
        )
    }

    /// The parts of `bars[range]` that custom hours profile, with their periods on the period
    /// grid ([`CustomProfileSessions`](crate::model::CustomProfileSessions)); none for other
    /// hours.
    pub(crate) fn custom_parts(
        &self,
        bars: &[Bar],
        range: Range<usize>,
        interval: Option<crate::model::Timeframe>,
    ) -> Vec<crate::model::chart_type::CustomPart> {
        let ProfileSession::Custom(custom) = &self.session else {
            return Vec::new();
        };
        custom.parts(
            bars,
            range,
            interval.unwrap_or_default(),
            self.regular_hours.as_ref(),
            true,
            Some(self.period_length(interval)),
        )
    }
}

/// Session classification for splitting TPO profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// Regular Trading Hours
    RTH,
    /// Extended Trading Hours
    ETH,
    /// Full session (RTH + ETH combined)
    Full,
}

impl TPOProfile {
    /// Create a new empty TPO profile on rows of one tick of `tick_size`
    pub fn new(session_date: DateTime<Utc>, tick_size: f64) -> Self {
        Self {
            session_date,
            tick_size,
            instrument_tick: tick_size,
            row_ticks: 1,
            letters: Vec::new(),
            price_tpo_count: HashMap::new(),
            row_volume: HashMap::new(),
            poc_price: 0.0,
            value_area_high: 0.0,
            value_area_low: 0.0,
            initial_balance_high: f64::MIN,
            initial_balance_low: f64::MAX,
            profile_high: f64::MIN,
            profile_low: f64::MAX,
            opening_price: 0.0,
            closing_price: 0.0,
            total_volume: 0.0,
            single_prints: Vec::new(),
            has_poor_high: false,
            has_poor_low: false,
        }
    }

    /// Row key of `price` on this profile's rows.
    fn key(&self, price: f64) -> i64 {
        level_key(price, self.instrument_tick, self.row_ticks)
    }

    /// Price of the row holding `price`: the row's lowest tick.
    pub fn row_price(&self, price: f64) -> f64 {
        key_to_price(self.key(price), self.tick_size)
    }

    /// Lowest and highest price of the row holding `price`, from half a tick below its first tick
    /// to half a tick below the next row's first tick.
    pub fn row_span(&self, price: f64) -> (f64, f64) {
        let (row, half_tick) = (self.row_price(price), self.instrument_tick * 0.5);
        (row - half_tick, row + self.tick_size - half_tick)
    }

    /// Get the TPO count at a specific price level
    pub fn tpo_count_at(&self, price: f64) -> usize {
        let price_key = self.key(price);
        *self.price_tpo_count.get(&price_key).unwrap_or(&0)
    }

    /// Volume at the row holding `price`, or zero. Built only with [`TPOConfig::show_volume_profile`].
    pub fn volume_at(&self, price: f64) -> f64 {
        self.row_volume
            .get(&self.key(price))
            .copied()
            .unwrap_or(0.0)
    }

    /// Get all unique price levels with TPOs
    pub fn price_levels(&self) -> Vec<f64> {
        let mut levels: Vec<f64> = self
            .price_tpo_count
            .keys()
            .map(|&k| key_to_price(k, self.tick_size))
            .collect();
        levels.sort_by(|a, b| b.total_cmp(a));
        levels
    }

    /// Check if a price is in the value area
    pub fn is_in_value_area(&self, price: f64) -> bool {
        price >= self.value_area_low && price <= self.value_area_high
    }

    /// Check if a price is in the initial balance
    pub fn is_in_initial_balance(&self, price: f64) -> bool {
        price >= self.initial_balance_low && price <= self.initial_balance_high
    }

    /// Get profile width (number of periods)
    pub fn width(&self) -> usize {
        self.letters.iter().map(|l| l.period_idx).max().unwrap_or(0) + 1
    }

    /// Add the letters of a later session on the same rows, keeping this session's open and
    /// initial balance. The statistical levels need recalculating.
    fn merge(&mut self, later: TPOProfile) {
        for (key, count) in later.price_tpo_count {
            *self.price_tpo_count.entry(key).or_insert(0) += count;
        }
        for (key, volume) in later.row_volume {
            *self.row_volume.entry(key).or_insert(0.0) += volume;
        }
        self.letters.extend(later.letters);
        self.closing_price = later.closing_price;
        self.total_volume += later.total_volume;
        self.profile_high = self.profile_high.max(later.profile_high);
        self.profile_low = self.profile_low.min(later.profile_low);
    }

    /// Get the letters at a specific price level
    pub fn letters_at(&self, price: f64) -> Vec<&TPOLetter> {
        let price_key = self.key(price);
        self.letters
            .iter()
            .filter(|l| self.key(l.price) == price_key)
            .collect()
    }
}

/// Derive a sensible TPO row size (price-bin height) from a price range and a
/// target number of rows.
///
/// Market Profile groups price into equal bins; the bin height controls how
/// many letters stack at each level. Rather than hard-coding a tick size that
/// only suits one instrument, this picks a "nice" step (1, 2, 2.5 or 5 times a
/// power of ten) so the profile lands close to `target_rows` regardless of
/// whether the series trades near 1.0 or 50_000.0. Degenerate inputs (zero or
/// non-finite range, non-positive target) fall back to `1.0`.
pub fn derive_tick_size(price_range: f64, target_rows: usize) -> f64 {
    if !price_range.is_finite() || price_range <= 0.0 || target_rows == 0 {
        return 1.0;
    }
    let raw = price_range / target_rows as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 2.5 {
        2.5
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    let tick = nice * magnitude;
    if tick.is_finite() && tick > 0.0 {
        tick
    } else {
        1.0
    }
}

/// Largest tick number whose rows keep finding their own prices.
const EXACT_TICKS: f64 = (1u64 << 50) as f64;

/// The fewest whole ticks that reach `height`, at least one, within one part in a billion so that
/// a height of exactly n ticks stays n ticks despite rounding.
pub(crate) fn whole_ticks(height: f64, tick: f64) -> f64 {
    (height / tick * (1.0 - 1e-9)).ceil().max(1.0)
}

/// The tick and ticks per row of rows of `grid` for prices up to `extreme` in magnitude: whole
/// instrument ticks reaching the grid, or the grid as one tick without a usable instrument tick
/// or when tick numbers reach 2^50, past which a row's price can key the row below.
fn row_ticks(grid: f64, instrument_tick: Option<f64>, extreme: f64) -> (f64, u64) {
    instrument_tick
        .filter(|tick| *tick > 0.0 && tick.is_finite() && extreme / tick < EXACT_TICKS)
        .map(|tick| (tick, whole_ticks(grid, tick)))
        .filter(|&(_, ticks)| ticks < EXACT_TICKS)
        .map_or((grid, 1), |(tick, ticks)| (tick, ticks as u64))
}

/// Convert integer key back to price
pub(crate) fn key_to_price(key: i64, tick_size: f64) -> f64 {
    key as f64 * tick_size
}

/// Whether an explicit split falls after `previous` and at or before `time`, keeping the new
/// profile from joining its predecessor by week or month.
pub(crate) fn split_between(
    splits: &[DateTime<Utc>],
    previous: Option<DateTime<Utc>>,
    time: DateTime<Utc>,
) -> bool {
    previous.is_some_and(|previous| splits.iter().any(|s| previous < *s && *s <= time))
}

/// First usable bars of profiles before composites, including starts from `splits`, shared by
/// the model, cache and menus.
pub(crate) fn session_starts(
    bars: &[Bar],
    sessions: &crate::model::ExchangeSessionProvider,
    split_sessions: bool,
    splits: &[DateTime<Utc>],
) -> Vec<usize> {
    let mut starts: Vec<_> = std::iter::once(0)
        .chain(sessions.session_breaks(bars).into_iter().map(|(i, _)| i))
        .collect();
    if split_sessions {
        use crate::model::session::WindowKind;
        let kinds: Vec<_> = bars
            .iter()
            .map(|bar| {
                sessions
                    .window_kind(bar.time)
                    .filter(|kind| *kind != WindowKind::Regular)
            })
            .collect();
        starts.extend((1..bars.len()).filter(|&i| kinds[i] != kinds[i - 1]));
    }
    starts.extend(
        splits
            .iter()
            .map(|split| bars.partition_point(|bar| bar.time < *split))
            .filter(|&i| i > 0 && i < bars.len()),
    );
    starts.sort_unstable();
    starts.dedup();
    starts.push(bars.len());
    starts
        .windows(2)
        .filter_map(|span| {
            (span[0]..span[1]).find(|&i| {
                let bar = &bars[i];
                bar.high.is_finite() && bar.low.is_finite() && bar.high >= bar.low
            })
        })
        .collect()
}

/// One profile's source bars before composites, with its period origin and overlap layer.
#[derive(Clone, Debug)]
pub(crate) struct ProfilePart {
    pub(crate) bars: Range<usize>,
    pub(crate) opens: bool,
    pub(crate) open: Option<DateTime<Utc>>,
    /// Custom windows use their own trading date, even before the chart calendar opens.
    pub(crate) date: Option<chrono::NaiveDate>,
    pub(crate) layer: usize,
}

/// The bars of each profile before composites, and whether it opens a session: from each start
/// of [`session_starts`] to the next for [`ProfileSession::Chart`], otherwise the parts of each
/// trading day that `config.session` keeps ([`ProfileSession::profiled`]). `config.splits` cut
/// these further into profiles that keep counting periods from the open of the part they split.
pub(crate) fn profile_parts(
    bars: &[Bar],
    sessions: &crate::model::ExchangeSessionProvider,
    config: &TPOConfig,
    interval: Option<crate::model::Timeframe>,
) -> Vec<ProfilePart> {
    let whole = |bars| ProfilePart {
        bars,
        opens: true,
        open: None,
        date: None,
        layer: 0,
    };
    let mut parts = Vec::new();
    if let ProfileSession::Custom(custom) = &config.session {
        let shared = custom.overlap == crate::model::SessionOverlap::InBoth;
        for part in config.custom_parts(bars, 0..bars.len(), interval) {
            parts.push(ProfilePart {
                bars: part.bars,
                opens: true,
                open: Some(part.open),
                date: Some(part.date),
                layer: if shared { part.session } else { 0 },
            });
        }
    } else if config.session == ProfileSession::Chart {
        let starts = session_starts(bars, sessions, config.split_hours(), &[]);
        for (n, &start) in starts.iter().enumerate() {
            let end = starts.get(n + 1).copied().unwrap_or(bars.len());
            parts.push(whole(start..end));
        }
    } else {
        // Whole trading days, unusable bars included: a day of them must not join the day before.
        let mut start = 0;
        let breaks = sessions.session_breaks(bars).into_iter().map(|(i, _)| i);
        for end in breaks.chain([bars.len()]) {
            for range in config.session.profiled(bars, start..end, sessions, true) {
                parts.push(whole(range));
            }
            start = end;
        }
    }
    let mut cuts: Vec<_> = (config.splits.iter())
        .map(|split| bars.partition_point(|bar| bar.time < *split))
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    if cuts.is_empty() {
        return parts;
    }
    let mut split = Vec::with_capacity(parts.len() + cuts.len());
    for part in parts {
        let mut start = part.bars.start;
        for &cut in &cuts {
            if start < cut && cut < part.bars.end {
                split.push(ProfilePart {
                    bars: start..cut,
                    opens: start == part.bars.start,
                    ..part.clone()
                });
                start = cut;
            }
        }
        split.push(ProfilePart {
            bars: start..part.bars.end,
            opens: start == part.bars.start,
            ..part
        });
    }
    split
}

/// Transform bars into TPO profiles
///
/// With `config.profile_period` a week or month, or `config.profile_periods` above one, sessions
/// opening in one period form a single profile, as [`TPOProfilePeriod`] describes.
///
/// Sessions are the trading days of `clock`: the exchange sessions it carries,
/// or calendar days in its time zone, split further at the moves between
/// regular, extended and overnight hours with `config.split_sessions` and combined over
/// `config.composites`. `config.splits` divides a session into further profiles that keep its
/// periods. A session's periods are whole multiples of
/// `config.period_minutes`, or of the clock's interval when longer, since its
/// open: that of its trading day with intraday bars and trading hours, or of
/// its stretch of regular, extended or overnight hours when split, whatever time its
/// first bar has. Custom hours keep whole periods on their grid instead: whole
/// periods from midnight in the window's zone, or of the regular hours when
/// clipped to them. A period that opens before the window's open is left out,
/// and the last period, which opens before its close, keeps its bars past the
/// close. Calendar days, daily and longer bars and bars split off
/// outside the hours count from their first bar, since their open is not
/// known. A gap leaves empty periods, and each period places one letter on
/// every row its bars touched. The initial balance covers the bars that open
/// within `config.initial_balance_minutes` of the same instant.
///
/// Rows are `config.tick_size` widened so the bars cover at most about 1,000
/// rows (see [`TPOProfile::tick_size`]). With `config.instrument_tick` rows are
/// whole ticks grouped from a multiple of the row, and each row's price is its
/// lowest tick. Bars with a non-finite high or low or an inverted range are
/// skipped.
///
/// # Arguments
/// * `bars` - Source OHLCV bars
/// * `config` - TPO configuration
/// * `clock` - Interval and calendar of the bars, such as the chart's `Chart::bar_clock`
///
/// # Returns
/// Vector of TPO profiles after splits and composites
///
/// ```
/// use egui_charts::model::{Bar, BarClock, TPOConfig, Timeframe, to_tpo_profiles};
///
/// let mut config = TPOConfig::default();
/// config.tick_size = 0.25;
/// let bars: Vec<Bar> = Vec::new();
/// let profiles = to_tpo_profiles(&bars, &config, &BarClock::new(Timeframe::Min30));
/// assert!(profiles.is_empty());
/// ```
pub fn to_tpo_profiles(bars: &[Bar], config: &TPOConfig, clock: &BarClock) -> Vec<TPOProfile> {
    if bars.is_empty() {
        return Vec::new();
    }

    let parts = profile_parts(bars, &clock.sessions(), config, clock.timeframe());
    if parts.iter().all(|part| part.layer == 0) {
        return profiles_for_parts(bars, config, clock, parts);
    }
    let top = parts.iter().map(|part| part.layer).max().unwrap_or(0);
    let mut profiles = Vec::new();
    for layer in 0..=top {
        let layer_parts: Vec<_> = parts.iter().filter(|p| p.layer == layer).cloned().collect();
        if !layer_parts.is_empty() {
            profiles.extend(profiles_for_parts(bars, config, clock, layer_parts));
        }
    }
    profiles.sort_by_key(|p| p.session_date);
    profiles
}

/// Build and combine one layer, so a bar shared by sessions contributes to each independently.
fn profiles_for_parts(
    bars: &[Bar],
    config: &TPOConfig,
    clock: &BarClock,
    parts: Vec<ProfilePart>,
) -> Vec<TPOProfile> {
    let letters = config
        .custom_letters
        .as_deref()
        .filter(|letters| !letters.is_empty())
        .unwrap_or(TPO_LETTERS)
        .chars()
        .collect::<Vec<_>>();

    let mut profiles: Vec<TPOProfile> = Vec::new();
    let mut opened = Vec::new();
    let mut current_profile: Option<TPOProfile> = None;
    let mut session_open = bars[0].time;
    // The open period: its index, first bar time and row-range endpoints
    let mut period_idx = 0;
    let mut period_ts = bars[0].time;
    let period_minutes = config.period_length(clock.timeframe()).duration_ms() / 60_000;
    let sessions = clock.sessions();
    let mut parts = parts.into_iter().peekable();
    let mut part_open = None;
    let mut part_date = None;
    // The end of the open profile's bars; bars from there to the next profile join none.
    let mut part_end = 0;
    // The pre-market, post-market and segment profiles count from the open of their own hours.
    let split = match config.session {
        ProfileSession::Chart => config.split_hours(),
        ProfileSession::PreMarket | ProfileSession::PostMarket | ProfileSession::Segments => true,
        _ => false,
    };
    let grid = level_grid(bars, config.tick_size);
    let extreme = bars
        .iter()
        .filter(|bar| bar.low.is_finite() && bar.high.is_finite() && bar.low <= bar.high)
        .fold(0.0_f64, |m, bar| m.max(bar.low.abs()).max(bar.high.abs()));
    let (tick, row_ticks) = row_ticks(grid, config.instrument_tick, extreme);
    let row = row_ticks as f64 * tick;
    let keys = bars
        .iter()
        .filter_map(|bar| bar_level_keys(bar, tick, row_ticks));
    let Some((first_key, last_key)) = keys.reduce(|(low, high), (l, h)| (low.min(l), high.max(h)))
    else {
        return Vec::new();
    };
    // One extra endpoint closes a range ending at the highest row, including i64::MAX.
    let mut row_changes = vec![0isize; last_key.abs_diff(first_key) as usize + 2];

    let mut touched = RangeInclusive::new(usize::MAX, 0);
    let mut previous = None;

    for (i, bar) in bars.iter().enumerate() {
        let Some((low_key, high_key)) = bar_level_keys(bar, tick, row_ticks) else {
            continue;
        };

        // A profile starting at a skipped bar starts at the next usable one.
        let mut is_new_session = current_profile.is_none();
        // A split starts a profile, while its periods keep counting from the session's open.
        let mut opens_session = is_new_session;
        while let Some(part) = parts.next_if(|part| part.bars.start <= i) {
            part_end = part.bars.end;
            part_date = part.date;
            is_new_session = true;
            opens_session |= part.opens;
            if part.opens {
                part_open = part.open;
            }
        }
        // Bars outside the chosen hours join no profile.
        if i >= part_end {
            continue;
        }
        if opens_session {
            session_open = part_open
                .or_else(|| sessions.session_open(bar.time, split))
                .unwrap_or(bar.time);
        }
        let elapsed = (bar.time - session_open).num_minutes().max(0);
        let bar_period = (elapsed / period_minutes) as usize;

        if is_new_session || bar_period != period_idx {
            if let Some(ref mut profile) = current_profile {
                let letter = letters[period_idx % letters.len()];
                let (lo, hi) = std::mem::replace(&mut touched, RangeInclusive::new(usize::MAX, 0))
                    .into_inner();
                add_tpo_letters(
                    profile,
                    row_changes.get_mut(lo..=hi).unwrap_or_default(),
                    first_key + lo as i64,
                    letter,
                    period_idx,
                    period_ts,
                );
            }
            period_idx = bar_period;
            period_ts = bar.time;
        }

        if is_new_session {
            // Save the current profile
            profiles.extend(current_profile.take());
            opened.push((
                opens_session && !split_between(&config.splits, previous, bar.time),
                part_date,
            ));

            // Start new profile
            current_profile = Some(TPOProfile {
                instrument_tick: tick,
                row_ticks,
                opening_price: bar.open,
                ..TPOProfile::new(bar.time, row)
            });
        }

        let (lo, hi) = (
            low_key.abs_diff(first_key) as usize,
            high_key.abs_diff(first_key) as usize + 1,
        );
        row_changes[lo] += 1;
        row_changes[hi] -= 1;
        touched = (*touched.start()).min(lo)..=(*touched.end()).max(hi);

        if let Some(ref mut profile) = current_profile {
            if bar.volume.is_finite() && bar.volume > 0.0 {
                profile.total_volume += bar.volume;
                if config.show_volume_profile {
                    let volume = bar.volume / (high_key.abs_diff(low_key) + 1) as f64;
                    for key in low_key..=high_key {
                        *profile.row_volume.entry(key).or_insert(0.0) += volume;
                    }
                }
            }
            profile.closing_price = bar.close;
            // Update profile high/low
            profile.profile_high = profile.profile_high.max(bar.high);
            profile.profile_low = profile.profile_low.min(bar.low);

            // Update initial balance (first N minutes)
            if elapsed < i64::from(config.initial_balance_minutes) {
                profile.initial_balance_high = profile.initial_balance_high.max(bar.high);
                profile.initial_balance_low = profile.initial_balance_low.min(bar.low);
            }
        }
        previous = Some(bar.time);
    }

    // Don't forget the last profile
    if let Some(mut profile) = current_profile.take() {
        let letter = letters[period_idx % letters.len()];
        let (lo, hi) = touched.into_inner();
        add_tpo_letters(
            &mut profile,
            row_changes.get_mut(lo..=hi).unwrap_or_default(),
            first_key + lo as i64,
            letter,
            period_idx,
            period_ts,
        );
        profiles.push(profile);
    }

    let mut combined: Vec<(TPOProfile, Option<chrono::NaiveDate>)> =
        Vec::with_capacity(profiles.len());
    let composite = |profile: &TPOProfile| {
        let open = &profile.session_date;
        config
            .composites
            .iter()
            .position(|range| range.contains(open))
    };
    let period = |profile: &TPOProfile, date: Option<chrono::NaiveDate>| {
        date.map_or_else(
            || config.period_key(&sessions, profile.session_date),
            |date| config.date_period_key(&sessions, date),
        )
    };
    for (profile, (opens, date)) in profiles.into_iter().zip(opened) {
        match combined.last_mut() {
            Some((last, last_date))
                if (composite(last).is_some() && composite(last) == composite(&profile))
                    || (opens
                        && period(last, *last_date).is_some()
                        && period(last, *last_date) == period(&profile, date)) =>
            {
                last.merge(profile)
            }
            _ => combined.push((profile, date)),
        }
    }
    combined
        .into_iter()
        .map(|(mut profile, _)| {
            calculate_profile_stats(&mut profile, config);
            profile
        })
        .collect()
}

/// Emit the union of a period's ranges once per row, then clear its endpoints.
fn add_tpo_letters(
    profile: &mut TPOProfile,
    row_changes: &mut [isize],
    first_key: i64,
    letter: char,
    period_idx: usize,
    ts: DateTime<Utc>,
) {
    let mut covering = 0;
    for (offset, change) in row_changes.iter_mut().enumerate() {
        covering += *change;
        *change = 0;
        if covering == 0 {
            continue;
        }
        let key = first_key + offset as i64;
        let price = key_to_price(key, profile.tick_size);
        profile.letters.push(TPOLetter {
            price,
            letter,
            period_idx,
            ts,
        });
        *profile.price_tpo_count.entry(key).or_insert(0) += 1;
    }
}

/// Calculate profile statistics (POC, Value Area, etc.)
fn calculate_profile_stats(profile: &mut TPOProfile, config: &TPOConfig) {
    if profile.price_tpo_count.is_empty() {
        return;
    }

    // Find POC (price with most TPOs); a tie takes the lower price
    let (poc_key, _) = profile
        .price_tpo_count
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .unwrap();
    profile.poc_price = key_to_price(*poc_key, profile.tick_size);

    // Calculate total TPOs
    let total_tpos: usize = profile.price_tpo_count.values().sum();

    // Calculate the Value Area (default 70% of TPOs) around the POC.
    //
    // This follows the canonical Market Profile expansion rule: start at the
    // POC, then repeatedly extend the contiguous range one row at a time,
    // always toward whichever adjacent side (the two rows immediately above
    // the current high, or the two rows immediately below the current low)
    // holds more TPOs; equal sums add one row on each side, as volume profiles
    // do. Counts are accumulated until the running total reaches
    // `value_area_pct` of all TPOs. The result is guaranteed contiguous and
    // centered on the POC, the usual market-profile convention.
    let target_tpos = (total_tpos as f64 * config.value_area_pct).ceil() as usize;
    let count_at = |key: i64| -> usize { *profile.price_tpo_count.get(&key).unwrap_or(&0) };

    let &min_key = profile.price_tpo_count.keys().min().unwrap();
    let &max_key = profile.price_tpo_count.keys().max().unwrap();

    let mut accumulated_tpos = count_at(*poc_key);
    let mut va_low_key = *poc_key;
    let mut va_high_key = *poc_key;

    // Two-rows-per-step look: pair the next row above the high with the row
    // beyond it, likewise below the low, then advance toward the heavier pair.
    while accumulated_tpos < target_tpos && (va_low_key > min_key || va_high_key < max_key) {
        let up_avail = va_high_key < max_key;
        let down_avail = va_low_key > min_key;

        let up_pair = if up_avail {
            count_at(va_high_key + 1) + count_at(va_high_key + 2)
        } else {
            0
        };
        let down_pair = if down_avail {
            count_at(va_low_key - 1) + count_at(va_low_key - 2)
        } else {
            0
        };

        // Prefer the heavier side; equal sums add one row on each side, as volume profiles do;
        // when only one side remains, take it.
        let go_up = match (up_avail, down_avail) {
            (true, true) if up_pair == down_pair => {
                va_low_key -= 1;
                va_high_key += 1;
                accumulated_tpos += count_at(va_low_key) + count_at(va_high_key);
                continue;
            }
            (true, true) => up_pair > down_pair,
            (true, false) => true,
            (false, true) => false,
            (false, false) => break,
        };

        if go_up {
            va_high_key += 1;
            accumulated_tpos += count_at(va_high_key);
            if accumulated_tpos < target_tpos && va_high_key < max_key {
                va_high_key += 1;
                accumulated_tpos += count_at(va_high_key);
            }
        } else {
            va_low_key -= 1;
            accumulated_tpos += count_at(va_low_key);
            if accumulated_tpos < target_tpos && va_low_key > min_key {
                va_low_key -= 1;
                accumulated_tpos += count_at(va_low_key);
            }
        }
    }

    profile.value_area_low = key_to_price(va_low_key, profile.tick_size);
    profile.value_area_high = key_to_price(va_high_key, profile.tick_size);

    // Find single prints (price levels with only 1 TPO)
    profile.single_prints = profile
        .price_tpo_count
        .iter()
        .filter(|(_, count)| **count == 1)
        .map(|(&key, _)| key_to_price(key, profile.tick_size))
        .collect();
    profile.single_prints.sort_by(f64::total_cmp);

    // Check for poor high/low (multiple TPOs at extreme)
    let high_key = profile.key(profile.profile_high);
    let low_key = profile.key(profile.profile_low);

    profile.has_poor_high = profile.price_tpo_count.get(&high_key).unwrap_or(&0) > &1;
    profile.has_poor_low = profile.price_tpo_count.get(&low_key).unwrap_or(&0) > &1;
}

/// Market profile shape classification.
///
/// The shape of a TPO profile conveys the type of market activity during
/// that session. Use [`TPOProfile::classify_shape`] to determine it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileShape {
    /// Normal distribution - balanced market
    Normal,
    /// Double distribution - potential breakout
    Double,
    /// P-shape - strong buying
    P,
    /// b-shape - strong selling
    B,
    /// D-shape - trending market
    D,
}

impl TPOProfile {
    /// Height of the initial balance, or `None` for a profile without one.
    pub fn initial_balance_range(&self) -> Option<f64> {
        (self.initial_balance_high != f64::MIN)
            .then_some(self.initial_balance_high - self.initial_balance_low)
    }

    /// TPOs of the rows below and of the rows above the point of control's row, `(below, above)`.
    /// The point of control's row counts in neither.
    pub fn tpo_counts_around_poc(&self) -> (usize, usize) {
        let poc = self.key(self.poc_price);
        let sum = |side: std::cmp::Ordering| -> usize {
            self.price_tpo_count
                .iter()
                .filter(|&(key, _)| key.cmp(&poc) == side)
                .map(|(_, count)| count)
                .sum()
        };
        (
            sum(std::cmp::Ordering::Less),
            sum(std::cmp::Ordering::Greater),
        )
    }

    /// Rotation factor: for each period after the first, +1 for a higher high and +1 for a higher
    /// low than the previous period's, -1 for a lower high and -1 for a lower low, summed. Highs
    /// and lows are compared on the profile's rows, so a move inside one row counts 0. A composite
    /// reads its periods in time order.
    pub fn rotation_factor(&self) -> i32 {
        let periods: Vec<(i64, i64)> = self
            .letters
            .chunk_by(|a, b| a.ts == b.ts)
            .map(|period| {
                let keys = period.iter().map(|letter| self.key(letter.price));
                (
                    keys.clone().max().unwrap_or_default(),
                    keys.min().unwrap_or_default(),
                )
            })
            .collect();
        periods
            .windows(2)
            .map(|pair| pair[1].0.cmp(&pair[0].0) as i32 + pair[1].1.cmp(&pair[0].1) as i32)
            .sum()
    }

    /// How far the profile's range reaches beyond its initial balance, `(below, above)`, in price,
    /// or `None` for a profile without one.
    pub fn range_extension(&self) -> Option<(f64, f64)> {
        self.initial_balance_range()?;
        Some((
            (self.initial_balance_low - self.profile_low).max(0.0),
            (self.profile_high - self.initial_balance_high).max(0.0),
        ))
    }

    /// Classify the profile shape
    pub fn classify_shape(&self) -> ProfileShape {
        let levels = self.price_levels();
        if levels.len() < 3 {
            return ProfileShape::Normal;
        }

        let total_tpos: usize = self.price_tpo_count.values().sum();
        let upper_third = &levels[..levels.len() / 3];
        let lower_third = &levels[levels.len() * 2 / 3..];

        let upper_tpos: usize = upper_third.iter().map(|p| self.tpo_count_at(*p)).sum();
        let lower_tpos: usize = lower_third.iter().map(|p| self.tpo_count_at(*p)).sum();

        let upper_pct = upper_tpos as f64 / total_tpos as f64;
        let lower_pct = lower_tpos as f64 / total_tpos as f64;

        if upper_pct > 0.5 && lower_pct < 0.2 {
            ProfileShape::P
        } else if lower_pct > 0.5 && upper_pct < 0.2 {
            ProfileShape::B
        } else if upper_pct > 0.35 && lower_pct > 0.35 {
            ProfileShape::Double
        } else if (upper_pct - lower_pct).abs() < 0.1 {
            ProfileShape::Normal
        } else {
            ProfileShape::D
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;
    use chrono::{Duration, TimeZone};

    #[test]
    fn initial_balance_spans_its_bars_whatever_their_sign() {
        // (minutes between bars, bars as (high, low), initial balance as (high, low)), tick 0.25
        // and the default 60-minute balance from 14:30 UTC
        type Case<'a> = (i64, &'a [(f64, f64)], (f64, f64));
        let cases: [Case; 3] = [
            // The first bar's high is zero
            (30, &[(0.0, -1.0), (-0.25, -0.5)], (0.0, -1.0)),
            // The running high reaches zero at the second bar
            (1, &[(-1.0, -2.0), (0.0, -1.5), (-0.5, -0.75)], (0.0, -2.0)),
            // Only the bars of the first hour count: not those at 60 and 90 minutes
            (
                30,
                &[
                    (102.0, 99.0),
                    (103.0, 100.0),
                    (104.0, 101.0),
                    (104.0, 102.0),
                ],
                (103.0, 99.0),
            ),
        ];
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap();
        let config = TPOConfig {
            tick_size: 0.25,
            ..Default::default()
        };
        for (spacing, bars, expected) in cases {
            let bars: Vec<_> = bars
                .iter()
                .enumerate()
                .map(|(i, &(high, low))| bar_at(start, spacing * i as i64, low, high, low, high))
                .collect();
            let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min1.into())[0];
            assert_eq!(
                (profile.initial_balance_high, profile.initial_balance_low),
                expected,
                "{bars:?}"
            );
        }
    }

    /// Build a single bar at a chosen offset from a fixed session start.
    fn bar_at(start: DateTime<Utc>, mins: i64, o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar {
            time: start + Duration::minutes(mins),
            open: o,
            high: h,
            low: l,
            close: c,
            volume: 1.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn each_period_counts_once_per_row() {
        // (bars as (minute, low, high), expected letters as (profile, period, letter, minute of ts, rows))
        type Case = (
            &'static [(i64, f64, f64)],
            &'static [(usize, usize, char, i64, RangeInclusive<i64>)],
        );
        let cases: [Case; 7] = [
            // One 30-minute bar: one TPO on each row it spans
            (&[(0, 100.0, 104.0)], &[(0, 0, 'A', 0, 100..=104)]),
            // Two bars of one period: their union, one TPO per row, stamped with the first bar
            (
                &[(0, 100.0, 102.0), (1, 101.0, 104.0)],
                &[(0, 0, 'A', 0, 100..=104)],
            ),
            // Rows between the bars of a period stay empty
            (
                &[(0, 103.0, 104.0), (1, 100.0, 101.0)],
                &[(0, 0, 'A', 0, 100..=101), (0, 0, 'A', 0, 103..=104)],
            ),
            // 95 minutes in is the fourth period, after two empty ones
            (
                &[(0, 100.0, 101.0), (95, 102.0, 103.0)],
                &[(0, 0, 'A', 0, 100..=101), (0, 3, 'D', 95, 102..=103)],
            ),
            // Periods follow the session start, not the bar that opened the previous period
            (
                &[(0, 100.0, 100.0), (45, 101.0, 101.0), (61, 102.0, 102.0)],
                &[
                    (0, 0, 'A', 0, 100..=100),
                    (0, 1, 'B', 45, 101..=101),
                    (0, 2, 'C', 61, 102..=102),
                ],
            ),
            // A new UTC date closes the open period in its own profile, even when both are period 0
            (
                &[(0, 100.0, 100.0), (900, 101.0, 101.0)],
                &[(0, 0, 'A', 0, 100..=100), (1, 0, 'A', 900, 101..=101)],
            ),
            // An inverted bar stays unusable even when both endpoints round to the same row.
            (
                &[(0, 100.0, 100.0), (900, 100.2, 100.1), (960, 101.0, 101.0)],
                &[(0, 0, 'A', 0, 100..=100), (1, 0, 'A', 960, 101..=101)],
            ),
        ];
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        for (bars, expected) in cases {
            let bars: Vec<Bar> = bars
                .iter()
                .map(|&(m, low, high)| bar_at(start, m, low, high, low, high))
                .collect();
            let profiles = to_tpo_profiles(&bars, &config, &Timeframe::Min1.into());
            let mut letters = Vec::new();
            let mut counts = Vec::new();
            for (i, profile) in profiles.iter().enumerate() {
                letters.extend(
                    profile
                        .letters
                        .iter()
                        .map(|l| (i, l.period_idx, l.letter, l.ts, profile.key(l.price))),
                );
                counts.extend(profile.price_tpo_count.iter().map(|(&k, &c)| (i, k, c)));
            }
            letters.sort_by_key(|&(i, period, _, _, key)| (i, period, key));
            counts.sort_unstable();
            let want: Vec<_> = expected
                .iter()
                .flat_map(|(i, period, letter, minute, rows)| {
                    let ts = start + Duration::minutes(*minute);
                    rows.clone().map(move |key| (*i, *period, *letter, ts, key))
                })
                .collect();
            assert_eq!(letters, want, "bars {bars:?}");
            // No row is shared between periods here, so every row counts one TPO
            let ones: Vec<_> = want.iter().map(|&(i, _, _, _, key)| (i, key, 1)).collect();
            assert_eq!(counts, ones, "bars {bars:?}");
        }
    }

    #[test]
    fn week_and_month_profiles_equal_composites_of_their_sessions() {
        let monday = Utc.with_ymd_and_hms(2024, 3, 4, 14, 30, 0).unwrap();
        let bars: Vec<_> = [
            (0, 0, 100.0, 102.0),
            (0, 30, 101.0, 103.0),
            (1, 0, 101.0, 101.0),
            (1, 30, 101.0, 102.0),
            (2, 0, 99.0, 100.0),
            (3, 0, 101.0, 104.0),
            (4, 0, 101.0, 101.0),
            (7, 0, 105.0, 105.0),
            (28, 0, 100.0, 100.0),
        ]
        .into_iter()
        .map(|(day, minute, low, high)| bar_at(monday, day * 1440 + minute, low, high, low, high))
        .collect();
        let clock = Timeframe::Min30.into();
        let config = TPOConfig {
            tick_size: 1.0,
            profile_period: TPOProfilePeriod::Week,
            ..Default::default()
        };
        let weekly = to_tpo_profiles(&bars, &config, &clock);
        assert_eq!(
            weekly.iter().map(|p| p.session_date).collect::<Vec<_>>(),
            [
                monday,
                monday + Duration::days(7),
                monday + Duration::days(28)
            ]
        );
        let first = &weekly[0];
        assert_eq!(first.closing_price, 101.0);
        // Counts 1,2,6,4,2,1 total 16. From POC 101, the upper pair wins 6 to 3;
        // adding 102 and 103 reaches the twelve-TPO target.
        assert_eq!(
            first
                .price_levels()
                .into_iter()
                .map(|price| (price, first.tpo_count_at(price)))
                .collect::<Vec<_>>(),
            [
                (104.0, 1),
                (103.0, 2),
                (102.0, 4),
                (101.0, 6),
                (100.0, 2),
                (99.0, 1)
            ]
        );
        assert_eq!(
            (first.poc_price, first.value_area_low, first.value_area_high),
            (101.0, 101.0, 103.0)
        );
        assert_eq!(first.single_prints, [99.0, 104.0]);
        assert_eq!(
            (first.initial_balance_low, first.initial_balance_high),
            (100.0, 103.0)
        );
        assert_eq!(
            first.letters.iter().map(|l| l.letter).collect::<String>(),
            "AAABBBABBAAAAAAA"
        );
        let composite = to_tpo_profiles(
            &bars,
            &TPOConfig {
                profile_period: TPOProfilePeriod::Session,
                composites: vec![monday..=monday + Duration::days(4)],
                ..config.clone()
            },
            &clock,
        );
        let merged = &composite[0];
        // Seven bars of volume 1 in the first week; merging sums the sessions' volumes.
        assert_eq!((first.total_volume, merged.total_volume), (7.0, 7.0));
        assert_eq!(
            (
                first.poc_price,
                first.value_area_low,
                first.value_area_high,
                first.initial_balance_low,
                first.initial_balance_high
            ),
            (
                merged.poc_price,
                merged.value_area_low,
                merged.value_area_high,
                merged.initial_balance_low,
                merged.initial_balance_high
            )
        );
        assert_eq!(first.single_prints, merged.single_prints);
        assert_eq!(first.price_tpo_count, merged.price_tpo_count);
        assert_eq!(
            first
                .letters
                .iter()
                .map(|l| (l.price, l.letter, l.period_idx, l.ts))
                .collect::<Vec<_>>(),
            merged
                .letters
                .iter()
                .map(|l| (l.price, l.letter, l.period_idx, l.ts))
                .collect::<Vec<_>>()
        );
        let monthly = to_tpo_profiles(
            &bars,
            &TPOConfig {
                profile_period: TPOProfilePeriod::Month,
                ..config
            },
            &clock,
        );
        assert_eq!(monthly.len(), 2);
        assert_eq!(monthly[1].session_date, monday + Duration::days(28));
        assert_eq!(monthly[0].letters.len(), 17);
        assert_eq!(monthly[0].closing_price, 105.0);
        assert_eq!(
            (
                monthly[0].poc_price,
                monthly[0].value_area_low,
                monthly[0].value_area_high
            ),
            (101.0, 101.0, 103.0)
        );
        assert_eq!(monthly[0].single_prints, [99.0, 104.0, 105.0]);
    }

    #[test]
    fn sessions_follow_the_clock_and_combine_over_composites() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let new_york = TimezoneId::fixed_offset(-5 * 3600).unwrap();
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        // 18:00 to 17:00 New York, so a session spans the UTC midnight
        let overnight = BarClock::new(Timeframe::Min30).with_session(
            new_york.clone(),
            Session::new([SessionWindow::new(
                [Sun, Mon, Tue, Wed, Thu],
                at(18, 0),
                at(17, 0),
            )]),
            false,
        );
        let stock = BarClock::new(Timeframe::Min30).with_session(
            new_york.clone(),
            Session::new([SessionWindow::new(weekdays, at(9, 30), at(16, 0))])
                .with_extended_windows([
                    SessionWindow::new(weekdays, at(4, 0), at(9, 30)),
                    SessionWindow::new(weekdays, at(16, 0), at(20, 0)),
                ]),
            true,
        );
        // 2024-03-05 is a Tuesday, 13:00 UTC is 08:00 in New York
        let start = Utc.with_ymd_and_hms(2024, 3, 5, 13, 0, 0).unwrap();
        let both_days = start..=start + Duration::days(1);
        // (case, clock, bar minutes from `start`, split, composites, splits, period and count,
        // profiles as (open, letters))
        type Case<'a> = (
            &'a str,
            BarClock,
            Vec<i64>,
            bool,
            Vec<RangeInclusive<DateTime<Utc>>>,
            Vec<i64>,
            (TPOProfilePeriod, u32),
            &'a [(i64, &'a str)],
        );
        let cases: [Case; 23] = [
            (
                "exchange day across UTC midnight",
                overnight,
                vec![600, 630, 690, 1950, 2040],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(600, "ABDt"), (2040, "A")],
            ),
            (
                "pre-market, regular and post-market in one day",
                stock.clone(),
                (0..19).map(|i| i * 30).collect(),
                false,
                vec![],
                vec![],
                // Pre-market opens at 04:00, eight periods before the first bar at 08:00
                (TPOProfilePeriod::Session, 1),
                &[(0, "IJKLMNOPQRSTUVWXYZa")],
            ),
            (
                "split at extended hours",
                stock.clone(),
                (0..19).map(|i| i * 30).collect(),
                true,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(0, "IJK"), (90, "ABCDEFGHIJKLM"), (480, "ABC")],
            ),
            (
                "split at overnight hours",
                BarClock::new(Timeframe::Min30).with_session(
                    new_york.clone(),
                    Session::new([SessionWindow::new(weekdays, at(9, 30), at(16, 0))])
                        .with_extended_windows([
                            SessionWindow::new(weekdays, at(4, 0), at(9, 30)),
                            SessionWindow::new(weekdays, at(16, 0), at(20, 0)),
                        ])
                        .with_overnight_windows([SessionWindow::new(
                            weekdays,
                            at(20, 0),
                            at(4, 0),
                        )]),
                    true,
                ),
                vec![720, 750, 1200, 1230, 1530],
                true,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(720, "AB"), (1200, "AB"), (1530, "A")],
            ),
            // Post-market opens where regular hours close, inside its all-day extended window
            (
                "split inside an extended window spanning regular hours",
                BarClock::new(Timeframe::Min30).with_session(
                    new_york.clone(),
                    Session::new([SessionWindow::new(weekdays, at(9, 30), at(16, 0))])
                        .with_extended_windows([SessionWindow::new(weekdays, at(4, 0), at(20, 0))]),
                    true,
                ),
                vec![540, 570],
                true,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(540, "CD")],
            ),
            // No extended hours divide a lunch break, so 13:00 counts from the 09:00 open
            (
                "split across a lunch break",
                BarClock::new(Timeframe::Min30).with_session(
                    new_york.clone(),
                    Session::new([
                        SessionWindow::new(weekdays, at(9, 0), at(11, 30)),
                        SessionWindow::new(weekdays, at(12, 30), at(15, 0)),
                    ]),
                    false,
                ),
                vec![300],
                true,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(300, "I")],
            ),
            (
                "hourly bars take one period each",
                BarClock::new(Timeframe::Hour1),
                vec![0, 60, 120],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(0, "ABC")],
            ),
            (
                "UTC days",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(0, "AB"), (1440, "AB")],
            ),
            (
                "UTC days merged",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470],
                false,
                vec![both_days],
                vec![],
                (TPOProfilePeriod::Session, 1),
                &[(0, "ABAB")],
            ),
            (
                "UTC day split at its third period",
                Timeframe::Min30.into(),
                vec![0, 30, 60, 90],
                false,
                vec![],
                vec![60],
                (TPOProfilePeriod::Session, 1),
                &[(0, "AB"), (60, "CD")],
            ),
            (
                "a split keeps the session's periods across a gap",
                Timeframe::Min30.into(),
                vec![0, 30, 90],
                false,
                vec![],
                vec![60],
                (TPOProfilePeriod::Session, 1),
                &[(0, "AB"), (90, "D")],
            ),
            (
                "a split between sessions changes nothing",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470],
                false,
                vec![],
                vec![1000],
                (TPOProfilePeriod::Session, 1),
                &[(0, "AB"), (1440, "AB")],
            ),
            (
                "UTC days by week",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 7200, 8640],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Week, 1),
                &[(0, "ABAA"), (8640, "A")],
            ),
            (
                "UTC days by month",
                Timeframe::Min30.into(),
                vec![0, 37440, 38880],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Month, 1),
                &[(0, "AA"), (38880, "A")],
            ),
            (
                "a split divides a week profile",
                Timeframe::Min30.into(),
                vec![0, 30, 60, 1440],
                false,
                vec![],
                vec![30],
                (TPOProfilePeriod::Week, 1),
                &[(0, "A"), (30, "BCA")],
            ),
            (
                "a week profile splits at a session open",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470, 2880],
                false,
                vec![],
                vec![1440],
                (TPOProfilePeriod::Week, 1),
                &[(0, "AB"), (1440, "ABA")],
            ),
            (
                "a month profile splits at a session open",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470, 2880],
                false,
                vec![],
                vec![1440],
                (TPOProfilePeriod::Month, 1),
                &[(0, "AB"), (1440, "ABA")],
            ),
            (
                "a composite joins week profiles",
                Timeframe::Min30.into(),
                vec![0, 8640],
                false,
                vec![start..=start + Duration::minutes(8640)],
                vec![],
                (TPOProfilePeriod::Week, 1),
                &[(0, "AA")],
            ),
            // Runs of days, weeks and months are counted from 1970, whatever bars are loaded.
            (
                "two UTC days",
                Timeframe::Min30.into(),
                vec![1440, 1470, 2880, 4320],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 2),
                &[(1440, "ABA"), (4320, "A")],
            ),
            (
                "two trading days span a weekend",
                stock.clone(),
                vec![4320, 8640, 10080],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Session, 2),
                &[(4320, "II"), (10080, "I")],
            ),
            (
                "two ISO weeks",
                Timeframe::Min30.into(),
                vec![0, 8640, 20160],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Week, 2),
                &[(0, "AA"), (20160, "A")],
            ),
            (
                "a quarter of months",
                Timeframe::Min30.into(),
                vec![0, 40320, 82080, 169920],
                false,
                vec![],
                vec![],
                (TPOProfilePeriod::Month, 3),
                &[(0, "A"), (40320, "AA"), (169920, "A")],
            ),
            (
                "week profiles keep whole sessions",
                stock,
                (0..19).map(|i| i * 30).collect(),
                true,
                vec![],
                vec![],
                (TPOProfilePeriod::Week, 1),
                &[(0, "IJKLMNOPQRSTUVWXYZa")],
            ),
        ];
        for (
            case,
            clock,
            minutes,
            split_sessions,
            composites,
            splits,
            (profile_period, profile_periods),
            expected,
        ) in cases
        {
            let bars: Vec<_> = minutes
                .iter()
                .map(|&m| bar_at(start, m, 100.0, 100.0, 100.0, 100.0))
                .collect();
            let config = TPOConfig {
                tick_size: 1.0,
                split_sessions,
                profile_period,
                profile_periods,
                composites,
                splits: splits
                    .into_iter()
                    .map(|m| start + Duration::minutes(m))
                    .collect(),
                ..Default::default()
            };
            let profiles: Vec<_> = to_tpo_profiles(&bars, &config, &clock)
                .iter()
                .map(|profile| {
                    let open = (profile.session_date - start).num_minutes();
                    let letters: String = profile.letters.iter().map(|l| l.letter).collect();
                    assert_eq!(profile.tpo_count_at(100.0), letters.len(), "{case}");
                    (open, letters)
                })
                .collect();
            let expected: Vec<_> = expected.iter().map(|&(m, l)| (m, l.to_owned())).collect();
            assert_eq!(profiles, expected, "{case}");
        }
    }

    #[test]
    fn periods_and_initial_balance_count_from_the_session_open() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        // Regular hours 09:30 to 16:00 New York (UTC-5); 2024-03-05 opens at 14:30 UTC.
        let clock = BarClock::new(Timeframe::Min5).with_session(
            TimezoneId::fixed_offset(-5 * 3600).unwrap(),
            Session::new([SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri],
                at(9, 30),
                at(16, 0),
            )]),
            false,
        );
        let open = Utc.with_ymd_and_hms(2024, 3, 5, 14, 30, 0).unwrap();
        // (case, bars as (minutes after the open, low, high), letters as (letter, row), initial
        // balance as (high, low)), tick 1, 30-minute periods
        type Case<'a> = (
            &'a str,
            &'a [(i64, f64, f64)],
            &'a [(char, f64)],
            (f64, f64),
        );
        let cases: [Case; 2] = [
            // First trade at 09:45. A is 09:30 to 10:00, B 10:00 to 10:30, C 10:30 to 11:00: the
            // 10:05 bar prints B and the 10:40 bar C, after the 09:30 to 10:30 initial balance.
            (
                "late first bar",
                &[(15, 100.0, 101.0), (35, 101.0, 102.0), (70, 103.0, 104.0)],
                &[
                    ('A', 100.0),
                    ('A', 101.0),
                    ('B', 101.0),
                    ('B', 102.0),
                    ('C', 103.0),
                    ('C', 104.0),
                ],
                (102.0, 100.0),
            ),
            // History that starts at 11:00, 90 minutes after the open: the fourth period, D, past
            // the initial balance.
            (
                "history from 11:00",
                &[(90, 100.0, 100.0), (125, 101.0, 101.0)],
                &[('D', 100.0), ('E', 101.0)],
                (f64::MIN, f64::MAX),
            ),
        ];
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        for (case, bars, letters, initial_balance) in cases {
            let bars: Vec<_> = bars
                .iter()
                .map(|&(m, low, high)| bar_at(open, m, low, high, low, high))
                .collect();
            let profiles = to_tpo_profiles(&bars, &config, &clock);
            assert_eq!(profiles.len(), 1, "{case}");
            let profile = &profiles[0];
            let mut placed: Vec<_> = profile
                .letters
                .iter()
                .map(|l| (l.letter, l.price))
                .collect();
            placed.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(placed, letters, "{case}");
            assert_eq!(
                (profile.initial_balance_high, profile.initial_balance_low),
                initial_balance,
                "{case}"
            );
        }
    }

    #[test]
    fn custom_letters_name_the_periods() {
        // Bars at minutes 0 and 45 fall in periods 0 and 1; an empty sequence means the default letters
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let bars = [0, 45].map(|m| bar_at(start, m, 100.0, 100.0, 100.0, 100.0));
        for (custom, want) in [(Some(""), ['A', 'B']), (Some("xy"), ['x', 'y'])] {
            let config = TPOConfig {
                tick_size: 1.0,
                custom_letters: custom.map(String::from),
                ..Default::default()
            };
            let profiles = to_tpo_profiles(&bars, &config, &Timeframe::Min1.into());
            let named: Vec<_> = profiles[0]
                .letters
                .iter()
                .map(|l| (l.period_idx, l.letter))
                .collect();
            assert_eq!(
                named,
                [(0, want[0]), (1, want[1])],
                "custom_letters {custom:?}"
            );
        }
    }

    #[test]
    fn poc_is_the_highest_count_bin() {
        // Three periods all overlap 102.0, fewer overlap the extremes, so 102.0
        // must accumulate the most TPOs and become the POC.
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let bars = vec![
            bar_at(start, 0, 100.0, 102.0, 100.0, 101.0),
            bar_at(start, 30, 101.0, 103.0, 101.0, 102.0),
            bar_at(start, 60, 102.0, 104.0, 102.0, 103.0),
        ];
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min1.into())[0];
        assert_eq!(profile.poc_price, 102.0);
    }

    #[test]
    fn ties_give_one_point_of_control_and_value_area_on_every_build() {
        // (case, half-hour periods from 14:30 UTC as (low, high), (POC, VAL, VAH), single prints)
        type Case<'a> = (&'a str, Vec<(f64, f64)>, (f64, f64, f64), Vec<f64>);
        let cases: [Case; 2] = [
            // A spans 100 to 109, B to D print 100 and E to G 109. Rows 100 and 109 hold 4 TPOs
            // and 101 to 108 one each, 16 in all, so the 70% value area needs 12. The tie takes
            // 100, from which the area can only grow upward, one TPO per row, reaching 12 at 108.
            (
                "tied point of control",
                [(100.0, 109.0)]
                    .into_iter()
                    .chain([(100.0, 100.0); 3])
                    .chain([(109.0, 109.0); 3])
                    .collect(),
                (100.0, 100.0, 108.0),
                (101..=108).map(f64::from).collect(),
            ),
            // A spans 100 to 104, B and C print 102: rows of 1, 1, 3, 1 and 1 TPOs, 7 in all, so
            // the value area needs 5. From 102 the two rows above and the two below hold 2 each,
            // so it adds 101 and 103 and stops at 5.
            (
                "tied value area expansion",
                vec![(100.0, 104.0), (102.0, 102.0), (102.0, 102.0)],
                (102.0, 101.0, 103.0),
                vec![100.0, 101.0, 103.0, 104.0],
            ),
        ];
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        for (case, periods, levels, single_prints) in cases {
            let bars: Vec<_> = periods
                .iter()
                .enumerate()
                .map(|(i, &(low, high))| bar_at(start, 30 * i as i64, low, high, low, high))
                .collect();
            // Each build hashes its rows with a new seed, so an unbroken tie varies between builds.
            for _ in 0..64 {
                let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min30.into())[0];
                assert_eq!(
                    (
                        profile.poc_price,
                        profile.value_area_low,
                        profile.value_area_high
                    ),
                    levels,
                    "{case}"
                );
                assert_eq!(profile.single_prints, single_prints, "{case}");
            }
        }
    }

    #[test]
    fn value_area_is_contiguous_and_covers_about_seventy_percent() {
        // Stack many overlapping periods to build a tall, well-populated profile.
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let mut bars = Vec::new();
        for p in 0..10 {
            // Each period drifts upward by one tick, all sharing the mid-band.
            let base = 100.0 + p as f64;
            bars.push(bar_at(start, p * 30, base, base + 5.0, base, base + 2.0));
        }
        let config = TPOConfig {
            tick_size: 1.0,
            value_area_pct: 0.70,
            ..Default::default()
        };
        let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min1.into())[0];

        // POC sits inside the value area.
        assert!(profile.is_in_value_area(profile.poc_price));
        assert!(profile.value_area_low <= profile.poc_price);
        assert!(profile.value_area_high >= profile.poc_price);

        // The value area is a contiguous price band: every level between VAL and
        // VAH (inclusive) that exists in the profile is part of one run.
        let total: usize = profile.price_tpo_count.values().sum();
        let va_lo = profile.key(profile.value_area_low);
        let va_hi = profile.key(profile.value_area_high);
        assert!(va_lo <= va_hi);

        let va_tpos: usize = profile
            .price_tpo_count
            .iter()
            .filter(|&(&k, _)| k >= va_lo && k <= va_hi)
            .map(|(_, &c)| c)
            .sum();

        // Covers at least the 70% target, and isn't the whole profile (the tails
        // outside the value area must hold something).
        let target = (total as f64 * 0.70).ceil() as usize;
        assert!(
            va_tpos >= target,
            "value area {va_tpos} should reach target {target} of {total}"
        );
        assert!(
            va_tpos < total,
            "value area should exclude the thin tails (got all {total})"
        );
    }

    #[test]
    fn derive_tick_size_picks_nice_steps_and_handles_degenerate_input() {
        // ~40 rows over a 200-wide range → a 5.0 step (40 rows * 5 = 200).
        assert_eq!(derive_tick_size(200.0, 40), 5.0);
        // Tiny range scales down to a sub-unit nice step.
        assert!(derive_tick_size(0.4, 40) > 0.0);
        // Degenerate inputs fall back to 1.0 rather than producing NaN/zero ticks.
        assert_eq!(derive_tick_size(0.0, 40), 1.0);
        assert_eq!(derive_tick_size(-5.0, 40), 1.0);
        assert_eq!(derive_tick_size(100.0, 0), 1.0);
        assert_eq!(derive_tick_size(f64::NAN, 40), 1.0);
    }

    #[test]
    fn single_bar_zero_range_does_not_panic_and_yields_a_poc() {
        // A flat bar (high == low) is the degenerate edge: one price level, one
        // TPO. The profile must still build with that level as the POC.
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let bars = vec![bar_at(start, 0, 100.0, 100.0, 100.0, 100.0)];
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profiles = to_tpo_profiles(&bars, &config, &Timeframe::Min1.into());
        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert_eq!(profile.poc_price, 100.0);
        assert_eq!(profile.value_area_low, 100.0);
        assert_eq!(profile.value_area_high, 100.0);
    }

    #[test]
    fn rows_per_series_are_bounded() {
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 9, 30, 0).unwrap();
        let bars = [bar_at(start, 0, 0.0, 1e6, 0.0, 1.0)];
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min1.into())[0];
        // 1e6 ticks widen to rows of 1,000 ticks: 0, 1,000, ..., 1e6
        assert_eq!((profile.tick_size, profile.letters.len()), (1_000.0, 1_001));
    }

    #[test]
    fn rows_of_several_ticks_hold_whole_ticks_from_a_multiple_of_the_row() {
        // (tick, ticks per row, tick numbers swept, quote divisor, prices with their row price and
        // span), worked out by hand from row = floor(round(price / tick) / ticks per row)
        type Case<'a> = (
            f64,
            u64,
            std::ops::Range<i64>,
            f64,
            &'a [(f64, f64, (f64, f64))],
        );
        let cases: [Case; 5] = [
            (
                0.01,
                2,
                10_000..20_000,
                100.0,
                &[(100.01, 100.0, (99.995, 100.015))],
            ),
            (
                0.01,
                2,
                -100..100,
                100.0,
                &[
                    (0.0, 0.0, (-0.005, 0.015)),
                    (-0.01, -0.02, (-0.025, -0.005)),
                ],
            ),
            (
                0.01,
                10,
                10_000..20_000,
                100.0,
                &[
                    (100.05, 100.0, (99.995, 100.095)),
                    (199.99, 199.9, (199.895, 199.995)),
                ],
            ),
            (
                0.25,
                4,
                -40..40,
                4.0,
                &[
                    (110.0, 110.0, (109.875, 110.875)),
                    // Between two ticks, and half a tick from the next row
                    (110.6, 110.0, (109.875, 110.875)),
                    (110.875, 111.0, (110.875, 111.875)),
                    (0.0, 0.0, (-0.125, 0.875)),
                    (-0.25, -1.0, (-1.125, -0.125)),
                ],
            ),
            (
                0.25,
                3,
                -12..12,
                4.0,
                &[
                    (110.0, 109.5, (109.375, 110.125)),
                    (0.0, 0.0, (-0.125, 0.625)),
                    (-0.25, -0.75, (-0.875, -0.125)),
                ],
            ),
        ];
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
        for (tick, row_ticks, sweep, divisor, rows) in cases {
            let profile = TPOProfile {
                instrument_tick: tick,
                row_ticks,
                ..TPOProfile::new(DateTime::UNIX_EPOCH, row_ticks as f64 * tick)
            };
            let mut held: HashMap<u64, u64> = HashMap::new();
            for n in sweep.clone() {
                let price = n as f64 / divisor;
                let (low, high) = profile.row_span(price);
                assert!(
                    low < price && price < high,
                    "{price} on {tick} x {row_ticks}"
                );
                *held.entry(profile.row_price(price).to_bits()).or_insert(0) += 1;
            }
            assert!(
                held.values().all(|&ticks| ticks == row_ticks),
                "uneven rows on {tick} x {row_ticks}"
            );
            assert_eq!(
                held.len() as u64 * row_ticks,
                (sweep.end - sweep.start) as u64,
                "{tick} x {row_ticks}"
            );
            for &(price, row, (low, high)) in rows {
                let at = (tick, row_ticks, price);
                assert!(close(profile.row_price(price), row), "{at:?}");
                let span = profile.row_span(price);
                assert!(close(span.0, low) && close(span.1, high), "{at:?} {span:?}");
                let own = profile.row_price(price);
                assert_eq!(profile.row_price(own), own, "{at:?}");
            }
        }

        // Past 2^50 ticks a row's price can key the row below, so the rows count as one tick.
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap();
        let bars = [bar_at(start, 0, 1e4, 1e4 + 1e-4, 1e4, 1e4)];
        let config = TPOConfig {
            tick_size: 1e-6,
            instrument_tick: Some(1e-12),
            ..Default::default()
        };
        let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min1.into())[0];
        assert_eq!(profile.letters.len(), 101);
        assert!(
            profile
                .letters
                .iter()
                .all(|letter| profile.row_price(letter.price) == letter.price)
        );
    }

    #[test]
    fn session_statistics_follow_a_hand_counted_session() {
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap();
        let mut bars = [
            bar_at(start, 0, 101.0, 103.0, 100.0, 102.0),
            bar_at(start, 30, 102.0, 104.0, 101.0, 103.0),
            bar_at(start, 60, 104.0, 106.0, 103.0, 105.0),
            bar_at(start, 90, 104.0, 105.0, 99.0, 100.0),
        ];
        for (bar, volume) in bars.iter_mut().zip([400.0, 600.0, 700.0, 800.0]) {
            bar.volume = volume;
        }
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let clock = Timeframe::Min30.into();
        // Rows 99 to 106 hold 1, 2, 3, 3, 4, 3, 2 and 1 TPOs, 19 in all. The value area takes 14:
        // from 103 (4), the two rows below (6) beat the two above (5), then the two above (5) beat
        // the rest below (3).
        let session = to_tpo_profiles(&bars, &config, &clock).remove(0);
        assert_eq!(session.poc_price, 103.0);
        assert_eq!(
            (session.value_area_low, session.value_area_high),
            (101.0, 105.0)
        );
        assert_eq!(session.initial_balance_range(), Some(4.0));
        assert_eq!(session.tpo_counts_around_poc(), (9, 6));
        // B over A +2, C over B +2, D under C -2.
        assert_eq!(session.rotation_factor(), 2);
        assert_eq!(session.range_extension(), Some((1.0, 2.0)));
        // Summed without the volume profile; unusable volumes add none.
        assert_eq!(session.total_volume, 2500.0);
        for volume in [f64::NAN, f64::INFINITY, -5.0, 0.0] {
            let mut bars = bars.clone();
            bars[3].volume = volume;
            let session = to_tpo_profiles(&bars, &config, &clock).remove(0);
            assert_eq!(session.total_volume, 1700.0, "volume {volume}");
        }

        // After A and B: rows 100 to 104 hold 1, 2, 2, 2 and 1, and a tie takes the lower price.
        let partial = to_tpo_profiles(&bars[..2], &config, &clock).remove(0);
        assert_eq!(partial.poc_price, 101.0);
        assert_eq!(partial.initial_balance_range(), Some(4.0));
        assert_eq!(partial.tpo_counts_around_poc(), (1, 5));
        assert_eq!(partial.rotation_factor(), 2);
        assert_eq!(partial.range_extension(), Some((0.0, 0.0)));
        assert_eq!(partial.total_volume, 1000.0);
    }
}
