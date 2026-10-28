//! Smart time-axis tick mark generation.
//!
//! [`TickMarkGenerator`] places time labels at hierarchical boundaries
//! (year > month > week > day > hour > minute > second) and assigns a
//! [`TickMarkWeight`] to each.  When the chart is too narrow to show every
//! mark, lower-weight marks are dropped first, ensuring that the most
//! significant boundaries (year, month changes) always remain visible.

use super::time_formatter::{DefaultTimeFormatter, TimeFormatter};
use chrono::{DateTime, Datelike, Duration, Months, TimeZone, Timelike, Utc};
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

    /// Whether the label is drawn bold where the time scale options allow bold labels
    pub bold: bool,
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
    uniform_distribution: bool,
}

impl TickMarkGenerator {
    /// Create a new tick mark generator with default config
    pub fn new() -> Self {
        Self {
            config: TickMarkGeneratorConfig::default(),
            formatter: Box::new(DefaultTimeFormatter::default()),
            uniform_distribution: false,
        }
    }

    /// Create a new tick mark generator with custom config
    pub fn with_config(config: TickMarkGeneratorConfig) -> Self {
        // Build formatter from config
        let formatter = Box::new(DefaultTimeFormatter {
            use_24_hour: config.use_24_hour,
            show_seconds: config.show_subseconds,
            date_format: None,
        });

        Self {
            config,
            formatter,
            uniform_distribution: false,
        }
    }

    /// Create a new tick mark generator with custom formatter
    pub fn with_formatter(
        config: TickMarkGeneratorConfig,
        formatter: Box<dyn TimeFormatter>,
    ) -> Self {
        Self {
            config,
            formatter,
            uniform_distribution: false,
        }
    }

    /// Keeps marks of one weight all or none (default `false`): when a mark of some weight
    /// does not fit between the marks already placed, that weight and every lighter one are
    /// left out, so labels of one kind stay evenly spaced.
    pub fn with_uniform_distribution(mut self, uniform: bool) -> Self {
        self.uniform_distribution = uniform;
        self
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

        let intraday = bars.windows(2).any(|pair| {
            let delta = pair[1].0 - pair[0].0;
            delta > Duration::zero() && delta < Duration::days(1)
        });
        let mut interval = self.calculate_optimal_interval(time_span, width_pixels);
        if !self.formatter.time_visible() {
            interval = interval.max(Duration::days(1));
        }
        // Daily charts at a multi-month zoom show calendar months, not alternating weeks.
        if !intraday && interval >= Duration::days(7) {
            interval = interval.max(Duration::days(30));
        }
        let (mark_type, weight_threshold) = self.determine_mark_type_and_weight(interval);

        // Session gaps take no horizontal space; use the supplied bar boundaries.
        let mut marks = if intraday {
            self.generate_bar_marks(bars, interval, mark_type)
        } else {
            self.generate_candidate_marks(
                self.formatter.wall_time(start_time),
                self.formatter.wall_time(end_time),
                interval,
                mark_type,
            )
            .into_iter()
            .filter_map(|mut mark| {
                mark.time = self.formatter.utc_time(mark.time)?;
                (mark.time >= start_time && mark.time <= end_time).then_some(mark)
            })
            .collect()
        };

        // Filter by weight threshold
        marks.retain(|mark| mark.weight >= weight_threshold);

        // Map marks to bar indices
        marks = self.map_marks_to_bars(marks, bars);

        // Apply spacing constraints
        marks = if intraday {
            let span = bars.last().unwrap().1.saturating_sub(bars[0].1).max(1) as f32;
            let min_bars = self.config.min_spacing * span / width_pixels;
            let mut kept = keep_spaced(marks, min_bars, self.uniform_distribution);
            kept.sort_by_key(|mark| mark.time);
            kept
        } else {
            self.apply_spacing_constraints(marks, width_pixels)
        };

        // Limit to max marks
        if marks.len() > self.config.max_marks {
            marks = self.reduce_marks_cnt(marks, self.config.max_marks);
        }

        for mark in &mut marks {
            mark.label = self.format_time_label(mark.time, mark.mark_type);
            mark.bold = matches!(
                mark.mark_type,
                TickMarkType::DayOfMonth | TickMarkType::Month | TickMarkType::Year
            );
        }
        marks
    }

    /// Calendar and clock boundaries on actual bars, in the formatter's timezone.
    fn generate_bar_marks(
        &self,
        bars: &[(DateTime<Utc>, usize)],
        interval: Duration,
        primary_type: TickMarkType,
    ) -> Vec<TickMark> {
        let mut marks = Vec::new();
        let mut previous: Option<DateTime<Utc>> = None;
        for &(time, index) in bars {
            let local = self.formatter.wall_time(time);
            let boundary = previous.filter(|previous| previous.date_naive() != local.date_naive());
            let (kind, weight) = if boundary.is_some_and(|previous| previous.year() != local.year())
            {
                (TickMarkType::Year, TickMarkWeight::YEAR)
            } else if boundary.is_some_and(|previous| previous.month() != local.month()) {
                (TickMarkType::Month, TickMarkWeight::MONTH)
            } else {
                self.classify_time_boundary(local, primary_type)
            };
            let day = boundary.is_some() || local.time().num_seconds_from_midnight() == 0;
            let millis = interval.num_milliseconds().max(1);
            let clock_bucket = |time: DateTime<Utc>| {
                (i64::from(time.time().num_seconds_from_midnight()) * 1000
                    + i64::from(time.timestamp_subsec_millis()))
                    / millis
            };
            let clock_mark = interval < Duration::days(1)
                && (previous.is_some_and(|previous| clock_bucket(previous) != clock_bucket(local))
                    || local.timestamp_millis().rem_euclid(millis) == 0);
            let day_mark = day
                && (interval < Duration::days(1)
                    || i64::from(local.num_days_from_ce()).rem_euclid(interval.num_days().max(1))
                        == 0);
            if kind == TickMarkType::Year || kind == TickMarkType::Month || day_mark || clock_mark {
                let (mark_type, weight) = if day && weight < TickMarkWeight::DAY {
                    (TickMarkType::DayOfMonth, TickMarkWeight::DAY)
                } else if clock_mark && weight < TickMarkWeight::DAY {
                    let boundary = local
                        - Duration::milliseconds(
                            (i64::from(local.time().num_seconds_from_midnight()) * 1000
                                + i64::from(local.timestamp_subsec_millis()))
                            .rem_euclid(millis),
                        );
                    self.classify_time_boundary(boundary, primary_type)
                } else {
                    (kind, weight)
                };
                marks.push(TickMark {
                    time,
                    mark_type,
                    weight,
                    label: String::new(),
                    index,
                    bold: false,
                });
            }
            previous = Some(local);
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

        // Beyond a budget of candidates, step by whole years to bound the work.
        let candidate_budget = (target_marks * 8.0).ceil().max(256.0) as i64;
        let interval = Duration::milliseconds((seconds * 1000.0) as i64);
        if time_span.num_milliseconds() / candidate_budget < interval.num_milliseconds() {
            return interval;
        }
        Duration::days(365 * ((seconds_per_mark / 31_536_000.0) as i64 + 1))
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
        // Multi-year intervals step by calendar years so every mark is a year boundary.
        let years = interval.num_days() / 365;
        if years > 1 {
            let first = start_time.year() - start_time.year().rem_euclid(years as i32);
            let mut current = Utc.with_ymd_and_hms(first, 1, 1, 0, 0, 0).single();
            while let Some(time) = current.filter(|time| *time <= end_time) {
                if time >= start_time {
                    marks.push(TickMark {
                        time,
                        mark_type: TickMarkType::Year,
                        weight: TickMarkWeight::YEAR,
                        label: String::new(),
                        index: 0, // Will be set later
                        bold: false,
                    });
                }
                current = time.checked_add_months(Months::new(12 * years as u32));
            }
            return marks;
        }
        if interval >= Duration::days(30) {
            let months = (interval.num_days() / 30).max(1) as u32;
            let month = start_time.month0() / months * months + 1;
            let mut current = Utc
                .with_ymd_and_hms(start_time.year(), month, 1, 0, 0, 0)
                .single();
            while let Some(time) = current.filter(|time| *time <= end_time) {
                if time >= start_time {
                    let (mark_type, weight) =
                        self.classify_time_boundary(time, TickMarkType::Month);
                    marks.push(TickMark {
                        time,
                        mark_type,
                        weight,
                        label: String::new(),
                        index: 0,
                        bold: false,
                    });
                }
                current = time.checked_add_months(Months::new(months));
            }
            return marks;
        }
        let mut current = self.round_time_to_boundary(start_time, interval);

        while current <= end_time {
            let (mark_type, weight) = self.classify_time_boundary(current, primary_type);

            marks.push(TickMark {
                time: current,
                mark_type,
                weight,
                label: String::new(),
                index: 0, // Will be set later
                bold: false,
            });

            let Some(next) = current.checked_add_signed(interval) else {
                break;
            };
            current = next;
        }

        // Calendar boundaries outrank a day stride even when the first is off that stride.
        if interval >= Duration::days(1) {
            marks.extend(self.generate_candidate_marks(
                start_time,
                end_time,
                Duration::days(30),
                TickMarkType::Month,
            ));
            marks.sort_by_key(|mark| mark.time);
            marks.dedup_by_key(|mark| mark.time);
        }
        marks
    }

    /// Round time to appropriate boundary
    fn round_time_to_boundary(&self, time: DateTime<Utc>, interval: Duration) -> DateTime<Utc> {
        let seconds = interval.num_seconds();

        if seconds < 1 {
            // Sub-second: round to milliseconds
            let millis = interval.num_milliseconds();
            // A zero-millisecond interval has no boundary to snap to; integer
            // division by it would panic, so leave the time unrounded.
            if millis == 0 {
                return time;
            }
            let timestamp_millis = time.timestamp_millis();
            let rounded = (timestamp_millis / millis) * millis;
            DateTime::from_timestamp_millis(rounded).unwrap_or(time)
        } else if seconds < 86400 {
            // Multiples of the interval since midnight keep their clock times as the view pans.
            // Start at or after the first bar: a mark before it would be drawn at that bar.
            let midnight = time.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
            let since_midnight = (time - midnight).num_seconds();
            let first = (since_midnight + seconds - 1) / seconds * seconds;
            midnight
                .checked_add_signed(Duration::seconds(first))
                .unwrap_or(time)
        } else {
            // Monday 1970-01-05 anchors weeks; the Unix epoch anchors other day strides.
            let anchor = if seconds == 604_800 { 4 * 86_400 } else { 0 };
            let elapsed = time.timestamp() - anchor;
            let first = elapsed.div_euclid(seconds) + i64::from(elapsed.rem_euclid(seconds) != 0);
            DateTime::from_timestamp(anchor + first * seconds, 0).unwrap_or(time)
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
        marks.retain_mut(|mark| {
            let position = bars.partition_point(|(time, _)| *time < mark.time);
            let Some(&(time, index)) = bars.get(position) else {
                return false;
            };
            mark.time = time;
            mark.index = index;
            true
        });
        // A gap can map several candidates to one bar; retain its strongest boundary.
        marks.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| b.weight.cmp(&a.weight)));
        marks.dedup_by_key(|mark| mark.index);

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
                } else if self.uniform_distribution {
                    filtered.retain(|other| other.weight > mark.weight);
                    break;
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
        if self.uniform_distribution {
            let cut = marks[max_marks].weight;
            marks.retain(|mark| mark.weight > cut);
        } else {
            marks.truncate(max_marks);
        }
        marks.sort_by_key(|m| m.time);
        marks
    }
}

/// The heaviest of `marks` that lie `min_bars` bars apart: lighter marks give way, and with a
/// `uniform` distribution a weight that does not fit everywhere is left out with every lighter
/// one.
pub(super) fn keep_spaced(mut marks: Vec<TickMark>, min_bars: f32, uniform: bool) -> Vec<TickMark> {
    marks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.index.cmp(&b.index)));
    let mut kept: Vec<TickMark> = Vec::new();
    for mark in marks {
        if kept
            .iter()
            .all(|other| mark.index.abs_diff(other.index) as f32 >= min_bars)
        {
            kept.push(mark);
        } else if uniform {
            kept.retain(|other| other.weight > mark.weight);
            break;
        }
    }
    kept
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

        let start = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        // 1000 px give 20 target marks and a budget of 256 candidates; beyond it the
        // interval is the whole number of years per target mark, plus one.
        for (span, expected_interval) in [
            (Duration::hours(1), Duration::minutes(2)),
            (Duration::days(1), Duration::hours(1)),
            (Duration::days(365 * 310), Duration::days(365 * 16)),
            (Duration::days(365 * 100_010), Duration::days(365 * 5001)),
        ] {
            let interval = generator.calculate_optimal_interval(span, 1000.0);
            assert_eq!(interval, expected_interval);
            let candidates = generator.generate_candidate_marks(
                start,
                start + span,
                interval,
                TickMarkType::Year,
            );
            assert!(candidates.len() <= 257, "{} candidates", candidates.len());
            if interval > Duration::days(365) {
                let bars = [(start, 0), (start + span, 1)];
                let marks = generator.generate_marks(start, start + span, 1000.0, &bars);
                assert!(marks.iter().all(|mark| mark.time == bars[mark.index].0));
                assert!(marks.len() <= bars.len());
                let years: Vec<_> = candidates.iter().map(|mark| mark.time.year()).collect();
                let step = years[1] - years[0];
                assert!(
                    years.windows(2).all(|pair| pair[1] - pair[0] == step),
                    "{years:?}"
                );
            }
        }
    }

    /// Three days of 5-minute bars on 600 points: day marks lie 288 bars apart and 4-hour
    /// marks 48 apart, under the 72-bar minimum of 50 points. Without uniform distribution
    /// every other 4-hour mark is kept; with it, none are.
    #[test]
    fn uniform_distribution_keeps_each_weight_whole() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars: Vec<_> = (0..864)
            .map(|i| (start + Duration::minutes(5 * i as i64), i))
            .collect();
        for (uniform, expected) in [
            (
                false,
                vec![
                    (0, 100),
                    (96, 50),
                    (192, 50),
                    (288, 60),
                    (384, 50),
                    (480, 50),
                    (576, 60),
                    (672, 50),
                    (768, 50),
                ],
            ),
            (true, vec![(0, 100), (288, 60), (576, 60)]),
        ] {
            let marks = TickMarkGenerator::new()
                .with_uniform_distribution(uniform)
                .generate_marks(start, bars[863].0, 600.0, &bars);
            let marks: Vec<_> = marks
                .iter()
                .map(|mark| (mark.index, mark.weight.value()))
                .collect();
            assert_eq!(marks, expected, "uniform {uniform}");
        }
    }

    #[test]
    fn time_marks_keep_boundaries_and_only_survivors_are_formatted() {
        use super::super::time_formatter::CustomTimeFormatter;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        use TickMarkType::{DayOfMonth, Time, Year};
        let mut formatting_counts = Vec::new();
        for (span, expected) in [
            (
                Duration::minutes(10),
                vec![
                    (0, Year, 100, "2024", 0),
                    (300, Time, 25, "00:05", 10),
                    (600, Time, 25, "00:10", 20),
                ],
            ),
            (
                Duration::hours(1),
                vec![
                    (0, Year, 100, "2024", 0),
                    (1800, Time, 35, "00:30", 60),
                    (3600, Time, 40, "01:00", 120),
                ],
            ),
            (
                Duration::days(1),
                vec![
                    (0, Year, 100, "2024", 0),
                    (14400, Time, 50, "04:00", 480),
                    (86400, DayOfMonth, 60, "02", 2880),
                ],
            ),
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let seen = Arc::clone(&calls);
            let formatter = CustomTimeFormatter::new(move |time, kind| {
                seen.fetch_add(1, Ordering::Relaxed);
                DefaultTimeFormatter::default().format(time, kind)
            });
            let generator = TickMarkGenerator::with_formatter(
                TickMarkGeneratorConfig {
                    max_marks: 3,
                    ..Default::default()
                },
                Box::new(formatter),
            );
            let bars: Vec<_> = (0..=span.num_seconds())
                .step_by(30)
                .enumerate()
                .map(|(i, seconds)| (start + Duration::seconds(seconds), i))
                .collect();
            let marks = generator.generate_marks(start, start + span, 1000.0, &bars);
            let actual: Vec<_> = marks
                .iter()
                .map(|mark| {
                    (
                        (mark.time - start).num_seconds(),
                        mark.mark_type,
                        mark.weight.value(),
                        mark.label.as_str(),
                        mark.index,
                    )
                })
                .collect();
            assert_eq!(actual, expected);
            formatting_counts.push((calls.load(Ordering::Relaxed), marks.len()));
        }
        assert!(
            formatting_counts.iter().all(|(calls, kept)| calls == kept),
            "format only surviving marks: {formatting_counts:?}"
        );

        let start = Utc.with_ymd_and_hms(2024, 1, 20, 15, 0, 0).unwrap();
        let bars: Vec<_> = (0..121)
            .map(|i| (start + Duration::hours(i), i as usize))
            .collect();
        // 119 hours at 1100 px choose 4-hour marks, from the first one at or after the first bar.
        // Spacing keeps each midnight with its date and 08:00 and 16:00 between them, and a pan
        // of one bar keeps every mark.
        let expected = [
            (1, Time, 50, "16:00", 1),
            (9, DayOfMonth, 60, "21", 9),
            (17, Time, 50, "08:00", 17),
            (25, Time, 50, "16:00", 25),
            (33, DayOfMonth, 70, "22", 33),
            (41, Time, 50, "08:00", 41),
            (49, Time, 50, "16:00", 49),
            (57, DayOfMonth, 60, "23", 57),
            (65, Time, 50, "08:00", 65),
            (73, Time, 50, "16:00", 73),
            (81, DayOfMonth, 60, "24", 81),
            (89, Time, 50, "08:00", 89),
            (97, Time, 50, "16:00", 97),
            (105, DayOfMonth, 60, "25", 105),
            (113, Time, 50, "08:00", 113),
        ];
        let generator = TickMarkGenerator::new();
        for pan in [0, 1] {
            let visible = &bars[pan..pan + 120];
            let marks = generator.generate_marks(visible[0].0, visible[119].0, 1100.0, visible);
            let actual: Vec<_> = marks
                .iter()
                .map(|mark| {
                    (
                        (mark.time - start).num_hours(),
                        mark.mark_type,
                        mark.weight.value(),
                        mark.label.as_str(),
                        mark.index,
                    )
                })
                .collect();
            assert_eq!(actual, expected, "pan {pan}");
        }
        use crate::scales::{TimeFormatterBuilder, TimezoneMode};
        // Local midnight, including a month change, is a date mark in either zone.
        let start = Utc.with_ymd_and_hms(2024, 3, 30, 0, 0, 0).unwrap();
        let bars: Vec<_> = (0..120)
            .map(|i| (start + Duration::hours(i), i as usize))
            .collect();
        for zone in [
            TimezoneMode::Utc,
            TimezoneMode::Timezone("+05:45".parse().unwrap()),
            #[cfg(feature = "timezones")]
            TimezoneMode::nyse(),
        ] {
            let formatter = TimeFormatterBuilder::new().with_timezone(zone).build();
            let generator = TickMarkGenerator::with_formatter(
                TickMarkGeneratorConfig::default(),
                formatter.clone_box(),
            );
            let marks = generator.generate_marks(bars[0].0, bars[119].0, 1100.0, &bars);
            assert!(
                marks
                    .iter()
                    .any(|mark| mark.mark_type == TickMarkType::Month && mark.label == "Apr")
            );
            for mark in marks
                .iter()
                .filter(|mark| mark.weight >= TickMarkWeight::DAY)
            {
                assert_eq!(formatter.wall_time(mark.time).hour(), 0, "{mark:?}");
            }
        }
        // Midnights on DST transition dates are 23 or 25 hours apart in each zone.
        for (zone, midnight, next, hours) in [
            (
                "America/New_York",
                "2024-03-10T05:00:00Z",
                "2024-03-11T04:00:00Z",
                23,
            ),
            (
                "America/New_York",
                "2024-11-03T04:00:00Z",
                "2024-11-04T05:00:00Z",
                25,
            ),
            (
                "Europe/Zurich",
                "2024-03-30T23:00:00Z",
                "2024-03-31T22:00:00Z",
                23,
            ),
            (
                "Europe/Zurich",
                "2024-10-26T22:00:00Z",
                "2024-10-27T23:00:00Z",
                25,
            ),
        ] {
            let Some(zone) = crate::model::TimezoneId::named(zone) else {
                continue;
            };
            let midnight: DateTime<Utc> = midnight.parse().unwrap();
            let next: DateTime<Utc> = next.parse().unwrap();
            let formatter = TimeFormatterBuilder::new()
                .with_timezone(TimezoneMode::Timezone(zone.clone()))
                .build();
            let generator = TickMarkGenerator::with_formatter(
                TickMarkGeneratorConfig::default(),
                formatter.clone_box(),
            );
            let bars: Vec<_> = (-24..=hours + 24)
                .enumerate()
                .map(|(i, hour)| (midnight + Duration::hours(hour), i))
                .collect();
            for width in [434.0, 734.0, 1274.0] {
                let marks =
                    generator.generate_marks(bars[0].0, bars.last().unwrap().0, width, &bars);
                for time in [midnight, next] {
                    assert!(
                        marks
                            .iter()
                            .any(|mark| mark.time == time && mark.weight >= TickMarkWeight::DAY),
                        "{zone} {width}: {marks:?}"
                    );
                }
                assert_eq!(next - midnight, Duration::hours(hours));
                assert!(marks.iter().filter(|mark| mark.weight >= TickMarkWeight::DAY).all(|mark| formatter.wall_time(mark.time).time() == chrono::NaiveTime::MIN));
            }
        }
        // Monthly intraday and daily calendar candidates must use the displayed month.
        for step in [Duration::hours(1), Duration::days(1)] {
            let first = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
            let count = Duration::days(350).num_seconds() / step.num_seconds();
            let bars: Vec<_> = (0..count)
                .map(|i| (first + step * i as i32, i as usize))
                .collect();
            let formatter = TimeFormatterBuilder::new()
                .with_timezone(TimezoneMode::Timezone("-05:00".parse().unwrap()))
                .build();
            let generator = TickMarkGenerator::with_formatter(
                TickMarkGeneratorConfig::default(),
                formatter.clone_box(),
            );
            let marks = generator.generate_marks(bars[0].0, bars.last().unwrap().0, 434.0, &bars);
            assert!(marks.len() >= 2);
            assert!(
                marks
                    .iter()
                    .all(|mark| formatter.wall_time(mark.time).day() == 1
                        && mark.time == bars[mark.index].0),
                "{marks:?}"
            );
        }
        let start = Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap();
        let daily: Vec<_> = (0..303)
            .map(|i| (start + Duration::days(i), i as usize))
            .collect();
        let marks =
            TickMarkGenerator::new().generate_marks(daily[0].0, daily[302].0, 1100.0, &daily);
        let panned =
            TickMarkGenerator::new().generate_marks(daily[1].0, daily[302].0, 1100.0, &daily[1..]);
        assert_eq!(
            marks
                .iter()
                .skip(1)
                .map(|mark| mark.time)
                .collect::<Vec<_>>(),
            panned.iter().map(|mark| mark.time).collect::<Vec<_>>()
        );
        let yearly = TickMarkGenerator::new().generate_candidate_marks(
            start,
            start + Duration::days(365 * 3),
            Duration::days(365),
            TickMarkType::Year,
        );
        assert!(
            yearly.len() >= 2
                && yearly
                    .iter()
                    .all(|mark| mark.mark_type == TickMarkType::Year)
        );
        assert_eq!(
            marks
                .iter()
                .map(|mark| mark.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Sep", "Oct", "Nov", "Dec", "2025", "Feb", "Mar", "Apr", "May", "Jun"
            ]
        );
        assert_eq!(marks[4].weight, TickMarkWeight::YEAR);
        // Hour bars may start at half past the hour and still need clock marks.
        let start = Utc.with_ymd_and_hms(2024, 3, 20, 9, 30, 0).unwrap();
        let session: Vec<_> = (0..7)
            .map(|i| (start + Duration::hours(i), i as usize))
            .collect();
        let marks =
            TickMarkGenerator::new().generate_marks(session[0].0, session[6].0, 300.0, &session);
        assert!(marks.len() >= 3, "half-hour session: {marks:?}");
        assert!(marks.iter().all(|mark| mark.time == session[mark.index].0));
        // Missing weekends and closed hours consume no slots; marks use actual session bars.
        let start = Utc.with_ymd_and_hms(2024, 3, 20, 13, 30, 0).unwrap();
        let bars: Vec<_> = (0..30)
            .flat_map(|day| {
                let time = start + Duration::days(day);
                (0..7).filter_map(move |hour| {
                    (time.weekday().number_from_monday() <= 5)
                        .then_some(time + Duration::hours(hour))
                })
            })
            .enumerate()
            .map(|(index, time)| (time, index))
            .collect();
        let marks = TickMarkGenerator::new().generate_marks(
            bars[0].0,
            bars.last().unwrap().0,
            1100.0,
            &bars,
        );
        assert!(
            marks
                .iter()
                .any(|mark| mark.mark_type == TickMarkType::Month && mark.label == "Apr")
        );
        assert!(
            marks
                .iter()
                .all(|mark| bars[mark.index].0 == mark.time && mark.time.hour() == 13)
        );
        let month_start = bars
            .windows(2)
            .position(|pair| pair[0].0.month() != pair[1].0.month())
            .unwrap()
            + 1;
        let around_month = &bars[month_start - 7..month_start + 7];
        let close_marks = TickMarkGenerator::new().generate_marks(
            around_month[0].0,
            around_month[13].0,
            800.0,
            around_month,
        );
        assert!(
            close_marks
                .iter()
                .any(|mark| mark.mark_type == TickMarkType::Month && mark.label == "Apr")
        );
        let min_bars = 50.0 * (bars.len() - 1) as f32 / 1100.0;
        assert!(
            marks
                .windows(2)
                .all(|pair| pair[0].index.abs_diff(pair[1].index) as f32 >= min_bars)
        );
    }

    #[test]
    fn daily_marks_keep_calendar_anchors_at_every_width() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars: Vec<_> = (0..600)
            .map(|i| (start + Duration::days(i), i as usize))
            .collect();
        let generator = TickMarkGenerator::new();
        for width in [434.0, 634.0, 734.0, 1274.0] {
            let marks = generator.generate_marks(bars[300].0, bars[419].0, width, &bars[300..420]);
            assert!(marks.len() >= 2, "width {width}: {marks:?}");
            for &(time, _) in bars[300..420].iter().filter(|(time, _)| time.day() == 1) {
                assert!(
                    marks.iter().any(|mark| mark.time == time),
                    "lost month boundary at width {width}: {time}"
                );
            }
            if width <= 734.0 {
                assert!(marks.iter().all(|mark| mark.time.day() == 1));
            }
        }
        // Weekends and a holiday month opening have no bar to label.
        let weekdays: Vec<_> = (0..400)
            .map(|day| start + Duration::days(day))
            .filter(|time| time.weekday().number_from_monday() <= 5 && time.day() != 1)
            .enumerate()
            .map(|(index, time)| (time, index))
            .collect();
        use crate::scales::{HorizontalScale, TickMarkLayout, TimeFormatterBuilder};
        let formatter = TimeFormatterBuilder::new().build();
        for spacing in [8.0, 14.0, 24.0, 40.0] {
            let layout = TickMarkLayout {
                width: 1000.0,
                bar_spacing: spacing,
                min_spacing: 80.0,
                uniform_distribution: false,
            };
            let visible = &weekdays[weekdays.len() - (1000.0 / spacing) as usize..];
            let marks = formatter.tick_marks(visible, &layout);
            assert!(!marks.is_empty());
            assert!(
                marks.iter().all(|mark| weekdays[mark.index].0 == mark.time),
                "{marks:?}"
            );
            assert!(marks.windows(2).all(|pair| pair[0].index < pair[1].index));
            let february = Utc.with_ymd_and_hms(2025, 2, 3, 0, 0, 0).unwrap();
            assert!(
                marks
                    .iter()
                    .any(|mark| mark.time == february && mark.label == "Feb"),
                "{spacing}: {marks:?}"
            );
        }
        // Several calendar candidates in one gap keep the strongest boundary; candidates
        // beyond the final supplied bar cannot produce a label.
        let sparse = [(start, 0), (start + Duration::days(35), 1)];
        let marks = generator.generate_marks(start, start + Duration::days(60), 1000.0, &sparse);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[1].time, sparse[1].0);
        assert_eq!(marks[1].mark_type, TickMarkType::Month);

        for interval in [Duration::days(2), Duration::days(7)] {
            let before = generator.generate_candidate_marks(
                bars[300].0,
                bars[400].0,
                interval,
                TickMarkType::DayOfMonth,
            );
            let after = generator.generate_candidate_marks(
                bars[301].0,
                bars[400].0,
                interval,
                TickMarkType::DayOfMonth,
            );
            let remaining: Vec<_> = before
                .iter()
                .filter(|mark| mark.time >= bars[301].0)
                .map(|mark| mark.time)
                .collect();
            assert_eq!(
                remaining,
                after.iter().map(|mark| mark.time).collect::<Vec<_>>()
            );
            if interval == Duration::days(7) {
                assert!(
                    after
                        .iter()
                        .filter(|mark| mark.weight < TickMarkWeight::MONTH)
                        .all(|mark| mark.time.weekday() == chrono::Weekday::Mon)
                );
            }
        }
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
    fn test_round_to_boundary_zero_interval_does_not_panic() {
        // A zero-millisecond interval has no boundary to snap to; integer
        // division by it would panic, so the time must pass through unchanged.
        let generator = TickMarkGenerator::new();
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();
        let rounded = generator.round_time_to_boundary(time, Duration::milliseconds(0));
        assert_eq!(rounded, time);
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
        // Shows the day number for day boundaries
        assert_eq!(
            generator.format_time_label(time, TickMarkType::DayOfMonth),
            "15"
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
