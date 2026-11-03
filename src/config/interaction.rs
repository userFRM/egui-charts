//! Scroll, zoom, and tracking interaction options.
//!
//! Controls how mouse/touch gestures are interpreted: which gestures
//! pan the chart, which zoom it, and how live-edge following and touch
//! crosshair tracking behave.

/// How a touch crosshair activated by a long press is exited.
/// This does not control live-edge following.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TouchTrackingExitMode {
    /// Hide the crosshair and readout when the touch ends.
    #[default]
    OnTouchEnd,
    /// Keep the crosshair and readout after release until the next tap.
    OnNextTap,
}

/// How tracking mode (live-edge following) is exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrackingModeOptions {
    /// How to exit tracking mode
    /// Default: OnNextTap
    pub exit_mode: TrackingModeExitMode,
}

/// Handle Scroll Options
/// Controls how scrolling/panning interactions work

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HandleScrollOptions {
    /// Enable scroll gestures and their momentum without changing individual options.
    /// Default: true
    pub enabled: bool,

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
            enabled: true,
            mouse_wheel: true,
            pressed_mouse_move: true,
            horz_touch_drag: true,
            vert_touch_drag: true,
        }
    }
}

/// Axis-specific pressed mouse move options
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HandleScaleOptions {
    /// Enable wheel, pinch, axis drag and axis reset gestures without changing individual options.
    /// Default: true
    pub enabled: bool,

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
            enabled: true,
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

    /// Layouts saved before group switches retain their individual permissions and stay enabled.
    #[test]
    fn saved_handler_options_default_missing_group_switches_to_enabled() {
        let scroll: HandleScrollOptions = serde_json::from_value(serde_json::json!({
            "mouse_wheel": false, "pressed_mouse_move": true,
            "horz_touch_drag": false, "vert_touch_drag": true
        }))
        .unwrap();
        let scale: HandleScaleOptions = serde_json::from_value(serde_json::json!({
            "mouse_wheel": true, "pinch": false,
            "axis_pressed_mouse_move": {"time": false, "price": true},
            "axis_double_click_reset": {"time": true, "price": false}
        }))
        .unwrap();
        assert!(scroll.enabled && scale.enabled);
        assert!(!scroll.mouse_wheel && !scroll.horz_touch_drag);
        assert!(!scale.pinch && !scale.axis_pressed_mouse_move.time);
        assert!(!scale.axis_double_click_reset.price);
    }
}
