//! Common drawing styles applied to renderer primitives.

use crate::drawings::{ArrowStyle, Drawing, DrawingToolType, FontWeight};
use crate::model::LineStyle;
use crate::theme::ThemeExt;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, epaint::ColorMode, layers::ShapeIdx};

/// Colours of the box around an annotation's text, from [`Drawing::text_box_colors`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextBoxColors {
    /// Colour of the text.
    pub(crate) text: Color32,
    /// Fill of the box; transparent when the background is off.
    pub(crate) fill: Color32,
    /// Border of the box, 1 px; `Stroke::NONE` when the border is off.
    pub(crate) border: Stroke,
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

impl Drawing {
    /// Size annotations before laying out both the text and its surrounding box.
    pub(super) fn annotation_font(&self) -> egui::FontId {
        egui::FontId::proportional(crate::styles::typography::font_size(self.font_size))
    }

    /// Whether the tool's text sits on a solid label in the drawing colour rather than on a card
    /// in the theme's panel colour.
    pub(crate) fn solid_label(&self) -> bool {
        matches!(
            self.tool_type,
            DrawingToolType::PriceNote | DrawingToolType::Comment | DrawingToolType::Signpost
        )
    }

    /// Colours of the box around an annotation's text. Unset colours follow the tool: a card
    /// takes the active theme's panel colour with its text and border in the drawing colour, and
    /// a solid label takes the drawing colour with white or black text, whichever reads on it.
    /// Text without a background takes the drawing colour.
    pub(crate) fn text_box_colors(&self, ctx: &egui::Context) -> TextBoxColors {
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let (fill, text) = if self.background {
            // The theme is read only for a card without a fill colour.
            let fill = self.fill_color32().unwrap_or_else(|| {
                if self.solid_label() {
                    self.color32()
                } else {
                    ctx.active_theme().ui.panel_bg
                }
            });
            let text = if self.solid_label() {
                label_text_color(fill)
            } else {
                self.color32()
            };
            (fill, text)
        } else {
            (Color32::TRANSPARENT, self.color32())
        };
        TextBoxColors {
            text: self.text_color.map_or(text, rgba),
            fill,
            border: if self.border {
                Stroke::new(
                    crate::styles::stroke::HAIRLINE,
                    self.border_color.map_or(self.color32(), rgba),
                )
            } else {
                Stroke::NONE
            },
        }
    }

    /// Lays annotation text out once, at the drawing's font size and slant, for both sizing its
    /// box and painting it.
    pub(super) fn annotation_galley(
        &self,
        painter: &Painter,
        text: String,
        color: Color32,
    ) -> std::sync::Arc<egui::Galley> {
        let format = egui::TextFormat {
            italics: self.italic,
            ..egui::TextFormat::simple(self.annotation_font(), color)
        };
        painter.layout_job(egui::text::LayoutJob::simple_format(text, format))
    }

    /// Apply line patterns and font weight without changing the renderer's widths or fills.
    pub(super) fn finish_style(&self, painter: &Painter, start: usize, rect: Rect) {
        let styled = self.fill_color.is_some()
            || self.line_style != LineStyle::Solid
            || self.font_weight == FontWeight::Bold
            || self.extend_left
            || self.extend_right;
        let clip = crate::painting::dashed_line::finite_clip(painter);
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            // Segments that continue one another become one path, which the tessellator joins
            // with half the vertices and a dash pattern runs along without restarting.
            let mut previous: Option<(ShapeIdx, Pos2, Stroke, Rect)> = None;
            for index in start..list.next_idx().0 {
                let mut segment = None;
                list.mutate_shape(ShapeIdx(index), |s| {
                    if styled
                        && matches!(
                            self.tool_type,
                            DrawingToolType::InfoLine
                                | DrawingToolType::TrendAngle
                                | DrawingToolType::Arrow
                                | DrawingToolType::ArrowMarker
                        )
                        && let Shape::LineSegment { points, .. } = &mut s.shape
                        && self.points.get(..2) == Some(points.as_slice())
                        && let Some((a, b)) = self.trend_line_ends(rect)
                    {
                        *points = [a, b];
                    }
                    if let Shape::LineSegment { points, stroke } = s.shape {
                        segment = Some((points, stroke, s.clip_rect));
                    }
                });
                if let Some((points, stroke, clip)) = segment {
                    if let Some((first, end, old_stroke, old_clip)) = previous
                        && end == points[0]
                        && old_stroke == stroke
                        && old_clip == clip
                    {
                        list.mutate_shape(first, |s| match &mut s.shape {
                            Shape::LineSegment { points: old, .. } => {
                                s.shape = Shape::line(vec![old[0], old[1], points[1]], stroke)
                            }
                            Shape::Path(p) => p.points.push(points[1]),
                            _ => {}
                        });
                        list.reset_shape(ShapeIdx(index));
                        previous = Some((first, points[1], stroke, clip));
                    } else {
                        previous = Some((ShapeIdx(index), points[1], stroke, clip));
                    }
                } else {
                    previous = None;
                }
            }
            if styled {
                for index in start..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(index), |s| self.style_shape(&mut s.shape, clip));
                }
            }
        });
        self.render_end_markers(painter, rect);
        let pixels_per_point = painter.pixels_per_point();
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            for index in start..list.next_idx().0 {
                list.mutate_shape(ShapeIdx(index), |s| {
                    fewer_circle_sides(&mut s.shape, pixels_per_point)
                });
            }
        });
    }

    /// Restyles one shape. Dashes are generated only inside `clip`, so a drawing reaching far off
    /// screen costs no more than its visible part; without a finite clip nothing is dashed.
    fn style_shape(&self, shape: &mut Shape, clip: Option<Rect>) {
        if let Shape::Vec(shapes) = shape {
            for s in shapes {
                self.style_shape(s, clip);
            }
            return;
        }
        if let Shape::Text(t) = shape {
            if self.font_weight == FontWeight::Bold {
                // Synthetic weight keeps host font families and fallback glyphs intact. The copy
                // moves along the baseline, which text turned along a line turns with it.
                let mut weighted = t.clone();
                weighted.pos += egui::Vec2::angled(t.angle) * 0.6;
                *shape = Shape::Vec(vec![Shape::Text(t.clone()), Shape::Text(weighted)]);
            }
            return;
        }
        let fork = matches!(
            self.tool_type,
            DrawingToolType::Pitchfork
                | DrawingToolType::SchiffPitchfork
                | DrawingToolType::ModifiedSchiffPitchfork
                | DrawingToolType::InsidePitchfork
                | DrawingToolType::Pitchfan
        );
        let custom_fork = fork && self.pitchfork.is_some();
        // Keep body fills of annotations and legacy forks; analysis labels keep their own fill.
        let fill = |color: &mut Color32| {
            if color.a() > 0
                && !custom_fork
                && matches!(
                    self.tool_type,
                    DrawingToolType::Pitchfork
                        | DrawingToolType::SchiffPitchfork
                        | DrawingToolType::ModifiedSchiffPitchfork
                        | DrawingToolType::InsidePitchfork
                        | DrawingToolType::TextLabel
                        | DrawingToolType::AnchoredText
                        | DrawingToolType::Note
                        | DrawingToolType::Callout
                        | DrawingToolType::PriceLabel
                        | DrawingToolType::FlagNote
                        | DrawingToolType::PriceNote
                        | DrawingToolType::Table
                        | DrawingToolType::Comment
                        | DrawingToolType::Signpost
                        | DrawingToolType::Image
                        | DrawingToolType::Tweet
                        | DrawingToolType::Idea
                        | DrawingToolType::Forecast
                        | DrawingToolType::ArrowMarkUp
                        | DrawingToolType::ArrowMarkDown
                        | DrawingToolType::ArrowMarkLeft
                        | DrawingToolType::ArrowMarkRight
                )
                && let Some(configured) = self.fill_color32()
            {
                *color = configured;
            }
        };
        let dash = if custom_fork
            || matches!(
                self.tool_type,
                DrawingToolType::RegressionTrend
                    | DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciExtension
                    | DrawingToolType::FibonacciChannel
                    | DrawingToolType::FibonacciTimeZones
                    | DrawingToolType::TrendBasedFibTime
                    | DrawingToolType::FibonacciSpeedFan
                    | DrawingToolType::FibonacciCircles
                    | DrawingToolType::FibonacciSpeedResistanceArcs
                    | DrawingToolType::FibonacciWedge
            ) {
            None
        } else {
            self.line_style.dash_pattern()
        };
        let mut path = Vec::new();
        let stroke = match shape {
            Shape::LineSegment { points, stroke } => {
                path.extend(*points);
                *stroke
            }
            Shape::Path(p) => {
                fill(&mut p.fill);
                let ColorMode::Solid(color) = &p.stroke.color else {
                    return;
                };
                let stroke = Stroke::new(p.stroke.width, *color);
                if dash.is_none() || stroke.is_empty() {
                    return;
                }
                path = p.points.clone();
                if p.closed
                    && let Some(&first) = path.first()
                {
                    path.push(first);
                }
                p.stroke.width = 0.0;
                stroke
            }
            Shape::Rect(r) => {
                fill(&mut r.fill);
                if dash.is_none() || r.stroke.is_empty() {
                    return;
                }
                egui::epaint::tessellator::path::rounded_rectangle(
                    &mut path,
                    egui::epaint::RoundedRect::new(r.rect, r.corner_radius),
                );
                if let Some(&first) = path.first() {
                    path.push(first);
                }
                let stroke = r.stroke;
                r.stroke = Stroke::NONE;
                stroke
            }
            Shape::Circle(c) => {
                if dash.is_none() || c.stroke.is_empty() {
                    return;
                }
                path = (0..=64)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 64.0;
                        c.center + c.radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect();
                let stroke = c.stroke;
                c.stroke = Stroke::NONE;
                stroke
            }
            Shape::Ellipse(e) => {
                if dash.is_none() || e.stroke.is_empty() {
                    return;
                }
                path = (0..=64)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 64.0;
                        e.center + e.radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect();
                let stroke = e.stroke;
                e.stroke = Stroke::NONE;
                stroke
            }
            _ => return,
        };
        if let Some((dash, gap)) = dash
            && !stroke.is_empty()
        {
            let mut shapes = clip.map_or_else(Vec::new, |clip| {
                crate::painting::dashed_line::dashed_polyline_shapes(&path, stroke, dash, gap, clip)
            });
            if !matches!(shape, Shape::LineSegment { .. }) {
                shapes.insert(0, std::mem::replace(shape, Shape::Noop));
            }
            *shape = Shape::Vec(shapes);
        }
    }

    fn render_end_markers(&self, painter: &Painter, rect: Rect) {
        use DrawingToolType as T;
        if !self.arrow_start_style.is_visible() && !self.arrow_end_style.is_visible() {
            return;
        }
        let Some(&a) = self.points.first() else {
            return;
        };
        let b = self.points.get(1).copied().unwrap_or(a);
        let ends = match self.tool_type {
            T::TrendLine
            | T::InfoLine
            | T::TrendAngle
            | T::Ray
            | T::ExtendedLine
            | T::Arrow
            | T::ArrowMarker => self.trend_line_ends(rect).map(|(a, b)| [a, b]),
            T::HorizontalLine => Some([Pos2::new(rect.min.x, a.y), Pos2::new(rect.max.x, a.y)]),
            T::HorizontalRay => Some([a, Pos2::new(rect.max.x, a.y)]),
            T::VerticalLine => Some([Pos2::new(a.x, rect.min.y), Pos2::new(a.x, rect.max.y)]),
            T::Polyline | T::Path => self.points.last().map(|&b| [a, b]),
            _ => None,
        };
        if let Some(mut ends) = ends {
            if matches!(
                self.tool_type,
                T::TrendLine
                    | T::InfoLine
                    | T::TrendAngle
                    | T::ExtendedLine
                    | T::Ray
                    | T::Arrow
                    | T::ArrowMarker
            ) && (b.x < a.x || (self.tool_type == T::ExtendedLine && b.x == a.x && b.y < a.y))
            {
                ends.swap(0, 1);
            }
            let [start, end] = ends;
            let (start_direction, end_direction) =
                if matches!(self.tool_type, T::Polyline | T::Path) && self.points.len() > 2 {
                    (a - b, end - self.points[self.points.len() - 2])
                } else {
                    (start - end, end - start)
                };
            self.render_marker(painter, start, start_direction, self.arrow_start_style);
            self.render_marker(painter, end, end_direction, self.arrow_end_style);
        }
    }

    fn render_marker(
        &self,
        painter: &Painter,
        tip: Pos2,
        direction: egui::Vec2,
        style: ArrowStyle,
    ) {
        if direction.length_sq() < 1e-6 {
            return;
        }
        let color = self.color32();
        let size = (self.stroke_width * style.size_multiplier()).max(4.0);
        let along = direction.normalized() * size;
        let across = egui::vec2(-along.y, along.x) / 2.0;
        let base = tip - along;
        match style {
            ArrowStyle::None => {}
            ArrowStyle::Arrow => {
                painter.add(Shape::convex_polygon(
                    vec![tip, base + across, base - across],
                    color,
                    Stroke::NONE,
                ));
            }
            ArrowStyle::OpenArrow => {
                painter.add(Shape::line(
                    vec![base + across, tip, base - across],
                    self.stroke(),
                ));
            }
            ArrowStyle::Circle => {
                painter.circle_filled(tip, size / 2.0, color);
            }
            ArrowStyle::Square => {
                painter.rect_filled(
                    Rect::from_center_size(tip, egui::Vec2::splat(size)),
                    0.0,
                    color,
                );
            }
            ArrowStyle::Diamond => {
                painter.add(Shape::convex_polygon(
                    vec![
                        tip + along / 2.0,
                        tip + across,
                        tip - along / 2.0,
                        tip - across,
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// Replaces a circle with the polygon of the fewest sides within a quarter of a physical pixel of
/// it. The tessellator gives a circle 32 sides from a radius of 5 points and 64 from 18, which made
/// the anchor dots and rings of a drawing most of its vertices. A small disc without an outline
/// stays a circle, which the tessellator draws as one textured quad.
fn fewer_circle_sides(shape: &mut Shape, pixels_per_point: f32) {
    match shape {
        Shape::Vec(shapes) => {
            for shape in shapes {
                fewer_circle_sides(shape, pixels_per_point);
            }
        }
        Shape::Circle(c) if !c.stroke.is_empty() || c.radius * pixels_per_point > 6.0 => {
            let sides = circle_sides(c.radius * pixels_per_point);
            let points = (0..sides)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / sides as f32;
                    c.center + c.radius * egui::vec2(angle.cos(), angle.sin())
                })
                .collect();
            let mut path = egui::epaint::PathShape::convex_polygon(points, c.fill, c.stroke);
            path.stroke = path.stroke.outside();
            *shape = Shape::Path(path);
        }
        _ => {}
    }
}

/// The fewest sides of a polygon whose sides stay within a quarter of a physical pixel of a
/// circle of `radius` physical pixels, from 8 to 128.
pub(super) fn circle_sides(radius: f32) -> usize {
    let step = (1.0 - 0.25 / radius).clamp(-1.0, 1.0).acos();
    ((std::f32::consts::PI / step).ceil() as usize).clamp(8, 128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::StrokeKind;

    fn paint(drawing: &Drawing) -> Vec<Shape> {
        paint_on(&egui::Context::default(), drawing, false)
    }

    fn paint_on(ctx: &egui::Context, drawing: &Drawing, selected: bool) -> Vec<Shape> {
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            },
            |ui| {
                drawing.render_with_selection(
                    ui.painter(),
                    pane,
                    &crate::drawings::rendering::ChartView {
                        price_formatter: None,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|x| x,
                        price_to_y: &|p| p as f32,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions: None,
                    },
                    selected,
                );
            },
        );
        fn collect(shape: &Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                _ => out.push(shape.clone()),
            }
        }
        let mut shapes = Vec::new();
        for s in &output.shapes {
            collect(&s.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        shapes
    }

    fn pattern(tool: DrawingToolType) -> Drawing {
        let mut drawing = Drawing::new(1, tool);
        drawing.points = [
            (100., 300.),
            (180., 100.),
            (260., 250.),
            (340., 150.),
            (420., 220.),
            (500., 180.),
            (580., 300.),
        ]
        .map(|(x, y)| Pos2::new(x, y))
        .to_vec();
        drawing.points.truncate(tool.required_points().unwrap_or(7));
        drawing.chart_points = drawing
            .points
            .iter()
            .map(|p| crate::drawings::ChartPoint::new(p.x, f64::from(p.y)))
            .collect();
        drawing
    }

    /// Text boxes take the theme's card, or a solid label in the drawing colour, unless their
    /// text, background and border settings say otherwise. Contrast of white on the red is
    /// 1.05 / 0.269 = 3.90 and on the yellow 1.05 / 0.860 = 1.22, so black reads on the yellow.
    #[test]
    fn text_boxes_follow_the_theme_and_their_settings() {
        use DrawingToolType::*;
        let red = Color32::from_rgb(242, 54, 69);
        let yellow = Color32::from_rgb(255, 235, 59);
        let blue = Color32::from_rgb(0, 0, 255);
        let green = Color32::from_rgb(0, 128, 0);
        for (theme, panel) in [
            (crate::theme::Theme::light(), Color32::WHITE),
            (crate::theme::Theme::dark(), Color32::BLACK),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            for tool in [
                TextLabel,
                AnchoredText,
                Note,
                AnchoredNote,
                FlagNote,
                Callout,
                Comment,
                PriceNote,
                Signpost,
            ] {
                let solid = matches!(tool, Comment | PriceNote | Signpost);
                let border = |color| vec![Stroke::new(1.0, color)];
                // Settings, text colour, box fills, box strokes and italics.
                type Case = (
                    &'static str,
                    fn(&mut Drawing),
                    Color32,
                    Vec<Color32>,
                    Vec<Stroke>,
                    bool,
                );
                let cases: [Case; 4] = [
                    (
                        "defaults",
                        |_| {},
                        if solid { Color32::WHITE } else { red },
                        vec![if solid { red } else { panel }],
                        border(red),
                        false,
                    ),
                    (
                        "all set",
                        |d| {
                            d.text_color = Some([0, 0, 255, 255]);
                            d.fill_color = Some([255, 235, 59, 255]);
                            d.border_color = Some([0, 128, 0, 255]);
                            d.italic = true;
                        },
                        blue,
                        vec![yellow],
                        border(green),
                        true,
                    ),
                    (
                        "fill only",
                        |d| d.fill_color = Some([255, 235, 59, 255]),
                        if solid { Color32::BLACK } else { red },
                        vec![yellow],
                        border(red),
                        false,
                    ),
                    (
                        "switched off",
                        |d| {
                            d.background = false;
                            d.border = false;
                        },
                        red,
                        vec![],
                        vec![],
                        false,
                    ),
                ];
                for (case, setup, text_color, fills, strokes, italics) in cases {
                    let mut drawing = pattern(tool);
                    // A red drawing with no text box settings of its own, whatever the tool's.
                    drawing.color = red.to_array();
                    (drawing.fill_color, drawing.text_color) = (None, None);
                    (drawing.background, drawing.border) = (true, true);
                    drawing.border_color = None;
                    drawing.points = vec![Pos2::new(100.0, 300.0), Pos2::new(200.0, 100.0)];
                    drawing.chart_points = (drawing.points.iter())
                        .map(|p| crate::drawings::ChartPoint::new(p.x, f64::from(p.y)))
                        .collect();
                    drawing.text = Some("Hi".into());
                    setup(&mut drawing);
                    let shapes = paint_on(&ctx, &drawing, true);
                    let texts: Vec<_> = (shapes.iter())
                        .filter_map(|s| match s {
                            Shape::Text(t) if t.galley.text().ends_with("Hi") => Some(t),
                            _ => None,
                        })
                        .collect();
                    let [text] = texts[..] else {
                        panic!("{tool:?} {case}: {} texts", texts.len());
                    };
                    let format = &text.galley.job.sections[0].format;
                    let label = text.visual_bounding_rect();
                    let (mut painted_fills, mut painted_strokes) = (vec![], vec![]);
                    for shape in &shapes {
                        let (bounds, fill, stroke) = match shape {
                            Shape::Rect(r) => (r.rect, r.fill, r.stroke),
                            Shape::Path(p) if p.closed => {
                                let ColorMode::Solid(color) = p.stroke.color else {
                                    continue;
                                };
                                (
                                    egui::Rect::from_points(&p.points),
                                    p.fill,
                                    Stroke::new(p.stroke.width, color),
                                )
                            }
                            _ => continue,
                        };
                        if bounds.expand(1.0).contains_rect(label) {
                            painted_fills.extend((fill.a() > 0).then_some(fill));
                            painted_strokes.extend((!stroke.is_empty()).then_some(stroke));
                        }
                    }
                    let name = format!("{tool:?} {case}, dark {}", theme.dark);
                    assert_eq!(format.color, text_color, "{name}");
                    assert_eq!(format.italics, italics, "{name}");
                    assert_eq!(painted_fills, fills, "{name}");
                    assert_eq!(painted_strokes, strokes, "{name}");
                }
            }
        }
    }

    /// Finished drawings leave swing points clear; small circles are reserved for previews,
    /// selection handles and the flag's pole cap. Wave label backgrounds remain larger circles.
    #[test]
    fn finished_drawings_leave_anchors_clear() {
        use DrawingToolType::*;
        let marker = |shape: &Shape| match shape {
            Shape::Circle(c) if c.radius <= 8.0 => Some((c.center, c.radius)),
            Shape::Path(p) if p.closed && p.points.len() >= 8 => {
                let bounds = egui::Rect::from_points(&p.points);
                (bounds.width() <= 16.01 && bounds.height() <= 16.01)
                    .then_some((bounds.center(), bounds.width() / 2.0))
            }
            _ => None,
        };
        for tool in [
            Ray,
            ExtendedLine,
            HorizontalRay,
            CrossLine,
            TrendAngle,
            PriceRange,
            DateRange,
            InfoLine,
            RegressionTrend,
            FlatTopBottom,
            DisjointChannel,
            Arc,
            Polyline,
            Path,
            Curve,
            Pitchfork,
            SchiffPitchfork,
            ModifiedSchiffPitchfork,
            InsidePitchfork,
            Pitchfan,
            GannFan,
            GannSquare,
            GannBox,
            GannFixed,
            FibonacciTimeZones,
            FibonacciChannel,
            FibonacciCircles,
            FibonacciSpeedFan,
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            ElliottDoubleCombo,
            ElliottTripleCombo,
            CyclicLines,
            TimeCycles,
            SineLine,
            AnchoredVWAP,
            FixedRangeVolumeProfile,
            AnchoredVolumeProfile,
            BarsPattern,
            GhostFeed,
            ProjectionTool,
            TextLabel,
            AnchoredText,
            Callout,
            FlagNote,
            Signpost,
        ] {
            let mut drawing = pattern(tool);
            drawing.completed = true;
            for shape in paint(&drawing) {
                if let Some((center, radius)) = marker(&shape) {
                    assert!(
                        tool == FlagNote
                            && center.distance(drawing.points[0]) < 0.01
                            && radius <= crate::tokens::DESIGN_TOKENS.rounding.sm,
                        "{tool:?} paints an anchor dot"
                    );
                }
            }
        }
        for (tool, count) in [
            (Ray, 1),
            (ExtendedLine, 1),
            (Arc, 1),
            (Arc, 2),
            (Polyline, 1),
            (Path, 1),
            (PriceRange, 1),
            (DateRange, 1),
            (Callout, 1),
            (Pitchfork, 1),
            (Pitchfork, 2),
            (SchiffPitchfork, 1),
            (ModifiedSchiffPitchfork, 1),
            (InsidePitchfork, 1),
            (XABCDPattern, 1),
            (ElliottImpulse, 1),
            (Curve, 2),
            (FibonacciChannel, 2),
            (BarsPattern, 2),
            (ProjectionTool, 2),
        ] {
            let mut drawing = pattern(tool);
            drawing.points.truncate(count);
            drawing.chart_points.truncate(count);
            assert!(
                paint(&drawing).iter().any(|s| marker(s).is_some()),
                "{tool:?} lost its {count}-anchor preview"
            );
        }
    }

    #[test]
    fn main_widths_are_exact_and_details_stay_thin() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            ElliottDoubleCombo,
            ElliottTripleCombo,
            CyclicLines,
            TimeCycles,
            SineLine,
            GannFan,
            GannBox,
            GannSquare,
            GannFixed,
            Brush,
            Highlighter,
        ] {
            for width in [1.0, 2.0, 3.0] {
                let mut drawing = pattern(tool);
                drawing.stroke_width = width;
                if matches!(tool, Brush | Highlighter) {
                    drawing.color = [41, 98, 255, 60];
                }
                let shapes = paint(&drawing);
                let mut main = 0;
                for shape in &shapes {
                    let (is_main, stroke) = match shape {
                        Shape::LineSegment { points, stroke } => (
                            drawing.points.windows(2).any(|p| p == points)
                                || tool == CyclicLines
                                || matches!(tool, GannFan | GannBox | GannSquare | GannFixed),
                            *stroke,
                        ),
                        Shape::Path(p) => {
                            let ColorMode::Solid(color) = p.stroke.color else {
                                continue;
                            };
                            (
                                p.points
                                    .windows(2)
                                    .any(|p| drawing.points.windows(2).any(|leg| leg == p))
                                    || (tool == SineLine && p.points.len() > 100)
                                    || matches!(tool, GannSquare | GannFixed)
                                    || (tool == TimeCycles
                                        && p.points.len() > 4
                                        && color == drawing.color32()),
                                Stroke::new(p.stroke.width, color),
                            )
                        }
                        Shape::Rect(r) => (false, r.stroke),
                        _ => continue,
                    };
                    if stroke.is_empty() {
                        continue;
                    }
                    if is_main {
                        main += 1;
                        assert_eq!(stroke.width, width, "{tool:?} main at {width}");
                        if matches!(tool, Brush | Highlighter) {
                            assert_eq!(stroke.color, drawing.color32(), "{tool:?} colour");
                        }
                    } else if stroke.color != Color32::WHITE
                        && (stroke.color.a() < 255
                            || matches!(shape, Shape::Path(p) if p.closed && p.points.len() >= 8))
                    {
                        assert!(
                            stroke.width <= 2.0,
                            "{tool:?} detail widened to {}",
                            stroke.width
                        );
                    }
                }
                assert!(main > 0, "{tool:?} missing main line");
            }
        }
    }

    #[test]
    fn pattern_fills_toggle_without_recolouring_labels() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            TrianglePattern,
        ] {
            let mut drawing = pattern(tool);
            assert!(
                drawing.fill_color.is_some(),
                "{tool:?} lost its default tint"
            );
            let coloured = Drawing::with_color(1, tool, [67, 89, 123, 255]);
            assert_eq!(
                &coloured.fill_color.unwrap()[..3],
                &[67, 89, 123],
                "{tool:?} default fill must follow its line colour"
            );
            let configured = Color32::from_rgba_unmultiplied(12, 34, 56, 80);
            for fill in [None, Some([12, 34, 56, 80])] {
                drawing.fill_color = fill;
                let shapes = paint(&drawing);
                let mut bodies = 0;
                for shape in &shapes {
                    match shape {
                        Shape::Path(p) if p.closed && p.points.len() <= 7 => {
                            bodies += usize::from(p.fill.a() > 0);
                            assert_eq!(
                                p.fill,
                                fill.map_or(Color32::TRANSPARENT, |_| configured),
                                "{tool:?} body"
                            );
                        }
                        Shape::Rect(r) if r.fill.a() > 0 => {
                            assert_ne!(r.fill, configured, "{tool:?} label recoloured");
                        }
                        _ => {}
                    }
                }
                assert_eq!(bodies > 0, fill.is_some(), "{tool:?} fill toggle");
            }
        }
    }

    /// Guards the circles of drawings at 1x and 2x: a circle with an outline, or too large for a
    /// textured quad, becomes a closed polygon whose corners lie on the circle and whose sides
    /// stay within a quarter of a physical pixel of it, with the outline still outside the
    /// radius; a small disc without an outline stays a circle.
    #[test]
    fn circles_become_polygons_within_a_quarter_pixel() {
        let center = Pos2::new(50.0, 40.0);
        let ring = Stroke::new(1.0, Color32::WHITE);
        for (radius, ppp, stroke, polygon) in [
            (3.0, 1.0, Stroke::NONE, false),
            (3.0, 2.0, Stroke::NONE, false),
            (3.0, 1.0, ring, true),
            (8.0, 1.0, Stroke::NONE, true),
            (8.0, 2.0, ring, true),
            (200.0, 2.0, ring, true),
        ] {
            let circle = egui::epaint::CircleShape {
                center,
                radius,
                fill: Color32::RED,
                stroke,
            };
            let mut shape = Shape::Circle(circle);
            fewer_circle_sides(&mut shape, ppp);
            let case = format!("radius {radius} at {ppp}x");
            let Shape::Path(path) = &shape else {
                assert!(!polygon, "{case}: still a circle");
                continue;
            };
            assert!(polygon, "{case}: a quad disc became a polygon");
            assert!(path.closed, "{case}");
            assert_eq!(
                (path.fill, path.stroke.kind),
                (Color32::RED, StrokeKind::Outside)
            );
            let corners = &path.points;
            for (i, a) in corners.iter().enumerate() {
                let b = corners[(i + 1) % corners.len()];
                assert!(((*a - center).length() - radius).abs() < 1e-3, "{case}");
                let error = (radius - (center.distance(a.lerp(b, 0.5)))) * ppp;
                assert!((0.0..=0.25).contains(&error), "{case}: {error} px");
            }
        }
    }

    /// Guards the frame cost of dashed drawings: a rect, a circle and an ellipse reaching a
    /// million points beyond an 800 x 600 pane are dashed only inside it, rather than with one
    /// shape per dash along their whole outline.
    #[test]
    fn far_reaching_dashed_outlines_are_dashed_only_inside_the_pane() {
        use crate::drawings::rendering::ChartView;
        use egui::{Context, RawInput};
        fn leaves(shape: &Shape) -> usize {
            match shape {
                Shape::Vec(shapes) => shapes.iter().map(leaves).sum(),
                _ => 1,
            }
        }
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (tool, a, b) in [
            (DrawingToolType::Rect, (-1e6, 100.0), (1e6, 200.0)),
            (
                DrawingToolType::Circle,
                (400.0, 1e6 + 300.0),
                (400.0, 300.0),
            ),
            (DrawingToolType::Ellipse, (-1e6, 300.0), (1e6, 2e6 + 300.0)),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.line_style = LineStyle::Dashed;
            drawing.points = vec![Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)];
            let ctx = Context::default();
            let input = RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                let view = ChartView {
                    price_formatter: None,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b,
                    price_to_y: &|p| p as f32,
                    y_to_price: &f64::from,
                    time_axis: true,
                    executions: None,
                };
                drawing.render_original(ui.painter(), pane, &view);
            });
            let shapes: usize = output.shapes.iter().map(|s| leaves(&s.shape)).sum();
            output.drop_without_applying_deltas();
            assert!((1..1_000).contains(&shapes), "{tool:?}: {shapes} shapes");
        }
    }

    /// Bold text paints a second copy 0.6 pt along its own baseline, so text turned along a
    /// horizontal, a 45 degree and a vertical line thickens its stems alike.
    #[test]
    fn bold_text_thickens_along_its_baseline() {
        for end in [(300.0, 300.0), (300.0, 100.0), (100.0, 100.0)] {
            let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
            drawing.points = vec![Pos2::new(100.0, 300.0), Pos2::new(end.0, end.1)];
            drawing.text = Some("Weekly support".into());
            drawing.font_weight = FontWeight::Bold;
            let texts: Vec<_> = paint(&drawing)
                .into_iter()
                .filter_map(|s| match s {
                    Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            let [text, copy] = texts.as_slice() else {
                panic!("{end:?}: {} texts", texts.len());
            };
            let offset = egui::emath::Rot2::from_angle(-text.angle) * (copy.pos - text.pos);
            assert!(
                (offset - egui::vec2(0.6, 0.0)).length() < 1e-4,
                "{end:?}: {offset:?}"
            );
        }
    }
}
