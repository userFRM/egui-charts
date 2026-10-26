//! Time-scale coordinate system (model layer).
//!
//! [`TimeScale`] manages the mapping between logical bar indices and pixel
//! coordinates on the horizontal axis.  It owns the bar spacing, scroll offset,
//! and edge constraints that control which bars are visible.
//!
//! [`LogicalRange`] represents a floating-point range of bar indices, which
//! can be converted to a strict integer range for data-array slicing.

/// A floating-point range of bar indices on the horizontal axis.
///
/// `left` and `right` may extend beyond `[0, bar_count)` when the chart is
/// scrolled past the data edges.  Use [`to_strict_range`](LogicalRange::to_strict_range)
/// to clamp to valid array indices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRange {
    /// Leftmost visible bar index (may be negative when scrolled past the start).
    pub left: f32,
    /// Rightmost visible bar index (may exceed `bar_count` when scrolled into the future).
    pub right: f32,
}

impl LogicalRange {
    pub fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }

    /// Convert to strict integer range for data access
    pub fn to_strict_range(&self) -> (usize, usize) {
        let start = self.left.floor().max(0.0) as usize;
        let end = self.right.ceil().max(0.0) as usize;
        (start, end)
    }

    /// Length of this range
    pub fn length(&self) -> f32 {
        self.right - self.left
    }
}

/// The minimum bar spacing used when the configured one is not a finite number.
const DEFAULT_MIN_BAR_SPACING: f32 = 0.5;

/// Time-scale coordinate engine -- pure logic, no UI dependencies.
///
/// Manages the horizontal mapping between logical bar indices and pixel
/// positions.  The chart widget reads from `TimeScale` to lay out candles and
/// writes back scroll/zoom deltas from user interaction.
///
/// # Coordinate model
///
/// The rightmost data point (the latest bar) is the *anchor*.  The
/// `right_offset` field shifts the anchor away from the right edge of the
/// chart to leave whitespace for price labels and visual breathing room.
///
/// ```text
///   bar index:  0  1  2  3  4  5  6  7  8  ·  ·
///               │  │  │  │  │  │  │  │  │        │
///               ◄──────── visible ──────────►     │
///               ◄──────────── width ─────────────►
///                                          ↑
///                                     right_offset
/// ```
#[derive(Debug, Clone)]
pub struct TimeScale {
    /// Bar spacing in pixels; retain fractional precision when fitting session gaps.
    bar_spacing: f64,
    /// Right offset in bars from the edge
    right_offset: f64,
    /// Configured right offset in bars, restored at the live edge
    live_right_offset: f32,
    /// Configured right offset in pixels, restored at the live edge; has priority over bars
    live_right_offset_pixels: Option<f32>,
    /// Width of the chart area in pixels
    width: f32,
    /// Total number of bars in dataset
    bar_cnt: usize,
    /// Min bar spacing constraint
    min_bar_spacing: f32,
    /// Max bar spacing constraint (0 = unlimited)
    max_bar_spacing: f32,
    /// Prevent scrolling past left edge
    fix_left_edge: bool,
    /// Prevent scrolling past right edge
    fix_right_edge: bool,
}

impl TimeScale {
    /// Create new TimeScale with default settings
    pub fn new() -> Self {
        Self {
            bar_spacing: 8.0,
            right_offset: 5.0, // Default: 5 bars of whitespace on the right
            live_right_offset: 2.5,
            live_right_offset_pixels: None,
            width: 800.0,
            bar_cnt: 0,
            min_bar_spacing: DEFAULT_MIN_BAR_SPACING,
            max_bar_spacing: 0.0, // unlimited
            fix_left_edge: false,
            fix_right_edge: false, // Allow scrolling past latest bar
        }
    }

    /// Apply options coming from `TimeScaleOptions`
    /// Note: this does not own `TimeScaleOptions` to avoid a dependency cycle.
    pub fn apply_options(
        &mut self,
        bar_spacing: f32,
        min_bar_spacing: f32,
        max_bar_spacing: f32,
        fix_left_edge: bool,
        fix_right_edge: bool,
        right_offset_bars: f32,
        right_offset_pixels: Option<f32>,
    ) {
        // Update constraints first so clamping of spacing uses new bounds
        self.min_bar_spacing = min_bar_spacing;
        self.max_bar_spacing = max_bar_spacing;
        self.fix_left_edge = fix_left_edge;
        self.fix_right_edge = fix_right_edge;

        // Apply spacing with clamping
        self.set_bar_spacing(bar_spacing);

        self.live_right_offset = right_offset_bars;
        self.live_right_offset_pixels = right_offset_pixels;
        self.set_right_offset(self.live_edge_offset());
    }

    /// The configured right offset in bars at the current spacing; the pixel
    /// option has priority over bars.
    pub(crate) fn live_edge_offset(&self) -> f32 {
        self.live_right_offset_pixels
            .map_or(self.live_right_offset as f64, |px| {
                px as f64 / self.bar_spacing
            }) as f32
    }

    /// Set the width of the chart area (must be called when chart resizes)
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
        // CRITICAL: Changing width changes visible_bars calculation in constraints
        self.apply_constraints();
    }

    /// Set the total number of bars in dataset
    pub fn set_bar_cnt(&mut self, count: usize) {
        self.bar_cnt = count;
        // CRITICAL: Changing bar_cnt affects max_offset calculation in constraints
        self.apply_constraints();
    }

    /// Set bar spacing (with clamping to constraints)
    pub fn set_bar_spacing(&mut self, spacing: f32) {
        self.bar_spacing = self.clamp_spacing(spacing as f64);
        // CRITICAL: Changing bar_spacing changes visible_bars calculation,
        // which affects edge constraints. Must re-apply constraints!
        self.apply_constraints();
    }

    /// Update min bar spacing
    pub fn set_min_bar_spacing(&mut self, min: f32) {
        self.min_bar_spacing = min.max(0.0);
        // Re-apply spacing to respect new constraints
        self.bar_spacing = self.clamp_spacing(self.bar_spacing);
        self.apply_constraints();
    }

    /// Update max bar spacing (0 = unlimited)
    pub fn set_max_bar_spacing(&mut self, max: f32) {
        self.max_bar_spacing = max.max(0.0);
        // Re-apply spacing to respect new constraints
        self.bar_spacing = self.clamp_spacing(self.bar_spacing);
        self.apply_constraints();
    }

    /// Configure whether left edge is fixed
    pub fn set_fix_left_edge(&mut self, fix: bool) {
        self.fix_left_edge = fix;
        self.apply_constraints();
    }

    /// Configure whether right edge is fixed
    pub fn set_fix_right_edge(&mut self, fix: bool) {
        self.fix_right_edge = fix;
        self.apply_constraints();
    }

    /// Set right offset (with constraint checking)
    pub fn set_right_offset(&mut self, offset: f32) {
        self.right_offset = offset as f64;
        self.apply_constraints();
    }

    /// Jump to latest bar position
    /// Restores the configured right offset, as [`scroll_to_realtime`](Self::scroll_to_realtime) does.
    pub fn jump_to_latest(&mut self) {
        self.scroll_to_realtime();
    }

    /// Get current bar spacing
    pub fn bar_spacing(&self) -> f32 {
        self.bar_spacing as f32
    }

    /// Get current right offset
    pub fn right_offset(&self) -> f32 {
        self.right_offset as f32
    }

    /// Within 1.5 slots of the live edge, including future whitespace.
    #[cfg(any(feature = "widget", test))]
    pub(crate) fn is_near_live_edge(&self) -> bool {
        self.right_offset >= -1.5
    }

    /// Get current width
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Get base index (last data point - coord anchor)
    pub fn base_idx(&self) -> usize {
        self.bar_cnt.saturating_sub(1)
    }

    /// Calculate visible logical range
    pub fn visible_logical_range(&self) -> LogicalRange {
        let base_idx = self.base_idx() as f64;
        let bars_len = self.width as f64 / self.bar_spacing;
        let right_border = base_idx + self.right_offset;
        let left_border = right_border - bars_len + 1.0;

        LogicalRange::new(left_border as f32, right_border as f32)
    }

    /// Convert logical index to x coord
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord(&self, index: usize, rect_min_x: f32, rect_width: f32) -> f32 {
        let base_idx = self.base_idx();
        let delta_from_right = base_idx as f64 + self.right_offset - index as f64;
        let relative_x = rect_width as f64 - (delta_from_right + 0.5) * self.bar_spacing - 1.0;
        (rect_min_x as f64 + relative_x) as f32
    }

    /// Convert fractional bar index to x coord (preserves sub-bar precision)
    /// Used for drawings which store fractional bar indices for precise positioning
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord_precise(&self, index: f32, rect_min_x: f32, rect_width: f32) -> f32 {
        let base_idx = self.base_idx() as f64;
        let delta_from_right = base_idx + self.right_offset - index as f64;
        let relative_x = rect_width as f64 - (delta_from_right + 0.5) * self.bar_spacing - 1.0;
        (rect_min_x as f64 + relative_x) as f32
    }

    /// Convert x coord to logical index
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being used,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn coord_to_idx(&self, x: f32, rect_min_x: f32, rect_width: f32) -> f32 {
        self.coord_to_idx_f64(x, rect_min_x, rect_width) as f32
    }

    /// Logical coordinates retain their precision across long gaps between bars.
    pub(crate) fn coord_to_idx_f64(&self, x: f32, rect_min_x: f32, rect_width: f32) -> f64 {
        let base_idx = self.base_idx() as f64;
        let relative_x = x as f64 - rect_min_x as f64;
        let delta_from_right = (rect_width as f64 - relative_x - 1.0) / self.bar_spacing - 0.5;
        base_idx + self.right_offset - delta_from_right
    }

    /// Fit fractional logical edges while retaining their sub-bar precision.
    #[cfg(feature = "widget")]
    pub(crate) fn fit_logical_range(&mut self, left: f64, right: f64) {
        self.bar_spacing = self.clamp_spacing((self.width as f64 - 1.0) / (right - left));
        self.right_offset = right - self.base_idx() as f64 - 0.5;
        self.apply_constraints();
    }

    /// Scroll by number of bars (negative = left, positive = right)
    pub fn scroll_bars(&mut self, bars: f32) {
        self.right_offset -= bars as f64;
        self.apply_constraints();
    }

    /// Scroll by pixels
    pub fn scroll_pixels(&mut self, pixels: f32) {
        self.right_offset -= pixels as f64 / self.bar_spacing;
        self.apply_constraints();
    }

    /// Zoom at a specific point
    ///
    /// IMPORTANT: rect_width must be the actual width of the chart rect being zoomed.
    pub fn zoom(&mut self, delta: f32, anchor_x: f32, rect_min_x: f32, rect_width: f32) {
        let old_spacing = self.bar_spacing;
        let _old_right_offset = self.right_offset;

        // Get the bar index at anchor point BEFORE zoom
        let anchor_bar_idx = self.coord_to_idx_f64(rect_min_x + anchor_x, rect_min_x, rect_width);

        // Calculate new bar spacing
        let zoom_scale = delta as f64;
        let new_spacing = old_spacing + zoom_scale * (old_spacing / 10.0);

        self.bar_spacing = self.clamp_spacing(new_spacing);

        // DIRECT CALCULATION: Calculate right_offset needed to keep anchor_bar_idx at anchor_x
        // Formula derived from idx_to_coord_precise:
        //   x = rect_min_x + rect_width - (base_idx + right_offset - bar_idx + 0.5) * bar_spacing - 1
        // Solving for right_offset:
        //   right_offset = bar_idx - base_idx + (rect_width - (x - rect_min_x) - 1) / bar_spacing - 0.5
        let base_idx = self.base_idx() as f64;
        let relative_anchor = anchor_x as f64; // anchor_x is already relative to rect_min_x
        let delta_from_right = (rect_width as f64 - relative_anchor - 1.0) / self.bar_spacing - 0.5;
        let calculated_offset = anchor_bar_idx - base_idx + delta_from_right;
        self.right_offset = calculated_offset;

        // Debug logging - check for width mismatch
        if (rect_width - self.width).abs() > 0.1 {
            log::warn!(
                "[ZOOM WIDTH MISMATCH] rect_width={:.1}, self.width={:.1}",
                rect_width,
                self.width
            );
        }

        // Apply constraints after setting right_offset
        let before_constraint = self.right_offset;
        self.apply_constraints();

        // Log if constraints modified right_offset
        if (self.right_offset - before_constraint).abs() > 0.01 {
            log::debug!(
                "[ZOOM CONSTRAINT] right_offset changed: {:.3} -> {:.3} (delta={:.3})",
                before_constraint,
                self.right_offset,
                self.right_offset - before_constraint
            );
        }
    }

    /// Fit all data in view, then restore the configured right offset.
    ///
    /// The bars and the configured offset in bars share the width, or the bars share the width
    /// less a configured pixel offset, so every bar stays in view.
    pub fn fit_content(&mut self) {
        if self.bar_cnt > 0 {
            let (pixels, bars) = self
                .live_right_offset_pixels
                .map_or((0.0, self.live_right_offset), |px| (px, 0.0));
            self.bar_spacing = self.clamp_spacing(
                (self.width as f64 - pixels as f64) / (self.bar_cnt as f64 + bars as f64).max(1.0),
            );
            self.scroll_to_realtime();
        }
    }

    /// Scroll to real-time (latest data)
    /// Restores the right offset given to [`apply_options`](Self::apply_options)
    /// (2.5 bars before it is called), the pixel offset converted at the
    /// current bar spacing.
    pub fn scroll_to_realtime(&mut self) {
        self.set_right_offset(self.live_edge_offset());
    }

    /// Apply edge constraints
    fn apply_constraints(&mut self) {
        if self.width <= 0.0 || self.bar_spacing <= 0.0 || self.bar_cnt == 0 {
            return;
        }

        // Calculate min and max right offset bounds
        let min_right = self.calculate_min_right_offset();
        let max_right = self.calculate_max_right_offset();

        // Ensure well-ordered bounds (min <= max)
        let (min_right_offset, max_right_offset) = if let Some(min_val) = min_right {
            if min_val <= max_right {
                (min_val, max_right)
            } else {
                (max_right, min_val)
            }
        } else {
            (f64::NEG_INFINITY, max_right)
        };

        // Clamp right_offset between bounds
        self.right_offset = self.right_offset.clamp(min_right_offset, max_right_offset);

        // Near-edge stabilization for fixed right edge
        if self.fix_right_edge && self.right_offset.abs() < 1e-6 {
            self.right_offset = 0.0;
        }
    }

    /// Calculate min allowed right_offset (most negative value).
    /// This limits how far we can scroll to the LEFT (showing older data): with
    /// `fix_left_edge` to the first bar at the left edge, also when every bar fits;
    /// without it to 100 bars past the first.
    fn calculate_min_right_offset(&self) -> Option<f64> {
        if self.bar_cnt == 0 || self.bar_spacing <= 0.0 || self.width <= 0.0 {
            return None;
        }

        if self.fix_left_edge {
            // The offset that puts the left border of `visible_logical_range` at bar 0.
            Some(self.width as f64 / self.bar_spacing - self.bar_cnt as f64)
        } else {
            Some(-(self.bar_cnt as f64 + 100.0))
        }
    }

    /// Calculate max allowed right_offset (most positive value).
    /// This limits how far we can scroll to the RIGHT (into the future).
    fn calculate_max_right_offset(&self) -> f64 {
        if self.bar_cnt == 0 {
            return 0.0;
        }

        if self.fix_right_edge {
            0.0
        } else {
            // Allow scrolling into future whitespace
            // Use a generous bound based on visible bars at minimum zoom

            self.width as f64 / self.spacing_bounds().0
        }
    }

    /// The spacing bounds in a form `clamp` accepts: a NaN or infinite minimum is the default, a NaN
    /// or infinite maximum is unlimited (0), and a minimum above a set maximum is that maximum.
    fn spacing_bounds(&self) -> (f64, f64) {
        let max = if self.max_bar_spacing.is_finite() {
            self.max_bar_spacing.max(0.0) as f64
        } else {
            0.0
        };
        let min = if self.min_bar_spacing.is_finite() {
            self.min_bar_spacing.max(0.0) as f64
        } else {
            DEFAULT_MIN_BAR_SPACING as f64
        };
        (if max > 0.0 { min.min(max) } else { min }, max)
    }

    fn clamp_spacing(&self, spacing: f64) -> f64 {
        match self.spacing_bounds() {
            (min, max) if max > 0.0 => spacing.clamp(min, max),
            (min, _) => spacing.max(min),
        }
    }

    /// Get number of visible candles
    pub fn visible_candles(&self) -> usize {
        (self.width as f64 / self.bar_spacing).floor() as usize
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returning to the live edge, by following it or by jumping back, restores
    /// the configured offset: 20 bars, or 60 px, which is 10 bars at a spacing
    /// of 6 and 5 bars once zoomed to 12.
    #[test]
    fn live_edge_keeps_the_configured_right_offset() {
        type Back = fn(&mut TimeScale);
        for (bars, pixels, spacing, expected) in [
            (20.0, None, 6.0, 20.0),
            (20.0, Some(60.0), 6.0, 10.0),
            (20.0, Some(60.0), 12.0, 5.0),
        ] {
            let mut ts = TimeScale::new();
            ts.set_bar_cnt(100);
            ts.apply_options(6.0, 0.5, 0.0, false, false, bars, pixels);
            ts.set_bar_spacing(spacing);
            let backs: [(&str, Back); 2] = [
                ("follow", TimeScale::scroll_to_realtime),
                ("jump", TimeScale::jump_to_latest),
            ];
            for (name, back) in backs {
                ts.scroll_bars(30.0);
                back(&mut ts);
                assert_eq!(
                    ts.right_offset(),
                    expected,
                    "{pixels:?} at {spacing}, {name}"
                );
            }
        }
    }

    /// Scrolling far into the past stops with the first bar at the left edge while
    /// `fix_left_edge` is set: 100 bars fit 800 px at a spacing of 8, so the right offset stops
    /// at 100 - 1,000 bars, and at 100 - 50 bars when every bar fits, leaving the empty space on
    /// the right. Unset, the view goes on to 100 bars past the first.
    #[test]
    fn fix_left_edge_stops_the_view_at_the_first_bar() {
        for (fix, bars, expected) in [
            (true, 1000, -900.0),
            (false, 1000, -1100.0),
            (true, 50, 50.0),
            (false, 50, -150.0),
        ] {
            let mut ts = TimeScale::new();
            ts.set_width(800.0);
            ts.set_bar_cnt(bars);
            ts.apply_options(8.0, 0.5, 0.0, fix, false, 2.5, None);
            ts.scroll_bars(5000.0);
            assert_eq!(ts.right_offset(), expected, "fix {fix}, {bars} bars");
        }
    }

    #[test]
    fn inconsistent_spacing_bounds_clamp_without_panicking() {
        // zoom(-100) asks for -9 times the spacing, so each row ends at its effective minimum.
        type Setup = fn(&mut TimeScale);
        let rows: [(&str, Setup, f32); 3] = [
            (
                "minimum above maximum",
                |ts| ts.apply_options(6.0, 10.0, 5.0, false, false, 2.0, None),
                5.0,
            ),
            (
                "NaN minimum",
                |ts| ts.apply_options(6.0, f32::NAN, 0.0, false, false, 2.0, None),
                0.5,
            ),
            (
                "minimum set above maximum",
                |ts| {
                    ts.set_max_bar_spacing(5.0);
                    ts.set_min_bar_spacing(10.0);
                },
                5.0,
            ),
        ];
        for (name, setup, expected) in rows {
            let mut ts = TimeScale::new();
            ts.set_bar_cnt(100);
            setup(&mut ts);
            ts.zoom(-100.0, 400.0, 0.0, 800.0);
            assert_eq!(ts.bar_spacing(), expected, "{name}");
        }
    }
}
