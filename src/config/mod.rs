//! Complete chart configuration options.
//!
//! This module provides comprehensive configuration for all aspects of chart
//! behavior and appearance, organized into focused sub-modules:
//!
//! | Sub-module     | Key types                        | Purpose                        |
//! |----------------|----------------------------------|--------------------------------|
//! | `chart`        | [`ChartConfig`], [`SessionConfig`] | Visual styling, session hours |
//! | `crosshair`    | [`CrosshairOptions`]             | Crosshair mode and colors      |
//! | `interaction`  | [`HandleScrollOptions`], [`HandleScaleOptions`] | Scroll/zoom behavior |
//! | `keyboard`     | [`KeyboardOptions`], [`KeyboardAction`] | Keyboard shortcuts      |
//! | `kinetic`      | [`KineticScrollOptions`]         | Inertial scroll animation      |
//! | `timescale`    | [`TimeScaleOptions`], [`TimezoneMode`] | Horizontal axis config   |
//! | `tooltip`      | [`TooltipOptions`], [`TooltipMode`] | Data tooltip display        |
//!
//! All sub-module types are re-exported at this level.  Use the top-level
//! [`ChartOptions`] struct to configure everything in one place.

mod chart;
mod crosshair;
mod interaction;
mod keyboard;
mod kinetic;
mod timescale;
mod tooltip;

pub use chart::{
    BackgroundStyle, ChartConfig, RealtimeButtonPos, SessionBreakStyle, SessionConfig, WatermarkPos,
};
pub use crosshair::{CrosshairLineStyle, CrosshairMode, CrosshairOptions, CrosshairStyle};
pub use interaction::{
    AxisDoubleClickResetOptions, AxisPressedMouseMoveOptions, HandleScaleOptions,
    HandleScrollOptions, TrackingModeExitMode, TrackingModeOptions,
};
pub use keyboard::{KeyboardAction, KeyboardOptions};
pub use kinetic::KineticScrollOptions;
pub use timescale::{TimeScaleOptions, TimezoneMode};
pub use tooltip::{TooltipMode, TooltipOptions};

/// Top-level chart interaction options.
///
/// Bundles all behavioural and interaction configuration into a single struct
/// that can be passed to the chart widget.  Each field corresponds to a
/// focused configuration sub-module.
#[derive(Debug, Clone, Default)]
pub struct ChartOptions {
    /// Time scale (horizontal) options
    pub time_scale: TimeScaleOptions,

    /// Scroll behavior options
    pub handle_scroll: HandleScrollOptions,

    /// Scale/zoom behavior options
    pub handle_scale: HandleScaleOptions,

    /// Kinetic scroll animation
    pub kinetic_scroll: KineticScrollOptions,

    /// Keyboard shortcuts
    pub keyboard: KeyboardOptions,

    /// Crosshair options
    pub crosshair: CrosshairOptions,

    /// Tracking mode options
    pub tracking_mode: TrackingModeOptions,

    /// Tooltip display options (Floating, Tracking, Magnifier)
    pub tooltip: TooltipOptions,
}
