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
mod bar;
mod candle;
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

pub use bar::render_ohlc_bar;
pub use candle::render_candle;

use crate::theme::ThemeExt;
pub use heikin_ashi::HeikinAshiCache;
pub(crate) use heikin_ashi::rising as heikin_rising;
pub(crate) use helpers::prev_close;
pub(crate) use line::{LAST_PRICE_PULSE_SECONDS, render_crosshair_marker, render_last_price_pulse};
pub use params::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
};

use crate::model::{BaselineLevel, ChartType, ChartTypeStyles};
use crate::scales::PriceMap;

/// The line colour of a line-based chart type at `value`, or `None` for other chart types. The
/// Baseline chart takes the colour of the side of `baseline` that `value` lies on, in the price
/// pane `rect` mapped by `price_map`.
pub(crate) fn line_color_at(
    chart_type: ChartType,
    styles: &ChartTypeStyles,
    baseline: BaselineLevel,
    price_map: &PriceMap,
    rect: egui::Rect,
    value: f64,
    bar: &crate::model::Bar,
) -> Option<egui::Color32> {
    if bar.is_whitespace() {
        return None;
    }
    let own = bar.colors.as_deref();
    let color = own.and_then(|colors| colors.color);
    match chart_type {
        ChartType::Line => Some(color.unwrap_or(styles.line.color)),
        ChartType::StepLine => Some(color.unwrap_or(styles.step_line.color)),
        ChartType::LineWithMarkers => Some(color.unwrap_or(styles.line_with_markers.color)),
        ChartType::Area => Some(color.unwrap_or(styles.area.line.color)),
        ChartType::Baseline => {
            let base = match baseline {
                BaselineLevel::Price(price) => price,
                BaselineLevel::PercentOfPane(percent) => {
                    price_map.y_to_price(rect.max.y - percent / 100.0 * rect.height())
                }
            };
            Some(if value >= base {
                own.and_then(|colors| colors.top_line)
                    .unwrap_or(styles.baseline.above.color)
            } else {
                own.and_then(|colors| colors.bottom_line)
                    .unwrap_or(styles.baseline.below.color)
            })
        }
        _ => None,
    }
}

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
            egui::FontId::proportional(painter.ctx().active_theme().text.title),
            ctx.colors.text,
        );
        return;
    }

    let x_of = |i| idx_to_coord(ctx.start_idx + i, params.chart_rect_min_x());

    match chart_type {
        // === Standard OHLC Types ===
        // Execution-based types draw candles for the bars without executions.
        ChartType::Candles | ChartType::VolumeFootprint | ChartType::SessionVolumeProfile => {
            candlestick::render_candles(
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
                &params.type_params.styles.candles,
            );
        }
        ChartType::Heikin => {
            heikin_ashi::render(ctx, params, &idx_to_coord);
        }
        ChartType::Bars => {
            candlestick::render_ohlc_bars(
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
                &params.type_params.styles.bars,
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
                &params.type_params.styles.hollow,
            );
        }
        ChartType::VolumeCandles => {
            candlestick::render_volume_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.full_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.volume_candles,
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
                &params.type_params.styles.hlc_area,
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
                params.type_params.columns_base,
                params.type_params.styles.columns.price_source,
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
                ctx.price_formatter,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &params.type_params.styles.high_low,
            );
        }
        ChartType::Range => {
            let built;
            let shown = if ctx.shows_elements {
                ctx.visible_data
            } else {
                built = price_based::range_bars(ctx.full_data, ctx.revision, params.type_params);
                built.shown_bars(ctx.start_idx, ctx.visible_data.len())
            };
            range::render_range_bars(
                painter,
                shown,
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
                params.type_params.styles.kagi,
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
                ctx.tick_size,
                ctx.clock,
                &idx_to_coord,
                params.chart_rect_min_x(),
                params.bullish_color(),
                params.bearish_color(),
            );
            line::render_line(
                painter,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                false,
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                &crate::model::LinePlotStyle {
                    color: painter.ctx().active_theme().ui.accent,
                    width: crate::tokens::DESIGN_TOKENS.stroke.hairline,
                    ..Default::default()
                },
                crate::model::PriceSource::Close,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarData, ChartType, ChartTypeParams, LineStyle, Timeframe};
    use crate::widget::Chart;
    use egui::{Color32, Context, RawInput, Rect, Shape, pos2, vec2};
    use std::collections::BTreeSet;

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
        styles.area.marker_radius = 8.0;
        styles.baseline.marker_radius = 8.0;
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
            let baseline = &settings.chart_type_params.styles.baseline;
            let baseline_fills = [baseline.above_fill, baseline.below_fill]
                .into_iter()
                .filter(|fill| {
                    output.shapes.iter().any(|s| {
                        matches!(&s.shape,
                        Shape::Mesh(mesh) if mesh.vertices.iter().any(|v| v.color == *fill))
                    })
                })
                .count();
            output.drop_without_applying_deltas();
            if kind != ChartType::Candles {
                assert!(has_line, "{kind:?} ignored its line style");
            }
            if matches!(
                kind,
                ChartType::LineWithMarkers | ChartType::Area | ChartType::Baseline
            ) {
                assert_eq!(markers, 4);
            }
            if kind == ChartType::Area {
                assert!(has_bottom, "custom bottom fill was lost");
            }
            if kind == ChartType::Baseline {
                assert_eq!(baseline_fills, 2, "baseline fills were lost");
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
    fn tpo_chart_type_draws_the_display_of_its_params() {
        let time = chrono::DateTime::parse_from_rfc3339("2024-01-02T14:30:00Z")
            .unwrap()
            .to_utc();
        let bars = (0..8)
            .map(|i| {
                let low = 100.0 + f64::from(i);
                Bar::new(
                    time + chrono::Duration::minutes(i64::from(i) * 30),
                    low + 0.25,
                    low + 1.0,
                    low,
                    low + 0.75,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.config_mut().show_volume = false;
        chart.set_timeframe(Timeframe::Min30);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let single_print = crate::tokens::DESIGN_TOKENS.semantic.tpo.single_print;
        let mut draw = |params: ChartTypeParams| {
            chart.set_chart_type_params(params);
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
            let letters: BTreeSet<String> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(text)
                        if text.galley.text().len() == 1
                            && text.galley.text().chars().all(|c| c.is_ascii_uppercase()) =>
                    {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            let captions = output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Mid"))
                .count();
            let highlights = output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == single_print))
                .count();
            let texts = |wanted: &str| {
                output
                    .shapes
                    .iter()
                    .filter(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == wanted))
                    .count()
            };
            let rotation = (texts("Rotation"), texts("+14"));
            output.drop_without_applying_deltas();
            (
                letters.into_iter().collect::<String>(),
                captions,
                highlights,
                rotation,
            )
        };

        // Settings saved before the options existed keep the blocks and midpoint they were saved
        // with.
        let saved = serde_json::from_str::<ChartTypeParams>("{}").unwrap();
        assert_eq!(draw(saved.clone()), (String::new(), 1, 0, (0, 0)));
        // A new chart letters its blocks and leaves the midpoint off.
        assert_eq!(
            draw(ChartTypeParams::default()),
            ("ABCDEFGH".to_owned(), 0, 0, (0, 0))
        );
        // Eight half-hour bars of one session are the periods A to H.
        let lettered = ChartTypeParams {
            tpo_letters: true,
            ..saved.clone()
        };
        assert_eq!(draw(lettered).0, "ABCDEFGH");
        let (_, captions, highlights, _) = draw(ChartTypeParams {
            tpo_midpoint: true,
            tpo_single_prints: true,
            ..saved.clone()
        });
        assert_eq!(captions, 1);
        assert!(highlights > 0);
        // Each of the seven periods after the first rises a row at both ends.
        let statistics = draw(ChartTypeParams {
            tpo_statistics: true,
            ..saved
        });
        assert_eq!(statistics.3, (1, 1));
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

    #[test]
    fn area_and_baseline_point_markers_follow_each_series_points_and_visibility() {
        use crate::model::{BarColors, BaselineLevel};
        use crate::widget::{SeriesId, SeriesOptions};

        let bars: Vec<_> = [70.0, 30.0, f64::NAN, 50.0, f64::NAN, 100.0]
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i as i64);
                if value.is_nan() {
                    Bar::whitespace(time)
                } else {
                    let mut bar = Bar::from_value(time, value);
                    bar.colors = Some(Box::new(BarColors {
                        color: (i == 0).then_some(Color32::GOLD),
                        top_line: (i == 0).then_some(Color32::CYAN),
                        bottom_line: (i == 1).then_some(Color32::MAGENTA),
                        ..Default::default()
                    }));
                    bar
                }
            })
            .collect();
        for kind in [ChartType::Area, ChartType::Baseline] {
            for inverted in [false, true] {
                for (radius, added_radius, main_visible, added_visible) in [
                    (0.0, None, true, true),
                    (4.0, Some(7.0), true, true),
                    (4.0, Some(0.0), true, true),
                    (0.0, Some(7.0), true, true),
                    (-1.0, Some(-2.0), true, true),
                    (f32::NAN, Some(f32::NAN), true, true),
                    (f32::INFINITY, Some(f32::INFINITY), true, true),
                    (4.0, Some(7.0), false, true),
                    (4.0, Some(7.0), true, false),
                ] {
                    let mut chart = Chart::new(BarData::from_bars(bars.clone()));
                    chart.config_mut().show_volume = false;
                    chart.set_chart_type(kind);
                    let mut params = chart.chart_type_params().clone();
                    params.baseline = BaselineLevel::Price(50.0);
                    params.styles.area.marker_radius = radius;
                    params.styles.baseline.marker_radius = radius;
                    let area_color = params.styles.area.line.color;
                    let above_color = params.styles.baseline.above.color;
                    chart.set_chart_type_params(params);
                    let added = chart.add_series(SeriesOptions {
                        chart_type: kind,
                        color: area_color,
                        up_color: above_color,
                        marker_radius: added_radius,
                        ..Default::default()
                    });
                    chart.set_data(added, bars.clone());
                    chart.set_series_visible(SeriesId::MAIN, main_visible);
                    chart.set_series_visible(added, added_visible);
                    let scale = chart.series_price_scale_mut(SeriesId::MAIN).unwrap();
                    scale.set_range(0.0, 100.0);
                    scale.options_mut().invert_scale = inverted;
                    let output = Context::default().run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                pos2(0.0, 0.0),
                                vec2(600.0, 400.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                        },
                    );
                    let rect = chart.get_rendered_price_rect();
                    let shapes = output.shapes.clone();
                    output.drop_without_applying_deltas();
                    for (id, radius, visible) in [
                        (SeriesId::MAIN, radius, main_visible),
                        (added, added_radius.unwrap_or(0.0), added_visible),
                    ] {
                        let circles: Vec<_> = shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                Shape::Circle(circle)
                                    if circle.radius == radius
                                        && circle.fill != Color32::TRANSPARENT =>
                                {
                                    assert_eq!(shape.clip_rect, rect);
                                    Some((circle.center, circle.fill))
                                }
                                _ => None,
                            })
                            .collect();
                        let expected: Vec<_> = if visible && radius.is_finite() && radius > 0.0 {
                            // In a 0..100 range, 70 is 30% down the pane and 30 is 70% down;
                            // inversion swaps them. Isolated points at 50 and 100 sit halfway
                            // and on the pane edge, where the marker is clipped in half.
                            [(0, 70.0_f32), (1, 30.0), (3, 50.0), (5, 100.0)]
                                .into_iter()
                                .map(|(i, value)| {
                                    let fraction = if inverted {
                                        value / 100.0
                                    } else {
                                        1.0 - value / 100.0
                                    };
                                    let point = pos2(
                                        chart.index_to_x(i as f64).unwrap(),
                                        rect.top() + rect.height() * fraction,
                                    );
                                    let color = match (kind, i) {
                                        (ChartType::Area, 0) => Color32::GOLD,
                                        (ChartType::Area, _) => area_color,
                                        (_, 0) => Color32::CYAN,
                                        (_, 1) => Color32::MAGENTA,
                                        _ => above_color,
                                    };
                                    (point, color)
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        assert_eq!(circles.len(), expected.len(), "{kind:?}, {id:?}, {radius}");
                        for ((point, color), (expected_point, expected_color)) in
                            circles.iter().zip(expected)
                        {
                            assert!(
                                point.distance(expected_point) < 0.001,
                                "{kind:?}, {id:?}: {point:?} != {expected_point:?}"
                            );
                            assert_eq!(*color, expected_color);
                        }
                    }
                }
            }
        }
    }
}
