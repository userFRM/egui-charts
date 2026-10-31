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
    /// Default plot padding: 0px
    pub const PADDING: f32 = 0.0;

    /// Bottom padding with time labels: 35px
    pub const BOTTOM_PADDING_WITH_TIME: f32 = 35.0;

    /// Right axis width: 70px
    pub const RIGHT_AXIS_WIDTH: f32 = 70.0;
}
