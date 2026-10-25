//! Cyclic tool rendering implementations
//!
//! Includes: cyclic lines, time cycles, and sine line.

use super::style::circle_sides;
use crate::drawings::domain::Drawing;
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders cyclic vertical lines with customizable period and phase.
    /// Features: repeating vertical lines, cycle number labels, phase shift support.
    pub(crate) fn render_cyclic_lines(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let period = (f64::from(p2.x) - f64::from(p1.x)).abs();
        if !period.is_finite() || period < 1.0 {
            return;
        }

        // Alternating fill between cycle lines
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);

        let font = egui::FontId::proportional(typography::XS);

        // Cycle numbers of the visible lines (negative left of the anchor), computed
        // directly so an anchor far off screen costs nothing.
        let anchor = f64::from(p1.x);
        let first = ((f64::from(chart_rect.min.x) - anchor) / period).ceil() as i64;
        let last = ((f64::from(chart_rect.max.x) - anchor) / period).floor() as i64;
        for cycle_num in first..=last {
            let x = (anchor + cycle_num as f64 * period) as f32;
            // Draw vertical line
            painter.vline(
                x,
                chart_rect.y_range(),
                Stroke::new(stroke::HAIRLINE, color),
            );

            // Alternating fill between lines
            if cycle_num % 2 != 0 && x + period as f32 <= chart_rect.max.x {
                let fill_rect = Rect::from_min_max(
                    Pos2::new(x, chart_rect.min.y),
                    Pos2::new(x + period as f32, chart_rect.max.y),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }

            // Cycle number label at top
            let label = format!("{cycle_num}");
            let label_bg = Rect::from_center_size(
                Pos2::new(x, chart_rect.min.y + DESIGN_TOKENS.spacing.xl),
                egui::vec2(
                    DESIGN_TOKENS.sizing.technical_labels.cycle_label_width / 2.0,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(
                label_bg,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.78),
            );
            painter.text(
                Pos2::new(x, chart_rect.min.y + 12.0),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                color,
            );
        }

        // Draw anchor points with white stroke
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

        // Period label at bottom center of first cycle
        let period_bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() as i32
        } else {
            (period / 10.0) as i32 // Approximate
        };
        let period_label = format!("Period: {period_bars} bars");
        let period_bg = Rect::from_center_size(
            Pos2::new(
                (p1.x + p2.x) / 2.0,
                chart_rect.max.y - DESIGN_TOKENS.spacing.xl - DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width
                    + DESIGN_TOKENS.spacing.section_lg
                    + DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            ),
        );
        painter.rect_filled(
            period_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            period_bg.center(),
            egui::Align2::CENTER_CENTER,
            period_label,
            font,
            color,
        );
    }

    /// Renders time cycles: the anchors span one cycle, repeated as half circles
    /// forward in time to the right edge of the chart.
    pub(crate) fn render_time_cycles(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let edge = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let arcs = time_cycle_arcs(center, edge, chart_rect.x_range());
        let Some(&(first_arc, base_radius)) = arcs.first() else {
            return;
        };
        // The last arc may run past the right edge of the pane.
        let painter = &painter.with_clip_rect(chart_rect);
        let font = egui::FontId::proportional(typography::XS);
        let arc_stroke = Stroke::new(self.stroke_width, color);
        for (arc_center, radius) in arcs {
            let segments = circle_sides(radius * painter.pixels_per_point()).div_ceil(2);
            // Upper half circle, from the cycle's start to its end.
            let points = (0..=segments)
                .map(|j| {
                    let angle = std::f32::consts::PI * (1.0 + j as f32 / segments as f32);
                    arc_center + radius * egui::vec2(angle.cos(), angle.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, arc_stroke));
        }

        // Draw center anchor point
        painter.circle_filled(center, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            center,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );

        // Draw edge point
        painter.circle_filled(edge, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            edge,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Base cycle info
        let bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() as i32
        } else {
            (base_radius / 10.0) as i32
        };
        let info_label = format!("Cycle: {bars} bars");
        let info_bg = Rect::from_center_size(
            Pos2::new(
                first_arc.x,
                first_arc.y - base_radius - DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width
                    + DESIGN_TOKENS.spacing.xxl,
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            ),
        );
        painter.rect_filled(
            info_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            info_bg.center(),
            egui::Align2::CENTER_CENTER,
            info_label,
            font,
            color,
        );
    }

    /// Renders a sine wave line for cyclical analysis.
    /// The anchors are a peak and a trough half a period apart; the curve extends across the chart.
    pub(crate) fn render_sine_line(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let wavelength = (p2.x - p1.x).abs() * 2.0;
        let amplitude = (p2.y - p1.y).abs() / 2.0;
        let center_y = (p1.y + p2.y) / 2.0;

        // Draw sine wave extending across chart
        let segments = 200;
        let start_x = chart_rect.min.x;
        let end_x = chart_rect.max.x;
        let x_range = end_x - start_x;

        let mut wave = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let x = start_x + t * x_range;

            // Anchors too close together draw nothing; the first sample returns before any paint.
            let Some(y) = sine_line_y(p1, p2, x) else {
                return;
            };
            wave.push(Pos2::new(x, y));
        }
        // One path, not a segment shape per sample, so the tessellator joins the samples.
        painter.add(egui::Shape::line(
            wave,
            Stroke::new(self.stroke_width, color),
        ));

        // Draw zero/center line (dashed)
        let zero_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        let dash_len = DESIGN_TOKENS.spacing.lg;
        let gap_len = DESIGN_TOKENS.spacing.md + DESIGN_TOKENS.spacing.xs;
        let (left, right) = (chart_rect.min.x, chart_rect.max.x);
        draw_dashed_line(
            painter,
            Pos2::new(left, center_y),
            Pos2::new(right, center_y),
            Stroke::new(stroke::HAIRLINE, zero_color),
            dash_len,
            gap_len,
        );

        // Draw amplitude guides (dashed horizontal lines at the peak and trough)
        for y in [p1.y, p2.y] {
            draw_dashed_line(
                painter,
                Pos2::new(left, y),
                Pos2::new(right, y),
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, zero_color),
                dash_len,
                gap_len,
            );
        }

        // Draw anchor points
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

        // Info labels
        let font = egui::FontId::proportional(typography::XS);

        // Wavelength label
        let wave_bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            ((self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() * 2.0) as i32
        } else {
            (wavelength / 10.0) as i32
        };
        let wavelength_label = format!("λ: {wave_bars} bars");
        let wave_bg = Rect::from_center_size(
            Pos2::new(
                (p1.x + p2.x) / 2.0,
                center_y + DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    + DESIGN_TOKENS.spacing.lg,
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
            ),
        );
        painter.rect_filled(
            wave_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78),
        );
        painter.text(
            wave_bg.center(),
            egui::Align2::CENTER_CENTER,
            wavelength_label,
            font.clone(),
            color,
        );

        // Amplitude label
        let amp_price = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].price - self.chart_points[0].price).abs() / 2.0
        } else {
            (amplitude / 10.0) as f64
        };
        let amp_label = format!("A: {amp_price:.2}");
        let amp_bg = Rect::from_center_size(
            Pos2::new(
                p2.x + DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm,
                (p1.y + p2.y) / 2.0,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_offset_x,
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
            ),
        );
        painter.rect_filled(
            amp_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78),
        );
        painter.text(
            amp_bg.center(),
            egui::Align2::CENTER_CENTER,
            amp_label,
            font,
            color,
        );
    }
}

/// The sine line's y at `x`, with a peak or trough at each anchor and half a period between them.
/// `None` when the anchors are less than 5 px apart horizontally.
fn sine_line_y(p1: Pos2, p2: Pos2, x: f32) -> Option<f32> {
    let half_period = p2.x - p1.x;
    if half_period.abs() < 5.0 {
        return None;
    }
    let center = (p1.y + p2.y) / 2.0;
    Some(center + (p1.y - center) * (std::f32::consts::PI * (x - p1.x) / half_period).cos())
}

/// Centre and radius of each time cycle arc that reaches into `x_range`: the
/// anchors span the first cycle, and arcs as wide follow it forward in time,
/// standing on the first anchor's price. Empty when the anchors are less than
/// 5 px apart horizontally.
fn time_cycle_arcs(p1: Pos2, p2: Pos2, x_range: egui::Rangef) -> Vec<(Pos2, f32)> {
    let period = (p2.x - p1.x).abs();
    if period < 5.0 {
        return Vec::new();
    }
    let start = p1.x.min(p2.x);
    // Skip the cycles that end left of the view without walking through them.
    let first = ((x_range.min - start) / period).floor().max(0.0) as u32;
    (first..)
        .map(|k| start + period * k as f32)
        .take_while(|&left| left < x_range.max)
        .map(|left| (Pos2::new(left + period / 2.0, p1.y), period / 2.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The anchors span one cycle; arcs of that width follow each other from
    /// the earlier anchor to the right edge, on the first anchor's price.
    #[test]
    fn time_cycles_repeat_the_anchor_interval_forward() {
        let cases = [
            // Cycle 100..140: arcs centred at 120, 160, 200 and 240, the last cut by the edge.
            (
                (100.0, 200.0),
                (140.0, 180.0),
                (0.0, 250.0),
                vec![120.0, 160.0, 200.0, 240.0],
            ),
            // Second anchor first in time; the arc ending at 140 is left of the view.
            (
                (140.0, 200.0),
                (100.0, 180.0),
                (170.0, 250.0),
                vec![160.0, 200.0, 240.0],
            ),
            ((100.0, 200.0), (104.0, 150.0), (0.0, 250.0), vec![]),
        ];
        for ((x1, y1), (x2, y2), (min, max), centres) in cases {
            let got = time_cycle_arcs(
                Pos2::new(x1, y1),
                Pos2::new(x2, y2),
                egui::Rangef::new(min, max),
            );
            let want: Vec<_> = centres.iter().map(|&x| (Pos2::new(x, y1), 20.0)).collect();
            assert!(
                got.len() == want.len()
                    && got
                        .iter()
                        .zip(&want)
                        .all(|(g, w)| g.0.distance(w.0) < 1e-3 && (g.1 - w.1).abs() < 1e-3),
                "anchors x {x1} and {x2}: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn sine_line_passes_through_both_anchors_half_a_period_apart() {
        // Expected values from y = c + (p1.y - c) * cos(pi * (x - p1.x) / (p2.x - p1.x)), c = (p1.y + p2.y) / 2.
        let (a, b) = (Pos2::new(100.0, 50.0), Pos2::new(150.0, 150.0));
        let cases = [
            (a, b, 100.0, Some(50.0)),
            (a, b, 150.0, Some(150.0)),
            (a, b, 125.0, Some(100.0)),
            (a, b, 200.0, Some(50.0)),
            (a, b, 112.5, Some(64.644_66)),
            (a, b, 75.0, Some(100.0)),
            (b, a, 100.0, Some(50.0)),
            (b, a, 125.0, Some(100.0)),
            (a, Pos2::new(104.0, 150.0), 100.0, None),
        ];
        for (p1, p2, x, want) in cases {
            let got = sine_line_y(p1, p2, x);
            let ok = match (got, want) {
                (Some(g), Some(w)) => (g - w).abs() < 1e-4,
                (g, w) => g == w,
            };
            assert!(ok, "p1 {p1:?} p2 {p2:?} x {x}: got {got:?}, want {want:?}");
        }
    }
}
