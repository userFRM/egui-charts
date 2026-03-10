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
#[derive(Debug, Clone, Copy)]
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
    /// Bar spacing in pixels
    bar_spacing: f32,
    /// Right offset in bars from the edge
    right_offset: f32,
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
            width: 800.0,
            bar_cnt: 0,
            min_bar_spacing: 0.5,
            max_bar_spacing: 0.0, // unlimited
            fix_left_edge: true,
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

        // Pixels option has priority over bars
        let right_offset = if let Some(px) = right_offset_pixels {
            // Convert pixels to bars using current spacing
            px / self.bar_spacing
        } else {
            right_offset_bars
        };
        self.set_right_offset(right_offset);
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
        let clamped = if self.max_bar_spacing > 0.0 {
            spacing.clamp(self.min_bar_spacing, self.max_bar_spacing)
        } else {
            spacing.max(self.min_bar_spacing)
        };
        self.bar_spacing = clamped;
        // CRITICAL: Changing bar_spacing changes visible_bars calculation,
        // which affects edge constraints. Must re-apply constraints!
        self.apply_constraints();
    }

    /// Update min bar spacing
    pub fn set_min_bar_spacing(&mut self, min: f32) {
        self.min_bar_spacing = min.max(0.0);
        // Re-apply spacing to respect new constraints
        self.set_bar_spacing(self.bar_spacing);
    }

    /// Update max bar spacing (0 = unlimited)
    pub fn set_max_bar_spacing(&mut self, max: f32) {
        self.max_bar_spacing = max.max(0.0);
        // Re-apply spacing to respect new constraints
        self.set_bar_spacing(self.bar_spacing);
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
        self.right_offset = offset;
        self.apply_constraints();
    }

    /// Jump to latest bar position
    /// Resets right_offset to the default sticky offset (2.5 bars)
    pub fn jump_to_latest(&mut self) {
        const DEFAULT_RIGHT_OFFSET: f32 = 2.5;
        self.right_offset = DEFAULT_RIGHT_OFFSET;
    }

    /// Get current bar spacing
    pub fn bar_spacing(&self) -> f32 {
        self.bar_spacing
    }

    /// Get current right offset
    pub fn right_offset(&self) -> f32 {
        self.right_offset
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
        let base_idx = self.base_idx() as f32;
        let bars_len = self.width / self.bar_spacing;
        let right_border = base_idx + self.right_offset;
        let left_border = right_border - bars_len + 1.0;

        LogicalRange::new(left_border, right_border)
    }

    /// Convert logical index to x coord
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord(&self, index: usize, rect_min_x: f32, rect_width: f32) -> f32 {
        let base_idx = self.base_idx();
        let delta_from_right = base_idx as f32 + self.right_offset - index as f32;
        let relative_x = rect_width - (delta_from_right + 0.5) * self.bar_spacing - 1.0;
        rect_min_x + relative_x
    }

    /// Convert fractional bar index to x coord (preserves sub-bar precision)
    /// Used for drawings which store fractional bar indices for precise positioning
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord_precise(&self, index: f32, rect_min_x: f32, rect_width: f32) -> f32 {
        let base_idx = self.base_idx() as f32;
        let delta_from_right = base_idx + self.right_offset - index;
        let relative_x = rect_width - (delta_from_right + 0.5) * self.bar_spacing - 1.0;
        rect_min_x + relative_x
    }

    /// Convert x coord to logical index
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being used,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn coord_to_idx(&self, x: f32, rect_min_x: f32, rect_width: f32) -> f32 {
        let base_idx = self.base_idx() as f32;
        let relative_x = x - rect_min_x;
        let delta_from_right = (rect_width - relative_x - 1.0) / self.bar_spacing - 0.5;
        base_idx + self.right_offset - delta_from_right
    }

    /// Scroll by number of bars (negative = left, positive = right)
    pub fn scroll_bars(&mut self, bars: f32) {
        self.right_offset -= bars;
        self.apply_constraints();
    }

    /// Scroll by pixels
    pub fn scroll_pixels(&mut self, pixels: f32) {
        let bars = pixels / self.bar_spacing;
        self.scroll_bars(bars);
    }

    /// Zoom at a specific point
    ///
    /// IMPORTANT: rect_width must be the actual width of the chart rect being zoomed.
    pub fn zoom(&mut self, delta: f32, anchor_x: f32, rect_min_x: f32, rect_width: f32) {
        let old_spacing = self.bar_spacing;
        let _old_right_offset = self.right_offset;

        // Get the bar index at anchor point BEFORE zoom
        let anchor_bar_idx = self.coord_to_idx(rect_min_x + anchor_x, rect_min_x, rect_width);

        // Calculate new bar spacing
        let zoom_scale = delta;
        let new_spacing = old_spacing + zoom_scale * (old_spacing / 10.0);

        // Clamp bar spacing to constraints
        let clamped = if self.max_bar_spacing > 0.0 {
            new_spacing.clamp(self.min_bar_spacing, self.max_bar_spacing)
        } else {
            new_spacing.max(self.min_bar_spacing)
        };
        self.bar_spacing = clamped;

        // DIRECT CALCULATION: Calculate right_offset needed to keep anchor_bar_idx at anchor_x
        // Formula derived from idx_to_coord_precise:
        //   x = rect_min_x + rect_width - (base_idx + right_offset - bar_idx + 0.5) * bar_spacing - 1
        // Solving for right_offset:
        //   right_offset = bar_idx - base_idx + (rect_width - (x - rect_min_x) - 1) / bar_spacing - 0.5
        let base_idx = self.base_idx() as f32;
        let relative_anchor = anchor_x; // anchor_x is already relative to rect_min_x
        let delta_from_right = (rect_width - relative_anchor - 1.0) / self.bar_spacing - 0.5;
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

    /// Fit all data in view
    pub fn fit_content(&mut self) {
        if self.bar_cnt > 0 {
            let spacing = self.width / self.bar_cnt as f32;
            self.set_bar_spacing(spacing);
            self.right_offset = 0.0;
        }
    }

    /// Scroll to real-time (latest data)
    /// Maintains sticky offset - keeps ~2.5 bars of whitespace on the right
    /// for price labels and visual breathing room
    pub fn scroll_to_realtime(&mut self) {
        const REALTIME_OFFSET: f32 = 2.5;
        self.right_offset = REALTIME_OFFSET;
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
            (f32::NEG_INFINITY, max_right)
        };

        // Clamp right_offset between bounds
        self.right_offset = self.right_offset.clamp(min_right_offset, max_right_offset);

        // Near-edge stabilization for fixed right edge
        if self.fix_right_edge && self.right_offset.abs() < 1e-6 {
            self.right_offset = 0.0;
        }
    }

    /// Calculate min allowed right_offset (most negative value).
    /// This limits how far we can scroll to the LEFT (showing older data).
    /// Uses a permissive bound to allow viewing all historical data.
    fn calculate_min_right_offset(&self) -> Option<f32> {
        if self.bar_cnt == 0 || self.bar_spacing <= 0.0 || self.width <= 0.0 {
            return None;
        }

        // Allow scrolling to view all data plus some buffer
        Some(-(self.bar_cnt as f32 + 100.0))
    }

    /// Calculate max allowed right_offset (most positive value).
    /// This limits how far we can scroll to the RIGHT (into the future).
    fn calculate_max_right_offset(&self) -> f32 {
        if self.bar_cnt == 0 {
            return 0.0;
        }

        if self.fix_right_edge {
            0.0
        } else {
            // Allow scrolling into future whitespace
            // Use a generous bound based on visible bars at minimum zoom

            self.width / self.min_bar_spacing
        }
    }

    /// Get number of visible candles
    pub fn visible_candles(&self) -> usize {
        (self.width / self.bar_spacing).floor() as usize
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::new()
    }
}
