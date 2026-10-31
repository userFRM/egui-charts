//! Price label and price note rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use crate::scales::{
    DefaultPriceFormatter, PriceFormatter, instrument_price_precision, price_decimals,
};
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{
    Color32, Pos2, Rect, Stroke,
    epaint::{PathShape, RoundedRect},
};
use std::f32::consts::{PI, TAU};

/// Midpoint of the side of `rect` that faces `target`: left or right when the target lies more
/// to the side than above or below, measured against the box's own proportions.
pub(super) fn facing_point(rect: Rect, target: Pos2) -> Pos2 {
    let d = target - rect.center();
    if d.x.abs() * rect.height() > d.y.abs() * rect.width() {
        if d.x > 0.0 {
            rect.right_center()
        } else {
            rect.left_center()
        }
    } else if d.y >= 0.0 {
        rect.center_bottom()
    } else {
        rect.center_top()
    }
}

/// White or black, whichever reads on `fill`, by the rule the price axis tags use. The chart
/// behind a translucent fill is not known here, so the colour the user picked decides.
pub(super) fn label_text_color(fill: Color32) -> Color32 {
    if crate::theme::contrast(Color32::WHITE, fill.to_opaque()) >= 2.0 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

/// Outline of `rect`, rounded by `radius`, with a pointer from `from` through `tip` to `to`, as one
/// clockwise path. `from` and `to` lie on the straight part of the outline. The path starts at the
/// pointer, so the fan fill from the first point covers the concave shape exactly, for a tip at
/// least 4 px outside `rect`.
pub(super) fn pointed_outline(
    rect: Rect,
    radius: f32,
    from: Pos2,
    tip: Pos2,
    to: Pos2,
) -> Vec<Pos2> {
    // Screen angles grow clockwise.
    let angle = |p: Pos2| (p - rect.center()).angle();
    let turn = |from: f32, to: f32| (to - from).rem_euclid(TAU);
    let (start, end) = if turn(angle(from), angle(to)) > PI {
        (to, from)
    } else {
        (from, to)
    };
    let span = turn(angle(start), angle(end));
    let mut ring = Vec::new();
    egui::epaint::tessellator::path::rounded_rectangle(&mut ring, RoundedRect::new(rect, radius));
    let mut kept: Vec<_> = ring
        .into_iter()
        .map(|p| (turn(angle(start), angle(p)), p))
        .filter(|&(d, _)| span + 1e-4 < d && d < TAU - 1e-4)
        .collect();
    kept.sort_by(|a, b| a.0.total_cmp(&b.0));
    // A stroke's join at a sharp point runs past it, far past on a long pointer, so the point is
    // blunt by a pixel.
    let across = (tip - start.lerp(end, 0.5)).normalized().rot90();
    let blunt = across * (start - end).dot(across).signum() * 0.5;
    [start, tip + blunt, tip, tip - blunt, end]
        .into_iter()
        .chain(kept.into_iter().map(|(_, p)| p))
        .collect()
}

/// Joins a label to the anchor it annotates: a thin line from the side facing the anchor, ending
/// in a small dot. An anchor under the label needs no line.
pub(super) fn paint_anchor_line(
    painter: &egui::Painter,
    label: Rect,
    anchor: Pos2,
    color: Color32,
) {
    if !label.contains(anchor) {
        painter.line_segment(
            [facing_point(label, anchor), anchor],
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.circle_filled(anchor, 3.0, color);
    }
}

impl Drawing {
    /// The first anchor's price in the chart's price format, else in the instrument's precision,
    /// else by its magnitude; `None` when the price is not finite.
    fn anchor_price_text(&self, view: &ChartView) -> Option<String> {
        let price = match self.chart_points.first() {
            Some(point) => point.price,
            None => (view.y_to_price)(self.points.first()?.y),
        };
        if !price.is_finite() {
            return None;
        }
        if let Some(formatter) = view.price_formatter {
            return Some(formatter.format(price));
        }
        let precision = view
            .instrument
            .and_then(instrument_price_precision)
            .map_or_else(|| price_decimals(price), usize::from);
        Some(DefaultPriceFormatter::with_precision(precision).format(price))
    }

    /// Renders a price label: the anchor's price in a badge up and to the right of the point, with
    /// a pointer from the badge's corner to the point.
    pub(crate) fn render_price_label(&self, painter: &egui::Painter, view: &ChartView) {
        let (Some(&point), Some(text)) = (self.points.first(), self.anchor_price_text(view)) else {
            return;
        };
        let fill = self.fill_color32().unwrap_or(self.color32());
        let text_color = label_text_color(fill);
        let galley = painter.layout_no_wrap(text, self.annotation_font(), text_color);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );
        let offset = DESIGN_TOKENS.sizing.annotation.pointer_size;
        let size = galley.size() + padding * 2.0;
        let badge =
            Rect::from_min_size(Pos2::new(point.x + offset, point.y - offset - size.y), size);
        // The pointer replaces the corner, with its base beyond the corner's arc on both edges.
        painter.add(PathShape {
            points: pointed_outline(
                badge,
                DESIGN_TOKENS.rounding.sm,
                Pos2::new(badge.left() + offset, badge.bottom()),
                point,
                Pos2::new(badge.left(), badge.bottom() - offset),
            ),
            closed: true,
            fill,
            stroke: Stroke::NONE.into(),
        });
        painter.galley(badge.min + padding, galley, text_color);
    }

    /// Renders a price note: the anchor's price and any typed text in a solid label at the second
    /// anchor, joined to the anchor by a thin line.
    pub(crate) fn render_price_note(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let mut text = self.anchor_price_text(view).unwrap_or_default();
        if let Some(typed) = self.text.as_deref().filter(|typed| !typed.is_empty()) {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(typed);
        }
        if text.is_empty() {
            return;
        }
        let color = self.color32();
        let fill = self.fill_color32().unwrap_or(color);
        let text_color = label_text_color(fill);
        let galley = painter.layout_no_wrap(text, self.annotation_font(), text_color);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );
        let corner = self.points.get(1).copied().unwrap_or(anchor);
        let label = Rect::from_min_size(corner, galley.size() + padding * 2.0);
        paint_anchor_line(painter, label, anchor, color);
        painter.rect_filled(label, DESIGN_TOKENS.rounding.sm, fill);
        painter.galley(label.min + padding, galley, text_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType};
    use crate::model::Instrument;
    use crate::scales::CustomPriceFormatter;

    /// The box and pointer fill as one shape: a path that started on the ring or ran the wrong way
    /// would leave the pointer or a sliver unfilled, which only the triangles show.
    #[test]
    fn pointed_outlines_fill_exactly() {
        let wide = Rect::from_min_max(Pos2::ZERO, Pos2::new(120.0, 40.0));
        let badge = Rect::from_min_max(Pos2::new(8.0, -29.0), Pos2::new(68.0, -8.0));
        let at = Pos2::new;
        for (rect, radius, from, tip, to) in [
            (wide, 8.0, at(0.0, 12.0), at(-60.0, 20.0), at(0.0, 28.0)),
            (wide, 8.0, at(120.0, 12.0), at(200.0, 20.0), at(120.0, 28.0)),
            (wide, 8.0, at(52.0, 0.0), at(60.0, -80.0), at(68.0, 0.0)),
            (wide, 8.0, at(52.0, 40.0), at(60.0, 140.0), at(68.0, 40.0)),
            (wide, 8.0, at(52.0, 40.0), at(130.0, 45.0), at(68.0, 40.0)),
            (badge, 4.0, at(16.0, -8.0), at(0.0, 0.0), at(8.0, -16.0)),
        ] {
            let points = pointed_outline(rect, radius, from, tip, to);
            assert!(points.contains(&tip));
            let mut tessellator = egui::epaint::Tessellator::new(
                1.0,
                egui::epaint::TessellationOptions {
                    feathering: false,
                    ..Default::default()
                },
                [1, 1],
                vec![],
            );
            let path = PathShape {
                points: points.clone(),
                closed: true,
                fill: Color32::WHITE,
                stroke: Stroke::NONE.into(),
            };
            let mut mesh = egui::Mesh::default();
            tessellator.tessellate_path(&path, &mut mesh);
            let mut filled = 0.0;
            for triangle in mesh.indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize].pos);
                // Screen-clockwise is positive.
                let area = (b - a).x * (c - a).y - (b - a).y * (c - a).x;
                assert!(area / 2.0 >= -1e-3, "{tip:?}: {a:?} {b:?} {c:?}");
                filled += area / 2.0;
            }
            let shoelace: f32 = (0..points.len())
                .map(|i| {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    (a.x * b.y - b.x * a.y) / 2.0
                })
                .sum();
            assert!(
                shoelace > 0.0 && (filled - shoelace).abs() < 1e-2,
                "{tip:?}: {filled} {shoelace}"
            );
            // A 1 px outline stays on the shape: its join does not run past the tip.
            let mut outlined = egui::Mesh::default();
            tessellator.tessellate_path(
                &PathShape {
                    stroke: Stroke::new(1.0, Color32::WHITE).into(),
                    ..path
                },
                &mut outlined,
            );
            let reach = Rect::from_points(&points).expand(1.0);
            assert!(
                outlined.vertices.iter().all(|v| reach.contains(v.pos)),
                "{tip:?}: {:?}",
                outlined
                    .vertices
                    .iter()
                    .map(|v| v.pos)
                    .find(|&p| !reach.contains(p))
            );
        }
    }

    /// Expected texts follow the published rules: the tick's decimals, explicit decimals over the
    /// tick, and by magnitude without an instrument (2 decimals from 100, 4 from 1).
    #[test]
    fn anchor_prices_follow_the_instrument() {
        let host = CustomPriceFormatter::new(|price| format!("host:{price:.1}"));
        let host = &host as &dyn PriceFormatter;
        for (tick_size, decimals, formatter, price, expected) in [
            (Some(0.00001), None, None, 1.234567, Some("1.23457")),
            (Some(1.0), None, None, 4321.6, Some("4322")),
            (Some(0.25), Some(3), None, 99.5, Some("99.500")),
            (None, None, None, 4321.6, Some("4321.60")),
            (None, None, None, 1.234567, Some("1.2346")),
            (None, None, None, -0.0, Some("0.000000")),
            (Some(1.0), None, Some(host), 4321.6, Some("host:4321.6")),
            (None, None, None, f64::NAN, None),
            (None, None, None, f64::INFINITY, None),
        ] {
            let mut instrument = Instrument::new("X");
            instrument.tick_size = tick_size;
            instrument.price_decimals = decimals;
            let view = ChartView {
                price_formatter: formatter,
                instrument: (tick_size.is_some() || decimals.is_some()).then_some(&instrument),
                bars: &[],
                revision: 0,
                bar_to_x: &|bar| bar,
                price_to_y: &|price| price as f32,
                y_to_price: &|y| f64::from(y),
                time_axis: true,
                executions: None,
            };
            let mut drawing = Drawing::new(7, DrawingToolType::PriceLabel);
            drawing.chart_points.push(ChartPoint::new(0.0, price));
            assert_eq!(
                drawing.anchor_price_text(&view).as_deref(),
                expected,
                "{price}"
            );
            // Standalone renders hold screen points only.
            drawing.chart_points.clear();
            drawing.points.push(Pos2::new(0.0, price as f32));
            assert_eq!(
                drawing.anchor_price_text(&view).as_deref(),
                expected,
                "{price}"
            );
        }
    }
}
