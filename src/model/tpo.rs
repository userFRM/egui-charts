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
use crate::model::{Bar, BarClock, SessionProvider};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::ops::RangeInclusive;

/// Letter sequence for TPO periods (A, B, C... Z, a, b...)
const TPO_LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

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
    /// Single prints: the prices of the rows with exactly one TPO, in ascending order
    pub single_prints: Vec<f64>,
    /// Poor highs (multiple TPOs at high)
    pub has_poor_high: bool,
    /// Poor lows (multiple TPOs at low)
    pub has_poor_low: bool,
}

/// Configuration for TPO chart generation and rendering.
#[derive(Debug, Clone)]
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
    /// Color mode for TPO letters
    pub color_mode: TPOColorMode,
    /// Show POC line
    pub show_poc: bool,
    /// Show Value Area
    pub show_value_area: bool,
    /// Show Initial Balance
    pub show_initial_balance: bool,
    /// Show single prints
    pub show_single_prints: bool,
    /// Highlight opening range
    pub show_opening_range: bool,
    /// Split each session where it moves between extended and regular hours, giving separate
    /// pre-market, regular and post-market profiles
    pub split_sessions: bool,
    /// Custom letters sequence; an empty string uses the default letters
    pub custom_letters: Option<String>,
    /// Sessions to combine: the profiles whose sessions open within one range form a single
    /// profile. Adding a range over adjacent sessions merges them, removing it splits them back,
    /// and a range over many sessions builds a composite profile.
    pub composites: Vec<RangeInclusive<DateTime<Utc>>>,
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
            color_mode: TPOColorMode::ByPeriod,
            show_poc: true,
            show_value_area: true,
            show_initial_balance: true,
            show_single_prints: false,
            show_opening_range: true,
            split_sessions: false,
            custom_letters: None,
            composites: Vec::new(),
            latest_session_only: false,
            instrument_tick: None,
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
    /// Single color
    Solid,
    /// Color by value area
    ByValueArea,
    /// Color by initial balance
    ByInitialBalance,
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
            poc_price: 0.0,
            value_area_high: 0.0,
            value_area_low: 0.0,
            initial_balance_high: f64::MIN,
            initial_balance_low: f64::MAX,
            profile_high: f64::MIN,
            profile_low: f64::MAX,
            opening_price: 0.0,
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
        self.letters.extend(later.letters);
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
fn key_to_price(key: i64, tick_size: f64) -> f64 {
    key as f64 * tick_size
}

/// First usable bars of the profiles before composites, shared by the model, cache and menus.
pub(crate) fn session_starts(
    bars: &[Bar],
    sessions: &crate::model::ExchangeSessionProvider,
    split_sessions: bool,
) -> Vec<usize> {
    let mut starts: Vec<_> = std::iter::once(0)
        .chain(sessions.session_breaks(bars).into_iter().map(|(i, _)| i))
        .collect();
    if split_sessions {
        let extended: Vec<_> = bars
            .iter()
            .map(|bar| sessions.is_extended(bar.time))
            .collect();
        starts.extend((1..bars.len()).filter(|&i| extended[i] != extended[i - 1]));
    }
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

/// Transform bars into TPO profiles
///
/// Sessions are the trading days of `clock`: the exchange sessions it carries,
/// or calendar days in its time zone, split further at the moves between
/// extended and regular hours with `config.split_sessions` and combined over
/// `config.composites`. A session's periods are whole multiples of
/// `config.period_minutes`, or of the clock's interval when longer, since its
/// open: that of its trading day with intraday bars and trading hours, or of
/// its stretch of extended or regular hours when split, whatever time its
/// first bar has. Calendar days, daily and longer bars and bars split off
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
/// Vector of TPO profiles, one per session
pub fn to_tpo_profiles(bars: &[Bar], config: &TPOConfig, clock: &BarClock) -> Vec<TPOProfile> {
    if bars.is_empty() {
        return Vec::new();
    }

    let letters = config
        .custom_letters
        .as_deref()
        .filter(|letters| !letters.is_empty())
        .unwrap_or(TPO_LETTERS)
        .chars()
        .collect::<Vec<_>>();

    let mut profiles: Vec<TPOProfile> = Vec::new();
    let mut current_profile: Option<TPOProfile> = None;
    let mut session_open = bars[0].time;
    // The open period: its index, first bar time and row-range endpoints
    let mut period_idx = 0;
    let mut period_ts = bars[0].time;
    let interval_minutes = clock.timeframe().map_or(0, |t| t.duration_ms() / 60_000);
    let period_minutes = i64::from(config.period_minutes.max(1)).max(interval_minutes);
    let sessions = clock.sessions();
    let mut starts = session_starts(bars, &sessions, config.split_sessions)
        .into_iter()
        .peekable();
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

    for (i, bar) in bars.iter().enumerate() {
        let Some((low_key, high_key)) = bar_level_keys(bar, tick, row_ticks) else {
            continue;
        };

        // A session starting at a skipped bar starts at the next usable one.
        let mut is_new_session = current_profile.is_none();
        while starts.next_if(|&start| start <= i).is_some() {
            is_new_session = true;
        }
        if is_new_session {
            session_open = sessions
                .session_open(bar.time, config.split_sessions)
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
            // Update profile high/low
            profile.profile_high = profile.profile_high.max(bar.high);
            profile.profile_low = profile.profile_low.min(bar.low);

            // Update initial balance (first N minutes)
            if elapsed < i64::from(config.initial_balance_minutes) {
                profile.initial_balance_high = profile.initial_balance_high.max(bar.high);
                profile.initial_balance_low = profile.initial_balance_low.min(bar.low);
            }
        }
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

    let mut combined: Vec<TPOProfile> = Vec::with_capacity(profiles.len());
    let composite = |profile: &TPOProfile| {
        let open = &profile.session_date;
        config
            .composites
            .iter()
            .position(|range| range.contains(open))
    };
    for profile in profiles {
        match combined.last_mut() {
            Some(last) if composite(last).is_some() && composite(last) == composite(&profile) => {
                last.merge(profile)
            }
            _ => combined.push(profile),
        }
    }
    for profile in &mut combined {
        calculate_profile_stats(profile, config);
    }
    combined
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
        // (case, clock, bar minutes from `start`, split, composites, profiles as (open, letters))
        type Case<'a> = (
            &'a str,
            BarClock,
            Vec<i64>,
            bool,
            Vec<RangeInclusive<DateTime<Utc>>>,
            &'a [(i64, &'a str)],
        );
        let cases: [Case; 8] = [
            (
                "exchange day across UTC midnight",
                overnight,
                vec![600, 630, 690, 1950, 2040],
                false,
                vec![],
                &[(600, "ABDt"), (2040, "A")],
            ),
            (
                "pre-market, regular and post-market in one day",
                stock.clone(),
                (0..19).map(|i| i * 30).collect(),
                false,
                vec![],
                // Pre-market opens at 04:00, eight periods before the first bar at 08:00
                &[(0, "IJKLMNOPQRSTUVWXYZa")],
            ),
            (
                "split at extended hours",
                stock,
                (0..19).map(|i| i * 30).collect(),
                true,
                vec![],
                &[(0, "IJK"), (90, "ABCDEFGHIJKLM"), (480, "ABC")],
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
                &[(300, "I")],
            ),
            (
                "hourly bars take one period each",
                BarClock::new(Timeframe::Hour1),
                vec![0, 60, 120],
                false,
                vec![],
                &[(0, "ABC")],
            ),
            (
                "UTC days",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470],
                false,
                vec![],
                &[(0, "AB"), (1440, "AB")],
            ),
            (
                "UTC days merged",
                Timeframe::Min30.into(),
                vec![0, 30, 1440, 1470],
                false,
                vec![both_days],
                &[(0, "ABAB")],
            ),
        ];
        for (case, clock, minutes, split_sessions, composites, expected) in cases {
            let bars: Vec<_> = minutes
                .iter()
                .map(|&m| bar_at(start, m, 100.0, 100.0, 100.0, 100.0))
                .collect();
            let config = TPOConfig {
                tick_size: 1.0,
                split_sessions,
                composites,
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
}
