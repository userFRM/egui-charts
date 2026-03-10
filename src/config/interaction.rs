//! Scroll, zoom, and tracking interaction options.
//!
//! Controls how mouse/touch gestures are interpreted: which gestures
//! pan the chart, which zoom it, and how the tracking-mode (crosshair
//! following) behaves.

/// How tracking mode (crosshair auto-follow) is exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackingModeExitMode {
    /// Exit tracking mode when touch/click ends
    OnTouchEnd,

    /// Exit tracking mode when mouse leaves the chart area
    OnMouseLeave,

    /// Exit tracking mode on the next tap/click
    #[default]
    OnNextTap,
}

/// Tracking mode options
/// Controls automatic price following behavior
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackingModeOptions {
    /// How to exit tracking mode
    /// Default: OnNextTap
    pub exit_mode: TrackingModeExitMode,
}

/// Handle Scroll Options
/// Controls how scrolling/panning interactions work

#[derive(Debug, Clone, Copy)]
pub struct HandleScrollOptions {
    /// Scroll with mouse wheel (horizontal)
    /// Default: true
    pub mouse_wheel: bool,

    /// Drag with left mouse button to pan
    /// Default: true
    pub pressed_mouse_move: bool,

    /// Horizontal drag on touch devices
    /// Default: true
    pub horz_touch_drag: bool,

    /// Vertical drag on touch devices
    /// Default: true
    pub vert_touch_drag: bool,
}

impl Default for HandleScrollOptions {
    fn default() -> Self {
        Self {
            mouse_wheel: true,
            pressed_mouse_move: true,
            horz_touch_drag: true,
            vert_touch_drag: true,
        }
    }
}

/// Axis-specific pressed mouse move options
#[derive(Debug, Clone, Copy)]
pub struct AxisPressedMouseMoveOptions {
    /// Drag along time axis (bottom)
    /// Default: true
    pub time: bool,

    /// Drag on price scale (right/left)
    /// Default: true
    pub price: bool,
}

impl Default for AxisPressedMouseMoveOptions {
    fn default() -> Self {
        Self {
            time: true,
            price: true,
        }
    }
}

/// Axis-specific double-click reset options
#[derive(Debug, Clone, Copy)]
pub struct AxisDoubleClickResetOptions {
    /// Double-click time axis to reset
    /// Default: true
    pub time: bool,

    /// Double-click price axis to reset
    /// Default: true
    pub price: bool,
}

impl Default for AxisDoubleClickResetOptions {
    fn default() -> Self {
        Self {
            time: true,
            price: true,
        }
    }
}

/// Handle Scale Options
/// Controls how zoom/scale interactions work

#[derive(Debug, Clone, Copy)]
pub struct HandleScaleOptions {
    /// Zoom with mouse wheel (vertical scroll)
    /// Default: true
    pub mouse_wheel: bool,

    /// Pinch zoom (touchpad / mobile)
    /// Default: true
    pub pinch: bool,

    /// Click + drag axes to zoom
    pub axis_pressed_mouse_move: AxisPressedMouseMoveOptions,

    /// Double-click to reset scaling
    pub axis_double_click_reset: AxisDoubleClickResetOptions,
}

impl Default for HandleScaleOptions {
    fn default() -> Self {
        Self {
            mouse_wheel: true,
            pinch: true,
            axis_pressed_mouse_move: AxisPressedMouseMoveOptions::default(),
            axis_double_click_reset: AxisDoubleClickResetOptions::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracking_mode_exit_mode_default() {
        let mode = TrackingModeExitMode::default();
        assert_eq!(mode, TrackingModeExitMode::OnNextTap);
    }

    #[test]
    fn test_tracking_mode_options_default() {
        let options = TrackingModeOptions::default();
        assert_eq!(options.exit_mode, TrackingModeExitMode::OnNextTap);
    }

    #[test]
    fn test_tracking_mode_exit_modes() {
        // Test all three exit modes
        let on_touch_end = TrackingModeExitMode::OnTouchEnd;
        let on_mouse_leave = TrackingModeExitMode::OnMouseLeave;
        let on_next_tap = TrackingModeExitMode::OnNextTap;

        // Verify they are distinct
        assert_ne!(on_touch_end, on_mouse_leave);
        assert_ne!(on_mouse_leave, on_next_tap);
        assert_ne!(on_next_tap, on_touch_end);
    }
}
