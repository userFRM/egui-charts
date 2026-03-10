//! Grid rendering logic
//! Grid lines are fixed to bar indices and price levels, moving 1:1 with the chart

use crate::chart::renderers::ChartMapping;
use crate::scales::{PriceMarkGenerator, PriceScaleMode};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// Renders horizontal price grid lines using smart mark generation
/// Lines anchor to "nice" price values and adapt density based on zoom
pub fn render_grid(painter: &Painter, rect: Rect, min: f64, max: f64, grid_color: Color32) {
    let generator = PriceMarkGenerator::new();
    let marks = generator.generate_marks(
        min,
        max,
        rect.height(),
        PriceScaleMode::Normal,
        rect.min.y,
        rect.max.y,
    );

    for mark in marks {
        let stroke_width = if mark.weight >= 80 {
            DESIGN_TOKENS.stroke.light
        } else {
            DESIGN_TOKENS.stroke.extra_thin
        };
        painter.line_segment(
            [
                Pos2::new(rect.min.x, mark.y_coord),
                Pos2::new(rect.max.x, mark.y_coord),
            ],
            Stroke::new(stroke_width, grid_color),
        );
    }
}

/// Renders vertical grid lines at fixed bar intervals
/// Grid lines are tied directly to bar indices, so they move 1:1 with the chart during pan
pub fn render_vertical_grid(
    painter: &Painter,
    rect: Rect,
    coords: &ChartMapping,
    grid_color: Color32,
) {
    // Calculate grid interval based on bar spacing (zoom level)
    // More zoomed in (larger bar_spacing) = more grid lines
    // More zoomed out (smaller bar_spacing) = fewer grid lines
    let min_pixel_spacing = 80.0; // Min pixels between grid lines
    let bars_per_grid = (min_pixel_spacing / coords.bar_spacing).ceil().max(1.0) as usize;

    // Round to nice intervals: 1, 2, 5, 10, 20, 50, 100, etc.
    let bars_per_grid = nice_interval(bars_per_grid);

    // Calculate visible bar range
    let chart_width = rect.width();
    let visible_bars = (chart_width / coords.bar_spacing).ceil() as usize + 2;

    // Find the first grid line that's visible
    // Grid lines are at indices 0, bars_per_grid, 2*bars_per_grid, etc.
    let first_visible_idx = if coords.start_idx > 0 {
        (coords.start_idx / bars_per_grid) * bars_per_grid
    } else {
        0
    };

    // Draw grid lines
    for i in 0..=(visible_bars / bars_per_grid + 2) {
        let grid_idx = first_visible_idx + i * bars_per_grid;

        // Convert bar index to x coord using same formula as candle rendering
        let x = coords.idx_to_x(grid_idx);

        // Skip if outside visible area
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
            continue;
        }

        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
        );
    }
}

/// Round to a nice interval (1, 2, 5, 10, 20, 50, 100, etc.)
fn nice_interval(raw: usize) -> usize {
    if raw <= 1 {
        return 1;
    }
    if raw <= 2 {
        return 2;
    }
    if raw <= 5 {
        return 5;
    }
    if raw <= 10 {
        return 10;
    }
    if raw <= 20 {
        return 20;
    }
    if raw <= 50 {
        return 50;
    }
    if raw <= 100 {
        return 100;
    }
    if raw <= 200 {
        return 200;
    }
    if raw <= 500 {
        return 500;
    }
    1000
}
