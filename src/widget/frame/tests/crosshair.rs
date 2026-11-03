use crate::widget::test_frames::{chart, frame, shapes};
use egui::{Event, Pos2, Shape};

#[test]
fn programmatic_crosshair_uses_the_target_series_scale_and_pane() {
    use crate::model::{Bar, LineStyle, SeriesScale};
    use crate::scales::CustomPriceFormatter;
    use crate::widget::{ImageExportOptions, SeriesId, SeriesOptions};

    for (scale, secondary, inverted, above) in [
        (Some(SeriesScale::Left), false, false, false),
        (None, false, false, false),
        (Some(SeriesScale::Left), false, true, false),
        (Some(SeriesScale::Overlay(1)), false, false, false),
        (Some(SeriesScale::Left), true, false, false),
        (Some(SeriesScale::Left), true, false, true),
    ] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        chart.config.show_volume = false;
        chart.chart_options.tooltip.enabled = false;
        chart.chart_options.crosshair.vert_line_color = egui::Color32::MAGENTA;
        chart.chart_options.crosshair.horz_line_color = egui::Color32::MAGENTA;
        chart.chart_options.crosshair.vert_line_style = LineStyle::Solid;
        chart.chart_options.crosshair.horz_line_style = LineStyle::Solid;
        let id = scale.map_or(SeriesId::MAIN, |scale| {
            let id = chart.add_series(SeriesOptions {
                scale,
                ..Default::default()
            });
            let bars = chart
                .bars(SeriesId::MAIN)
                .iter()
                .enumerate()
                .map(|(i, bar)| Bar::from_value(bar.time, if i % 2 == 0 { 1000.0 } else { 2000.0 }))
                .collect();
            chart.set_data(id, bars);
            if secondary {
                let pane = chart.add_pane();
                chart.move_series_to_pane(id, pane);
                if above {
                    chart.move_pane(pane, 0);
                }
            }
            id
        });
        chart.set_series_formatter(
            id,
            Some(std::sync::Arc::new(CustomPriceFormatter::new(|price| {
                format!("target {price:.2}")
            }))),
        );
        frame(&ctx, &mut chart, 0.0, vec![]);
        let price = if scale.is_some() { 1375.0 } else { 137.5 };
        let target_scale = chart.series_price_scale_mut(id).unwrap();
        target_scale.set_range(
            if scale.is_some() { 1000.0 } else { 100.0 },
            if scale.is_some() { 2000.0 } else { 200.0 },
        );
        target_scale.options_mut().invert_scale = inverted;
        let time = chart.bars(SeriesId::MAIN)[50].time;
        let label = format!("target {price:.2}");
        let lines = |painted: &[Shape], horizontal| {
            painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == egui::Color32::MAGENTA
                            && if horizontal {
                                points[0].y == points[1].y
                            } else {
                                points[0].x == points[1].x
                            } =>
                    {
                        Some(*points)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        chart.set_series_crosshair_position(id, Some((time, price)));
        let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerGone]);
        let main = chart.get_rendered_price_rect();
        let pane = if secondary {
            chart
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.members.contains(&id))
                .unwrap()
                .rect
        } else {
            main
        };
        // 137.5 in [100, 200], or 1375 in [1000, 2000], is 3/8 up the range.
        let expected_y = pane.top() + pane.height() * if inverted { 0.375 } else { 0.625 };
        let horizontal = lines(&painted, true);
        assert_eq!(horizontal.len(), 1, "{scale:?} {secondary} {inverted}");
        assert!(
            (horizontal[0][0].y - expected_y).abs() < 0.02,
            "{horizontal:?}, expected {expected_y}"
        );
        let vertical = lines(&painted, false);
        let x = chart.get_chart_mapping().idx_to_x(50);
        assert!(!vertical.is_empty());
        assert!(vertical.iter().all(|line| (line[0].x - x).abs() < 0.02));
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == chart.format_time(time))));
        let tag = painted.iter().find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text() == label => Some(text),
            _ => None,
        });
        let axis = secondary || scale != Some(SeriesScale::Overlay(1));
        assert_eq!(tag.is_some(), axis, "{scale:?} {secondary}");
        if let Some(tag) = tag {
            assert!(pane.y_range().contains(tag.pos.y));
            if scale == Some(SeriesScale::Left) && !secondary {
                assert!(tag.pos.x < main.left());
            }
        }
        let mut snapshot = chart.export_snapshot(
            &ctx,
            &ImageExportOptions {
                include_crosshair: true,
                ..Default::default()
            },
        );
        let exported = shapes(&ctx, &mut snapshot, 0.2, vec![]);
        assert_eq!(lines(&exported, true), horizontal);
        // The pointer wins, then the saved host position returns when the pointer leaves.
        for (step, rect) in [Some(main), secondary.then_some(pane)]
            .into_iter()
            .flatten()
            .enumerate()
        {
            let pointer = Pos2::new(x, rect.top() + rect.height() * 0.25);
            let painted = shapes(
                &ctx,
                &mut chart,
                0.3 + step as f64 * 0.2,
                vec![Event::PointerMoved(pointer)],
            );
            assert_eq!(chart.get_hover_bar_idx(), Some(50.0));
            assert!(
                !painted
                    .iter()
                    .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == label))
            );
            let painted = shapes(
                &ctx,
                &mut chart,
                0.4 + step as f64 * 0.2,
                vec![Event::PointerGone],
            );
            assert_eq!(lines(&painted, true), horizontal);
        }
        for (step, position) in [
            None,
            Some((time, f64::NAN)),
            Some((time, f64::INFINITY)),
            Some((time, f64::NEG_INFINITY)),
            Some((time - chrono::TimeDelta::days(100), price)),
            Some((time + chrono::TimeDelta::days(100), price)),
        ]
        .into_iter()
        .enumerate()
        {
            chart.set_series_crosshair_position(id, Some((time, price)));
            chart.set_series_crosshair_position(id, position);
            let painted = shapes(&ctx, &mut chart, 1.0 + step as f64, vec![]);
            assert!(lines(&painted, true).is_empty());
            assert!(lines(&painted, false).is_empty());
            assert!(chart.last_crosshair.is_none());
        }
        chart.set_series_crosshair_position(id, Some((time, price)));
        chart.remove_series(id);
        let painted = shapes(&ctx, &mut chart, 10.0, vec![]);
        assert!(lines(&painted, true).is_empty());
        assert!(lines(&painted, false).is_empty());
        assert!(chart.last_crosshair.is_none());
    }
}

/// Eligibility follows current data and visibility, independently of price snapping, across
/// panes and host positioning. Distances are logical slots: at 50.5, 48 is nearer than 54;
/// at 51 they tie, so 54 wins. An added value at 51 wins over either neighbour.
#[test]
fn crosshair_skips_ineligible_indices_without_removing_data() {
    use crate::config::CrosshairMode;
    use crate::model::{Bar, LineStyle};
    use crate::widget::{ChartEvent, SeriesId, SeriesOptions};

    let vertical_lines = |painted: &[Shape]| {
        painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::MAGENTA && points[0].x == points[1].x =>
                {
                    Some(points[0].x)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for pane in [false, true] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config.show_volume = false;
        let original = chart.bars(SeriesId::MAIN).to_vec();
        let mut bars = original.clone();
        for bar in &mut bars[49..54] {
            *bar = Bar::whitespace(bar.time);
        }
        chart.set_data(SeriesId::MAIN, bars.clone());
        let added = chart.add_series(SeriesOptions::default());
        if pane {
            let id = chart.add_pane();
            assert!(chart.move_series_to_pane(added, id));
        }
        chart.set_data(added, vec![Bar::from_value(original[51].time, 105.0)]);
        chart.chart_options.crosshair.vert_line_color = egui::Color32::MAGENTA;
        chart.chart_options.crosshair.vert_line_style = LineStyle::Solid;
        chart.chart_options.crosshair.horz_line_color = egui::Color32::MAGENTA;
        chart.chart_options.crosshair.horz_line_style = LineStyle::Solid;
        chart.chart_options.tooltip.enabled = false;
        frame(&ctx, &mut chart, 0.0, vec![]);
        // (skip whitespace, skip hidden-only, main visible, added visible, added valued,
        //  pointer index, expected index). These edits deliberately reuse the same chart.
        for (step, case) in [
            (false, false, true, false, true, 51.0, Some(51)),
            (true, false, true, false, true, 51.0, Some(51)),
            (false, true, true, false, true, 51.0, Some(52)),
            (false, true, true, false, false, 51.0, Some(51)),
            (true, true, true, false, true, 50.5, Some(48)),
            (true, true, true, false, true, 51.0, Some(54)),
            (true, true, true, true, true, 51.0, Some(51)),
            (true, true, true, true, false, 51.0, Some(54)),
            (true, true, true, true, true, 51.0, Some(51)),
            (true, true, false, true, true, 48.0, Some(51)),
            (true, true, false, false, true, 51.0, None),
            (true, false, false, false, false, 51.0, Some(54)),
        ]
        .into_iter()
        .enumerate()
        {
            let (whitespace, hidden, main_visible, added_visible, valued, index, expected) = case;
            let mut options = chart.chart_options().clone();
            options.time_scale.ignore_whitespace_indices = whitespace;
            options.time_scale.ignore_hidden_indices = hidden;
            options.crosshair.mode = [
                CrosshairMode::Normal,
                CrosshairMode::Magnet,
                CrosshairMode::MagnetClose,
            ][step % 3];
            chart.set_chart_options(options);
            chart
                .series_price_scale_mut(SeriesId::MAIN)
                .unwrap()
                .options_mut()
                .mode = if step % 2 == 1 {
                crate::scales::PriceScaleMode::Logarithmic
            } else {
                crate::scales::PriceScaleMode::Normal
            };
            chart.set_main_series_visible(main_visible);
            chart.set_series_visible(added, added_visible);
            chart.update(
                added,
                if valued {
                    Bar::from_value(original[51].time, 105.0)
                } else {
                    Bar::whitespace(original[51].time)
                },
            );
            let time = 1.0 + step as f64;
            frame(&ctx, &mut chart, time, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
            let map = chart.get_chart_mapping();
            let y = if pane && added_visible {
                chart.series_price_map(added).unwrap().price_to_y(105.0)
            } else {
                map.rect.center().y
            };
            let pos = Pos2::new(map.idx_to_x(50) + (index - 50.0) * map.bar_spacing, y);
            let painted = shapes(&ctx, &mut chart, time + 0.1, vec![Event::PointerMoved(pos)]);
            assert_eq!(
                chart.get_hover_bar_idx(),
                expected.map(|i| i as f64),
                "{pane} {case:?}, pointer {pos:?}, before {map:?}, after {:?}",
                chart.get_chart_mapping()
            );
            let pointer = chart
                .drain_events()
                .find_map(|event| match event {
                    ChartEvent::CrosshairMoved(pointer) => Some(pointer),
                    _ => None,
                })
                .flatten();
            assert_eq!(
                pointer.as_ref().map(|p| p.index),
                expected.map(|i| i as isize),
                "{pane} {case:?}"
            );
            if let Some(pointer) = pointer {
                assert_eq!(pointer.time, original[pointer.index as usize].time);
                assert_eq!(pointer.pos, pos, "host retains the actual pointer position");
            }
            let lines = vertical_lines(&painted);
            if let Some(index) = expected {
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        Shape::LineSegment { points, stroke }
                            if stroke.color == egui::Color32::MAGENTA
                                && points[0].y == points[1].y && points[0].y.is_finite()
                    )),
                    "{pane} {case:?}: missing horizontal crosshair"
                );
                assert!(!lines.is_empty(), "{pane} {case:?}");
                assert!(
                    lines
                        .iter()
                        .all(|x| (*x - map.idx_to_x(index)).abs() < 0.01),
                    "{pane} {case:?}: {lines:?}"
                );
                let label = chart.format_time(original[index].time);
                assert!(
                    painted.iter().any(
                        |shape| matches!(shape, Shape::Text(text) if text.galley.text() == label)
                    ),
                    "{pane} {case:?}: missing {label}"
                );
            } else {
                assert!(lines.is_empty(), "{pane} {case:?}");
                assert!(
                    chart.last_crosshair.is_none(),
                    "clear the previous sync position"
                );
            }
            assert_eq!(chart.bars(SeriesId::MAIN).len(), bars.len());
            assert!(chart.bars(SeriesId::MAIN)[51].is_whitespace());
        }
        chart.set_main_series_visible(true);
        chart.set_series_visible(added, true);
        chart.chart_options.time_scale.ignore_hidden_indices = true;
        for (step, filter, index, expected) in [
            (0, false, 50.5, 51),
            (1, true, 50.5, 48),
            (2, true, 51.0, 54),
        ] {
            chart.chart_options.time_scale.ignore_whitespace_indices = filter;
            chart.chart_options.time_scale.ignore_hidden_indices = filter;
            chart.set_synced_crosshair_bar_idx(Some(index));
            let painted = shapes(
                &ctx,
                &mut chart,
                20.0 + step as f64,
                vec![Event::PointerGone],
            );
            assert_eq!(
                chart.last_crosshair.map(|(time, _)| time),
                Some(original[expected].time)
            );
            let label = chart.format_time(original[expected].time);
            assert!(
                painted
                    .iter()
                    .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == label))
            );
            let lines = vertical_lines(&painted);
            let x = chart.get_chart_mapping().idx_to_x(expected);
            assert!(!lines.is_empty());
            assert!(
                lines.iter().all(|line| (*line - x).abs() < 0.01),
                "{pane} {index}: {lines:?}"
            );
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    Shape::LineSegment { points, stroke }
                        if stroke.color == egui::Color32::MAGENTA && points[0].y == points[1].y
                            && points[0].y.is_finite()
                )),
                "a synchronized whitespace slot keeps the horizontal crosshair"
            );
        }
        // Replacing history fills a gap immediately; replacing all values clears the cursor.
        chart.set_synced_crosshair_bar_idx(None);
        bars[51] = original[51].clone();
        let whitespace: Vec<_> = original
            .iter()
            .map(|bar| Bar::whitespace(bar.time))
            .collect();
        for (step, (data, expected)) in [
            (bars.clone(), Some(51.0)),
            (whitespace.clone(), None),
            (bars.clone(), Some(51.0)),
            (Vec::new(), None),
        ]
        .into_iter()
        .enumerate()
        {
            if data.is_empty() {
                chart.set_data(added, Vec::new());
            }
            chart.set_data(SeriesId::MAIN, data);
            let map = chart.get_chart_mapping();
            frame(
                &ctx,
                &mut chart,
                23.0 + step as f64,
                vec![Event::PointerMoved(Pos2::new(
                    map.idx_to_x(51),
                    map.rect.center().y,
                ))],
            );
            assert_eq!(chart.get_hover_bar_idx(), expected);
            assert_eq!(chart.last_crosshair.is_some(), expected.is_some());
        }
        chart.set_data(SeriesId::MAIN, whitespace);
        let study = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
            "Value in gap",
            |bars| {
                (0..bars.len())
                    .map(|index| {
                        if index == 51 {
                            crate::studies::IndicatorValue::Single(105.0)
                        } else {
                            crate::studies::IndicatorValue::None
                        }
                    })
                    .collect()
            },
        )));
        for (step, (hidden, visible, plot_visible, expected)) in [
            (true, true, true, Some(51.0)),
            (true, false, true, None),
            (false, false, true, Some(51.0)),
            (true, true, false, None),
            (false, true, false, Some(51.0)),
        ]
        .into_iter()
        .enumerate()
        {
            chart.chart_options.time_scale.ignore_hidden_indices = hidden;
            chart.set_indicator_visible(study, visible);
            let indicator = chart.indicator_mut(study).unwrap();
            let mut settings = indicator.settings();
            settings.plots[0].visible = plot_visible;
            indicator.apply_settings(&[], settings).unwrap();
            let map = chart.get_chart_mapping();
            frame(
                &ctx,
                &mut chart,
                28.0 + step as f64,
                vec![Event::PointerMoved(Pos2::new(
                    map.idx_to_x(51),
                    map.rect.center().y,
                ))],
            );
            assert_eq!(
                chart.get_hover_bar_idx(),
                expected,
                "study {hidden} {visible} {plot_visible}"
            );
        }
    }
}

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
