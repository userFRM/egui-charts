//! Smart time-axis tick mark generation.
//!
//! [`TickMarkGenerator`] places time labels at hierarchical boundaries
//! (year > month > week > day > hour > minute > second) and assigns a
//! [`TickMarkWeight`] to each.  When the chart is too narrow to show every
//! mark, lower-weight marks are dropped first, ensuring that the most
//! significant boundaries (year, month changes) always remain visible.

use super::time_formatter::{DefaultTimeFormatter, TimeFormatter};
/// Smart time axis mark generation.
/// Reference: lightweight-charts TickMarkType.
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// Type of tick mark on the time scale
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TickMarkType {
    /// Year mark (e.g., "2024")
    Year,

    /// Month mark (e.g., "Jan", "Feb")
    Month,

    /// Day of month mark (e.g., "15", "20")
    DayOfMonth,

    /// Time mark without seconds (e.g., "12:30")
    Time,

    /// Time mark with seconds (e.g., "12:30:45")
    TimeWithSeconds,
}

/// Weight of a tick mark (0-100 scale)
/// Higher weight = more important mark = displayed at lower zoom levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TickMarkWeight(u8);

impl TickMarkWeight {
    /// Create a new tick mark weight (0-100)
    pub fn new(weight: u8) -> Self {
        Self(weight.min(100))
    }

    /// Get the raw weight value
    pub fn value(&self) -> u8 {
        self.0
    }

    /// Weight for year boundaries
    pub const YEAR: TickMarkWeight = TickMarkWeight(100);

    /// Weight for month boundaries
    pub const MONTH: TickMarkWeight = TickMarkWeight(80);

    /// Weight for week boundaries
    pub const WEEK: TickMarkWeight = TickMarkWeight(70);

    /// Weight for day boundaries
    pub const DAY: TickMarkWeight = TickMarkWeight(60);

    /// Weight for 4-hour marks
    pub const HOUR_4: TickMarkWeight = TickMarkWeight(50);

    /// Weight for hour boundaries
    pub const HOUR: TickMarkWeight = TickMarkWeight(40);

    /// Weight for 30-minute marks
    pub const MIN_30: TickMarkWeight = TickMarkWeight(35);

    /// Weight for 15-minute marks
    pub const MIN_15: TickMarkWeight = TickMarkWeight(30);

    /// Weight for 5-minute marks
    pub const MIN_5: TickMarkWeight = TickMarkWeight(25);

    /// Weight for minute boundaries
    pub const MINUTE: TickMarkWeight = TickMarkWeight(20);

    /// Weight for 10-second marks
    pub const SEC_10: TickMarkWeight = TickMarkWeight(15);

    /// Weight for second boundaries
    pub const SECOND: TickMarkWeight = TickMarkWeight(10);

    /// Weight for sub-second marks
    pub const SUBSECOND: TickMarkWeight = TickMarkWeight(5);
}

/// A single tick mark on the time axis
#[derive(Debug, Clone)]
pub struct TickMark {
    /// Time of this tick mark
    pub time: DateTime<Utc>,

    /// Type of tick mark
    pub mark_type: TickMarkType,

    /// Weight of this mark (importance)
    pub weight: TickMarkWeight,

    /// Formatted label text
    pub label: String,

    /// Index in the data series
    pub index: usize,
}

/// Configuration for tick mark generation
#[derive(Debug, Clone)]
pub struct TickMarkGeneratorConfig {
    /// Min spacing between marks in pixels
    pub min_spacing: f32,

    /// Max number of marks to generate
    pub max_marks: usize,

    /// Whether to show sub-second marks
    pub show_subseconds: bool,

    /// Whether to use 24-hour time format
    pub use_24_hour: bool,

    /// Target density (marks per 100 pixels)
    pub target_density: f32,
}

impl Default for TickMarkGeneratorConfig {
    fn default() -> Self {
        Self {
            min_spacing: 50.0,
            max_marks: 50,
            show_subseconds: true,
            use_24_hour: true,
            target_density: 2.0,
        }
    }
}

/// Smart time axis mark generator
/// Implements intelligent mark distribution algorithm
pub struct TickMarkGenerator {
    config: TickMarkGeneratorConfig,
    formatter: Box<dyn TimeFormatter>,
}

impl TickMarkGenerator {
    /// Create a new tick mark generator with default config
    pub fn new() -> Self {
        Self {
            config: TickMarkGeneratorConfig::default(),
            formatter: Box::new(DefaultTimeFormatter::default()),
        }
    }

    /// Create a new tick mark generator with custom config
    pub fn with_config(config: TickMarkGeneratorConfig) -> Self {
        // Build formatter from config
        let formatter = Box::new(DefaultTimeFormatter {
            use_24_hour: config.use_24_hour,
            show_seconds: config.show_subseconds,
        });

        Self { config, formatter }
    }

    /// Create a new tick mark generator with custom formatter
    pub fn with_formatter(
        config: TickMarkGeneratorConfig,
        formatter: Box<dyn TimeFormatter>,
    ) -> Self {
        Self { config, formatter }
    }

    /// Generate tick marks for a given time range and display width
    pub fn generate_marks(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        width_pixels: f32,
        bars: &[(DateTime<Utc>, usize)], // (time, index) pairs
    ) -> Vec<TickMark> {
        if bars.is_empty() || width_pixels <= 0.0 {
            return Vec::new();
        }

        let time_span = end_time - start_time;

        // Calculate optimal mark interval based on time span and display width
        let interval = self.calculate_optimal_interval(time_span, width_pixels);

        // Determine mark type and weight threshold
        let (mark_type, weight_threshold) = self.determine_mark_type_and_weight(interval);

        // Generate candidate marks
        let mut marks = self.generate_candidate_marks(start_time, end_time, interval, mark_type);

        // Filter by weight threshold
        marks.retain(|mark| mark.weight >= weight_threshold);

        // Map marks to bar indices
        marks = self.map_marks_to_bars(marks, bars);

        // Apply spacing constraints
        marks = self.apply_spacing_constraints(marks, width_pixels);

        // Limit to max marks
        if marks.len() > self.config.max_marks {
            marks = self.reduce_marks_cnt(marks, self.config.max_marks);
        }

        marks
    }

    /// Calculate optimal interval between marks
    fn calculate_optimal_interval(&self, time_span: Duration, width_pixels: f32) -> Duration {
        let target_marks = (width_pixels / 100.0 * self.config.target_density).max(2.0);
        let seconds_per_mark = time_span.num_seconds() as f32 / target_marks;

        // Snap to nice intervals
        let seconds = if seconds_per_mark < 1.0 {
            // Sub-second intervals: 100ms, 250ms, 500ms
            if seconds_per_mark < 0.25 {
                0.1
            } else if seconds_per_mark < 0.5 {
                0.25
            } else {
                0.5
            }
        } else if seconds_per_mark < 60.0 {
            // Second intervals: 1s, 2s, 5s, 10s, 15s, 30s
            if seconds_per_mark < 2.0 {
                1.0
            } else if seconds_per_mark < 5.0 {
                2.0
            } else if seconds_per_mark < 10.0 {
                5.0
            } else if seconds_per_mark < 15.0 {
                10.0
            } else if seconds_per_mark < 30.0 {
                15.0
            } else {
                30.0
            }
        } else if seconds_per_mark < 3600.0 {
            // Minute intervals: 1m, 2m, 5m, 10m, 15m, 30m
            let minutes = seconds_per_mark / 60.0;
            if minutes < 2.0 {
                60.0
            } else if minutes < 5.0 {
                120.0
            } else if minutes < 10.0 {
                300.0
            } else if minutes < 15.0 {
                600.0
            } else if minutes < 30.0 {
                900.0
            } else {
                1800.0
            }
        } else if seconds_per_mark < 86400.0 {
            // Hour intervals: 1h, 2h, 4h, 6h, 12h
            let hours = seconds_per_mark / 3600.0;
            if hours < 2.0 {
                3600.0
            } else if hours < 4.0 {
                7200.0
            } else if hours < 6.0 {
                14400.0
            } else if hours < 12.0 {
                21600.0
            } else {
                43200.0
            }
        } else if seconds_per_mark < 2592000.0 {
            // Day intervals: 1d, 2d, 7d
            let days = seconds_per_mark / 86400.0;
            if days < 2.0 {
                86400.0
            } else if days < 7.0 {
                172800.0
            } else {
                604800.0
            }
        } else {
            // Month intervals: 1M, 3M, 6M, 1Y
            let days = seconds_per_mark / 86400.0;
            if days < 90.0 {
                2592000.0 // ~30 days
            } else if days < 180.0 {
                7776000.0 // ~90 days
            } else if days < 365.0 {
                15552000.0 // ~180 days
            } else {
                31536000.0 // ~365 days
            }
        };

        Duration::milliseconds((seconds * 1000.0) as i64)
    }

    /// Determine mark type and min weight threshold based on interval
    fn determine_mark_type_and_weight(&self, interval: Duration) -> (TickMarkType, TickMarkWeight) {
        let seconds = interval.num_seconds();

        if seconds < 1 {
            (TickMarkType::TimeWithSeconds, TickMarkWeight::SUBSECOND)
        } else if seconds < 60 {
            (TickMarkType::TimeWithSeconds, TickMarkWeight::SECOND)
        } else if seconds < 3600 {
            (TickMarkType::Time, TickMarkWeight::MINUTE)
        } else if seconds < 86400 {
            (TickMarkType::Time, TickMarkWeight::HOUR)
        } else if seconds < 2592000 {
            (TickMarkType::DayOfMonth, TickMarkWeight::DAY)
        } else if seconds < 31536000 {
            (TickMarkType::Month, TickMarkWeight::MONTH)
        } else {
            (TickMarkType::Year, TickMarkWeight::YEAR)
        }
    }

    /// Generate candidate marks at appropriate boundaries
    fn generate_candidate_marks(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        interval: Duration,
        primary_type: TickMarkType,
    ) -> Vec<TickMark> {
        let mut marks = Vec::new();
        let mut current = self.round_time_to_boundary(start_time, interval);

        while current <= end_time {
            let (mark_type, weight) = self.classify_time_boundary(current, primary_type);
            let label = self.format_time_label(current, mark_type);

            marks.push(TickMark {
                time: current,
                mark_type,
                weight,
                label,
                index: 0, // Will be set later
            });

            current += interval;
        }

        marks
    }

    /// Round time to appropriate boundary
    fn round_time_to_boundary(&self, time: DateTime<Utc>, interval: Duration) -> DateTime<Utc> {
        let seconds = interval.num_seconds();

        if seconds < 1 {
            // Sub-second: round to milliseconds
            let millis = interval.num_milliseconds();
            let timestamp_millis = time.timestamp_millis();
            let rounded = (timestamp_millis / millis) * millis;
            DateTime::from_timestamp_millis(rounded).unwrap_or(time)
        } else if seconds < 60 {
            // Seconds: round down to second boundary
            time.date_naive()
                .and_hms_opt(time.hour(), time.minute(), time.second())
                .unwrap()
                .and_utc()
        } else if seconds < 3600 {
            // Minutes: round down to minute boundary
            time.date_naive()
                .and_hms_opt(time.hour(), time.minute(), 0)
                .unwrap()
                .and_utc()
        } else if seconds < 86400 {
            // Hours: round down to hour boundary
            time.date_naive()
                .and_hms_opt(time.hour(), 0, 0)
                .unwrap()
                .and_utc()
        } else {
            // Days or more: round down to day boundary
            time.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc()
        }
    }

    /// Classify a time boundary and assign weight
    /// Respects primary_type hierarchy - won't downgrade day-level marks to time-level
    fn classify_time_boundary(
        &self,
        time: DateTime<Utc>,
        primary_type: TickMarkType,
    ) -> (TickMarkType, TickMarkWeight) {
        // Check for year boundary
        if time.month() == 1
            && time.day() == 1
            && time.hour() == 0
            && time.minute() == 0
            && time.second() == 0
        {
            return (TickMarkType::Year, TickMarkWeight::YEAR);
        }

        // Check for month boundary
        if time.day() == 1 && time.hour() == 0 && time.minute() == 0 && time.second() == 0 {
            return (TickMarkType::Month, TickMarkWeight::MONTH);
        }

        // Check for week boundary (Monday)
        if time.weekday().num_days_from_monday() == 0
            && time.hour() == 0
            && time.minute() == 0
            && time.second() == 0
        {
            return (TickMarkType::DayOfMonth, TickMarkWeight::WEEK);
        }

        // Check for day boundary
        if time.hour() == 0 && time.minute() == 0 && time.second() == 0 {
            return (TickMarkType::DayOfMonth, TickMarkWeight::DAY);
        }

        // IMPORTANT: If primary_type is day-level or higher, don't downgrade to time-of-day marks
        // This fixes the bug where daily data shows "14:30" instead of "Jun 15"
        let is_day_or_higher = matches!(
            primary_type,
            TickMarkType::Year | TickMarkType::Month | TickMarkType::DayOfMonth
        );

        if is_day_or_higher {
            // For day+ intervals, always show as day of month with appropriate weight
            // Use the day number for display (e.g., "15", "Jun 15")
            return (TickMarkType::DayOfMonth, TickMarkWeight::DAY);
        }

        // Below here only applies to time-level primary types (intraday data)

        // Check for 4-hour boundary
        if time.hour().is_multiple_of(4) && time.minute() == 0 && time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::HOUR_4);
        }

        // Check for hour boundary
        if time.minute() == 0 && time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::HOUR);
        }

        // Check for 30-minute boundary
        if time.minute().is_multiple_of(30) && time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::MIN_30);
        }

        // Check for 15-minute boundary
        if time.minute().is_multiple_of(15) && time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::MIN_15);
        }

        // Check for 5-minute boundary
        if time.minute().is_multiple_of(5) && time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::MIN_5);
        }

        // Check for minute boundary
        if time.second() == 0 {
            return (TickMarkType::Time, TickMarkWeight::MINUTE);
        }

        // Check for 10-second boundary
        if time.second().is_multiple_of(10) {
            return (TickMarkType::TimeWithSeconds, TickMarkWeight::SEC_10);
        }

        // Default: use primary type with second weight
        (primary_type, TickMarkWeight::SECOND)
    }

    /// Format time label based on mark type
    fn format_time_label(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.formatter.format(time, mark_type)
    }

    /// Map marks to bar indices
    fn map_marks_to_bars(
        &self,
        mut marks: Vec<TickMark>,
        bars: &[(DateTime<Utc>, usize)],
    ) -> Vec<TickMark> {
        for mark in &mut marks {
            // Find closest bar
            let closest = bars.iter().min_by_key(|(time, _)| {
                let diff = if *time > mark.time {
                    *time - mark.time
                } else {
                    mark.time - *time
                };
                diff.num_milliseconds().abs()
            });

            if let Some((_, index)) = closest {
                mark.index = *index;
            }
        }

        marks
    }

    /// Apply min spacing constraints between marks
    fn apply_spacing_constraints(
        &self,
        mut marks: Vec<TickMark>,
        width_pixels: f32,
    ) -> Vec<TickMark> {
        if marks.is_empty() {
            return marks;
        }

        // Calculate pixels per mark
        let pixels_per_mark = width_pixels / marks.len() as f32;

        // If spacing is too tight, filter by weight
        if pixels_per_mark < self.config.min_spacing {
            // Calculate min time difference BEFORE sorting
            // Formula: min_time_diff = min_spacing * (time_span / width_pixels)
            let min_time = marks.iter().map(|m| m.time).min().unwrap();
            let max_time = marks.iter().map(|m| m.time).max().unwrap();
            let time_span = (max_time - min_time).num_seconds() as f32;
            let min_time_diff =
                Duration::seconds((self.config.min_spacing * (time_span / width_pixels)) as i64);

            // Sort by weight descending to prioritize important marks
            // Secondary sort by time ensures deterministic ordering when weights are equal
            marks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.time.cmp(&b.time)));

            // Keep marks with sufficient spacing
            let mut filtered = vec![marks[0].clone()];

            for mark in marks.iter().skip(1) {
                if filtered.iter().all(|m| {
                    let diff = if m.time > mark.time {
                        m.time - mark.time
                    } else {
                        mark.time - m.time
                    };
                    diff >= min_time_diff
                }) {
                    filtered.push(mark.clone());
                }
            }

            // Sort back by time
            filtered.sort_by_key(|m| m.time);
            marks = filtered;
        }

        marks
    }

    /// Reduce marks count to max
    fn reduce_marks_cnt(&self, mut marks: Vec<TickMark>, max_marks: usize) -> Vec<TickMark> {
        if marks.len() <= max_marks {
            return marks;
        }

        // Keep the highest weight marks
        marks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.time.cmp(&b.time)));
        marks.truncate(max_marks);
        marks.sort_by_key(|m| m.time);
        marks
    }
}

impl Default for TickMarkGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_tick_mark_weight_ordering() {
        assert!(TickMarkWeight::YEAR > TickMarkWeight::MONTH);
        assert!(TickMarkWeight::MONTH > TickMarkWeight::DAY);
        assert!(TickMarkWeight::DAY > TickMarkWeight::HOUR);
        assert!(TickMarkWeight::HOUR > TickMarkWeight::MINUTE);
    }

    #[test]
    fn test_optimal_interval_calculation() {
        let generator = TickMarkGenerator::new();

        // 1 hour span, 1000 pixels -> should get minute-level marks
        let span = Duration::hours(1);
        let interval = generator.calculate_optimal_interval(span, 1000.0);
        assert!(interval.num_seconds() >= 60 && interval.num_seconds() <= 600);

        // 1 day span, 1000 pixels -> should get hour-level marks
        let span = Duration::days(1);
        let interval = generator.calculate_optimal_interval(span, 1000.0);
        assert!(interval.num_seconds() >= 3600);
    }

    #[test]
    fn test_time_boundary_classification() {
        let generator = TickMarkGenerator::new();

        // Year boundary
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let (mark_type, weight) = generator.classify_time_boundary(time, TickMarkType::Time);
        assert_eq!(mark_type, TickMarkType::Year);
        assert_eq!(weight, TickMarkWeight::YEAR);

        // Month boundary
        let time = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let (mark_type, weight) = generator.classify_time_boundary(time, TickMarkType::Time);
        assert_eq!(mark_type, TickMarkType::Month);
        assert_eq!(weight, TickMarkWeight::MONTH);

        // Hour boundary (use hour 13, not 12, since 12 is a 4-hour boundary)
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 13, 0, 0).unwrap();
        let (mark_type, weight) = generator.classify_time_boundary(time, TickMarkType::Time);
        assert_eq!(mark_type, TickMarkType::Time);
        assert_eq!(weight, TickMarkWeight::HOUR);
    }

    #[test]
    fn test_label_formatting() {
        let generator = TickMarkGenerator::new();
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();

        assert_eq!(
            generator.format_time_label(time, TickMarkType::Year),
            "2024"
        );
        assert_eq!(
            generator.format_time_label(time, TickMarkType::Month),
            "Jun"
        );
        // Shows "Jun 15" for day boundaries
        assert_eq!(
            generator.format_time_label(time, TickMarkType::DayOfMonth),
            "Jun 15"
        );
        assert_eq!(
            generator.format_time_label(time, TickMarkType::Time),
            "14:30"
        );
        assert_eq!(
            generator.format_time_label(time, TickMarkType::TimeWithSeconds),
            "14:30:45"
        );
    }
}
