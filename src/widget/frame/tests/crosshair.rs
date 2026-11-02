use crate::widget::test_frames::{chart, frame, shapes};
use egui::{Event, Pos2, Shape};

/// An update starts one last price pulse, loading does not, and a continuous pulse always
/// runs; the crosshair marker sits on the line at the hovered bar's close.
#[test]
fn line_pulse_and_crosshair_marker_follow_their_bars() {
    use crate::model::{ChartType, LastPriceAnimation};
    use egui::Color32;
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0]);
    chart.set_chart_type(ChartType::Line);
    let mut params = chart.chart_type_params().clone();
    params.styles.last_price_animation = LastPriceAnimation::OnDataUpdate;
    params.styles.crosshair_marker_radius = 5.0;
    chart.set_chart_type_params(params.clone());
    let color = params.styles.line.color;
    let circles = |shapes: Vec<Shape>, radius: f32| {
        shapes
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Circle(circle) if circle.radius == radius && circle.fill == color => {
                    Some(circle.center)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(circles(shapes(&ctx, &mut chart, 0.0, vec![]), 3.0).is_empty());
    let mut last = chart.bars(crate::widget::SeriesId::MAIN)[99].clone();
    last.close = 105.0;
    chart.update(crate::widget::SeriesId::MAIN, last);
    let map = chart.get_chart_mapping();
    assert_eq!(
        circles(shapes(&ctx, &mut chart, 1.0, vec![]), 3.0),
        vec![Pos2::new(map.idx_to_x(99), map.price_to_y(105.0))]
    );
    assert!(circles(shapes(&ctx, &mut chart, 3.7, vec![]), 3.0).is_empty());
    params.styles.last_price_animation = LastPriceAnimation::Continuous;
    chart.set_chart_type_params(params);
    assert_eq!(circles(shapes(&ctx, &mut chart, 5.0, vec![]), 3.0).len(), 1);

    let map = chart.get_chart_mapping();
    let hover = Pos2::new(map.idx_to_x(70), chart.last_rendered_price_rect.center().y);
    assert_eq!(
        circles(
            shapes(&ctx, &mut chart, 5.1, vec![Event::PointerMoved(hover)]),
            5.0
        ),
        vec![Pos2::new(map.idx_to_x(70), map.price_to_y(110.0))]
    );
    // Trailing whitespace leaves the pulse on the last valued bar; both dots use that
    // bar's own colour, and a hovered empty slot has no dot. Vary the hover height to
    // stay outside the price-action button beside the last bars.
    let mut bars = chart.bars(crate::widget::SeriesId::MAIN).to_vec();
    bars[98].colors = Some(Box::new(crate::model::BarColors {
        color: Some(Color32::GOLD),
        ..Default::default()
    }));
    bars[99] = crate::model::Bar::whitespace(bars[99].time);
    chart.set_data(crate::widget::SeriesId::MAIN, bars);
    let map = chart.get_chart_mapping();
    for (time, index, marker) in [(5.2, 98, true), (5.3, 99, false)] {
        let hover = Pos2::new(
            map.idx_to_x(index),
            chart.last_rendered_price_rect.center().y + if marker { 20.0 } else { -20.0 },
        );
        let painted = shapes(&ctx, &mut chart, time, vec![Event::PointerMoved(hover)]);
        for radius in [3.0, 5.0] {
            let dots: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(dot) if dot.radius == radius && dot.fill == Color32::GOLD => {
                        Some(dot.center)
                    }
                    _ => None,
                })
                .collect();
            let expected = if radius == 3.0 || marker {
                vec![Pos2::new(map.idx_to_x(98), map.price_to_y(110.0))]
            } else {
                Vec::new()
            };
            assert_eq!(dots, expected, "{radius} at {index}");
        }
    }
}

/// Held values appear only for a pressed pointer and disappear on release or disable. A
/// hidden crosshair keeps them and shows no axis label while the pointer is held.
#[test]
fn held_values_follow_press_release_and_switch() {
    for (_, theme) in crate::theme::presets() {
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = chart(&[100.0], &[110.0, 120.0]);
        chart.config.bullish_color = egui::Color32::BLUE;
        chart.config.show_ohlc_info = true;
        chart.chart_options.crosshair.style = crate::config::CrosshairStyle::Hidden;
        chart.chart_options.tooltip.on_long_press = true;
        chart.chart_options.tooltip.show_time = false;
        #[cfg(feature = "studies")]
        chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let map = chart.get_chart_mapping();
        let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
        for (step, enabled, pressed, expected, volume_details) in [
            (1, true, false, false, false),
            (2, true, true, true, false),
            (3, true, true, true, true),
            (4, true, false, false, false),
            (5, false, true, false, false),
            (6, false, false, false, false),
        ] {
            chart.set_tooltip_enabled(enabled);
            chart.chart_options.tooltip.show_volume_details = volume_details;
            let painted = shapes(
                &ctx,
                &mut chart,
                step as f64 * 0.1,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels.contains(&"Open"),
                expected,
                "step {step}: {labels:?}"
            );
            assert_eq!(chart.axis_hover, None, "step {step}");
            assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                if text.galley.text() == "O 100.00" && text.galley.job.sections[1].format.color == egui::Color32::BLUE)));
            if expected {
                assert_eq!(
                    labels.iter().any(|label| label.starts_with("Volume:")),
                    volume_details && cfg!(feature = "studies"),
                    "volume study details are opt-in"
                );
                assert!(labels.contains(&"Time"));
                assert!(!labels.contains(&"Change"));
                assert!(labels.contains(&"Vol"));
                #[cfg(feature = "studies")]
                {
                    assert!(labels.contains(&"SMA"), "other study values remain visible");
                    assert!(
                        chart
                            .study_values_at(50)
                            .iter()
                            .any(|row| row.study == "Volume")
                    );
                }
                let legend = labels
                    .iter()
                    .position(|label| *label == "O 100.00")
                    .unwrap();
                let tooltip = labels.iter().position(|label| *label == "Open").unwrap();
                assert!(
                    tooltip > legend,
                    "the floating readout must paint above the legend"
                );
                assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                    if text.galley.text() == "110.00" && text.galley.job.sections[0].format.color == egui::Color32::BLUE)));
            }
        }
    }
}

#[test]
fn runtime_options_size_and_crosshair_position_apply_to_a_built_chart() {
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[200.0]);
    frame(&ctx, &mut chart, 0.0, Vec::new());
    // Unrelated options keep the view; new bounds clamp it; spacing and offset apply.
    chart.apply_synced_time_scale(3.0, -20.0);
    let mut options = chart.chart_options().clone();
    options.crosshair.mode = crate::config::CrosshairMode::Magnet;
    chart.set_chart_options(options.clone());
    assert_eq!(chart.get_time_scale_state(), (3.0, -20.0));
    // Invalid options retain the configured values, even after a pan and repeated applies.
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for field in 0..3 {
            let mut invalid_options = options.clone();
            match field {
                0 => invalid_options.time_scale.right_offset = invalid,
                1 => invalid_options.time_scale.right_offset_pixels = Some(invalid),
                _ => invalid_options.time_scale.bar_spacing = invalid,
            }
            chart.apply_synced_time_scale(3.0, -30.0);
            for _ in 0..3 {
                chart.set_chart_options(invalid_options.clone());
                assert_eq!(
                    chart.get_time_scale_state(),
                    (3.0, -30.0),
                    "{field}, {invalid}"
                );
            }
            assert_eq!(chart.chart_options(), &options);
            chart = chart.with_chart_options(invalid_options);
            assert_eq!(chart.chart_options(), &options);
            chart.apply_synced_time_scale(3.0, -30.0);
            chart.set_chart_options(chart.chart_options().clone());
            assert_eq!(chart.get_time_scale_state(), (3.0, -30.0));
        }
    }
    chart.apply_synced_time_scale(3.0, -20.0);
    options.time_scale.min_bar_spacing = 4.0;
    chart.set_chart_options(options.clone());
    assert_eq!(chart.get_time_scale_state(), (4.0, -20.0));
    options.time_scale.bar_spacing = 8.0;
    options.time_scale.right_offset = 5.0;
    chart.set_chart_options(options.clone());
    assert_eq!(chart.get_time_scale_state(), (8.0, 5.0));
    assert_eq!(chart.chart_options(), &options);

    chart.set_size(Some(egui::vec2(300.0, 200.0)));
    frame(&ctx, &mut chart, 0.1, Vec::new());
    let plot = chart.get_rendered_price_rect();
    assert!(plot.max.x < 300.0 && plot.max.y < 200.0 && plot.width() > 200.0);
    chart.set_size(None);

    // The price label follows the host's price, which no axis tick shows.
    let time = chart.state.shown()[50].time;
    let label = |chart: &mut crate::widget::Chart, t| {
        shapes(&ctx, chart, t, Vec::new())
            .iter()
            .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "137.50"))
    };
    chart.set_crosshair_position(Some((time, 137.5)));
    assert!(label(&mut chart, 0.2));
    chart.set_synced_crosshair_time(Some(time));
    assert!(!label(&mut chart, 0.3));
}

#[test]
fn hovered_series_draw_above_the_others_when_enabled() {
    use crate::studies::{CustomIndicator, IndicatorValue};
    use egui::Color32;
    let levels = [
        (150.0, Color32::from_rgb(201, 1, 1)),
        (170.0, Color32::from_rgb(1, 201, 1)),
    ];
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[200.0]);
    chart.chart_options.tooltip.enabled = false;
    let mut study_ids = Vec::new();
    for (level, color) in levels {
        study_ids.push(
            chart.add_indicator(Box::new(
                CustomIndicator::new("Level", move |bars| {
                    vec![IndicatorValue::Single(level); bars.len()]
                })
                .with_color(color),
            )),
        );
    }
    let added_color = Color32::from_rgb(1, 1, 201);
    let added = chart.add_series(crate::widget::SeriesOptions {
        color: added_color,
        ..Default::default()
    });
    let bars = chart
        .bars(crate::widget::SeriesId::MAIN)
        .iter()
        .map(|bar| crate::model::Bar::from_value(bar.time, 130.0))
        .collect();
    chart.set_data(added, bars);
    frame(&ctx, &mut chart, 0.0, Vec::new());
    let map = chart.get_chart_mapping();
    let candle = chart.config().bullish_color;
    let first = |shapes: &[Shape], color| {
        shapes.iter().position(|shape| match shape {
            Shape::Rect(rect) => rect.fill == color,
            Shape::LineSegment { stroke, .. } => stroke.color == color,
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
            Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| vertex.color == color),
            _ => false,
        })
    };
    // (enabled, hovered price, colours from the bottom up)
    for (t, (enabled, price, order)) in [
        (
            false,
            120.0,
            [candle, added_color, levels[0].1, levels[1].1],
        ),
        (true, 120.0, [added_color, levels[0].1, levels[1].1, candle]),
        (true, 150.0, [candle, added_color, levels[1].1, levels[0].1]),
        (true, 130.0, [candle, levels[0].1, levels[1].1, added_color]),
    ]
    .into_iter()
    .enumerate()
    {
        let t = t as f64;
        chart.chart_options.hovered_series_on_top = enabled;
        let pointer = || {
            vec![Event::PointerMoved(Pos2::new(
                map.idx_to_x(90),
                map.price_to_y(price),
            ))]
        };
        frame(&ctx, &mut chart, t + 0.1, pointer());
        let painted = shapes(&ctx, &mut chart, t + 0.2, pointer());
        let positions = order.map(|color| first(&painted, color).unwrap());
        assert!(positions.is_sorted(), "{enabled} {price}: {positions:?}");
    }
    let bars_at = |price| {
        chart
            .bars(crate::widget::SeriesId::MAIN)
            .iter()
            .map(|bar| crate::model::Bar::from_value(bar.time, price))
            .collect::<Vec<_>>()
    };
    let overlapping = bars_at(150.0);
    let original = bars_at(130.0);
    chart.set_data(added, overlapping);
    for (step, order, expected) in [
        (
            0,
            crate::widget::VisualOrder::SendBackward,
            crate::widget::ChartElementId::Series(added),
        ),
        (
            1,
            crate::widget::VisualOrder::BringToFront,
            crate::widget::ChartElementId::Indicator(study_ids[0]),
        ),
    ] {
        assert!(chart.reorder_indicator(study_ids[0], order));
        chart.drain_events().for_each(drop);
        frame(
            &ctx,
            &mut chart,
            4.1 + step as f64 * 0.1,
            vec![Event::PointerMoved(Pos2::new(
                map.idx_to_x(90),
                map.price_to_y(150.0),
            ))],
        );
        assert!(
            chart.drain_events().any(|event| matches!(event,
                crate::widget::ChartEvent::CrosshairMoved(Some(pointer))
                    if pointer.target == Some(crate::widget::PointerTarget::Element(expected))
            )),
            "visual order {order:?}"
        );
    }
    chart.set_data(added, original);
    let pane = chart.add_pane();
    for id in study_ids.iter().copied().chain([added]) {
        assert!(chart.move_series_to_pane(id, pane));
    }
    for (step, (enabled, price, order)) in [
        (false, 150.0, [levels[0].1, levels[1].1, added_color]),
        (true, 150.0, [levels[1].1, added_color, levels[0].1]),
        (true, 130.0, [levels[0].1, levels[1].1, added_color]),
    ]
    .into_iter()
    .enumerate()
    {
        chart.chart_options.hovered_series_on_top = enabled;
        frame(&ctx, &mut chart, 5.0 + step as f64, vec![]);
        let map = chart.series_price_map(added).unwrap();
        let pos = Pos2::new(
            chart.get_chart_mapping().idx_to_x(90),
            map.price_to_y(price),
        );
        let painted = shapes(
            &ctx,
            &mut chart,
            5.1 + step as f64,
            vec![Event::PointerMoved(pos)],
        );
        let positions = order.map(|color| first(&painted, color).unwrap());
        assert!(
            positions.is_sorted(),
            "pane {enabled} {price}: {positions:?}"
        );
    }
}

/// The crosshair's time label names the bar nearest the pointer, and a chart that follows
/// another one's index names the same bar. The bars are daily from 2023-11-14, so bar 50 is
/// 3 Jan 2024 and bar 51 is 4 Jan 2024; 0.7 of a bar past the centre of bar 50 is nearer to 51.
#[test]
fn crosshair_time_label_names_the_nearest_bar() {
    use crate::model::{ChartType, ChartTypeParams};
    let mut highs = vec![120.0; 100];
    highs[99] = 3120.0;
    // (type, highs, element index, pointer over chart, synced index, expected label)
    for case in [
        (
            ChartType::Candles,
            &[200.0][..],
            50,
            true,
            None,
            "Jan 04, 2024",
            false,
        ),
        (
            ChartType::Candles,
            &[200.0][..],
            50,
            false,
            Some(50.7),
            "Jan 04, 2024",
            false,
        ),
        #[cfg(feature = "studies")]
        (
            ChartType::Candles,
            &[200.0][..],
            50,
            true,
            None,
            "Jan 04, 2024",
            true,
        ),
        // Bar 99 alone completes 300 bricks, all dated 21 February 2024.
        (
            ChartType::Renko,
            highs.as_slice(),
            250,
            true,
            None,
            "Feb 21, 2024",
            false,
        ),
    ] {
        let (kind, highs, index, pointer, synced, expected, panes) = case;
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], highs);
        chart.chart_options.tooltip.enabled = false;
        #[cfg(feature = "studies")]
        if panes {
            chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
        }
        chart.set_chart_type(kind);
        chart.set_chart_type_params(ChartTypeParams {
            renko_brick_size: 10.0,
            ..Default::default()
        });
        chart.set_synced_crosshair_bar_idx(synced);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        let map = chart.get_chart_mapping();
        let hover = Pos2::new(
            map.idx_to_x(index) + 0.7 * map.bar_spacing,
            map.rect.center().y,
        );
        let events = || {
            pointer
                .then_some(Event::PointerMoved(hover))
                .into_iter()
                .collect()
        };
        frame(&ctx, &mut chart, 0.1, events());
        let painted = shapes(&ctx, &mut chart, 0.2, events());
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.galley.text().ends_with(", 2024") => {
                    if panes {
                        let bottom = chart
                            .get_rendered_indicator_panes()
                            .last()
                            .unwrap()
                            .panel_rect
                            .bottom();
                        assert!(text.pos.y >= bottom, "time tag above lowest pane");
                    }
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect();
        assert_eq!(labels, [expected], "{case:?}");
        let time = chart.state.shown()[index + 1].time;
        assert_eq!(chart.format_time(time), expected, "host readout: {case:?}");
    }
}
