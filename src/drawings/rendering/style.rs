//! Common drawing styles applied to renderer primitives.

use crate::drawings::{ArrowStyle, Drawing, DrawingToolType, FontWeight, LineStyle};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, epaint::ColorMode, layers::ShapeIdx};

impl Drawing {
    /// Size annotations before laying out both the text and its surrounding box.
    pub(super) fn annotation_font(&self) -> egui::FontId {
        egui::FontId::proportional(self.font_size)
    }

    /// Apply common styles to this drawing's paint range. Defaults retain the renderer's
    /// hierarchy; custom fork levels and semantic position colours take precedence.
    pub(super) fn finish_style(&self, painter: &Painter, start: usize, rect: Rect) {
        if self.line_style != LineStyle::Solid
            || self.stroke_width != 2.0
            || self.fill_color.is_some()
            || self.font_weight == FontWeight::Bold
            || self.extend_left
            || self.extend_right
        {
            painter.ctx().graphics_mut(|g| {
                let list = g.entry(painter.layer_id());
                let mut previous: Option<(ShapeIdx, Pos2, Stroke)> = None;
                for index in start..list.next_idx().0 {
                    let mut segment = None;
                    list.mutate_shape(ShapeIdx(index), |s| {
                        if matches!(
                            self.tool_type,
                            DrawingToolType::InfoLine
                                | DrawingToolType::TrendAngle
                                | DrawingToolType::Arrow
                                | DrawingToolType::ArrowMarker
                        ) && let Shape::LineSegment { points, .. } = &mut s.shape
                            && self.points.get(..2) == Some(points.as_slice())
                            && let Some((a, b)) = self.trend_line_ends(rect.min.x, rect.max.x)
                        {
                            *points = [a, b];
                        }
                        if let Shape::LineSegment { points, stroke } = s.shape {
                            segment = Some((points, stroke));
                        }
                    });
                    if self.line_style != LineStyle::Solid {
                        if let Some((points, stroke)) = segment {
                            if let Some((first, end, old_stroke)) = previous
                                && end == points[0]
                                && old_stroke == stroke
                            {
                                list.mutate_shape(first, |s| match &mut s.shape {
                                    Shape::LineSegment { points: old, .. } => {
                                        s.shape =
                                            Shape::line(vec![old[0], old[1], points[1]], stroke)
                                    }
                                    Shape::Path(p) => p.points.push(points[1]),
                                    _ => {}
                                });
                                list.reset_shape(ShapeIdx(index));
                                previous = Some((first, points[1], stroke));
                            } else {
                                previous = Some((ShapeIdx(index), points[1], stroke));
                            }
                        } else {
                            previous = None;
                        }
                    }
                }
                for index in start..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(index), |s| self.style_shape(&mut s.shape));
                }
            });
        }
        self.render_end_markers(painter, rect);
    }

    fn style_shape(&self, shape: &mut Shape) {
        if let Shape::Vec(shapes) = shape {
            for s in shapes {
                self.style_shape(s);
            }
            return;
        }
        if let Shape::Text(t) = shape {
            if self.font_weight == FontWeight::Bold {
                // Synthetic weight keeps host font families and fallback glyphs intact.
                let mut weighted = t.clone();
                weighted.pos.x += 0.6;
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
        );
        let custom_fork = fork && self.pitchfork.is_some();
        let width = |stroke: &mut Stroke| {
            if !stroke.is_empty()
                && self.stroke_width != 2.0
                && !fork
                && !matches!(
                    self.tool_type,
                    DrawingToolType::Brush | DrawingToolType::Highlighter
                )
            {
                stroke.width = self.stroke_width;
            }
        };
        let fill = |color: &mut Color32| {
            if color.a() > 0
                && !custom_fork
                && !matches!(
                    self.tool_type,
                    DrawingToolType::LongPos | DrawingToolType::ShortPos
                )
                && let Some(configured) = self.fill_color32()
            {
                *color = configured;
            }
        };
        let dash = if custom_fork {
            None
        } else {
            self.line_style.dash_pattern()
        };
        let mut path = Vec::new();
        let stroke = match shape {
            Shape::LineSegment { points, stroke } => {
                width(stroke);
                path.extend(*points);
                *stroke
            }
            Shape::Path(p) => {
                fill(&mut p.fill);
                let ColorMode::Solid(color) = &p.stroke.color else {
                    return;
                };
                let mut stroke = Stroke::new(p.stroke.width, *color);
                width(&mut stroke);
                p.stroke.width = stroke.width;
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
                width(&mut r.stroke);
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
                width(&mut c.stroke);
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
                width(&mut e.stroke);
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
            let mut shapes = Shape::dashed_line(&path, stroke, dash, gap);
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
            T::TrendLine | T::InfoLine | T::TrendAngle => self
                .trend_line_ends(rect.min.x, rect.max.x)
                .map(|(a, b)| [a, b]),
            T::Ray => Some([a, super::lines::ray_end(a, b, rect)]),
            T::ExtendedLine => Some(super::lines::extended_line(a, b, rect)),
            T::Arrow | T::ArrowMarker => self
                .trend_line_ends(rect.min.x, rect.max.x)
                .map(|(a, b)| [a, b]),
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
