//! Common rendering helpers
//!
//! Shared utilities for price-to-pixel conversion and drawing primitives.

use crate::model::Bar;
use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::RectShape;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use std::ops::Range;

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

/// Queue the wicks of a candle; a trending bar has no wick on one side, and a segment without
/// length paints nothing.
pub(super) fn push_wicks(shapes: &mut Vec<Shape>, x: f32, y: &OhlcYCoords, stroke: Stroke) {
    for (from, to) in [(y.wick_top, y.body_top), (y.body_bottom, y.wick_bottom)] {
        if from != to {
            shapes.push(Shape::line_segment(
                [Pos2::new(x, from), Pos2::new(x, to)],
                stroke,
            ));
        }
    }
}

/// Queue the fill and the inside border of a candle body, if any.
///
/// A border in the colour of an opaque fill changes no pixel and is skipped. The tessellator
/// stops a shared fill at the middle of the stroke, so only an opaque border may share the
/// fill's shape.
pub(super) fn push_body(
    shapes: &mut Vec<Shape>,
    rect: Rect,
    fill: Option<Color32>,
    border: Option<Stroke>,
) {
    let border = border.filter(|b| !(fill == Some(b.color) && b.color.is_opaque()));
    match (fill, border) {
        (Some(fill), Some(border)) if !border.color.is_opaque() => {
            shapes.push(Shape::rect_filled(rect, 0.0, fill));
            shapes.push(Shape::rect_stroke(rect, 0.0, border, StrokeKind::Inside));
        }
        (None, None) => {}
        (fill, border) => shapes.push(
            RectShape::new(
                rect,
                0.0,
                fill.unwrap_or(Color32::TRANSPARENT),
                border.unwrap_or(Stroke::NONE),
                StrokeKind::Inside,
            )
            .into(),
        ),
    }
}

/// The body of a candle, at least the design's minimum height.
pub fn body_rect(x: f32, bar_width: f32, y: &OhlcYCoords) -> Rect {
    Rect::from_min_max(
        Pos2::new(x - bar_width / 2.0, y.body_top),
        Pos2::new(
            x + bar_width / 2.0,
            y.body_bottom
                .max(y.body_top + DESIGN_TOKENS.sizing.candle.min_body_height),
        ),
    )
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

/// The ranges of the consecutive bars of `data` that have a value, between its whitespace bars.
pub fn valued_runs(data: &[Bar]) -> impl Iterator<Item = Range<usize>> + '_ {
    data.chunk_by(|a, b| a.is_whitespace() == b.is_whitespace())
        .scan(0, |start, run| {
            *start += run.len();
            Some(*start - run.len()..*start)
        })
        .filter(|run| !data[run.start].is_whitespace())
}

/// Close of the last bar with a value before `idx` in `full`; a bar without one before it
/// compares with its own open.
pub fn prev_close(full: &[Bar], idx: usize) -> f64 {
    full[..idx]
        .iter()
        .rev()
        .find(|bar| !bar.is_whitespace())
        .map_or(full[idx].open, |bar| bar.close)
}
