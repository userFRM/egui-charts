//! Chart Type Rendering
//!
//! Renders all chart visualization types (the `candles` directory name is
//! historical; this module handles every `ChartType` variant):
//!
//! - Standard: Candlestick, Bars, Hollow Candles, Volume Candles
//! - Line-based: Line, Line with Markers, Step Line
//! - Area-based: Area, HLC Area, Baseline, Columns
//! - Japanese: Renko, Kagi, Line Break, Heikin-Ashi
//! - Range: High-Low, Range Bars, Point & Figure
//! - Advanced: TPO

mod area;
mod candlestick;
mod heikin_ashi;
mod helpers;
mod japanese;
mod line;
mod params;
pub(crate) mod price_based;
mod range;
pub mod tpo;
pub(crate) mod volume;

pub use params::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
};

use crate::model::ChartType;
use crate::styles::typography;

/// Renders all chart types
///
/// # Parameters
/// - `chart_type`: Which chart type to render
/// - `ctx`: Core rendering context (price_ctx, volume_ctx, price_scale, colors, data)
/// - `params`: Chart rendering parameters (dimensions, colors, settings)
/// - `idx_to_coord`: Function to map bar index to x coordinate
pub fn render_chart_type(
    chart_type: ChartType,
    ctx: &CandleDataContext,
    params: &ChartTypeParams<'_>,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    let price_rect = ctx.price_ctx.rect;
    let painter = ctx.price_ctx.painter;

    if let Some(message) = params.type_params.invalid(chart_type) {
        painter.text(
            price_rect.center(),
            egui::Align2::CENTER_CENTER,
            message,
            egui::FontId::proportional(typography::LG),
            ctx.colors.text,
        );
        return;
    }

    let x_of = |i| idx_to_coord(ctx.start_idx + i, params.chart_rect_min_x());

    match chart_type {
        // === Standard OHLC Types ===
        ChartType::Candles => {
            candlestick::render_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
            );
        }
        ChartType::Heikin => {
            candlestick::render_heikin_ashi(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.full_data,
                ctx.revision,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Bars => {
            candlestick::render_ohlc_bars(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
            );
        }
        ChartType::HollowCandles => {
            candlestick::render_hollow_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.full_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::VolumeCandles => {
            candlestick::render_volume_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }

        // === Line-Based Types ===
        ChartType::Line => {
            line::render_line(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.line,
                params.price_source(),
            );
        }
        ChartType::LineWithMarkers => {
            line::render_line_with_markers(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.line_with_markers,
                params.price_source(),
                params.type_params.styles.marker_radius,
            );
        }
        ChartType::StepLine => {
            line::render_step_line(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.step_line,
                params.price_source(),
            );
        }

        // === Area-Based Types ===
        ChartType::Area => {
            area::render_area(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.area,
                params.price_source(),
            );
        }
        ChartType::HlcArea => {
            area::render_hlc_area(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
                params.price_source(),
            );
        }
        ChartType::Columns => {
            candlestick::render_columns(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.full_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Baseline => {
            area::render_baseline(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.baseline,
                params.price_source(),
                params.type_params.baseline,
            );
        }

        // === Range Types ===
        ChartType::HighLow => {
            range::render_high_low(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Range => {
            let built = price_based::range_bars(ctx.full_data, ctx.revision, params.type_params);
            range::render_range_bars(
                painter,
                built.shown(ctx.start_idx, ctx.visible_data.len()),
                params.bar_width(),
                ctx.price_scale,
                params.bullish_color(),
                params.bearish_color(),
                x_of,
            );
        }

        // === Japanese Chart Types ===
        ChartType::Renko => {
            let built = price_based::renko(ctx.full_data, ctx.revision, params.type_params);
            japanese::render_renko(
                painter,
                built.shown(ctx.start_idx, ctx.visible_data.len()),
                params.type_params.renko_wicks,
                params.bar_width(),
                ctx.price_scale,
                params.bullish_color(),
                params.bearish_color(),
                x_of,
            );
        }
        ChartType::Kagi => {
            let built = price_based::kagi(ctx.full_data, ctx.revision, params.type_params);
            japanese::render_kagi(
                painter,
                built.shown(ctx.start_idx, ctx.visible_data.len()),
                ctx.price_scale,
                params.bullish_color(),
                params.bearish_color(),
                x_of,
            );
        }
        ChartType::LineBreak => {
            let built = price_based::line_break(ctx.full_data, ctx.revision, params.type_params);
            japanese::render_line_break(
                painter,
                built.shown(ctx.start_idx, ctx.visible_data.len()),
                params.bar_width(),
                ctx.price_scale,
                params.bullish_color(),
                params.bearish_color(),
                x_of,
            );
        }
        ChartType::PointAndFigure => {
            let built =
                price_based::point_and_figure(ctx.full_data, ctx.revision, params.type_params);
            japanese::render_point_and_figure(
                painter,
                price_rect,
                built.shown(ctx.start_idx, ctx.visible_data.len()),
                params.bar_width(),
                ctx.price_scale,
                params.bullish_color(),
                params.bearish_color(),
                x_of,
            );
        }

        // === Advanced Types ===
        ChartType::TimePriceOpportunity => {
            let tpo_config = params.type_params.tpo_config(ctx.tick_size);
            tpo::render_live_tpo(
                painter,
                price_rect,
                ctx.full_data,
                ctx.start_idx..ctx.start_idx + ctx.visible_data.len(),
                ctx.revision,
                ctx.price_scale,
                &tpo_config,
                ctx.clock,
                &idx_to_coord,
                params.chart_rect_min_x(),
                params.bullish_color(),
                params.bearish_color(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarData, ChartType, LineStyle};
    use crate::widget::Chart;
    use egui::{Color32, Context, RawInput, Rect, Shape, pos2, vec2};

    #[test]
    fn chart_type_styles_paint_and_survive_switching_and_serialization() {
        let time = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let bars = [100.0, 110.0, 90.0, 105.0]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(time + chrono::Duration::minutes(i as i64), c, c, c, c, 1.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.config_mut().show_volume = false;
        let candle_color = chart.config().bullish_color;
        let mut settings = chart.series_settings();
        let styles = &mut settings.chart_type_params.styles;
        let color = Color32::from_rgb(119, 17, 199);
        for line in [
            &mut styles.line,
            &mut styles.step_line,
            &mut styles.line_with_markers,
            &mut styles.area.line,
            &mut styles.baseline.above,
            &mut styles.baseline.below,
        ] {
            line.color = color;
            line.width = 3.5;
        }
        styles.line.style = LineStyle::Dashed;
        styles.step_line.style = LineStyle::Dotted;
        styles.marker_radius = 8.0;
        styles.area.top_fill = Color32::from_rgba_unmultiplied(255, 0, 0, 200);
        styles.area.bottom_fill = Color32::from_rgba_unmultiplied(0, 0, 255, 40);
        styles.baseline.above_fill = Color32::from_rgba_unmultiplied(255, 255, 0, 100);
        styles.baseline.below_fill = Color32::from_rgba_unmultiplied(0, 255, 255, 80);
        chart.apply_series_settings(&settings);
        assert_eq!(chart.config().bullish_color, candle_color);
        let json = serde_json::to_string(chart.chart_type_params()).unwrap();
        let restored = serde_json::from_str(&json).unwrap();
        chart.set_chart_type_params(restored);

        for kind in [
            ChartType::Line,
            ChartType::StepLine,
            ChartType::LineWithMarkers,
            ChartType::Area,
            ChartType::Baseline,
            ChartType::Candles,
            ChartType::Line,
        ] {
            chart.set_chart_type(kind);
            assert_eq!(
                chart.series_settings().chart_type_params,
                settings.chart_type_params
            );
            let ctx = Context::default();
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0))),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            );
            let has_line = output.shapes.iter().any(|s| match &s.shape {
                Shape::Path(path) => {
                    path.stroke.width == 3.5
                        && path.stroke.color == egui::epaint::ColorMode::Solid(color)
                }
                Shape::LineSegment { stroke, .. } => stroke.width == 3.5 && stroke.color == color,
                _ => false,
            });
            let markers = output
                .shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape,
                Shape::Circle(circle) if circle.radius == 8.0 && circle.fill == color)
                })
                .count();
            let has_bottom = output.shapes.iter().any(|s| matches!(&s.shape,
                Shape::Mesh(mesh) if mesh.vertices.iter().any(|v| v.color == settings.chart_type_params.styles.area.bottom_fill)));
            let baseline_fills = output.shapes.iter().filter(|s| matches!(&s.shape,
                Shape::Path(path) if path.fill == settings.chart_type_params.styles.baseline.above_fill
                    || path.fill == settings.chart_type_params.styles.baseline.below_fill)).count();
            output.drop_without_applying_deltas();
            if kind != ChartType::Candles {
                assert!(has_line, "{kind:?} ignored its line style");
            }
            if kind == ChartType::LineWithMarkers {
                assert_eq!(markers, 4);
            }
            if kind == ChartType::Area {
                assert!(has_bottom, "custom bottom fill was lost");
            }
            if kind == ChartType::Baseline {
                assert!(baseline_fills >= 2, "baseline fills were lost");
            }
        }
        // Subpixel segments still carry a continuous 8-on, 4-off pattern: 22 points of ink
        // over a 30-point path, independent of how densely its points are sampled.
        let ctx = Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::line::draw_styled_line(
                ui.painter(),
                (0..=60).map(|i| pos2(i as f32 / 2.0, 50.0)).collect(),
                &settings.chart_type_params.styles.line,
            );
        });
        let ink: f32 = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let Shape::LineSegment { points, stroke } = &s.shape {
                    (stroke.color == color).then_some(points[1].x - points[0].x)
                } else {
                    None
                }
            })
            .sum();
        output.drop_without_applying_deltas();
        assert_eq!(ink, 22.0, "dash phase must survive short segments");
        let mut reset = chart.series_settings();
        reset.reset_to_default();
        assert_eq!(
            reset.chart_type,
            ChartType::Line,
            "reset must keep the dialog's active type"
        );
    }

    #[test]
    fn a_line_breaks_at_whitespace_and_takes_the_colour_of_the_bar_it_starts_at() {
        let red = Color32::RED;
        let mut bars: Vec<Bar> = (0..6)
            .map(|i| Bar::from_value(chrono::DateTime::UNIX_EPOCH, i as f64))
            .collect();
        bars[1].colors = Some(Box::new(crate::model::BarColors {
            color: Some(red),
            ..Default::default()
        }));
        bars[3] = Bar::whitespace(bars[3].time);
        let points: Vec<_> = (0..6).map(|i| pos2(i as f32 * 10.0, 50.0)).collect();
        let style = crate::model::LinePlotStyle::default();
        let output = Context::default().run_ui(RawInput::default(), |ui| {
            super::line::draw_bar_line(ui.painter(), &bars, &points, &style, false);
        });
        let paths: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Path(path) => Some((
                    path.stroke.color.clone(),
                    path.points.iter().map(|p| p.x).collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        let solid = egui::epaint::ColorMode::Solid;
        assert_eq!(
            paths,
            [
                (solid(style.color), vec![0.0, 10.0]),
                (solid(red), vec![10.0, 20.0]),
                (solid(style.color), vec![40.0, 50.0]),
            ]
        );
    }
}
