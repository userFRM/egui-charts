//! Elliott wave tool rendering: impulse, correction, triangle, double and triple combination.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Pos2, Stroke, vec2};

impl Drawing {
    /// Renders an Elliott wave count: its legs in the drawing's colour, width and line style,
    /// and the label of each wave in the drawing's degree, above a high and below a low. The
    /// start point is not labelled.
    pub(crate) fn render_elliott_waves(&self, painter: &egui::Painter) {
        let color = self.color32();
        for leg in self.points.windows(2) {
            painter.line_segment([leg[0], leg[1]], self.stroke());
        }
        // Placement anchors
        if !self.completed {
            for &point in &self.points {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }
        }
        let waves = match self.tool_type {
            DrawingToolType::ElliottImpulse => "12345",
            DrawingToolType::ElliottCorrection => "ABC",
            DrawingToolType::ElliottTriangle => "ABCDE",
            DrawingToolType::ElliottDoubleCombo => "WXY",
            DrawingToolType::ElliottTripleCombo => "WXYXZ",
            _ => "",
        };
        // Clears the placement anchor and its ring.
        let gap = DESIGN_TOKENS.spacing.xl;
        for (index, wave) in (1..self.points.len()).zip(waves.chars()) {
            let point = self.points[index];
            let (pos, align) = if is_high(&self.points, index) {
                (point - vec2(0.0, gap), Align2::CENTER_BOTTOM)
            } else {
                (point + vec2(0.0, gap), Align2::CENTER_TOP)
            };
            painter.text(
                pos,
                align,
                self.elliott_degree.label(wave),
                self.annotation_font(),
                color,
            );
        }
    }
}

/// Whether `points[index]` is a high: above the point before it, or level with it and above the
/// point after it, as wave W of a combination saved before storage version 5 is level with the
/// start point it was given on load.
fn is_high(points: &[Pos2], index: usize) -> bool {
    let (y, before) = (points[index].y, points[index - 1].y);
    if y != before {
        return y < before;
    }
    points.get(index + 1).is_some_and(|after| y < after.y)
}

#[cfg(test)]
mod tests {
    use crate::drawings::domain::{Drawing, DrawingToolType, ElliottDegree};
    use crate::tokens::DESIGN_TOKENS;
    use egui::{Pos2, Shape, epaint::ColorMode};

    /// A tool and degree, its points and the labels it must draw.
    type Case = (
        DrawingToolType,
        ElliottDegree,
        &'static [(f32, f32)],
        &'static [&'static str],
    );

    /// Highs and lows in turn after a low start: labels go above, below, above...
    const ZIGZAG: [(f32, f32); 6] = [
        (100., 300.),
        (160., 100.),
        (220., 250.),
        (280., 150.),
        (340., 220.),
        (400., 180.),
    ];

    #[test]
    fn elliott_waves_draw_plain_legs_and_a_label_at_each_wave_end() {
        use DrawingToolType::*;
        use ElliottDegree::*;
        let zigzag = &ZIGZAG[..];
        let cases: [Case; 17] = [
            (
                ElliottImpulse,
                GrandSupercycle,
                zigzag,
                &["((I))", "((II))", "((III))", "((IV))", "((V))"],
            ),
            (
                ElliottImpulse,
                Supercycle,
                zigzag,
                &["(I)", "(II)", "(III)", "(IV)", "(V)"],
            ),
            (
                ElliottImpulse,
                Cycle,
                zigzag,
                &["I", "II", "III", "IV", "V"],
            ),
            (
                ElliottImpulse,
                Primary,
                zigzag,
                &["((1))", "((2))", "((3))", "((4))", "((5))"],
            ),
            (
                ElliottImpulse,
                Intermediate,
                zigzag,
                &["(1)", "(2)", "(3)", "(4)", "(5)"],
            ),
            (ElliottImpulse, Minor, zigzag, &["1", "2", "3", "4", "5"]),
            (
                ElliottImpulse,
                Minute,
                zigzag,
                &["((i))", "((ii))", "((iii))", "((iv))", "((v))"],
            ),
            (
                ElliottImpulse,
                Minuette,
                zigzag,
                &["(i)", "(ii)", "(iii)", "(iv)", "(v)"],
            ),
            (
                ElliottImpulse,
                Subminuette,
                zigzag,
                &["i", "ii", "iii", "iv", "v"],
            ),
            (ElliottCorrection, Minor, &zigzag[..4], &["A", "B", "C"]),
            (
                ElliottCorrection,
                Minuette,
                &zigzag[..4],
                &["(a)", "(b)", "(c)"],
            ),
            (
                ElliottTriangle,
                Primary,
                zigzag,
                &["((A))", "((B))", "((C))", "((D))", "((E))"],
            ),
            (
                ElliottDoubleCombo,
                Minute,
                &zigzag[..4],
                &["((w))", "((x))", "((y))"],
            ),
            (
                ElliottTripleCombo,
                Cycle,
                zigzag,
                &["W", "X", "Y", "X", "Z"],
            ),
            // A combination saved before storage version 5 loads with its start on wave W.
            (
                ElliottDoubleCombo,
                Minor,
                &[(100., 100.), (100., 100.), (160., 250.), (220., 150.)],
                &["W", "X", "Y"],
            ),
            // Placing: the start alone has no label; the first wave is labelled at once.
            (ElliottImpulse, Minor, &zigzag[..1], &[]),
            (ElliottImpulse, Minor, &zigzag[..2], &["1"]),
        ];
        for (tool, degree, points, labels) in cases {
            let case = format!("{tool:?} {degree:?} {}", points.len());
            let mut drawing = Drawing::new(7, tool);
            assert!(drawing.fill_color.is_none(), "{case}: default fill");
            drawing.elliott_degree = degree;
            drawing.fill_color = Some([12, 34, 56, 80]); // as saved by an earlier release
            drawing.points = points.iter().map(|&(x, y)| Pos2::new(x, y)).collect();
            let stroke = drawing.stroke();
            let mut legs = Vec::new();
            let mut texts = Vec::new();
            for shape in super::super::tests::paint(&drawing) {
                match &shape {
                    Shape::Text(t) => texts.push((
                        t.galley.text().to_owned(),
                        egui::Rect::from_min_size(t.pos, t.galley.size()),
                    )),
                    Shape::LineSegment { points, stroke: s } => {
                        assert_eq!(*s, stroke, "{case}");
                        legs.push(*points);
                    }
                    Shape::Path(p) if !p.closed => {
                        assert_eq!(p.stroke.width, stroke.width, "{case}");
                        let ColorMode::Solid(color) = &p.stroke.color else {
                            panic!("{case}: textured leg");
                        };
                        assert_eq!(*color, stroke.color, "{case}");
                        legs.extend(p.points.windows(2).map(|w| [w[0], w[1]]));
                    }
                    Shape::Noop => {}
                    // Placement anchors, as circles or polygons.
                    other => {
                        let r = other.visual_bounding_rect();
                        assert!(
                            drawing.points.iter().any(|p| r.center().distance(*p) < 1.0)
                                && r.width() <= 2.0 * DESIGN_TOKENS.rounding.lg + 4.0,
                            "{case}: {other:?}"
                        );
                    }
                }
            }
            let want: Vec<_> = drawing.points.windows(2).map(|w| [w[0], w[1]]).collect();
            assert_eq!(legs, want, "{case}");
            let got: Vec<_> = texts.iter().map(|(text, _)| text.as_str()).collect();
            assert_eq!(got, *labels, "{case}");
            for (i, (text, rect)) in texts.iter().enumerate() {
                let point = drawing.points[i + 1];
                assert!((rect.center().x - point.x).abs() < 0.5, "{case} {text}");
                if i % 2 == 0 {
                    assert!(rect.bottom() < point.y, "{case} {text} above");
                } else {
                    assert!(rect.top() > point.y, "{case} {text} below");
                }
            }
        }
    }
}
