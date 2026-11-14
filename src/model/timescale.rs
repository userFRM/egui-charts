//! Time-scale coordinate system (model layer).
//!
//! [`TimeScale`] manages the mapping between logical bar indices and pixel
//! coordinates on the horizontal axis.  It owns the bar spacing, scroll offset,
//! and edge constraints that control which bars are visible.
//! Numeric scales retain row indices and space them by their encoded key differences.
//!
//! [`LogicalRange`] represents a floating-point range of bar indices, which
//! can be converted to a strict integer range for data-array slicing.

#[cfg(feature = "widget")]
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// A snapshot of numeric sample positions, shared by the chart's coordinate mappings.
///
/// Obtained from the chart's time scale or rendering mapping. Logical indices still address
/// rows; their positions are proportional to the encoded numeric keys. One spacing unit is
/// the first mean key gap, retained across updates, or one numeric unit until keys differ.
#[derive(Debug, Clone)]
pub struct NumericPositions {
    // Two edge slots retain both the first step and the later cadence of key interpolation.
    points: Arc<[f64]>,
    #[cfg(feature = "widget")]
    pub(crate) unit: Option<f64>,
    #[cfg(feature = "widget")]
    origin: DateTime<Utc>,
}

impl NumericPositions {
    #[cfg(feature = "widget")]
    pub(crate) fn new(points: Vec<f64>, unit: Option<f64>, origin: DateTime<Utc>) -> Self {
        Self {
            points: points.into(),
            unit,
            origin,
        }
    }

    #[cfg(feature = "widget")]
    pub(crate) fn index_at(&self, time: DateTime<Utc>) -> f64 {
        self.index(self.position_at(time))
    }

    #[cfg(feature = "widget")]
    pub(crate) fn position_at(&self, time: DateTime<Utc>) -> f64 {
        let elapsed = time - self.origin;
        let seconds = elapsed.num_seconds() as f64 + f64::from(elapsed.subsec_nanos()) / 1e9;
        self.position(0.0) + seconds / self.unit.unwrap_or(1.0)
    }

    #[cfg(feature = "widget")]
    pub(crate) fn min_gap(&self, left: f64, right: f64) -> f64 {
        if self.points.len() < 5 {
            return 1.0;
        }
        let points = &self.points[2..self.points.len() - 2];
        let start = points
            .partition_point(|&point| point < left)
            .saturating_sub(1);
        let end = (points.partition_point(|&point| point <= right) + 1).min(points.len());
        points[start.min(end)..end]
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .filter(|gap| *gap > 0.0)
            .fold(1.0, f64::min)
    }

    pub(crate) fn position(&self, index: f64) -> f64 {
        if self.points.len() < 2 || !index.is_finite() {
            return index;
        }
        let at = (index + 2.0)
            .floor()
            .clamp(0.0, (self.points.len() - 2) as f64) as usize;
        self.points[at] + (index + 2.0 - at as f64) * (self.points[at + 1] - self.points[at])
    }

    pub(crate) fn index(&self, position: f64) -> f64 {
        if self.points.len() < 2 || !position.is_finite() {
            return position;
        }
        let right = self
            .points
            .partition_point(|&point| point < position)
            .clamp(1, self.points.len() - 1);
        let gap = self.points[right] - self.points[right - 1];
        right as f64 - 3.0
            + if gap > 0.0 {
                (position - self.points[right - 1]) / gap
            } else {
                0.0
            }
    }
}

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
    /// Construct a range from its left and right logical indices.
    pub fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }

    /// Convert to an end-exclusive integer range, including partially visible edge slots.
    pub fn to_strict_range(&self) -> (usize, usize) {
        let start = self.left.floor().max(0.0) as usize;
        let end = (f64::from(self.right).ceil() + 1.0).max(0.0) as usize;
        (start, end)
    }

    /// Length of this range
    pub fn length(&self) -> f32 {
        self.right - self.left
    }
}

/// The minimum bar spacing used when the configured one is not a finite number.
const DEFAULT_MIN_BAR_SPACING: f32 = 0.5;

/// Keep spacing divisors positive without restricting subpixel views of large histories.
const MIN_BAR_SPACING: f32 = 1e-6;

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
    /// Numeric positions, or ordinal spacing when absent.
    numeric_positions: Option<NumericPositions>,
    #[cfg(feature = "widget")]
    numeric_revision: Option<(u64, usize, Option<chrono::DateTime<chrono::Utc>>)>,
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
    /// Extent of participating series in spacing units, relative to the main series.
    content_range: Option<(f64, f64)>,
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
            numeric_positions: None,
            #[cfg(feature = "widget")]
            numeric_revision: None,
            bar_spacing: 8.0,
            right_offset: 5.0, // Default: 5 bars of whitespace on the right
            live_right_offset: 2.5,
            live_right_offset_pixels: None,
            width: 800.0,
            bar_cnt: 0,
            content_range: None,
            min_bar_spacing: DEFAULT_MIN_BAR_SPACING,
            max_bar_spacing: 0.0, // unlimited
            fix_left_edge: false,
            fix_right_edge: false, // Allow scrolling past latest bar
        }
    }

    /// Apply options coming from `TimeScaleOptions`
    /// Note: this does not own `TimeScaleOptions` to avoid a dependency cycle.
    /// Non-finite spacing retains the current spacing, clamped to the new bounds.
    /// Non-finite right offsets retain their previous values.
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

        if right_offset_bars.is_finite() {
            self.live_right_offset = right_offset_bars;
        }
        if right_offset_pixels.is_none_or(f32::is_finite) {
            self.live_right_offset_pixels = right_offset_pixels;
        }
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

    /// Set the width of the chart area (must be called when chart resizes).
    /// Non-finite values are ignored.
    pub fn set_width(&mut self, width: f32) {
        if !width.is_finite() {
            return;
        }
        self.width = width;
        // CRITICAL: Changing width changes visible_bars calculation in constraints
        self.apply_constraints();
    }

    /// Set the total number of bars in dataset
    pub fn set_bar_cnt(&mut self, count: usize) {
        let moved = count.saturating_sub(1) as f64 - self.base_idx() as f64;
        self.bar_cnt = count;
        // The cached right end stays as far from the last slot until the next refresh, so that
        // the right offset is not clamped against the last slot of the previous frame.
        if let Some((_, right)) = &mut self.content_range {
            *right += moved;
        }
        // CRITICAL: Changing bar_cnt affects max_offset calculation in constraints
        self.apply_constraints();
    }

    #[cfg(feature = "widget")]
    pub(crate) fn set_content_range(&mut self, range: Option<(f64, f64)>) {
        self.content_range = range;
        self.apply_constraints();
    }

    fn content_range(&self) -> (f64, f64) {
        self.content_range
            .unwrap_or((self.position(0.0), self.base_idx() as f64))
    }

    /// Numeric sample positions used by this scale, or `None` for ordinal time charts.
    pub fn numeric_positions(&self) -> Option<&NumericPositions> {
        self.numeric_positions.as_ref()
    }

    #[cfg(feature = "widget")]
    pub(crate) fn set_numeric_positions(&mut self, positions: Option<NumericPositions>) {
        self.numeric_positions = positions;
        self.numeric_revision = None;
        self.apply_constraints();
    }

    #[cfg(feature = "widget")]
    pub(crate) fn refresh_numeric_positions(&mut self, bars: &[crate::model::Bar], revision: u64) {
        let key = (revision, bars.len(), bars.last().map(|bar| bar.time));
        if let Some(old) = &self.numeric_positions
            && self.numeric_revision != Some(key)
        {
            let new = crate::model::numeric_positions(bars, old.unit);
            if !old.points.is_empty() && !bars.is_empty() {
                let shift = new.position_at(old.origin) - old.position(0.0);
                let base = self.base_idx() as f64;
                let last = (bars.len() - 1) as f64;
                self.right_offset += base + shift - last;
                if let Some((left, right)) = &mut self.content_range {
                    *left = (*left + shift).min(new.position(0.0));
                    // set_bar_cnt moves this cached edge by the row-count change next.
                    *right = (*right + shift).max(last) - (last - base);
                }
            }
            self.numeric_positions = Some(new);
            self.numeric_revision = Some(key);
        }
    }

    pub(crate) fn position(&self, index: f64) -> f64 {
        self.numeric_positions
            .as_ref()
            .map_or(index, |positions| positions.position(index))
    }

    fn index(&self, position: f64) -> f64 {
        self.numeric_positions
            .as_ref()
            .map_or(position, |positions| positions.index(position))
    }

    /// Set bar spacing (with clamping to constraints). Non-finite values are ignored.
    pub fn set_bar_spacing(&mut self, spacing: f32) {
        self.bar_spacing = self.clamp_spacing(spacing as f64);
        // CRITICAL: Changing bar_spacing changes visible_bars calculation,
        // which affects edge constraints. Must re-apply constraints!
        self.apply_constraints();
    }

    /// Sets the width and scales the bar spacing so a plot resized from `old_width` to
    /// `new_width` shows the same slots. The plot spans `width - 1` pixels of slots, as in
    /// [`Self::coord_to_idx`].
    #[cfg(feature = "widget")]
    pub(crate) fn keep_slots_on_resize(&mut self, old_width: f32, new_width: f32) {
        let ratio = (f64::from(new_width) - 1.0) / (f64::from(old_width) - 1.0);
        self.width = new_width;
        self.bar_spacing = self.clamp_spacing(self.bar_spacing * ratio);
        self.apply_constraints();
    }

    /// Update min bar spacing, floored at 0.000001 pixels. Non-finite values are ignored.
    pub fn set_min_bar_spacing(&mut self, min: f32) {
        if !min.is_finite() {
            return;
        }
        self.min_bar_spacing = min.max(0.0);
        // Re-apply spacing to respect new constraints
        self.bar_spacing = self.clamp_spacing(self.bar_spacing);
        self.apply_constraints();
    }

    /// Update max bar spacing (0 = unlimited), with positive limits floored at 0.000001 pixels.
    /// Non-finite values are ignored.
    pub fn set_max_bar_spacing(&mut self, max: f32) {
        if !max.is_finite() {
            return;
        }
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

    /// Set right offset (with constraint checking). Non-finite values are ignored.
    pub fn set_right_offset(&mut self, offset: impl Into<f64>) {
        let offset = offset.into();
        if !offset.is_finite() {
            return;
        }
        self.right_offset = offset;
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

    /// Get current right offset. [`Self::right_offset_precise`] retains sub-bar precision.
    pub fn right_offset(&self) -> f32 {
        self.right_offset as f32
    }

    /// The scroll offset without narrowing fractional movement far into a large history.
    pub fn right_offset_precise(&self) -> f64 {
        self.right_offset
    }

    /// Within 1.5 slots of the live edge, including future whitespace.
    #[cfg(feature = "widget")]
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
        let bars_len = (self.width as f64 - 1.0) / self.bar_spacing;
        let right_border = base_idx + self.right_offset;
        let left_border = right_border - bars_len + 1.0;

        LogicalRange::new(
            self.index(left_border) as f32,
            self.index(right_border) as f32,
        )
    }

    /// Numeric samples can be much closer than a mean-gap slot; cull at the plot edges.
    pub(crate) fn visible_data_range(&self) -> LogicalRange {
        if self.numeric_positions.is_none() {
            return self.visible_logical_range();
        }
        let (left, right) = self.visible_positions();
        LogicalRange::new(
            (self.index(left) + 0.5) as f32,
            (self.index(right) - 0.5) as f32,
        )
    }

    /// Plot boundaries in spacing units, retaining precision across large numeric gaps.
    pub(crate) fn visible_positions(&self) -> (f64, f64) {
        let right = self.base_idx() as f64 + self.right_offset + 0.5;
        (
            right - (f64::from(self.width) - 1.0) / self.bar_spacing,
            right,
        )
    }

    /// Convert logical index to x coord
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord(&self, index: usize, rect_min_x: f32, rect_width: f32) -> f32 {
        self.idx_to_coord_precise(index as f64, rect_min_x, rect_width)
    }

    /// Convert fractional bar index to x coord (preserves sub-bar precision)
    /// Used for drawings which store fractional bar indices for precise positioning
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being drawn in,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn idx_to_coord_precise(
        &self,
        index: impl Into<f64>,
        rect_min_x: f32,
        rect_width: f32,
    ) -> f32 {
        let delta_from_right =
            (self.base_idx() as f64 - self.position(index.into())) + self.right_offset;
        (f64::from(rect_min_x) + f64::from(rect_width)
            - (delta_from_right + 0.5) * self.bar_spacing
            - 1.0) as f32
    }

    /// Convert x coord to logical index
    ///
    /// IMPORTANT: rect_width must be the actual width of the rect being used,
    /// NOT self.width (which may differ from the chart area's rect).
    pub fn coord_to_idx(&self, x: f32, rect_min_x: f32, rect_width: f32) -> f32 {
        self.coord_to_idx_precise(x, rect_min_x, rect_width) as f32
    }

    /// Convert x to a fractional logical index without narrowing a large history to `f32`.
    /// `rect_width` is the actual width of the chart rect, as in [`Self::coord_to_idx`].
    pub fn coord_to_idx_precise(&self, x: f32, rect_min_x: f32, rect_width: f32) -> f64 {
        let relative_x = f64::from(x) - f64::from(rect_min_x);
        let delta_from_right = (f64::from(rect_width) - relative_x - 1.0) / self.bar_spacing - 0.5;
        self.index(self.base_idx() as f64 + (self.right_offset - delta_from_right))
    }

    /// Fit fractional edges, centring spans that exceed the configured or fallback spacing cap.
    /// A fit that would leave the last bar left of the plot shows the newest bars when the span
    /// exceeds the plot at the minimum spacing, and keeps the last bar's slot in view otherwise.
    #[cfg(feature = "widget")]
    pub(crate) fn fit_logical_range(&mut self, left: f64, right: f64, default_max: f64) {
        let (left, right) = (self.position(left), self.position(right));
        let (min, max) = self.spacing_bounds();
        let max = if max > 0.0 { max } else { default_max.max(min) };
        let spacing = (self.width as f64 - 1.0) / (right - left);
        self.bar_spacing = self.clamp_spacing(spacing.min(max));
        let slots = (self.width as f64 - 1.0) / self.bar_spacing;
        let right = if spacing > max {
            (left + right) / 2.0 + slots / 2.0
        } else {
            right
        };
        let last = self.base_idx() as f64;
        let right = if right - slots <= last {
            right
        } else if spacing < min {
            last + 0.5
        } else {
            // A slot wider than the plot centres the bar instead.
            last + (slots - 0.5).max(slots / 2.0)
        };
        self.right_offset = right - last - 0.5;
        self.apply_constraints();
    }

    /// Scroll by number of bars (negative = left, positive = right).
    /// Non-finite deltas are ignored.
    pub fn scroll_bars(&mut self, bars: f32) {
        if !bars.is_finite() {
            return;
        }
        self.set_right_offset(self.right_offset - f64::from(bars));
    }

    /// Scroll by pixels. Non-finite deltas are ignored.
    pub fn scroll_pixels(&mut self, pixels: f32) {
        if !pixels.is_finite() {
            return;
        }
        self.set_right_offset(self.right_offset - pixels as f64 / self.bar_spacing);
    }

    /// Zoom at a specific point
    ///
    /// IMPORTANT: rect_width must be the actual width of the chart rect being zoomed.
    /// Non-finite deltas, anchors and widths are ignored.
    pub fn zoom(&mut self, delta: f32, anchor_x: f32, _rect_min_x: f32, rect_width: f32) {
        if !delta.is_finite() || !anchor_x.is_finite() || !rect_width.is_finite() {
            return;
        }
        let old_spacing = self.bar_spacing;
        let anchor_distance = f64::from(rect_width) - f64::from(anchor_x) - 1.0;

        // Calculate new bar spacing
        let zoom_scale = delta as f64;
        let new_spacing = old_spacing + zoom_scale * (old_spacing / 10.0);

        self.bar_spacing = self.clamp_spacing(new_spacing);

        // The absolute bar index cancels: only the change in distance from the right matters.
        let right_offset = self.right_offset
            + (anchor_distance / self.bar_spacing - anchor_distance / old_spacing);

        // Debug logging - check for width mismatch
        if (rect_width - self.width).abs() > 0.1 {
            log::warn!(
                "[ZOOM WIDTH MISMATCH] rect_width={:.1}, self.width={:.1}",
                rect_width,
                self.width
            );
        }

        // Apply constraints after setting right_offset
        let before_constraint = right_offset;
        self.set_right_offset(right_offset);

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

    /// Fit all participating data in view, reserving the configured right offset after it.
    ///
    /// The bars and the configured offset in bars share the width, or the bars share the width
    /// less a configured pixel offset. Spacing limits take precedence; a fixed right edge
    /// suppresses the margin.
    pub fn fit_content(&mut self) {
        if self.bar_cnt > 0 {
            let (left, right) = self.content_range();
            let (pixels, bars) = if self.fix_right_edge {
                (0.0, 0.0)
            } else {
                self.live_right_offset_pixels
                    .map_or((0.0, self.live_right_offset), |px| (px, 0.0))
            };
            self.bar_spacing = self.clamp_spacing(
                (self.width as f64 - 1.0 - pixels as f64)
                    / (right - left + 1.0 + bars as f64).max(1.0),
            );
            let margin = if self.fix_right_edge {
                0.0
            } else {
                f64::from(self.live_edge_offset())
            };
            self.set_right_offset(right - self.base_idx() as f64 + margin);
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
        if !(self.width > 0.0 && self.bar_spacing > 0.0) || self.bar_cnt == 0 {
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
        if self.bar_cnt == 0 || !(self.width > 0.0 && self.bar_spacing > 0.0) {
            return None;
        }

        if self.fix_left_edge {
            // Keep the first participating slot at the left border.
            Some(
                self.content_range().0 - self.base_idx() as f64
                    + (self.width as f64 - 1.0) / self.bar_spacing
                    - 1.0,
            )
        } else {
            Some(self.content_range().0 - self.base_idx() as f64 - 101.0)
        }
    }

    /// Calculate max allowed right_offset (most positive value).
    /// This limits how far we can scroll to the RIGHT (into the future).
    fn calculate_max_right_offset(&self) -> f64 {
        if self.bar_cnt == 0 {
            return 0.0;
        }

        let last = self.content_range().1 - self.base_idx() as f64;
        if self.fix_right_edge {
            last
        } else {
            // Allow scrolling into future whitespace
            // Use a generous bound based on visible bars at minimum zoom

            last + self.width as f64 / self.spacing_bounds().0
        }
    }

    /// The spacing bounds in a form `clamp` accepts: a NaN or infinite minimum is the default, a NaN
    /// or infinite maximum is unlimited (0), and a minimum above a set maximum is that maximum.
    /// Both the minimum and a positive maximum have a small positive floor.
    fn spacing_bounds(&self) -> (f64, f64) {
        let max = if self.max_bar_spacing.is_finite() && self.max_bar_spacing > 0.0 {
            self.max_bar_spacing.max(MIN_BAR_SPACING) as f64
        } else {
            0.0
        };
        let min = if self.min_bar_spacing.is_finite() {
            self.min_bar_spacing.max(MIN_BAR_SPACING) as f64
        } else {
            DEFAULT_MIN_BAR_SPACING as f64
        };
        (if max > 0.0 { min.min(max) } else { min }, max)
    }

    fn clamp_spacing(&self, spacing: f64) -> f64 {
        let spacing = if spacing.is_finite() {
            spacing
        } else {
            self.bar_spacing
        };
        match self.spacing_bounds() {
            (min, max) if max > 0.0 => spacing.clamp(min, max),
            (min, _) => spacing.max(min),
        }
    }

    /// The slots across the plot's `width - 1` pixels, rounded, so the count given to
    /// `Chart::set_visible_bars` reads back.
    pub fn visible_candles(&self) -> usize {
        ((self.width as f64 - 1.0) / self.bar_spacing).round() as usize
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

    /// At 200 points per bar a one-point pan is 0.005 bars, even with a million-bar anchor.
    #[test]
    fn large_history_keeps_small_pans_and_zoom_anchors() {
        for (offset, index) in [(0.0, 999_999), (-500_000.0, 499_999)] {
            let mut scale = TimeScale::new();
            scale.set_bar_cnt(1_000_000);
            scale.set_width(800.0);
            scale.set_bar_spacing(200.0);
            scale.set_right_offset(offset);
            assert_eq!(scale.idx_to_coord(index, 0.0, 800.0), 699.0);
            scale.scroll_pixels(1.0);
            assert_eq!(scale.idx_to_coord(index, 0.0, 800.0), 700.0);
            let anchor = scale.idx_to_coord(index - 2, 0.0, 800.0);
            for delta in [1.0, -1.0, 2.0] {
                scale.zoom(delta, anchor, 0.0, 800.0);
                assert!((scale.idx_to_coord(index - 2, 0.0, 800.0) - anchor).abs() < 1e-3);
            }
        }
    }

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
    /// `fix_left_edge` is set: the 799 pixels of an 800 px plot hold 99.875 bars at a spacing of 8,
    /// so the right offset stops at 99.875 - 1,000 bars, and at 99.875 - 50 bars when every bar
    /// fits, leaving the empty space on the right. Unset, the view goes on to 100 bars past the
    /// first.
    #[test]
    fn fix_left_edge_stops_the_view_at_the_first_bar() {
        for (fix, bars, expected) in [
            (true, 1000, -900.125),
            (false, 1000, -1100.0),
            (true, 50, 49.875),
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
    fn non_finite_inputs_keep_the_time_scale_state() {
        type Change = fn(&mut TimeScale, f32);
        let changes: [(&str, Change); 11] = [
            ("width", TimeScale::set_width),
            ("spacing", TimeScale::set_bar_spacing),
            ("minimum spacing", TimeScale::set_min_bar_spacing),
            ("maximum spacing", TimeScale::set_max_bar_spacing),
            ("offset", |ts, value| ts.set_right_offset(value)),
            ("bars", TimeScale::scroll_bars),
            ("pixels", TimeScale::scroll_pixels),
            ("zoom delta", |ts, value| ts.zoom(value, 400.0, 0.0, 800.0)),
            ("zoom anchor", |ts, value| ts.zoom(1.0, value, 0.0, 800.0)),
            ("zoom width", |ts, value| ts.zoom(1.0, 400.0, 0.0, value)),
            ("options", |ts, value| {
                ts.apply_options(value, 0.5, 50.0, true, false, value, Some(value));
            }),
        ];
        for count in [0, 100] {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                for (name, change) in changes {
                    let mut ts = TimeScale::new();
                    ts.set_bar_cnt(count);
                    ts.apply_options(8.0, 0.5, 50.0, true, false, 7.0, Some(80.0));
                    change(&mut ts, invalid);
                    assert_eq!(ts.width(), 800.0, "{name}, {invalid}");
                    assert_eq!(ts.bar_spacing(), 8.0, "{name}, {invalid}");
                    assert_eq!(ts.right_offset(), 10.0, "{name}, {invalid}");
                    ts.set_bar_cnt(100);
                    ts.scroll_bars(1.0);
                    ts.scroll_to_realtime();
                    assert_eq!(ts.right_offset(), 10.0, "{name}, {invalid}");
                    ts.set_bar_spacing(0.0);
                    assert_eq!(ts.bar_spacing(), 0.5, "{name}, {invalid}");
                    ts.set_bar_spacing(100.0);
                    assert_eq!(ts.bar_spacing(), 50.0, "{name}, {invalid}");
                    ts.apply_options(8.0, 0.5, 50.0, true, false, invalid, None);
                    assert_eq!(ts.right_offset(), 7.0, "bar offset, {invalid}");
                    assert_eq!(ts.visible_logical_range(), LogicalRange::new(7.125, 106.0));
                }
            }
        }
    }

    #[test]
    fn inconsistent_spacing_bounds_clamp_without_panicking() {
        // Zoom-outs stop at the effective minimum and leave zooming and scrolling usable.
        type Setup = fn(&mut TimeScale);
        let rows: [(&str, Setup, (f32, f32)); 10] = [
            (
                "zero minimum",
                |ts| ts.set_min_bar_spacing(0.0),
                (8.0, 1e-6),
            ),
            (
                "negative minimum option",
                |ts| ts.apply_options(6.0, -1.0, 0.0, false, false, 2.0, None),
                (6.0, 1e-6),
            ),
            (
                "maximum below floor",
                |ts| ts.set_max_bar_spacing(1e-8),
                (1e-6, 1e-6),
            ),
            (
                "NaN minimum setter",
                |ts| ts.set_min_bar_spacing(f32::NAN),
                (8.0, 0.5),
            ),
            (
                "minimum above maximum",
                |ts| ts.apply_options(6.0, 10.0, 5.0, false, false, 2.0, None),
                (5.0, 5.0),
            ),
            (
                "NaN minimum",
                |ts| ts.apply_options(6.0, f32::NAN, 0.0, false, false, 2.0, None),
                (6.0, 0.5),
            ),
            (
                "minimum set above maximum",
                |ts| {
                    ts.set_max_bar_spacing(5.0);
                    ts.set_min_bar_spacing(10.0);
                },
                (5.0, 5.0),
            ),
            (
                "NaN spacing with a raised minimum",
                |ts| ts.apply_options(f32::NAN, 10.0, 0.0, false, false, 2.0, None),
                (10.0, 10.0),
            ),
            (
                "NaN spacing with a lowered maximum",
                |ts| ts.apply_options(f32::NAN, 0.5, 5.0, false, false, 2.0, None),
                (5.0, 0.5),
            ),
            (
                "NaN spacing with a lowered maximum and fixed left edge",
                |ts| {
                    ts.set_bar_spacing(6.0);
                    ts.apply_options(f32::NAN, 0.5, 4.0, true, false, 2.0, None);
                },
                (4.0, 0.5),
            ),
        ];
        for (name, setup, (set, zoomed)) in rows {
            let mut ts = TimeScale::new();
            ts.set_bar_cnt(100);
            setup(&mut ts);
            assert_eq!(ts.bar_spacing(), set, "{name}");
            for fix_left in [false, true] {
                ts.set_fix_left_edge(fix_left);
                for delta in [-10.0, -9.999_999, -100.0] {
                    for _ in 0..100 {
                        ts.zoom(delta, 799.0, 0.0, 800.0);
                        assert_eq!(ts.bar_spacing(), zoomed, "{name}");
                        let range = ts.visible_logical_range();
                        assert!(range.left.is_finite() && range.right.is_finite(), "{name}");
                        assert!(ts.right_offset().is_finite(), "{name}");
                        assert!(ts.right_offset_precise().is_finite(), "{name}");
                    }
                }
                // At the fixed left edge, (width - 1) / spacing - bar count is the offset.
                if fix_left {
                    ts.scroll_bars(f32::MAX);
                    let expected = 799.0 / f64::from(zoomed) - 100.0;
                    assert!(
                        (ts.right_offset_precise() - expected).abs() < 1e-6,
                        "{name}"
                    );
                }
                ts.zoom(5.0, 400.0, 0.0, 800.0);
                if set > zoomed {
                    assert!(ts.bar_spacing() > zoomed, "{name}");
                }
                ts.set_bar_spacing(set);
                ts.set_fix_left_edge(false);
                ts.set_right_offset(0.0);
                ts.scroll_pixels(10.0);
                let expected = (-10.0 / f64::from(set)).max(-200.0);
                assert!(
                    (ts.right_offset_precise() - expected).abs() < 1e-6,
                    "{name}"
                );
            }
        }
    }
}
