//! Chart state management (model layer, no rendering).
//!
//! [`ChartState`] is the back-end owner of bar data and coordinate systems.
//! It tracks which bars are visible, manages price auto-scaling, and maintains
//! a zoom-history stack for undo/redo.

/// ChartState Backend - Pure state management (no rendering).
/// Combines data with coordinate systems.
use super::{BarData, TimeScale};

/// A snapshot of zoom parameters, used for undo/redo of zoom operations.
///
/// The chart maintains a stack of these snapshots so the user can press
/// "zoom out" to return to a previous view.
#[derive(Debug, Clone, Copy)]
pub struct ZoomState {
    /// Pixels per bar at the time of the snapshot.
    pub bar_spacing: f32,
    /// Right offset in bars from the chart edge.
    pub right_offset: f32,
    /// Manual price range override, if any.
    pub price_range: Option<(f64, f64)>,
}

/// Backend chart state -- owns bar data, time-scale coordinate system,
/// price auto-scaling logic, and zoom history.
///
/// This is the pure-logic layer with no UI or rendering dependencies.  The UI
/// layer reads from `ChartState` to determine what to draw and writes back
/// interaction results (scroll, zoom, data updates).
///
/// # Example
///
/// ```
/// use egui_charts::model::{BarData, ChartState};
///
/// let data = BarData::new();
/// let state = ChartState::new(data);
/// assert!(state.visible_data().is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct ChartState {
    /// The OHLCV bar dataset.
    data: BarData,
    /// Time coordinate system (logical index <-> pixel mapping).
    time_scale: TimeScale,
    /// Whether to auto-calculate the visible price range from data.
    price_auto_scale: bool,
    /// Manual price range override `(min, max)`, active when auto-scale is off.
    price_range: Option<(f64, f64)>,
    /// Stack of previous zoom snapshots for the "zoom out" button.
    zoom_history: Vec<ZoomState>,
}

impl ChartState {
    /// Create new chart state with data
    pub fn new(data: BarData) -> Self {
        let mut time_scale = TimeScale::new();
        time_scale.set_bar_cnt(data.len());

        Self {
            data,
            time_scale,
            price_auto_scale: true,
            price_range: None,
            zoom_history: Vec::new(),
        }
    }

    /// Get reference to bar data
    pub fn data(&self) -> &BarData {
        &self.data
    }

    /// Get mutable reference to time scale
    pub fn time_scale_mut(&mut self) -> &mut TimeScale {
        &mut self.time_scale
    }

    /// Get reference to time scale
    pub fn time_scale(&self) -> &TimeScale {
        &self.time_scale
    }

    /// Update data (also updates bar count in time scale)
    pub fn set_data(&mut self, data: BarData) {
        self.time_scale.set_bar_cnt(data.len());
        self.data = data;
    }

    /// Get visible data based on current time scale
    pub fn visible_data(&self) -> &[crate::model::Bar] {
        if self.data.bars.is_empty() {
            return &[];
        }

        let logical_range = self.time_scale.visible_logical_range();
        log::debug!(
            "[visible_data] logical_range: left={}, right={}",
            logical_range.left,
            logical_range.right
        );

        let (mut start, mut end) = logical_range.to_strict_range();
        log::debug!("[visible_data] BEFORE clamping: start={start}, end={end}");

        // Clamp to valid data range
        let data_len = self.data.len();
        end = end.min(data_len);
        start = start.min(end);

        log::debug!("[visible_data] AFTER clamping: start={start}, end={end}, data_len={data_len}");

        // Calculate how many bars should be visible
        let expected_visible = self.time_scale.visible_candles();

        // If we have too few bars visible (scrolled past beginning), extend the range
        let actual_visible = end.saturating_sub(start);
        log::debug!(
            "[visible_data] actual_visible={actual_visible}, expected_visible={expected_visible}"
        );

        if actual_visible < expected_visible && start == 0 && data_len > 1 {
            // We're at the beginning but don't have enough bars visible
            // Extend end to show the expected number of bars
            // Note: Only extend if we have more than 1 bar to avoid log spam during startup
            let old_end = end;
            end = expected_visible.min(data_len);
            if end > old_end {
                log::debug!(
                    "[visible_data] EXTENDING RANGE: old_end={}, new_end={} (gained {} bars)",
                    old_end,
                    end,
                    end - old_end
                );
            }
        }

        log::debug!(
            "[visible_data] FINAL RESULT: returning bars[{}..{}] ({} bars)",
            start,
            end,
            end - start
        );

        &self.data.bars[start..end]
    }

    /// Get visible data range indices
    pub fn visible_range(&self) -> (usize, usize) {
        if self.data.bars.is_empty() {
            return (0, 0);
        }

        let logical_range = self.time_scale.visible_logical_range();
        let (mut start, mut end) = logical_range.to_strict_range();

        log::debug!("[visible_range] BEFORE clamping: start={start}, end={end}");

        // Clamp to valid data range
        let data_len = self.data.len();
        end = end.min(data_len);
        start = start.min(end);

        // Calculate how many bars should be visible
        let expected_visible = self.time_scale.visible_candles();

        // If we have too few bars visible (scrolled past beginning), extend the range
        let actual_visible = end.saturating_sub(start);

        if actual_visible < expected_visible && start == 0 && data_len > 1 {
            // We're at the beginning but don't have enough bars visible
            // Extend end to show the expected number of bars
            // Note: Only extend if we have more than 1 bar to avoid log spam during startup
            let old_end = end;
            end = expected_visible.min(data_len);
            if end > old_end {
                log::debug!(
                    "[visible_range] EXTENDING RANGE: old_end={}, new_end={} (gained {} bars)",
                    old_end,
                    end,
                    end - old_end
                );
            }
        }

        log::debug!(
            "[visible_range] FINAL RESULT: ({}, {}) - {} bars",
            start,
            end,
            end - start
        );

        (start, end)
    }

    /// Enable/disable price auto-scaling
    pub fn set_price_auto_scale(&mut self, enabled: bool) {
        self.price_auto_scale = enabled;
        if enabled {
            self.price_range = None;
        }
    }

    /// Set manual price range
    pub fn set_price_range(&mut self, min: f64, max: f64) {
        self.price_auto_scale = false;
        self.price_range = Some((min, max));
    }

    /// Get price range (auto-calculated or manual)
    pub fn price_range(&self) -> (f64, f64) {
        if let Some((min, max)) = self.price_range {
            return (min, max);
        }

        // Auto-calculate from visible data
        let visible = self.visible_data();
        if visible.is_empty() {
            return (0.0, 100.0);
        }

        let vmin = visible
            .iter()
            .map(|c| c.low)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        let vmax = visible
            .iter()
            .map(|c| c.high)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        // Apply 10% margin
        let range = (vmax - vmin).max(1e-12);
        let margin = range * 0.1;
        (vmin - margin, vmax + margin)
    }

    /// Check if price auto-scaling is enabled
    pub fn is_price_auto_scale(&self) -> bool {
        self.price_auto_scale
    }

    /// Save current zoom state to history
    pub fn push_zoom_state(&mut self) {
        let curr_state = ZoomState {
            bar_spacing: self.time_scale.bar_spacing(),
            right_offset: self.time_scale.right_offset(),
            price_range: self.price_range,
        };
        self.zoom_history.push(curr_state);
    }

    /// Restore previous zoom state (zoom out button)
    pub fn pop_zoom_state(&mut self) -> bool {
        if let Some(state) = self.zoom_history.pop() {
            self.time_scale.set_bar_spacing(state.bar_spacing);
            self.time_scale.set_right_offset(state.right_offset);
            self.price_range = state.price_range;
            self.price_auto_scale = state.price_range.is_none();
            true
        } else {
            false
        }
    }

    /// Clear zoom history
    pub fn clear_zoom_history(&mut self) {
        self.zoom_history.clear();
    }

    /// Check if there are zoom states to restore
    pub fn has_zoom_history(&self) -> bool {
        !self.zoom_history.is_empty()
    }
}
