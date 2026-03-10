//! Multi-chart layout system (stub)
//!
//! Provides type definitions for multi-chart synchronization options.
//! The full tiled layout implementation requires the `egui_tiles` crate
//! and is provided by the frontend application.

/// Configuration for multi-chart synchronization
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSyncOptions {
    /// Synchronize time axis (scroll/zoom) across all charts
    pub sync_time_axis: bool,
    /// Synchronize crosshair position
    pub sync_crosshair: bool,
    /// Synchronize drawing tools
    pub sync_drawings: bool,
    /// Synchronize symbol changes across all charts
    pub sync_symbol: bool,
    /// Synchronize timeframe changes across all charts
    pub sync_timeframe: bool,
}

impl Default for ChartSyncOptions {
    fn default() -> Self {
        Self {
            sync_time_axis: true,
            sync_crosshair: true,
            sync_drawings: false,
            sync_symbol: false,
            sync_timeframe: false,
        }
    }
}
