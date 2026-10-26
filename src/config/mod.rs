//! Appearance and interaction options for the chart widget.
//!
//! [`ChartConfig`] controls visual styling, while [`ChartOptions`] groups time scale,
//! scrolling, zoom, keyboard, crosshair and tooltip behavior. Start with `Default`
//! and set the fields needed by your application.

mod chart;
mod crosshair;
mod drawing_tool;
mod interaction;
mod keyboard;
mod kinetic;
mod timescale;
mod tooltip;

pub use chart::{
    BackgroundStyle, ChartConfig, NavigationButtons, PriceAxisOptions, RealtimeButtonPos,
    SessionConfig, WatermarkPos,
};
pub use crosshair::{CrosshairMode, CrosshairOptions, CrosshairStyle};
pub use drawing_tool::{DrawingInteractionMode, DrawingToolType, ParseToolTypeError};
pub use interaction::{
    AxisDoubleClickResetOptions, AxisPressedMouseMoveOptions, HandleScaleOptions,
    HandleScrollOptions, TrackingModeExitMode, TrackingModeOptions,
};
pub use keyboard::{KeyboardAction, KeyboardOptions};
pub use kinetic::KineticScrollOptions;
pub use timescale::TimeScaleOptions;
pub use tooltip::{TooltipMode, TooltipOptions};

pub use crate::scales::TimezoneMode;

/// Top-level chart interaction options.
///
/// Groups time scale, scrolling, zoom, keyboard, crosshair and tooltip settings
/// for the chart widget. Visual styling is configured separately with [`ChartConfig`].
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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

    /// Draw the main series or overlay study under the pointer above the others in the main
    /// pane, in the priority a click selects them.
    /// Default: false
    pub hovered_series_on_top: bool,
}

/// Font weight shared by chart watermarks and drawing annotations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FontWeight {
    /// Regular weight.
    #[default]
    Normal,
    /// Bold weight.
    Bold,
}
