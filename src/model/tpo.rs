//! TPO (Time Price Opportunity) / Market Profile chart model.
//!
//! Market Profile charts show price distribution over time by placing a
//! letter (A, B, C, ...) at each price level touched during successive
//! time periods (typically 30-minute brackets). The resulting profile
//! reveals key structural levels:
//!
//! - **POC** (Point of Control) -- price with the most TPOs (highest volume).
//! - **Value Area** -- price range containing 70% of TPO activity.
//! - **Initial Balance** -- range established in the first hour of trading.
//! - **Single Prints** -- levels with only one TPO, often acting as
//!   support/resistance.
//!
//! Use [`to_tpo_profiles`] to transform OHLCV bars into per-session
//! [`TPOProfile`]s.

use crate::model::Bar;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Letter sequence for TPO periods (A, B, C... Z, a, b...)
const TPO_LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// A single TPO letter placed at a price level during one time period.
#[derive(Debug, Clone)]
pub struct TPOLetter {
    /// Price level (quantised to `tick_size`).
    pub price: f64,
    /// The letter assigned to this time period (A, B, C, ...).
    pub letter: char,
    /// Zero-based index of the time period within the session.
    pub period_idx: usize,
    /// Timestamp of the bar that generated this letter.
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
    /// All TPO letters in this profile
    pub letters: Vec<TPOLetter>,
    /// TPO count at each price level
    pub price_tpo_count: HashMap<i64, usize>,
    /// Point of Control - price with most TPOs
    pub poc_price: f64,
    /// Value Area High (70% of volume)
    pub value_area_high: f64,
    /// Value Area Low (70% of volume)
    pub value_area_low: f64,
    /// Initial Balance High (first hour)
    pub initial_balance_high: f64,
    /// Initial Balance Low (first hour)
    pub initial_balance_low: f64,
    /// Profile high
    pub profile_high: f64,
    /// Profile low
    pub profile_low: f64,
    /// Opening price
    pub opening_price: f64,
    /// Single prints (price levels with only 1 TPO)
    pub single_prints: Vec<f64>,
    /// Poor highs (multiple TPOs at high)
    pub has_poor_high: bool,
    /// Poor lows (multiple TPOs at low)
    pub has_poor_low: bool,
}

/// Configuration for TPO chart generation and rendering.
#[derive(Debug, Clone)]
pub struct TPOConfig {
    /// Tick size for price grouping (e.g., 0.25 for ES futures)
    pub tick_size: f64,
    /// Duration of each period in minutes (typically 30)
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
    /// Split by session (RTH vs ETH)
    pub split_sessions: bool,
    /// Custom letters sequence
    pub custom_letters: Option<String>,
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
            show_single_prints: true,
            show_opening_range: true,
            split_sessions: false,
            custom_letters: None,
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
    /// Create a new empty TPO profile
    pub fn new(session_date: DateTime<Utc>) -> Self {
        Self {
            session_date,
            letters: Vec::new(),
            price_tpo_count: HashMap::new(),
            poc_price: 0.0,
            value_area_high: 0.0,
            value_area_low: 0.0,
            initial_balance_high: 0.0,
            initial_balance_low: 0.0,
            profile_high: f64::MIN,
            profile_low: f64::MAX,
            opening_price: 0.0,
            single_prints: Vec::new(),
            has_poor_high: false,
            has_poor_low: false,
        }
    }

    /// Get the TPO count at a specific price level
    pub fn tpo_count_at(&self, price: f64, tick_size: f64) -> usize {
        let price_key = price_to_key(price, tick_size);
        *self.price_tpo_count.get(&price_key).unwrap_or(&0)
    }

    /// Get all unique price levels with TPOs
    pub fn price_levels(&self, tick_size: f64) -> Vec<f64> {
        let mut levels: Vec<f64> = self
            .price_tpo_count
            .keys()
            .map(|&k| key_to_price(k, tick_size))
            .collect();
        levels.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
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

    /// Get the letters at a specific price level
    pub fn letters_at(&self, price: f64, tick_size: f64) -> Vec<&TPOLetter> {
        let price_key = price_to_key(price, tick_size);
        self.letters
            .iter()
            .filter(|l| price_to_key(l.price, tick_size) == price_key)
            .collect()
    }
}

/// Convert price to integer key for HashMap
fn price_to_key(price: f64, tick_size: f64) -> i64 {
    (price / tick_size).round() as i64
}

/// Convert integer key back to price
fn key_to_price(key: i64, tick_size: f64) -> f64 {
    key as f64 * tick_size
}

/// Transform bars into TPO profiles
///
/// # Arguments
/// * `bars` - Source OHLCV bars
/// * `config` - TPO configuration
///
/// # Returns
/// Vector of TPO profiles, one per session
pub fn to_tpo_profiles(bars: &[Bar], config: &TPOConfig) -> Vec<TPOProfile> {
    if bars.is_empty() {
        return Vec::new();
    }

    let letters = config
        .custom_letters
        .as_deref()
        .unwrap_or(TPO_LETTERS)
        .chars()
        .collect::<Vec<_>>();

    let mut profiles: Vec<TPOProfile> = Vec::new();
    let mut current_profile: Option<TPOProfile> = None;
    let mut current_session_start: Option<DateTime<Utc>> = None;
    let mut period_idx = 0;
    let mut last_period_start: Option<DateTime<Utc>> = None;

    for bar in bars {
        // Check if we need to start a new session (new day)
        let session_date = bar.time.date_naive();
        let is_new_session = current_session_start
            .map(|s| s.date_naive() != session_date)
            .unwrap_or(true);

        if is_new_session {
            // Save the current profile
            if let Some(mut profile) = current_profile.take() {
                calculate_profile_stats(&mut profile, config);
                profiles.push(profile);
            }

            // Start new profile
            current_profile = Some(TPOProfile::new(bar.time));
            current_session_start = Some(bar.time);
            period_idx = 0;
            last_period_start = Some(bar.time);

            // Set opening price
            if let Some(ref mut profile) = current_profile {
                profile.opening_price = bar.open;
            }
        }

        // Check if we need to advance to a new period
        if let Some(period_start) = last_period_start {
            let elapsed_minutes = (bar.time - period_start).num_minutes();
            if elapsed_minutes >= config.period_minutes as i64 {
                period_idx += 1;
                last_period_start = Some(bar.time);
            }
        }

        // Get the letter for this period
        let letter = letters[period_idx % letters.len()];

        // Add TPO letters for all price levels touched by this bar
        if let Some(ref mut profile) = current_profile {
            add_tpo_letters(profile, bar, letter, period_idx, config.tick_size);

            // Update profile high/low
            profile.profile_high = profile.profile_high.max(bar.high);
            profile.profile_low = profile.profile_low.min(bar.low);

            // Update initial balance (first N minutes)
            if let Some(session_start) = current_session_start {
                let elapsed = (bar.time - session_start).num_minutes();
                if elapsed < config.initial_balance_minutes as i64 {
                    if profile.initial_balance_high == 0.0 {
                        profile.initial_balance_high = bar.high;
                        profile.initial_balance_low = bar.low;
                    } else {
                        profile.initial_balance_high = profile.initial_balance_high.max(bar.high);
                        profile.initial_balance_low = profile.initial_balance_low.min(bar.low);
                    }
                }
            }
        }
    }

    // Don't forget the last profile
    if let Some(mut profile) = current_profile.take() {
        calculate_profile_stats(&mut profile, config);
        profiles.push(profile);
    }

    profiles
}

/// Add TPO letters for a bar
fn add_tpo_letters(
    profile: &mut TPOProfile,
    bar: &Bar,
    letter: char,
    period_idx: usize,
    tick_size: f64,
) {
    let low_key = price_to_key(bar.low, tick_size);
    let high_key = price_to_key(bar.high, tick_size);

    for key in low_key..=high_key {
        let price = key_to_price(key, tick_size);

        profile.letters.push(TPOLetter {
            price,
            letter,
            period_idx,
            ts: bar.time,
        });

        *profile.price_tpo_count.entry(key).or_insert(0) += 1;
    }
}

/// Calculate profile statistics (POC, Value Area, etc.)
fn calculate_profile_stats(profile: &mut TPOProfile, config: &TPOConfig) {
    if profile.price_tpo_count.is_empty() {
        return;
    }

    // Find POC (price with most TPOs)
    let (poc_key, _) = profile
        .price_tpo_count
        .iter()
        .max_by_key(|(_, count)| *count)
        .unwrap();
    profile.poc_price = key_to_price(*poc_key, config.tick_size);

    // Calculate total TPOs
    let total_tpos: usize = profile.price_tpo_count.values().sum();

    // Calculate Value Area (70% of TPOs centered around POC)
    let target_tpos = (total_tpos as f64 * config.value_area_pct).ceil() as usize;

    // Sort price levels by distance from POC
    let mut sorted_levels: Vec<(i64, usize)> = profile
        .price_tpo_count
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    sorted_levels.sort_by_key(|(k, _)| (*k - *poc_key).abs());

    // Accumulate TPOs starting from POC
    let mut accumulated_tpos = 0;
    let mut va_low_key = *poc_key;
    let mut va_high_key = *poc_key;

    for (key, count) in sorted_levels {
        accumulated_tpos += count;
        va_low_key = va_low_key.min(key);
        va_high_key = va_high_key.max(key);

        if accumulated_tpos >= target_tpos {
            break;
        }
    }

    profile.value_area_low = key_to_price(va_low_key, config.tick_size);
    profile.value_area_high = key_to_price(va_high_key, config.tick_size);

    // Find single prints (price levels with only 1 TPO)
    profile.single_prints = profile
        .price_tpo_count
        .iter()
        .filter(|(_, count)| **count == 1)
        .map(|(&key, _)| key_to_price(key, config.tick_size))
        .collect();

    // Check for poor high/low (multiple TPOs at extreme)
    let high_key = price_to_key(profile.profile_high, config.tick_size);
    let low_key = price_to_key(profile.profile_low, config.tick_size);

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
    pub fn classify_shape(&self, tick_size: f64) -> ProfileShape {
        let levels = self.price_levels(tick_size);
        if levels.len() < 3 {
            return ProfileShape::Normal;
        }

        let total_tpos: usize = self.price_tpo_count.values().sum();
        let upper_third = &levels[..levels.len() / 3];
        let lower_third = &levels[levels.len() * 2 / 3..];

        let upper_tpos: usize = upper_third
            .iter()
            .map(|p| self.tpo_count_at(*p, tick_size))
            .sum();
        let lower_tpos: usize = lower_third
            .iter()
            .map(|p| self.tpo_count_at(*p, tick_size))
            .sum();

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
    use chrono::Duration;

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        vec![
            Bar {
                time: start,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(30),
                open: 101.0,
                high: 103.0,
                low: 100.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(60),
                open: 102.0,
                high: 104.0,
                low: 101.0,
                close: 103.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(90),
                open: 103.0,
                high: 104.0,
                low: 102.0,
                close: 103.5,
                volume: 1000.0,
            },
        ]
    }

    #[test]
    fn test_tpo_profile_creation() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profiles = to_tpo_profiles(&bars, &config);

        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];

        // Should have letters at multiple price levels
        assert!(!profile.letters.is_empty());
        assert!(!profile.price_tpo_count.is_empty());
    }

    #[test]
    fn test_tpo_poc_calculation() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profiles = to_tpo_profiles(&bars, &config);

        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];

        // POC should be within the price range
        assert!(profile.poc_price >= 99.0 && profile.poc_price <= 104.0);
    }

    #[test]
    fn test_tpo_value_area() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            value_area_pct: 0.70,
            ..Default::default()
        };
        let profiles = to_tpo_profiles(&bars, &config);

        let profile = &profiles[0];

        // Value area should contain POC
        assert!(profile.is_in_value_area(profile.poc_price));
        // Value area high should be >= POC
        assert!(profile.value_area_high >= profile.poc_price);
        // Value area low should be <= POC
        assert!(profile.value_area_low <= profile.poc_price);
    }

    #[test]
    fn test_tpo_initial_balance() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            initial_balance_minutes: 60,
            ..Default::default()
        };
        let profiles = to_tpo_profiles(&bars, &config);

        let profile = &profiles[0];

        // Initial balance should be set from first 60 minutes
        assert!(profile.initial_balance_high > 0.0);
        assert!(profile.initial_balance_low > 0.0);
        assert!(profile.initial_balance_high >= profile.initial_balance_low);
    }

    #[test]
    fn test_price_key_conversion() {
        let price = 100.5;
        let tick_size = 0.25;

        let key = price_to_key(price, tick_size);
        let back = key_to_price(key, tick_size);

        assert!((back - 100.5).abs() < 0.01);
    }
}
