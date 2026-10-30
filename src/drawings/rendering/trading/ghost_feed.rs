//! Ghost feed rendering.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Project translucent candles in instrument ticks along a thin neutral path.
    pub(crate) fn render_ghost_feed(&self, painter: &egui::Painter, view: &ChartView) {
        // Without tick metadata, one price unit is one tick.
        let tick_size = view
            .instrument
            .and_then(|i| i.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .unwrap_or(1.0);
        let neutral = Color32::from_rgba_unmultiplied(128, 128, 128, 128);
        for c in self.ghost_candles(tick_size) {
            let x = (view.bar_to_x)(c.bar_idx + 0.5);
            let half_width =
                ((view.bar_to_x)(c.bar_idx + 1.0) - (view.bar_to_x)(c.bar_idx)).abs() * 0.35;
            let [open, high, low, close] = [c.open, c.high, c.low, c.close].map(view.price_to_y);
            if ![x, half_width, open, high, low, close]
                .into_iter()
                .all(f32::is_finite)
            {
                continue;
            }
            let (body, border) = if c.close >= c.open {
                ([184, 227, 220], [8, 153, 129])
            } else {
                ([237, 165, 166], [242, 54, 69])
            };
            painter.vline(x, high.min(low)..=high.max(low), Stroke::new(1.0, neutral));
            let rect = Rect::from_min_max(
                Pos2::new(x - half_width, open.min(close)),
                Pos2::new(x + half_width, open.max(close).max(open.min(close) + 1.0)),
            );
            painter.rect(
                rect,
                0.0,
                Color32::from_rgba_unmultiplied(body[0], body[1], body[2], 128),
                Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(border[0], border[1], border[2], 128),
                ),
                egui::StrokeKind::Inside,
            );
        }
        for pair in self.points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(1.0, neutral));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::ChartView;
    use crate::drawings::{ChartPoint, DrawingToolType};
    use egui::Shape;

    #[test]
    fn ghost_candles_keep_direction_colours_without_decorations() {
        for direction in [-1.0, 1.0] {
            let mut drawing = Drawing::new(1404, DrawingToolType::GhostFeed);
            drawing.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(32.0, 11.0)];
            drawing.stroke_width = 4.0;
            drawing.fill_color = Some([255, 0, 255, 255]);
            let bar_to_x = |b| 20.0 + b * 10.0;
            let price_to_y = |p| 200.0 + direction * p as f32 * 10.0;
            drawing.update_screen_coords(bar_to_x, price_to_y);
            let instrument = crate::model::Instrument::new("TEST").with_tick_size(0.25);
            let view = ChartView {
                price_formatter: None,
                instrument: Some(&instrument),
                bars: &[],
                revision: 0,
                bar_to_x: &bar_to_x,
                price_to_y: &price_to_y,
                y_to_price: &|y| f64::from((y - 200.0) / (direction * 10.0)),
                time_axis: true,
                executions: None,
            };
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let painter = ui.painter();
                drawing.render(
                    painter,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(600.0, 400.0)),
                    &view,
                );
            });
            output.textures_delta.clear();
            let mut up = 0;
            let mut down = 0;
            let mut lines = 0;
            for shape in &output.shapes {
                match &shape.shape {
                    Shape::Rect(body) => {
                        let rising =
                            body.fill == Color32::from_rgba_unmultiplied(184, 227, 220, 128);
                        let (fill, border, count) = if rising {
                            ([184, 227, 220], [8, 153, 129], &mut up)
                        } else {
                            ([237, 165, 166], [242, 54, 69], &mut down)
                        };
                        assert_eq!(
                            body.fill,
                            Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], 128)
                        );
                        assert_eq!(
                            body.stroke.color,
                            Color32::from_rgba_unmultiplied(border[0], border[1], border[2], 128)
                        );
                        assert_eq!(body.stroke.width, 1.0);
                        let price = f64::from((body.rect.center().y - 200.0) * direction / 10.0);
                        // The path is 10..11, displaced by at most 5 ticks of 0.25.
                        assert!((8.7..=12.3).contains(&price), "tick-scaled body: {price}");
                        *count += 1;
                    }
                    Shape::LineSegment { stroke, .. } => {
                        assert_eq!(stroke.width, 1.0);
                        assert_eq!(
                            stroke.color,
                            Color32::from_rgba_unmultiplied(128, 128, 128, 128)
                        );
                        lines += 1;
                    }
                    shape => panic!("unexpected ghost decoration: {shape:?}"),
                }
            }
            assert!(up > 0 && down > 0);
            assert_eq!(up + down, 32);
            assert_eq!(lines, 33);
        }
    }
}
