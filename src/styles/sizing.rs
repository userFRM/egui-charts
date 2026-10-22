//! Chart layout sizing: fixed desktop constants.
//!
//! # Usage
//!
//! ```
//! use egui_charts::styles::sizing;
//!
//! let axis_width = sizing::chart::RIGHT_AXIS_WIDTH;
//! ```

// =============================================================================
// Chart Layout - Fixed Desktop Constants
// =============================================================================

/// Chart layout sizing - fixed desktop padding and margins
pub mod chart {
    /// Chart padding: 40px (desktop)
    pub const PADDING: f32 = 40.0;

    /// Top padding with OHLC: 40px
    pub const TOP_PADDING_WITH_OHLC: f32 = 40.0;

    /// Top padding without OHLC: 20px
    pub const TOP_PADDING_NO_OHLC: f32 = 20.0;

    /// Bottom padding with time labels: 30px
    pub const BOTTOM_PADDING_WITH_TIME: f32 = 30.0;

    /// Bottom padding without time labels: 20px
    pub const BOTTOM_PADDING_NO_TIME: f32 = 20.0;

    /// Right axis width: 70px
    pub const RIGHT_AXIS_WIDTH: f32 = 70.0;

    /// Minimum chart width: 100px
    pub const MIN_CHART_WIDTH: f32 = 100.0;

    /// Minimum chart height: 150px
    pub const MIN_CHART_HEIGHT: f32 = 150.0;
}
