//! Shape and curve tool rendering implementations
//!
//! Includes: rotated rect, double curve, brush, highlighter, path, curve, and sine line.

use super::style::{Acceleration, ellipse_arc, smooth_path};
use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke};

impl Drawing {
    /// The first anchor is the center, the second fixes the radius, and the third fixes
    /// the end ray. The sweep takes the short arc in (-180°, 180°], clockwise at 180°.
    pub(super) fn sector_points(&self, clip: Rect, ppp: f32) -> Option<Vec<Pos2>> {
        let [center, start, end, ..] = *self.points else {
            return None;
        };
        let radius = center.distance(start);
        if radius < 1e-6 || center.distance_sq(end) < 1e-12 {
            return None;
        }
        let from = (start - center).angle();
        let mut sweep = (end - center).angle() - from;
        if sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        } else if sweep <= -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        let mut points = vec![center];
        points.extend(ellipse_arc(
            center,
            egui::Vec2::splat(radius),
            [f64::from(from), f64::from(from) + f64::from(sweep)],
            clip,
            ppp,
        ));
        Some(points)
    }

    pub(super) fn render_sector(&self, painter: &egui::Painter) {
        let Some(points) = self.sector_points(
            painter.clip_rect().expand(self.stroke_width + 2.0),
            painter.pixels_per_point(),
        ) else {
            return;
        };
        if let Some(fill) = self.fill_color32() {
            // One mesh keeps translucent fill continuous across the fan's internal edges.
            let mut mesh = egui::Mesh::default();
            for &point in &points {
                mesh.colored_vertex(point, fill);
            }
            for i in 1..points.len() - 1 {
                mesh.add_triangle(0, i as u32, (i + 1) as u32);
            }
            painter.add(Shape::mesh(mesh));
        }
        painter.add(Shape::closed_line(points, self.stroke()));
    }

    /// A rectangle whose first two anchors form its base and whose third sets the width.
    pub(crate) fn render_rotated_rect(&self, painter: &egui::Painter) {
        let Some(corners) = self.rotated_rect_corners() else {
            return;
        };
        let fill = self.fill_color.map_or(Color32::TRANSPARENT, |c| {
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
        });
        painter.add(egui::Shape::convex_polygon(
            corners.to_vec(),
            fill,
            Stroke::new(self.stroke_width, self.color32()),
        ));
    }

    pub(super) fn rotated_rect_corners(&self) -> Option<[Pos2; 4]> {
        let &[a, b, c, ..] = self.points.as_slice() else {
            return None;
        };
        let base = b - a;
        if base.length_sq() < 1e-6 {
            return None;
        }
        let normal = egui::vec2(-base.y, base.x);
        let offset = normal * ((c - a).dot(normal) / base.length_sq());
        Some([a, b, b + offset, a + offset])
    }

    /// Cubic Bezier curve with two independently editable control points.
    pub(crate) fn render_double_curve(&self, painter: &egui::Painter) {
        let Some(points) = self.double_curve_points(
            painter.clip_rect().expand(self.stroke_width + 2.0),
            painter.pixels_per_point(),
        ) else {
            return;
        };
        self.paint_enclosed_fill(painter);
        painter.add(egui::Shape::line(
            points,
            Stroke::new(self.stroke_width, self.color32()),
        ));
    }

    /// The double curve's start, controls and end: its first four anchors, or else, from its
    /// first and last anchors, controls at the chord's thirds set off half the chord to either
    /// side, an S bend. `None` below two anchors.
    pub(super) fn double_curve_controls(&self) -> Option<[Pos2; 4]> {
        if let &[a, b, c, d, ..] = self.points.as_slice() {
            return Some([a, b, c, d]);
        }
        let (&a, &d) = (self.points.first()?, self.points.last()?);
        if self.points.len() < 2 {
            return None;
        }
        let chord = d - a;
        let side = egui::vec2(-chord.y, chord.x) * 0.5;
        Some([a, a + chord / 3.0 + side, a + chord * (2.0 / 3.0) - side, d])
    }

    pub(super) fn double_curve_points(&self, clip: Rect, ppp: f32) -> Option<Vec<Pos2>> {
        Some(bezier_points(self.double_curve_controls()?, clip, ppp))
    }

    /// Brush and highlighter strokes: one path through the samples at the drawing's width and
    /// colour, so a translucent stroke does not darken where its segments meet.
    pub(crate) fn render_brush(&self, painter: &egui::Painter) {
        if self.points.len() >= 2 {
            painter.add(Shape::line(
                self.brush_points(
                    painter.clip_rect().expand(self.stroke_width + 2.0),
                    painter.pixels_per_point(),
                ),
                self.stroke(),
            ));
        }
    }

    // Note: render_sine_line is now in cycles.rs as it's a cyclic analysis tool

    /// Render Path - Multi-point connected freehand path
    pub(crate) fn render_path(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        // Draw connected line segments through all points
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment([self.points[i - 1], point], stroke);
            }
            // Keep the endpoint clear for its arrowhead.
            if !self.completed && (i + 1 < self.points.len() || !self.arrow_end_style.is_visible())
            {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
            }
        }

        // Larger circle at start point during placement
        if !self.completed {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render Curve - Bezier quadratic curve
    pub(crate) fn render_curve(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }
        let Some([p0, p1, p2]) = self.curve_controls() else {
            return;
        };
        self.paint_enclosed_fill(painter);

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        painter.add(Shape::line(
            curve_points(
                p0,
                p1,
                p2,
                painter.clip_rect().expand(stroke.width + 2.0),
                painter.pixels_per_point(),
            ),
            stroke,
        ));

        if !self.completed {
            painter.circle_filled(p0, DESIGN_TOKENS.rounding.md, color);
            painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
            // Draw control point (smaller, semi-transparent)
            painter.circle_filled(
                p1,
                DESIGN_TOKENS.rounding.sm,
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 128),
            );
        }
    }

    /// The curve's start, control and end points: the control point is the
    /// second anchor of three, or else set off to the side of the midpoint.
    /// `None` below two anchors.
    pub(super) fn curve_controls(&self) -> Option<[Pos2; 3]> {
        let (&p0, &p2) = (self.points.first()?, self.points.last()?);
        if self.points.len() < 2 {
            return None;
        }
        let p1 = if self.points.len() >= 3 {
            self.points[1]
        } else {
            let mid = Pos2::new((p0.x + p2.x) / 2.0, (p0.y + p2.y) / 2.0);
            Pos2::new(mid.x - (p2.y - p0.y) * 0.3, mid.y + (p2.x - p0.x) * 0.3)
        };
        Some([p0, p1, p2])
    }

    /// Stores the bend a curve or double curve placed by its two ends is drawn with as its
    /// control anchors, between the ends, so its handles reshape it. Other tools, and curves
    /// that already hold their controls, are left as they are.
    pub(crate) fn store_default_curve_controls(
        &mut self,
        x_to_bar: impl Fn(f32) -> f32,
        y_to_price: impl Fn(f32) -> f64,
    ) {
        let controls = match (self.tool_type, self.points.len()) {
            (DrawingToolType::Curve, 2) => self.curve_controls().map(|[_, c, _]| vec![c]),
            (DrawingToolType::DoubleCurve, 2) => {
                self.double_curve_controls().map(|[_, b, c, _]| vec![b, c])
            }
            _ => None,
        };
        for (i, control) in controls.into_iter().flatten().enumerate() {
            self.points.insert(1 + i, control);
            self.chart_points.insert(
                1 + i,
                ChartPoint::new(x_to_bar(control.x), y_to_price(control.y)),
            );
        }
    }

    /// The polygon whose even-odd inside the fill colour fills: a polyline's anchors, and an arc
    /// or curve closed by the chord between its ends. `None` for other tools.
    pub(super) fn enclosed_area(&self, clip: Rect, ppp: f32) -> Option<Vec<Pos2>> {
        match self.tool_type {
            DrawingToolType::Polyline => Some(self.points.clone()),
            DrawingToolType::Arc => Some(self.arc_path(clip, ppp)),
            DrawingToolType::Curve => self
                .curve_controls()
                .map(|[a, b, c]| curve_points(a, b, c, clip, ppp)),
            DrawingToolType::DoubleCurve => self.double_curve_points(clip, ppp),
            _ => None,
        }
    }

    /// Fills the even-odd inside of [`Self::enclosed_area`] with the fill colour. Call it before
    /// painting the line, which then lies over the fill.
    pub(super) fn paint_enclosed_fill(&self, painter: &egui::Painter) {
        let Some(fill) = self.fill_color32() else {
            return;
        };
        let Some(area) = self.enclosed_area(
            painter.clip_rect().expand(self.stroke_width + 2.0),
            painter.pixels_per_point(),
        ) else {
            return;
        };
        let mut mesh = egui::Mesh::default();
        for corners in even_odd_trapezoids(&area) {
            let first = mesh.vertices.len() as u32;
            for corner in corners {
                mesh.colored_vertex(corner, fill);
            }
            mesh.add_triangle(first, first + 1, first + 2);
            mesh.add_triangle(first, first + 2, first + 3);
        }
        if !mesh.is_empty() {
            painter.add(Shape::mesh(mesh));
        }
    }
}

/// The quadratic Bezier curve from `a` to `c` pulled towards `b`.
pub(super) fn curve_points(a: Pos2, b: Pos2, c: Pos2, clip: Rect, ppp: f32) -> Vec<Pos2> {
    bezier_points(
        [a, a + (b - a) * (2.0 / 3.0), c + (b - c) * (2.0 / 3.0), c],
        clip,
        ppp,
    )
}

/// Cubic Bezier sampling with |B''| bounded by the larger endpoint second derivative.
fn bezier_points(controls: [Pos2; 4], clip: Rect, ppp: f32) -> Vec<Pos2> {
    if controls.iter().any(|p| !p.is_finite()) {
        return Vec::new();
    }
    let acceleration = controls.windows(3).fold(Acceleration::default(), |m, p| {
        let x = 6.0 * (f64::from(p[0].x) - 2.0 * f64::from(p[1].x) + f64::from(p[2].x));
        let y = 6.0 * (f64::from(p[0].y) - 2.0 * f64::from(p[1].y) + f64::from(p[2].y));
        Acceleration {
            x: m.x.max(x.abs()),
            y: m.y.max(y.abs()),
            length: m.length.max(x.hypot(y)),
        }
    });
    smooth_path([0.0, 1.0], acceleration, clip, ppp, |t| {
        if t == 0.0 {
            return controls[0];
        }
        if t == 1.0 {
            return controls[3];
        }
        let u = 1.0 - t;
        let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let (x, y) = controls
            .iter()
            .zip(weights)
            .fold((0.0, 0.0), |(x, y), (p, w)| {
                (x + f64::from(p.x) * w, y + f64::from(p.y) * w)
            });
        Pos2::new(x as f32, y as f32)
    })
}

impl Drawing {
    /// Centripetal Catmull-Rom interpolation of the recorded samples. Repeated samples
    /// have no span; reflected endpoints give the first and last spans finite tangents.
    pub(super) fn brush_points(&self, clip: Rect, ppp: f32) -> Vec<Pos2> {
        let mut samples = self.points.clone();
        if samples.iter().any(|p| !p.is_finite()) {
            return Vec::new();
        }
        samples.dedup();
        let mut points = Vec::new();
        for i in 0..samples.len().saturating_sub(1) {
            let (a, b) = (samples[i], samples[i + 1]);
            let before = i.checked_sub(1).map_or(a + (a - b), |j| samples[j]);
            let after = samples.get(i + 2).copied().unwrap_or(b + (b - a));
            let (dt0, dt1, dt2) = (
                before.distance(a).sqrt(),
                a.distance(b).sqrt(),
                b.distance(after).sqrt(),
            );
            let m1 = ((a - before) / dt0 - (b - before) / (dt0 + dt1) + (b - a) / dt1) * dt1;
            let m2 = ((b - a) / dt1 - (after - a) / (dt1 + dt2) + (after - b) / dt2) * dt1;
            let span = bezier_points([a, a + m1 / 3.0, b - m2 / 3.0, b], clip, ppp);
            points.extend(span.into_iter().skip(usize::from(i > 0)));
        }
        points
    }
}

/// The even-odd inside of the closed polygon through `points`, as trapezoids between the
/// horizontal lines through its vertices and edge crossings, each listed as top left, top
/// right, bottom right and bottom left. Concave and self-crossing polygons fill as the
/// even-odd rule fills them; fewer than three points or a non-finite one fill nothing.
pub(super) fn even_odd_trapezoids(points: &[Pos2]) -> Vec<[Pos2; 4]> {
    if points.len() < 3 || points.iter().any(|p| !p.is_finite()) {
        return Vec::new();
    }
    // Horizontal edges bound no slab.
    let edges: Vec<[Pos2; 2]> = (0..points.len())
        .map(|i| [points[i], points[(i + 1) % points.len()]])
        .filter(|[a, b]| a.y != b.y)
        .collect();
    let mut ys: Vec<f32> = points.iter().map(|p| p.y).collect();
    for (i, &[a, b]) in edges.iter().enumerate() {
        for &[c, d] in &edges[i + 1..] {
            let (r, s, q) = (b - a, d - c, c - a);
            let denom = r.x * s.y - r.y * s.x;
            if denom != 0.0 {
                let t = (q.x * s.y - q.y * s.x) / denom;
                let u = (q.x * r.y - q.y * r.x) / denom;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    ys.push(a.y + t * r.y);
                }
            }
        }
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let x_at = |[a, b]: [Pos2; 2], y: f32| a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
    let mut trapezoids = Vec::new();
    for slab in ys.windows(2) {
        let (top, bottom) = (slab[0], slab[1]);
        // No two edges cross inside a slab, so their order at its middle holds across it.
        let mut crossing: Vec<_> = edges
            .iter()
            .filter(|[a, b]| a.y.min(b.y) <= top && a.y.max(b.y) >= bottom)
            .map(|&edge| (x_at(edge, (top + bottom) / 2.0), edge))
            .collect();
        crossing.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in crossing.as_chunks::<2>().0 {
            let (left, right) = (pair[0].1, pair[1].1);
            trapezoids.push([
                Pos2::new(x_at(left, top), top),
                Pos2::new(x_at(right, top), top),
                Pos2::new(x_at(right, bottom), bottom),
                Pos2::new(x_at(left, bottom), bottom),
            ]);
        }
    }
    trapezoids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;

    #[test]
    fn shape_and_fan_geometry_uses_the_placed_controls() {
        let rect = egui::Rect::from_min_max(egui::pos2(-400.0, -400.0), egui::pos2(400.0, 400.0));
        for (tool, anchors, want) in [
            (
                DrawingToolType::Sector,
                vec![(100.0, 100.0), (200.0, 100.0), (100.0, 150.0)],
                vec![(200.0, 100.0), (170.71068, 170.71068), (100.0, 200.0)],
            ),
            (
                DrawingToolType::Sector,
                vec![(100.0, 100.0), (200.0, 100.0), (100.0, 50.0)],
                vec![(200.0, 100.0), (170.71068, 29.28932), (100.0, 0.0)],
            ),
            (
                DrawingToolType::RotatedRect,
                vec![(0.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
                vec![(50.0, 150.0), (-50.0, 50.0)],
            ),
            (
                DrawingToolType::DoubleCurve,
                vec![(0.0, 0.0), (0.0, 100.0), (100.0, -100.0), (100.0, 0.0)],
                vec![(50.0, 0.0), (100.0, 0.0)],
            ),
            (
                DrawingToolType::Arc,
                vec![(-50.0, 0.0), (50.0, 0.0), (0.0, 50.0)],
                vec![(-50.0, 0.0), (50.0, 0.0), (0.0, 50.0)],
            ),
            (
                DrawingToolType::Pitchfan,
                vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
                vec![(400.0, 0.0), (400.0, 200.0), (400.0, 400.0)],
            ),
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.points = anchors.into_iter().map(Pos2::from).collect();
            let outline = drawing.outline(rect).unwrap();
            for point in want.into_iter().map(Pos2::from) {
                assert!(
                    outline
                        .lines
                        .iter()
                        .flatten()
                        .any(|p| p.distance(point) < 1e-3),
                    "{tool:?} missing {point:?}"
                );
            }
        }
        for tool in [DrawingToolType::Curve, DrawingToolType::DoubleCurve] {
            let mut drawing = Drawing::new(0, tool);
            drawing.points = vec![
                Pos2::ZERO,
                Pos2::new(f32::NAN, 100.0),
                Pos2::new(100.0, 100.0),
                Pos2::new(200.0, 0.0),
            ];
            assert!(drawing.enclosed_area(rect, 1.0).unwrap().is_empty());
        }
        let mut sector = Drawing::new(0, DrawingToolType::Sector);
        for points in [
            vec![Pos2::ZERO; 3],
            vec![Pos2::ZERO, Pos2::new(100.0, 0.0), Pos2::ZERO],
        ] {
            sector.points = points;
            assert!(sector.outline(rect).is_none());
        }
    }
}
