//! Shape and curve tool rendering implementations
//!
//! Includes: rotated rect, double curve, brush, highlighter, path, curve, and sine line, and the
//! hint a selected rectangle, circle or ellipse shows in place of its text.

use super::style::{Acceleration, ellipse_arc, smooth_path};
use crate::drawings::domain::{ChartPoint, Drawing, DrawingToolType};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke};

impl Drawing {
    /// Paints "Add text" where a selected rectangle, circle or ellipse without text would write
    /// it, in the muted colour the on-chart editor gives an empty draft, so that its text field
    /// can be found and clicked. A drawing that is locked, unselectable or closed to user edits
    /// has no field to offer.
    pub(super) fn render_text_hint(&self, painter: &egui::Painter) {
        let center = match (self.tool_type, self.points.as_slice()) {
            (DrawingToolType::Rect, &[a, b, ..]) => Rect::from_two_pos(a, b).center(),
            (DrawingToolType::Circle, &[center, _, ..]) => center,
            (DrawingToolType::Ellipse, &[a, b, ..]) => a.lerp(b, 0.5),
            _ => return,
        };
        if !self.accepts_user_edits()
            || !self.selectable
            || !center.is_finite()
            || self.text.as_deref().is_some_and(|t| !t.trim().is_empty())
        {
            return;
        }
        let color = painter.ctx().global_style().visuals.weak_text_color();
        let hint = crate::localization::text(painter.ctx(), "Add text");
        let galley = self.annotation_galley(painter, hint, color);
        let min = egui::Align2::CENTER_CENTER
            .anchor_size(center, galley.size())
            .min;
        painter.galley(min, galley, color);
    }

    /// The centre, radius and start/end angles in radians on the first three anchors. The first
    /// is the centre, the second fixes the radius and start, and the third the end, over the
    /// short arc in (-180°, 180°], clockwise at 180°. `None` for missing or non-finite anchors,
    /// a radius below 1e-6 or a third anchor on the centre.
    pub(super) fn sector_arc(&self) -> Option<(Pos2, f32, [f64; 2])> {
        let [center, start, end, ..] = *self.points else {
            return None;
        };
        if [center, start, end].iter().any(|p| !p.is_finite()) {
            return None;
        }
        let radius = center.distance(start);
        if !radius.is_finite() || radius < 1e-6 || center.distance_sq(end) < 1e-12 {
            return None;
        }
        let from = (start - center).angle();
        let mut sweep = (end - center).angle() - from;
        if sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        } else if sweep <= -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        Some((
            center,
            radius,
            [f64::from(from), f64::from(from) + f64::from(sweep)],
        ))
    }

    /// The sector's centre followed by its sampled arc, shared with projection and selection.
    pub(super) fn sector_points(&self, clip: Rect, ppp: f32) -> Option<Vec<Pos2>> {
        let (center, radius, range) = self.sector_arc()?;
        let mut points = vec![center];
        points.extend(ellipse_arc(
            center,
            egui::Vec2::splat(radius),
            range,
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

    /// A rectangle whose first two anchors form its base and whose third sets the width. With two
    /// anchors, as while the second is placed, it draws the base.
    pub(crate) fn render_rotated_rect(&self, painter: &egui::Painter) {
        let Some(corners) = self.rotated_rect_corners() else {
            if let &[a, b] = self.points.as_slice() {
                painter.line_segment([a, b], self.stroke());
            }
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
    pub(crate) fn render_double_curve(&self, painter: &egui::Painter, rect: Rect) {
        let Some(controls) = self.double_curve_controls() else {
            return;
        };
        let points = self.extended_curve(
            controls,
            rect,
            painter.clip_rect().expand(self.stroke_width + 2.0),
            painter.pixels_per_point(),
        );
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
    /// colour, so a translucent stroke does not darken where its segments meet, over a brush's
    /// fill.
    pub(crate) fn render_brush(&self, painter: &egui::Painter) {
        if self.points.len() >= 2 {
            self.paint_enclosed_fill(painter);
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
    pub(crate) fn render_curve(&self, painter: &egui::Painter, rect: Rect) {
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
            self.extended_curve(
                quadratic_controls(p0, p1, p2),
                rect,
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

    /// Samples the curve, continuing extended ends to the ends [`Self::curve_ends`] gives. Left
    /// passes the first anchor and Right the last, in either drawing direction. Non-finite
    /// controls give an empty path.
    pub(super) fn extended_curve(
        &self,
        controls: [Pos2; 4],
        rect: Rect,
        clip: Rect,
        ppp: f32,
    ) -> Vec<Pos2> {
        let mut points = bezier_points(controls, clip, ppp);
        if points.is_empty() {
            return points;
        }
        if let Some([(start, _), (end, _)]) = self.curve_ends(controls, rect) {
            if start != controls[0] {
                points.insert(0, start);
            }
            if end != controls[3] {
                points.push(end);
            }
        }
        points
    }

    /// Where each end of the curve with `controls` is painted, start then end, with the direction
    /// the curve leaves it in: the first and last control, or with Extend left or right the point
    /// where the tangent at that end leaves the pane, through whichever edge it meets first. The
    /// tangent runs from the nearest control apart from the end; where every control coincides
    /// there is none, and that end keeps its control and a zero direction. `None` when a control
    /// is not finite.
    pub(super) fn curve_ends(
        &self,
        controls: [Pos2; 4],
        rect: Rect,
    ) -> Option<[(Pos2, egui::Vec2); 2]> {
        if controls.iter().any(|p| !p.is_finite()) {
            return None;
        }
        let [a, b, c, d] = controls;
        let apart = |p: Pos2, end: Pos2| (end - p).abs().max_elem() > 1e-6;
        // How far along `step` from `from` the edge ahead lies; a steep tangent meets the top or
        // bottom edge before a side one, and the line end marker must sit in the pane.
        let reach = |from: f32, step: f32, min: f32, max: f32| match step {
            step if step > 1e-6 => (max - from) / step,
            step if step < -1e-6 => (min - from) / step,
            _ => f32::INFINITY,
        };
        let end = |end: Pos2, others: [Pos2; 3], extend: bool| {
            others
                .into_iter()
                .find(|&p| apart(p, end))
                .map_or((end, egui::Vec2::ZERO), |p| {
                    let along = end - p;
                    let edge = reach(p.x, along.x, rect.min.x, rect.max.x)
                        .min(reach(p.y, along.y, rect.min.y, rect.max.y));
                    let tip = if extend {
                        p + along * edge.max(1.0)
                    } else {
                        end
                    };
                    (tip, along)
                })
        };
        Some([
            end(a, [b, c, d], self.extend_left),
            end(d, [c, b, a], self.extend_right),
        ])
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

    /// The polygon whose even-odd inside the fill colour fills: a polyline's anchors, and an arc,
    /// curve or brush stroke closed by the chord between its ends. `None` for other tools.
    pub(super) fn enclosed_area(&self, clip: Rect, ppp: f32) -> Option<Vec<Pos2>> {
        match self.tool_type {
            DrawingToolType::Polyline => Some(self.points.clone()),
            DrawingToolType::Arc => Some(self.arc_path(clip, ppp)),
            DrawingToolType::Curve => self
                .curve_controls()
                .map(|[a, b, c]| curve_points(a, b, c, clip, ppp)),
            DrawingToolType::DoubleCurve => self.double_curve_points(clip, ppp),
            // shortcut: the fill costs quadratic time in the stroke's samples, about 170 ms per
            // frame for 3000 in a debug build; simplify the samples if long filled strokes lag.
            DrawingToolType::Brush => Some(self.brush_points(clip, ppp)),
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
    bezier_points(quadratic_controls(a, b, c), clip, ppp)
}

/// The cubic controls of the quadratic Bezier curve from `a` to `c` pulled towards `b`.
pub(super) fn quadratic_controls(a: Pos2, b: Pos2, c: Pos2) -> [Pos2; 4] {
    [a, a + (b - a) * (2.0 / 3.0), c + (b - c) * (2.0 / 3.0), c]
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
    /// The samples a brush or highlighter stroke passes through: the recorded points without
    /// repeats, a highlighter's evened out over its [`smoothing`](Self::smoothing). Empty when a
    /// point is not finite.
    pub(super) fn brush_samples(&self) -> Vec<Pos2> {
        if self.points.iter().any(|p| !p.is_finite()) {
            return Vec::new();
        }
        let mut samples = self.points.clone();
        samples.dedup();
        let reach = usize::from(self.smoothing / 2);
        if self.tool_type == DrawingToolType::Highlighter && reach > 0 && samples.len() > 2 {
            let last = samples.len() - 1;
            samples = (0..=last)
                .map(|i| {
                    // A centred window, narrower towards the ends, which stay where they are.
                    let r = reach.min(i).min(last - i);
                    let window = &samples[i - r..=i + r];
                    let sum = window
                        .iter()
                        .fold(egui::Vec2::ZERO, |sum, p| sum + p.to_vec2());
                    (sum / window.len() as f32).to_pos2()
                })
                .collect();
            // Averaging can bring neighbours together, which would give a span no length.
            samples.dedup();
        }
        samples
    }

    /// Centripetal Catmull-Rom interpolation of [`Self::brush_samples`], which hold no repeats
    /// to give a span no length; reflected endpoints give the first and last spans finite
    /// tangents.
    pub(super) fn brush_points(&self, clip: Rect, ppp: f32) -> Vec<Pos2> {
        let samples = self.brush_samples();
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

    /// Each extended end follows its nearest distinct control, and can be selected there.
    #[test]
    fn curves_extend_along_their_end_tangents() {
        use crate::drawings::services::hit_test;
        let pane = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 300.0));
        for (tool, anchors, left, right, first, last) in [
            (
                DrawingToolType::Curve,
                vec![(100.0, 150.0), (200.0, 100.0), (300.0, 150.0)],
                false,
                false,
                (100.0, 150.0),
                (300.0, 150.0),
            ),
            (
                DrawingToolType::Curve,
                vec![(100.0, 150.0), (200.0, 100.0), (300.0, 150.0)],
                true,
                true,
                (0.0, 200.0),
                (400.0, 200.0),
            ),
            (
                DrawingToolType::DoubleCurve,
                vec![
                    (100.0, 150.0),
                    (150.0, 100.0),
                    (250.0, 100.0),
                    (300.0, 150.0),
                ],
                true,
                true,
                (0.0, 250.0),
                (400.0, 250.0),
            ),
            (
                DrawingToolType::DoubleCurve,
                vec![
                    (100.0, 150.0),
                    (100.0, 150.0),
                    (250.0, 100.0),
                    (300.0, 150.0),
                ],
                true,
                false,
                (0.0, 183.33333),
                (300.0, 150.0),
            ),
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = anchors.into_iter().map(Pos2::from).collect();
            drawing.completed = true;
            drawing.extend_left = left;
            drawing.extend_right = right;
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let painter = ui.painter_at(pane);
                match tool {
                    DrawingToolType::Curve => drawing.render_curve(&painter, pane),
                    _ => drawing.render_double_curve(&painter, pane),
                }
            });
            let path = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Path(path)
                        if path.stroke.color
                            == egui::epaint::ColorMode::Solid(drawing.color32()) =>
                    {
                        Some(path)
                    }
                    _ => None,
                })
                .unwrap()
                .clone();
            output.drop_without_applying_deltas();
            assert!(
                path.points[0].distance(first.into()) < 1e-3,
                "{tool:?}: first {:?}",
                path.points[0]
            );
            assert!(
                path.points.last().unwrap().distance(last.into()) < 1e-3,
                "{tool:?}: last"
            );
            if left {
                assert_eq!(path.points[1], drawing.points[0]);
            }
            if right {
                assert_eq!(
                    path.points[path.points.len() - 2],
                    *drawing.points.last().unwrap()
                );
                let click = if tool == DrawingToolType::Curve {
                    egui::pos2(350.0, 175.0)
                } else {
                    egui::pos2(350.0, 200.0)
                };
                assert_eq!(
                    hit_test(click, std::slice::from_ref(&drawing), "1m"),
                    Some(drawing.id)
                );
                drawing.extend_left = false;
                drawing.extend_right = false;
                assert_eq!(hit_test(click, &[drawing], "1m"), None);
            }
        }
    }

    /// Line ends sit on the painted ends of a curve, double curve or brush and point the way it
    /// leaves them. The quadratic curve pulled towards (200, 100) has cubic controls
    /// (166.667, 116.667) and (233.333, 116.667), so it leaves (300, 150) at slope 1/2 and,
    /// extended right, meets x = 500 at y = 250. A double curve whose second anchor is its first
    /// leaves that anchor along the third, at slope -1/3, and extended left meets x = 0 at
    /// y = 183.333. The steep curve pulled towards (150, 50) leaves (100, 250) along
    /// (-33.333, 133.333) from its control (133.333, 116.667), so extended left it meets the
    /// bottom edge, y = 400, at x = 62.5, long before x = 0. A brush leaves each end along the
    /// chord to its neighbouring sample, a repeated last sample counting once.
    #[test]
    fn curve_and_brush_end_markers_follow_the_end_tangents() {
        use crate::drawings::ArrowStyle;
        use DrawingToolType as T;
        fn markers(shapes: &[Shape], fill: Color32, found: &mut Vec<(Pos2, egui::Vec2)>) {
            for shape in shapes {
                match shape {
                    Shape::Vec(shapes) => markers(shapes, fill, found),
                    Shape::Path(path)
                        if path.closed && path.points.len() == 3 && path.fill == fill =>
                    {
                        let [tip, left, right] = [path.points[0], path.points[1], path.points[2]];
                        found.push((tip, (tip - left.lerp(right, 0.5)).normalized()));
                    }
                    _ => {}
                }
            }
        }
        use std::f32::consts::FRAC_1_SQRT_2;
        let (curve, diagonal) = ((0.894427, 0.447214), (FRAC_1_SQRT_2, FRAC_1_SQRT_2));
        let steep = (0.242536, 0.970143);
        let brush = vec![
            (100.0, 100.0),
            (150.0, 120.0),
            (200.0, 100.0),
            (250.0, 140.0),
        ];
        let brush_ends = (
            ((100.0, 100.0), (-0.928477, -0.371391)),
            ((250.0, 140.0), (0.780869, 0.624695)),
        );
        let mut repeated = brush.clone();
        repeated.push((250.0, 140.0));
        for (tool, anchors, left, right, (start, end)) in [
            (
                T::Curve,
                vec![(100.0, 150.0), (200.0, 100.0), (300.0, 150.0)],
                false,
                false,
                (
                    ((100.0, 150.0), (-curve.0, curve.1)),
                    ((300.0, 150.0), curve),
                ),
            ),
            (
                T::Curve,
                vec![(100.0, 150.0), (200.0, 100.0), (300.0, 150.0)],
                false,
                true,
                (
                    ((100.0, 150.0), (-curve.0, curve.1)),
                    ((500.0, 250.0), curve),
                ),
            ),
            (
                T::Curve,
                vec![(100.0, 250.0), (150.0, 50.0), (200.0, 250.0)],
                true,
                false,
                (
                    ((62.5, 400.0), (-steep.0, steep.1)),
                    ((200.0, 250.0), steep),
                ),
            ),
            (
                T::DoubleCurve,
                vec![
                    (100.0, 150.0),
                    (150.0, 100.0),
                    (250.0, 100.0),
                    (300.0, 150.0),
                ],
                false,
                false,
                (
                    ((100.0, 150.0), (-diagonal.0, diagonal.1)),
                    ((300.0, 150.0), diagonal),
                ),
            ),
            (
                T::DoubleCurve,
                vec![
                    (100.0, 150.0),
                    (100.0, 150.0),
                    (250.0, 100.0),
                    (300.0, 150.0),
                ],
                true,
                false,
                (
                    ((0.0, 183.33333), (-0.948683, 0.316228)),
                    ((300.0, 150.0), diagonal),
                ),
            ),
            (T::Brush, brush, false, false, brush_ends),
            (T::Brush, repeated, false, false, brush_ends),
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = anchors.into_iter().map(Pos2::from).collect();
            drawing.completed = true;
            (drawing.extend_left, drawing.extend_right) = (left, right);
            (drawing.arrow_start_style, drawing.arrow_end_style) =
                (ArrowStyle::Arrow, ArrowStyle::Arrow);
            // The stored colour, not its shade on the default dark theme.
            drawing.theme_shade = false;
            let mut found = Vec::new();
            markers(
                &super::super::tests::paint(&drawing),
                drawing.color32(),
                &mut found,
            );
            assert_eq!(found.len(), 2, "{tool:?} {left} {right}: {found:?}");
            for ((tip, direction), (want_tip, want_direction)) in
                found.into_iter().zip([start, end])
            {
                assert!(
                    tip.distance(want_tip.into()) < 1e-3
                        && (direction - egui::Vec2::from(want_direction)).length() < 1e-4,
                    "{tool:?} {left} {right}: {tip:?} {direction:?}"
                );
            }
        }
    }

    /// A highlighter's smoothing moves each sample to the mean of itself and the samples within
    /// half the smoothing on either side, fewer near the ends, which stay where they are: 5
    /// averages five samples where it can and 255 all seven at the middle. 0 and 1 draw through
    /// every sample, as a brush does whatever its smoothing.
    #[test]
    fn highlighter_smoothing_evens_the_stroke_and_keeps_its_ends() {
        use DrawingToolType as T;
        // The third of seven samples stands 30 above the others' line.
        let samples = [
            (100.0, 100.0),
            (110.0, 100.0),
            (120.0, 130.0),
            (130.0, 100.0),
            (140.0, 100.0),
            (150.0, 100.0),
            (160.0, 100.0),
        ];
        let recorded = vec![(110.0, 100.0), (120.0, 130.0), (130.0, 100.0)];
        // Means of the three or five samples about each: y = 330 / 3, then 530 / 5 three times.
        let five = vec![
            (110.0, 110.0),
            (120.0, 106.0),
            (130.0, 106.0),
            (140.0, 106.0),
            (150.0, 100.0),
        ];
        // The middle sample is the mean of all seven, y = 730 / 7.
        let mut all = five.clone();
        all[2] = (130.0, 104.28571);
        for (tool, smoothing, on_path) in [
            (T::Highlighter, 5, five),
            (T::Highlighter, 255, all),
            (T::Highlighter, 0, recorded.clone()),
            (T::Highlighter, 1, recorded.clone()),
            (T::Brush, 5, recorded),
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = samples.map(Pos2::from).to_vec();
            drawing.completed = true;
            drawing.smoothing = smoothing;
            // The stored colour, not its shade on the default dark theme.
            drawing.theme_shade = false;
            let stroke = egui::epaint::ColorMode::Solid(drawing.stroke().color);
            let path = super::super::tests::paint(&drawing)
                .into_iter()
                .find_map(|shape| match shape {
                    Shape::Path(path) if !path.closed && path.stroke.color == stroke => Some(path),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                (path.points[0], *path.points.last().unwrap()),
                (Pos2::new(100.0, 100.0), Pos2::new(160.0, 100.0)),
                "{tool:?} {smoothing}: ends"
            );
            for point in on_path.into_iter().map(Pos2::from) {
                assert!(
                    path.points.iter().any(|p| p.distance(point) < 1e-3),
                    "{tool:?} {smoothing}: {point:?}"
                );
            }
        }
    }

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
                DrawingToolType::ProjectionTool,
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
