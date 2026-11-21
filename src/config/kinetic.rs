//! Kinetic (inertial) scroll animation parameters.
//!
//! Releasing a touch drag (and a mouse drag when `mouse` is set) while moving makes the chart
//! continue scrolling with decaying velocity, giving a "momentum" feel similar to mobile OS
//! list scrolling.
//!
//! Speeds are in points per frame and the damping is per frame, both at 60 frames per
//! second whatever the display's refresh rate: one point per frame is 60 points per second.

/// Kinetic scroll animation parameters.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct KineticScrollOptions {
    /// Continue scrolling after a horizontal touch drag is released while moving
    /// Default: true
    pub enabled: bool,

    /// Continue scrolling after a mouse drag is released, independently of `enabled`
    /// Default: false (a mouse drag stops on release)
    #[serde(default)]
    pub mouse: bool,

    /// Speed below which a release does not start scrolling and the scrolling ends, in
    /// points per frame at 60 frames per second.
    /// Accepts finite values above zero, since at zero the scrolling would never end; others are
    /// replaced by the default when chart options are applied.
    /// Default: 0.2 (12 points per second)
    pub min_scroll_speed: f32,

    /// Highest speed the scrolling starts at, in points per frame at 60 frames per second;
    /// a faster release is limited to it.
    /// Accepts values from zero up, infinity for no limit; NaN and negative values are replaced by
    /// the default when chart options are applied.
    /// Default: 50.0 (3000 points per second)
    pub max_scroll_speed: f32,

    /// Damping coefficient: the share of scroll velocity kept per frame at 60 frames per second.
    /// Accepts finite values in `0.0..1.0` (zero stops immediately, one is excluded).
    /// Invalid values are replaced by the default when chart options are applied.
    /// Default: 0.95 (a release at 2000 points per second scrolls on for 100 frames, 1.7 seconds)
    pub damping_coeff: f32,

    /// Horizontal distance a drag must end from its start for its release to start
    /// scrolling, in points
    /// Default: 15.0
    pub scroll_min_move: f32,
}

impl Default for KineticScrollOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            mouse: false,
            min_scroll_speed: 0.2,
            max_scroll_speed: 50.0,
            damping_coeff: 0.95,
            scroll_min_move: 15.0,
        }
    }
}
