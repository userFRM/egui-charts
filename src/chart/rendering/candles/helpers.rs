//! Common rendering helpers
//!
//! Shared utilities for price-to-pixel conversion and drawing primitives.

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

/// Pre-computed pixel Y coordinates for a single OHLC bar.
///
/// Converts a bar's open/high/low/close prices into screen-space Y values
/// and orders the wick and body ends on screen (the high is the lower end on
/// an inverted scale).
pub struct OhlcYCoords {
    pub wick_top: f32,
    pub wick_bottom: f32,
    pub body_top: f32,
    pub body_bottom: f32,
}

impl OhlcYCoords {
    pub fn from_bar(bar: &crate::model::Bar, coords: &PriceMap) -> Self {
        let y_open = coords.price_to_y(bar.open);
        let y_high = coords.price_to_y(bar.high);
        let y_low = coords.price_to_y(bar.low);
        let y_close = coords.price_to_y(bar.close);
        Self {
            wick_top: y_high.min(y_low),
            wick_bottom: y_high.max(y_low),
            body_top: y_open.min(y_close),
            body_bottom: y_open.max(y_close),
        }
    }
}

/// Draw candle wicks (upper and lower)
pub fn draw_wicks(painter: &Painter, x: f32, y: &OhlcYCoords, wick_width: f32, color: Color32) {
    // Upper wick
    painter.add(Shape::line_segment(
        [Pos2::new(x, y.wick_top), Pos2::new(x, y.body_top)],
        Stroke::new(wick_width, color),
    ));
    // Lower wick
    painter.add(Shape::line_segment(
        [Pos2::new(x, y.body_bottom), Pos2::new(x, y.wick_bottom)],
        Stroke::new(wick_width, color),
    ));
}

/// Draw filled candle body
pub fn draw_body_filled(
    painter: &Painter,
    x: f32,
    bar_width: f32,
    y: &OhlcYCoords,
    color: Color32,
) {
    let body_rect = Rect::from_min_max(
        Pos2::new(x - bar_width / 2.0, y.body_top),
        Pos2::new(
            x + bar_width / 2.0,
            y.body_bottom
                .max(y.body_top + DESIGN_TOKENS.sizing.candle.min_body_height),
        ),
    );
    painter.add(Shape::rect_filled(body_rect, 0.0, color));
}

/// Draw hollow candle body (stroke only)
pub fn draw_body_hollow(
    painter: &Painter,
    x: f32,
    bar_width: f32,
    y: &OhlcYCoords,
    color: Color32,
) {
    let body_rect = Rect::from_min_max(
        Pos2::new(x - bar_width / 2.0, y.body_top),
        Pos2::new(x + bar_width / 2.0, y.body_bottom),
    );
    painter.add(Shape::rect_stroke(
        body_rect,
        0.0,
        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
        egui::epaint::StrokeKind::Inside,
    ));
}

/// Draw a brick/block with border (for Renko, Line Break)
pub fn draw_brick(painter: &Painter, rect: Rect, color: Color32, corner_radius: f32) {
    painter.add(Shape::rect_filled(rect, corner_radius, color));
    let border_color = color.gamma_multiply(0.7);
    painter.add(Shape::rect_stroke(
        rect,
        corner_radius,
        Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
        egui::epaint::StrokeKind::Inside,
    ));
}

/// Draw an X symbol (for Point & Figure up columns)
pub fn draw_x_symbol(painter: &Painter, center: Pos2, half_size: f32, color: Color32) {
    painter.add(Shape::line_segment(
        [
            Pos2::new(center.x - half_size, center.y - half_size),
            Pos2::new(center.x + half_size, center.y + half_size),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.thick, color),
    ));
    painter.add(Shape::line_segment(
        [
            Pos2::new(center.x - half_size, center.y + half_size),
            Pos2::new(center.x + half_size, center.y - half_size),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.thick, color),
    ));
}

/// Draw an O symbol (for Point & Figure down columns)
pub fn draw_o_symbol(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    painter.add(Shape::circle_stroke(
        center,
        radius,
        Stroke::new(DESIGN_TOKENS.stroke.thick, color),
    ));
}

/// Select bullish or bearish color based on bar direction
#[inline]
pub fn bar_color(bar: &crate::model::Bar, bullish: Color32, bearish: Color32) -> Color32 {
    if bar.close > bar.open {
        bullish
    } else {
        bearish
    }
}
