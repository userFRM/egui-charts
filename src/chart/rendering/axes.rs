//! Price and time axis rendering

use crate::drawings::rendering::utils::draw_dashed_line;
use crate::scales::PriceMap;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Renders the last price line and label
pub fn render_last_price_line(
    painter: &Painter,
    price_rect: Rect,
    price_map: &PriceMap,
    last_close: f64,
    last_open: f64,
    bullish_color: Color32,
    bearish_color: Color32,
    show_right_axis: bool,
) {
    let y = price_map.price_to_y(last_close);
    // The newest bar can be scrolled or zoomed off the scale.
    if !(price_rect.min.y..=price_rect.max.y).contains(&y) {
        return;
    }

    let price_color = if last_close >= last_open {
        bullish_color
    } else {
        bearish_color
    };

    // Draw dashed horizontal line at last price
    draw_dashed_line(
        painter,
        Pos2::new(price_rect.min.x, y),
        Pos2::new(price_rect.max.x, y),
        Stroke::new(DESIGN_TOKENS.stroke.hairline, price_color),
        4.0,
        3.0,
    );

    // Draw price label on the right with colored background
    if show_right_axis {
        let transform = price_map.transform();
        let label = transform.label(transform.to_scale(last_close), 2);
        let label_pos = Pos2::new(price_rect.max.x + 5.0, y);

        // Measure text size first
        let galley = painter.layout_no_wrap(
            label.clone(),
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.chart.crosshair_label_text,
        );

        // Draw colored background box
        let padding = Vec2::new(6.0, 3.0);
        let label_rect = Rect::from_min_size(
            Pos2::new(label_pos.x, label_pos.y - galley.size().y / 2.0 - padding.y),
            galley.size() + padding * 2.0,
        );
        painter.rect_filled(label_rect, 2.0, price_color);

        // Draw text
        painter.galley(
            Pos2::new(label_pos.x + padding.x, label_pos.y - galley.size().y / 2.0),
            galley,
            Color32::TRANSPARENT,
        );
    }
}
