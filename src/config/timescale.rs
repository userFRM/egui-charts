//! Time-scale (X-axis) options and timezone handling.
//!
//! [`TimeScaleOptions`] configures bar spacing, scroll constraints, edge
//! fixing, and time-label visibility.  [`TimezoneMode`] controls how
//! UTC timestamps are converted before rendering axis labels.

/// Time Scale Options.
/// Controls bar spacing, scrolling constraints, and time display.
use chrono_tz::Tz;

/// Timezone display mode for time axis labels.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TimezoneMode {
    /// Display in UTC
    #[default]
    Utc,
    /// Display in user's local timezone
    Local,
    /// Display in specific timezone
    Timezone(Tz),
    /// Display in exchange/venue timezone (user must specify via Timezone)
    Exchange(Tz),
}

impl TimezoneMode {
    /// Common exchange timezones
    pub fn nyse() -> Self {
        TimezoneMode::Exchange(Tz::America__New_York)
    }

    pub fn lse() -> Self {
        TimezoneMode::Exchange(Tz::Europe__London)
    }

    pub fn jse() -> Self {
        TimezoneMode::Exchange(Tz::Asia__Tokyo)
    }

    pub fn hkex() -> Self {
        TimezoneMode::Exchange(Tz::Asia__Hong_Kong)
    }

    /// Get the underlying Tz for conversion
    pub fn to_tz(&self) -> Option<Tz> {
        match self {
            TimezoneMode::Utc => None,   // Already UTC
            TimezoneMode::Local => None, // Will use system local
            TimezoneMode::Timezone(tz) | TimezoneMode::Exchange(tz) => Some(*tz),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeScaleOptions {
    /// The margin space in bars from the right side of the chart.
    /// Default: 0
    pub right_offset: f32,

    /// The margin space in pixels from the right side of the chart.
    /// This option has priority over right_offset.
    /// Default: None
    pub right_offset_pixels: Option<f32>,

    /// The space between bars in pixels.
    /// Default: 6.0
    pub bar_spacing: f32,

    /// The min space between bars in pixels.
    /// Default: 0.5
    pub min_bar_spacing: f32,

    /// The max space between bars in pixels.
    /// Has no effect if value is set to 0 (unlimited).
    /// Default: 0.0
    pub max_bar_spacing: f32,

    /// Prevent scrolling to the left of the first bar.
    /// Default: false
    pub fix_left_edge: bool,

    /// Prevent scrolling to the right of the most recent bar.
    /// Default: false
    pub fix_right_edge: bool,

    /// Prevent changing the visible time range during chart resizing.
    /// Default: false
    pub lock_visible_time_range_on_resize: bool,

    /// Prevent the hovered bar from moving when scrolling.
    /// Default: false
    pub right_bar_stays_on_scroll: bool,

    /// Shift the visible range to the right (into the future) by the number of new bars when new data is added.
    /// Note that this only applies when the last bar is visible.
    /// Default: true
    pub shift_visible_range_on_new_bar: bool,

    /// Allow the visible range to be shifted to the right when a new bar is added which
    /// is replacing an existing whitespace time point on the chart.
    /// Default: false
    pub allow_shift_visible_range_on_whitespace_replacement: bool,

    /// Show the time, not just the date, in the time scale and vertical crosshair label.
    /// Default: false
    pub time_visible: bool,

    /// Show seconds in the time scale and vertical crosshair label in hh:mm:ss format for intraday data.
    /// Default: true
    pub seconds_visible: bool,

    /// Draw small vertical line on time axis labels.
    /// Default: false
    pub ticks_visible: bool,

    /// Changes horizontal scale marks generation.
    /// With this flag equal to true, marks of the same weight are either all drawn or none are drawn at all.
    /// Default: false
    pub uniform_distribution: bool,

    /// Allow major time scale labels to be rendered in a bolder font weight.
    /// Default: true
    pub allow_bold_labels: bool,

    /// Timezone mode for displaying ts
    /// Default: UTC
    pub timezone: TimezoneMode,
}

impl Default for TimeScaleOptions {
    fn default() -> Self {
        Self {
            right_offset: 2.5, // Default: keep 2-3 bars of whitespace on the right
            right_offset_pixels: None,
            bar_spacing: 6.0,
            min_bar_spacing: 0.5,
            max_bar_spacing: 0.0,
            fix_left_edge: true,   // Prevent scrolling beyond first bar
            fix_right_edge: false, // Allow scrolling past latest bar
            lock_visible_time_range_on_resize: false,
            right_bar_stays_on_scroll: false,
            shift_visible_range_on_new_bar: true,
            allow_shift_visible_range_on_whitespace_replacement: false,
            time_visible: false,
            seconds_visible: true,
            ticks_visible: false,
            uniform_distribution: false,
            allow_bold_labels: true,
            timezone: TimezoneMode::default(),
        }
    }
}

/// Zoom implementation
/// Calculates new bar spacing based on scale factor
pub(crate) fn calculate_zoom(
    _zoom_point_coord: f32,
    curr_bar_spacing: f32,
    scale: f32,
    min_bar_spacing: f32,
    max_bar_spacing: f32,
) -> f32 {
    // Formula: newBarSpacing = barSpacing + scale * (barSpacing / 10)
    // Scale is in 1/10 parts of current bar spacing
    let new_bar_spacing = curr_bar_spacing + scale * (curr_bar_spacing / 10.0);

    // Clamp to bounds

    if max_bar_spacing > 0.0 {
        new_bar_spacing.clamp(min_bar_spacing, max_bar_spacing)
    } else {
        new_bar_spacing.max(min_bar_spacing)
    }
}

/// Calculate right offset adjustment to keep point under cursor stable during zoom
pub(crate) fn calculate_zoom_offset_adjustment(
    zoom_point_before: f32,
    zoom_point_after: f32,
) -> f32 {
    zoom_point_before - zoom_point_after
}
