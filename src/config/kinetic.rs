//! Kinetic (inertial) scroll animation parameters.
//!
//! When enabled, releasing a scroll/drag gesture causes the chart to continue
//! scrolling with decaying velocity, giving a "momentum" feel similar to
//! mobile OS list scrolling.

/// Kinetic scroll animation params.
/// From lightweight-charts: gui/pane-widget.ts lines 37-42.
#[derive(Debug, Clone, Copy)]
pub struct KineticScrollOptions {
    /// Enable kinetic scrolling (inertial scrolling)
    /// Default: false (chart stops immediately on release)
    pub enabled: bool,

    /// Min scroll speed (pixels/frame)
    /// Default: 0.2
    pub min_scroll_speed: f32,

    /// Max scroll speed (pixels/frame)
    /// Default: 7.0
    pub max_scroll_speed: f32,

    /// Dumping coefficient (friction) per frame
    /// Default: 0.997 (~99.7% of speed retained each frame)
    pub dumping_coeff: f32,

    /// Min pixels moved to trigger kinetic scroll
    /// Default: 15.0
    pub scroll_min_move: f32,
}

impl Default for KineticScrollOptions {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default (no inertial scrolling)
            min_scroll_speed: 0.2,
            max_scroll_speed: 7.0,
            dumping_coeff: 0.997,
            scroll_min_move: 15.0,
        }
    }
}
