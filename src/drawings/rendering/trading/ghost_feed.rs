//! Ghost feed rendering.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Project candles in instrument ticks along a thin neutral path, in the config's body,
    /// border and wick colours faded by its transparency.
    pub(crate) fn render_ghost_feed(&self, painter: &egui::Painter, view: &ChartView) {
        // Without tick metadata, one price unit is one tick.
        let tick_size = view
            .instrument
            .and_then(|i| i.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .unwrap_or(1.0);
        let neutral = Color32::from_rgba_unmultiplied(128, 128, 128, 128);
        let style = self.ghost_feed.unwrap_or_default();
        let opacity = u16::from(100 - style.transparency.min(100));
        // Rounded half up, so that 50% fades an opaque colour to the 128 saved feeds were drawn at.
        let color = |[r, g, b, a]: [u8; 4]| {
            let a = (u16::from(a) * opacity + 50) / 100;
            Color32::from_rgba_unmultiplied(r, g, b, a as u8)
        };
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
                (style.body_up, style.border_up)
            } else {
                (style.body_down, style.border_down)
            };
            if style.show_wick {
                let wick = Stroke::new(1.0, color(style.wick_color));
                painter.vline(x, high.min(low)..=high.max(low), wick);
            }
            let rect = Rect::from_min_max(
                Pos2::new(x - half_width, open.min(close)),
                Pos2::new(x + half_width, open.max(close).max(open.min(close) + 1.0)),
            );
            let border = if style.show_border {
                Stroke::new(1.0, color(border))
            } else {
                Stroke::NONE
            };
            painter.rect(rect, 0.0, color(body), border, egui::StrokeKind::Inside);
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
    use crate::drawings::{ChartPoint, DrawingToolType, GhostFeedConfig};
    use egui::Shape;

    /// The feed's settings, then the rising and falling body colours, border colours and wick
    /// colour it paints, each `None` when switched off.
    type Row = (
        fn(&mut Option<GhostFeedConfig>),
        [[u8; 4]; 2],
        Option<[[u8; 4]; 2]>,
        Option<[u8; 4]>,
    );

    /// Candles take the body and border colours of their direction and the wick colour, faded by
    /// the transparency, and never the drawing's own fill or width. Feeds saved without these
    /// settings keep the colours they were drawn in, and the switches remove exactly the borders
    /// and the wicks; the path between the anchors stays a neutral hairline.
    #[test]
    fn ghost_candles_keep_direction_colours_without_decorations() {
        fn styled(feed: &mut Option<GhostFeedConfig>) -> &mut GhostFeedConfig {
            let config = feed.insert(GhostFeedConfig::default());
            config.body_up = [38, 166, 154, 255];
            config.body_down = [239, 83, 80, 255];
            config.border_up = [0, 0, 255, 255];
            config.border_down = [255, 165, 0, 255];
            config.wick_color = [255, 0, 0, 200];
            config
        }
        const SAVED: &str = r#"{"average_range_ticks":10.0,"noise_ticks":5.0}"#;
        let grey = [128, 128, 128, 128];
        let borders = Some([[8, 153, 129, 128], [242, 54, 69, 128]]);
        let rows: [Row; 5] = [
            // Saved before candle colours, without a config or with the one the release before
            // saved.
            (
                |feed| *feed = None,
                [[184, 227, 220, 128], [237, 165, 166, 128]],
                borders,
                Some(grey),
            ),
            (
                |feed| *feed = serde_json::from_str(SAVED).unwrap(),
                [[184, 227, 220, 128], [237, 165, 166, 128]],
                borders,
                Some(grey),
            ),
            // As placed.
            (
                |_| {},
                [[172, 229, 220, 128], [250, 161, 164, 128]],
                borders,
                Some(grey),
            ),
            // 75% of each alpha, rounded.
            (
                |feed| styled(feed).transparency = 25,
                [[38, 166, 154, 191], [239, 83, 80, 191]],
                Some([[0, 0, 255, 191], [255, 165, 0, 191]]),
                Some([255, 0, 0, 150]),
            ),
            (
                |feed| {
                    let config = styled(feed);
                    (config.show_border, config.show_wick) = (false, false);
                    config.transparency = 0;
                },
                [[38, 166, 154, 255], [239, 83, 80, 255]],
                None,
                None,
            ),
        ];
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        for (row, (setup, fills, borders, wick)) in rows.into_iter().enumerate() {
            for direction in [-1.0, 1.0] {
                let mut drawing = Drawing::new(1404, DrawingToolType::GhostFeed);
                setup(&mut drawing.ghost_feed);
                drawing.chart_points =
                    vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(32.0, 11.0)];
                drawing.stroke_width = 4.0;
                drawing.fill_color = Some([255, 0, 255, 255]);
                let bar_to_x = |b| 20.0 + b * 10.0;
                let price_to_y = |p| 200.0 + direction * p as f32 * 10.0;
                drawing.update_screen_coords(bar_to_x, price_to_y);
                let instrument = crate::model::Instrument::new("TEST").with_tick_size(0.25);
                let view = ChartView {
                    price_formatter: None,
                    thousands_sep: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: Some(&instrument),
                    bars: &[],
                    revision: 0,
                    bar_to_x: &bar_to_x,
                    bar_spacing: 10.0,
                    price_to_y: &price_to_y,
                    y_to_price: &|y| f64::from((y - 200.0) / (direction * 10.0)),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
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
                let (mut up, mut down, mut wicks, mut paths) = (0, 0, 0, 0);
                for shape in &output.shapes {
                    match &shape.shape {
                        Shape::Rect(body) => {
                            let at = usize::from(body.fill != rgba(fills[0]));
                            assert_eq!(body.fill, rgba(fills[at]), "row {row}");
                            assert_eq!(
                                body.stroke,
                                borders.map_or(Stroke::NONE, |b| Stroke::new(1.0, rgba(b[at]))),
                                "row {row}"
                            );
                            let price =
                                f64::from((body.rect.center().y - 200.0) * direction / 10.0);
                            // The path is 10..11, displaced by at most the noise in ticks of
                            // 0.25, and 0.05 for the 1 px least body.
                            let reach =
                                drawing.ghost_feed.unwrap_or_default().noise_ticks * 0.25 + 0.05;
                            assert!(
                                (10.0 - reach..=11.0 + reach).contains(&price),
                                "tick-scaled body: {price}"
                            );
                            if at == 0 {
                                up += 1;
                            } else {
                                down += 1;
                            }
                        }
                        Shape::LineSegment { points, stroke } => {
                            if points[..] == drawing.points[..] {
                                assert_eq!(*stroke, Stroke::new(1.0, rgba(grey)), "row {row}");
                                paths += 1;
                            } else {
                                assert_eq!(
                                    Some(*stroke),
                                    wick.map(|c| Stroke::new(1.0, rgba(c))),
                                    "row {row}"
                                );
                                wicks += 1;
                            }
                        }
                        shape => panic!("unexpected ghost decoration: {shape:?}"),
                    }
                }
                assert!(up > 0 && down > 0, "row {row}");
                assert_eq!(up + down, 32, "row {row}");
                assert_eq!(
                    (wicks, paths),
                    (if wick.is_some() { 32 } else { 0 }, 1),
                    "row {row}"
                );
            }
        }
    }
}
