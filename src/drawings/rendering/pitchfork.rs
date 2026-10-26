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

        self.draw_fork(painter, chart_rect);
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

        self.draw_fork(painter, chart_rect);
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

        self.draw_fork(painter, chart_rect);
    }

    /// The pitchfork's median and parallels towards the pane edge in their
    /// direction, then the construction links. `None`
    /// below three anchors.
    fn pitchfork_base_lines(&self, rect: Rect) -> Option<Vec<[Pos2; 2]>> {
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

    /// All visible level lines, followed by the construction links, shared with selection.
    pub(super) fn pitchfork_lines(&self, rect: Rect) -> Option<Vec<[Pos2; 2]>> {
        let base = self.pitchfork_base_lines(rect)?;
        let config = self.pitchfork.clone().unwrap_or_default();
        let mut lines = Vec::new();
        for level in config
            .levels
            .iter()
            .filter(|l| l.visible && l.value.is_finite() && l.value >= 0.0)
        {
            lines.extend(level_lines(&base, level.value, rect));
        }
        lines.extend_from_slice(&base[3..]);
        Some(lines)
    }

    /// Bands stop on either side of a hidden level instead of bridging across it.
    pub(super) fn pitchfork_bands(&self, rect: Rect) -> Vec<Vec<Pos2>> {
        let Some(base) = self.pitchfork_base_lines(rect) else {
            return Vec::new();
        };
        let mut config = self.pitchfork.clone().unwrap_or_default();
        if !config.show_background {
            return Vec::new();
        }
        config
            .levels
            .retain(|l| l.value.is_finite() && l.value >= 0.0);
        config.levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        let mut bands = Vec::new();
        for pair in config
            .levels
            .windows(2)
            .filter(|p| p[0].visible && p[1].visible)
        {
            let inner = level_lines(&base, pair[0].value, rect);
            let outer = level_lines(&base, pair[1].value, rect);
            for (side, outside) in outer.iter().enumerate() {
                let inside = inner[side.min(inner.len() - 1)];
                bands.push(vec![inside[0], inside[1], outside[1], outside[0]]);
            }
        }
        bands
    }

    /// Paints the levels, band fill, construction links and anchors.
    fn draw_fork(&self, painter: &egui::Painter, rect: Rect) {
        let Some(base) = self.pitchfork_base_lines(rect) else {
            return;
        };
        let config = self.pitchfork.clone().unwrap_or_default();
        let fill = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            config.background_opacity,
        );
        for band in self.pitchfork_bands(rect) {
            painter.add(egui::Shape::convex_polygon(band, fill, Stroke::NONE));
        }
        for level in config
            .levels
            .iter()
            .filter(|l| l.visible && l.value.is_finite() && l.value >= 0.0)
        {
            let rgba = if self.pitchfork.is_none() {
                self.color
            } else {
                level.color
            };
            let color = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
            let width = if self.pitchfork.is_none() && level.value == 1.0 {
                stroke::HAIRLINE
            } else {
                self.stroke_width
            };
            for [a, b] in level_lines(&base, level.value, rect) {
                if let Some((dash, gap)) = level.line_style.dash_pattern() {
                    super::utils::draw_dashed_line(
                        painter,
                        a,
                        b,
                        Stroke::new(width, color),
                        dash,
                        gap,
                    );
                } else {
                    painter.line_segment([a, b], Stroke::new(width, color));
                }
            }
        }
        for &line in &base[3..] {
            painter.line_segment(
                line,
                Stroke::new(stroke::HAIRLINE / 2.0, self.color32().gamma_multiply(0.4)),
            );
        }
        for &p in &self.points[..3] {
            painter.circle_filled(p, DESIGN_TOKENS.rounding.md, self.color32());
        }
    }

    pub(crate) fn render_pitchfan(&self, painter: &egui::Painter, rect: Rect) {
        let &[center, end, boundary, ..] = self.points.as_slice() else {
            return;
        };
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let line_stroke_main = Stroke::new(self.stroke_width, color);

        for (i, (&ratio, [_, fan_end])) in PITCHFAN_RATIOS
            .iter()
            .zip(pitchfan_lines(center, end, boundary, rect))
            .enumerate()
        {
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

            // Place labels at the visible end of each ray, where the fan is widest.
            let run = fan_end - center;
            let t = if fan_end.y < rect.min.y {
                (rect.min.y - center.y) / run.y
            } else if fan_end.y > rect.max.y {
                (rect.max.y - center.y) / run.y
            } else {
                1.0
            };
            let label_pos = center + run * t;
            if ratio > 0.0 {
                painter.text(
                    label_pos,
                    if fan_end.x >= center.x {
                        egui::Align2::RIGHT_BOTTOM
                    } else {
                        egui::Align2::LEFT_BOTTOM
                    },
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

/// Interpolate each tine's anchor from the median origin, then run parallel to the median.
fn level_lines(base: &[[Pos2; 2]], value: f32, rect: Rect) -> Vec<[Pos2; 2]> {
    if value == 0.0 {
        return vec![base[0]];
    }
    let origin = base[0][0];
    let direction = base[0][1] - origin;
    base[1..3]
        .iter()
        .map(|line| {
            let start = origin + (line[0] - origin) * value;
            [start, super::lines::ray_end(start, start + direction, rect)]
        })
        .collect()
}

/// Fractions along the pitchfork's tine span through which the fan rays pass.
const PITCHFAN_RATIOS: [f32; 7] = [0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0];

/// Rays from the first pivot through fractional points on the second-to-third leg.
pub(super) fn pitchfan_lines(
    center: Pos2,
    end: Pos2,
    boundary: Pos2,
    rect: Rect,
) -> Vec<[Pos2; 2]> {
    PITCHFAN_RATIOS
        .iter()
        .map(|&ratio| {
            [
                center,
                super::lines::ray_end(center, end.lerp(boundary, ratio), rect),
            ]
        })
        .collect()
}

/// A pitchfork's median from `origin` through `through`, extended towards the
/// pane edge, and its parallels from `p1` and `p2`.
fn fork_lines(origin: Pos2, through: Pos2, p1: Pos2, p2: Pos2, rect: Rect) -> [[Pos2; 2]; 3] {
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
                .pitchfork_base_lines(Rect::from_min_max(
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
    #[test]
    fn pitchfork_levels_are_mirrored_filled_saved_and_selectable() {
        let rect = Rect::from_min_max(Pos2::new(-100.0, -2000.0), Pos2::new(400.0, 2000.0));
        // At x=40: median, half-distance lines, tines, one-and-a-half-distance lines.
        for (tool, ys) in [
            (
                DrawingToolType::Pitchfork,
                [66.6667, 108.3333, 25.0, 150.0, -16.6667, 191.6667, -58.3333],
            ),
            (
                DrawingToolType::SchiffPitchfork,
                [-16.6667, 16.6667, -50.0, 50.0, -83.3333, 83.3333, -116.6667],
            ),
            (
                DrawingToolType::ModifiedSchiffPitchfork,
                [-37.5, -6.25, -68.75, 25.0, -100.0, 56.25, -131.25],
            ),
            (
                DrawingToolType::InsidePitchfork,
                [
                    -141.6667, -162.5, -120.8333, -183.3333, -100.0, -204.1667, -79.1667,
                ],
            ),
        ] {
            let mut drawing = Drawing::new(12, tool);
            drawing.points = vec![Pos2::ZERO, Pos2::new(10.0, 100.0), Pos2::new(20.0, -50.0)];
            let lines = drawing.pitchfork_lines(rect).unwrap();
            assert!(lines.len() >= 7, "{tool:?}: {lines:?}");
            for ([a, b], want) in lines.iter().take(7).zip(ys) {
                let y = a.y + (40.0 - a.x) * (b.y - a.y) / (b.x - a.x);
                assert!((y - want).abs() < 1e-3, "{tool:?}: {y} != {want}");
            }
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                drawing.draw_fork(ui.painter(), rect)
            });
            let first_color = output.shapes.iter().find_map(|s| {
                if let egui::Shape::LineSegment { stroke, .. } = &s.shape {
                    Some(stroke.color)
                } else {
                    None
                }
            });
            output.drop_without_applying_deltas();
            assert_eq!(
                first_color,
                Some(drawing.color32()),
                "default levels inherit the drawing colour"
            );
            let outline = drawing.outline(rect).unwrap();
            assert_eq!(outline.areas.len(), 6);
            let point = Pos2::new(40.0, ys[5]);
            assert_eq!(
                crate::drawings::services::DrawingInteraction::new().hit_test(
                    point,
                    &[drawing.clone()],
                    "1m"
                ),
                Some(12)
            );

            // Inject custom settings through the saved form so old files and new fields both use serde.
            let mut saved = serde_json::to_value(&drawing).unwrap();
            let config = serde_json::json!({"levels":[
                {"value":0.0,"visible":true,"color":[10,20,30,255],"line_style":"Dotted"},
                {"value":0.5,"visible":false,"color":[10,20,30,255],"line_style":"Dashed"},
                {"value":1.0,"visible":true,"color":[10,20,30,255],"line_style":"Solid"},
                {"value":1.5,"visible":true,"color":[10,20,30,255],"line_style":"Solid"}
            ],"show_background":true,"background_opacity":35});
            saved["pitchfork"] = config.clone();
            let mut restored: Drawing = serde_json::from_value(saved.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&restored).unwrap()["pitchfork"],
                config
            );
            restored.points = drawing.points.clone();
            assert_eq!(restored.outline(rect).unwrap().areas.len(), 2);
            assert_eq!(
                restored.pitchfork_lines(rect).unwrap().len(),
                lines.len() - 2
            );
            saved["pitchfork"]["show_background"] = false.into();
            let mut unfilled: Drawing = serde_json::from_value(saved.clone()).unwrap();
            unfilled.points = drawing.points.clone();
            assert!(unfilled.outline(rect).unwrap().areas.is_empty());
            saved.as_object_mut().unwrap().remove("pitchfork");
            let mut legacy: Drawing = serde_json::from_value(saved).unwrap();
            legacy.points = drawing.points;
            assert_eq!(legacy.pitchfork_lines(rect).unwrap(), lines);
        }
    }
}
