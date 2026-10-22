//! Channel tool rendering implementations
//!
//! Includes: parallel channel, disjoint channel, regression trend, and flat top/bottom.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
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
    pub(crate) fn render_parallel_channel(&self, painter: &egui::Painter) {
        if self.points.len() < 3 {
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        // Draw first trendline
        painter.line_segment([p1, p2], stroke);

        // Calculate parallel offset vector
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;

        // Calculate perpendicular distance from p3 to line p1-p2
        let line_len_sq = dx * dx + dy * dy;
        if line_len_sq < 1e-6 {
            return; // Degenerate line
        }

        // Project p3 onto line p1-p2
        let t = ((p3.x - p1.x) * dx + (p3.y - p1.y) * dy) / line_len_sq;
        let proj_x = p1.x + t * dx;
        let proj_y = p1.y + t * dy;

        // Calculate offset
        let offset_x = p3.x - proj_x;
        let offset_y = p3.y - proj_y;

        // Draw parallel line
        let p4 = Pos2::new(p1.x + offset_x, p1.y + offset_y);
        let p5 = Pos2::new(p2.x + offset_x, p2.y + offset_y);
        painter.line_segment([p4, p5], stroke);

        // Draw conn lines (dashed)
        let dashed_stroke = Stroke::new(stroke::HAIRLINE, Color32::from_white_alpha(128));
        painter.line_segment([p1, p4], dashed_stroke);
        painter.line_segment([p2, p5], dashed_stroke);
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
    pub(crate) fn render_flat_top_bottom(&self, painter: &egui::Painter) {
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
        let extend_amount = typography::XXL + typography::LG; // Extend beyond the trend line
        let flat_start = Pos2::new(p1.x.min(p2.x) - extend_amount, p3.y);
        let flat_end = Pos2::new(p1.x.max(p2.x) + extend_amount, p3.y);
        painter.line_segment(
            [flat_start, flat_end],
            Stroke::new(self.stroke_width, color),
        );

        // Draw trendline from p1 to p2
        painter.line_segment([p1, p2], Stroke::new(self.stroke_width, color));

        // Create fill polygon between trend line and flat line
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 35);

        // Fill polygon between trend line and flat line
        let fill_points = if is_flat_top {
            vec![
                Pos2::new(p1.x, p1.y),
                Pos2::new(p2.x, p2.y),
                Pos2::new(p2.x, p3.y),
                Pos2::new(p1.x, p3.y),
            ]
        } else {
            vec![
                Pos2::new(p1.x, p3.y),
                Pos2::new(p2.x, p3.y),
                Pos2::new(p2.x, p2.y),
                Pos2::new(p1.x, p1.y),
            ]
        };
        painter.add(egui::epaint::PathShape::convex_polygon(
            fill_points,
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
        let height_diff = (p3.y - (p1.y + p2.y) / 2.0).abs();
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
            format!("{height_diff:.0}px"),
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
    pub(crate) fn render_disjoint_channel(&self, painter: &egui::Painter) {
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
        let p4 = Pos2::new(p3.x + dx, p3.y + dy);

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
            let channel_width = ((p3.x - p1.x).powi(2) + (p3.y - p1.y).powi(2)).sqrt();
            let mid_connector = Pos2::new((mid1.x + mid2.x) / 2.0, (mid1.y + mid2.y) / 2.0);

            // Label background
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);
            let label_rect = Rect::from_center_size(
                mid_connector,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.line_label_width
                        + DESIGN_TOKENS.spacing.lg,
                    typography::XXL,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);

            painter.text(
                mid_connector,
                egui::Align2::CENTER_CENTER,
                format!("{channel_width:.0}px"),
                egui::FontId::proportional(typography::XS),
                connector_color,
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

/// Least-squares line through the closes of a span of bars, against their
/// position in the span.
#[derive(Clone, Copy)]
struct LinearFit {
    /// Price change per bar.
    slope: f64,
    /// Fitted price at the first bar.
    intercept: f64,
    /// Standard deviation of the closes around the line (n - 1 denominator).
    sigma: f64,
    /// Share of the closes' variance the line explains.
    r_squared: f64,
    /// Pearson correlation of the closes with their position; `None` when
    /// the closes are flat.
    correlation: Option<f64>,
}

/// Means of the closes of a span of bars and of their positions in it, and
/// the sums of the products of their deviations from those means.
#[derive(Clone, Copy, Default)]
struct Moments {
    n: f64,
    mean_x: f64,
    mean_y: f64,
    sxx: f64,
    sxy: f64,
    syy: f64,
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

impl Moments {
    /// Moments of the closes of `bars`: the mean, corrected for rounding, then the centred sums.
    fn of(bars: &[Bar]) -> Self {
        let n = bars.len() as f64;
        if bars.is_empty() {
            return Self::default();
        }
        let mean_x = (n - 1.0) / 2.0;
        let mean_y = bars.iter().map(|bar| bar.close).sum::<f64>() / n;
        // Correct the rounding of the sum, so equal closes give their close as the mean.
        let mean_y = mean_y + bars.iter().map(|bar| bar.close - mean_y).sum::<f64>() / n;
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for (i, bar) in bars.iter().enumerate() {
            let (dx, dy) = (i as f64 - mean_x, bar.close - mean_y);
            sxx += dx * dx;
            sxy += dx * dy;
            syy += dy * dy;
        }
        Self {
            n,
            mean_x,
            mean_y,
            sxx,
            sxy,
            syy,
        }
    }

    /// These moments with the close `y` of the next bar added (Welford's update).
    fn with(&self, y: f64) -> Self {
        let n = self.n + 1.0;
        let (dx, dy) = (self.n - self.mean_x, y - self.mean_y);
        let (mean_x, mean_y) = (self.mean_x + dx / n, self.mean_y + dy / n);
        Self {
            n,
            mean_x,
            mean_y,
            sxx: self.sxx + dx * (self.n - mean_x),
            sxy: self.sxy + dx * (y - mean_y),
            syy: self.syy + dy * (y - mean_y),
        }
    }

    /// The least-squares line; `None` for fewer than two bars.
    fn fit(&self) -> Option<LinearFit> {
        if self.n < 2.0 {
            return None;
        }
        let slope = self.sxy / self.sxx;
        let residual = (self.syy - slope * self.sxy).max(0.0);
        // Closes are flat when they vary by no more than the rounding of their mean.
        let flat = self.syy <= 16.0 * self.n * (self.mean_y * f64::EPSILON).powi(2);
        Some(LinearFit {
            slope,
            intercept: self.mean_y - slope * self.mean_x,
            sigma: (residual / (self.n - 1.0)).sqrt(),
            // Flat closes are fitted exactly.
            r_squared: if flat { 1.0 } else { 1.0 - residual / self.syy },
            correlation: (!flat).then(|| self.sxy / (self.sxx * self.syy).sqrt()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

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
                ..bars[0]
            };
            100
        ])
        .fit()
        .unwrap();
        assert_eq!((flat.r_squared, flat.correlation), (1.0, None));
    }
}
