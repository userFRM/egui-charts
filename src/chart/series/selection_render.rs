//! Series selection rendering - visual feedback when series is selected.
//!
//! Selection style: small circles on the data points at regular intervals.

use crate::chart::coords::ChartMapping;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2};

// Selection dot sizing constants (matching chart-rendering.md Handles spec)
/// Outer radius of selection dots (donut outer circle)
pub const SELECTION_DOT_OUTER_RADIUS: f32 = 4.5;
/// Inner radius of selection dots (donut hole)
pub const SELECTION_DOT_INNER_RADIUS: f32 = 2.5;
/// Target pixel gap between selection dots (adjusts with zoom)
pub const SELECTION_DOT_TARGET_GAP: f32 = 70.0;
/// Minimum interval between dots (bars)
pub const SELECTION_DOT_MIN_INTERVAL: usize = 3;
/// Maximum interval between dots (bars)
pub const SELECTION_DOT_MAX_INTERVAL: usize = 50;
/// Default interval between dots (bars)
pub const SELECTION_DOT_DEFAULT_INTERVAL: usize = 5;

/// Configuration for the donut-style selection dots rendered on a selected series.
///
/// Selection dots appear at regular intervals along the series data to visually
/// indicate which series is selected. They use a "donut" style: a filled outer
/// ring with an inner circle in the chart background color.
#[derive(Clone, Debug)]
pub struct SelectionHandleConfig {
    /// Outer radius of selection dots
    pub outer_radius: f32,
    /// Inner radius (for donut effect)
    pub inner_radius: f32,
    /// Color of the outer ring
    pub ring_color: Color32,
    /// Color of the inner circle (chart background)
    pub inner_color: Color32,
    /// Interval between dots (every N data points)
    pub dot_interval: usize,
}

impl Default for SelectionHandleConfig {
    fn default() -> Self {
        Self {
            outer_radius: SELECTION_DOT_OUTER_RADIUS,
            inner_radius: SELECTION_DOT_INNER_RADIUS,
            ring_color: DESIGN_TOKENS.semantic.extended.accent,
            inner_color: DESIGN_TOKENS.semantic.chart.bg,
            dot_interval: SELECTION_DOT_DEFAULT_INTERVAL,
        }
    }
}

/// Calculate dynamic dot interval based on bar spacing (zoom level).
///
/// Ensures dots are spaced roughly 60-80 pixels apart to reduce visual noise
/// when zoomed out, while showing more dots when zoomed in.
pub fn calculate_dot_interval(bar_spacing: f32) -> usize {
    let interval = (SELECTION_DOT_TARGET_GAP / bar_spacing).round() as usize;
    interval.clamp(SELECTION_DOT_MIN_INTERVAL, SELECTION_DOT_MAX_INTERVAL)
}

/// Draw a single selection dot (donut style - filled ring with inner circle)
fn draw_selection_dot(painter: &Painter, pos: Pos2, config: &SelectionHandleConfig) {
    // Outer filled circle
    painter.circle_filled(pos, config.outer_radius, config.ring_color);
    // Inner circle (chart background color) to create donut effect
    painter.circle_filled(pos, config.inner_radius, config.inner_color);
}

/// Render selection dots on data points
///
/// Draws small donut-style circles directly on the plotted data at regular intervals
/// to indicate the series is selected.
pub fn render_series_selection_on_points(
    painter: &Painter,
    points: &[Pos2],
    config: &SelectionHandleConfig,
) {
    if points.is_empty() {
        return;
    }

    // Draw donut dots at regular intervals
    for (i, &point) in points.iter().enumerate() {
        if i % config.dot_interval == 0 {
            draw_selection_dot(painter, point, config);
        }
    }
}

/// Resolve the close price for an absolute bar index against a visible-slice
/// array.
///
/// `visible_range` is expressed in absolute (data-space) bar indices because
/// [`ChartMapping::idx_to_x`] positions bars by their absolute index. The
/// `closes` slice, however, holds only the visible bars and is therefore
/// indexed locally: the close for absolute bar `abs_idx` lives at
/// `closes[abs_idx - start_idx]`. Conflating the two index spaces drops every
/// handle once the chart is scrolled past the first bar (a non-zero
/// `start_idx`), which is why the local offset is applied explicitly here.
///
/// Returns `None` when `abs_idx` precedes the visible window or falls beyond the
/// available data.
#[inline]
fn close_for_abs_idx(closes: &[f64], start_idx: usize, abs_idx: usize) -> Option<f64> {
    let local = abs_idx.checked_sub(start_idx)?;
    closes.get(local).copied()
}

/// Render selection dots on candle data points.
///
/// For candlestick charts, places dots at the close price of each candle at
/// regular intervals.
pub fn render_candle_selection_dots<F>(
    painter: &Painter,
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    closes: &[f64],
    price_to_y: F,
    config: &SelectionHandleConfig,
) where
    F: Fn(f64) -> f32,
{
    let start_idx = visible_range.start;
    for i in visible_range {
        if i % config.dot_interval != 0 {
            continue;
        }

        let Some(close) = close_for_abs_idx(closes, start_idx, i) else {
            continue;
        };

        let x = coords.idx_to_x(i);
        let y = price_to_y(close);

        // Only draw if within chart bounds
        if coords.rect.contains(Pos2::new(x, y)) {
            draw_selection_dot(painter, Pos2::new(x, y), config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_lookup_resolves_for_nonzero_start_index() {
        // Mirrors the demo: ~100 visible bars drawn from a larger series, so the
        // visible window starts well past index 0. The `closes` slice is the
        // local visible slice; absolute index `i` must map to `closes[i -
        // start]`, not `closes[i]`.
        let start_idx = 400;
        let closes: Vec<f64> = (0..100).map(|local| 10.0 + local as f64).collect();
        let visible_range = start_idx..start_idx + closes.len();

        // Every absolute index in the visible window must resolve.
        for abs_idx in visible_range.clone() {
            let resolved = close_for_abs_idx(&closes, start_idx, abs_idx)
                .expect("close must resolve for an in-window absolute index");
            assert_eq!(resolved, closes[abs_idx - start_idx]);
        }

        // The first and last handle positions are the regression cases: with the
        // old `closes.get(i)` lookup these returned `None` and nothing drew.
        assert_eq!(close_for_abs_idx(&closes, start_idx, start_idx), Some(10.0));
        assert_eq!(
            close_for_abs_idx(&closes, start_idx, visible_range.end - 1),
            Some(10.0 + 99.0)
        );
    }

    #[test]
    fn close_lookup_rejects_out_of_window_indices() {
        let start_idx = 400;
        let closes: Vec<f64> = vec![1.0, 2.0, 3.0];

        // Before the window: would underflow if subtracted naively.
        assert_eq!(close_for_abs_idx(&closes, start_idx, start_idx - 1), None);
        // Past the available data.
        assert_eq!(close_for_abs_idx(&closes, start_idx, start_idx + 3), None);
    }
}
