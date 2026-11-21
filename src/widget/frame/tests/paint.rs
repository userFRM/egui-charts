use crate::widget::test_frames::{candle_ys, chart, frame, shapes};
use egui::{Pos2, Rect, Shape};

#[test]
fn news_badge_has_a_distinct_icon_and_colour() {
    use crate::model::{EventKind, TimeScaleEvent};
    use crate::widget::SeriesId;
    for dark in [false, true] {
        let ctx = egui::Context::default();
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let mut chart = chart(&[100.0], &[110.0]);
        let kinds = [EventKind::News, EventKind::Economic, EventKind::Earnings];
        let events = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                TimeScaleEvent::new(
                    kind.label(),
                    chart.bars(SeriesId::MAIN)[90 + index * 2].time,
                    *kind,
                    kind.label(),
                )
            })
            .collect();
        chart.set_time_scale_events(events);
        let mut visibility = chart.event_visibility().clone();
        visibility.show_earnings = true;
        visibility.show_economic_events = true;
        chart.set_event_visibility(visibility);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let badges: Vec<_> = kinds
            .iter()
            .map(|kind| {
                painted
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.text() == kind.label() => Some((
                            text.pos + text.galley.size() * 0.5,
                            text.galley.job.sections[0].format.color,
                        )),
                        _ => None,
                    })
                    .expect("event badge")
            })
            .collect();
        assert_ne!(badges[0].1, badges[1].1);
        assert_ne!(badges[0].1, badges[2].1);
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
            if path.closed && path.points.len() == 4 && path.visual_bounding_rect().contains(badges[0].0))));
        for (centre, _) in &badges[1..] {
            assert!(painted.iter().any(|shape| matches!(shape, Shape::Circle(circle) if circle.center.distance(*centre) < 1.0)));
        }
    }
}

/// Each chart of a layout carries the replay mark exactly while it replays itself.
#[test]
fn legend_marks_a_chart_only_while_it_replays() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut chart = chart(&[100.0, 101.0, 102.0], &[110.0, 111.0, 112.0]);
    let cutoff = chart.bars(crate::widget::SeriesId::MAIN)[2].time;
    chart.set_replay_time(Some(cutoff));
    let mut harness = Harness::builder()
        .allow_missing_glyphs()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, chart: &mut crate::widget::Chart| {
                chart.show(ui);
            },
            chart,
        );
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Image, "Replay")
            .is_some()
    );
    harness.state_mut().set_replay_time(None);
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Image, "Replay")
            .is_none()
    );
}

/// Renders `chart` with a footprint of one two-lot buy at every tick of every bar, the example
/// of #2458, aggregated `input` ticks a row, at `spacing` points per bar, and returns the shapes
/// with the chart.
fn host_footprint(
    mut chart: crate::widget::Chart,
    config: &crate::chart::overlays::FootprintConfig,
    input: u32,
    spacing: f32,
) -> (Vec<egui::epaint::ClippedShape>, crate::widget::Chart) {
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    let ctx = egui::Context::default();
    chart.set_symbol("FLOW");
    chart.apply_synced_time_scale(spacing, 1.0);
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, input).unwrap();
    for bar in chart.bars(crate::widget::SeriesId::MAIN) {
        for price in bar.low as i32..=bar.high as i32 {
            flow.push(&Tick::new(
                bar.time,
                f64::from(price),
                2.0,
                Some(OrderSide::Buy),
            ))
            .unwrap();
        }
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 500.0))),
            ..Default::default()
        },
        |ui| {
            chart.show_footprint(ui, &flow, config);
        },
    );
    output.textures_delta.clear();
    (std::mem::take(&mut output.shapes), chart)
}

/// Asserts that every bar of `chart` with a low of `low` whose column lies in the plot writes its
/// delta and total below `edge`, the bottom edge of its lowest row, and inside the plot.
fn assert_lowest_summaries_in_plot(
    shapes: &[egui::epaint::ClippedShape],
    chart: &crate::widget::Chart,
    low: f64,
    edge: f64,
    case: &str,
) {
    let map = chart.get_chart_mapping();
    let plot = chart.get_rendered_price_rect();
    let edge = map.price_to_y(edge);
    let boxes: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) if text.galley.text() == "Delta" => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .collect();
    assert!(
        boxes
            .iter()
            .any(|rect| rect.top() > edge && plot.contains_rect(*rect)),
        "{case}: plot bottom {}, low {low} at {}, row edge at {edge}, Delta at {boxes:?}",
        plot.bottom(),
        map.price_to_y(low)
    );
    for (index, bar) in chart.bars(crate::widget::SeriesId::MAIN).iter().enumerate() {
        let x = map.idx_to_x(index);
        let half = map.bar_spacing * 0.46;
        if bar.low != low || x - half < plot.left() || x + half > plot.right() {
            continue;
        }
        for label in ["Delta", "Total"] {
            assert!(
                shapes.iter().any(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == label => {
                        let rect = Rect::from_min_size(text.pos, text.galley.size());
                        (rect.center().x - x).abs() < half
                            && rect.top() > edge
                            && plot.contains_rect(rect)
                    }
                    _ => false,
                }),
                "{case}, low {low}: bar {index} keeps {label} below its lowest row"
            );
        }
    }
}

/// A low of 103 lies in the four-tick row 100..103, whose bottom edge is 99.5, whether
/// `row_ticks` groups one-tick input rows by four or the input rows hold four ticks each.
/// The fit must leave the summary's room below that edge, including with single-tick rows and
/// with automatic rows, which never group below `row_ticks` and ignore an unusable height.
/// Fixed single-tick rows fit their edges with a `row_ticks` of 0 as with 1, so without a bottom
/// scale margin the low of 100 keeps its summary too. Beside a bar reaching 250, without a bottom
/// scale margin, automatic rows group seven ticks, with a `row_ticks` of 0 or 1 too: the low of
/// 101 then lies in the row 98..104, whose bottom edge is 97.5.
#[test]
fn footprint_summary_below_a_grouped_lowest_row_stays_in_the_plot() {
    use crate::chart::overlays::FootprintConfig;
    for (auto_row_height, row_ticks, input, low, high, bottom, edge) in [
        (None, 1, 1, 100.0, 110.0, 0.1, 99.5),
        (None, 0, 1, 100.0, 110.0, 0.0, 99.5),
        (None, 4, 1, 103.0, 113.0, 0.1, 99.5),
        (Some(16.0), 4, 1, 103.0, 113.0, 0.1, 99.5),
        (Some(0.0), 4, 1, 103.0, 113.0, 0.1, 99.5),
        (Some(f32::NAN), 4, 1, 103.0, 113.0, 0.1, 99.5),
        (Some(16.0), 4, 1, 101.0, 250.0, 0.0, 97.5),
        (Some(16.0), 0, 1, 101.0, 250.0, 0.0, 97.5),
        (Some(16.0), 1, 1, 101.0, 250.0, 0.0, 97.5),
        (Some(16.0), 0, 4, 103.0, 113.0, 0.1, 99.5),
        (None, 0, 4, 103.0, 113.0, 0.1, 99.5),
    ] {
        let config = FootprintConfig {
            auto_row_height,
            row_ticks,
            ..Default::default()
        };
        let mut bars = chart(&[low, low + 4.0], &[low + 6.0, high]);
        bars.price_scale_mut().options_mut().scale_margins.bottom = bottom;
        let (shapes, chart) = host_footprint(bars, &config, input, 160.0);
        assert_lowest_summaries_in_plot(
            &shapes,
            &chart,
            low,
            edge,
            &format!("auto {auto_row_height:?}, row_ticks {row_ticks}, input {input}"),
        );
    }
}

/// Below one point of cell width every bar is a candle, so fixed rows must not widen the fit.
#[test]
fn footprint_drawn_as_candles_keeps_the_candle_fit() {
    use crate::chart::overlays::FootprintConfig;
    let lows: Vec<f32> = [0, 4]
        .into_iter()
        .map(|row_ticks| {
            let config = FootprintConfig {
                auto_row_height: None,
                row_ticks,
                ..Default::default()
            };
            let bars = chart(&[103.0, 107.0], &[109.0, 113.0]);
            let (_, chart) = host_footprint(bars, &config, 1, 3.0);
            chart.get_chart_mapping().price_to_y(103.0)
        })
        .collect();
    assert!(
        (lows[0] - lows[1]).abs() < 0.5,
        "low 103 at y {} with input rows, at y {} with four-tick rows",
        lows[0],
        lows[1]
    );
}

/// A plot 70 points high leaves 8 between its scale margins and the summary's room, half an
/// automatic row. No range holds the rows zoom groups there, since each wider fit groups more
/// ticks than the last, so the fit must stay the one fixed rows of `row_ticks` get.
#[test]
fn footprint_short_of_two_automatic_rows_keeps_the_minimum_group_fit() {
    use crate::chart::overlays::FootprintConfig;
    let ranges: Vec<_> = [None, Some(16.0)]
        .into_iter()
        .map(|auto_row_height| {
            let config = FootprintConfig {
                auto_row_height,
                row_ticks: 4,
                ..Default::default()
            };
            let mut bars = chart(&[101.0, 105.0], &[107.0, 250.0]);
            bars.set_size(Some(egui::vec2(1440.0, 105.0)));
            let (_, mut chart) = host_footprint(bars, &config, 1, 160.0);
            chart.price_scale_mut().range()
        })
        .collect();
    assert_eq!(
        ranges[0], ranges[1],
        "fixed four-tick rows, then automatic rows"
    );
}

/// Rows that zoom leaves one tick tall keep the candles' fit. Bars 100..110 in a plot 465 points
/// high, with a top scale margin of 46.5 points and none at the bottom, leave 370.5 points for
/// 10 ticks, rows of 37 points, so the low of 100 sits the summary's room, 48 points, above the
/// plot bottom; fitting the edge 99.5 of its row would lift it a further 16.8.
#[test]
fn one_tick_automatic_rows_keep_the_candle_fit() {
    use crate::chart::overlays::FootprintConfig;
    let mut bars = chart(&[100.0, 104.0], &[106.0, 110.0]);
    bars.price_scale_mut().options_mut().scale_margins.bottom = 0.0;
    let (_, chart) = host_footprint(bars, &FootprintConfig::default(), 1, 160.0);
    let plot = chart.get_rendered_price_rect();
    let low = chart.get_chart_mapping().price_to_y(100.0);
    assert!(
        (low - (plot.bottom() - 48.0)).abs() < 0.1,
        "low 100 at y {low}, plot bottom {}",
        plot.bottom()
    );
}

/// The Volume Footprint chart type's default parameters size its rows by zoom from one-tick
/// executions. Over bars alternating 101..107 and 105..250 without a bottom scale margin, as in
/// the summary test above, rows group seven ticks, so the low of 101 lies in the row 98..104,
/// whose bottom edge is 97.5, and the fit must leave the summary's room below that edge.
#[test]
fn zoom_sized_rows_keep_the_lowest_summary_in_the_plot() {
    use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest};
    use crate::model::{Bar, ChartType, Instrument, OrderSide, Tick, Timeframe};
    use crate::widget::ChartBuilder;
    use chrono::{DateTime, TimeDelta, Utc};
    /// 100 one-minute bars from 10:00 alternating 101..107 and 105..250.
    fn bars() -> Vec<Bar> {
        let start = "2026-01-05T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        (0..100)
            .map(|i| {
                let (low, high) = if i % 2 == 0 {
                    (101.0, 107.0)
                } else {
                    (105.0, 250.0)
                };
                Bar::new(start + TimeDelta::minutes(i), low, high, low, high, 1.0)
            })
            .collect()
    }
    struct Executions;
    impl Datafeed for Executions {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::new(bars(), true)));
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            // One two-lot buy at every tick of every bar.
            let trades = bars()
                .iter()
                .flat_map(|bar| {
                    (bar.low as i32..=bar.high as i32).map(move |price| {
                        Tick::new(bar.time, f64::from(price), 2.0, Some(OrderSide::Buy))
                    })
                })
                .collect();
            reply.send(Ok(TradePage::new(trades, true)));
        }
    }
    let ctx = egui::Context::default();
    let mut chart = ChartBuilder::new()
        .with_feed(std::sync::Arc::new(Executions))
        .with_symbol("FLOW")
        .with_timeframe(Timeframe::Min1)
        .with_chart_type(ChartType::VolumeFootprint)
        .build();
    chart.set_server_time(Some(
        "2026-01-05T11:41:00Z".parse::<DateTime<Utc>>().unwrap(),
    ));
    chart.price_scale_mut().options_mut().scale_margins.bottom = 0.0;
    let mut shapes = Vec::new();
    for _ in 0..5 {
        chart.apply_synced_time_scale(160.0, 1.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 500.0))),
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
            },
        );
        output.textures_delta.clear();
        shapes = output.shapes;
    }
    assert_lowest_summaries_in_plot(&shapes, &chart, 101.0, 97.5, "feed, default parameters");
}

#[test]
fn footprint_stays_below_legend_at_wide_and_narrow_sizes() {
    use crate::chart::overlays::{FootprintColor, FootprintConfig};
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use crate::widget::{Primitive, PrimitivePass, PrimitiveView};
    struct Underlay(egui::Color32);
    impl Primitive for Underlay {
        fn draw(&self, painter: &egui::Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            if pass == PrimitivePass::Below {
                painter.rect_filled(view.mapping.rect, 0.0, self.0);
            }
        }
    }
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for (width, pane) in [(1440.0, false), (600.0, false), (600.0, true)] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[101.0]);
            chart.set_symbol("FLOW");
            chart.apply_synced_time_scale(100.0, 1.0);
            let line_color = egui::Color32::from_rgb(191, 72, 216);
            chart.main_series_style.price_line_color = Some(line_color);
            let below_color = egui::Color32::from_rgb(93, 17, 126);
            chart
                .attach_primitive(crate::widget::SeriesId::MAIN, Underlay(below_color))
                .unwrap();
            #[cfg(feature = "studies")]
            if pane {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            }
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            for (i, bar) in chart.bars(crate::widget::SeriesId::MAIN).iter().enumerate() {
                for (price, size) in [(100.0, 2.0), (101.0, if i % 2 == 0 { 1.0 } else { 4.0 })] {
                    flow.push(&Tick::new(bar.time, price, size, Some(OrderSide::Buy)))
                        .unwrap();
                }
            }
            let config = FootprintConfig {
                modes: vec![crate::model::FootprintMode::BidAsk],
                auto_row_height: None,
                color_by: FootprintColor::Volume,
                abbreviate: false,
                text_color: theme.chart.axis_text,
                neutral_color: egui::Color32::from_rgb(21, 45, 78),
                ..Default::default()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 500.0))),
                    ..Default::default()
                },
                |ui| {
                    chart.show_footprint(ui, &flow, &config);
                },
            );
            output.textures_delta.clear();
            assert!(
                output.shapes.iter().any(|shape| match &shape.shape {
                    Shape::Rect(rect) => rect.fill == theme.chart.volume_up(),
                    _ => false,
                }),
                "footprint keeps the volume histogram"
            );
            let cells = output
                .shapes
                .iter()
                .position(|shape| match &shape.shape {
                    Shape::Mesh(mesh) => mesh
                        .vertices
                        .iter()
                        .any(|v| v.color == config.neutral_color.gamma_multiply(0.5)),
                    _ => false,
                })
                .expect("footprint cells");
            let Shape::Mesh(mesh) = &output.shapes[cells].shape else {
                unreachable!()
            };
            for text in output.shapes.iter().filter_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.text().contains('×') => Some(text),
                _ => None,
            }) {
                let bounds = Rect::from_min_size(text.pos, text.galley.size());
                assert!(
                    mesh.vertices.as_chunks::<4>().0.iter().any(|quad| {
                        let opaque = quad.iter().all(|v| v.color.a() == 255);
                        let backing = quad.iter().fold(Rect::NOTHING, |rect, vertex| {
                            rect.union(Rect::from_min_max(vertex.pos, vertex.pos))
                        });
                        opaque && backing.contains_rect(bounds)
                    }),
                    "opaque clearance beneath {}",
                    text.galley.text()
                );
            }
            let strokes: Vec<_> = output
                .shapes
                .iter()
                .enumerate()
                .filter_map(|(i, shape)| {
                    let is_line = match &shape.shape {
                        Shape::LineSegment { stroke, .. } => stroke.color == line_color,
                        Shape::Mesh(mesh) => mesh.vertices.iter().any(|v| v.color == line_color),
                        _ => false,
                    };
                    is_line.then_some(i)
                })
                .collect();
            assert!(!strokes.is_empty(), "current-price line remains visible");
            let below = output
                .shapes
                .iter()
                .position(
                    |shape| matches!(&shape.shape, Shape::Rect(rect) if rect.fill == below_color),
                )
                .expect("below-series content");
            assert!(
                strokes.iter().all(|i| below < *i),
                "current-price guide stays above below-series content"
            );
            assert!(
                strokes.iter().all(|i| *i < cells),
                "current-price line covers footprint cells"
            );
            for value in ["0 × 1", "0 × 4"] {
                assert!(
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    Shape::Text(text) if text.galley.text() == value)),
                    "ordinary and POC close rows retain {value}"
                );
            }
            let badge = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.fill == line_color => Some(rect.rect),
                    _ => None,
                })
                .expect("current-price badge retains its fill");
            assert!(
                output.shapes.iter().any(|shape| matches!(&shape.shape,
                Shape::Text(text) if text.galley.text().starts_with("101")
                    && badge.contains(text.pos + text.galley.size() * 0.5))),
                "current-price badge retains its value at width {width}, pane {pane}"
            );
            let legend = output
                .shapes
                .iter()
                .position(|shape| match &shape.shape {
                    Shape::Text(text) => text.galley.text().contains("FLOW"),
                    _ => false,
                })
                .expect("chart legend");
            assert!(
                cells < legend,
                "width {width}, pane {pane}: cells cover legend"
            );
        }
    }
}

/// The chart paints execution annotations and reserves a separate table; a live revisit
/// ends the originating zones and updates the table in the same frame.
#[cfg(feature = "studies")]
#[test]
fn footprint_annotations_and_statistics_follow_executions() {
    use crate::chart::overlays::FootprintConfig;
    use crate::data::Footprint;
    use crate::model::{Bar, BarClock, BarData, OrderSide, Tick, Timeframe};
    use crate::studies::FootprintStudyConfig;
    use crate::widget::{Chart, SeriesId};
    use chrono::TimeDelta;
    let start: chrono::DateTime<chrono::Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    for (second, price, size, side) in [
        (0, 100.0, 1.0, OrderSide::Sell),
        (1, 100.0, 1.0, OrderSide::Buy),
        (2, 101.0, 1.0, OrderSide::Sell),
        (3, 101.0, 4.0, OrderSide::Buy),
        (4, 102.0, 1.0, OrderSide::Sell),
        (5, 102.0, 4.0, OrderSide::Buy),
        (6, 103.0, 4.0, OrderSide::Buy),
        (60, 110.0, 1.0, OrderSide::Buy),
        (120, 110.0, 1.0, OrderSide::Buy),
    ] {
        flow.push(&Tick::new(
            start + TimeDelta::seconds(second),
            price,
            size,
            Some(side),
        ))
        .unwrap();
    }
    let mut bars = vec![
        Bar::new(start, 100.0, 103.0, 100.0, 103.0, 16.0),
        Bar::new(
            start + TimeDelta::minutes(1),
            110.0,
            110.0,
            110.0,
            110.0,
            1.0,
        ),
        Bar::new(
            start + TimeDelta::minutes(2),
            110.0,
            110.0,
            110.0,
            110.0,
            1.0,
        ),
    ];
    let config = FootprintConfig {
        auto_row_height: None,
        show_poc: false,
        statistics: true,
        studies: Some(FootprintStudyConfig {
            cluster_share: Some(0.3),
            single_print_max: Some(2.0),
            delta_percent: Some(62.5),
            ratio: Some(4.0),
            min_delta: 4.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars.clone()));
    let mut draw = |flow: &Footprint, bars: &[Bar], config: &FootprintConfig| {
        chart.set_data(SeriesId::MAIN, bars.to_vec());
        chart.apply_synced_time_scale(220.0, 1.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                chart.show_footprint(ui, flow, config);
            },
        );
        output.textures_delta.clear();
        assert!(
            chart.get_rendered_volume_rect().bottom() <= chart.get_rendered_price_rect().bottom()
        );
        (output.shapes, chart.get_chart_mapping())
    };
    for revisited in [false, true] {
        if revisited {
            flow.push(&Tick::new(
                start + TimeDelta::seconds(121),
                100.0,
                2.0,
                Some(OrderSide::Sell),
            ))
            .unwrap();
            bars[2] = Bar::new(bars[2].time, 110.0, 110.0, 100.0, 100.0, 3.0);
        }
        let (shapes, map) = draw(&flow, &bars, &config);
        let text = |expected: &str| {
            shapes.iter().find_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.text() == expected => Some(t),
                _ => None,
            })
        };
        // First bar: 13 buys, 3 sells, volume 16, delta 10, delta share 62.5%.
        for expected in [
            "16",
            "3",
            "13",
            "10",
            "62.5%",
            "-1",
            "4.33:1",
            "Cluster",
            "Single",
            "Delta POC",
        ] {
            assert!(
                text(expected).is_some(),
                "missing {expected}, revisited={revisited}"
            );
        }
        // The summary below the bar writes the volume too; the table's is the lowest.
        let table = shapes.iter().filter_map(|s| match &s.shape {
            Shape::Text(t) if t.galley.text() == "16" => Some(t.pos.y),
            _ => None,
        });
        assert!(
            table.fold(f32::MIN, f32::max) >= map.rect.bottom(),
            "table overlaps prices"
        );
        assert!(
            shapes
                .iter()
                .any(|shape| matches!(&shape.shape, Shape::Text(t)
            if t.galley.text() == if revisited { "10" } else { "12" }
                && (t.pos.x + t.galley.size().x * 0.5 - map.idx_to_x(2)).abs() < 0.01
                && (t.pos.y - map.rect.bottom() - 72.0).abs() < 0.01)),
            "cumulative delta updates in its row under the last bar"
        );
        let stack_color = if revisited {
            config.ask_color.gamma_multiply(0.45)
        } else {
            config.ask_color
        };
        let stack = shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Rect(r)
                    if r.stroke.color == stack_color
                        && (r.rect.top() - map.price_to_y(103.5)).abs() < 0.01
                        && (r.rect.bottom() - map.price_to_y(100.5)).abs() < 0.01 =>
                {
                    Some(r)
                }
                _ => None,
            })
            .expect("diagonal ask stack at 101..103 follows its revisit state");
        assert!((stack.rect.right() - (map.idx_to_x(2) + map.bar_spacing * 0.46)).abs() < 0.01);
        let level_color = if revisited {
            config.text_color.gamma_multiply(0.45)
        } else {
            config.text_color
        };
        for level in [100.0, 101.0] {
            assert!(
                shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r)
                if r.stroke.color == level_color
                    && (r.rect.top() - map.price_to_y(level + 0.5)).abs() < 0.01
                    && (r.rect.bottom() - map.price_to_y(level - 0.5)).abs() < 0.01
                    && (r.rect.right() - (map.idx_to_x(2) + map.bar_spacing * 0.46)).abs() < 0.01)),
                "unfinished low and naked POC follow their revisit state"
            );
        }
        // Delta-share and ratio buy markers on all three bars, until the revisiting sell
        // turns the last bar to delta -1 of 3 and an ask/bid ratio of 0.5.
        assert_eq!(
            shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Path(p)
            if p.points.len() == 3 && p.fill == config.ask_color))
                .count(),
            if revisited { 4 } else { 6 }
        );
    }
    let (_, map) = draw(&flow, &bars, &config);
    let mut grouped = config.clone();
    grouped.auto_row_height = Some((map.price_to_y(104.0) - map.price_to_y(103.0)).abs() * 1.5);
    let (shapes, map) = draw(&flow, &bars, &grouped);
    // Two input rows per cell: 100..101 hold volume 7, delta 3; 102..103 hold 9, delta 7.
    // Both POCs therefore move to the upper displayed cell, and neither extreme is <= 2.
    assert!(shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r)
        if r.stroke.color == config.text_color
            && (r.rect.top() - map.price_to_y(103.5)).abs() < 0.01
            && (r.rect.bottom() - map.price_to_y(101.5)).abs() < 0.01)));
    assert!(!shapes.iter().any(|s| matches!(&s.shape, Shape::Text(t)
                if t.galley.text() == "Single" && t.pos.x < map.idx_to_x(0))));
    let disabled = FootprintConfig {
        studies: Some(FootprintStudyConfig {
            stacked_imbalances: false,
            naked_poc: false,
            delta_poc: false,
            unfinished_auctions: false,
            ..Default::default()
        }),
        ..grouped
    };
    let (shapes, _) = draw(&flow, &bars, &disabled);
    assert!(!shapes.iter().any(|s| matches!(&s.shape, Shape::Text(t)
        if ["POC", "Delta POC", "Cluster", "Single", "UH", "UL", "Ask stack", "Bid stack"].contains(&t.galley.text()))));
}

/// The main series' legend actions hide it, request its settings and remove it through the
/// chart's typed operations, leaving the studies and other series in place.
#[cfg(feature = "studies")]
#[test]
fn main_series_legend_actions_preserve_studies_and_comparisons() {
    use crate::chart::rendering::SeriesAction;
    use crate::widget::{SeriesId, SeriesOptions};
    let mut chart = chart(&[100.0], &[110.0]);
    let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
    let other = chart.add_series(SeriesOptions::default());
    chart.apply_main_series_action(SeriesAction::ToggleVisibility);
    assert!(!chart.is_main_series_visible());
    chart.apply_main_series_action(SeriesAction::ToggleVisibility);
    assert!(chart.is_main_series_visible());
    chart.apply_main_series_action(SeriesAction::Settings);
    assert_eq!(
        chart.take_indicator_settings_request(),
        Some(SeriesId::MAIN)
    );
    chart.apply_main_series_action(SeriesAction::Remove);
    assert!(!chart.main_series_present);
    assert!(chart.indicators().any(|(id, _)| id == study));
    assert!(chart.series_scale(other).is_some());
}

#[cfg(feature = "studies")]
#[test]
fn status_visibility_reaches_the_chart_legend() {
    use crate::model::{DataStatus, Instrument, Level1Quote, Session};
    use crate::theme::ThemeExt;
    for (description, hidden) in [
        (true, 0),
        (false, 0),
        (true, 1),
        (true, 2),
        (true, 3),
        (true, 4),
        (true, 5),
        (true, 6),
        (true, 7),
        (true, 8),
        (true, 9),
        (true, 10),
        (true, 11),
        (true, 12),
        (true, 13),
    ] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.set_instrument(Some(
            Instrument::new("TICKER")
                .with_description("Instrument description")
                .with_data_status(DataStatus::Delayed)
                .with_session(Session::new([])),
        ));
        let mut settings = crate::widget::settings::ChartSettingsState::from_chart(&chart);
        settings.status_line.show_logo = hidden != 12;
        settings.status_line.quote_panel = hidden == 2;
        settings.status_line.show_market_status = hidden != 13;
        settings.apply_to_chart(&mut chart).unwrap();
        assert_eq!(
            crate::widget::settings::ChartSettingsState::from_chart(&chart).status_line,
            settings.status_line
        );
        let bar = chart.state.data().bars.last().unwrap();
        chart.set_quote(Some(Level1Quote {
            symbol: "TICKER".into(),
            ts: bar.time,
            bid: 109.0,
            ask: 111.0,
            bid_size: 1.0,
            ask_size: 1.0,
        }));
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
        chart.set_indicator_inputs(study, "1 close");
        let oscillator = chart.add_indicator(Box::new(crate::studies::RSI::new(2)));
        chart.set_indicator_inputs(oscillator, "2 close");
        let config = chart.config_mut();
        config.title_as_description = description;
        config.show_volume = hidden != 10;
        config.show_symbol_labels = hidden != 1;
        config.show_quote_boxes = hidden != 2;
        config.show_ohlc_info = hidden != 3;
        config.show_ohlc_change = hidden != 4;
        config.show_ohlc_change_percent = hidden != 9;
        config.show_legend_volume = hidden != 5;
        config.show_indicator_labels = hidden != 6;
        config.show_indicator_inputs = hidden != 7;
        config.show_indicator_last_val = hidden != 8;
        chart.set_main_series_visible(hidden != 11);
        frame(&ctx, &mut chart, 0.0, vec![]);
        if let Some(volume) = chart.volume_study {
            chart
                .indicator_mut(volume)
                .unwrap()
                .set_input("show_average", crate::studies::InputValue::Toggle(true))
                .unwrap();
        }
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Rect(rect)
            if rect.fill == chart.config.bullish_color && chart.last_rendered_price_rect.contains(rect.rect.center()))), hidden != 11, "main series body: hidden={hidden}");
        assert_eq!(chart.bars(crate::widget::SeriesId::MAIN).len(), 100);
        let texts: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.right() => {
                    Some(text.galley.text())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            texts.contains(&"T"),
            !matches!(hidden, 1 | 12),
            "logo hidden={hidden}"
        );
        assert_eq!(
            texts.contains(&"D"),
            !matches!(hidden, 1 | 13),
            "market status hidden={hidden}"
        );
        assert_eq!(
            painted
                .iter()
                .any(|shape| matches!(shape, Shape::Circle(circle)
            if circle.fill == ctx.active_theme().ui.success && circle.radius == 4.0
                && chart.last_rendered_price_rect.contains(circle.center))),
            !matches!(hidden, 1 | 13)
        );
        for (text, flag) in [
            ("SELL", 2),
            ("BUY", 2),
            ("SMA", 6),
            ("1 close", 7),
            ("110.00", 8),
            ("O 100.00", 3),
        ] {
            assert_eq!(
                texts.contains(&text),
                hidden != flag && (hidden != 11 || matches!(flag, 6..=8)),
                "{text}: hidden={hidden} {texts:?}"
            );
        }
        for (text, visible) in [
            ("Vol 1", hidden == 10),
            ("Vol", !matches!(hidden, 5 | 6 | 10 | 11)),
            ("SMA 9", !matches!(hidden, 5 | 7 | 10 | 11)),
            ("1", !matches!(hidden, 5 | 8 | 10 | 11)),
        ] {
            assert_eq!(texts.contains(&text), visible, "{text}: hidden={hidden}");
        }
        assert_eq!(
            texts.iter().any(|text| text.contains("RSI")),
            hidden != 6,
            "{texts:?}"
        );
        assert_eq!(
            texts.iter().any(|text| text.contains("2 close")),
            hidden != 7,
            "{texts:?}"
        );
        let title = if description {
            "Instrument description"
        } else {
            "TICKER"
        };
        assert_eq!(
            texts.iter().any(|text| text.starts_with(title)),
            hidden != 1,
            "{texts:?}"
        );
        assert_eq!(
            chart.last_rendered_volume_rect.is_positive(),
            hidden != 10 && hidden != 11,
            "legend visibility and histogram visibility are independent"
        );
    }
}

#[test]
fn zoomed_out_bars_merge_per_pixel_and_keep_their_extremes() {
    use crate::model::{Bar, BarData, ChartType};
    use chrono::{TimeZone, Utc};
    let bars = (0..65_536)
        .map(|i| {
            let t = Utc.timestamp_opt(1_600_000_000 + i * 60, 0).unwrap();
            let (low, high) = match i {
                34_321 => (100.0, 1000.0),
                47_777 => (1.0, 101.0),
                _ => (100.0, 101.0 + (i % 7) as f64),
            };
            Bar::new(t, 100.0, high, low, 101.0, 1.0)
        })
        .collect();
    struct MergeProbe(std::sync::Arc<std::sync::Mutex<Vec<std::ops::Range<usize>>>>);
    impl crate::widget::Primitive for MergeProbe {
        fn draw_merged(
            &self,
            _: &egui::Painter,
            view: &crate::widget::PrimitiveView<'_>,
            pass: crate::widget::PrimitivePass,
            factor: usize,
        ) {
            if pass == crate::widget::PrimitivePass::Series {
                *self.0.lock().unwrap() = view.point_groups(factor).collect();
            }
        }
    }
    let groups = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ctx = egui::Context::default();
    let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
    // Axis value tags share candle colours but are not candle geometry.
    chart.config_mut().show_right_axis = false;
    chart
        .attach_primitive(crate::widget::SeriesId::MAIN, MergeProbe(groups.clone()))
        .unwrap();
    let mut options = chart.chart_options().clone();
    options.time_scale.min_bar_spacing = 0.0;
    chart.set_chart_options(options);
    frame(&ctx, &mut chart, 0.0, Vec::new());
    chart.apply_synced_time_scale(chart.get_rendered_price_rect().width() / 65_536.0, 0.0);
    frame(&ctx, &mut chart, 0.1, Vec::new());
    {
        let groups = groups.lock().unwrap();
        assert!(
            !groups.is_empty() && groups.len() < 1000,
            "custom renderer receives pixel groups"
        );
        assert_eq!(groups.first().unwrap().start, 0);
        assert_eq!(groups.last().unwrap().end, 65_536);
        assert!(groups.windows(2).all(|pair| pair[0].end == pair[1].start));
        let view = chart.primitive_view(crate::widget::SeriesId::MAIN, chart.get_chart_mapping());
        let mut edge = view.clone();
        edge.visible = 1..7;
        assert_eq!(edge.point_groups(4).collect::<Vec<_>>(), [0..4, 4..8]);
        for visible in [1..1, 65_536..65_536, 65_537..65_538] {
            edge.visible = visible;
            assert_eq!(edge.point_groups(4).next(), None);
        }
    }
    let color = chart.config().bullish_color;
    let painted_ys = |chart: &mut crate::widget::Chart, t| -> Vec<f32> {
        candle_ys(&shapes(&ctx, chart, t, Vec::new()), color)
    };
    let ys = painted_ys(&mut chart, 0.2);
    // About one bar per pixel column, each still reaching the highest and lowest price.
    assert!(ys.len() < 20_000, "{} points", ys.len());
    let map = chart.get_chart_mapping();
    let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!((top - map.price_to_y(1000.0)).abs() < 1.0, "{top}");
    assert!((bottom - map.price_to_y(1.0)).abs() < 1.0, "{bottom}");

    // Without executions the footprint types draw the same merged candles.
    for chart_type in [ChartType::VolumeFootprint, ChartType::SessionVolumeProfile] {
        chart.set_chart_type(chart_type);
        let drawn = painted_ys(&mut chart, 0.2);
        assert!(
            drawn == ys,
            "{chart_type:?}: {} points against {}",
            drawn.len(),
            ys.len()
        );
    }
    chart.set_chart_type(ChartType::Candles);

    let spacing = chart.state.time_scale().bar_spacing();
    let default_factor = chart.merge_groups.factor();
    for (threshold, expected) in [
        (None, 1),
        (Some(0.25), default_factor / 4),
        (Some(4.0), default_factor * 4),
        (Some(1.0), default_factor),
    ] {
        chart.config_mut().conflation_threshold = threshold;
        assert_eq!(chart.prepare_conflation(spacing, 1.0), expected);
        assert_eq!(
            chart.merge_groups.groups.len(),
            if expected == 1 { 0 } else { 65_536 / expected }
        );
        // Give the cached tail a unique payload: reconstructing it would drop this witness.
        let witness = (expected > 1).then(|| {
            let payload = std::sync::Arc::new(serde_json::Value::Null);
            let weak = std::sync::Arc::downgrade(&payload);
            chart.merge_groups.groups.last_mut().unwrap().bar.payload = Some(payload);
            weak
        });
        chart.prepare_conflation(spacing, 1.0);
        let drawn = painted_ys(&mut chart, 0.25);
        if let Some(witness) = witness {
            assert!(
                witness.upgrade().is_some(),
                "rendering reuses prepared groups"
            );
        }
        assert_eq!(groups.lock().unwrap().first().unwrap().len(), expected);
        assert_eq!(chart.merge_groups.factor(), expected);
        assert_eq!(
            drawn.len().cmp(&ys.len()),
            expected.cmp(&default_factor).reverse(),
            "disabled and custom thresholds change rendered sample counts"
        );
        for (price, edge) in [
            (1000.0, drawn.iter().copied().fold(f32::INFINITY, f32::min)),
            (1.0, drawn.iter().copied().fold(f32::NEG_INFINITY, f32::max)),
        ] {
            assert!((edge - chart.get_chart_mapping().price_to_y(price)).abs() < 1.0);
        }
    }

    // A source change must replace the cached line extrema even at the same grouping factor.
    chart.config_mut().price_source = crate::model::PriceSource::High;
    chart.prepare_conflation(spacing, 1.0);
    assert_eq!(
        chart.merge_groups.groups[34_321 / default_factor].extremes[1],
        34_321
    );
    chart.config_mut().price_source = crate::model::PriceSource::Close;
    chart.prepare_conflation(spacing, 1.0);
    assert_eq!(
        chart.merge_groups.groups[34_321 / default_factor].extremes[1],
        34_321 / default_factor * default_factor
    );
    let mut corrected = chart.bars(crate::widget::SeriesId::MAIN)[34_321].clone();
    corrected.high = 1100.0;
    chart.update(crate::widget::SeriesId::MAIN, corrected);
    chart.prepare_conflation(spacing, 1.0);
    assert_eq!(
        chart.merge_groups.groups[34_321 / default_factor].bar.high,
        1100.0
    );

    // A live bar opening a group, then updating it, is drawn in it, with its volume.
    let time = Utc.timestamp_opt(1_600_000_000 + 65_536 * 60, 0).unwrap();
    for (t, high, threshold) in [
        (0.3, 1500.0, Some(1.0)),
        (0.5, 2000.0, Some(4.0)),
        (0.7, 2500.0, Some(4.0)),
        (0.9, 2000.0, Some(4.0)),
        (1.1, 3000.0, None),
    ] {
        chart.config_mut().conflation_threshold = threshold;
        chart.update(
            crate::widget::SeriesId::MAIN,
            Bar::new(time, 100.0, high, 100.0, 101.0, 1.0),
        );
        chart.prepare_conflation(spacing, 1.0);
        frame(&ctx, &mut chart, t, Vec::new());
        let ys = painted_ys(&mut chart, t + 0.1);
        let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let y = chart.get_chart_mapping().price_to_y(high);
        assert!((top - y).abs() < 1.0, "{high}: {top} against {y}");
    }

    chart.set_chart_type(ChartType::Range);
    chart.config_mut().conflation_threshold = Some(4.0);
    assert_eq!(chart.prepare_conflation(spacing, 1.0), 1);
    assert!(chart.merge_groups.groups.is_empty());
}

/// A line-based main series runs on to both plot edges while bars exist beyond them, as an
/// added series does, its fill follows, and Line with markers marks only the bars in view.
#[test]
fn line_based_main_series_reach_the_plot_edges() {
    use crate::model::{BarData, ChartType, ChartTypeStyles};
    use egui::{Color32, epaint::ColorMode};
    type Row = (ChartType, fn(&ChartTypeStyles) -> [Color32; 2], bool);
    let rows: [Row; 6] = [
        (ChartType::Line, |s| [s.line.color; 2], false),
        (ChartType::StepLine, |s| [s.step_line.color; 2], false),
        (
            ChartType::LineWithMarkers,
            |s| [s.line_with_markers.color; 2],
            false,
        ),
        (ChartType::Area, |s| [s.area.line.color; 2], true),
        (
            ChartType::Baseline,
            |s| [s.baseline.above.color, s.baseline.below.color],
            true,
        ),
        (
            ChartType::HlcArea,
            |s| [s.hlc_area.close_line.color; 2],
            false,
        ),
    ];
    let ctx = egui::Context::default();
    // 30.1 bars back neither edge cuts a bar's point; 30.9 bars back the left edge falls 9
    // points before the first point in view.
    for offset in [-30.1, -30.9] {
        for (chart_type, colors, fills) in rows {
            let mut chart = chart(&[100.0, 104.0], &[110.0, 108.0]);
            chart.set_chart_type(chart_type);
            // The price line spans the plot in the series' colour.
            chart.main_series_style.price_line_visible = false;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.apply_synced_time_scale(40.0, offset);
            let shapes = shapes(&ctx, &mut chart, 0.1, Vec::new());
            let plot = chart.get_rendered_price_rect();
            let colors = colors(&chart.chart_type_params.styles);
            let xs: Vec<f32> = shapes
                .iter()
                .flat_map(|shape| match shape {
                    Shape::Path(path)
                        if colors
                            .iter()
                            .any(|c| path.stroke.color == ColorMode::Solid(*c)) =>
                    {
                        path.points.iter().map(|p| p.x).collect()
                    }
                    Shape::LineSegment { points, stroke } if colors.contains(&stroke.color) => {
                        points.iter().map(|p| p.x).collect()
                    }
                    _ => Vec::new(),
                })
                .collect();
            let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let row = format!("{chart_type:?} at {offset}");
            assert!(
                left < plot.left() && right > plot.right(),
                "{row}: {left}..{right}"
            );
            if fills {
                // The fill has a vertex at each end of the line.
                let vertex_at = |x: f32| {
                    shapes.iter().any(|shape| {
                        matches!(shape, Shape::Mesh(mesh)
                            if mesh.vertices.iter().any(|v| v.pos.x == x))
                    })
                };
                assert!(vertex_at(left) && vertex_at(right), "{row}");
            }
            if chart_type == ChartType::LineWithMarkers {
                let half = chart.state.time_scale().bar_spacing() / 2.0;
                let marked = |x: f32| x > plot.left() - half && x < plot.right() + half;
                assert!(
                    shapes.iter().all(|shape| match shape {
                        Shape::Circle(circle) if circle.fill == colors[0] =>
                            marked(circle.center.x),
                        _ => true,
                    }),
                    "{row}"
                );
            }
        }
    }
    // Without bars of its own the chart draws the whitespace slots of its added series.
    let bars = chart(&[100.0], &[110.0])
        .bars(crate::widget::SeriesId::MAIN)
        .to_vec();
    let mut chart = crate::widget::Chart::new(BarData::new());
    let added = chart.add_series(crate::widget::SeriesOptions::default());
    chart.set_data(added, bars);
    chart.set_chart_type(ChartType::Line);
    frame(&ctx, &mut chart, 0.0, Vec::new());
    assert!(chart.state.shows_fallback());
    // Scrolled into history, the first slot in view is past the empty `full_data`.
    chart.apply_synced_time_scale(40.0, -10.0);
    shapes(&ctx, &mut chart, 0.1, Vec::new());
}

/// Bars whose footprint cells are not drawn show candles: all bars, merged per pixel column,
/// at the minimum spacing, and only the bars without complete executions at a wide one.
#[test]
fn footprint_leaves_bars_without_cells_to_candles() {
    use crate::chart::overlays::{FootprintColor, FootprintConfig};
    use crate::data::Footprint;
    use crate::model::{Bar, BarClock, BarData, OrderSide, Tick, Timeframe};
    use chrono::{TimeDelta, TimeZone, Utc};
    let start = Utc.with_ymd_and_hms(2024, 3, 11, 0, 0, 0).unwrap();
    let time = |i: usize| start + TimeDelta::minutes(i as i64);
    // Bar 3985 precedes the executions of the wide case, 3995 has none, 3997 has them.
    let bars = (0..4000)
        .map(|i| {
            let (low, high) = match i {
                3985 => (100.0, 500.0),
                3995 => (20.0, 101.0),
                3997 => (100.0, 300.0),
                _ => (100.0, 101.0),
            };
            Bar::new(time(i), 100.0, high, low, 101.0, 1.0)
        })
        .collect();
    let config = FootprintConfig {
        // Keep price spines out of the candle-only geometry check.
        ask_color: egui::Color32::from_rgb(12, 87, 104),
        color_by: FootprintColor::Volume,
        neutral_color: egui::Color32::from_rgb(21, 45, 78),
        ..Default::default()
    };
    let cell = config.neutral_color.gamma_multiply(0.5);
    let ctx = egui::Context::default();
    let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
    let color = chart.config().bullish_color;
    for (spacing, traded, cells) in [(0.5, 0..4000, false), (20.0, 3990..4000, true)] {
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for i in traded.filter(|&i| !cells || i != 3995) {
            flow.push(&Tick::new(time(i), 100.0, 1.0, Some(OrderSide::Buy)))
                .unwrap();
        }
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.apply_synced_time_scale(spacing, 0.0);
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let plain = candle_ys(&shapes(&ctx, &mut chart, 0.2, Vec::new()), color);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                time: Some(0.2),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chart.show_footprint(ui, &flow, &config);
                });
            },
        );
        let painted = std::mem::take(&mut output.shapes)
            .into_iter()
            .map(|clipped| clipped.shape)
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        let ys = candle_ys(&painted, color);
        let map = chart.get_chart_mapping();
        let reaches = |price| ys.iter().any(|y| (y - map.price_to_y(price)).abs() < 1.0);
        assert!(reaches(500.0) && reaches(20.0), "spacing {spacing}: {ys:?}");
        assert_eq!(reaches(300.0), !cells, "spacing {spacing}");
        if !cells {
            assert!(ys == plain, "{} points against {}", ys.len(), plain.len());
        }
        let has_cells = painted.iter().any(|shape| match shape {
            Shape::Mesh(mesh) => mesh.vertices.iter().any(|v| v.color == cell),
            _ => false,
        });
        assert_eq!(has_cells, cells, "spacing {spacing}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn first_frame_is_settled_and_an_idle_chart_repaints_only_for_a_drawn_countdown() {
    // A host that repaints only on input or new data shows this frame until the next event.
    use crate::model::{Bar, BarData, ChartType};
    use std::time::Duration;
    // A drawn countdown repaints when its label next changes. Each frame sets the server clock
    // half a second into a second, so that is half a second less the frame's own time.
    let countdown = Duration::from_millis(250)..=Duration::from_millis(500);
    let settled = || Duration::MAX..=Duration::MAX;
    let cases = [
        (false, true, false, settled(), None, ChartType::Candles),
        (true, true, false, countdown, None, ChartType::Candles),
        (true, false, false, settled(), None, ChartType::Candles),
        // A paused replay's clock stands still, so its countdown never changes.
        (true, true, true, settled(), None, ChartType::Candles),
    ]
    .into_iter()
    .chain([0, 1, 128].into_iter().flat_map(|count| {
        [
            ChartType::Candles,
            ChartType::Line,
            ChartType::Area,
            ChartType::Heikin,
        ]
        .map(move |kind| (false, true, false, settled(), Some(count), kind))
    }));
    for (server_time, axis, replay, delay, flat_count, kind) in cases {
        let ctx = egui::Context::default();
        let mut chart = if let Some(count) = flat_count {
            let bars = (0..count)
                .map(|i| {
                    let time = chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap();
                    Bar::new(time, 100.0, 100.0, 100.0, 100.0, 0.0)
                })
                .collect();
            crate::widget::Chart::new(BarData::from_bars(bars))
        } else {
            chart(&[100.0, 104.0], &[110.0, 108.0])
        };
        chart.set_chart_type(kind);
        chart.set_visible_bars(50);
        chart.set_timeframe(crate::model::Timeframe::Min1);
        chart.config.show_right_axis = axis;
        let last = chart.state.data().bars.last().map(|bar| bar.time);
        if replay {
            // Bar replay forms the newest bar from the finer bars before its cursor.
            let last = last.unwrap();
            chart.set_replay_time(Some(last + chrono::Duration::seconds(30)));
            let source = vec![Bar::new(last, 104.0, 108.0, 104.0, 108.0, 1.0)];
            let clock = crate::model::BarClock::new(crate::model::Timeframe::Sec5);
            assert!(chart.set_replay_source(source, clock));
            assert!(chart.current_bar_countdown().is_some());
        }
        let mut run = || {
            // Set in each frame, so the frame's own time is all that has run of the second.
            chart.set_server_time(
                server_time.then(|| last.unwrap() + chrono::Duration::milliseconds(30_500)),
            );
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                chart.show(ui);
            });
            let shapes = std::mem::take(&mut output.shapes);
            assert!(
                !shapes.is_empty() && shapes.len() < 2_000,
                "{flat_count:?} bars, {kind:?}"
            );
            let meshes = ctx.tessellate(shapes.clone(), output.pixels_per_point);
            let mut vertices = 0;
            for primitive in meshes {
                assert!(primitive.clip_rect.is_finite());
                if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                    assert!(mesh.is_valid());
                    vertices += mesh.vertices.len();
                    assert!(
                        mesh.vertices.iter().all(|v| v.pos.is_finite()
                            && v.pos.x.abs() < 1_600.0
                            && v.pos.y.abs() < 1_200.0),
                        "{flat_count:?} bars, {kind:?}: geometry exceeds two screen dimensions"
                    );
                }
            }
            assert!(
                vertices > 0 && vertices < 100_000,
                "{flat_count:?} bars, {kind:?}: {vertices} vertices"
            );
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            output.drop_without_applying_deltas();
            (shapes, delay)
        };
        let (first, _) = run();
        for _ in 0..3 {
            let (shapes, actual) = run();
            if flat_count.is_none() {
                // A drawn countdown changes its text as the clock runs.
                if *delay.start() == Duration::MAX {
                    assert!(
                        shapes == first,
                        "the first frame must draw the settled chart"
                    );
                }
                assert!(
                    delay.contains(&actual),
                    "server time {server_time}, axis {axis}, replay {replay}: {actual:?}"
                );
            }
        }
    }
}

#[test]
fn volume_uses_themes_alpha_and_pitch_above_a_configured_baseline() {
    use crate::chart::selection::{ChartElementId, SeriesId};
    use crate::model::ChartType;
    use crate::theme::{Theme, apply_to_egui};
    use egui::Color32;

    // Volume uses the theme's volume colours with and without the volume study.
    // A second opacity checks that the default study inherits the applied theme.
    for (mut theme, alpha, candle_width, kind, height_fraction, inset, expected) in [
        (
            Theme::dark(),
            153,
            0.6,
            ChartType::Candles,
            0.12,
            12.0,
            [[23, 100, 92, 153], [143, 50, 48, 153]],
        ),
        (
            Theme::dark(),
            51,
            0.2,
            ChartType::Heikin,
            0.12,
            12.0,
            [[8, 33, 31, 51], [48, 17, 16, 51]],
        ),
        (
            Theme::light(),
            128,
            0.6,
            ChartType::Line,
            0.25,
            0.0,
            [[19, 83, 77, 128], [120, 42, 40, 128]],
        ),
        (
            Theme::high_contrast(),
            128,
            0.6,
            ChartType::Area,
            0.4,
            6.0,
            [[19, 83, 77, 128], [120, 42, 40, 128]],
        ),
    ] {
        let ctx = egui::Context::default();
        theme.chart.volume_alpha = alpha;
        apply_to_egui(&ctx, &theme);
        let mut chart = chart(&[100.0], &[110.0]);
        // The last visible bar falls, and has half the other bars' volume.
        let mut bars = chart.state.data().bars.clone();
        let last = bars.last_mut().unwrap();
        last.open = 110.0;
        last.close = 100.0;
        last.volume = 0.5;
        chart.set_data(crate::chart::selection::SeriesId::MAIN, bars);
        chart.config.candle_width = candle_width;
        chart.config.volume_height_fraction = height_fraction;
        chart.config.volume_baseline_inset = inset;
        chart.set_chart_type(kind);
        chart
            .selection
            .select(ChartElementId::Series(SeriesId::VOLUME), None);
        let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
        let price = chart.last_rendered_price_rect;
        let volume = chart.last_rendered_volume_rect;
        let layout = chart.calculate_layout_rects(egui::Rect::from_min_max(
            Pos2::new(8.0, 8.0),
            Pos2::new(792.0, 592.0),
        ));
        assert_eq!(layout.price_rect, price);
        assert_eq!(layout.volume_rect, volume);
        assert!(
            (volume.height() - (price.height() - inset) * height_fraction).abs() < 0.001,
            "{volume:?} in {price:?}"
        );
        assert_eq!(volume.bottom(), price.bottom() - inset);
        let dots: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Circle(dot) if dot.radius == 4.5 => Some(dot.center),
                _ => None,
            })
            .collect();
        assert!(!dots.is_empty(), "missing volume selection handles");
        assert!(dots.iter().all(|dot| (dot.y - volume.top()).abs() < 0.001));
        let pitch = chart.state.time_scale().bar_spacing();
        for (colour, fraction) in expected.into_iter().zip([1.0, 0.5]) {
            let colour =
                Color32::from_rgba_premultiplied(colour[0], colour[1], colour[2], colour[3]);
            let bars: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == colour => Some(rect.rect),
                    _ => None,
                })
                .collect();
            assert!(!bars.is_empty(), "alpha {alpha}: missing volume {colour:?}");
            for bar in bars {
                assert!(
                    (bar.height() - volume.height() * fraction).abs() < 0.001,
                    "{bar:?}"
                );
                assert_eq!(bar.bottom(), volume.bottom());
                assert!(
                    (bar.width() - pitch * 0.8).abs() < 0.001,
                    "{bar:?} at pitch {pitch}"
                );
            }
        }
    }
}

/// Saved settings drive feed footprints, selected statistics and session profiles together.
///
/// Rows from the average true range take the bars before the newest, which may still be
/// forming. Their true ranges are 103 - 98 = 5, then max(102 - 97, |102 - 100|, |97 - 100|) = 5,
/// then max(124 - 114, |124 - 99|, |114 - 99|) = 25 from the close of 99 below the gap. Length
/// 14 averages the three bars there are: 35 / 3 = 11.67, a fifth is 2.33, so rows of 2 ticks.
/// Length 2 averages the last two, the older by its own range of 5: (5 + 25) / 2 = 15, a fifth
/// is 3, so rows of 3.
///
/// Open interest of 500 from 09:59 is in effect at the 10:00 open and 507 is published at
/// 10:00:30, so the bar reads 507 and 500; without open interest its rows stay out of the table.
#[cfg(feature = "ui")]
#[test]
fn execution_chart_settings_drive_grouping_statistics_restore_and_undo() {
    use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest};
    use crate::model::{
        Bar, ChartType, ChartTypeParams, FootprintMode, FootprintStatistic, Instrument, OrderSide,
        Tick, Timeframe, TradeProfileSettings,
    };
    use crate::widget::{Chart, ChartBuilder, ChartSnapshot};
    use chrono::{TimeDelta, Utc};
    /// One traded bar, a session profile, or the bars of the average true range above.
    #[derive(PartialEq)]
    enum Flow {
        Bar,
        Profile,
        Atr,
    }
    struct Executions(Flow);
    impl Datafeed for Executions {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            let bars: Vec<_> = match self.0 {
                Flow::Atr => [
                    (100.0, 103.0, 98.0, 100.0),
                    (100.0, 102.0, 97.0, 99.0),
                    (115.0, 124.0, 114.0, 115.0),
                    (104.0, 105.0, 100.0, 101.0),
                ]
                .into_iter()
                .zip((-3..=0).map(|minutes| time + TimeDelta::minutes(minutes)))
                .map(|((open, high, low, close), time)| {
                    Bar::new(time, open, high, low, close, 10.0)
                })
                .collect(),
                Flow::Profile => vec![Bar::new(time, 100.0, 102.0, 100.0, 101.0, 10.0)],
                Flow::Bar => vec![Bar::new(time, 100.0, 101.0, 100.0, 101.0, 10.0)],
            };
            reply.send(Ok(HistoryPage::new(bars, true)));
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            let levels = match self.0 {
                Flow::Atr => (0..6).map(|i| (100.0 + i as f64, 2f64.powi(i))).collect(),
                Flow::Profile => vec![(100.0, 1.0), (101.0, 6.0), (102.0, 3.0)],
                Flow::Bar => vec![(100.0, 2.0), (101.0, 3.0)],
            };
            reply.send(Ok(TradePage::new(
                levels
                    .into_iter()
                    .enumerate()
                    .map(|(i, (price, size))| {
                        Tick::new(
                            time + TimeDelta::seconds(i as i64),
                            price,
                            size,
                            Some(if i == 0 {
                                OrderSide::Sell
                            } else {
                                OrderSide::Buy
                            }),
                        )
                    })
                    .collect(),
                true,
            )));
        }
    }
    let ctx = egui::Context::default();
    let mut chart = ChartBuilder::new()
        .with_feed(std::sync::Arc::new(Executions(Flow::Bar)))
        .with_symbol("FLOW")
        .with_timeframe(Timeframe::Min1)
        .with_chart_type(ChartType::VolumeFootprint)
        .build();
    chart.set_server_time(Some(
        "2026-01-05T10:02:00Z"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap(),
    ));
    let draw_at = |chart: &mut Chart, spacing: f32| {
        chart.apply_synced_time_scale(spacing, 1.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
            },
        );
        output.textures_delta.clear();
        output.shapes
    };
    let draw = |chart: &mut Chart| draw_at(chart, 160.0);
    for _ in 0..5 {
        draw(&mut chart);
    }
    for (ticks, expected) in [(2, vec!["5"]), (1, vec!["2", "3"])] {
        chart.set_chart_type_params(ChartTypeParams {
            footprint_modes: vec![FootprintMode::Volume],
            footprint_row_ticks: ticks,
            footprint_poc: false,
            ..Default::default()
        });
        let shapes = draw(&mut chart);
        let map = chart.get_chart_mapping();
        assert!(map.price_to_y(99.5) <= map.rect.bottom());
        assert!(map.price_to_y(101.5) >= map.rect.top());
        let mut numbers: Vec<_> = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(text)
                    if text.pos.x > map.idx_to_x(0) - 70.0
                        && text.pos.x < map.idx_to_x(0) + 70.0
                        && text.pos.y > map.rect.top() + 40.0
                        && text.pos.y < map.rect.bottom() - 20.0 =>
                {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect();
        numbers.sort();
        assert_eq!(numbers, expected, "ticks per row {ticks}");
        if ticks == 2 {
            let flow = chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap();
            assert_eq!(
                flow.live.price_step(),
                1.0,
                "display grouping keeps the source executions"
            );
        }
    }
    // Cells narrower than one point are candles, so four-tick rows must not widen the fit.
    let lows: Vec<f32> = [0, 4]
        .into_iter()
        .map(|ticks| {
            chart.set_chart_type_params(ChartTypeParams {
                footprint_row_ticks: ticks,
                ..Default::default()
            });
            for _ in 0..2 {
                draw_at(&mut chart, 3.0);
            }
            chart.get_chart_mapping().price_to_y(100.0)
        })
        .collect();
    assert!(
        (lows[0] - lows[1]).abs() < 0.5,
        "low 100 at y {} with automatic rows, at y {} with four-tick rows",
        lows[0],
        lows[1]
    );
    let mut atr = ChartBuilder::new()
        .with_feed(std::sync::Arc::new(Executions(Flow::Atr)))
        .with_symbol("ATR")
        .with_timeframe(Timeframe::Min1)
        .with_chart_type(ChartType::VolumeFootprint)
        .build();
    atr.set_server_time(Some(
        "2026-01-05T10:02:00Z"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap(),
    ));
    for _ in 0..5 {
        draw(&mut atr);
    }
    // Rows of 100..105 traded 1, 2, 4, 8, 16 and 32, so every row has a number at its centre.
    for (length, ticks) in [(14, 2.0), (2, 3.0)] {
        atr.set_chart_type_params(ChartTypeParams {
            footprint_modes: vec![FootprintMode::Volume],
            footprint_row_atr: true,
            footprint_row_atr_length: length,
            footprint_poc: false,
            ..Default::default()
        });
        // A zoom in time, and a zoom and a pan in price.
        for (spacing, low, high) in [(160.0, 90.0, 120.0), (120.0, 80.0, 140.0)] {
            atr.price_scale_mut().set_range(low, high);
            draw_at(&mut atr, spacing);
            let shapes = draw_at(&mut atr, spacing);
            let map = atr.get_chart_mapping();
            let mut centres: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(text)
                        if (text.pos.x - map.idx_to_x(3)).abs() < 70.0
                            && text.pos.y > map.rect.top() + 40.0
                            && text.pos.y < map.rect.bottom() - 20.0
                            && text.galley.text().parse::<u32>().is_ok() =>
                    {
                        Some(map.y_to_price(text.pos.y + text.galley.size().y / 2.0))
                    }
                    _ => None,
                })
                .collect();
            centres.sort_by(f64::total_cmp);
            let heights: Vec<_> = centres.windows(2).map(|pair| pair[1] - pair[0]).collect();
            assert!(
                heights.len() >= 2 && heights.iter().all(|row| (row - ticks).abs() < 0.25),
                "length {length}, spacing {spacing}, prices {low} to {high}: rows {heights:?}"
            );
        }
    }
    let params = ChartTypeParams {
        footprint_modes: vec![FootprintMode::Delta],
        footprint_row_ticks: 1,
        footprint_row_atr: true,
        footprint_row_atr_length: 30,
        footprint_statistics: true,
        footprint_statistics_rows: vec![
            FootprintStatistic::Volume,
            FootprintStatistic::Delta,
            FootprintStatistic::OpenInterestHigh,
            FootprintStatistic::OpenInterestLow,
        ],
        profile: TradeProfileSettings {
            value_area: Some(0.5),
            ..Default::default()
        },
        ..Default::default()
    };
    let table = |chart: &mut Chart| {
        let shapes = draw(chart);
        let bottom = chart.get_rendered_price_rect().bottom();
        shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(text) if text.pos.y >= bottom && text.pos.y < bottom + 52.0 => {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let open_interest = crate::data::StudyDataRequest {
        symbol: String::new(),
        timeframe: None,
        metric: Some("open_interest".to_owned()),
    };
    // Open interest supplied before a row reads it is kept by the frame that selects the rows.
    chart.set_study_data(
        open_interest.clone(),
        [
            ("2026-01-05T09:59:00Z", 500.0),
            ("2026-01-05T10:00:30Z", 507.0),
        ]
        .into_iter()
        .map(|(at, value)| Bar::new(at.parse().unwrap(), value, value, value, value, 0.0))
        .collect(),
        None,
    );
    chart.user_action(|chart| chart.set_chart_type_params(params.clone()));
    table(&mut chart);
    assert_eq!(
        table(&mut chart),
        ["Vol", "Delta", "OI high", "OI low", "5", "1", "507", "500"]
    );
    let four_rows = chart.get_rendered_price_rect().bottom();
    // Without open interest the rows are left out, and the table is two rows shorter.
    chart.set_study_data(open_interest, Vec::new(), None);
    assert_eq!(table(&mut chart), ["Vol", "Delta", "5", "1"]);
    assert_eq!(chart.get_rendered_price_rect().bottom() - four_rows, 24.0);
    assert_eq!(chart.trade_profile_settings().value_area, Some(0.5));
    let saved = serde_json::to_vec(&chart.snapshot().unwrap()).unwrap();
    let snapshot: ChartSnapshot = serde_json::from_slice(&saved).unwrap();
    let mut restored = Chart::new(crate::model::BarData::new());
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.chart_type_params(), &params);
    assert!(chart.undo());
    assert_eq!(chart.trade_profile_settings().value_area, Some(0.7));
    assert!(!chart.chart_type_params().footprint_row_atr);
    assert!(chart.redo());
    assert_eq!(chart.chart_type_params(), &params);
    let mut profile = ChartBuilder::new()
        .with_feed(std::sync::Arc::new(Executions(Flow::Profile)))
        .with_symbol("PROFILE")
        .with_timeframe(Timeframe::Min1)
        .with_chart_type(ChartType::SessionVolumeProfile)
        .build();
    for _ in 0..5 {
        draw(&mut profile);
    }
    for (share, high) in [(0.7, 102.5), (0.5, 101.5)] {
        profile.set_chart_type_params(ChartTypeParams {
            profile: TradeProfileSettings {
                value_area: Some(share),
                value_area_guides: true,
                ..Default::default()
            },
            ..Default::default()
        });
        let shapes = draw(&mut profile);
        let map = profile.get_chart_mapping();
        let left = map.idx_to_x(0) - map.bar_spacing * 0.45;
        for edge in [100.5, high] {
            assert!(
                shapes.iter().any(|s| matches!(&s.shape,
                Shape::LineSegment { points, .. }
                    if (points[0].x - left).abs() < 0.01
                    && (points[0].y - map.price_to_y(edge)).abs() < 0.01
                    && points[0].y == points[1].y)),
                "share {share}, edge {edge}"
            );
        }
    }
}

/// The colour of the mesh quad painted last at `point`.
fn mesh_color_at(shapes: &[egui::epaint::ClippedShape], point: Pos2) -> Option<egui::Color32> {
    shapes
        .iter()
        .rev()
        .filter_map(|s| match &s.shape {
            Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        })
        .find_map(|mesh| {
            mesh.vertices.chunks(4).rev().find_map(|q| {
                Rect::from_two_pos(q[0].pos, q[3].pos)
                    .contains(point)
                    .then_some(q[0].color)
            })
        })
}

/// The footprint's volume shades are mixed with the background the chart paints. With the light
/// theme and a black chart background, as the Canvas settings set it, the sell shades must be
/// those drawn on black: 20, 40, 60 and 80% of rgb(242, 54, 69), hand-worked as (48, 11, 14),
/// (97, 22, 28), (145, 32, 41) and (194, 43, 55). Sells of 10, 20, 30 and 50 at 102 to 105 fall
/// in the four quarters of the bar's sell range, 10 to 50; 101 holds the POC.
#[test]
fn footprint_shades_follow_the_chart_background() {
    use crate::chart::overlays::FootprintConfig;
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    let ctx = egui::Context::default();
    crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::light());
    let mut chart = chart(&[100.0], &[106.0]);
    chart.set_symbol("FLOW");
    chart.apply_synced_time_scale(100.0, 1.0);
    let time = chart.bars(crate::widget::SeriesId::MAIN)[99].time;
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    for (price, sell, buy) in [
        (101.0, 10.0, 200.0),
        (102.0, 10.0, 1.0),
        (103.0, 20.0, 1.0),
        (104.0, 30.0, 1.0),
        (105.0, 50.0, 1.0),
    ] {
        for (size, side) in [(sell, OrderSide::Sell), (buy, OrderSide::Buy)] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
    }
    let config = FootprintConfig {
        auto_row_height: None,
        text_color: egui::Color32::WHITE,
        ..Default::default()
    };
    let draw = |chart: &mut crate::widget::Chart| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                chart.show_footprint(ui, &flow, &config);
            },
        );
        output.textures_delta.clear();
        output.shapes
    };
    // The first frame follows the light palette; the edit after it is the user's.
    draw(&mut chart);
    chart.config_mut().background_color = egui::Color32::BLACK;
    let shapes = draw(&mut chart);
    let map = chart.get_chart_mapping();
    let x = map.idx_to_x(99) - map.bar_spacing * 0.46 + 4.0;
    let shades: Vec<_> = [102.0, 103.0, 104.0, 105.0]
        .into_iter()
        .map(|price| {
            mesh_color_at(&shapes, egui::pos2(x, map.price_to_y(price)))
                .map(|c| [c.r(), c.g(), c.b()])
        })
        .collect();
    let expected = [[48, 11, 14], [97, 22, 28], [145, 32, 41], [194, 43, 55]];
    assert!(
        shades.iter().zip(expected).all(|(shade, expected)| {
            shade.is_some_and(|shade| shade.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1))
        }),
        "sell shades on a black chart background {shades:?}, expected {expected:?}"
    );
}

/// A footprint row whose volume is all unclassified must not look like the bar's POC, in the chart
/// type, which passes its text colour as the neutral colour, and in a host's config that does the
/// same. Bar: 100 holds 2 unclassified, 101 holds 2 sold and 6 bought (the POC, 8), 102 holds 1
/// sold and 2 bought.
#[test]
fn unclassified_footprint_rows_do_not_look_like_the_poc() {
    use crate::chart::coords::ChartMapping;
    use crate::chart::overlays::FootprintConfig;
    use crate::data::{
        Datafeed, Footprint, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest,
    };
    use crate::model::{Bar, BarClock, ChartType, Instrument, OrderSide, Tick, Timeframe};
    use crate::widget::ChartBuilder;
    use chrono::{TimeDelta, Utc};
    const TRADES: [(f64, f64, Option<OrderSide>); 5] = [
        (100.0, 2.0, None),
        (101.0, 2.0, Some(OrderSide::Sell)),
        (101.0, 6.0, Some(OrderSide::Buy)),
        (102.0, 1.0, Some(OrderSide::Sell)),
        (102.0, 2.0, Some(OrderSide::Buy)),
    ];
    /// The fills of the sell cells of 100 and 101 of the bar at `index`, clear of their numbers.
    fn sell_fills(
        shapes: &[egui::epaint::ClippedShape],
        map: &ChartMapping,
        index: usize,
    ) -> (Option<egui::Color32>, Option<egui::Color32>) {
        let x = map.idx_to_x(index) - map.bar_spacing * 0.46 + 4.0;
        let at = |price: f64| mesh_color_at(shapes, egui::pos2(x, map.price_to_y(price)));
        (at(100.0), at(101.0))
    }
    struct Executions;
    impl Datafeed for Executions {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            reply.send(Ok(HistoryPage::new(
                vec![Bar::new(time, 100.0, 102.0, 100.0, 101.0, 13.0)],
                true,
            )));
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            reply.send(Ok(TradePage::new(
                TRADES
                    .into_iter()
                    .enumerate()
                    .map(|(i, (price, size, side))| {
                        Tick::new(time + TimeDelta::seconds(i as i64), price, size, side)
                    })
                    .collect(),
                true,
            )));
        }
    }
    let mut fills = Vec::new();
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = ChartBuilder::new()
            .with_feed(std::sync::Arc::new(Executions))
            .with_symbol("FLOW")
            .with_timeframe(Timeframe::Min1)
            .with_chart_type(ChartType::VolumeFootprint)
            .build();
        chart.set_server_time(Some(
            "2026-01-05T10:02:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap(),
        ));
        let mut shapes = Vec::new();
        for _ in 0..5 {
            chart.apply_synced_time_scale(160.0, 1.0);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            );
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        fills.push(sell_fills(&shapes, &chart.get_chart_mapping(), 0));

        // The same bar from a host, whose neutral colour equals its text colour.
        let mut host = self::chart(&[100.0], &[102.0]);
        host.set_symbol("FLOW");
        host.apply_synced_time_scale(160.0, 1.0);
        let time = host.bars(crate::widget::SeriesId::MAIN)[99].time;
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size, side) in TRADES {
            flow.push(&Tick::new(time, price, size, side)).unwrap();
        }
        let ink = theme.chart.axis_text;
        let config = FootprintConfig {
            auto_row_height: None,
            text_color: ink,
            neutral_color: ink,
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                host.show_footprint(ui, &flow, &config);
            },
        );
        output.textures_delta.clear();
        fills.push(sell_fills(&output.shapes, &host.get_chart_mapping(), 99));
    }
    assert!(
        fills
            .iter()
            .all(|(unclassified, poc)| poc.is_some() && unclassified != poc),
        "(unclassified row, POC row) sell cell fills, chart then host, light then dark: {fills:?}"
    );
}

#[cfg(all(feature = "drawings", feature = "studies"))]
fn rect_at(painted: &[Shape], fill: egui::Color32) -> usize {
    painted
        .iter()
        .position(|shape| match shape {
            Shape::Rect(rect) => rect.fill == fill,
            Shape::Mesh(mesh) => mesh.vertices.iter().any(|v| v.color == fill),
            _ => false,
        })
        .expect("rectangle painted, including its cached mesh")
}

#[cfg(all(feature = "drawings", feature = "studies"))]
fn line_at(painted: &[Shape], color: egui::Color32) -> (usize, usize) {
    let is_line = |shape: &Shape| match shape {
        Shape::LineSegment { stroke, .. } => stroke.color == color,
        Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
        _ => false,
    };
    (
        painted.iter().position(is_line).unwrap(),
        painted.iter().rposition(is_line).unwrap(),
    )
}

#[cfg(all(feature = "drawings", feature = "studies"))]
fn add_rect(
    chart: &mut crate::widget::Chart,
    pane: Option<usize>,
    prices: [f64; 2],
    fill: egui::Color32,
) -> usize {
    use crate::drawings::{Drawing, DrawingToolType};
    use crate::widget::SeriesId;
    let bars = chart.bars(SeriesId::MAIN);
    let anchors = [(bars[85].time, prices[0]), (bars[98].time, prices[1])];
    let mut drawing = Drawing::new(0, DrawingToolType::Rect);
    drawing.pane = pane;
    drawing.fill_color = Some(fill.to_array());
    drawing.color = [201, 1, 201, 255];
    drawing.theme_shade = false;
    drawing.completed = true;
    chart.create_drawing(drawing, &anchors).unwrap()
}

/// Series and study edits preserve the other items' paint and pointer order through undo,
/// including drawings passed to a surviving study and drawings whose layers merge (#3614). In a
/// user action an added series and the main series moving into a pane or back go under the
/// drawings they meet there, as under a drawing placed there, and a host adding its series again
/// after a restore keeps the restored places (#3739).
#[cfg(all(feature = "drawings", feature = "studies"))]
#[test]
fn drawings_keep_their_place_when_the_series_and_studies_change() {
    use crate::studies::{RSI, SMA};
    use crate::widget::{PaneId, SeriesId, SeriesOptions, VisualOrder};
    use egui::{Color32, Event};
    const FILL: Color32 = Color32::from_rgb(200, 0, 200);
    const SECOND: Color32 = Color32::from_rgb(0, 180, 200);
    let mut failures = Vec::new();
    for (case, under, after_layer) in [
        ("remove study", true, 0),
        ("move study out", true, 0),
        ("reorder study", false, 2),
        ("reorder series", false, 2),
    ] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
        chart.config.show_volume = false;
        chart.set_visible_bars(20);
        // Every pair of closes is 110 and 104, so SMA(2) is 107, above the probed price.
        let item = if case == "reorder series" {
            chart.add_series(SeriesOptions::default())
        } else {
            chart.add_indicator(Box::new(SMA::new(2)))
        };
        let color = chart
            .indicators()
            .find(|(id, _)| *id == item)
            .map(|(_, study)| study.plot_style(0).color);
        if under {
            assert!(chart.reorder_indicator(item, VisualOrder::SendToBack));
        }
        let id = add_rect(&mut chart, None, [109.0, 101.0], FILL);
        assert!(chart.reorder_drawing(id, VisualOrder::SendBackward));
        let second = (case == "remove study").then(|| {
            let second = add_rect(&mut chart, None, [109.0, 101.0], SECOND);
            let z = chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .unwrap()
                .z_order;
            chart
                .drawings_mut()
                .edit_drawing(second, |d| {
                    d.series_layer = Some(0);
                    d.z_order = z + 10;
                    d.locked = true;
                })
                .unwrap();
            second
        });
        for step in 0..3 {
            match step {
                0 => chart.user_action(|chart| {
                    assert!(match case {
                        "remove study" => chart.remove_series(item),
                        "move study out" => chart.move_indicator_to_pane(item, item),
                        "reorder study" => chart.reorder_indicator(item, VisualOrder::SendToBack),
                        _ => chart.set_series_order(item, 0),
                    })
                }),
                1 => assert!(chart.undo()),
                _ => assert!(chart.redo()),
            }
            frame(&ctx, &mut chart, f64::from(step), vec![]);
            let map = chart.get_chart_mapping();
            // Bar 96's bullish body covers 100..110; neither SMA nor comparison covers here.
            let on = Pos2::new(map.idx_to_x(96), map.price_to_y(105.0));
            let painted = shapes(
                &ctx,
                &mut chart,
                f64::from(step) + 0.1,
                vec![Event::PointerMoved(on)],
            );
            let rect = rect_at(&painted, FILL);
            let candles: Vec<_> = painted
                .iter()
                .enumerate()
                .filter_map(|(at, shape)| {
                    matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                    && chart.get_rendered_price_rect().contains_rect(r.rect))
                    .then_some(at)
                })
                .collect();
            let over_candles = if under {
                rect < candles[0]
            } else {
                *candles.last().unwrap() < rect
            };
            let study_order = match color {
                Some(color) if step == 1 || case == "reorder study" => {
                    let line = line_at(&painted, color);
                    if under || step != 1 {
                        line.1 < rect
                    } else {
                        rect < line.0
                    }
                }
                _ => true,
            };
            let layer = chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .unwrap()
                .series_layer;
            let expected = (
                Some(if step == 1 { 1 } else { after_layer }),
                true,
                true,
                (!under).then_some(id),
            );
            let actual = (
                layer,
                over_candles,
                study_order,
                chart.drawings().hit_test(on),
            );
            if actual != expected {
                failures.push(format!(
                    "{case}, step {step}: {actual:?}, expected {expected:?}"
                ));
            }
            if let Some(second) = second {
                let other = chart
                    .drawings()
                    .drawings
                    .iter()
                    .find(|d| d.id == second)
                    .unwrap();
                assert_eq!(other.series_layer, Some(0));
                assert!(other.locked);
                if rect_at(&painted, SECOND) >= rect {
                    failures.push(format!("{case}, step {step}: merged drawings reversed"));
                }
            }
        }
    }

    const UPPER: Color32 = Color32::from_rgb(230, 120, 0);
    const NEW: Color32 = Color32::from_rgb(20, 160, 60);
    const INTO: &str = "main series into a study pane";
    const BACK: &str = "main series back";
    const ADD: &str = "add study";
    const TEMPLATE: &str = "apply template";
    for (case, backward, layers) in [
        // Over the candles and SMA(2), under SMA(3).
        ("add series", 1, [Some(2), Some(3)]),
        // Over the candles, under both SMAs.
        (INTO, 2, [Some(1), Some(2)]),
        // Under RSI and the candles, in the RSI pane.
        (BACK, 2, [Some(0), Some(2)]),
        // Over every item, with the count that removing a study drawn over it leaves (#3897).
        (ADD, 0, [Some(3), Some(4)]),
        (TEMPLATE, 0, [Some(3), Some(4)]),
    ] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
        chart.config.show_volume = false;
        chart.set_visible_bars(20);
        // Closes alternate 110 and 104, so SMA(2) is 107 on every bar and SMA(3) 108 on even
        // bars and 106 on odd ones, where the candles end at 104.
        let sma = chart.add_indicator(Box::new(SMA::new(2)));
        chart.add_indicator(Box::new(SMA::new(3).with_color(UPPER)));
        let rsi = chart.add_indicator(Box::new(RSI::new(14)));
        let color = |chart: &crate::widget::Chart, id| {
            let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
            study.plot_style(0).color
        };
        let (sma_color, rsi_color) = (color(&chart, sma), color(&chart, rsi));
        let pane = chart.series_pane(rsi).unwrap();
        if case == BACK {
            assert!(chart.move_series_to_pane(SeriesId::MAIN, pane));
        }
        let id = add_rect(&mut chart, None, [109.0, 101.0], FILL);
        for _ in 0..backward {
            assert!(chart.reorder_drawing(id, VisualOrder::SendBackward));
        }
        let joins = case == ADD || case == TEMPLATE;
        if joins {
            chart
                .drawings_mut()
                .edit_drawing(id, |d| d.series_layer = Some(3))
                .unwrap();
        }
        // SMA(4) is 107 on every bar, along SMA(2).
        let template = {
            let mut set = chart.study_template_set();
            set.price_pane.push(set.studies.len());
            set.studies.push(crate::studies::StudyTemplate::from_study(
                &SMA::new(4).with_color(NEW),
            ));
            set
        };
        for step in 0..3 {
            match step {
                0 => chart.user_action(|chart| match case {
                    INTO => assert!(chart.move_series_to_pane(SeriesId::MAIN, pane)),
                    BACK => assert!(chart.move_series_to_pane(SeriesId::MAIN, PaneId::MAIN)),
                    ADD => {
                        chart.add_indicator(Box::new(SMA::new(4).with_color(NEW)));
                    }
                    TEMPLATE => {
                        chart.apply_chosen_study_template_set(&template).unwrap();
                    }
                    _ => {
                        chart.add_series(SeriesOptions::default());
                    }
                }),
                1 => assert!(chart.undo()),
                _ => assert!(chart.redo()),
            }
            frame(&ctx, &mut chart, f64::from(step), vec![]);
            let map = chart.get_chart_mapping();
            let on = Pos2::new(map.idx_to_x(96), map.price_to_y(105.0));
            let painted = shapes(
                &ctx,
                &mut chart,
                f64::from(step) + 0.1,
                vec![Event::PointerMoved(on)],
            );
            let rect = rect_at(&painted, FILL);
            let candles: Vec<_> = painted
                .iter()
                .enumerate()
                .filter_map(|(at, shape)| {
                    matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                    && chart.get_rendered_price_rect().contains_rect(r.rect))
                    .then_some(at)
                })
                .collect();
            let over_candles = case != BACK;
            let candle_side = if over_candles {
                *candles.last().unwrap() < rect
            } else {
                rect < candles[0]
            };
            // The studies sharing the rectangle's pane, and whether it is drawn over each.
            let price_pane = chart.series_pane(SeriesId::MAIN) == Some(PaneId::MAIN);
            let sides = if price_pane {
                let mut sides = vec![(sma_color, case != INTO), (UPPER, case == BACK || joins)];
                if joins && step != 1 {
                    sides.push((NEW, true));
                }
                sides
            } else {
                vec![(rsi_color, case == INTO)]
            };
            let study_sides = sides.iter().all(|(color, over)| {
                let (first, last) = line_at(&painted, *color);
                if *over { last < rect } else { rect < first }
            });
            let hit = chart.drawings().hit_test(on);
            // On SMA(2) over odd bar 95: no candle there, and SMA(3) passes a price lower.
            let on_sma = Pos2::new(map.idx_to_x(95), map.price_to_y(107.0));
            let sma_hit = price_pane.then(|| {
                shapes(
                    &ctx,
                    &mut chart,
                    f64::from(step) + 0.2,
                    vec![Event::PointerMoved(on_sma)],
                );
                chart.drawings().hit_test(on_sma)
            });
            let layer = chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .unwrap()
                .series_layer;
            let expected = (
                layers[usize::from(step != 1)],
                true,
                true,
                over_candles.then_some(id),
                price_pane.then_some((case != INTO).then_some(id)),
            );
            let actual = (layer, candle_side, study_sides, hit, sma_hit);
            if actual != expected {
                failures.push(format!(
                    "{case}, step {step}: {actual:?}, expected {expected:?}"
                ));
            }
        }
    }

    #[cfg(feature = "ui")]
    {
        // A host that restores a layout without its series adds them again outside a user action,
        // and the restored drawings already count them.
        let mut saved = chart(&[100.0, 100.0], &[110.0, 104.0]);
        saved.add_indicator(Box::new(SMA::new(2)));
        saved.add_series(SeriesOptions::default());
        // Over the candles, under the added series and SMA(2).
        let id = add_rect(&mut saved, None, [109.0, 101.0], FILL);
        for _ in 0..2 {
            assert!(saved.reorder_drawing(id, VisualOrder::SendBackward));
        }
        let mut snapshot = saved.snapshot().unwrap();
        snapshot.series = None;
        let mut restored = chart(&[100.0, 100.0], &[110.0, 104.0]);
        restored.restore(&snapshot).unwrap();
        restored.add_series(SeriesOptions::default());
        let layer = |chart: &crate::widget::Chart| chart.drawings().drawings[0].series_layer;
        let layers = (layer(&saved), layer(&restored));
        if layers != (Some(1), Some(1)) {
            failures.push(format!("series added again after a restore: {layers:?}"));
        }
    }

    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
    chart.config.show_volume = false;
    chart.set_visible_bars(20);
    let rsi14 = chart.add_indicator(Box::new(RSI::new(14)));
    let mut rsi = RSI::new(5);
    crate::studies::Indicator::set_colors(&mut rsi, vec![Color32::from_rgb(30, 160, 210)]);
    let rsi5 = chart.add_indicator(Box::new(rsi));
    assert!(chart.move_indicator_to_pane(rsi5, rsi14));
    let colors: Vec<_> = chart
        .indicators()
        .map(|(_, study)| study.plot_style(0).color)
        .collect();
    let id = add_rect(&mut chart, Some(rsi14.0), [80.0, 20.0], FILL);
    assert!(chart.reorder_drawing(id, VisualOrder::SendBackward));
    for step in 0..3 {
        match step {
            0 => chart.user_action(|chart| assert!(chart.remove_series(rsi14))),
            1 => assert!(chart.undo()),
            _ => assert!(chart.redo()),
        }
        frame(&ctx, &mut chart, f64::from(step), vec![]);
        let painted = shapes(&ctx, &mut chart, f64::from(step) + 0.1, vec![]);
        let rect = rect_at(&painted, FILL);
        let drawing = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .unwrap();
        let actual = (
            drawing.pane,
            drawing.series_layer,
            rect < line_at(&painted, colors[1]).0,
        );
        let expected = if step == 1 {
            (Some(rsi14.0), Some(1), true)
        } else {
            (Some(rsi5.0), Some(0), true)
        };
        if actual != expected {
            failures.push(format!(
                "study owner removed, step {step}: {actual:?}, expected {expected:?}"
            ));
        }
        if step == 1 {
            assert!(line_at(&painted, colors[0]).1 < rect);
        }
    }
    // A study placed on its source study's pane goes under a drawing over every item there, with
    // the count that removing a study drawn over it leaves (#3897).
    let ctx = egui::Context::default();
    let mut chart = crate::widget::test_frames::chart(&[100.0, 100.0], &[110.0, 104.0]);
    chart.config.show_volume = false;
    chart.set_visible_bars(20);
    let rsi = chart.add_indicator(Box::new(RSI::new(14)));
    let id = add_rect(&mut chart, Some(rsi.0), [80.0, 20.0], FILL);
    chart
        .drawings_mut()
        .edit_drawing(id, |d| d.series_layer = Some(1))
        .unwrap();
    for step in 0..3 {
        match step {
            0 => chart.user_action(|chart| {
                use crate::studies::{Indicator, InputValue, StudySource};
                let mut study: Box<dyn Indicator> = Box::new(crate::studies::ConfiguredStudy::new(
                    Box::new(SMA::new(2).with_color(NEW)),
                ));
                let source = StudySource::Study { id: rsi.0, plot: 0 };
                study
                    .set_input("source", InputValue::Source(source))
                    .unwrap();
                let added = chart.add_indicator(study);
                assert_eq!(chart.series_pane(added), chart.series_pane(rsi));
            }),
            1 => assert!(chart.undo()),
            _ => assert!(chart.redo()),
        }
        frame(&ctx, &mut chart, f64::from(step), vec![]);
        let painted = shapes(&ctx, &mut chart, f64::from(step) + 0.1, vec![]);
        // A point of the study on RSI over bar 95, or of RSI after the undo.
        let x = chart.get_chart_mapping().idx_to_x(95);
        let color = if step == 1 {
            color_of(&chart, rsi)
        } else {
            NEW
        };
        let on = painted
            .iter()
            .flat_map(|shape| match shape {
                Shape::LineSegment { points, stroke } if stroke.color == color => points.to_vec(),
                Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(color) => {
                    path.points.clone()
                }
                _ => Vec::new(),
            })
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .unwrap();
        let painted = shapes(
            &ctx,
            &mut chart,
            f64::from(step) + 0.2,
            vec![Event::PointerMoved(on)],
        );
        let rect = rect_at(&painted, FILL);
        let layer = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .unwrap()
            .series_layer;
        let actual = (
            layer,
            line_at(&painted, color).1 < rect,
            chart.drawings().hit_test(on),
        );
        let expected = (Some(if step == 1 { 1 } else { 2 }), true, Some(id));
        if actual != expected {
            failures.push(format!(
                "study on its source, step {step}: {actual:?}, expected {expected:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A new study's temporary price-pane placement does not edit that pane's drawings.
#[cfg(all(feature = "drawings", feature = "studies"))]
#[test]
fn chained_studies_only_restack_the_pane_they_join() {
    use crate::studies::{
        ConfiguredStudy, Indicator, InputValue, RSI, SMA, StudySource, StudyTemplate,
        StudyTemplateSet,
    };
    use crate::widget::{Chart, ChartEvent, SeriesId};
    use egui::Color32;

    const FILL: Color32 = Color32::from_rgb(200, 0, 200);
    let mut failures = Vec::new();
    for (name, action, template_pane) in [
        ("add in action", true, None),
        ("host add", false, None),
        ("unlisted template in action", true, Some(false)),
        ("listed template in action", true, Some(true)),
        ("host unlisted template", false, Some(false)),
        ("host listed template", false, Some(true)),
    ] {
        for layer in [1, 2] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
            chart.config.show_volume = false;
            chart.set_visible_bars(20);
            let rsi = if template_pane.is_none() {
                chart.add_indicator(Box::new(RSI::new(14)))
            } else {
                SeriesId(0) // Templates refer to their source by its position in the set.
            };
            let id = add_rect(&mut chart, None, [109.0, 101.0], FILL);
            chart
                .drawings_mut()
                .edit_drawing(id, |d| d.series_layer = Some(layer))
                .unwrap();
            chart.drain_events().for_each(drop);
            chart.drawings().pending_sync.lock().unwrap().clear();

            let add = |chart: &mut Chart| {
                let mut study = ConfiguredStudy::new(Box::new(SMA::new(2)));
                study
                    .set_input(
                        "source",
                        InputValue::Source(StudySource::Study { id: rsi.0, plot: 0 }),
                    )
                    .unwrap();
                if let Some(listed) = template_pane {
                    let set = StudyTemplateSet {
                        studies: vec![
                            StudyTemplate::from_study(&RSI::new(14)),
                            StudyTemplate::from_study(&study),
                        ],
                        panes: if listed { vec![vec![0, 1]] } else { vec![] },
                        ..StudyTemplateSet::default()
                    };
                    chart.apply_study_template_set(&set).unwrap();
                } else {
                    let added = chart.add_indicator(Box::new(study));
                    assert_eq!(chart.series_pane(added), chart.series_pane(rsi));
                }
            };
            if action {
                chart.user_action(add);
            } else {
                add(&mut chart);
            }
            let events: Vec<_> = chart
                .drain_events()
                .filter(|event| matches!(event, ChartEvent::Drawing { .. }))
                .collect();
            let saves = chart.drawings().pending_sync.lock().unwrap().clone();
            let actual = chart.drawings().drawings[0].series_layer;
            if actual != Some(layer) || !events.is_empty() || !saves.is_empty() {
                failures.push(format!(
                    "{name}, layer {layer}: count {actual:?}, events {events:?}, saves {saves:?}"
                ));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let rect = rect_at(&painted, FILL);
            let candles: Vec<_> = painted
                .iter()
                .enumerate()
                .filter_map(|(at, shape)| {
                    matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                        && chart.get_rendered_price_rect().contains_rect(r.rect))
                    .then_some(at)
                })
                .collect();
            assert!(!candles.is_empty());
            assert!(candles.iter().all(|&at| at < rect), "{name}, layer {layer}");
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[cfg(all(feature = "drawings", feature = "studies"))]
fn color_of(chart: &crate::widget::Chart, id: crate::widget::SeriesId) -> egui::Color32 {
    let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
    study.plot_style(0).color
}

/// Drawings cross series and studies one item at a time, keep that place through undo and
/// storage, and yield the pointer to a series painted over them, in either theme and pane.
#[cfg(all(feature = "drawings", feature = "studies"))]
#[test]
fn drawings_take_their_place_among_the_series_and_studies() {
    use crate::studies::{RSI, SMA};
    use crate::widget::VisualOrder;
    use egui::{Color32, Event};
    const FILL: Color32 = Color32::from_rgb(200, 0, 200);

    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
        chart.set_theme_override(Some(theme));
        chart.config.show_volume = false;
        chart.set_visible_bars(20);
        // Every pair of closes is 110 and 104, so SMA(2) is 107.
        let sma = chart.add_indicator(Box::new(SMA::new(2)));
        let color = chart
            .indicators()
            .find(|(id, _)| *id == sma)
            .unwrap()
            .1
            .plot_style(0)
            .color;
        let mut id = add_rect(&mut chart, None, [109.0, 101.0], FILL);
        // The step between bars and SMA is a Reorder; the extreme is a Modify.
        for (step, (order, layer)) in [
            (None, None),
            (Some(VisualOrder::SendBackward), Some(1)),
            (None, None),    // undo the step across SMA
            (None, Some(1)), // redo it
            (Some(VisualOrder::BringForward), None),
            (Some(VisualOrder::SendBackward), Some(1)),
            (Some(VisualOrder::SendToBack), Some(0)),
            (None, Some(1)), // undo the extreme
            (None, Some(0)), // redo it
            (None, Some(0)), // restore the saved drawing
            (Some(VisualOrder::BringToFront), None),
            (None, Some(2)), // a stored count at the pane's item count is also front
            (None, Some(usize::MAX)),
        ]
        .into_iter()
        .enumerate()
        {
            match step {
                2 | 7 => assert!(chart.undo()),
                3 | 8 => assert!(chart.redo()),
                11 | 12 => chart
                    .drawings_mut()
                    .edit_drawing(id, |d| d.series_layer = layer)
                    .unwrap(),
                9 => {
                    let saved = chart.save_drawings().unwrap().to_json().unwrap();
                    chart.load_drawings(&saved).unwrap();
                    id = chart.drawings().drawings[0].id;
                }
                _ => {
                    if let Some(order) = order {
                        chart.user_action(|chart| assert!(chart.reorder_drawing(id, order)));
                    }
                }
            }
            assert_eq!(
                chart.drawings().drawings[0].series_layer,
                layer,
                "step {step}"
            );
            frame(&ctx, &mut chart, step as f64, vec![]);
            let map = chart.get_chart_mapping();
            // Even bar 96 covers 100..110; odd bar 95 ends at 104, below this point.
            let on = Pos2::new(map.idx_to_x(96), map.price_to_y(105.0));
            let off = Pos2::new(map.idx_to_x(95), map.price_to_y(105.5));
            let painted = shapes(
                &ctx,
                &mut chart,
                step as f64 + 0.1,
                vec![Event::PointerMoved(on)],
            );
            let rect = rect_at(&painted, FILL);
            let candles: Vec<_> = painted
                .iter()
                .enumerate()
                .filter_map(|(at, shape)| {
                    matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                    && chart.get_rendered_price_rect().contains_rect(r.rect))
                    .then_some(at)
                })
                .collect();
            let sma = line_at(&painted, color);
            match layer {
                None => assert!(
                    *candles.last().unwrap() < rect && sma.1 < rect,
                    "step {step}"
                ),
                Some(1) => assert!(
                    *candles.last().unwrap() < rect && rect < sma.0,
                    "step {step}"
                ),
                Some(0) => assert!(rect < candles[0], "step {step}"),
                Some(_) => assert!(
                    *candles.last().unwrap() < rect && sma.1 < rect,
                    "step {step}"
                ),
            }
            assert_eq!(
                chart.drawings().hit_test(on),
                (layer != Some(0)).then_some(id),
                "step {step}"
            );
            shapes(
                &ctx,
                &mut chart,
                step as f64 + 0.2,
                vec![Event::PointerMoved(off)],
            );
            assert_eq!(chart.drawings().hit_test(off), Some(id), "step {step}");
        }
    }

    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0, 100.0], &[110.0, 104.0]);
    chart.config.show_volume = false;
    chart.set_visible_bars(20);
    let rsi = chart.add_indicator(Box::new(RSI::new(14)));
    let color = chart
        .indicators()
        .find(|(id, _)| *id == rsi)
        .unwrap()
        .1
        .plot_style(0)
        .color;
    let id = add_rect(&mut chart, Some(rsi.0), [80.0, 20.0], FILL);
    for order in [
        None,
        Some(VisualOrder::SendToBack),
        Some(VisualOrder::BringToFront),
    ] {
        if let Some(order) = order {
            chart.user_action(|chart| assert!(chart.reorder_drawing(id, order)));
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let rect = rect_at(&painted, FILL);
        let rsi = line_at(&painted, color);
        if order == Some(VisualOrder::SendToBack) {
            assert!(rect < rsi.0, "study covers rectangle");
        } else {
            assert!(rsi.1 < rect, "rectangle covers study");
        }
    }
}

/// The panel shares quote intents, replaces the legend buttons, clamps movement, counts one
/// layout edit per drag and restores its place. Old saves retain the default placement.
#[cfg(feature = "ui")]
#[test]
fn quote_panel_trades_like_the_legend_and_keeps_its_place() {
    use crate::widget::{Chart, ChartSnapshot, QuoteSide, SeriesId};
    use egui::{Event, PointerButton};
    fn label(painted: &[Shape], wanted: &str) -> Pos2 {
        let found: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.galley.text() == wanted => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .collect();
        assert_eq!(found.len(), 1, "exactly one {wanted}");
        found[0]
    }
    fn outline(painted: &[Shape]) -> Rect {
        let sell = label(painted, "SELL");
        let buy = label(painted, "BUY");
        painted
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(rect)
                    if rect.stroke.width == 1.0
                        && rect.rect.contains(sell)
                        && rect.rect.contains(buy) =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("panel outline")
    }
    let setup = || {
        let mut chart = chart(&[90.0], &[110.0]);
        chart.set_quote(Some(crate::model::Level1Quote {
            symbol: chart.symbol().into(),
            ts: chart.bars(SeriesId::MAIN)[99].time,
            bid: 99.0,
            ask: 101.0,
            bid_size: 1.0,
            ask_size: 1.0,
        }));
        chart.config_mut().quote_panel = true;
        chart
    };
    let button = |at, pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let near = |a: f32, b: f32| assert!((a - b).abs() < 0.5, "{a} != {b}");
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut time = 0.0;
        let mut draw = |chart: &mut Chart, events| {
            time += 0.1;
            shapes(&ctx, chart, time, events)
        };
        let mut chart = setup();
        draw(&mut chart, vec![]);
        let mut painted = draw(&mut chart, vec![]);
        let initial = outline(&painted);
        let free = chart.last_rendered_price_rect.shrink(8.0);
        near(initial.center().x, free.center().x);
        near(initial.top(), free.top());
        // Hiding the legend's contents leaves the independent panel available.
        chart.config_mut().show_symbol_labels = false;
        chart.config_mut().show_ohlc_info = false;
        chart.config_mut().show_ohlc_change = false;
        chart.config_mut().show_ohlc_change_percent = false;
        for (wanted, expected) in [("SELL", QuoteSide::Sell), ("BUY", QuoteSide::Buy)] {
            let at = label(&painted, wanted);
            for events in [
                vec![Event::PointerMoved(at)],
                vec![button(at, true)],
                vec![button(at, false)],
                vec![],
            ] {
                painted = draw(&mut chart, events);
            }
            assert_eq!(chart.take_quote_action(), Some(expected));
        }
        let before = chart.edit_count();
        let grip = Pos2::new(initial.left() + 11.0, initial.center().y);
        let end = grip + egui::vec2(2000.0, 2000.0);
        for events in [
            vec![Event::PointerMoved(grip)],
            vec![button(grip, true)],
            vec![Event::PointerMoved(end)],
            vec![button(end, false)],
            vec![],
        ] {
            painted = draw(&mut chart, events);
        }
        let moved = outline(&painted);
        near(moved.right(), free.right());
        near(moved.bottom(), free.bottom());
        let snapshot = chart.snapshot().unwrap();
        assert_eq!(snapshot.quote_panel, Some([1.0, 1.0]));
        assert_eq!(chart.edit_count(), before + 1);
        let mut restored = setup();
        restored.restore(&snapshot).unwrap();
        draw(&mut restored, vec![]);
        let rect = outline(&draw(&mut restored, vec![]));
        near(rect.left(), moved.left());
        near(rect.top(), moved.top());
        near(rect.right(), moved.right());
        near(rect.bottom(), moved.bottom());
        let mut old = serde_json::to_value(snapshot).unwrap();
        assert!(old.as_object_mut().unwrap().remove("quote_panel").is_some());
        let old: ChartSnapshot = serde_json::from_value(old).unwrap();
        restored.restore(&old).unwrap();
        draw(&mut restored, vec![]);
        let rect = outline(&draw(&mut restored, vec![]));
        near(rect.center().x, free.center().x);
        near(rect.top(), free.top());
        restored.config_mut().show_quote_boxes = false;
        let hidden = draw(&mut restored, vec![]);
        assert!(
            !hidden
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "BUY"))
        );
    }
}

/// Footprint cells and the row values of a session volume profile follow the instrument's
/// volume decimals, as the chart's other volume readouts do (#3738). Each of ten minute bars
/// trades 0.375 on the bid and 123.456 on the ask at 105 and spreads a volume of 456.789 evenly
/// over 100 to 110, so each of the profile's five two-point rows holds 10 × 456.789 / 5 =
/// 913.578, apart from the legend's bar volume.
#[cfg(feature = "studies")]
#[test]
fn footprint_and_session_profile_write_the_instrument_volume_decimals() {
    use crate::chart::overlays::{FootprintConfig, FootprintMode};
    use crate::data::Footprint;
    use crate::model::{
        Bar, BarClock, BarData, FootprintStatistic, Instrument, OrderSide, Tick, Timeframe,
    };
    use crate::studies::{InputValue, VolProfileSession};
    // Decimals, the bid-by-ask cell, a profile row value and the bar volume of the statistics,
    // 0.375 + 123.456; without decimals they read `0.375 × 123`, `913.58` and `124`. The trade
    // count of the statistics stays `2`.
    for (decimals, cell, row, volume) in [
        (0, "0 × 123", "914", "124"),
        (3, "0.375 × 123.456", "913.578", "123.831"),
    ] {
        let bars = (0..10)
            .map(|minute| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
                Bar::new(time, 100.0, 110.0, 100.0, 110.0, 456.789)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_symbol("FLOW");
        chart.set_instrument(Some(Instrument::new("FLOW").with_volume_decimals(decimals)));
        let mut profile = VolProfileSession::new(5);
        for (id, value) in [
            ("show_values", InputValue::Toggle(true)),
            ("display", InputValue::Choice(0)),
        ] {
            crate::studies::Indicator::set_input(&mut profile, id, value).unwrap();
        }
        chart.add_indicator(Box::new(profile));
        chart.apply_synced_time_scale(150.0, 1.0);
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for bar in chart.bars(crate::widget::SeriesId::MAIN).to_vec() {
            for (size, side) in [(0.375, OrderSide::Sell), (123.456, OrderSide::Buy)] {
                flow.push(&Tick::new(bar.time, 105.0, size, Some(side)))
                    .unwrap();
            }
        }
        let config = FootprintConfig {
            modes: vec![FootprintMode::BidAsk],
            statistics: true,
            statistics_rows: vec![FootprintStatistic::Volume, FootprintStatistic::Trades],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 500.0))),
                    ..Default::default()
                },
                |ui| {
                    chart.show_footprint(ui, &flow, &config);
                },
            );
            output.textures_delta.clear();
            texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>();
        }
        for expected in [cell, row, volume, "2"] {
            assert!(
                texts.iter().any(|text| text == expected),
                "volume decimals {decimals}: {expected} not in {texts:?}"
            );
        }
    }
}
