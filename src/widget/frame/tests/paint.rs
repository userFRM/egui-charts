use crate::widget::test_frames::{candle_ys, chart, frame, shapes};
use egui::{Pos2, Rect, Shape};

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

/// Renders the two-bar footprint example of #2458 whose lowest bar low is `low`, at `spacing`
/// points per bar, and returns the shapes with the chart.
fn host_footprint(
    low: f64,
    config: &crate::chart::overlays::FootprintConfig,
    spacing: f32,
) -> (Vec<egui::epaint::ClippedShape>, crate::widget::Chart) {
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    let ctx = egui::Context::default();
    let mut chart = chart(&[low, low + 4.0], &[low + 6.0, low + 10.0]);
    chart.set_symbol("FLOW");
    chart.apply_synced_time_scale(spacing, 1.0);
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
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

/// A low of 103 lies in the four-tick row 100..103, whose bottom edge is 99.5.
/// The fit must leave the summary's room below that edge, including with single-tick rows and
/// with automatic rows, which never group below `row_ticks` and ignore an unusable height.
#[test]
fn footprint_summary_below_a_grouped_lowest_row_stays_in_the_plot() {
    use crate::chart::overlays::FootprintConfig;
    for (auto_row_height, row_ticks, low, edge) in [
        (None, 1, 100.0, 99.5),
        (None, 4, 103.0, 99.5),
        (Some(16.0), 4, 103.0, 99.5),
        (Some(0.0), 4, 103.0, 99.5),
        (Some(f32::NAN), 4, 103.0, 99.5),
    ] {
        let config = FootprintConfig {
            auto_row_height,
            row_ticks,
            ..Default::default()
        };
        let (shapes, chart) = host_footprint(low, &config, 160.0);
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
            "auto {auto_row_height:?}, row_ticks {row_ticks}: plot bottom {}, low {low} at {}, row edge at {edge}, Delta at {boxes:?}",
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
                    "auto {auto_row_height:?}, row_ticks {row_ticks}: bar {index} keeps {label} below its lowest row"
                );
            }
        }
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
            let (_, chart) = host_footprint(103.0, &config, 3.0);
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
    let cases = [
        (false, true, Duration::MAX, None, ChartType::Candles),
        (true, true, Duration::from_secs(1), None, ChartType::Candles),
        (true, false, Duration::MAX, None, ChartType::Candles),
    ]
    .into_iter()
    .chain([0, 1, 128].into_iter().flat_map(|count| {
        [
            ChartType::Candles,
            ChartType::Line,
            ChartType::Area,
            ChartType::Heikin,
        ]
        .map(move |kind| (false, true, Duration::MAX, Some(count), kind))
    }));
    for (server_time, axis, delay, flat_count, kind) in cases {
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
        chart.set_server_time(
            server_time.then(|| {
                chart.state.data().bars.last().unwrap().time + chrono::Duration::seconds(30)
            }),
        );
        let mut run = || {
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
                if delay == Duration::MAX {
                    assert!(
                        shapes == first,
                        "the first frame must draw the settled chart"
                    );
                }
                assert_eq!(actual, delay, "server time {server_time}, axis {axis}");
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
    struct Executions(bool);
    impl Datafeed for Executions {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            let high = if self.0 { 102.0 } else { 101.0 };
            reply.send(Ok(HistoryPage::new(
                vec![Bar::new(time, 100.0, high, 100.0, 101.0, 10.0)],
                true,
            )));
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            let time = "2026-01-05T10:00:00Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap();
            let levels = if self.0 {
                vec![(100.0, 1.0), (101.0, 6.0), (102.0, 3.0)]
            } else {
                vec![(100.0, 2.0), (101.0, 3.0)]
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
        .with_feed(std::sync::Arc::new(Executions(false)))
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
                flow.price_step(),
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
    let params = ChartTypeParams {
        footprint_modes: vec![FootprintMode::Delta],
        footprint_row_ticks: 1,
        footprint_statistics: true,
        footprint_statistics_rows: vec![FootprintStatistic::Volume, FootprintStatistic::Delta],
        profile: TradeProfileSettings {
            value_area: Some(0.5),
            ..Default::default()
        },
        ..Default::default()
    };
    chart.user_action(|chart| chart.set_chart_type_params(params.clone()));
    let shapes = draw(&mut chart);
    let bottom = chart.get_rendered_price_rect().bottom();
    let labels: Vec<_> = shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(text) if text.pos.y >= bottom && text.pos.y < bottom + 28.0 => {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["Vol", "Delta", "5", "1"]);
    assert_eq!(chart.trade_profile_settings().value_area, Some(0.5));
    let saved = serde_json::to_vec(&chart.snapshot().unwrap()).unwrap();
    let snapshot: ChartSnapshot = serde_json::from_slice(&saved).unwrap();
    let mut restored = Chart::new(crate::model::BarData::new());
    restored.restore(&snapshot).unwrap();
    assert_eq!(restored.chart_type_params(), &params);
    assert!(chart.undo());
    assert_eq!(chart.trade_profile_settings().value_area, Some(0.7));
    assert!(chart.redo());
    assert_eq!(chart.chart_type_params(), &params);
    let mut profile = ChartBuilder::new()
        .with_feed(std::sync::Arc::new(Executions(true)))
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
