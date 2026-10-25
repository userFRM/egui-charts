//! Bars pattern rendering

use crate::drawings::domain::{ChartPoint, Drawing};
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::Bar;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders Bars Pattern: the bars between the first two anchors, copied
    /// to start at the third anchor's bar and shifted by its price distance
    /// from the first anchor.
    pub(crate) fn render_bars_pattern(&self, painter: &egui::Painter, view: &ChartView) {
        let (&[source_start, source_end, ..], &[from, to, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };
        let color = self.color32();

        // Source region box around the selected bars
        let source = bars_between(view.bars, from.bar_idx, to.bar_idx);
        let source_rect = source.map_or_else(
            || Rect::from_two_pos(source_start, source_end),
            |(first, bars)| bars_rect(view, first as f32, bars),
        );
        self.draw_source_region(painter, source_rect, color);

        // If we have a third point, draw the projection
        if let (Some(&proj_anchor), Some(&at)) = (self.points.get(2), self.chart_points.get(2))
            && let Some((start, projected)) = projected_bars(view.bars, [from, to, at])
        {
            self.draw_pattern_projection(
                painter,
                view,
                source_rect,
                proj_anchor,
                start,
                &projected,
                color,
            );
        }

        // Source anchor points
        self.draw_pattern_anchors(painter, source_start, source_end, color);
    }

    fn draw_source_region(&self, painter: &egui::Painter, source_rect: Rect, color: Color32) {
        let source_fill =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);
        painter.rect_filled(source_rect, 0.0, source_fill);
        painter.rect_stroke(
            source_rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );

        // Source label
        let font = egui::FontId::proportional(typography::XS);
        let source_label_bg = Rect::from_center_size(
            Pos2::new(source_rect.center().x, source_rect.min.y - 12.0),
            egui::vec2(60.0, 16.0),
        );
        painter.rect_filled(source_label_bg, DESIGN_TOKENS.rounding.sm, color);
        painter.text(
            source_label_bg.center(),
            egui::Align2::CENTER_CENTER,
            "SOURCE",
            font,
            Color32::WHITE,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_pattern_projection(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        source_rect: Rect,
        proj_anchor: Pos2,
        start: f32,
        projected: &[Bar],
        color: Color32,
    ) {
        let font = egui::FontId::proportional(typography::XS);
        let proj_rect = bars_rect(view, start, projected);

        // Projected region (more transparent)
        let proj_fill =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 40);
        painter.rect_filled(proj_rect, 0.0, proj_fill);

        // Projected bars as OHLC bars: open tick left, close tick right
        let bar_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
        let bar_stroke = Stroke::new(stroke::MEDIUM, bar_color);
        let tick = ((view.bar_to_x)(1.0) - (view.bar_to_x)(0.0)).abs() * 0.3;
        for (i, bar) in projected.iter().enumerate() {
            let x = (view.bar_to_x)(start + i as f32);
            let y = |price| (view.price_to_y)(price);
            painter.line_segment(
                [Pos2::new(x, y(bar.high)), Pos2::new(x, y(bar.low))],
                bar_stroke,
            );
            painter.line_segment(
                [Pos2::new(x - tick, y(bar.open)), Pos2::new(x, y(bar.open))],
                bar_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(x, y(bar.close)),
                    Pos2::new(x + tick, y(bar.close)),
                ],
                bar_stroke,
            );
        }

        // Dashed border for projection
        let proj_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        self.draw_dashed_rect(painter, proj_rect, proj_color);

        // Projection label
        let proj_label_bg = Rect::from_center_size(
            Pos2::new(proj_rect.center().x, proj_rect.min.y - 12.0),
            egui::vec2(80.0, 16.0),
        );
        painter.rect_filled(proj_label_bg, DESIGN_TOKENS.rounding.sm, proj_color);
        painter.text(
            proj_label_bg.center(),
            egui::Align2::CENTER_CENTER,
            "PROJECTION",
            font,
            Color32::WHITE,
        );

        // Projection anchor point
        painter.circle_filled(proj_anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            proj_anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Connection arrow from source to projection
        self.draw_connection_arrow(painter, source_rect, proj_rect);
    }

    fn draw_dashed_rect(&self, painter: &egui::Painter, rect: Rect, color: Color32) {
        let dash = 6.0;
        let gap = 4.0;

        let stroke = Stroke::new(stroke::HAIRLINE, color);
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        for (i, &corner) in corners.iter().enumerate() {
            draw_dashed_line(painter, corner, corners[(i + 1) % 4], stroke, dash, gap);
        }
    }

    fn draw_connection_arrow(&self, painter: &egui::Painter, source_rect: Rect, proj_rect: Rect) {
        let arrow_start = Pos2::new(source_rect.max.x + 10.0, source_rect.center().y);
        let arrow_end = Pos2::new(proj_rect.min.x - 10.0, proj_rect.center().y);
        let arrow_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);

        // Dashed arrow line
        let dash = 6.0;
        let gap = 4.0;
        if arrow_start.x < arrow_end.x - 10.0 {
            draw_dashed_line(
                painter,
                arrow_start,
                Pos2::new(arrow_end.x - 10.0, arrow_start.y),
                Stroke::new(stroke::HAIRLINE, arrow_color),
                dash,
                gap,
            );
        }

        // Arrow head
        let arrow_size = 8.0;
        painter.line_segment(
            [
                arrow_end,
                Pos2::new(arrow_end.x - arrow_size, arrow_end.y - arrow_size * 0.6),
            ],
            Stroke::new(stroke::MEDIUM, arrow_color),
        );
        painter.line_segment(
            [
                arrow_end,
                Pos2::new(arrow_end.x - arrow_size, arrow_end.y + arrow_size * 0.6),
            ],
            Stroke::new(stroke::MEDIUM, arrow_color),
        );
    }

    fn draw_pattern_anchors(
        &self,
        painter: &egui::Painter,
        source_start: Pos2,
        source_end: Pos2,
        color: Color32,
    ) {
        painter.circle_filled(source_start, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            source_start,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(source_end, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            source_end,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }
}

/// Screen rect around `bars` drawn from bar index `first` on.
fn bars_rect(view: &ChartView, first: f32, bars: &[Bar]) -> Rect {
    let high = bars
        .iter()
        .map(|bar| bar.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = bars.iter().map(|bar| bar.low).fold(f64::INFINITY, f64::min);
    Rect::from_two_pos(
        Pos2::new((view.bar_to_x)(first - 0.5), (view.price_to_y)(high)),
        Pos2::new(
            (view.bar_to_x)(first + bars.len() as f32 - 0.5),
            (view.price_to_y)(low),
        ),
    )
}

/// The bars between the first two anchors, moved to start at the third
/// anchor's bar and shifted by the third anchor's price minus the first's.
/// Returns the first projected bar index and the shifted copies.
fn projected_bars(bars: &[Bar], anchors: [ChartPoint; 3]) -> Option<(f32, Vec<Bar>)> {
    let [from, to, at] = anchors;
    let (_, source) = bars_between(bars, from.bar_idx, to.bar_idx)?;
    let shift = at.price - from.price;
    let projected = source
        .iter()
        .map(|bar| Bar {
            open: bar.open + shift,
            high: bar.high + shift,
            low: bar.low + shift,
            close: bar.close + shift,
            ..bar.clone()
        })
        .collect();
    Some((at.bar_idx.round(), projected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

    /// Anchors between bars snap to the nearest bar: bars 1..=3 of the sample
    /// move to bars 8..=10 and up by 14 - 12 = 2 (values added by hand).
    #[test]
    fn projected_bars_copy_source_bars() {
        let anchors = [
            ChartPoint::new(0.6, 12.0),
            ChartPoint::new(3.4, 13.0),
            ChartPoint::new(8.4, 14.0),
        ];
        let (start, projected) = projected_bars(&sample_bars(), anchors).unwrap();
        assert_eq!(start, 8.0);
        let ohlc: Vec<_> = projected
            .iter()
            .map(|b| (b.open, b.high, b.low, b.close))
            .collect();
        assert_eq!(
            ohlc,
            [
                (13.0, 15.0, 12.0, 14.0),
                (14.0, 14.5, 13.0, 13.0),
                (13.5, 16.0, 13.0, 15.5)
            ]
        );
    }
}
