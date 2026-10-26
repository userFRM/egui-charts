//! Time-scale (X-axis) options.
//!
//! [`TimeScaleOptions`] configures bar spacing, scroll constraints, edge
//! fixing, time-label visibility and spacing, the axis size and border, and the
//! [`TimezoneMode`] used to convert UTC timestamps before rendering axis labels.

use crate::scales::TimezoneMode;
use egui::Color32;

/// Time Scale Options.
/// Controls bar spacing, scrolling constraints, and time display.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimeScaleOptions {
    /// The margin space in bars from the right side of the chart, kept
    /// whenever the view returns to the live edge.
    /// Default: 2.5
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

    /// Prevent scrolling to the left of the first bar: the view stops with the first bar at the
    /// left edge, and when every bar fits the empty space is on the right.
    /// Default: false
    pub fix_left_edge: bool,

    /// Prevent scrolling to the right of the most recent bar.
    /// Default: false
    pub fix_right_edge: bool,

    /// Prevent changing the visible time range during chart resizing.
    /// Default: false
    pub lock_visible_time_range_on_resize: bool,

    /// Keep the right offset when zooming with the mouse wheel, a pinch or the keyboard, so the
    /// bar at the right edge stays in place instead of the bar under the pointer.
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

    /// Include the clock in intraday time-scale and crosshair labels.
    /// Default: true
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

    /// Allow major time scale labels (day, month and year boundaries) to be rendered in a
    /// bolder font weight.
    /// Default: true
    pub allow_bold_labels: bool,

    /// The longest expected tick mark label in characters, which sets the minimum distance
    /// between time scale labels; raise it for a custom formatter with longer labels.
    /// Zero is treated as the default.
    /// Default: 8
    pub tick_mark_max_character_length: usize,

    /// The minimum height of the time scale in points. The time scale is taller when its
    /// labels need more space, and has no height while time labels are hidden.
    /// Default: 0.0
    pub minimum_height: f32,

    /// Draw a line along the top edge of the time scale.
    /// Default: false
    pub border_visible: bool,

    /// The colour of the time scale border; `None` uses the theme's border colour.
    /// Default: None
    pub border_color: Option<Color32>,

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
            fix_left_edge: false,  // Allow scrolling past the first bar
            fix_right_edge: false, // Allow scrolling past latest bar
            lock_visible_time_range_on_resize: false,
            right_bar_stays_on_scroll: false,
            shift_visible_range_on_new_bar: true,
            allow_shift_visible_range_on_whitespace_replacement: false,
            time_visible: true,
            seconds_visible: true,
            ticks_visible: false,
            uniform_distribution: false,
            allow_bold_labels: true,
            tick_mark_max_character_length: 8,
            minimum_height: 0.0,
            border_visible: false,
            border_color: None,
            timezone: TimezoneMode::default(),
        }
    }
}
