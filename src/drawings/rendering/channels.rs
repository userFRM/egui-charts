//! Channel tool rendering implementations
//!
//! Includes: parallel channel, disjoint channel, regression trend, and flat top/bottom.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::math::{LinearFit, Moments};
use crate::model::{Bar, SpanCache};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static MOMENTS: RefCell<SpanCache<Moments>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    pub(crate) fn render_parallel_channel(&self, painter: &egui::Painter, rect: Rect) {
        let Some(outline) = self.parallel_channel_outline(rect) else {
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
        let (&[start, end, ..], &[a, b, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };
        let color = self.color32();

        if let Some((first, bars)) = bars_between(view.bars, a.bar_idx, b.bar_idx)
            && let Some(fit) = span_moments(view, first, bars).fit()
        {
            self.draw_regression_channel(painter, view, first, bars.len(), &fit, color);
        }

        // Draw anchor points with white border
        for anchor in [start, end] {
            painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                anchor,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );
        }
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

        // Fill between the bands
        let fill = Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![upper[0], upper[1], lower[1], lower[0]],
            fill,
            Stroke::NONE,
        ));

        // Draw the band lines
        let band_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        for band in [upper, lower] {
            painter.line_segment(band, Stroke::new(stroke::HAIRLINE, band_color));
        }

        // Draw main regression line (thicker)
        painter.line_segment(center, Stroke::new(self.stroke_width + 0.5, color));

        // Draw info label with R² and the correlation
        let line_end = center[1];
        let label_pos = Pos2::new(line_end.x + typography::XS, line_end.y - typography::XXL);
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.9);
        let label_rect = Rect::from_min_size(
            label_pos,
            egui::Vec2::new(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width,
                DESIGN_TOKENS.sizing.technical_labels.channel_label_height,
            ),
        );
        painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
        painter.rect_stroke(
            label_rect,
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(stroke::HAIRLINE, color),
            egui::epaint::StrokeKind::Inside,
        );

        // R² value
        painter.text(
            Pos2::new(label_pos.x + 35.0, label_pos.y + typography::XS),
            egui::Align2::CENTER_CENTER,
            format!("R² = {:.2}", fit.r_squared),
            egui::FontId::proportional(typography::XS),
            color,
        );

        // Pearson correlation
        painter.text(
            Pos2::new(label_pos.x + 35.0, label_pos.y + typography::XXL),
            egui::Align2::CENTER_CENTER,
            fit.correlation
                .map_or_else(|| "r = n/a".to_owned(), |r| format!("r = {r:.2}")),
            egui::FontId::proportional(typography::XS),
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180),
        );
    }

    /// Render Flat Top/Bottom - Trend line with horizontal support/resistance
    pub(crate) fn render_flat_top_bottom(&self, painter: &egui::Painter, view: &ChartView) {
        if self.points.len() < 3 {
            let color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                self.color[3],
            );
            for (i, &p) in self.points.iter().enumerate() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.md, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], p],
                        Stroke::new(self.stroke_width, color),
                    );
                }
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        // Determine if this is a flat top or flat bottom
        let is_flat_top = p3.y < (p1.y + p2.y) / 2.0;

        // Draw flat horizontal line at p3's y level
        let [flat_start, flat_end] = flat_line(p1, p2, p3);
        painter.line_segment(
            [flat_start, flat_end],
            Stroke::new(self.stroke_width, color),
        );

        // Draw trendline from p1 to p2
        painter.line_segment([p1, p2], Stroke::new(self.stroke_width, color));

        // Create fill polygon between trend line and flat line
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 35);

        painter.add(egui::epaint::PathShape::convex_polygon(
            flat_top_bottom_fill(p1, p2, p3),
            fill_color,
            Stroke::NONE,
        ));

        // Draw dashed connector lines
        let connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        super::utils::draw_dashed_line(
            painter,
            Pos2::new(p1.x, p1.y),
            Pos2::new(p1.x, p3.y),
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.sm,
        );
        super::utils::draw_dashed_line(
            painter,
            Pos2::new(p2.x, p2.y),
            Pos2::new(p2.x, p3.y),
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.sm,
        );

        // Draw price labels
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.85);
        let label_font = egui::FontId::proportional(typography::XS);

        // Flat line price label (shown at the right end)
        let flat_label_pos = Pos2::new(flat_end.x + DESIGN_TOKENS.rounding.lg, p3.y);
        let flat_price_text = if is_flat_top { "Resistance" } else { "Support" };
        let flat_label_rect = Rect::from_min_size(
            flat_label_pos,
            egui::Vec2::new(
                DESIGN_TOKENS.sizing.technical_labels.channel_offset_x,
                typography::XXL,
            ),
        );
        painter.rect_filled(flat_label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
        painter.text(
            Pos2::new(
                flat_label_pos.x + DESIGN_TOKENS.sizing.technical_labels.cycle_label_width
                    - DESIGN_TOKENS.spacing.lg,
                flat_label_pos.y + DESIGN_TOKENS.spacing.lg,
            ),
            egui::Align2::CENTER_CENTER,
            flat_price_text,
            label_font.clone(),
            color,
        );

        // Height measurement between lines
        let mid_x = (p1.x + p2.x) / 2.0;
        let mid_trend_y = (p1.y + p2.y) / 2.0;

        // Draw measurement indicator
        let measure_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);

        // Vertical measurement line
        let measure_top = Pos2::new(mid_x, p3.y.min(mid_trend_y));
        let measure_bottom = Pos2::new(mid_x, p3.y.max(mid_trend_y));
        super::utils::draw_dashed_line(
            painter,
            measure_top,
            measure_bottom,
            Stroke::new(stroke::HAIRLINE, measure_color),
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS.rounding.sm,
        );

        // Height label
        let height_label_pos = Pos2::new(
            mid_x + DESIGN_TOKENS.rounding.lg,
            (p3.y + mid_trend_y) / 2.0,
        );
        painter.text(
            height_label_pos,
            egui::Align2::LEFT_CENTER,
            self.size_labels(view.price_to_y).concat(),
            egui::FontId::proportional(typography::XS),
            measure_color,
        );

        // Draw anchor points with white border
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p3, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p3,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Render Disjoint Channel - Two parallel channel segments with connector
    pub(crate) fn render_disjoint_channel(&self, painter: &egui::Painter, view: &ChartView) {
        if self.points.len() < 3 {
            let color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                self.color[3],
            );
            for (i, &p) in self.points.iter().enumerate() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.md, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], p],
                        Stroke::new(self.stroke_width, color),
                    );
                }
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        // Calculate direction vector for first segment
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let line_length = (dx * dx + dy * dy).sqrt();

        // Second segment endpoint (parallel to first)
        let [_, p4] = disjoint_second_line(p1, p2, p3);

        // Calculate midpoints for the connector
        let mid1 = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
        let mid2 = Pos2::new((p3.x + p4.x) / 2.0, (p3.y + p4.y) / 2.0);

        // Fill between the two channel segments
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![p1, p2, p4, p3],
            fill_color,
            Stroke::NONE,
        ));

        // Draw first segment (main stroke)
        painter.line_segment([p1, p2], Stroke::new(self.stroke_width, color));

        // Draw second segment (parallel)
        painter.line_segment([p3, p4], Stroke::new(self.stroke_width, color));

        // Draw middle connector line (dashed)
        let connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 120);
        super::utils::draw_dashed_line(
            painter,
            mid1,
            mid2,
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.md,
        );

        // Draw edge connectors (lighter dashed)
        let edge_connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        super::utils::draw_dashed_line(
            painter,
            p1,
            p3,
            Stroke::new(stroke::HAIRLINE, edge_connector_color),
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS.rounding.sm,
        );
        super::utils::draw_dashed_line(
            painter,
            p2,
            p4,
            Stroke::new(stroke::HAIRLINE, edge_connector_color),
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS.rounding.sm,
        );

        // Draw width measurement
        if line_length > 20.0 {
            let mid_connector = Pos2::new((mid1.x + mid2.x) / 2.0, (mid1.y + mid2.y) / 2.0);

            // Label background
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);
            super::measurements::boxed_label(
                painter,
                egui::Align2::CENTER_CENTER,
                mid_connector,
                self.size_labels(view.price_to_y).concat(),
                connector_color,
                label_bg,
            );
        }

        // Draw anchor points with white border
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p3, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p3,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Also draw the derived point p4
        let derived_point_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        painter.circle_filled(p4, DESIGN_TOKENS.rounding.md, derived_point_color);
        painter.circle_stroke(
            p4,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, Color32::WHITE),
        );
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

/// The flat top/bottom's horizontal line at `p3`, run past both ends of the
/// trend line from `p1` to `p2`.
pub(super) fn flat_line(p1: Pos2, p2: Pos2, p3: Pos2) -> [Pos2; 2] {
    let extend = typography::XXL + typography::LG;
    [p1.x.min(p2.x) - extend, p1.x.max(p2.x) + extend].map(|x| Pos2::new(x, p3.y))
}

/// The area between the flat top/bottom's trend line from `p1` to `p2` and
/// its flat line at `p3`, over the trend line's bars, wound clockwise.
pub(super) fn flat_top_bottom_fill(p1: Pos2, p2: Pos2, p3: Pos2) -> Vec<Pos2> {
    let (f1, f2) = (Pos2::new(p1.x, p3.y), Pos2::new(p2.x, p3.y));
    if p3.y < (p1.y + p2.y) / 2.0 {
        vec![p1, p2, f2, f1]
    } else {
        vec![f1, f2, p2, p1]
    }
}

/// The disjoint channel's second line: from `p3`, parallel to and as long as
/// the line from `p1` to `p2`.
pub(super) fn disjoint_second_line(p1: Pos2, p2: Pos2, p3: Pos2) -> [Pos2; 2] {
    [p3, p3 + (p2 - p1)]
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
}
