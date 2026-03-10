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

/// Render selection dots on candle data points
///
/// For candlestick charts, places dots at the close price of each candle
/// at regular intervals.
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
    for i in visible_range {
        if i % config.dot_interval != 0 {
            continue;
        }

        if let Some(&close) = closes.get(i) {
            let x = coords.idx_to_x(i);
            let y = price_to_y(close);

            // Only draw if within chart bounds
            if coords.rect.contains(Pos2::new(x, y)) {
                draw_selection_dot(painter, Pos2::new(x, y), config);
            }
        }
    }
}
