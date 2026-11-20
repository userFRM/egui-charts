//! Channel tool rendering implementations
//!
//! Includes: parallel channel, disjoint channel, regression trend, and flat top/bottom.

use crate::drawings::domain::{Drawing, DrawingToolType, NoteTextStyle};
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::math::{LinearFit, Moments};
use crate::model::{Bar, PriceSource};
use crate::painting::span_cache::SpanCache;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static MOMENTS: RefCell<SpanCache<Moments, PriceSource>> =
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
        // A parallel channel's middle line, the third line, is painted after the drawing's style.
        for [a, b] in outline.lines.into_iter().take(2) {
            self.styled_segment(
                painter,
                a,
                b,
                Stroke::new(self.stroke_width, self.color32()),
            );
        }
        self.render_placed_line_text(painter, self.points[0], self.points[1]);
    }

    /// Paints a parallel channel's middle line, when shown, in its own colour, width and pattern,
    /// after the drawing's own style so that the channel's pattern leaves it alone.
    pub(super) fn render_channel_middle_line(&self, painter: &egui::Painter, rect: Rect) {
        if !self.channel_midline {
            return;
        }
        let Some(&[a, b]) = self
            .parallel_channel_outline(rect)
            .as_ref()
            .and_then(|outline| outline.lines.get(2))
        else {
            return;
        };
        let style = self.middle_line;
        let color = style.color.map_or(self.color32(), |[r, g, b, a]| {
            Color32::from_rgba_unmultiplied(r, g, b, a)
        });
        patterned_segment(
            painter,
            [a, b],
            Stroke::new(style.width, color),
            style.line_style,
        );
    }

    /// The style of a flat top/bottom's or disjoint channel's prices: their own, else the line
    /// colour at 12 points, regular and upright.
    pub(crate) fn anchor_price_style_or_default(&self) -> NoteTextStyle {
        self.anchor_price_style
            .unwrap_or(NoteTextStyle::new(self.color, typography::SM_MD))
    }

    /// Writes the price at each end of both lines of a flat top/bottom or disjoint channel beside
    /// it, in the chart's price format and [`Self::anchor_price_style_or_default`]: the anchors'
    /// prices on the base line, the third anchor's on a flat line and the price under each end of
    /// a mirrored line. Painted after the drawing's own style, so the text's bold leaves them alone.
    pub(super) fn render_channel_prices(&self, painter: &egui::Painter, view: &ChartView) {
        let (&[a, b, ..], &[pa, pb, pc, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };
        let Some([c, d]) = self.channel_second_line() else {
            return;
        };
        let far = |p: Pos2| match self.tool_type {
            DrawingToolType::FlatTopBottom => pc.price,
            _ => (view.y_to_price)(p.y),
        };
        let style = self.anchor_price_style_or_default();
        let [red, green, blue, alpha] = style.color;
        let format = egui::TextFormat {
            italics: style.italic,
            ..egui::TextFormat::simple(
                egui::FontId::proportional(typography::font_size(style.font_size)),
                Color32::from_rgba_unmultiplied(red, green, blue, alpha),
            )
        };
        let gap = DESIGN_TOKENS.spacing.sm + self.stroke_width / 2.0;
        for (point, other, price) in [
            (a, b, pa.price),
            (b, a, pb.price),
            (c, d, far(c)),
            (d, c, far(d)),
        ] {
            let Some(text) = super::annotations::price_text(price, view) else {
                continue;
            };
            if !point.is_finite() {
                continue;
            }
            let (align, away) = super::measurements::beside_anchor(point, other, gap);
            let galley =
                painter.layout_job(egui::text::LayoutJob::simple_format(text, format.clone()));
            let min = align.anchor_size(point + away, galley.size()).min;
            painter.galley(min, galley.clone(), format.color);
            if style.bold {
                // The same synthetic weight as the drawing's own bold text.
                painter.galley(
                    min + egui::vec2(super::style::BOLD_OFFSET, 0.0),
                    galley,
                    format.color,
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

    /// Render Regression Trend - least-squares line through the source prices of
    /// the bars between the anchors, with each band the configured number of
    /// standard deviations away or through that side's extreme price. Returns the correlation
    /// label, which the caller paints after the drawing's style.
    pub(crate) fn render_regression_trend(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) -> Option<egui::Shape> {
        self.regression_fit(view).and_then(|(first, len, fit)| {
            self.draw_regression_channel(painter, view, first, len, &fit, self.color32())
        })
    }

    /// The first fitted bar, the number of bars and the least-squares fit of the anchored range,
    /// or `None` without bars or when the fit is not finite.
    fn regression_fit(&self, view: &ChartView) -> Option<(usize, usize, LinearFit)> {
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return None;
        };
        let (first, bars) = bars_between(view.bars, a.bar_idx, b.bar_idx)?;
        let source = self.regression_trend.unwrap_or_default().source;
        let fit = span_moments(view, source, first, bars).fit()?;
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
    ) -> Option<egui::Shape> {
        let last = len - 1;
        let config = self.regression_trend.unwrap_or_default();
        let [above, below] = fit.bounds(&view.bars[first..first + len], config.band_deviations());
        // The fitted line shifted by `offset` in price, from the first bar to the last
        let line = |offset: f64| {
            [0, last].map(|i| {
                let price = fit.intercept + fit.slope * i as f64 + offset;
                Pos2::new(
                    (view.bar_to_x)((first + i) as f32),
                    (view.price_to_y)(price),
                )
            })
        };
        let [upper, center, lower] = [above, 0.0, -below].map(line);
        let fitted = center;

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
        for (band, line) in [(upper, config.upper_line), (lower, config.lower_line)] {
            if line.visible {
                let [r, g, b, a] = line.color;
                let width = line.width().unwrap_or(self.stroke_width);
                patterned_segment(
                    painter,
                    band,
                    Stroke::new(width, Color32::from_rgba_unmultiplied(r, g, b, a)),
                    line.line_style,
                );
            }
        }
        if config.show_base_line {
            patterned_segment(
                painter,
                center,
                Stroke::new(self.stroke_width, color),
                config.line_style,
            );
        }
        self.render_placed_line_text(painter, fitted[0], fitted[1]);
        config.show_correlation.then(|| {
            let places = usize::from(config.precision().unwrap_or(2));
            let text = fit.correlation.map_or_else(
                || crate::localization::text(painter.ctx(), "n/a"),
                |r| format!("{r:.places$}"),
            );
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(typography::XS),
                band_color,
            );
            egui::Shape::galley(label_pos, galley, band_color)
        })
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

/// Moments of the `source` prices of `bars`, the span that starts at bar `first`. A tick on
/// the forming bar, or a new bar, adds one bar to the moments of the bars before it instead
/// of refitting the span.
fn span_moments(view: &ChartView, source: PriceSource, first: usize, bars: &[Bar]) -> Rc<Moments> {
    MOMENTS.with_borrow_mut(|cache| {
        view.cached(
            cache,
            source,
            first,
            bars,
            |bars| Moments::of(bars, source),
            |moments, bar| {
                Some(moments.with(source.compute(bar.open, bar.high, bar.low, bar.close)))
            },
        )
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

/// Draws `line` with `stroke` in `style`.
pub(super) fn patterned_segment(
    painter: &egui::Painter,
    line: [Pos2; 2],
    stroke: Stroke,
    style: crate::model::LineStyle,
) {
    match style.dash_pattern() {
        None => {
            painter.line_segment(line, stroke);
        }
        Some((dash, gap)) => {
            super::utils::draw_dashed_line(painter, line[0], line[1], stroke, dash, gap)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use crate::drawings::rendering::sample_bars;

    fn collect(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => out.push(shape.clone()),
        }
    }

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
        assert_eq!(
            crate::drawings::services::hit_test(Pos2::new(80.0, 45.0), &[restored.clone()], "1m"),
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

    /// Flattened shapes of `drawing` painted in the shared 500 x 400 test pane, where a price is
    /// 500 - y.
    fn painted(drawing: &Drawing) -> Vec<egui::Shape> {
        let mut shapes = Vec::new();
        for shape in crate::drawings::rendering::tests::paint(drawing) {
            collect(&shape, &mut shapes);
        }
        shapes
    }

    /// The middle line is drawn in its own colour, width and pattern, by default a 1 px line
    /// dashed 8 on 4 in the line colour, whatever width and pattern the channel's two lines take.
    /// Its dashes start at the first anchor's side, so on the 200 px line from x = 100 they run
    /// from 100 + 12 i, or 100 + 24 i for large dashes, up to the end at 300.
    #[test]
    fn parallel_channel_middle_line_takes_its_own_style() {
        use crate::drawings::MiddleLineStyle;
        use crate::model::LineStyle;
        let channel = Drawing::new(1, DrawingToolType::ParallelChannel);
        // Saved before the middle line had settings of its own.
        let mut saved = serde_json::to_value(&channel).unwrap();
        saved.as_object_mut().unwrap().remove("middle_line");
        let saved: Drawing = serde_json::from_value(saved).unwrap();
        let set = MiddleLineStyle {
            color: Some([10, 20, 30, 128]),
            width: 2.5,
            line_style: LineStyle::Solid,
        };
        let large = MiddleLineStyle {
            color: None,
            width: 2.0,
            line_style: LineStyle::LargeDashed,
        };
        let dashes = |period: f32, dash: f32, count: usize| -> Vec<[f32; 2]> {
            (0..count)
                .map(|i| {
                    let start = 100.0 + period * i as f32;
                    [start, (start + dash).min(300.0)]
                })
                .collect()
        };
        let line = Color32::from_rgba_unmultiplied(255, 0, 0, 255);
        for (row, mut drawing, line_style, middle, stroke, spans) in [
            (
                "saved before",
                saved.clone(),
                LineStyle::Solid,
                None,
                Stroke::new(1.0, line),
                dashes(12.0, 8.0, 17),
            ),
            (
                "channel dotted",
                saved,
                LineStyle::Dotted,
                None,
                Stroke::new(1.0, line),
                dashes(12.0, 8.0, 17),
            ),
            (
                "solid, set",
                channel.clone(),
                LineStyle::Solid,
                Some(set),
                Stroke::new(2.5, Color32::from_rgba_unmultiplied(10, 20, 30, 128)),
                vec![[100.0, 300.0]],
            ),
            (
                "large dashes",
                channel,
                LineStyle::Solid,
                Some(large),
                Stroke::new(2.0, line),
                dashes(24.0, 16.0, 9),
            ),
        ] {
            drawing.points = vec![
                Pos2::new(100.0, 100.0),
                Pos2::new(300.0, 100.0),
                Pos2::new(100.0, 200.0),
            ];
            drawing.color = [255, 0, 0, 255];
            drawing.stroke_width = 3.0;
            drawing.fill_color = None;
            drawing.theme_shade = false;
            drawing.line_style = line_style;
            if let Some(middle) = middle {
                drawing.middle_line = middle;
            }
            let shapes = painted(&drawing);
            let on_line = |y: f32| -> Vec<_> {
                shapes
                    .iter()
                    .filter_map(|s| match s {
                        egui::Shape::LineSegment { points, stroke }
                            if points.iter().all(|p| p.y == y) =>
                        {
                            Some((*stroke, [points[0].x, points[1].x]))
                        }
                        _ => None,
                    })
                    .collect()
            };
            if line_style == LineStyle::Solid {
                for y in [100.0, 200.0] {
                    let strokes: Vec<_> = on_line(y).into_iter().map(|(s, _)| s).collect();
                    assert_eq!(strokes, [Stroke::new(3.0, line)], "{row}: line at {y}");
                }
            }
            let middle = on_line(150.0);
            assert_eq!(middle.len(), spans.len(), "{row}: {middle:?}");
            for ((got, [a, b]), want) in middle.into_iter().zip(spans) {
                assert_eq!(got, stroke, "{row}");
                assert!(
                    (a - want[0]).abs() < 1e-3 && (b - want[1]).abs() < 1e-3,
                    "{row}: {a}..{b}, want {want:?}"
                );
            }
        }
    }

    /// The flat top/bottom and disjoint channel write the price at each end of both lines beside
    /// it, in the chart's format (two decimals here), in their own style or by default in the
    /// line colour at 12 points, and in either anchor order. Anchors (100, 300), (300, 100) and
    /// (200, 250) give a base line of slope -1; the flat line lies at y = 250, price 250, and the
    /// mirrored line of slope +1 through (200, 250) at y = 150 and 350 at x = 100 and 300.
    /// Labels sit 4 + 1 pt beside their ends, outside the lines.
    #[test]
    fn channel_prices_sit_beside_both_lines_in_their_own_style() {
        use crate::drawings::{ChartPoint, FontWeight, NoteTextStyle};
        use egui::Align2 as A;
        type Label = (&'static str, A, (f32, f32));
        let flat: [Label; 4] = [
            ("200.00", A::RIGHT_CENTER, (95.0, 300.0)),
            ("400.00", A::LEFT_CENTER, (305.0, 100.0)),
            ("250.00", A::RIGHT_CENTER, (95.0, 250.0)),
            ("250.00", A::LEFT_CENTER, (305.0, 250.0)),
        ];
        let disjoint: [Label; 4] = [
            ("200.00", A::RIGHT_CENTER, (95.0, 300.0)),
            ("400.00", A::LEFT_CENTER, (305.0, 100.0)),
            ("350.00", A::RIGHT_CENTER, (95.0, 150.0)),
            ("150.00", A::LEFT_CENTER, (305.0, 350.0)),
        ];
        let own = NoteTextStyle {
            color: [10, 20, 30, 128],
            font_size: 20.0,
            bold: true,
            italic: true,
        };
        for (tool, labels) in [
            (DrawingToolType::FlatTopBottom, flat),
            (DrawingToolType::DisjointChannel, disjoint),
        ] {
            for reverse in [false, true] {
                let mut drawing = Drawing::new(1, tool);
                let mut points = vec![
                    Pos2::new(100.0, 300.0),
                    Pos2::new(300.0, 100.0),
                    Pos2::new(200.0, 250.0),
                ];
                if reverse {
                    points.swap(0, 1);
                }
                drawing.chart_points = points
                    .iter()
                    .map(|p| ChartPoint::new(p.x / 10.0, f64::from(500.0 - p.y)))
                    .collect();
                drawing.points = points;
                drawing.fill_color = None;
                drawing.theme_shade = false;
                drawing.stroke_width = 2.0;
                let prices = |drawing: &Drawing| -> Vec<egui::epaint::TextShape> {
                    painted(drawing)
                        .into_iter()
                        .filter_map(|shape| match shape {
                            egui::Shape::Text(text) if text.galley.text().contains(".00") => {
                                Some(text)
                            }
                            _ => None,
                        })
                        .collect()
                };
                assert!(prices(&drawing).is_empty(), "{tool:?}: off by default");
                drawing.line_measurements.price_labels = true;
                for (style, weight) in [(None, FontWeight::Bold), (Some(own), FontWeight::Normal)] {
                    drawing.anchor_price_style = style;
                    // The text's bold does not reach the prices.
                    drawing.font_weight = weight;
                    let (color, size, italic, copies) = match style {
                        None => (drawing.color32(), 12.0, false, 1),
                        Some(_) => (
                            Color32::from_rgba_unmultiplied(10, 20, 30, 128),
                            20.0,
                            true,
                            2,
                        ),
                    };
                    let painted = prices(&drawing);
                    assert_eq!(painted.len(), 4 * copies, "{tool:?} {reverse} {style:?}");
                    for (label, align, at) in labels {
                        let at = Pos2::new(at.0, at.1);
                        // A bold copy sits 0.6 pt right of the plain one.
                        let copy: Vec<_> = painted
                            .iter()
                            .filter(|text| text.galley.text() == label)
                            .filter(|text| {
                                let point =
                                    text.pos + align.pos_in_rect(&text.galley.rect).to_vec2();
                                (0..copies).any(|i| {
                                    point.distance(at + egui::vec2(0.6 * i as f32, 0.0)) < 1e-3
                                })
                            })
                            .collect();
                        assert_eq!(copy.len(), copies, "{tool:?} {reverse} {label} at {at:?}");
                        for text in copy {
                            let format = &text.galley.job.sections[0].format;
                            assert_eq!(format.color, color, "{tool:?} {label}");
                            assert_eq!(format.font_id.size, size, "{tool:?} {label}");
                            assert_eq!(format.italics, italic, "{tool:?} {label}");
                        }
                    }
                }
            }
        }
    }

    /// A flat top/bottom's or disjoint channel's line start and line end mark both of its lines
    /// at the first and second anchor's ends, as extended. By hand: the base line of slope -1
    /// meets the pane's left edge at y = 400 and its right edge at y = -100; the mirrored line of
    /// slope +1 through (100, 150) meets them at y = 50 and 550. The flat line lies at y = 250.
    #[test]
    fn channel_line_ends_mark_both_lines_from_the_first_anchor() {
        use crate::drawings::ArrowStyle;
        type Ends = [(f32, f32); 2];
        type Row = (DrawingToolType, bool, bool, Ends, Ends);
        let rows: [Row; 6] = [
            (
                DrawingToolType::DisjointChannel,
                false,
                false,
                [(100.0, 300.0), (100.0, 150.0)],
                [(300.0, 100.0), (300.0, 350.0)],
            ),
            (
                DrawingToolType::DisjointChannel,
                true,
                false,
                [(300.0, 100.0), (300.0, 350.0)],
                [(100.0, 300.0), (100.0, 150.0)],
            ),
            (
                DrawingToolType::DisjointChannel,
                false,
                true,
                [(0.0, 400.0), (0.0, 50.0)],
                [(500.0, -100.0), (500.0, 550.0)],
            ),
            (
                DrawingToolType::DisjointChannel,
                true,
                true,
                [(500.0, -100.0), (500.0, 550.0)],
                [(0.0, 400.0), (0.0, 50.0)],
            ),
            (
                DrawingToolType::FlatTopBottom,
                false,
                false,
                [(100.0, 300.0), (100.0, 250.0)],
                [(300.0, 100.0), (300.0, 250.0)],
            ),
            (
                DrawingToolType::FlatTopBottom,
                true,
                true,
                [(500.0, -100.0), (500.0, 250.0)],
                [(0.0, 400.0), (0.0, 250.0)],
            ),
        ];
        let sorted = |mut points: Vec<Pos2>| {
            points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
            points
        };
        for (tool, reverse, extend, starts, ends) in rows {
            let mut drawing = Drawing::new(1, tool);
            drawing.points = vec![
                Pos2::new(100.0, 300.0),
                Pos2::new(300.0, 100.0),
                Pos2::new(200.0, 250.0),
            ];
            if reverse {
                drawing.points.swap(0, 1);
            }
            drawing.fill_color = None;
            drawing.extend_left = extend;
            drawing.extend_right = extend;
            // An arrow is a path with its tip first, a square a rectangle centred on its tip.
            drawing.arrow_start_style = ArrowStyle::Arrow;
            drawing.arrow_end_style = ArrowStyle::Square;
            let (mut tips, mut squares) = (Vec::new(), Vec::new());
            for shape in painted(&drawing) {
                match shape {
                    egui::Shape::Path(path) if path.fill != Color32::TRANSPARENT => {
                        tips.push(path.points[0])
                    }
                    egui::Shape::Rect(rect) => squares.push(rect.rect.center()),
                    _ => {}
                }
            }
            let want = |ends: Ends| sorted(ends.iter().map(|&(x, y)| Pos2::new(x, y)).collect());
            let row = format!("{tool:?} reverse {reverse} extend {extend}");
            for (got, want) in [(sorted(tips), want(starts)), (sorted(squares), want(ends))] {
                assert_eq!(got.len(), 2, "{row}: {got:?}");
                for (got, want) in got.iter().zip(&want) {
                    assert!(got.distance(*want) < 1e-3, "{row}: {got:?}, want {want:?}");
                }
            }
        }
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
        let extended = Moments::of(&bars[..5], PriceSource::Close).with(bars[5].close);
        for moments in [Moments::of(bars, PriceSource::Close), streamed, extended] {
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
        let flat = Moments::of(
            &vec![
                Bar {
                    close: 0.1,
                    ..bars[0].clone()
                };
                100
            ],
            PriceSource::Close,
        )
        .fit()
        .unwrap();
        assert_eq!((flat.r_squared, flat.correlation), (1.0, None));
    }

    /// Closes 10, 14, 12 have Sxx = 2, Sxy = 2, Syy = 8: y = 11 + x,
    /// residual sigma = sqrt(3), r = 1/2. Each half ends on that same centre line.
    #[test]
    fn regression_channel_respects_fill_extension_and_line_roles() {
        use crate::drawings::{ChartPoint, RegressionBandLine, RegressionTrendConfig};
        use crate::model::LineStyle;
        use egui::{Context, RawInput, Shape};
        // Highs 13, 15, 16 rise 2, 3, 3 above y = 11 + x and lows 9, 10, 12 fall 2, 2, 1 below.
        let bars: Vec<_> = [(10.0, 13.0, 9.0), (14.0, 15.0, 10.0), (12.0, 16.0, 12.0)]
            .into_iter()
            .map(|(close, high, low)| Bar {
                close,
                high,
                low,
                ..sample_bars()[0].clone()
            })
            .collect();
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0));
        let paint = |drawing: &Drawing| {
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
                            price_formatter: None,
                            thousands_sep: None,
                            time_formatter: None,
                            bar_seconds: 0,
                            instrument: None,
                            bars: &bars,
                            revision: 0,
                            bar_to_x: &|b| 100.0 + b * 100.0,
                            bar_spacing: 100.0,
                            price_to_y: &|p| 500.0 - p as f32 * 10.0,
                            y_to_price: &|y| f64::from((500.0 - y) / 10.0),
                            time_axis: true,
                            executions: None,
                            timeframe: None,
                            finer: &[],
                        },
                    );
                },
            );
            let mut shapes = Vec::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut shapes);
            }
            output.drop_without_applying_deltas();
            shapes
        };
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
                    let shapes = paint(&drawing);
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

        // Each side a deviation away or through its extreme price; the HL2 midpoints 11, 12.5, 14
        // lie on y = 11 + 1.5x (r = 1), their highs and lows 2, 2.5, 2 away.
        let sigma = 3f64.sqrt();
        for (use_upper, use_lower, source, line_style, precision, slope, above, below, label) in [
            (
                true,
                true,
                PriceSource::Close,
                LineStyle::Dashed,
                None,
                1.0,
                sigma,
                sigma,
                "0.50",
            ),
            (
                false,
                true,
                PriceSource::Close,
                LineStyle::Solid,
                Some(3),
                1.0,
                3.0,
                sigma,
                "0.500",
            ),
            (
                true,
                false,
                PriceSource::Close,
                LineStyle::Dotted,
                None,
                1.0,
                sigma,
                2.0,
                "0.50",
            ),
            (
                false,
                false,
                PriceSource::HL2,
                LineStyle::Dashed,
                Some(3),
                1.5,
                2.5,
                2.5,
                "1.000",
            ),
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::RegressionTrend);
            drawing.chart_points = vec![ChartPoint::new(0.0, 30.0), ChartPoint::new(2.0, 5.0)];
            drawing.update_screen_coords(|b| 100.0 + b * 100.0, |p| 500.0 - p as f32 * 10.0);
            let mut config = RegressionTrendConfig::new(1.0)
                .and_then(|config| config.with_precision(precision))
                .unwrap();
            config.use_upper = use_upper;
            config.use_lower = use_lower;
            config.source = source;
            config.line_style = line_style;
            drawing.regression_trend = Some(config);
            let shapes = paint(&drawing);
            let y = |i: f64, offset: f64| 500.0 - ((11.0 + slope * i + offset) * 10.0) as f32;
            let lines: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::LineSegment { points, stroke } => Some((*points, stroke.color)),
                    _ => None,
                })
                .collect();
            for offset in [above, -below] {
                let band = [
                    Pos2::new(100.0, y(0.0, offset)),
                    Pos2::new(300.0, y(2.0, offset)),
                ];
                assert!(
                    lines
                        .iter()
                        .any(|(p, color)| *color == Color32::from_rgb(41, 98, 255)
                            && p[0].distance(band[0]) < 1e-3
                            && p[1].distance(band[1]) < 1e-3),
                    "{label}: missing band {band:?}"
                );
            }
            let center: Vec<_> = lines
                .iter()
                .filter(|(_, color)| *color == drawing.color32())
                .map(|(p, _)| *p)
                .collect();
            let longest = center
                .iter()
                .map(|p| p[0].distance(p[1]))
                .fold(0.0, f32::max);
            match line_style.dash_pattern() {
                None => assert_eq!(center.len(), 1, "{label}: solid line"),
                Some((dash, _)) => assert!((longest - dash).abs() < 1e-3, "{label}: {longest}"),
            }
            for p in center.iter().flatten() {
                assert!((p.y - y(f64::from((p.x - 100.0) / 100.0), 0.0)).abs() < 1e-3);
            }
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s, Shape::Text(t) if t.galley.job.text == label))
            );
        }

        // Each line drawn from its own switch, colour, width and pattern, and the correlation
        // from its switch. The base line runs (100, 390) to (300, 370) and the bands sqrt(3)
        // above and below it, 10 * sqrt(3) px.
        let band_offset = 10.0 * 3f32.sqrt();
        for (show_base, show_upper, show_lower, show_correlation) in [
            (true, true, true, true),
            (false, true, true, false),
            (true, false, true, true),
            (true, true, false, false),
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::RegressionTrend);
            drawing.chart_points = vec![ChartPoint::new(0.0, 30.0), ChartPoint::new(2.0, 5.0)];
            drawing.update_screen_coords(|b| 100.0 + b * 100.0, |p| 500.0 - p as f32 * 10.0);
            drawing.color = [0, 128, 0, 200];
            drawing.stroke_width = 3.0;
            let mut config = RegressionTrendConfig::new(1.0).unwrap();
            config.line_style = LineStyle::Solid;
            config.show_base_line = show_base;
            config.upper_line = RegressionBandLine::new([200, 0, 0, 128])
                .with_width(Some(5.0))
                .unwrap();
            config.upper_line.line_style = LineStyle::Dotted;
            config.upper_line.visible = show_upper;
            config.lower_line = RegressionBandLine::new([0, 0, 0, 255]);
            config.lower_line.line_style = LineStyle::LargeDashed;
            config.lower_line.visible = show_lower;
            config.show_correlation = show_correlation;
            drawing.regression_trend = Some(config);
            let shapes = paint(&drawing);
            let row = (show_base, show_upper, show_lower, show_correlation);
            for (shown, color, width, offset, style) in [
                (show_base, drawing.color32(), 3.0, 0.0, LineStyle::Solid),
                (
                    show_upper,
                    Color32::from_rgba_unmultiplied(200, 0, 0, 128),
                    5.0,
                    -band_offset,
                    LineStyle::Dotted,
                ),
                (
                    show_lower,
                    Color32::from_rgb(0, 0, 0),
                    3.0,
                    band_offset,
                    LineStyle::LargeDashed,
                ),
            ] {
                let pieces: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match s {
                        Shape::LineSegment { points, stroke } if stroke.color == color => {
                            Some((*points, stroke.width))
                        }
                        _ => None,
                    })
                    .collect();
                if !shown {
                    assert!(pieces.is_empty(), "{row:?}: hidden {style:?} line painted");
                    continue;
                }
                let on_line =
                    |p: Pos2| (p.y - (390.0 - (p.x - 100.0) / 10.0 + offset)).abs() < 1e-3;
                for (points, stroke_width) in &pieces {
                    assert_eq!(*stroke_width, width, "{row:?}: {style:?} width");
                    assert!(points.iter().all(|p| on_line(*p)), "{row:?}: {points:?}");
                }
                let longest = pieces
                    .iter()
                    .map(|(p, _)| p[0].distance(p[1]))
                    .fold(0.0, f32::max);
                match style.dash_pattern() {
                    None => assert_eq!(
                        pieces.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                        [[Pos2::new(100.0, 390.0), Pos2::new(300.0, 370.0)]],
                        "{row:?}: solid base line"
                    ),
                    Some((dash, _)) => {
                        assert!(
                            (longest - dash).abs() < 1e-3,
                            "{row:?}: {style:?} {longest}"
                        );
                    }
                }
            }
            assert_eq!(
                shapes
                    .iter()
                    .any(|s| matches!(s, Shape::Text(t) if t.galley.job.text == "0.50")),
                show_correlation,
                "{row:?}: correlation"
            );
        }
    }
}
