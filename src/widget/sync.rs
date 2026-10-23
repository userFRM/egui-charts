//! Multi-chart crosshair and time-scale synchronization.

use super::Chart;

impl Chart {
    // =========================================================================
    // Multi-Chart Sync Methods
    // =========================================================================

    /// Sets an external crosshair position from a synced chart (in bar-index coordinates).
    ///
    /// When set to `Some(idx)`, a crosshair is drawn at the given bar index even
    /// if the user is not hovering over this chart. Pass `None` to clear it.
    /// This is the receiver side of multi-chart crosshair synchronization. Indices count elements
    /// on price-based types; charts of different types still synchronize by index, not time.
    pub fn set_synced_crosshair_bar_idx(&mut self, bar_idx: Option<f64>) {
        self.synced_crosshair_bar_idx = bar_idx;
    }

    /// Returns the hovered index in the last frame, counting elements on price-based types.
    ///
    /// Returns `None` if the cursor was not over the chart. This is the emitter
    /// side of multi-chart crosshair synchronization: read this value and pass
    /// it to [`Chart::set_synced_crosshair_bar_idx`] on other charts.
    pub fn get_hover_bar_idx(&self) -> Option<f64> {
        self.last_hover_bar_idx
    }

    /// Applies time-scale state from another chart for synchronized scrolling/zooming.
    ///
    /// Sets both bar spacing and right offset to match the source chart so that
    /// both charts display the same time range. Use together with
    /// [`Chart::get_time_scale_state`] on the source chart.
    pub fn apply_synced_time_scale(&mut self, bar_spacing: f32, right_offset: f32) {
        self.state.time_scale_mut().set_bar_spacing(bar_spacing);
        self.state.time_scale_mut().set_right_offset(right_offset);
    }

    /// Returns the current time-scale state as `(bar_spacing, right_offset)`.
    ///
    /// This is the emitter side of multi-chart time-scale synchronization.
    /// Pass the returned values to [`Chart::apply_synced_time_scale`] on other
    /// charts to keep them scrolled/zoomed in unison.
    pub fn get_time_scale_state(&self) -> (f32, f32) {
        (
            self.state.time_scale().bar_spacing(),
            self.state.time_scale().right_offset(),
        )
    }
}
