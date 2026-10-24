//! Three-anchor circular sectors and horizontal arrow marks.

use crate::drawings::Drawing;
use egui::{Pos2, Shape, Stroke};

impl Drawing {
    /// The first anchor is the center, the second fixes the radius, and the third fixes
    /// the end ray. Positive screen angles sweep clockwise, including sectors over 180°.
    pub(super) fn sector_points(&self) -> Option<Vec<Pos2>> {
        let [center, start, end, ..] = *self.points else {
            return None;
        };
        let radius = center.distance(start);
        if radius < 1e-6 || center.distance_sq(end) < 1e-12 {
            return None;
        }
        let from = (start - center).angle();
        let sweep = ((end - center).angle() - from).rem_euclid(std::f32::consts::TAU);
        let steps = ((sweep / std::f32::consts::TAU * 128.0).ceil() as usize).max(1);
        let mut points = vec![center];
        points.extend((0..=steps).map(|i| {
            let angle = from + sweep * i as f32 / steps as f32;
            center + egui::Vec2::angled(angle) * radius
        }));
        Some(points)
    }

    pub(super) fn render_sector(&self, painter: &egui::Painter) {
        let Some(points) = self.sector_points() else {
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

    pub(super) fn render_horizontal_mark(&self, painter: &egui::Painter, direction: f32) {
        let Some(&tip) = self.points.first() else {
            return;
        };
        let size = 14.0;
        painter.add(Shape::convex_polygon(
            vec![
                tip,
                tip + egui::vec2(-direction * size, -size * 0.6),
                tip + egui::vec2(-direction * size, size * 0.6),
            ],
            self.color32(),
            Stroke::NONE,
        ));
    }
}
