//! Timeframe (bar aggregation interval) for financial charts.
//!
//! A [`Timeframe`] specifies how long each bar/candle represents.  The library
//! ships with standard presets from 100 ms up to 1 month, and a [`Custom`](Timeframe::Custom)
//! variant for arbitrary intervals.
//!
//! # Parsing
//!
//! Timeframes can be parsed from human-readable strings via [`FromStr`]:
//!
//! ```
//! use egui_charts::model::Timeframe;
//!
//! let tf: Timeframe = "15min".parse().unwrap();
//! assert_eq!(tf, Timeframe::Min15);
//!
//! let custom: Timeframe = "45s".parse().unwrap();
//! assert!(custom.is_custom());
//! ```

use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

/// Timeframe for bar aggregation.
///
/// Each variant represents a fixed time interval over which OHLCV data is aggregated
/// into a single [`Bar`](super::Bar). The default timeframe is [`Min1`](Timeframe::Min1).
///
/// Timeframes are divided into tiers:
///
/// | Tier         | Variants                                               |
/// |-------------|-------------------------------------------------------|
/// | Millisecond | `Ms100`, `Ms250`, `Ms500`                              |
/// | Second      | `Sec1` .. `Sec30`                                      |
/// | Minute      | `Min1`, `Min5`, `Min15`, `Min30`                       |
/// | Hourly      | `Hour1`, `Hour4`                                       |
/// | Daily+      | `Day1`, `Week1`, `Month1`                              |
/// | Custom      | `Custom(seconds)` -- any user-defined interval          |
///
/// # Example
///
/// ```
/// use egui_charts::model::Timeframe;
///
/// let tf = Timeframe::Hour4;
/// assert_eq!(tf.duration_ms(), 14_400_000);
/// assert_eq!(tf.to_string(), "4h");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    // Millisecond timeframes (for high-frequency trading)
    /// 100-millisecond bars (HFT).
    Ms100,
    /// 250-millisecond bars (HFT).
    Ms250,
    /// 500-millisecond bars (HFT).
    Ms500,

    // Second timeframes
    /// 1-second bars.
    Sec1,
    /// 2-second bars.
    Sec2,
    /// 5-second bars.
    Sec5,
    /// 10-second bars.
    Sec10,
    /// 30-second bars.
    Sec30,

    // Minute timeframes
    /// 1-minute bars (default).
    Min1,
    /// 5-minute bars.
    Min5,
    /// 15-minute bars.
    Min15,
    /// 30-minute bars.
    Min30,

    // Hourly timeframes
    /// 1-hour bars.
    Hour1,
    /// 4-hour bars.
    Hour4,

    // Daily and longer timeframes
    /// Daily bars.
    Day1,
    /// Weekly bars.
    Week1,
    /// Monthly bars (approximated as 30 days for duration calculations).
    Month1,

    /// Custom timeframe defined in seconds.
    /// Allows user-defined intervals (e.g., 45s, 3min, 2h).
    Custom(u64),
}

impl Timeframe {
    /// Returns the canonical string label for this timeframe (e.g. `"1min"`, `"4h"`, `"1D"`).
    pub fn as_str(&self) -> Cow<'static, str> {
        match self {
            Timeframe::Ms100 => Cow::Borrowed("100ms"),
            Timeframe::Ms250 => Cow::Borrowed("250ms"),
            Timeframe::Ms500 => Cow::Borrowed("500ms"),
            Timeframe::Sec1 => Cow::Borrowed("1s"),
            Timeframe::Sec2 => Cow::Borrowed("2s"),
            Timeframe::Sec5 => Cow::Borrowed("5s"),
            Timeframe::Sec10 => Cow::Borrowed("10s"),
            Timeframe::Sec30 => Cow::Borrowed("30s"),
            Timeframe::Min1 => Cow::Borrowed("1min"),
            Timeframe::Min5 => Cow::Borrowed("5min"),
            Timeframe::Min15 => Cow::Borrowed("15min"),
            Timeframe::Min30 => Cow::Borrowed("30min"),
            Timeframe::Hour1 => Cow::Borrowed("1h"),
            Timeframe::Hour4 => Cow::Borrowed("4h"),
            Timeframe::Day1 => Cow::Borrowed("1D"),
            Timeframe::Week1 => Cow::Borrowed("1W"),
            Timeframe::Month1 => Cow::Borrowed("1M"),
            Timeframe::Custom(seconds) => Cow::Owned(format_custom_seconds(*seconds)),
        }
    }

    /// Returns all preset (non-custom) timeframes in ascending order.
    pub fn all() -> Vec<Timeframe> {
        vec![
            Timeframe::Ms100,
            Timeframe::Ms250,
            Timeframe::Ms500,
            Timeframe::Sec1,
            Timeframe::Sec2,
            Timeframe::Sec5,
            Timeframe::Sec10,
            Timeframe::Sec30,
            Timeframe::Min1,
            Timeframe::Min5,
            Timeframe::Min15,
            Timeframe::Min30,
            Timeframe::Hour1,
            Timeframe::Hour4,
            Timeframe::Day1,
            Timeframe::Week1,
            Timeframe::Month1,
        ]
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> i64 {
        match self {
            Timeframe::Ms100 => 100,
            Timeframe::Ms250 => 250,
            Timeframe::Ms500 => 500,
            Timeframe::Sec1 => 1000,
            Timeframe::Sec2 => 2000,
            Timeframe::Sec5 => 5000,
            Timeframe::Sec10 => 10000,
            Timeframe::Sec30 => 30000,
            Timeframe::Min1 => 60_000,
            Timeframe::Min5 => 300_000,
            Timeframe::Min15 => 900_000,
            Timeframe::Min30 => 1_800_000,
            Timeframe::Hour1 => 3_600_000,
            Timeframe::Hour4 => 14_400_000,
            Timeframe::Day1 => 86_400_000,
            Timeframe::Week1 => 604_800_000,
            Timeframe::Month1 => 2_592_000_000, // Approximation: 30 days
            Timeframe::Custom(seconds) => (*seconds as i64) * 1000,
        }
    }

    /// Get chrono Duration
    pub fn duration(&self) -> Duration {
        Duration::milliseconds(self.duration_ms())
    }

    /// Returns true if this is a user-defined custom timeframe
    pub fn is_custom(&self) -> bool {
        matches!(self, Timeframe::Custom(_))
    }

    /// Get the number of seconds for this timeframe
    pub fn total_seconds(&self) -> u64 {
        (self.duration_ms() / 1000).max(0) as u64
    }

    /// Convert timeframe to seconds (signed, for range calculations)
    pub fn as_seconds(self) -> i64 {
        self.total_seconds() as i64
    }

    /// Alias for `as_seconds` for backward compatibility
    pub fn to_seconds(self) -> i64 {
        self.as_seconds()
    }

    /// Convert a resolution string to Timeframe
    pub fn from_resolution(resolution: &str) -> Option<Self> {
        match resolution {
            "1" => Some(Timeframe::Min1),
            "5" => Some(Timeframe::Min5),
            "15" => Some(Timeframe::Min15),
            "30" => Some(Timeframe::Min30),
            "60" | "1H" => Some(Timeframe::Hour1),
            "240" | "4H" => Some(Timeframe::Hour4),
            "1D" | "D" => Some(Timeframe::Day1),
            "1W" | "W" => Some(Timeframe::Week1),
            "1M" | "M" => Some(Timeframe::Month1),
            _ => None,
        }
    }
}

/// Format a custom seconds value into a human-readable label.
/// Chooses the largest clean unit: days > hours > minutes > seconds.
fn format_custom_seconds(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }
    if seconds >= 86400 && seconds % 86400 == 0 {
        format!("{}D", seconds / 86400)
    } else if seconds >= 3600 && seconds % 3600 == 0 {
        format!("{}h", seconds / 3600)
    } else if seconds >= 60 && seconds % 60 == 0 {
        format!("{}min", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}

impl Default for Timeframe {
    /// Default timeframe is 1 minute
    fn default() -> Self {
        Timeframe::Min1
    }
}

impl fmt::Display for Timeframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Timeframe {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Try known preset names first
        match s.to_lowercase().as_str() {
            "100ms" => return Ok(Timeframe::Ms100),
            "250ms" => return Ok(Timeframe::Ms250),
            "500ms" => return Ok(Timeframe::Ms500),
            "1s" | "1sec" => return Ok(Timeframe::Sec1),
            "2s" | "2sec" => return Ok(Timeframe::Sec2),
            "5s" | "5sec" => return Ok(Timeframe::Sec5),
            "10s" | "10sec" => return Ok(Timeframe::Sec10),
            "30s" | "30sec" => return Ok(Timeframe::Sec30),
            "1min" => return Ok(Timeframe::Min1),
            "5min" => return Ok(Timeframe::Min5),
            "15min" => return Ok(Timeframe::Min15),
            "30min" => return Ok(Timeframe::Min30),
            "1h" | "1hour" => return Ok(Timeframe::Hour1),
            "4h" | "4hour" => return Ok(Timeframe::Hour4),
            "1d" | "1day" => return Ok(Timeframe::Day1),
            "1w" | "1week" => return Ok(Timeframe::Week1),
            "1m" | "1month" => return Ok(Timeframe::Month1),
            _ => {}
        }

        // Try parsing as custom timeframe: "<number><unit>"
        let lower = s.to_lowercase();
        if let Some(seconds) = parse_custom_timeframe(&lower) {
            if seconds == 0 {
                return Err("Custom timeframe must be greater than zero".to_string());
            }
            return Ok(Timeframe::Custom(seconds));
        }

        Err(format!(
            "Invalid timeframe '{s}'. Valid formats: 100ms, 250ms, 500ms, 1s-30s, 1min-30min, 1h, 4h, 1D, 1W, 1M, or custom (e.g. 45s, 3min, 2h)"
        ))
    }
}

/// Parse a custom timeframe string like "45s", "3min", "2h", "2d"
/// and return the total number of seconds.
///
/// Note: "m" suffix is intentionally excluded to avoid ambiguity with months.
/// Use "min" for minutes (e.g. "3min", "45min").
fn parse_custom_timeframe(s: &str) -> Option<u64> {
    let suffixes: &[(&str, u64)] = &[
        ("month", 2_592_000),
        ("hour", 3600),
        ("min", 60),
        ("sec", 1),
        ("day", 86400),
        ("s", 1),
        ("h", 3600),
        ("d", 86400),
        ("w", 604_800),
    ];

    for &(suffix, multiplier) in suffixes {
        if let Some(num_str) = s.strip_suffix(suffix) {
            let num: u64 = num_str.trim().parse().ok()?;
            return Some(num * multiplier);
        }
    }

    // Try bare number as minutes (common convention)
    let num: u64 = s.trim().parse().ok()?;
    Some(num * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_as_str() {
        assert_eq!(&*Timeframe::Min1.as_str(), "1min");
        assert_eq!(&*Timeframe::Hour4.as_str(), "4h");
        assert_eq!(&*Timeframe::Day1.as_str(), "1D");
    }

    #[test]
    fn test_custom_as_str() {
        assert_eq!(&*Timeframe::Custom(45).as_str(), "45s");
        assert_eq!(&*Timeframe::Custom(180).as_str(), "3min");
        assert_eq!(&*Timeframe::Custom(7200).as_str(), "2h");
        assert_eq!(&*Timeframe::Custom(172800).as_str(), "2D");
    }

    #[test]
    fn test_custom_duration_ms() {
        assert_eq!(Timeframe::Custom(45).duration_ms(), 45_000);
        assert_eq!(Timeframe::Custom(180).duration_ms(), 180_000);
        assert_eq!(Timeframe::Custom(7200).duration_ms(), 7_200_000);
    }

    #[test]
    fn test_is_custom() {
        assert!(!Timeframe::Min1.is_custom());
        assert!(!Timeframe::Day1.is_custom());
        assert!(Timeframe::Custom(45).is_custom());
        assert!(Timeframe::Custom(180).is_custom());
    }

    #[test]
    fn test_from_str_presets() {
        assert_eq!("1min".parse::<Timeframe>(), Ok(Timeframe::Min1));
        assert_eq!("1h".parse::<Timeframe>(), Ok(Timeframe::Hour1));
        assert_eq!("1d".parse::<Timeframe>(), Ok(Timeframe::Day1));
        assert_eq!("1w".parse::<Timeframe>(), Ok(Timeframe::Week1));
        assert_eq!("1m".parse::<Timeframe>(), Ok(Timeframe::Month1));
    }

    #[test]
    fn test_from_str_custom() {
        assert_eq!("45s".parse::<Timeframe>(), Ok(Timeframe::Custom(45)));
        assert_eq!("45sec".parse::<Timeframe>(), Ok(Timeframe::Custom(45)));
        assert_eq!("3min".parse::<Timeframe>(), Ok(Timeframe::Custom(180)));
        assert_eq!("2h".parse::<Timeframe>(), Ok(Timeframe::Custom(7200)));
        assert_eq!("2hour".parse::<Timeframe>(), Ok(Timeframe::Custom(7200)));
        assert_eq!("2d".parse::<Timeframe>(), Ok(Timeframe::Custom(172800)));
        assert_eq!("2day".parse::<Timeframe>(), Ok(Timeframe::Custom(172800)));
    }

    #[test]
    fn test_from_str_custom_zero_rejected() {
        assert!("0s".parse::<Timeframe>().is_err());
        assert!("0min".parse::<Timeframe>().is_err());
    }

    #[test]
    fn test_from_str_invalid() {
        assert!("xyz".parse::<Timeframe>().is_err());
        assert!("".parse::<Timeframe>().is_err());
    }

    #[test]
    fn test_custom_display() {
        assert_eq!(Timeframe::Custom(45).to_string(), "45s");
        assert_eq!(Timeframe::Custom(180).to_string(), "3min");
        assert_eq!(Timeframe::Custom(7200).to_string(), "2h");
    }

    #[test]
    fn test_total_seconds() {
        assert_eq!(Timeframe::Min1.total_seconds(), 60);
        assert_eq!(Timeframe::Hour1.total_seconds(), 3600);
        assert_eq!(Timeframe::Custom(45).total_seconds(), 45);
    }

    #[test]
    fn test_custom_equality() {
        assert_eq!(Timeframe::Custom(60), Timeframe::Custom(60));
        assert_ne!(Timeframe::Custom(60), Timeframe::Custom(120));
        // Custom(60) is NOT equal to Min1 - they are different variants
        assert_ne!(Timeframe::Custom(60), Timeframe::Min1);
    }

    #[test]
    fn test_preset_from_str_takes_precedence() {
        // "1s" should resolve to preset Sec1, not Custom(1)
        assert_eq!("1s".parse::<Timeframe>(), Ok(Timeframe::Sec1));
        assert_eq!("5min".parse::<Timeframe>(), Ok(Timeframe::Min5));
        assert_eq!("1h".parse::<Timeframe>(), Ok(Timeframe::Hour1));
    }
}
