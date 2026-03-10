//! Animation and interaction state structs for the `Chart` widget.
//!
//! These structs hold ephemeral, per-frame interaction state used by the
//! [`pan_zoom`](super::pan_zoom) module to implement smooth scrolling,
//! box-zoom selection, and elastic edge bounce effects.

use egui::Pos2;
use web_time::Instant;

/// Tracks kinetic (momentum-based) scrolling after a flick gesture.
///
/// When the user drags and releases quickly (trackpad or touch), the chart
/// continues to scroll with decaying velocity, providing a natural "flick"
/// feel. The velocity is measured in pixels per second and decays each frame
/// using the configurable damping coefficient.
#[derive(Debug, Clone)]
pub struct KineticScrollState {
    /// Current scroll velocity in pixels/second (positive = scrolling right).
    pub velocity: f32,
    /// Last pointer position during drag (for velocity calculation).
    pub last_pos: Option<Pos2>,
    /// Timestamp of the last pointer sample.
    pub last_time: Option<Instant>,
    /// Whether kinetic animation is currently running.
    pub is_active: bool,
    /// Last animation frame time (for delta-time calculation).
    pub anim_last_time: Option<Instant>,
}

impl Default for KineticScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl KineticScrollState {
    /// Create a new idle kinetic scroll state with zero velocity.
    pub fn new() -> Self {
        Self {
            velocity: 0.0,
            last_pos: None,
            last_time: None,
            is_active: false,
            anim_last_time: None,
        }
    }
}

/// Box zoom mode (right-click drag functionality)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxZoomMode {
    /// Draw rect and zoom to selected region (default)
    Zoom,
    /// Draw measurement ruler showing bars/price change
    Measure,
}

/// Tracks an active box-zoom or box-measure selection.
///
/// The user initiates box zoom by left-clicking when zoom mode is active,
/// then drags to define a rectangular region. On release the chart either
/// zooms into the selection or displays measurement info, depending on
/// [`BoxZoomMode`].
#[derive(Debug, Clone)]
pub struct BoxZoomState {
    /// Whether box zoom is currently active
    pub active: bool,
    /// Start position of box drag
    pub start_pos: Option<Pos2>,
    /// Current mouse position during drag
    pub curr_pos: Option<Pos2>,
    /// Mode: Zoom or Measure
    pub mode: BoxZoomMode,
}

impl Default for BoxZoomState {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxZoomState {
    /// Create a new inactive box-zoom state.
    pub fn new() -> Self {
        Self {
            active: false,
            start_pos: None,
            curr_pos: None,
            mode: BoxZoomMode::Zoom,
        }
    }

    /// Reset the box-zoom to inactive, clearing start and current positions.
    pub fn reset(&mut self) {
        self.active = false;
        self.start_pos = None;
        self.curr_pos = None;
    }
}

/// Tracks elastic bounce-back animation when the user scrolls past chart edges.
///
/// When the chart reaches its data boundaries, a spring-like bounce animation
/// pushes the viewport back into valid range. This prevents the user from
/// scrolling into empty space and provides tactile feedback.
#[derive(Debug, Clone)]
pub struct ElasticBounceState {
    /// Bounce velocity (pixels per second)
    pub velocity: f32,
    /// Whether bounce animation is active
    pub active: bool,
    /// Last update time for animation
    pub last_time: Option<Instant>,
}

impl Default for ElasticBounceState {
    fn default() -> Self {
        Self::new()
    }
}

impl ElasticBounceState {
    /// Create a new idle elastic bounce state.
    pub fn new() -> Self {
        Self {
            velocity: 0.0,
            active: false,
            last_time: None,
        }
    }
}
