use crate::scales::{PriceMarkGenerator, PriceScaleMode};
use egui::{Color32, Pos2, Rect, Stroke};

/// Renders price grid lines using smart mark generation
/// Grid lines anchor to "nice" price values and adapt density based on zoom level
pub fn render_price_grid(
    painter: &egui::Painter,
    rect: Rect,
    min_price: f64,
    max_price: f64,
    grid_color: Color32,
) {
    let generator = PriceMarkGenerator::new();
    let marks = generator.generate_marks(
        min_price,
        max_price,
        rect.height(),
        PriceScaleMode::Normal,
        rect.min.y,
        rect.max.y,
    );

    for mark in marks {
        // Use weight for line styling - thicker lines for major price levels
        let stroke_width = if mark.weight >= 80 { 0.7 } else { 0.5 };

        painter.line_segment(
            [
                Pos2::new(rect.min.x, mark.y_coord),
                Pos2::new(rect.max.x, mark.y_coord),
            ],
            Stroke::new(stroke_width, grid_color),
        );
    }
}
