use super::*;
use crate::model::{BarData, LineStyle, SeriesDisplayFormat, SeriesFormat};
use crate::tokens::DESIGN_TOKENS;
use crate::widget::test_frames::frame;
use crate::widget::{PaneId, SeriesId, SeriesOptions};
use chrono::{TimeDelta, Utc};
use egui::Pos2;

fn bars(prices: &[f64]) -> Vec<Bar> {
    let start = "2024-01-01T00:00:00Z"
        .parse::<chrono::DateTime<Utc>>()
        .unwrap();
    prices
        .iter()
        .enumerate()
        .map(|(index, &price)| {
            Bar::new(
                start + TimeDelta::minutes(index as i64),
                price,
                price,
                price,
                price,
                1.0,
            )
        })
        .collect()
}

/// Tick hooks see each axis separately in displayed units, while value labels keep
/// using the individual hook. A move from 100 to 200 or 1000 to 2000 is +100%, index 200.
#[test]
fn tick_formatters_receive_complete_axis_lists() {
    use crate::scales::PriceFormatter;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Recording {
        lists: Arc<Mutex<Vec<Vec<f64>>>>,
        label_delta: i8,
    }
    impl PriceFormatter for Recording {
        fn format(&self, price: f64) -> String {
            format!("value {price:.2}")
        }
        fn format_tick(&self, price: f64) -> String {
            format!("tick {price:.2}")
        }
        fn format_ticks(&self, prices: &[f64]) -> Vec<String> {
            self.lists.lock().unwrap().push(prices.to_vec());
            let mut labels: Vec<_> = prices
                .iter()
                .map(|price| format!("batch {} {price:.2}", prices.len()))
                .collect();
            if self.label_delta < 0 {
                let _ = labels.pop();
            } else if self.label_delta > 0 {
                labels.push("extra".into());
            }
            labels
        }
        fn clone_box(&self) -> Box<dyn PriceFormatter> {
            Box::new(self.clone())
        }
    }

    for mode in [
        PriceScaleMode::Normal,
        PriceScaleMode::Logarithmic,
        PriceScaleMode::Percentage,
        PriceScaleMode::IndexedTo100,
    ] {
        for edges in [false, true] {
            let ctx = egui::Context::default();
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 200.0])));
            chart.config.show_volume = false;
            chart.config.price_axis.edge_ticks_visible = edges;
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
            let other = chart.add_series(SeriesOptions {
                scale: SeriesScale::Left,
                ..Default::default()
            });
            chart.set_data(other, bars(&[1000.0, 2000.0]));
            let raw = Arc::new(Recording::default());
            let series = Arc::new(Recording::default());
            let percentage = Arc::new(Recording::default());
            chart.set_price_formatter(Some(raw.clone()));
            chart.set_series_formatter(other, Some(series.clone()));
            chart.set_percentage_formatter(Some(percentage.clone()));
            for id in [SeriesId::MAIN, other] {
                chart.series_price_scale_mut(id).unwrap().options_mut().mode = mode;
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let relative = matches!(
                mode,
                PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
            );
            for (id, low, high) in [
                (SeriesId::MAIN, 100.0_f64, 200.0_f64),
                (other, 1000.0, 2000.0),
            ] {
                let (low, high) = match mode {
                    PriceScaleMode::Normal => (low, high),
                    PriceScaleMode::Logarithmic => (low.ln(), high.ln()),
                    PriceScaleMode::Percentage => (0.0, 100.0),
                    PriceScaleMode::IndexedTo100 => (100.0, 200.0),
                };
                chart
                    .series_price_scale_mut(id)
                    .unwrap()
                    .set_range(low, high);
            }
            for recorder in [&raw, &series, &percentage] {
                recorder.lists.lock().unwrap().clear();
            }
            let pos = chart.last_rendered_price_rect.center();
            let painted = crate::widget::test_frames::shapes(
                &ctx,
                &mut chart,
                0.1,
                vec![egui::Event::PointerMoved(pos)],
            );
            for (id, recorder, low, high) in [
                (
                    SeriesId::MAIN,
                    if relative { &percentage } else { &raw },
                    100.0,
                    200.0,
                ),
                (
                    other,
                    if relative { &percentage } else { &series },
                    1000.0,
                    2000.0,
                ),
            ] {
                let map = chart.series_price_map(id).unwrap();
                let expected: Vec<_> = map.marks().iter().map(|mark| mark.price).collect();
                let calls = recorder.lists.lock().unwrap();
                assert!(calls.contains(&expected), "{mode:?}: {id:?}: {calls:?}");
                let bounds = match mode {
                    PriceScaleMode::Percentage => (0.0, 100.0),
                    PriceScaleMode::IndexedTo100 => (100.0, 200.0),
                    _ => (low, high),
                };
                for values in calls.iter() {
                    assert!(
                        values.windows(2).all(|pair| pair[0] < pair[1]),
                        "{values:?}"
                    );
                    assert!(
                        values
                            .iter()
                            .all(|&value| value >= bounds.0 - 1e-8 && value <= bounds.1 + 1e-8),
                        "{mode:?}: {values:?}"
                    );
                    assert!(
                        values.len() > 2,
                        "edge ticks must share the axis list: {values:?}"
                    );
                }
                if edges {
                    assert!(
                        calls.iter().any(|values| values != &expected),
                        "edge ticks are included"
                    );
                } else {
                    assert!(calls.iter().all(|values| values == &expected));
                }
            }
            if relative {
                assert!(raw.lists.lock().unwrap().is_empty());
                assert!(series.lists.lock().unwrap().is_empty());
            } else {
                assert!(percentage.lists.lock().unwrap().is_empty());
            }
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            assert!(
                labels.iter().any(|label| label.starts_with("batch ")),
                "{labels:?}"
            );
            assert!(!labels.iter().any(|label| label.starts_with("tick ")));
            let value = if mode == PriceScaleMode::Percentage {
                100.0
            } else {
                200.0
            };
            assert_eq!(
                chart.format_series_price(SeriesId::MAIN, 200.0),
                Some(format!("value {value:.2}"))
            );
            assert!(
                labels.iter().any(|label| label.starts_with("value ")),
                "value tags stay individual"
            );
            let map = chart.series_price_map(SeriesId::MAIN).unwrap();
            let hover = chart
                .format_series_price(SeriesId::MAIN, map.y_to_price(pos.y))
                .unwrap();
            assert!(
                labels.contains(&hover.as_str()),
                "crosshair stays individual"
            );
            if relative {
                let study = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
                    "Relative study",
                    |_| {
                        vec![
                            crate::studies::IndicatorValue::Single(200.0),
                            crate::studies::IndicatorValue::Single(240.0),
                        ]
                    },
                )));
                assert!(chart.set_indicator_scale(
                    study,
                    crate::studies::StudyScale::Shared(SeriesScale::Separate(9))
                ));
                chart
                    .series_scales
                    .get_mut(&SeriesScale::Separate(9))
                    .unwrap()
                    .options_mut()
                    .mode = mode;
                percentage.lists.lock().unwrap().clear();
                frame(&ctx, &mut chart, 0.2, vec![]);
                let map = chart
                    .indicators()
                    .find(|(id, _)| *id == study)
                    .unwrap()
                    .1
                    .scale_mapping()
                    .unwrap();
                let expected: Vec<_> = map.marks().iter().map(|mark| mark.price).collect();
                assert!(
                    percentage.lists.lock().unwrap().contains(&expected),
                    "study axis ticks share one list"
                );
            }
        }
    }
    // A host returning too few or too many labels cannot drop ticks or mislabel them.
    let map = PriceMap::new(
        Default::default(),
        crate::scales::PriceRange::new(100.0, 200.0),
        0.0,
        400.0,
        false,
    );
    for label_delta in [-1, 1] {
        let formatter = Recording {
            label_delta,
            ..Default::default()
        };
        let marks = crate::chart::rendering::axes::price_marks(&map, Some(&formatter));
        assert_eq!(marks.len(), map.marks().len());
        for mark in marks {
            assert_eq!(mark.label, formatter.format_tick(mark.price));
        }
    }
}

/// Added candles keep independent body, wick and inside-border colours in either pane.
/// A zero width suppresses the border even when the bar supplies its colour.
#[test]
fn added_candle_borders_preserve_per_bar_colours_and_body_bounds() {
    use crate::model::BarColors;

    let mut legacy = serde_json::to_value(SeriesOptions::default()).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("candle_border_width");
    assert_eq!(
        serde_json::from_value::<SeriesOptions>(legacy)
            .unwrap()
            .candle_border_width,
        0.0
    );
    for own_pane in [false, true] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[10.0, 20.0])));
        let colours = [
            [Color32::GREEN, Color32::BLUE, Color32::MAGENTA],
            [Color32::RED, Color32::CYAN, Color32::GOLD],
        ];
        let data = bars(&[20.0, 10.0])
            .into_iter()
            .zip(colours)
            .map(|(mut bar, [body, wick, border])| {
                bar.open = 30.0 - bar.close;
                bar.high = 25.0;
                bar.low = 5.0;
                bar.colors = Some(Box::new(BarColors {
                    color: Some(body),
                    wick: Some(wick),
                    border: Some(border),
                    ..Default::default()
                }));
                bar
            })
            .collect();
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Candles,
            ..Default::default()
        });
        chart.set_data(id, data);
        if own_pane {
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(id, pane));
        }
        for (at, width) in [2.0, 0.0, -1.0, f32::NAN, f32::INFINITY]
            .into_iter()
            .enumerate()
        {
            let mut options = chart.series().next().unwrap().1.clone();
            options.candle_border_width = width;
            assert!(chart.set_series_options(id, options));
            let painted =
                crate::widget::test_frames::shapes(&ctx, &mut chart, at as f64 * 0.1, vec![]);
            let map = chart.series_price_map(id).unwrap();
            let body_width = chart.state.time_scale().bar_spacing() * chart.config.candle_width;
            for (index, [body, wick, border]) in colours.into_iter().enumerate() {
                let x = chart.index_to_x(index as f64).unwrap();
                // Both 10 -> 20 and 20 -> 10 occupy the same price interval.
                let expected = Rect::from_min_max(
                    egui::pos2(x - body_width / 2.0, map.price_to_y(20.0)),
                    egui::pos2(x + body_width / 2.0, map.price_to_y(10.0)),
                );
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Rect(rect) if rect.fill == body && rect.rect == expected
                    )),
                    "pane={own_pane}, width={width}: body {index}"
                );
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::LineSegment { points, stroke }
                            if stroke.color == wick && stroke.width == chart.config.wick_width
                                && *points == [
                                    egui::pos2(x, map.price_to_y(25.0)),
                                    egui::pos2(x, map.price_to_y(5.0)),
                                ]
                    )),
                    "pane={own_pane}, width={width}: wick {index}"
                );
                let borders: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Rect(rect) if rect.stroke.color == border => Some(rect),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    borders.len(),
                    usize::from(width.is_finite() && width > 0.0),
                    "pane={own_pane}, width={width}: border {index}"
                );
                for rect in borders {
                    assert_eq!(rect.rect, expected);
                    assert_eq!(rect.stroke.width, width);
                    assert_eq!(rect.stroke_kind, egui::StrokeKind::Inside);
                }
            }
        }
    }
}

/// Saved display formats label raw units on shared and independent axes without transforming
/// the data. A quarter-unit movement rounds 12.62 to 12.50, and 1500 / 1000 is 1.50K.
#[test]
fn series_display_formats_reach_ticks_tags_readouts_and_snapshots() {
    for (kind, value, step, expected) in [
        (SeriesFormat::Price, 12.62, 0.25, "12.50"),
        (SeriesFormat::Price, 12.625, 0.25, "12.75"),
        (SeriesFormat::Price, -12.625, 0.25, "-12.75"),
        (SeriesFormat::Volume, 1500.0, 1.0, "1.50K"),
        (SeriesFormat::Volume, 1500.0, 3.0, "1.50K"),
        (SeriesFormat::Percentage, 12.5, 0.25, "12.50%"),
    ] {
        for assignment in [
            None,
            Some(SeriesScale::Right),
            Some(SeriesScale::Left),
            Some(SeriesScale::Separate(1)),
        ] {
            let ctx = egui::Context::default();
            let data = bars(&[value - step, value]);
            let mut chart = Chart::new(BarData::from_bars(data.clone()));
            chart
                .series_price_scale_mut(SeriesId::MAIN)
                .unwrap()
                .set_price_precision(Some(4));
            let id = assignment.map_or(SeriesId::MAIN, |scale| {
                let id = chart.add_series(SeriesOptions {
                    scale,
                    ..Default::default()
                });
                chart.set_data(id, data.clone());
                id
            });
            if assignment == Some(SeriesScale::Separate(1)) {
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(id, pane));
            }
            chart.series_style_mut(id).unwrap().display_format = Some(SeriesDisplayFormat {
                kind,
                precision: 2,
                min_move: step,
            });
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
            assert_eq!(
                chart.format_series_price(id, value).as_deref(),
                Some(expected)
            );
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Text(text) if text.galley.text() == expected
                )),
                "{kind:?} {assignment:?}: last-value tag"
            );
            assert_eq!(chart.bars(id), data);
            let map = chart.series_price_map(id).unwrap();
            assert_eq!(map.transform().mode, PriceScaleMode::Normal);
            assert_eq!(
                map.price_precision(),
                if assignment == Some(SeriesScale::Right) {
                    Some(4)
                } else {
                    Some(2)
                }
            );
            let formatter = chart
                .series_label_formatter(id, PriceScaleMode::Normal)
                .unwrap();
            assert_eq!(formatter.format(value), expected);
            if id != SeriesId::MAIN {
                assert_eq!(
                    chart.series_tag_label(&chart.price_series[0], &map, value, true),
                    expected
                );
            }
            // On a shared axis the visible main series owns the ticks, while each series
            // keeps its own tag and readout format.
            if assignment != Some(SeriesScale::Right) {
                let marks =
                    crate::chart::rendering::axes::price_marks(&map, Some(formatter.as_ref()));
                assert!(!marks.is_empty());
                for adjacent in marks.windows(2) {
                    assert_ne!(adjacent[0].label, adjacent[1].label);
                }
                for mark in marks {
                    assert!((mark.price / step - (mark.price / step).round()).abs() < 1e-8);
                    assert_eq!(mark.label, formatter.format(mark.price));
                }
            }
            for mode in [
                PriceScaleMode::Logarithmic,
                PriceScaleMode::Percentage,
                PriceScaleMode::IndexedTo100,
            ] {
                if mode == PriceScaleMode::Logarithmic && value <= 0.0 {
                    continue;
                }
                chart.series_price_scale_mut(id).unwrap().options_mut().mode = mode;
                frame(&ctx, &mut chart, 0.1, vec![]);
                if mode == PriceScaleMode::Logarithmic {
                    assert_eq!(
                        chart.format_series_price(id, value).as_deref(),
                        Some(expected)
                    );
                } else {
                    // Explicit transformed-mode hooks receive scale units even with a raw declaration.
                    chart.set_percentage_formatter(Some(std::sync::Arc::new(
                        crate::scales::CustomPriceFormatter::new(|_| "transformed".into()),
                    )));
                    assert_eq!(
                        chart.format_series_price(id, value).as_deref(),
                        Some("transformed")
                    );
                    chart.set_percentage_formatter(None);
                    assert_ne!(
                        chart.format_series_price(id, value).as_deref(),
                        Some(expected)
                    );
                }
            }
            chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Normal;
            frame(&ctx, &mut chart, 0.2, vec![]);
            chart.set_price_formatter(Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|_| "chart".into()),
            )));
            assert_eq!(
                chart.format_series_price(id, value).as_deref(),
                Some("chart")
            );
            chart.set_series_formatter(
                id,
                Some(std::sync::Arc::new(
                    crate::scales::CustomPriceFormatter::new(|_| "series".into()),
                )),
            );
            assert_eq!(
                chart.format_series_price(id, value).as_deref(),
                Some("series")
            );
            chart.set_series_formatter(id, None);
            chart.set_price_formatter(None);
            if assignment == Some(SeriesScale::Right) {
                chart.set_main_series_visible(false);
                frame(&ctx, &mut chart, 0.3, vec![]);
                assert_eq!(
                    chart.series_price_map(id).unwrap().price_precision(),
                    Some(2)
                );
                assert!(chart.remove_series(SeriesId::MAIN));
                frame(&ctx, &mut chart, 0.4, vec![]);
                assert_eq!(chart.last_price_map.price_precision(), Some(2));
            }
            #[cfg(feature = "ui")]
            {
                let saved = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
                let mut restored = Chart::new(BarData::from_bars(data.clone()));
                restored
                    .restore(&serde_json::from_str(&saved).unwrap())
                    .unwrap();
                let restored_id = if id == SeriesId::MAIN {
                    id
                } else {
                    restored.restored_series_id(id.0).unwrap()
                };
                restored.set_data(restored_id, data);
                frame(&egui::Context::default(), &mut restored, 0.0, vec![]);
                assert_eq!(
                    restored.format_series_price(restored_id, value).as_deref(),
                    Some(expected)
                );
            }
        }
    }
}

/// The magnet reaches a visible comparison through its own scale, so a series whose prices
/// are far from the main ones still attracts the crosshair, and a hidden one does not.
#[test]
fn magnet_snaps_to_visible_comparison_values_on_their_own_scale() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
    chart.set_crosshair_mode(crate::config::CrosshairMode::Magnet);
    let id = chart.add_comparison("Other", Color32::BLUE);
    chart.set_series_scale(id, SeriesScale::Left);
    chart.set_data(id, bars(&[1300.0, 1100.0, 1000.0]));
    frame(&ctx, &mut chart, 0.0, vec![]);
    let rect = chart.last_rendered_price_rect;
    let main = chart.last_price_map.price_to_y(120.0);
    let other = chart
        .series_map(&chart.price_series[0], rect)
        .price_to_y(1000.0);
    assert!((main - other).abs() > 20.0, "{main} {other}");
    let at = |chart: &Chart, y: f32| {
        chart
            .snap_crosshair(egui::pos2(rect.center().x, y), &chart.last_price_map, 2)
            .y
    };
    assert_eq!(at(&chart, other + 1.0), other);
    assert_eq!(at(&chart, main + 1.0), main);
    chart.set_series_visible(id, false);
    assert_eq!(at(&chart, other + 1.0), main);
}

/// The magnet reaches a study drawn on its own independent scale through that scale, not
/// the main one, so its output near the bottom of the pane attracts the crosshair.
#[test]
fn magnet_snaps_to_study_values_on_their_own_scale() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
    chart.set_crosshair_mode(crate::config::CrosshairMode::Magnet);
    let id = chart.add_indicator(Box::new(
        crate::studies::CustomIndicator::new("Independent", |bars| {
            (0..bars.len())
                .map(|at| crate::studies::IndicatorValue::Single(30.0 - 10.0 * at as f64))
                .collect()
        })
        .with_scale(crate::studies::StudyScale::Independent),
    ));
    frame(&ctx, &mut chart, 0.0, vec![]);
    let rect = chart.last_rendered_price_rect;
    let main = chart.last_price_map.price_to_y(120.0);
    let at = |chart: &Chart, y: f32| {
        chart
            .snap_crosshair(egui::pos2(rect.center().x, y), &chart.last_price_map, 2)
            .y
    };
    let study = at(&chart, rect.bottom() - 1.0);
    assert!(study > rect.center().y, "{study} {main}");
    assert_eq!(at(&chart, main + 1.0), main);
    chart.set_indicator_visible(id, false);
    assert_eq!(at(&chart, rect.bottom() - 1.0), main);
}

/// Logarithms are monotone on positive prices. With 10% margins, ln(2)..ln(100)
/// occupies 80% of the pane; a lone zero-crossing bar instead starts at ln(5).
#[test]
fn log_fit_includes_positive_ohlc_of_zero_crossing_bars() {
    for secondary in [false, true] {
        for count in [1, 2] {
            let mut input = bars(&[10.0, 15.0]);
            input[0].open = 5.0;
            input[0].high = 100.0;
            input[0].low = -1.0;
            input[1].high = 20.0;
            input[1].low = 2.0;
            input.truncate(count);
            let mut chart = Chart::new(BarData::from_bars(input.clone()));
            let id = if secondary {
                let id = chart.add_series(SeriesOptions {
                    chart_type: ChartType::Candles,
                    scale: SeriesScale::Left,
                    ..Default::default()
                });
                chart.set_data(id, input);
                id
            } else {
                SeriesId::MAIN
            };
            chart.series_price_scale_mut(id).unwrap().options_mut().mode =
                PriceScaleMode::Logarithmic;
            frame(&egui::Context::default(), &mut chart, 0.0, vec![]);
            let map = chart.series_price_map(id).unwrap();
            let low: f64 = if count == 1 { 5.0 } else { 2.0 };
            let span = (100.0_f64.ln() - low.ln()) / 0.8;
            assert!(
                (map.range().min - (low.ln() - span * 0.1)).abs() < 1e-10,
                "secondary={secondary}, count={count}: {:?}",
                map.range()
            );
            assert!((map.range().max - (100.0_f64.ln() + span * 0.1)).abs() < 1e-10);
            for price in [low, 5.0, 10.0, 100.0] {
                let y = map.price_to_y(price);
                let (top, bottom) = map.extent();
                assert!((top..=bottom).contains(&y), "{price}: {y}");
            }
        }
    }
}

/// The first transformed open after the gap is (100 + 100) / 2 = 100,
/// although its raw prices are between 9 and 11. A right edge at 1.5 also fits bar 2.
#[test]
fn heikin_ashi_fit_uses_the_transformed_visible_candles() {
    for (secondary, right, high) in [
        (false, 8.0, 100.0),
        (true, 8.0, 100.0),
        (false, 1.5, 1001.0),
        (true, 1.5, 1001.0),
    ] {
        let mut input = bars(&[10.0; 9]);
        for bar in &mut input {
            bar.high = 11.0;
            bar.low = 9.0;
        }
        input[0].open = 100.0;
        input[0].close = 100.0;
        input[0].high = 101.0;
        input[0].low = 99.0;
        if right == 1.5 {
            input[2].open = 1000.0;
            input[2].close = 1000.0;
            input[2].high = high;
            input[2].low = 999.0;
        }
        let mut chart = Chart::new(BarData::from_bars(input.clone()));
        let id = if secondary {
            let id = chart.add_series(SeriesOptions {
                chart_type: ChartType::Heikin,
                scale: SeriesScale::Left,
                ..Default::default()
            });
            chart.set_data(id, input);
            id
        } else {
            chart.set_chart_type(ChartType::Heikin);
            SeriesId::MAIN
        };
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.set_visible_logical_range(crate::model::LogicalRange::new(1.0, right));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let map = chart.series_price_map(id).unwrap();
        assert!(
            (map.range().min - (9.0 - (high - 9.0) / 8.0)).abs() < 1e-8,
            "secondary={secondary}, right={right}: {:?}",
            map.range()
        );
        assert!((map.range().max - (high + (high - 9.0) / 8.0)).abs() < 1e-8);
        for price in [9.0, 10.0, 100.0, high] {
            let (top, bottom) = map.extent();
            assert!((top..=bottom).contains(&map.price_to_y(price)));
        }
        let x = chart.index_to_x(1.0).unwrap();
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Rect(body)
                    if (body.rect.center().x - x).abs() < 1e-3
                        && (body.rect.top() - map.price_to_y(100.0)).abs() < 1e-3
                        && (body.rect.bottom() - map.price_to_y(10.0)).abs() < 1e-3
            )),
            "secondary={secondary}: the first visible body must span 10..100"
        );
    }
}

/// Percent compares returns, not absolute prices: 100 -> 110 and 200 -> 220
/// coincide at +10%; 200 -> 240 extends the shared range to 20%, plus an eighth of that on
/// each side for the 10% of the pane above and below.
#[test]
fn comparisons_share_percent_ranges_with_individual_visible_references() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
    let id = chart.add_comparison("Other", Color32::BLUE);
    chart.set_data(id, bars(&[200.0, 220.0, 240.0]));
    frame(&ctx, &mut chart, 0.0, vec![]);
    let rect = chart.last_rendered_price_rect;
    let map = chart.series_map(&chart.price_series[0], rect);
    assert!((map.price_to_y(220.0) - chart.last_price_map.price_to_y(110.0)).abs() < 1e-4);
    let range = chart.state.price_scale().range();
    assert!(
        (range.min + 2.5).abs() < 1e-10 && (range.max - 22.5).abs() < 1e-10,
        "{range:?}"
    );
    chart.state.time_scale_mut().set_bar_spacing(100.0);
    chart.state.time_scale_mut().set_right_offset(0.0);
    for width in [200.0, 150.0] {
        chart.state.time_scale_mut().set_width(width);
        // Logical left edges 1 and 1.5 both retain the partially visible bar at 1.
        assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
    }
    chart.state.time_scale_mut().set_bar_spacing(6.0);
    // A missing first timestamp uses the series' first available visible close.
    chart.set_data(id, bars(&[200.0, 220.0, 240.0])[1..].to_vec());
    frame(&ctx, &mut chart, 0.1, vec![]);
    assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
    chart.set_data(id, bars(&[f64::NAN]));
    frame(&ctx, &mut chart, 0.2, vec![]);
    let map = chart.series_map(&chart.price_series[0], rect);
    assert!(!map.price_to_y(f64::NAN).is_finite());
    for mode in [PriceScaleMode::Percentage, PriceScaleMode::IndexedTo100] {
        chart.price_scale_mut().options_mut().mode = mode;
        chart.set_data(id, bars(&[0.0, 1.0, 1.0]));
        frame(&ctx, &mut chart, 0.25, vec![]);
        let map = chart.series_price_map(id).unwrap();
        assert_eq!(map.transform(), PriceTransform::default());
        for price in [0.0, 1.0] {
            let y = map.price_to_y(price);
            assert!(y.is_finite(), "{mode:?}, {price}: {y}");
            assert!((rect.top()..=rect.bottom()).contains(&y));
        }
    }
    for change in ["hide", "remove", "empty", "join_main"] {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let first = chart.add_comparison("First", Color32::BLUE);
        let second = chart.add_comparison("Second", Color32::GREEN);
        for (id, prices) in [(first, [200.0, 220.0]), (second, [400.0, 440.0])] {
            chart.set_series_scale(id, SeriesScale::Left);
            chart.set_data(id, bars(&prices));
        }
        chart
            .series_price_scale_mut(first)
            .unwrap()
            .options_mut()
            .mode = PriceScaleMode::Percentage;
        frame(&ctx, &mut chart, 0.2, vec![]);
        chart
            .series_price_scale_mut(first)
            .unwrap()
            .set_range(-5.0, 25.0);
        match change {
            "hide" => {
                chart.set_series_visible(first, false);
            }
            "remove" => {
                chart.remove_series(first);
            }
            "join_main" => {
                chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left);
            }
            _ => chart.set_data(first, Vec::new()),
        }
        frame(&ctx, &mut chart, 0.3, vec![]);
        let range = chart.series_price_scale(second).unwrap().range();
        assert_eq!((range.min, range.max), (-5.0, 25.0), "{change}");
        if change == "join_main" {
            chart.set_series_scale(SeriesId::MAIN, SeriesScale::Right);
            frame(&ctx, &mut chart, 0.4, vec![]);
            let range = chart.series_price_scale(second).unwrap().range();
            assert_eq!((range.min, range.max), (-5.0, 25.0), "main leaves scale");
        }
        chart
            .series_price_scale_mut(second)
            .unwrap()
            .options_mut()
            .mode = PriceScaleMode::Normal;
        frame(&ctx, &mut chart, 0.5, vec![]);
        let range = chart.series_price_scale(second).unwrap().range();
        let expected = if change == "join_main" {
            (190.0, 250.0)
        } else {
            (380.0, 500.0)
        };
        assert_eq!((range.min, range.max), expected, "mode change: {change}");
    }
}

/// Secondary history and corrections use the main merge rules, and visibility/removal
/// cannot lose data or resurrect an id. A scale move preserves the other memberships.
#[test]
fn bar_series_keep_independent_data_and_scale_membership() {
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
    let first = chart.add_series(SeriesOptions::default());
    let second = chart.add_series(SeriesOptions {
        scale: SeriesScale::Left,
        ..Default::default()
    });
    let data = bars(&[200.0, 220.0]);
    chart.set_data(
        first,
        vec![
            data[1].clone(),
            data[0].clone(),
            bars(&[f64::NAN]).remove(0),
        ],
    );
    chart.update(first, bars(&[210.0]).remove(0));
    chart.update(second, data[1].clone());
    assert_eq!(
        chart
            .bars(first)
            .iter()
            .map(|bar| bar.close)
            .collect::<Vec<_>>(),
        [210.0, 220.0]
    );
    assert_eq!(chart.feed_state(first).unwrap().rejected_bars, 1);
    assert_eq!(chart.bars(second), &data[1..]);
    assert_eq!(chart.bars(SeriesId::MAIN), bars(&[100.0, 110.0]));
    for scale in [
        SeriesScale::Left,
        SeriesScale::Overlay(4),
        SeriesScale::Separate(7),
        SeriesScale::Right,
    ] {
        assert!(chart.set_series_scale(first, scale));
        assert_eq!(chart.series_scale(first), Some(scale));
        assert_eq!(chart.series_scale(second), Some(SeriesScale::Left));
        assert!(chart.set_series_visible(first, false));
        assert_eq!(chart.bars(first).len(), 2);
        assert!(!chart.is_series_visible(first));
        assert!(chart.set_series_visible(first, true));
    }
    assert!(chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left));
    assert_eq!(chart.series_scale(first), Some(SeriesScale::Right));
    assert!(chart.remove_series(first));
    assert!(!chart.remove_series(first));
    chart.update(first, data[0].clone());
    assert!(chart.bars(first).is_empty());
    assert!(chart.remove_series(SeriesId::MAIN));
    assert!(!chart.remove_series(SeriesId::MAIN));
    assert!(!chart.set_series_visible(SeriesId::MAIN, true));
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    assert_eq!(chart.bars(second), &data[1..]);
}

/// Columns of 200 and 220 over a base of 150 fit 150 to 220 in the middle 80% of the pane,
/// 141.25 to 228.75; a provider's 0 to 1,000 fits -125 to 1,125, and the main series' 50 to
/// 60 fits 48.75 to 61.25.
/// The price line, the base line of a percentage scale at the first visible close and the
/// last value label follow the series style.
#[test]
fn series_style_draws_columns_and_lines_and_fits_provided_ranges() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
    let id = chart.add_series(SeriesOptions {
        scale: SeriesScale::Separate(2),
        chart_type: ChartType::Columns,
        color: Color32::MAGENTA,
        up_color: Color32::MAGENTA,
        columns_base: Some(150.0),
        ..Default::default()
    });
    chart.set_data(id, bars(&[200.0, 220.0]));
    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
    let range = chart.series_price_scale(id).unwrap().range();
    assert!((range.min - 141.25).abs() < 1e-10 && (range.max - 228.75).abs() < 1e-10);
    let map = chart.series_map(&chart.price_series[0], chart.last_rendered_price_rect);
    let columns: Vec<_> = painted
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(column)
                if column.fill == Color32::MAGENTA
                    && chart.last_rendered_price_rect.contains_rect(column.rect) =>
            {
                Some(column.rect.y_range())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        columns,
        [200.0, 220.0]
            .map(|close| { egui::Rangef::new(map.price_to_y(close), map.price_to_y(150.0)) })
    );

    let style = chart.series_style_mut(id).unwrap();
    style.autoscale = Some(crate::model::AutoscaleProvider::new(|_, _| {
        Some((0.0, 1000.0))
    }));
    style.price_line_visible = true;
    style.price_line_color = Some(Color32::GOLD);
    style.price_line_style = LineStyle::Solid;
    style.base_line_visible = true;
    style.last_value_visible = false;
    chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Percentage;
    chart.series_style_mut(SeriesId::MAIN).unwrap().autoscale =
        Some(crate::model::AutoscaleProvider::new(|bars, default| {
            assert_eq!(bars.len(), 2);
            assert_eq!(default, Some((100.0, 110.0)));
            Some((50.0, 60.0))
        }));
    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
    let map = chart.series_map(&chart.price_series[0], chart.last_rendered_price_rect);
    let range = chart.series_price_scale(id).unwrap().range();
    let fitted = (
        map.transform().to_price(range.min),
        map.transform().to_price(range.max),
    );
    assert!((fitted.0 + 125.0).abs() < 1e-9 && (fitted.1 - 1125.0).abs() < 1e-9);
    let range = chart.state.price_scale().range();
    assert!((range.min - 48.75).abs() < 1e-10 && (range.max - 61.25).abs() < 1e-10);
    for (price, color) in [
        (220.0, Color32::GOLD),
        (200.0, DESIGN_TOKENS.semantic.extended.gray),
    ] {
        assert!(painted.iter().any(|shape| matches!(shape,
            egui::Shape::LineSegment { points, stroke }
                if stroke.color == color && points[0].y == map.price_to_y(price)
        )));
    }
    assert_eq!(
        chart
            .secondary_axis_values(SeriesScale::Separate(2), chart.last_rendered_price_rect)
            .count(),
        0
    );
}

/// Each added series writes its last value with its own scale's decimals, led by its own
/// name once the axis shows names, and a host formatter still replaces the decimals.
#[test]
fn added_series_value_tags_use_their_scale_precision_and_name() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
    let mut add = |name: &str, key, decimals, last| {
        let id = chart.add_series(SeriesOptions {
            name: name.into(),
            scale: key,
            ..Default::default()
        });
        chart.set_data(id, bars(&[last, last]));
        chart
            .series_price_scale_mut(id)
            .unwrap()
            .set_price_precision(Some(decimals));
        id
    };
    let alpha = add("Alpha", SeriesScale::Separate(2), 3, 200.5);
    add("Beta", SeriesScale::Separate(3), 0, 300.5);
    chart.config.price_axis.show_symbol_name = true;
    frame(&ctx, &mut chart, 0.0, vec![]);
    let labels = |chart: &Chart, key, rect| {
        chart
            .secondary_axis_values(key, rect)
            .map(|(.., label)| label)
            .collect::<Vec<_>>()
    };
    let rect = chart.last_rendered_price_rect;
    assert_eq!(
        labels(&chart, SeriesScale::Separate(2), rect),
        ["Alpha 200.500"]
    );
    assert_eq!(labels(&chart, SeriesScale::Separate(3), rect), ["Beta 300"]);
    chart.config.price_axis.show_symbol_name = false;
    assert_eq!(labels(&chart, SeriesScale::Separate(2), rect), ["200.500"]);
    chart.set_series_formatter(
        alpha,
        Some(std::sync::Arc::new(
            crate::scales::CustomPriceFormatter::new(|price| format!("{price:.1} USD")),
        )),
    );
    assert_eq!(
        labels(&chart, SeriesScale::Separate(2), rect),
        ["200.5 USD"]
    );
}

/// Every scale is fitted once across all its visible members; hidden and differently
/// assigned values cannot stretch it. Labels reserve space only for visible axes.
#[test]
fn series_scales_fit_visible_members_and_reserve_their_axes() {
    let ctx = egui::Context::default();
    for key in [
        SeriesScale::Right,
        SeriesScale::Left,
        SeriesScale::Overlay(1),
        SeriesScale::Separate(2),
    ] {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let id = chart.add_series(SeriesOptions {
            scale: key,
            color: Color32::MAGENTA,
            ..Default::default()
        });
        chart.set_data(id, bars(&[200.0, 220.0]));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let range = chart.series_price_scale_mut(id).unwrap().range();
        let expected = if key == SeriesScale::Right {
            (85.0, 235.0)
        } else {
            (197.5, 222.5)
        };
        assert!(
            (range.min - expected.0).abs() < 1e-10 && (range.max - expected.1).abs() < 1e-10,
            "{key:?}: {range:?}"
        );
        assert_eq!(
            chart.series_axis_keys().is_empty(),
            matches!(key, SeriesScale::Right | SeriesScale::Overlay(_))
        );
        chart.set_series_visible(id, false);
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert!(chart.series_axis_keys().is_empty());
        let range = chart.state.price_scale().range();
        assert!((range.min - 98.75).abs() < 1e-10 && (range.max - 111.25).abs() < 1e-10);
        // Secondary bars can occupy whitespace beyond the main history.
        let future = bars(&[200.0, 220.0])
            .into_iter()
            .map(|mut bar| {
                bar.time += TimeDelta::minutes(10);
                bar
            })
            .collect();
        chart.set_data(id, future);
        chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Percentage;
        chart.set_series_visible(id, true);
        chart.state.time_scale_mut().set_bar_spacing(100.0);
        chart.state.time_scale_mut().set_right_offset(12.0);
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
        assert!(chart.state.visible_data().is_empty());
        if key == SeriesScale::Right {
            assert_eq!(
                chart.last_price_map.transform().mode,
                PriceScaleMode::Percentage
            );
        }
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::MAGENTA)
            )),
            "secondary line in main whitespace: {key:?}"
        );
    }
    for key in [
        SeriesScale::Right,
        SeriesScale::Left,
        SeriesScale::Separate(5),
        SeriesScale::Separate(8),
    ] {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        chart.config.show_volume = false;
        let first = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
            "Shared low",
            |_| vec![crate::studies::IndicatorValue::Single(200.0); 2],
        )));
        let second = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
            "Shared high",
            |_| vec![crate::studies::IndicatorValue::Single(300.0); 2],
        )));
        for id in [first, second] {
            assert!(chart.set_indicator_scale(id, crate::studies::StudyScale::Shared(key)));
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        let mapping = |chart: &Chart, id| {
            let study = chart
                .indicators()
                .find(|(study, _)| *study == id)
                .unwrap()
                .1;
            crate::chart::rendering::study_mapping(study, &chart.get_chart_mapping()).price
        };
        let one = mapping(&chart, first);
        let two = mapping(&chart, second);
        assert_eq!(one.range(), two.range(), "assigned studies share a range");
        assert!((one.price_to_y(250.0) - two.price_to_y(250.0)).abs() < 0.01);
        assert!(one.range().min <= 200.0 && one.range().max >= 300.0);
        assert_eq!(
            chart.series_axis_keys().len(),
            usize::from(key != SeriesScale::Right)
        );
        if key == SeriesScale::Right {
            assert!(chart.set_indicator_autoscale(second, false));
            frame(&ctx, &mut chart, 0.05, vec![]);
            assert!(mapping(&chart, first).range().max < 300.0);
            assert!(chart.set_indicator_autoscale(second, true));
        }
        // A manually fixed 100..300 scale places 200 halfway down the pane.
        if key == SeriesScale::Right {
            chart.state.price_scale_mut().set_range(100.0, 300.0);
        } else {
            chart
                .series_scales
                .get_mut(&key)
                .unwrap()
                .set_range(100.0, 300.0);
        }
        frame(&ctx, &mut chart, 0.1, vec![]);
        let map = mapping(&chart, first);
        let rect = chart.last_rendered_price_rect;
        assert!((map.price_to_y(200.0) - rect.center().y).abs() < 0.01);
        if key == SeriesScale::Left {
            chart.series_scales.get_mut(&key).unwrap().reset();
            chart
                .series_scales
                .get_mut(&key)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Percentage;
            chart.set_percentage_formatter(Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|value| format!("p {value:.2}")),
            )));
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.15, vec![]);
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Text(text) if text.galley.text().starts_with("p ")
                        && text.pos.x < chart.last_rendered_price_rect.left()
                )),
                "shared study ticks use percentage units and the host formatter"
            );
            chart
                .series_scales
                .get_mut(&key)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Normal;
        }
        // A group moved away from the main pane must not stretch the main price scale.
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(first, pane));
        assert!(chart.move_series_to_pane(second, pane));
        chart.state.price_scale_mut().reset();
        frame(&ctx, &mut chart, 0.2, vec![]);
        assert!(chart.state.price_scale().range().max < 200.0);
        for id in [first, second] {
            let idx = chart
                .study_ids
                .iter()
                .position(|study| *study == id)
                .unwrap();
            let rendered = chart
                .get_rendered_indicator_panes()
                .iter()
                .find(|rendered| rendered.indicator_idx == idx)
                .unwrap();
            assert_eq!(
                chart.series_price_map(id).unwrap().range(),
                rendered.mapping.price.range()
            );
        }
        let host = chart.add_series(SeriesOptions {
            scale: key,
            ..Default::default()
        });
        chart.set_data(host, bars(&[220.0, 280.0]));
        assert!(chart.move_series_to_pane(host, pane));
        chart.set_series_formatter(
            host,
            Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|value| format!("b{value:.0}")),
            )),
        );
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.25, vec![]);
        if !matches!(key, SeriesScale::Overlay(_)) {
            let plot = chart.get_rendered_price_rect();
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Text(text) if text.pos.y > plot.bottom()
                        && (text.pos.x < plot.left() || text.pos.x > plot.right())
                        && text.galley.text().starts_with('b') && text.galley.text() != "b280"
                )),
                "the bar series owns shared ticks as well as its last-value tag"
            );
        }
        assert_eq!(
            chart.series_price_map(first).unwrap().range(),
            chart.series_price_scale(host).unwrap().range()
        );
        assert!(
            chart.series_axis_keys().is_empty(),
            "secondary axes stay out of the price pane"
        );
        if key == SeriesScale::Left {
            assert!(chart.set_indicator_scale(
                first,
                crate::studies::StudyScale::Shared(SeriesScale::Right)
            ));
            chart
                .pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane)
                .unwrap()
                .scale
                .options_mut()
                .mode = PriceScaleMode::Logarithmic;
            frame(&ctx, &mut chart, 0.3, vec![]);
            let map = chart.series_price_map(first).unwrap();
            assert!(
                map.transform().to_price(map.range().max) < 300.0,
                "a secondary study cannot stretch the logarithmic primary pane scale"
            );
        }
    }
    // Hidden study axes do not consume a secondary scale's reserved gutter.
    for width in [45.0, 240.0, 800.0] {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        chart.set_series_scale(SeriesId::MAIN, SeriesScale::Overlay(0));
        chart.config.padding = 8.0;
        chart.add_series(SeriesOptions {
            scale: SeriesScale::Left,
            ..Default::default()
        });
        chart.add_indicator(Box::new(
            crate::studies::CustomIndicator::new("Independent", |_| Vec::new())
                .with_scale(crate::studies::StudyScale::Independent),
        ));
        let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, 400.0));
        let plot = chart.calculate_layout_rects(widget).price_rect;
        let axes = chart.secondary_axes(widget, plot);
        assert_eq!(axes.len(), 1);
        assert_eq!(axes[0].1.left(), widget.left() + 8.0, "{width}");
        assert_eq!(axes[0].1.right(), plot.left(), "{width}");
    }
}

/// A candle series moved to its own pane is fitted to its lows and highs on that pane's
/// scale and drawn there, leaving the main scale alone, and draws in the main pane again
/// once moved back.
#[test]
fn series_move_between_panes_with_their_chart_type_and_scale() {
    let ctx = egui::Context::default();
    let up = Color32::from_rgb(1, 200, 2);
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
    let id = chart.add_series(SeriesOptions {
        chart_type: ChartType::Candles,
        up_color: up,
        ..Default::default()
    });
    let candles = bars(&[0.0; 3])
        .into_iter()
        .map(|bar| Bar::new(bar.time, 200.0, 230.0, 190.0, 220.0, 1.0))
        .collect();
    chart.set_data(id, candles);
    assert!(!chart.move_series_to_pane(id, PaneId(99)));
    let pane = chart.add_pane();
    assert!(chart.move_series_to_pane(id, pane));
    assert_eq!(chart.series_pane(id), Some(pane));
    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
    let main = chart.last_rendered_price_rect;
    assert!(chart.state.price_scale().range().max < 200.0);
    let range = chart.series_price_scale(id).unwrap().range();
    assert!(
        range.min <= 190.0 && (230.0..300.0).contains(&range.max),
        "{range:?}"
    );
    let map = chart.series_price_map(id).unwrap();
    assert!(map.price_to_y(230.0) > main.bottom());
    assert_eq!(
        chart.format_series_price(id, 230.0).as_deref(),
        Some("230.00")
    );
    // Candle bodies in the plot, not the last-value tag on the axis.
    let bodies = |painted: &[egui::Shape]| -> Vec<Rect> {
        painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Rect(body) if body.fill == up && body.rect.right() < main.right() => {
                    Some(body.rect)
                }
                _ => None,
            })
            .collect()
    };
    let drawn = bodies(&painted);
    assert_eq!(drawn.len(), 3);
    assert!(drawn.iter().all(|body| body.top() > main.bottom()));

    #[derive(Clone)]
    struct Host;
    impl crate::scales::PriceFormatter for Host {
        fn format(&self, value: f64) -> String {
            format!("label {value:.2}")
        }
        fn format_tick(&self, value: f64) -> String {
            format!("tick {value:.2}")
        }
        fn clone_box(&self) -> Box<dyn crate::scales::PriceFormatter> {
            Box::new(self.clone())
        }
    }
    chart.set_price_formatter(Some(std::sync::Arc::new(Host)));
    chart.set_percentage_formatter(Some(std::sync::Arc::new(Host)));
    chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
    for (index, (mode, expected)) in [
        (PriceScaleMode::Normal, "label 230.00"),
        (PriceScaleMode::Logarithmic, "label 230.00"),
        (PriceScaleMode::Percentage, "label 4.55"),
        (PriceScaleMode::IndexedTo100, "label 104.55"),
    ]
    .into_iter()
    .enumerate()
    {
        let time = 0.2 + index as f64 * 0.2;
        chart.series_price_scale_mut(id).unwrap().options_mut().mode = mode;
        frame(&ctx, &mut chart, time, vec![]);
        let map = chart.series_price_map(id).unwrap();
        let pos = Pos2::new(
            chart.get_chart_mapping().rect.center().x,
            map.price_to_y(210.0),
        );
        let painted = crate::widget::test_frames::shapes(
            &ctx,
            &mut chart,
            time + 0.1,
            vec![egui::Event::PointerMoved(pos)],
        );
        assert_eq!(
            chart.format_series_price(id, 230.0).as_deref(),
            Some(expected)
        );
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(text) if text.pos.y > main.bottom() => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        assert!(
            labels.iter().any(|text| text.starts_with("tick ")),
            "{mode:?}: {labels:?}"
        );
        let hover_label = chart.format_series_price(id, 210.0).unwrap();
        assert!(
            labels.contains(&hover_label.as_str()),
            "{mode:?}: {labels:?}"
        );
    }
    chart.series_price_scale_mut(id).unwrap().options_mut().mode = PriceScaleMode::Normal;
    frame(&ctx, &mut chart, 0.94, vec![]);
    assert!(chart.set_series_formatter(
        id,
        Some(std::sync::Arc::new(
            crate::scales::CustomPriceFormatter::new(|price| format!("own {price:.1}")),
        ))
    ));
    assert_eq!(
        chart.format_series_price(id, 230.0).as_deref(),
        Some("own 230.0")
    );
    frame(&ctx, &mut chart, 0.95, vec![]);
    assert!(chart.set_series_formatter(id, None));
    assert_eq!(
        chart.format_series_price(id, 230.0).as_deref(),
        Some("label 230.00")
    );

    assert!(chart.move_series_to_pane(id, PaneId::MAIN));
    assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
    let main = chart.last_rendered_price_rect;
    let drawn = bodies(&painted);
    assert_eq!(drawn.len(), 3);
    assert!(drawn.iter().all(|body| main.contains_rect(*body)));
    for show_study_values in [false, true] {
        chart.config.price_axis.show_study_values = show_study_values;
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.1, vec![]);
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.right()
                    && text.galley.text().contains("label 220.00")
            )),
            "study axis settings must preserve comparison legend values"
        );
    }
}

/// Secondary panes keep assigned ranges, axes and hit positions local through moves and saves.
#[test]
fn secondary_pane_assignments_keep_ranges_axes_and_queries_local() {
    use crate::widget::ChartElementId;
    use egui::{Event, Modifiers, PointerButton, Shape};

    let history = |values: [f64; 2]| {
        bars(
            &values
                .into_iter()
                .flat_map(|value| [value; 15])
                .collect::<Vec<_>>(),
        )
    };
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
    chart.config.show_volume = false;
    chart.chart_options.hovered_series_on_top = false;
    let pane = chart.add_pane();
    let other = chart.add_pane();
    let mut ids = Vec::new();
    for (key, values, caption) in [
        (SeriesScale::Left, [10.0, 20.0], "small"),
        (SeriesScale::Right, [1000.0, 2000.0], "large"),
        (SeriesScale::Overlay(7), [300.0, 600.0], "hidden"),
        (SeriesScale::Left, [50.0, 60.0], "other"),
    ] {
        let id = chart.add_series(SeriesOptions {
            name: caption.into(),
            scale: key,
            ..Default::default()
        });
        chart.set_data(id, history(values));
        chart.set_series_formatter(
            id,
            Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(move |value| {
                    format!("{caption}:{value:.0}")
                }),
            )),
        );
        assert!(chart.move_series_to_pane(id, if ids.len() == 3 { other } else { pane }));
        ids.push(id);
    }
    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
    // Each data span gains 10% on either side: 10..20 becomes 9..21.
    for (&id, (low, high)) in
        ids.iter()
            .zip([(9.0, 21.0), (900.0, 2100.0), (270.0, 630.0), (49.0, 61.0)])
    {
        let range = chart.series_price_scale(id).unwrap().range();
        assert!(
            (range.min - low).abs() < 1e-9 && (range.max - high).abs() < 1e-9,
            "{id:?}: {range:?}"
        );
    }
    let plot = chart.get_rendered_price_rect();
    for (id, prefix, left) in [(ids[0], "small:", true), (ids[1], "large:", false)] {
        let map = chart.series_price_map(id).unwrap();
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
            if text.pos.y > plot.bottom() && text.pos.y < map.price_to_y(map.transform().to_price(map.range().min))
                && text.galley.text().starts_with(prefix)
                && if left { text.pos.x < plot.left() } else { text.pos.x > plot.right() }
        )), "{prefix} axis");
    }
    assert!(
        !painted.iter().any(|shape| matches!(shape, Shape::Text(text)
            if text.pos.x > plot.right() && text.galley.text().starts_with("hidden:")
        ))
    );

    for (id, precision, min_move) in [(ids[0], 3, 0.001), (ids[1], 1, 0.1), (ids[3], 2, 0.01)] {
        chart.series_style_mut(id).unwrap().display_format = Some(SeriesDisplayFormat {
            kind: SeriesFormat::Price,
            precision,
            min_move,
        });
    }
    assert_eq!(
        chart.series_price_map(ids[0]).unwrap().price_precision(),
        Some(3)
    );
    assert_eq!(
        chart.series_price_map(ids[1]).unwrap().price_precision(),
        Some(1)
    );

    chart
        .series_price_scale_mut(ids[0])
        .unwrap()
        .set_range(0.0, 40.0);
    frame(&ctx, &mut chart, 0.1, vec![]);
    let map = chart.series_price_map(ids[0]).unwrap();
    // 20 is halfway through 0..40, so both conversions must reach the pane's midpoint.
    let panel = chart
        .pane_layout
        .panes
        .iter()
        .find(|item| item.id == pane)
        .unwrap()
        .rect;
    assert!((map.price_to_y(20.0) - panel.center().y).abs() < 1e-4);
    assert!((map.y_to_price(panel.center().y) - 20.0).abs() < 1e-4);
    let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(20), map.price_to_y(20.0));
    for (step, pressed) in [None, Some(true), Some(false)].into_iter().enumerate() {
        let mut events = vec![Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        frame(&ctx, &mut chart, 0.2 + step as f64 * 0.1, events);
    }
    assert_eq!(
        chart.selected_element(),
        Some(ChartElementId::Series(ids[0]))
    );
    assert!(
        chart.drain_events().any(|event| matches!(event,
            crate::widget::ChartEvent::CrosshairMoved(Some(point))
                if point.series == ids[0] && (point.price - 20.0).abs() < 1e-4
        )),
        "pointer values use their owner's assigned map"
    );
    assert!(chart.move_series_to_pane(ids[0], other));
    frame(&ctx, &mut chart, 0.6, vec![]);
    assert!(!chart.series_price_scale(ids[0]).unwrap().is_manual());
    assert_eq!(
        chart.series_price_map(ids[0]).unwrap().price_precision(),
        Some(2)
    );
    assert_eq!(
        chart.series_price_scale(ids[0]).unwrap().range(),
        chart.series_price_scale(ids[3]).unwrap().range()
    );
    chart
        .series_price_scale_mut(ids[0])
        .unwrap()
        .set_range(-10.0, 90.0);
    assert!(chart.move_series_to_pane(ids[0], pane));
    assert_eq!(
        chart.series_price_map(ids[0]).unwrap().price_precision(),
        Some(3)
    );
    assert_eq!(
        chart.series_price_scale(ids[0]).unwrap().range(),
        crate::scales::PriceRange::new(0.0, 40.0)
    );

    #[cfg(feature = "ui")]
    {
        chart.add_indicator(Box::new(crate::studies::RSI::new(100)));
        frame(&ctx, &mut chart, 0.7, vec![Event::PointerGone]);
        let snapshot =
            serde_json::from_str(&serde_json::to_string(&chart.snapshot().unwrap()).unwrap())
                .unwrap();
        chart.restore(&snapshot).unwrap();
        for (id, values) in
            ids.iter_mut()
                .zip([[10.0, 20.0], [1000.0, 2000.0], [300.0, 600.0], [50.0, 60.0]])
        {
            *id = chart.restored_series_id(id.0).unwrap();
            chart.set_data(*id, history(values));
        }
        frame(&ctx, &mut chart, 1.0, vec![]);
        assert_eq!(
            chart.series_price_scale(ids[0]).unwrap().range(),
            crate::scales::PriceRange::new(0.0, 40.0)
        );
        assert_eq!(
            chart.series_price_scale(ids[3]).unwrap().range(),
            crate::scales::PriceRange::new(-10.0, 90.0)
        );
        assert_eq!(
            chart.series_price_scale(ids[1]).unwrap().range(),
            crate::scales::PriceRange::new(900.0, 2100.0)
        );
        assert_eq!(
            chart.series_price_scale(ids[2]).unwrap().range(),
            crate::scales::PriceRange::new(270.0, 630.0)
        );
    }
    chart.reset_chart_view();
    for id in ids {
        assert!(!chart.series_price_scale(id).unwrap().is_manual());
    }
}

/// A secondary pane's own scale follows the price axis settings: it moves to the left axis
/// when only that one is shown and draws no labels when both are hidden.
#[test]
fn secondary_pane_scale_follows_price_axis_visibility() {
    for (right, left, side) in [
        (true, false, Some(false)),
        (false, true, Some(true)),
        (false, false, None),
    ] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        chart.config.show_volume = false;
        chart.config.show_right_axis = right;
        chart.config.show_left_axis = left;
        let pane = chart.add_pane();
        let id = chart.add_series(SeriesOptions::default());
        chart.set_data(id, bars(&[10.0; 30]));
        chart.set_series_formatter(
            id,
            Some(std::sync::Arc::new(
                crate::scales::CustomPriceFormatter::new(|value| format!("pane:{value:.0}")),
            )),
        );
        assert!(chart.move_series_to_pane(id, pane));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
        let plot = chart.get_rendered_price_rect();
        let sides: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("pane:") => {
                    Some(text.pos.x < plot.left())
                }
                _ => None,
            })
            .collect();
        match side {
            Some(left) => assert!(
                !sides.is_empty() && sides.iter().all(|side| *side == left),
                "right {right}, left {left}: {sides:?}"
            ),
            None => assert!(sides.is_empty(), "hidden axes: {sides:?}"),
        }
    }
}

/// The main series can sit before, between or after added series; paint and selection use
/// that order on every frame, including after restoring a serialized chart.
#[test]
fn main_series_order_controls_paint_and_selection() {
    use crate::widget::ChartElementId;
    use egui::{Event, Modifiers, PointerButton, Shape};

    let ctx = egui::Context::default();
    let data = bars(&[100.0, 105.0, 110.0].repeat(33));
    let mut chart = Chart::new(BarData::from_bars(data.clone()));
    chart.set_chart_type(ChartType::Line);
    chart.config.show_volume = false;
    chart.chart_options.hovered_series_on_top = false;
    let colors = [Color32::RED, Color32::GREEN, Color32::BLUE];
    let mut params = chart.chart_type_params().clone();
    params.styles.line.color = colors[0];
    chart.set_chart_type_params(params);
    let mut ids = vec![SeriesId::MAIN];
    for color in &colors[1..] {
        let id = chart.add_series(SeriesOptions {
            color: *color,
            ..Default::default()
        });
        chart.set_data(id, data.clone());
        ids.push(id);
    }
    assert_eq!(chart.series_order(SeriesId::MAIN), Some(0));
    assert!(!chart.set_series_order(SeriesId(999), 0));

    for (step, (moved, order, expected)) in [
        (0, 0, [0, 1, 2]),
        (0, 1, [1, 0, 2]),
        (0, usize::MAX, [1, 2, 0]),
        (2, 0, [2, 1, 0]),
        (0, 0, [0, 2, 1]),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(chart.set_series_order(ids[moved], order));
        for (position, index) in expected.iter().enumerate() {
            assert_eq!(chart.series_order(ids[*index]), Some(position));
        }
        chart.clear_selection();
        let time = step as f64;
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, time, vec![]);
        let drawn: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path) if path.points.len() == data.len() => colors
                    .iter()
                    .position(|color| path.stroke.color == egui::epaint::ColorMode::Solid(*color)),
                _ => None,
            })
            .collect();
        assert_eq!(drawn, expected, "paint at step {step}");
        let mapping = chart.get_chart_mapping();
        let pos = Pos2::new(mapping.idx_to_x(49), mapping.price_to_y(105.0));
        assert!(
            mapping.rect.contains(pos),
            "click at {pos:?} in {:?}",
            mapping.rect
        );
        for (input, pressed) in [None, Some(true), Some(false), None]
            .into_iter()
            .enumerate()
        {
            let mut events = vec![Event::PointerMoved(pos)];
            if let Some(pressed) = pressed {
                events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            frame(&ctx, &mut chart, time + (input + 1) as f64 * 0.1, events);
        }
        assert_eq!(
            chart.selected_element(),
            Some(ChartElementId::Series(ids[expected[2]])),
            "selection at step {step}"
        );
        #[cfg(feature = "ui")]
        {
            let json = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
            let snapshot: crate::widget::ChartSnapshot = serde_json::from_str(&json).unwrap();
            if step == 0 {
                let mut host_owned = snapshot.clone();
                host_owned.series = None;
                let mut preserved = Chart::new(BarData::from_bars(data.clone()));
                let kept = preserved.add_series(SeriesOptions::default());
                preserved.set_data(kept, data.clone());
                preserved.restore(&host_owned).unwrap();
                assert_eq!(preserved.series().count(), 1);
                assert_eq!(preserved.bars(kept), data);
            }
            chart.restore(&snapshot).unwrap();
            for id in &mut ids[1..] {
                *id = chart.restored_series_id(id.0).unwrap();
                chart.set_data(*id, data.clone());
            }
            for (position, index) in expected.iter().enumerate() {
                assert_eq!(
                    chart.series_order(ids[*index]),
                    Some(position),
                    "restored order"
                );
            }
        }
    }

    let pane = chart.add_pane();
    assert!(chart.move_series_to_pane(ids[1], pane));
    assert_eq!(chart.series_order(ids[1]), Some(0));
    assert!(chart.set_series_order(ids[1], usize::MAX));
    assert!(chart.move_series_to_pane(ids[2], pane));
    assert!(chart.set_series_order(ids[2], 0));
    assert_eq!(chart.series_order(ids[1]), Some(1));
    assert_eq!(chart.series_order(ids[2]), Some(0));
    assert!(chart.move_series_to_pane(ids[2], PaneId::MAIN));
    assert_eq!(chart.series_order(ids[0]), Some(0));
    assert_eq!(chart.series_order(ids[2]), Some(1));
    assert!(chart.set_series_visible(ids[2], false));
    frame(&ctx, &mut chart, 6.0, vec![]);
    assert_eq!(chart.series_order(ids[2]), Some(1));
    assert!(chart.set_series_visible(ids[2], true));
    assert!(chart.set_series_visible(ids[0], false));
    assert_eq!(chart.series_order(ids[0]), Some(0));
    assert!(chart.remove_series(ids[0]));
    assert_eq!(chart.series_order(ids[0]), None);
    assert_eq!(chart.series_order(ids[2]), Some(0));
    assert!(!chart.set_series_order(ids[0], 0));
}

/// Bars can be popped from, and looked up in, any bar series; live changes are reported
/// once per series in a row; the draw order can be changed. Discrete lookups distinguish shared
/// logical indices from stored rows and preserve gaps through time-coordinate round trips.
#[test]
fn series_data_is_queried_popped_and_reported() {
    use crate::model::LogicalRange;
    use crate::widget::{BarsInRange, ChartEvent, MismatchDirection};
    let mut chart = Chart::new(BarData::from_bars(bars(&[1.0, 2.0, 3.0, 4.0, 5.0])));
    let id = chart.add_series(SeriesOptions::default());
    // Bars at the main bars 0, 2 and 4.
    let data = bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
    chart.set_data(id, vec![data[0].clone(), data[2].clone(), data[4].clone()]);
    let minute = |index| data[0].time + TimeDelta::minutes(index);
    for (time, direction, expected) in [
        (minute(0), MismatchDirection::Exact, Some(0)),
        (minute(2), MismatchDirection::Exact, Some(1)),
        (minute(4), MismatchDirection::Exact, Some(2)),
        (minute(2), MismatchDirection::NearestLeft, Some(1)),
        (minute(2), MismatchDirection::NearestRight, Some(1)),
        (minute(1), MismatchDirection::Exact, None),
        (minute(1), MismatchDirection::NearestLeft, Some(0)),
        (minute(1), MismatchDirection::NearestRight, Some(1)),
        (minute(-1), MismatchDirection::Exact, None),
        (minute(-1), MismatchDirection::NearestLeft, None),
        (minute(-1), MismatchDirection::NearestRight, Some(0)),
        (minute(5), MismatchDirection::Exact, None),
        (minute(5), MismatchDirection::NearestLeft, Some(2)),
        (minute(5), MismatchDirection::NearestRight, None),
    ] {
        assert_eq!(
            chart.bar_index_at(id, time, direction),
            expected,
            "{time} {direction:?}"
        );
        let index = (time - minute(0)).num_minutes() as f64;
        assert_eq!(
            chart.bar_at_index(id, index, direction),
            expected.map(|row| &data[row * 2]),
            "logical {index} {direction:?}"
        );
    }
    for (index, left, right) in [
        (-0.25, None, Some(0)),
        (1.25, Some(0), Some(2)),
        (3.75, Some(2), Some(4)),
        (4.25, Some(4), None),
        (-f64::MAX, None, Some(0)),
        (f64::MAX, Some(4), None),
        (f64::NAN, None, None),
        (f64::NEG_INFINITY, None, None),
        (f64::INFINITY, None, None),
    ] {
        for (direction, expected) in [
            (MismatchDirection::Exact, None),
            (MismatchDirection::NearestLeft, left),
            (MismatchDirection::NearestRight, right),
        ] {
            assert_eq!(
                chart.bar_at_index(id, index, direction),
                expected.map(|row| &data[row]),
                "logical {index} {direction:?}"
            );
        }
    }
    for (time, direction, expected) in [
        (minute(0), MismatchDirection::Exact, Some(0)),
        (minute(2), MismatchDirection::Exact, Some(2)),
        (minute(4), MismatchDirection::Exact, Some(4)),
        (minute(2), MismatchDirection::NearestLeft, Some(2)),
        (minute(2), MismatchDirection::NearestRight, Some(2)),
        (
            minute(1) + TimeDelta::seconds(30),
            MismatchDirection::Exact,
            None,
        ),
        (
            minute(1) + TimeDelta::seconds(30),
            MismatchDirection::NearestLeft,
            Some(1),
        ),
        (
            minute(1) + TimeDelta::seconds(30),
            MismatchDirection::NearestRight,
            Some(2),
        ),
        (minute(-1), MismatchDirection::Exact, None),
        (minute(-1), MismatchDirection::NearestLeft, None),
        (minute(-1), MismatchDirection::NearestRight, Some(0)),
        (minute(5), MismatchDirection::Exact, None),
        (minute(5), MismatchDirection::NearestLeft, Some(4)),
        (minute(5), MismatchDirection::NearestRight, None),
    ] {
        assert_eq!(
            chart.time_index_at(time, direction),
            expected,
            "{time} {direction:?}"
        );
    }
    for index in 0..5 {
        assert_eq!(chart.time_at_index(index), Some(minute(index as i64)));
        assert_eq!(chart.time_to_x(minute(index as i64)), None, "before layout");
    }
    assert_eq!(chart.time_at_index(5), None);
    assert_eq!(chart.time_at_index(usize::MAX), None);
    assert_eq!(chart.x_to_time(0.0), None, "before layout");

    // A five-slot range places each centre one spacing apart, including sparse gaps.
    let ctx = egui::Context::default();
    frame(&ctx, &mut chart, 0.0, vec![]);
    assert!(chart.set_visible_logical_range(LogicalRange::new(0.0, 4.0)));
    frame(&ctx, &mut chart, 0.1, vec![]);
    let plot = chart.get_rendered_price_rect();
    let spacing = plot.width() / 5.0;
    for index in 0..5 {
        let time = minute(index);
        let x = chart.time_to_x(time).unwrap();
        let expected = plot.left() + (index as f32 + 0.5) * spacing - 1.0;
        assert!(
            (x - expected).abs() < 0.001,
            "centre {index}: {x} != {expected}"
        );
        assert_eq!(chart.x_to_time(x), Some(time));
        assert_eq!(chart.x_to_time(x + spacing * 0.25), Some(time));
        assert_eq!(
            chart.bar_at_index(id, index as f64, MismatchDirection::Exact),
            (index % 2 == 0).then(|| &data[index as usize])
        );
    }
    for time in [minute(-1), minute(5), minute(1) + TimeDelta::seconds(30)] {
        assert_eq!(chart.time_to_x(time), None);
        assert!(
            chart.time_to_index(time).is_some(),
            "continuous mapping stays available"
        );
    }
    for index in [-1.0, 5.0] {
        assert_eq!(chart.x_to_time(chart.index_to_x(index).unwrap()), None);
    }
    for x in [f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
        assert_eq!(chart.x_to_time(x), None);
    }
    // Binary-exact spacing makes half-slot boundary expectations independent of pixel rounding.
    chart.state.time_scale_mut().set_bar_spacing(128.0);
    chart.state.time_scale_mut().set_right_offset(0.0);
    for (index, expected) in [
        (-0.75, None),
        (-0.5, Some(minute(0))),
        (0.5, Some(minute(1))),
        (3.5, Some(minute(4))),
        (4.5, None),
    ] {
        assert_eq!(
            chart.x_to_time(chart.index_to_x(index).unwrap()),
            expected,
            "nearest {index}"
        );
    }
    for (series, (left, right), bars, bars_before, bars_after) in [
        // The bar at 2 is 2 - 0 logical slots from the first and 4 - 2 from the last.
        (id, (1.0, 3.0), 1..2, 2.0, 2.0),
        // Two bars of whitespace before the first bar.
        (id, (-2.0, 2.4), 0..2, -2.0, 2.0),
        // Five bars of whitespace after the last bar, at 4.
        (id, (2.6, 9.0), 2..3, 4.0, -5.0),
        // Slice selection still rounds the edges; distance from an outer edge does not.
        (id, (0.25, 3.75), 0..3, 0.25, 0.25),
        (id, (-2.25, 2.25), 0..2, -2.25, 2.0),
        (id, (2.75, 9.25), 2..3, 4.0, -5.25),
        (id, (-0.25, 4.25), 0..3, -0.25, -0.25),
        // Empty intersections measure from the requested edges, including inside a gap.
        (id, (0.75, 1.25), 1..1, 0.75, 2.75),
        (id, (-3.25, -1.25), 0..0, -3.25, 5.25),
        (id, (5.25, 7.25), 3..3, 5.25, -3.25),
        (SeriesId::MAIN, (1.0, 3.0), 1..4, 1.0, 1.0),
        (SeriesId::MAIN, (0.75, 3.25), 1..4, 1.0, 1.0),
        (SeriesId::MAIN, (-2.25, 2.25), 0..3, -2.25, 2.0),
        (SeriesId::MAIN, (2.75, 9.25), 3..5, 3.0, -5.25),
        (SeriesId::MAIN, (-3.25, -1.25), 0..0, -3.25, 5.25),
        (SeriesId::MAIN, (5.25, 7.25), 5..5, 5.25, -3.25),
    ] {
        assert_eq!(
            chart.bars_in_logical_range(series, LogicalRange::new(left, right)),
            Some(BarsInRange {
                bars,
                bars_before,
                bars_after
            }),
            "{series:?}: {left}..{right}"
        );
    }

    let _ = chart.drain_events().count();
    chart.update(id, data[3].clone());
    chart.update(id, bars(&[f64::NAN]).remove(0));
    chart.update(id, Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0));
    assert_eq!(
        chart.pop_bars(id, 2),
        [
            Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0),
            data[4].clone()
        ]
    );
    assert_eq!(
        chart.pop_bars(SeriesId::MAIN, 1),
        bars(&[1.0, 2.0, 3.0, 4.0, 5.0])[4..]
    );
    assert!(chart.pop_bars(SeriesId(99), 1).is_empty());
    assert_eq!(chart.bars(id), [data[0].clone(), data[2].clone()]);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 4);
    assert_eq!(
        chart.drain_events().collect::<Vec<_>>(),
        [
            ChartEvent::DataUpdated {
                series: id,
                bars: 2
            },
            ChartEvent::DataUpdated {
                series: SeriesId::MAIN,
                bars: 4
            },
        ]
    );

    let other = chart.add_series(SeriesOptions::default());
    for series in [other, SeriesId(99), SeriesId::VOLUME] {
        for direction in [
            MismatchDirection::Exact,
            MismatchDirection::NearestLeft,
            MismatchDirection::NearestRight,
        ] {
            assert_eq!(chart.bar_at_index(series, 2.0, direction), None);
        }
    }
    assert!(chart.set_series_order(other, 0));
    assert_eq!(
        chart.series().map(|(id, _)| id).collect::<Vec<_>>(),
        [other, id]
    );
    assert_eq!(chart.series_order(id), Some(2));

    let range = LogicalRange::new(0.0, 4.0);
    assert_eq!(chart.bars_in_logical_range(other, range), None);
    assert_eq!(chart.bars_in_logical_range(SeriesId(99), range), None);
    chart.set_data(other, vec![Bar::whitespace(minute(1))]);
    assert!(
        chart
            .bar_at_index(other, 1.0, MismatchDirection::Exact)
            .unwrap()
            .is_whitespace()
    );
    chart.set_data(SeriesId::MAIN, Vec::new());
    assert_eq!(chart.bars_in_logical_range(SeriesId::MAIN, range), None);
    assert_eq!(chart.bars_in_logical_range(id, range), None);
    for direction in [
        MismatchDirection::Exact,
        MismatchDirection::NearestLeft,
        MismatchDirection::NearestRight,
    ] {
        assert_eq!(chart.time_index_at(minute(0), direction), None);
        assert_eq!(chart.bar_at_index(id, 0.0, direction), None);
    }
    assert_eq!(chart.time_at_index(0), None);
    assert_eq!(chart.time_to_x(minute(0)), None);
    assert_eq!(chart.x_to_time(plot.center().x), None);

    chart.set_data(SeriesId::MAIN, vec![Bar::whitespace(minute(2))]);
    assert_eq!(chart.time_at_index(0), Some(minute(2)));
    assert_eq!(chart.time_at_index(1), None);
    let x = chart.time_to_x(minute(2)).unwrap();
    assert_eq!(chart.x_to_time(x), Some(minute(2)));
    assert!(
        chart
            .bar_at_index(SeriesId::MAIN, 0.0, MismatchDirection::Exact)
            .unwrap()
            .is_whitespace()
    );
}

/// Moving the main plot retains its bars and x positions while painting, picking and saving
/// its annotations in the destination pane. Returning it is one undoable placement edit.
#[test]
fn main_series_moves_with_rendering_pointer_and_saved_placement() {
    use crate::model::PriceLine;
    use crate::widget::{ChartElementId, ChartEvent, PointerTarget};
    use egui::{Event, Modifiers, PointerButton, Shape};

    for (above, inverted) in [(false, false), (true, true)] {
        let ctx = egui::Context::default();
        let data = bars(&[100.0, 110.0, 120.0].repeat(33));
        let mut chart = Chart::new(BarData::from_bars(data.clone()));
        chart.set_chart_type(ChartType::Line);
        chart.config.show_volume = above;
        chart.config.right_axis_width = 150.0;
        chart.config.left_axis_width = 150.0;
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
        let mut params = chart.chart_type_params().clone();
        params.styles.line.color = Color32::RED;
        chart.set_chart_type_params(params);
        let other = chart.add_series(SeriesOptions {
            color: Color32::BLUE,
            scale: if above {
                SeriesScale::Separate(9)
            } else {
                SeriesScale::Right
            },
            ..Default::default()
        });
        chart.set_data(other, bars(&[1000.0; 99]));
        let pane = chart.add_pane();
        chart.set_preserve_empty_pane(pane, true);
        let companion = chart.add_series(SeriesOptions {
            scale: SeriesScale::Separate(7),
            ..Default::default()
        });
        chart.set_data(companion, bars(&[105.0; 99]));
        assert!(chart.move_series_to_pane(companion, pane));
        chart
            .series_price_scale_mut(companion)
            .unwrap()
            .set_range(102.0, 108.0);
        chart
            .pane_price_scale_mut(pane, SeriesScale::Right)
            .set_range(200.0, 300.0);
        let mut members = vec![companion];
        #[cfg(feature = "studies")]
        {
            let average = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
            assert!(chart.move_series_to_pane(average, pane));
            members.push(average);
        }
        chart
            .series_price_scale_mut(SeriesId::MAIN)
            .unwrap()
            .set_range(90.0, 130.0);
        chart
            .series_price_scale_mut(SeriesId::MAIN)
            .unwrap()
            .options_mut()
            .invert_scale = inverted;
        if above {
            chart.move_pane(pane, 0);
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        assert_eq!(chart.last_rendered_volume_rect.is_positive(), above);
        let x = chart.get_chart_mapping().idx_to_x(70);
        let original_members: Vec<_> = chart
            .pane_series(PaneId::MAIN)
            .into_iter()
            .filter(|id| *id != SeriesId::MAIN)
            .collect();
        chart.set_markers(
            SeriesId::MAIN,
            vec![
                crate::model::Marker::new(data[60].time)
                    .at_price(112.0, crate::model::MarkerPos::AtPriceMiddle)
                    .with_shape(crate::model::MarkerShape::Circle)
                    .with_color(Color32::MAGENTA),
            ],
        );
        chart.set_price_lines(
            SeriesId::MAIN,
            vec![PriceLine::new(115.0, Color32::GOLD).with_style(LineStyle::Solid)],
        );
        assert!(chart.user_action(|chart| chart.move_series_to_pane(SeriesId::MAIN, pane)));
        assert_eq!(chart.series_pane(SeriesId::MAIN), Some(pane));
        members.push(SeriesId::MAIN);
        assert_eq!(chart.pane_series(pane), members);
        assert_eq!(chart.pane_series(PaneId::MAIN), original_members);
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let rect = chart
            .pane_layout
            .panes
            .iter()
            .find(|item| item.id == pane)
            .unwrap()
            .rect;
        let mapping = chart.get_chart_mapping();
        #[cfg(feature = "studies")]
        assert!(
            !chart.last_rendered_volume_rect.is_positive(),
            "the original pane's volume does not capture the main plot's pointer values"
        );
        assert!(rect.contains_rect(mapping.rect));
        assert!((mapping.idx_to_x(70) - x).abs() < 0.01);
        assert_eq!(chart.bars(SeriesId::MAIN), data);
        let map = chart.series_price_map(SeriesId::MAIN).unwrap();
        assert_eq!(map, mapping.price);
        let original_map = chart.series_price_map(other).unwrap();
        assert!(original_map.range().contains(1000.0));
        assert_ne!(original_map, map);
        // Halfway through [90, 130], 110 maps to the vertical midpoint, inverted or not.
        assert!((map.price_to_y(110.0) - mapping.rect.center().y).abs() < 1e-4);
        let companion_map = chart.series_price_map(companion).unwrap();
        assert_eq!(
            (companion_map.range().min, companion_map.range().max),
            (102.0, 108.0)
        );
        assert_ne!(companion_map, map);
        assert!((companion_map.price_to_y(105.0) - mapping.rect.center().y).abs() < 1e-4);
        #[cfg(feature = "studies")]
        {
            let rendered = chart
                .get_rendered_indicator_panes()
                .iter()
                .find(|study| study.chart_rect == mapping.rect)
                .unwrap();
            assert_eq!(
                chart.series_price_map(chart.study_ids[rendered.indicator_idx]),
                Some(rendered.mapping.price)
            );
        }
        assert!(
            painted.iter().any(|shape| matches!(shape, Shape::Path(path)
            if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::RED)
                && path.points.len() > 3 && path.points.iter().all(|point| rect.contains(*point))))
        );
        assert!(painted.iter().any(|shape| matches!(shape, Shape::LineSegment { points, stroke }
            if stroke.color == Color32::GOLD && points.iter().all(|point| (point.y - map.price_to_y(115.0)).abs() < 0.01))));
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Circle(circle)
            if circle.fill == Color32::MAGENTA && (circle.center.y - map.price_to_y(112.0)).abs() < 0.01 && rect.contains(circle.center))));
        let pos = Pos2::new(mapping.idx_to_x(70), map.price_to_y(110.0));
        frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut chart,
                0.3,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        assert_eq!(
            chart.selected_element(),
            Some(ChartElementId::Series(SeriesId::MAIN))
        );
        assert!(chart.drain_events().any(|event| matches!(event, ChartEvent::Click { position, .. }
            if position.series == SeriesId::MAIN && position.pane == usize::from(!above)
                && (position.price - 110.0).abs() < 1e-4
                && position.target == Some(PointerTarget::Element(ChartElementId::Series(SeriesId::MAIN))))));
        assert!(chart.undo());
        assert_eq!(chart.series_pane(SeriesId::MAIN), Some(PaneId::MAIN));
        assert!(chart.redo());
        assert_eq!(chart.series_pane(SeriesId::MAIN), Some(pane));
        let range = chart.series_price_scale(SeriesId::MAIN).unwrap().range();
        assert_eq!((range.min, range.max), (90.0, 130.0));
        #[cfg(feature = "ui")]
        {
            let saved =
                serde_json::from_str(&serde_json::to_string(&chart.snapshot().unwrap()).unwrap())
                    .unwrap();
            chart.restore(&saved).unwrap();
            let restored = chart.series_pane(SeriesId::MAIN).unwrap();
            assert_ne!(restored, PaneId::MAIN);
            frame(&ctx, &mut chart, 0.4, vec![]);
            assert!(chart.pane_series(restored).contains(&SeriesId::MAIN));
            assert_eq!(chart.bars(SeriesId::MAIN), data);
            let restored_companion = chart
                .price_series
                .iter()
                .find(|item| item.options.scale == SeriesScale::Separate(7))
                .unwrap()
                .id;
            let range = chart
                .series_price_scale(restored_companion)
                .unwrap()
                .range();
            assert_eq!((range.min, range.max), (102.0, 108.0));
            let range = chart.series_price_scale(SeriesId::MAIN).unwrap().range();
            assert_eq!((range.min, range.max), (90.0, 130.0));
            assert!(
                chart.user_action(|chart| chart.move_series_to_pane(SeriesId::MAIN, PaneId::MAIN))
            );
            assert!(chart.undo());
            assert_eq!(chart.series_pane(SeriesId::MAIN), Some(restored));
        }
        let destination = chart.series_pane(SeriesId::MAIN).unwrap();
        assert!(chart.move_series_to_pane(SeriesId::MAIN, PaneId::MAIN));
        assert_eq!(chart.series_pane(SeriesId::MAIN), Some(PaneId::MAIN));
        let range = chart
            .pane_price_scale(destination, SeriesScale::Right)
            .range();
        assert_eq!((range.min, range.max), (200.0, 300.0));
    }
}
