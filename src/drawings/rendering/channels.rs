//! Channel tool rendering implementations
//!
//! Includes: parallel channel, disjoint channel, regression trend, and flat top/bottom.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::math::{LinearFit, Moments};
use crate::model::{Bar, SpanCache};
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static MOMENTS: RefCell<SpanCache<Moments>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    pub(crate) fn render_channel(&self, painter: &egui::Painter, rect: Rect) {
        let Some(outline) = self.outline(rect) else {
            for pair in self.points.windows(2) {
                painter.line_segment(
                    [pair[0], pair[1]],
                    Stroke::new(self.stroke_width, self.color32()),
                );
            }
            return;
        };
        if let Some(fill) = self.fill_color {
            for area in outline.areas {
                painter.add(egui::Shape::convex_polygon(
                    area,
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]),
                    Stroke::NONE,
                ));
            }
        }
        for (i, [a, b]) in outline.lines.into_iter().enumerate() {
            if i == 2 && self.channel_midline {
                super::utils::draw_dashed_line(
                    painter,
                    a,
                    b,
                    Stroke::new(self.stroke_width, self.color32()),
                    8.0,
                    4.0,
                );
            } else {
                self.styled_segment(
                    painter,
                    a,
                    b,
                    Stroke::new(self.stroke_width, self.color32()),
                );
            }
        }
    }

    pub(super) fn parallel_channel_outline(&self, rect: Rect) -> Option<super::Outline> {
        let [c, d] = self.parallel_channel_second_line()?;
        let [a, b] = [self.points[0], self.points[1]];
        let extend = |a: Pos2, b: Pos2| {
            let [left, right] = super::lines::extended_line(a, b, rect);
            let forward = if (b.x - a.x).abs() < 1e-6 {
                b.y >= a.y
            } else {
                b.x >= a.x
            };
            let (a, b) = if forward { (a, b) } else { (b, a) };
            [
                if self.extend_left { left } else { a },
                if self.extend_right { right } else { b },
            ]
        };
        let (base, other) = (extend(a, b), extend(c, d));
        let mut outline = super::Outline {
            lines: vec![base, other],
            ..Default::default()
        };
        if self.channel_midline {
            outline
                .lines
                .push([base[0].lerp(other[0], 0.5), base[1].lerp(other[1], 0.5)]);
        }
        if self.fill_color.is_some() {
            outline
                .areas
                .push(vec![base[0], base[1], other[1], other[0]]);
        }
        Some(outline)
    }

    /// The channel's second line: the base line from the first to the second
    /// anchor, moved straight up or down in price through the third anchor, so
    /// the channel is as wide in price at every bar. A vertical base moves
    /// sideways instead. `None` below three anchors or when the first two coincide.
    pub(crate) fn parallel_channel_second_line(&self) -> Option<[Pos2; 2]> {
        let &[p1, p2, p3, ..] = self.points.as_slice() else {
            return None;
        };
        if (p2 - p1).length_sq() < 1e-6 {
            return None;
        }
        let offset = offset_onto(p1, p2, p3);
        Some([p1 + offset, p2 + offset])
    }

    /// Render Regression Trend - least-squares line through the closes of the
    /// bars between the anchors, with bands the configured number of standard
    /// deviations away
    pub(crate) fn render_regression_trend(&self, painter: &egui::Painter, view: &ChartView) {
        if let Some((first, len, fit)) = self.regression_fit(view) {
            self.draw_regression_channel(painter, view, first, len, &fit, self.color32());
        }
    }

    /// The first fitted bar, the number of bars and the least-squares fit of the anchored range,
    /// or `None` without bars or when the fit is not finite.
    fn regression_fit(&self, view: &ChartView) -> Option<(usize, usize, LinearFit)> {
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return None;
        };
        let (first, bars) = bars_between(view.bars, a.bar_idx, b.bar_idx)?;
        let fit = span_moments(view, first, bars).fit()?;
        (fit.intercept.is_finite()
            && fit.slope.is_finite()
            && fit.sigma.is_finite()
            && fit.r_squared.is_finite())
        .then_some((first, bars.len(), fit))
    }

    /// Fitted endpoints in anchor order, before extension, for selection and hit testing.
    pub(crate) fn regression_ends(&self, view: &ChartView) -> Option<[Pos2; 2]> {
        let (first, len, fit) = self.regression_fit(view)?;
        let mut ends = [0, len - 1].map(|i| {
            Pos2::new(
                (view.bar_to_x)((first + i) as f32),
                (view.price_to_y)(fit.intercept + fit.slope * i as f64),
            )
        });
        if self.chart_points[0].bar_idx > self.chart_points[1].bar_idx {
            ends.reverse();
        }
        ends.iter().all(|p| p.is_finite()).then_some(ends)
    }

    fn draw_regression_channel(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        first: usize,
        len: usize,
        fit: &LinearFit,
        color: Color32,
    ) {
        let last = len - 1;
        let deviations = self.regression_trend.unwrap_or_default().deviations();
        // The fitted line shifted by `k` standard deviations, from the first bar to the last
        let line = |k: f64| {
            [0, last].map(|i| {
                let price = fit.intercept + fit.slope * i as f64 + k * fit.sigma;
                Pos2::new(
                    (view.bar_to_x)((first + i) as f32),
                    (view.price_to_y)(price),
                )
            })
        };
        let [upper, center, lower] = [deviations, 0.0, -deviations].map(line);

        let label_pos = lower[0] + egui::vec2(4.0, 4.0);
        let extend = |[a, b]: [Pos2; 2]| {
            let at = |x: f32| {
                let dx = b.x - a.x;
                Pos2::new(
                    x,
                    if dx.abs() < 1e-6 {
                        a.y
                    } else {
                        a.y + (b.y - a.y) * ((x - a.x) / dx)
                    },
                )
            };
            [
                if self.extend_left {
                    at(painter.clip_rect().left())
                } else {
                    a
                },
                if self.extend_right {
                    at(painter.clip_rect().right())
                } else {
                    b
                },
            ]
        };
        let [upper, center, lower] = [upper, center, lower].map(extend);
        let band_color = DESIGN_TOKENS.semantic.brand.accent;
        if let Some(fill) = self.fill_color {
            for (a, b, rgb) in [
                (upper, center, [fill[0], fill[1], fill[2]]),
                (center, lower, [self.color[0], self.color[1], self.color[2]]),
            ] {
                painter.add(egui::Shape::convex_polygon(
                    vec![a[0], a[1], b[1], b[0]],
                    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], fill[3]),
                    Stroke::NONE,
                ));
            }
        }
        for band in [upper, lower] {
            painter.line_segment(band, Stroke::new(self.stroke_width, band_color));
        }
        super::utils::draw_dashed_line(
            painter,
            center[0],
            center[1],
            Stroke::new(self.stroke_width, color),
            8.0,
            4.0,
        );
        painter.text(
            label_pos,
            egui::Align2::LEFT_TOP,
            fit.correlation
                .map_or_else(|| "n/a".to_owned(), |r| format!("{r:.2}")),
            egui::FontId::proportional(typography::XS),
            band_color,
        );
    }

    /// Unextended endpoints in anchor order, shared with selection handles.
    pub(crate) fn channel_second_line(&self) -> Option<[Pos2; 2]> {
        let &[a, b, c, ..] = self.points.as_slice() else {
            return None;
        };
        match self.tool_type {
            DrawingToolType::ParallelChannel => self.parallel_channel_second_line(),
            DrawingToolType::FlatTopBottom => Some(flat_line(a, b, c)),
            DrawingToolType::DisjointChannel => Some(disjoint_second_line(a, b, c)),
            _ => None,
        }
    }

    /// The flat or mirrored channel, split at a crossing into convex fill areas.
    pub(super) fn wedge_channel_outline(&self, rect: Rect) -> Option<super::Outline> {
        let other = self.channel_second_line()?;
        let extend = |mut line: [Pos2; 2]| {
            if line[0].x > line[1].x {
                line.reverse();
            }
            let [a, b] = line;
            let dx = b.x - a.x;
            if dx.abs() < 1e-6 {
                return line;
            }
            let at = |x| Pos2::new(x, a.y + (x - a.x) * (b.y - a.y) / dx);
            [
                if self.extend_left {
                    at(rect.left().min(a.x))
                } else {
                    a
                },
                if self.extend_right {
                    at(rect.right().max(b.x))
                } else {
                    b
                },
            ]
        };
        let [a, b] = extend([self.points[0], self.points[1]]);
        let [c, d] = extend(other);
        let mut outline = super::Outline {
            lines: vec![[a, b], [c, d]],
            ..Default::default()
        };
        if self.fill_color.is_some() {
            let (start_gap, end_gap) = (c.y - a.y, d.y - b.y);
            if start_gap * end_gap < 0.0 {
                let crossing = a.lerp(b, start_gap / (start_gap - end_gap));
                outline.areas = vec![vec![a, crossing, c], vec![crossing, b, d]];
            } else {
                outline.areas.push(vec![a, b, d, c]);
            }
        }
        Some(outline)
    }
}

/// Moments of `bars`, the span that starts at bar `first`. A tick on the
/// forming bar, or a new bar, adds one bar to the moments of the bars before
/// it instead of refitting the span.
fn span_moments(view: &ChartView, first: usize, bars: &[Bar]) -> Rc<Moments> {
    MOMENTS.with_borrow_mut(|cache| {
        view.cached(cache, (), first, bars, Moments::of, |moments, bar| {
            Some(moments.with(bar.close))
        })
    })
}

/// The horizontal boundary over the base line's bars, in anchor order.
fn flat_line(p1: Pos2, p2: Pos2, p3: Pos2) -> [Pos2; 2] {
    [p1.x, p2.x].map(|x| Pos2::new(x, p3.y))
}

/// Reflect the base slope through the third anchor over the same bars. A vertical
/// base reflects about the third anchor's height, since it has no finite slope.
fn disjoint_second_line(p1: Pos2, p2: Pos2, p3: Pos2) -> [Pos2; 2] {
    let d = p2 - p1;
    if d.x.abs() < 1e-6 {
        let axis = (p1.y + p2.y) / 2.0;
        return [p1, p2].map(|p| Pos2::new(p.x, p3.y + axis - p.y));
    }
    [p1.x, p2.x].map(|x| Pos2::new(x, p3.y - (x - p3.x) * d.y / d.x))
}

/// The move that carries the line through `p1` and `p2` onto `p3`: straight up
/// or down, so the moved line is as far in price at every bar, or sideways for
/// a vertical line.
pub(super) fn offset_onto(p1: Pos2, p2: Pos2, p3: Pos2) -> egui::Vec2 {
    let d = p2 - p1;
    if d.x.abs() < 1e-3 {
        egui::vec2(p3.x - p1.x, 0.0)
    } else {
        egui::vec2(0.0, p3.y - (p1.y + (p3.x - p1.x) * d.y / d.x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use crate::drawings::rendering::sample_bars;

    /// Reflect y = 100 - x about y = 85: the other line is y = 70 + x,
    /// through (50, 120), with the same bars in either anchor order.
    #[test]
    fn disjoint_second_line_mirrors_the_slope_through_the_third_anchor() {
        for (a, b, c, expected) in [
            (
                (10.0, 90.0),
                (90.0, 10.0),
                (50.0, 120.0),
                [(10.0, 80.0), (90.0, 160.0)],
            ),
            (
                (90.0, 10.0),
                (10.0, 90.0),
                (150.0, 220.0),
                [(90.0, 160.0), (10.0, 80.0)],
            ),
            (
                (10.0, 90.0),
                (90.0, 90.0),
                (50.0, 120.0),
                [(10.0, 120.0), (90.0, 120.0)],
            ),
            (
                (10.0, 90.0),
                (10.0, 10.0),
                (50.0, 120.0),
                [(10.0, 80.0), (10.0, 160.0)],
            ),
        ] {
            let p = |(x, y)| Pos2::new(x, y);
            assert_eq!(disjoint_second_line(p(a), p(b), p(c)), expected.map(p));
        }
    }

    /// The second line keeps the base line's slope and passes through the third
    /// anchor straight above or below it, so the channel's price width is the
    /// same at every bar. A vertical base moves sideways onto the third anchor.
    #[test]
    fn parallel_channel_second_line_passes_through_the_third_anchor_vertically() {
        let cases = [
            // Base y = 100 - x; at x = 0 the third anchor is 50 px below it.
            (
                [(0.0, 100.0), (100.0, 0.0), (0.0, 150.0)],
                [(0.0, 150.0), (100.0, 50.0)],
            ),
            // Base y = 100 - x; at x = 50 the base is at 50, the third anchor 30 px above.
            (
                [(0.0, 100.0), (100.0, 0.0), (50.0, 20.0)],
                [(0.0, 70.0), (100.0, -30.0)],
            ),
            (
                [(50.0, 0.0), (50.0, 100.0), (80.0, 30.0)],
                [(80.0, 0.0), (80.0, 100.0)],
            ),
        ];
        for (anchors, want) in cases {
            let mut drawing = Drawing::new(1, DrawingToolType::ParallelChannel);
            drawing.points = anchors.iter().map(|&(x, y)| Pos2::new(x, y)).collect();
            let got = drawing.parallel_channel_second_line().unwrap();
            let want = want.map(|(x, y)| Pos2::new(x, y));
            assert!(
                got.iter().zip(want).all(|(g, w)| g.distance(w) < 1e-3),
                "anchors {anchors:?}: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn parallel_channel_options_share_rendered_and_selected_geometry() {
        let mut drawing = Drawing::new(1, crate::drawings::DrawingToolType::ParallelChannel);
        drawing.points = vec![
            Pos2::new(10.0, 100.0),
            Pos2::new(30.0, 80.0),
            Pos2::new(20.0, 120.0),
        ];
        drawing.extend_left = true;
        drawing.extend_right = true;
        drawing.fill_color = Some([33, 150, 243, 30]);
        let mut saved = serde_json::to_value(&drawing).unwrap();
        saved["channel_midline"] = true.into();
        let mut restored: Drawing = serde_json::from_value(saved).unwrap();
        restored.points = drawing.points;
        restored.id = 1;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 200.0));
        let outline = restored.outline(rect).unwrap();
        assert_eq!(outline.lines.len(), 3);
        for line in [
            [Pos2::new(0.0, 110.0), Pos2::new(100.0, 10.0)],
            [Pos2::new(0.0, 140.0), Pos2::new(100.0, 40.0)],
            [Pos2::new(0.0, 125.0), Pos2::new(100.0, 25.0)],
        ] {
            assert!(outline.lines.contains(&line), "missing {line:?}");
        }
        assert_eq!(outline.areas.len(), 1);
        let hit = crate::drawings::services::DrawingInteraction::new();
        assert_eq!(
            hit.hit_test(Pos2::new(80.0, 45.0), &[restored.clone()], "1m"),
            Some(1)
        );
        // Left and right are chart directions, independent of anchor order.
        for reverse in [false, true] {
            if reverse {
                restored.points.swap(0, 1);
            }
            for (left, right, xs) in [
                (true, false, [0.0, 30.0]),
                (false, true, [10.0, 100.0]),
                (false, false, [10.0, 30.0]),
            ] {
                restored.extend_left = left;
                restored.extend_right = right;
                for [a, b] in restored.outline(rect).unwrap().lines {
                    assert_eq!([a.x, b.x], xs);
                }
            }
        }
        restored.fill_color = None;
        assert!(restored.outline(rect).unwrap().areas.is_empty());
    }

    /// Reference: ordinary least squares of the closes of bars 0..=5 against
    /// 0..=5, residual standard deviation over n - 1, R² = 1 - SSres / SStot,
    /// Pearson r = Sxy / sqrt(Sxx Syy). The moments give the same fit computed
    /// over all bars at once, streamed bar by bar, or with the last bar added to the others.
    #[test]
    fn linear_fit_matches_reference() {
        let bars = &sample_bars()[..6];
        let streamed = bars
            .iter()
            .fold(Moments::default(), |moments, bar| moments.with(bar.close));
        let extended = Moments::of(&bars[..5]).with(bars[5].close);
        for moments in [Moments::of(bars), streamed, extended] {
            let fit = moments.fit().unwrap();
            for (got, want) in [
                (fit.slope, 0.45714285714285713),
                (fit.intercept, 11.190476190476192),
                (fit.sigma, 0.9139136147569393),
                (fit.r_squared, 0.4668693009118541),
                (fit.correlation.unwrap(), 0.6832783480484758),
            ] {
                assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
            }
        }
        // Flat closes whose mean does not round to the close are fitted exactly.
        let flat = Moments::of(&vec![
            Bar {
                close: 0.1,
                ..bars[0].clone()
            };
            100
        ])
        .fit()
        .unwrap();
        assert_eq!((flat.r_squared, flat.correlation), (1.0, None));
    }

    /// Closes 10, 14, 12 have Sxx = 2, Sxy = 2, Syy = 8: y = 11 + x,
    /// residual sigma = sqrt(3), r = 1/2. Each half ends on that same centre line.
    #[test]
    fn regression_channel_respects_fill_extension_and_line_roles() {
        use crate::drawings::{ChartPoint, RegressionTrendConfig};
        use egui::{Context, RawInput, Shape};
        let bars: Vec<_> = [10.0, 14.0, 12.0]
            .into_iter()
            .map(|close| Bar {
                close,
                ..sample_bars()[0].clone()
            })
            .collect();
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0));
        for reverse in [false, true] {
            for (left, right, xs) in [
                (false, false, [100.0, 300.0]),
                (false, true, [100.0, 500.0]),
                (true, false, [0.0, 300.0]),
            ] {
                for filled in [false, true] {
                    let mut drawing = Drawing::new(1, DrawingToolType::RegressionTrend);
                    drawing.chart_points =
                        vec![ChartPoint::new(0.0, 30.0), ChartPoint::new(2.0, 5.0)];
                    if reverse {
                        drawing.chart_points.reverse();
                    }
                    drawing
                        .update_screen_coords(|b| 100.0 + b * 100.0, |p| 500.0 - p as f32 * 10.0);
                    drawing.regression_trend = RegressionTrendConfig::new(1.0);
                    drawing.stroke_width = 3.0;
                    drawing.line_style = crate::model::LineStyle::Dotted;
                    drawing.fill_color = filled.then_some([41, 98, 255, 51]);
                    drawing.extend_left = left;
                    drawing.extend_right = right;
                    let ctx = Context::default();
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(pane),
                            ..Default::default()
                        },
                        |ui| {
                            drawing.render(
                                ui.painter(),
                                pane,
                                &ChartView {
                                    instrument: None,
                                    bars: &bars,
                                    revision: 0,
                                    bar_to_x: &|b| 100.0 + b * 100.0,
                                    price_to_y: &|p| 500.0 - p as f32 * 10.0,
                                    y_to_price: &|y| f64::from((500.0 - y) / 10.0),
                                    time_axis: true,
                                    executions: None,
                                },
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
                    for shape in &output.shapes {
                        collect(&shape.shape, &mut shapes);
                    }
                    output.drop_without_applying_deltas();
                    let fills: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Path(p) if p.fill.a() > 0 => Some(p),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(fills.len(), if filled { 2 } else { 0 });
                    let center_y = |x: f32| 400.0 - x / 10.0;
                    if filled {
                        for (color, offset) in [
                            (
                                Color32::from_rgba_unmultiplied(41, 98, 255, 51),
                                -10.0 * 3f32.sqrt(),
                            ),
                            (
                                Color32::from_rgba_unmultiplied(242, 54, 69, 51),
                                10.0 * 3f32.sqrt(),
                            ),
                        ] {
                            let fill = fills
                                .iter()
                                .find(|p| p.fill == color)
                                .expect("half fill colour");
                            for expected in [
                                Pos2::new(xs[0], center_y(xs[0])),
                                Pos2::new(xs[1], center_y(xs[1])),
                                Pos2::new(xs[0], center_y(xs[0]) + offset),
                                Pos2::new(xs[1], center_y(xs[1]) + offset),
                            ] {
                                assert!(
                                    fill.points.iter().any(|p| p.distance(expected) < 1e-3),
                                    "missing {expected:?}"
                                );
                            }
                        }
                    }
                    let lines: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match s {
                            Shape::LineSegment { points, stroke } => Some((points, stroke)),
                            _ => None,
                        })
                        .collect();
                    for offset in [-10.0 * 3f32.sqrt(), 10.0 * 3f32.sqrt()] {
                        assert!(lines.iter().any(|(p, s)| s.width == 3.0
                            && s.color == Color32::from_rgb(41, 98, 255)
                            && p[0].distance(Pos2::new(xs[0], center_y(xs[0]) + offset)) < 1e-3
                            && p[1].distance(Pos2::new(xs[1], center_y(xs[1]) + offset)) < 1e-3));
                    }
                    let center: Vec<_> = lines
                        .iter()
                        .filter(|(_, s)| s.color == drawing.color32())
                        .collect();
                    assert!(center.len() > 1, "the centre line is dashed");
                    let mut points = Vec::new();
                    for (p, s) in center {
                        assert_eq!(s.width, 3.0);
                        assert!(p[0].distance(p[1]) < (xs[1] - xs[0]) / 2.0);
                        for point in p.iter() {
                            assert!((point.y - center_y(point.x)).abs() < 1e-3);
                            points.push(*point);
                        }
                    }
                    let bounds = Rect::from_points(&points);
                    let tolerance = (xs[1] - xs[0]) * 0.1;
                    assert!(
                        bounds.left() <= xs[0] + tolerance && bounds.right() >= xs[1] - tolerance
                    );
                    let text: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Text(t) => Some(t),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(text.len(), 1);
                    assert_eq!(text[0].galley.job.text, "0.50");
                    assert!((text[0].pos.x - 100.0).abs() < 15.0);
                    assert!(
                        !shapes
                            .iter()
                            .any(|s| matches!(s, Shape::Circle(_) | Shape::Rect(_)))
                    );
                }
            }
        }
    }
}
