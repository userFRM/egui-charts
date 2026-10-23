//! Pitchfork tool rendering implementations
//!
//! Includes: standard pitchfork, modified Schiff, inside pitchfork, and pitchfan.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Standard and Schiff pitchfork
    pub(crate) fn render_pitchfork(&self, painter: &egui::Painter, chart_rect: Rect) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Need at least 3 points for pitchfork
        if self.points.len() < 3 {
            // Draw points we have so far
            for (i, &point) in self.points.iter().enumerate() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], point],
                        Stroke::new(stroke::HAIRLINE, color),
                    );
                }
            }
            return;
        }

        if let Some(lines) = self.pitchfork_lines(chart_rect) {
            self.draw_fork(painter, &lines);
        }
    }

    pub(crate) fn render_modified_schiff_pitchfork(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
    ) {
        // Modified Schiff moves the anchor point to midway between p0 and p1
        if self.points.len() < 3 {
            for &p in &self.points {
                painter.circle_filled(
                    p,
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        if let Some(lines) = self.pitchfork_lines(chart_rect) {
            self.draw_fork(painter, &lines);
        }
    }

    pub(crate) fn render_inside_pitchfork(&self, painter: &egui::Painter, chart_rect: Rect) {
        // Inside uses the first leg midpoint as one tine, and the second anchor as the other.
        if self.points.len() < 3 {
            for &p in &self.points {
                painter.circle_filled(
                    p,
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        if let Some(lines) = self.pitchfork_lines(chart_rect) {
            self.draw_fork(painter, &lines);
        }
    }

    /// The pitchfork's lines as drawn: the median and its parallels to the right
    /// edge of `rect`, then the lines between anchors the tool draws. `None`
    /// below three anchors.
    pub(super) fn pitchfork_lines(&self, rect: Rect) -> Option<Vec<[Pos2; 2]>> {
        let &[p0, p1, p2, ..] = self.points.as_slice() else {
            return None;
        };
        let mid = |a: Pos2, b: Pos2| a + (b - a) / 2.0;
        let (origin, through, upper, lower, links) = match self.tool_type {
            DrawingToolType::Pitchfork | DrawingToolType::SchiffPitchfork => {
                let schiff = self.tool_type == DrawingToolType::SchiffPitchfork;
                let (origin, through) = median_line(p0, p1, p2, schiff);
                (origin, through, p1, p2, vec![[p0, p1], [p0, p2], [p1, p2]])
            }
            DrawingToolType::ModifiedSchiffPitchfork => {
                (mid(p0, p1), mid(p1, p2), p1, p2, vec![[p0, p1], [p1, p2]])
            }
            DrawingToolType::InsidePitchfork => {
                let inside = mid(p0, p1);
                (
                    mid(inside, p1),
                    mid(p1, p2),
                    inside,
                    p1,
                    vec![[p0, p1], [p1, p2]],
                )
            }
            _ => return None,
        };
        let mut lines = fork_lines(origin, through, upper, lower, rect).to_vec();
        lines.extend(links);
        Some(lines)
    }

    /// Paints [`Self::pitchfork_lines`] and the three anchors.
    fn draw_fork(&self, painter: &egui::Painter, lines: &[[Pos2; 2]]) {
        let color = self.color32();
        let link = Stroke::new(stroke::HAIRLINE / 2.0, Color32::from_white_alpha(100));
        for (i, &line) in lines.iter().enumerate() {
            let line_stroke = match i {
                0 => Stroke::new(self.stroke_width, color),
                1 | 2 => Stroke::new(stroke::HAIRLINE, color),
                _ => link,
            };
            painter.line_segment(line, line_stroke);
        }
        for &p in &self.points[..3] {
            painter.circle_filled(p, DESIGN_TOKENS.rounding.md, color);
        }
    }

    pub(crate) fn render_pitchfan(&self, painter: &egui::Painter, rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let end = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let line_stroke_main = Stroke::new(self.stroke_width, color);

        for (i, (&ratio, [_, fan_end])) in PITCHFAN_RATIOS
            .iter()
            .zip(pitchfan_lines(center, end, rect))
            .enumerate()
        {
            let fan_angle = (fan_end.y - center.y).atan2(fan_end.x - center.x);

            let line_stroke = if i == 0 || i == PITCHFAN_RATIOS.len() - 1 {
                line_stroke_main
            } else {
                Stroke::new(
                    stroke::HAIRLINE,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        150,
                    ),
                )
            };

            painter.line_segment([center, fan_end], line_stroke);

            // Draw ratio label
            let label_dist = 60.0 + i as f32 * 10.0;
            let label_pos = Pos2::new(
                center.x + label_dist * fan_angle.cos(),
                center.y + label_dist * fan_angle.sin(),
            );
            if ratio > 0.0 {
                painter.text(
                    label_pos,
                    egui::Align2::LEFT_CENTER,
                    format!("{:.1}%", ratio * 100.0),
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }

        painter.circle_filled(center, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(end, DESIGN_TOKENS.rounding.sm, color);
    }
}

/// The fan lines' ratios: 0 turns 45 degrees one way from the line to the
/// second anchor, 0.5 runs along it and 1 turns 45 degrees the other way.
const PITCHFAN_RATIOS: [f32; 7] = [0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0];

/// The pitchfan's lines from `center`, one per ratio, turned from the line to
/// `end` and run past every edge of `rect`.
pub(super) fn pitchfan_lines(center: Pos2, end: Pos2, rect: Rect) -> Vec<[Pos2; 2]> {
    let base_angle = (end.y - center.y).atan2(end.x - center.x);
    let extend_dist = rect.width().max(rect.height()) * 2.0;
    PITCHFAN_RATIOS
        .iter()
        .map(|ratio| {
            let angle = base_angle + (ratio - 0.5) * std::f32::consts::FRAC_PI_2;
            [
                center,
                center + extend_dist * egui::vec2(angle.cos(), angle.sin()),
            ]
        })
        .collect()
}

/// A pitchfork's median from `origin` through `through` to the right edge of
/// `rect`, and its parallels from `p1` and `p2`, as long.
pub(super) fn fork_lines(
    origin: Pos2,
    through: Pos2,
    p1: Pos2,
    p2: Pos2,
    rect: Rect,
) -> [[Pos2; 2]; 3] {
    let direction = through - origin;
    [origin, p1, p2].map(|start| [start, super::lines::ray_end(start, start + direction, rect)])
}

/// Origin of the median line and the point it passes through, the midpoint of the
/// second and third anchors. The Schiff variant keeps the first anchor's time and
/// moves its price halfway to the second anchor.
fn median_line(p0: Pos2, p1: Pos2, p2: Pos2, is_schiff: bool) -> (Pos2, Pos2) {
    let origin = if is_schiff {
        Pos2::new(p0.x, (p0.y + p1.y) / 2.0)
    } else {
        p0
    };
    (origin, p1.lerp(p2, 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference: anchors (0, 0), (10, 100), (20, -50). Both medians run through
    /// (15, 25), the midpoint of the second and third anchors. The standard median
    /// starts at the first anchor (slope 5/3); the Schiff median starts at (0, 50),
    /// halfway up the first leg (slope -5/3).
    #[test]
    fn median_line_matches_construction() {
        let anchors = [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 100.0),
            Pos2::new(20.0, -50.0),
        ];
        for (is_schiff, origin, through) in [
            (false, Pos2::new(0.0, 0.0), Pos2::new(15.0, 25.0)),
            (true, Pos2::new(0.0, 50.0), Pos2::new(15.0, 25.0)),
        ] {
            let got = median_line(anchors[0], anchors[1], anchors[2], is_schiff);
            assert!(
                got.0.distance(origin) < 1e-4 && got.1.distance(through) < 1e-4,
                "schiff {is_schiff}: got {got:?}, want ({origin:?}, {through:?})"
            );
        }
        // Inside: I=(A+B)/2=(5,50), median from (I+B)/2=(7.5,75)
        // through (B+C)/2=(15,25), parallel to I->C, slope -20/3.
        for (tool, origin, slope) in [
            (DrawingToolType::Pitchfork, Pos2::new(0.0, 0.0), 5.0 / 3.0),
            (
                DrawingToolType::SchiffPitchfork,
                Pos2::new(0.0, 50.0),
                -5.0 / 3.0,
            ),
            (
                DrawingToolType::ModifiedSchiffPitchfork,
                Pos2::new(5.0, 50.0),
                -2.5,
            ),
            (
                DrawingToolType::InsidePitchfork,
                Pos2::new(7.5, 75.0),
                -20.0 / 3.0,
            ),
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.points = anchors.to_vec();
            let lines = drawing
                .pitchfork_lines(Rect::from_min_max(
                    Pos2::new(-100.0, -1000.0),
                    Pos2::new(100.0, 1000.0),
                ))
                .unwrap();
            assert!(
                lines[0][0].distance(origin) < 1e-4,
                "{tool:?}: {:?}",
                lines[0]
            );
            for [a, b] in &lines[..3] {
                assert!(
                    ((b.y - a.y) / (b.x - a.x) - slope).abs() < 1e-4,
                    "{tool:?}: {a:?} {b:?}"
                );
            }
        }
    }
}
