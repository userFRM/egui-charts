use super::*;
use crate::model::{BarData, LineStyle, SeriesDisplayFormat, SeriesFormat};
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

/// Fitting includes all loaded series and whitespace, independent of visibility and pane.
/// Minutes 0..30 span 31 slots: spacing is width / (31 + margin), or (width - px) / 31.
#[test]
fn fit_content_includes_added_series_across_panes() {
    for case in [
        "main only",
        "both sides",
        "pane",
        "pixels",
        "hidden",
        "added only",
        "added only pane",
        "added only numeric",
        "added only Heikin",
        "added only multiple",
        "whitespace",
        "minimum",
        "maximum",
        "fixed edges",
        "numeric",
        "multiple series",
        "fractional",
    ] {
        let ctx = egui::Context::default();
        let mut data = bars(&[10.0; 41]);
        if case == "fractional" {
            data[0].time += TimeDelta::seconds(30);
            data[30].time += TimeDelta::seconds(15);
        }
        let mut chart = Chart::new(BarData::from_bars(if case.starts_with("added only") {
            vec![]
        } else if case == "numeric" {
            vec![data[10].clone(), data[11].clone(), data[20].clone()]
        } else {
            data[10..=20].to_vec()
        }));
        chart.config.show_volume = false;
        if case == "added only Heikin" {
            chart.set_chart_type(ChartType::Heikin);
        }
        let mut options = chart.chart_options().clone();
        options.time_scale.right_offset = 3.0;
        options.time_scale.right_offset_pixels = (case == "pixels").then_some(60.0);
        options.time_scale.min_bar_spacing = if case == "minimum" { 100.0 } else { 0.5 };
        options.time_scale.max_bar_spacing = if case == "maximum" { 4.0 } else { 0.0 };
        options.time_scale.fix_left_edge = case == "fixed edges";
        options.time_scale.fix_right_edge = case == "fixed edges";
        chart = chart.with_chart_options(options);
        if case.contains("numeric") {
            chart.set_horizontal_scale(Some(std::sync::Arc::new(
                crate::scales::PriceHorizontalScale::default(),
            )));
        }
        let color = Color32::from_rgb(133, 77, 211);
        let mut id = chart.add_series(SeriesOptions {
            color,
            ..Default::default()
        });
        if case != "main only" {
            chart.set_data(
                id,
                vec![data[0].clone(), data[15].clone(), data[30].clone()],
            );
        }
        if case == "added only multiple" {
            chart.set_data(id, vec![data[0].clone(), data[5].clone()]);
            id = chart.add_series(SeriesOptions {
                color,
                ..Default::default()
            });
            chart.set_data(
                id,
                vec![data[5].clone(), data[25].clone(), data[30].clone()],
            );
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(id, pane));
        }
        if case == "multiple series" {
            chart.set_data(id, vec![data[5].clone(), data[25].clone()]);
            id = chart.add_series(SeriesOptions {
                color,
                ..Default::default()
            });
            chart.set_data(id, vec![data[0].clone(), data[30].clone()]);
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(id, pane));
        }
        if case == "hidden" {
            chart.set_series_visible(id, false);
            chart.set_main_series_visible(false);
        }
        if case.contains("pane") {
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(id, pane));
        }
        if case == "whitespace" {
            chart.set_data(
                id,
                vec![
                    Bar::whitespace(data[0].time),
                    data[15].clone(),
                    Bar::whitespace(data[30].time),
                ],
            );
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.05, vec![]);
        chart.fit_content();
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let slots = if case == "main only" {
            11.0
        } else if case == "added only multiple" {
            4.0
        } else if case.starts_with("added only") {
            3.0
        } else if case == "numeric" {
            7.0
        } else if case == "fractional" {
            30.75
        } else {
            31.0
        };
        let margin = if case == "fixed edges" { 0.0 } else { 3.0 };
        let pixels = if case == "pixels" { 60.0 } else { 0.0 };
        let expected =
            ((plot.width() - pixels) / (slots + if pixels > 0.0 { 0.0 } else { margin })).clamp(
                if case == "minimum" { 100.0 } else { 0.5 },
                if case == "maximum" { 4.0 } else { f32::MAX },
            );
        assert!(
            (chart.get_time_scale_state().0 - expected).abs() < 0.001,
            "{case}: spacing {:?} != {expected}",
            chart.get_time_scale_state()
        );
        let endpoints = if case == "main only" {
            [10, 20]
        } else {
            [0, 30]
        };
        for minute in endpoints {
            let index = chart.time_to_index(data[minute].time).unwrap();
            let x = chart.index_to_x(index).unwrap();
            if case != "minimum" {
                assert!(
                    plot.x_range().contains(x),
                    "{case}: endpoint {minute} at {x}, {plot:?}"
                );
            }
            if case != "main only" && case != "hidden" && case != "whitespace" && case != "minimum"
            {
                assert!(painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(color)
                        && path.points.iter().any(|point| (point.x - x).abs() < 0.01)
                )), "{case}: added endpoint {minute} is painted");
            }
        }
        let last = chart.time_to_index(data[endpoints[1]].time).unwrap();
        let last_x = chart.index_to_x(last).unwrap();
        let gap = if pixels > 0.0 {
            pixels
        } else {
            margin * expected
        };
        assert!(
            (plot.right() - last_x - (gap + expected / 2.0 + 1.0)).abs() < 0.01,
            "{case}: right margin"
        );
        if case.starts_with("added only") && case != "added only multiple" {
            assert!(chart.bars(SeriesId::MAIN).is_empty());
            // Appends and replacements update the fallback mapping before the next fit.
            chart.update(id, data[40].clone());
            chart.fit_content();
            frame(&ctx, &mut chart, 0.2, vec![]);
            let x = chart
                .index_to_x(chart.time_to_index(data[40].time).unwrap())
                .unwrap();
            assert!(chart.get_rendered_price_rect().x_range().contains(x));
            if case == "added only numeric" {
                chart.set_data(
                    id,
                    vec![
                        data[0].clone(),
                        data[5].clone(),
                        data[30].clone(),
                        data[40].clone(),
                    ],
                );
                chart.fit_content();
                frame(&ctx, &mut chart, 0.25, vec![]);
                let x = |minute: usize| {
                    chart
                        .index_to_x(chart.time_to_index(data[minute].time).unwrap())
                        .unwrap()
                };
                // Replacing minute 15 by minute 5 preserves the endpoints and row count.
                assert!(((x(30) - x(5)) / (x(5) - x(0)) - 5.0).abs() < 0.001);
            }
            chart.set_data(SeriesId::MAIN, data[10..=20].to_vec());
            chart.fit_content();
            frame(&ctx, &mut chart, 0.3, vec![]);
            assert_eq!(chart.time_at_index(0), Some(data[10].time));
            chart.remove_series(id);
            chart.fit_content();
            assert!(
                (chart.get_time_scale_state().0 - chart.get_rendered_price_rect().width() / 14.0)
                    .abs()
                    < 0.001
            );
        }
    }
}

/// A bar appended to the series of a chart without main bars keeps a view scrolled 30 bars
/// back on the same times, as a bar appended to the main series does; one or two per frame.
/// A view near the live edge returns to it. The slots are rebuilt on the next frame.
#[test]
fn added_only_appends_keep_a_scrolled_back_view() {
    let data = bars(&[10.0; 202]);
    for added_only in [false, true] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(if added_only {
            vec![]
        } else {
            data[..199].to_vec()
        }));
        let id = if added_only {
            let id = chart.add_series(SeriesOptions::default());
            chart.set_data(id, data[..199].to_vec());
            id
        } else {
            SeriesId::MAIN
        };
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.scroll_to_position(-30.0, false);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let before = chart.visible_time_range();
        assert!(before.is_some());
        chart.update(id, data[199].clone());
        frame(&ctx, &mut chart, 0.2, vec![]);
        assert_eq!(chart.visible_time_range(), before, "{added_only}");
        assert_eq!(chart.get_time_scale_state().1, -31.0, "{added_only}");
        chart.update(id, data[200].clone());
        chart.update(id, data[201].clone());
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(chart.visible_time_range(), before, "{added_only}");
        assert_eq!(chart.get_time_scale_state().1, -33.0, "{added_only}");

        chart.scroll_to_position(-1.0, false);
        frame(&ctx, &mut chart, 0.4, vec![]);
        let next = data[201].time + TimeDelta::minutes(1);
        chart.update(id, Bar::new(next, 10.0, 10.0, 10.0, 10.0, 1.0));
        frame(&ctx, &mut chart, 0.5, vec![]);
        assert_eq!(chart.get_time_scale_state().1, 2.5, "{added_only}");
    }
}

/// A fit of 1000 added bars into 800 points is below one point per bar, so the visible range groups
/// the slots of the added series while the main series has no groups to read. The volume pane, on
/// by default, draws nothing and the frame ends on the newest added bar.
#[test]
fn added_only_fit_paints_the_volume_pane() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(vec![]));
    let id = chart.add_series(SeriesOptions::default());
    chart.set_data(id, bars(&[10.0; 1000]));
    frame(&ctx, &mut chart, 0.0, vec![]);
    chart.fit_content();
    frame(&ctx, &mut chart, 0.1, vec![]);
    assert!(chart.get_time_scale_state().0 < 1.0);
    assert_eq!(chart.state.visible_range().1, 1000);
}

/// Last-value queries skip whitespace and use shared indices, each pane's scale and the
/// value formatter. From a visible base of 100, 120 is +20% and a loaded 150 is +50%.
/// A move from 100 to 130 forms three size-10 bricks with the same source timestamp.
#[test]
fn last_value_queries_select_loaded_or_visible_on_the_series_scale() {
    use crate::model::{BarColors, LogicalRange};
    use crate::scales::CustomPriceFormatter;
    use std::sync::Arc;

    for own_pane in [false, true] {
        for (mode, visible_text, loaded_text) in [
            (
                PriceScaleMode::Normal,
                ["120.000", "1,200.000"],
                ["150.000", "1,500.000"],
            ),
            (
                PriceScaleMode::Logarithmic,
                ["120.000", "1,200.000"],
                ["150.000", "1,500.000"],
            ),
            (PriceScaleMode::Percentage, ["+20.00%"; 2], ["+50.00%"; 2]),
            (PriceScaleMode::IndexedTo100, ["120.00"; 2], ["150.00"; 2]),
        ] {
            let ctx = egui::Context::default();
            let mut data = bars(&[100.0, 110.0, 120.0, 140.0, 150.0, 160.0]);
            data[2].colors = Some(Box::new(BarColors {
                color: Some(Color32::CYAN),
                ..Default::default()
            }));
            data[4].open = 160.0;
            data[4].high = 160.0;
            data[5] = Bar::whitespace(data[5].time);
            let mut chart = Chart::new(BarData::from_bars(data.clone()));
            let id = chart.add_series(SeriesOptions {
                scale: SeriesScale::Left,
                color: Color32::MAGENTA,
                ..Default::default()
            });
            chart.set_data(
                id,
                vec![
                    data[0].clone(),
                    data[2].clone(),
                    data[4].clone(),
                    data[5].clone(),
                ]
                .into_iter()
                .map(|mut bar| {
                    bar.open *= 10.0;
                    bar.high *= 10.0;
                    bar.low *= 10.0;
                    bar.close *= 10.0;
                    bar
                })
                .collect(),
            );
            let empty = chart.add_series(SeriesOptions::default());
            if own_pane {
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(id, pane));
            }
            assert_eq!(chart.series_last_value(id, PriceLineSource::LastBar), None);
            for series in [SeriesId::MAIN, id] {
                let scale = chart.series_price_scale_mut(series).unwrap();
                scale.options_mut().mode = mode;
                scale.set_price_precision(Some(3));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            assert!(chart.set_visible_logical_range(LogicalRange::new(0.0, 2.0)));
            frame(&ctx, &mut chart, 0.1, vec![]);
            for series in [SeriesId::MAIN, id] {
                let base = if series == id { 1000.0_f64 } else { 100.0_f64 };
                let (low, high) = match mode {
                    PriceScaleMode::Normal => (base, base * 2.0),
                    PriceScaleMode::Logarithmic => (base.ln(), (base * 2.0).ln()),
                    PriceScaleMode::Percentage => (0.0, 100.0),
                    PriceScaleMode::IndexedTo100 => (100.0, 200.0),
                };
                chart
                    .series_price_scale_mut(series)
                    .unwrap()
                    .set_range(low, high);
            }
            frame(&ctx, &mut chart, 0.2, vec![]);
            for series in [SeriesId::MAIN, id] {
                let added = usize::from(series == id);
                for (source, value, index, expected_text, fraction) in [
                    (
                        PriceLineSource::LastVisible,
                        [120.0, 1200.0][added],
                        2.0,
                        visible_text[added],
                        0.2,
                    ),
                    (
                        PriceLineSource::LastBar,
                        [150.0, 1500.0][added],
                        4.0,
                        loaded_text[added],
                        0.5,
                    ),
                ] {
                    let last: crate::widget::SeriesLastValue =
                        chart.series_last_value(series, source).unwrap();
                    assert_eq!((last.value, last.index), (value, index));
                    assert_eq!(last.text, expected_text, "{mode:?}");
                    let expected_color = if source == PriceLineSource::LastVisible {
                        Color32::CYAN
                    } else if series == id {
                        Color32::MAGENTA
                    } else {
                        chart.config.bearish_color
                    };
                    assert_eq!(last.color, expected_color);
                    let (top, bottom) = chart.series_price_map(series).unwrap().extent();
                    // ln(1.2)/ln(2) and ln(1.5)/ln(2), independently calculated.
                    let fraction = if mode == PriceScaleMode::Logarithmic {
                        if source == PriceLineSource::LastVisible {
                            0.263_034_4
                        } else {
                            0.584_962_5
                        }
                    } else {
                        fraction
                    };
                    assert!((last.coordinate - (bottom - (bottom - top) * fraction)).abs() < 0.001);
                }
                chart.set_series_formatter(
                    series,
                    Some(Arc::new(CustomPriceFormatter::new(|value| {
                        format!("raw {value:.1}")
                    }))),
                );
                chart.set_percentage_formatter(Some(Arc::new(CustomPriceFormatter::new(
                    |value| format!("relative {value:.1}"),
                ))));
                let expected = match mode {
                    PriceScaleMode::Normal | PriceScaleMode::Logarithmic => {
                        ["raw 120.0", "raw 1200.0"][added]
                    }
                    PriceScaleMode::Percentage => "relative 20.0",
                    PriceScaleMode::IndexedTo100 => "relative 120.0",
                };
                assert_eq!(
                    chart
                        .series_last_value(series, PriceLineSource::LastVisible)
                        .unwrap()
                        .text,
                    expected
                );
                chart.set_series_formatter(series, None);
                chart.set_percentage_formatter(None);
            }
            for source in [PriceLineSource::LastBar, PriceLineSource::LastVisible] {
                for series in [empty, SeriesId(99), SeriesId::VOLUME] {
                    assert_eq!(chart.series_last_value(series, source), None);
                }
            }
            assert!(chart.set_series_visible(id, false));
            chart.series_style_mut(id).unwrap().last_value_visible = false;
            assert!(
                chart
                    .series_last_value(id, PriceLineSource::LastBar)
                    .is_some()
            );
            assert!(chart.set_visible_logical_range(LogicalRange::new(10.0, 13.0)));
            frame(&ctx, &mut chart, 0.3, vec![]);
            for series in [SeriesId::MAIN, id] {
                assert_eq!(
                    chart.series_last_value(series, PriceLineSource::LastVisible),
                    None,
                    "{:?}",
                    chart.visible_logical_range()
                );
            }
            for series in [SeriesId::MAIN, id] {
                for data in [vec![Bar::whitespace(data[5].time)], Vec::new()] {
                    chart.set_data(series, data);
                    for source in [PriceLineSource::LastBar, PriceLineSource::LastVisible] {
                        assert_eq!(chart.series_last_value(series, source), None);
                    }
                }
                assert!(chart.remove_series(series));
                for source in [PriceLineSource::LastBar, PriceLineSource::LastVisible] {
                    assert_eq!(chart.series_last_value(series, source), None);
                }
            }
        }
    }
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 130.0])));
    chart.set_chart_type(ChartType::Renko);
    let mut params = chart.chart_type_params().clone();
    params.renko_brick_size = 10.0;
    chart.set_chart_type_params(params);
    frame(&ctx, &mut chart, 0.0, vec![]);
    assert_eq!(
        chart
            .state
            .shown()
            .iter()
            .map(|bar| bar.close)
            .collect::<Vec<_>>(),
        [110.0, 120.0, 130.0]
    );
    let last = chart
        .series_last_value(SeriesId::MAIN, PriceLineSource::LastVisible)
        .unwrap();
    assert_eq!((last.value, last.index), (130.0, 2.0));
}

/// Columns, hollow candles, and candles coloured by the previous close, take their direction from
/// the close before them. The last bar opens at 13 and closes at 12.5, above the previous close of
/// 12, so it is painted in the rising colour, and its price line, label and last value take that
/// colour too. Candles compared with their open stay falling, and the bar before the first in
/// view still counts when only the last bar is in view.
#[test]
fn last_value_colour_follows_the_previous_close_rule() {
    use crate::widget::test_frames::shapes;
    let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
    let start = "2024-01-01T00:00:00Z"
        .parse::<chrono::DateTime<Utc>>()
        .unwrap();
    let data = vec![
        Bar::new(start, 10.0, 12.5, 9.0, 12.0, 1.0),
        Bar::new(start + TimeDelta::days(1), 13.0, 13.5, 12.0, 12.5, 1.0),
    ];
    for (kind, by_previous_close, expected) in [
        (ChartType::HollowCandles, false, up),
        (ChartType::Columns, false, up),
        (ChartType::Candles, true, up),
        (ChartType::Candles, false, down),
        (ChartType::VolumeCandles, true, up),
        (ChartType::VolumeCandles, false, down),
    ] {
        for source in [PriceLineSource::LastBar, PriceLineSource::LastVisible] {
            let ctx = egui::Context::default();
            let mut chart = Chart::new(BarData::from_bars(data.clone()));
            chart.set_chart_type(kind);
            chart.config_mut().show_volume = false;
            chart.config_mut().bullish_color = up;
            chart.config_mut().bearish_color = down;
            let mut params = chart.chart_type_params().clone();
            params.styles.hollow.body_up = up;
            params.styles.hollow.body_down = down;
            params.styles.volume_candles.body_up = up;
            params.styles.volume_candles.body_down = down;
            params.styles.candles.color_by_previous_close = by_previous_close;
            params.styles.volume_candles.color_by_previous_close = by_previous_close;
            chart.set_chart_type_params(params);
            chart.main_series_style.price_line_source = source;
            frame(&ctx, &mut chart, 0.0, vec![]);
            if source == PriceLineSource::LastVisible {
                assert!(chart.set_visible_logical_range(crate::model::LogicalRange::new(1.0, 1.4)));
            }
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            assert_eq!(
                chart.state.visible_range(),
                (usize::from(source == PriceLineSource::LastVisible), 2,)
            );
            let label = painted.iter().find_map(|shape| match shape {
                egui::Shape::Rect(r)
                    if !plot.contains(r.rect.center()) && [up, down].contains(&r.fill) =>
                {
                    Some(r.fill)
                }
                _ => None,
            });
            let line = painted.iter().find_map(|shape| match shape {
                egui::Shape::LineSegment { points, stroke }
                    if points[0].y == points[1].y && [up, down].contains(&stroke.color) =>
                {
                    Some(stroke.color)
                }
                _ => None,
            });
            let value = chart.series_last_value(SeriesId::MAIN, source).unwrap();
            assert_eq!(
                (label, line, value.color),
                (Some(expected), Some(expected), expected),
                "{kind:?} {by_previous_close} {source:?}"
            );
        }
    }
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

/// Added series pass explicit line, OHLC and area settings to both pane render paths and
/// retain them in snapshots. Old options keep the same renderer defaults.
#[test]
fn added_series_styles_reach_frames_and_saved_layouts() {
    use egui::Shape;
    use serde_json::json;

    for own_pane in [false, true] {
        for (kind, settings) in [
            (
                "curve",
                json!({"line_style": {"color": [255, 0, 255, 255], "width": 4.0, "line_type": "Curved"}}),
            ),
            (
                "dash",
                json!({"line_style": {"color": [255, 0, 255, 255], "width": 4.0, "style": "Dashed"}}),
            ),
            (
                "dots",
                json!({"chart_type": "StepLine", "line_style": {"color": [255, 0, 255, 255], "width": 4.0, "style": "Dotted"}}),
            ),
            (
                "markers",
                json!({"chart_type": "LineWithMarkers", "line_style": {"color": [255, 0, 255, 255], "width": 4.0, "line_type": "Curved"}}),
            ),
            (
                "ohlc",
                json!({"chart_type": "Bars", "bars_style": {"open_visible": false, "thin_bars": false}}),
            ),
            (
                "thin",
                json!({"chart_type": "Bars", "bars_style": {"open_visible": true, "thin_bars": true}}),
            ),
            (
                "area",
                json!({"chart_type": "Area", "area_style": {"line": {"width": 0.0}, "top_fill": [0, 0, 0, 200], "bottom_fill": [0, 0, 0, 40], "relative_gradient": false, "invert_filled_area": false}}),
            ),
            (
                "relative",
                json!({"chart_type": "Area", "area_style": {"line": {"width": 0.0}, "top_fill": [0, 0, 0, 200], "bottom_fill": [0, 0, 0, 40], "relative_gradient": true, "invert_filled_area": false}}),
            ),
            (
                "inverted",
                json!({"chart_type": "Area", "area_style": {"line": {"width": 0.0}, "top_fill": [0, 0, 0, 200], "bottom_fill": [0, 0, 0, 40], "relative_gradient": false, "invert_filled_area": true}}),
            ),
            (
                "relative inverted",
                json!({"chart_type": "Area", "area_style": {"line": {"width": 0.0}, "top_fill": [0, 0, 0, 200], "bottom_fill": [0, 0, 0, 40], "relative_gradient": true, "invert_filled_area": true}}),
            ),
        ] {
            let ctx = egui::Context::default();
            let data = bars(&[10.0, 20.0, 15.0, 25.0]);
            let mut chart = Chart::new(BarData::from_bars(data.clone()));
            chart.set_main_series_visible(false);
            chart.config.show_volume = false;
            let mut saved = serde_json::to_value(SeriesOptions {
                up_color: Color32::MAGENTA,
                down_color: Color32::MAGENTA,
                scale: SeriesScale::Separate(7),
                ..Default::default()
            })
            .unwrap();
            saved
                .as_object_mut()
                .unwrap()
                .extend(settings.as_object().unwrap().clone());
            saved["last_price_animation"] = json!("OnDataUpdate");
            let options: SeriesOptions = serde_json::from_value(saved).unwrap();
            let id = chart.add_series(options.clone());
            chart.set_data(id, data.clone());
            if own_pane {
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(id, pane));
            }
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.series_price_map(id).unwrap();
            let rect = Rect::from_min_max(
                Pos2::new(0.0, map.price_to_y(map.range().max)),
                Pos2::new(800.0, map.price_to_y(map.range().min)),
            );
            match kind {
                "curve" | "markers" => {
                    let path = painted
                        .iter()
                        .find_map(|shape| match shape {
                            Shape::Path(path)
                                if path.stroke.color
                                    == egui::epaint::ColorMode::Solid(Color32::MAGENTA) =>
                            {
                                Some(path)
                            }
                            _ => None,
                        })
                        .expect("explicit curved line");
                    assert_eq!(path.stroke.width, 4.0);
                    assert!(path.points.len() > data.len(), "{kind}: curve subdivision");
                    for (index, bar) in data.iter().enumerate() {
                        assert!(path.points.contains(&Pos2::new(
                            chart.index_to_x(index as f64).unwrap(),
                            map.price_to_y(bar.close)
                        )));
                    }
                }
                "dash" | "dots" | "ohlc" | "thin" => {
                    let segments: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::LineSegment { points, stroke }
                                if stroke.color == Color32::MAGENTA =>
                            {
                                Some((points, stroke))
                            }
                            _ => None,
                        })
                        .collect();
                    if matches!(kind, "ohlc" | "thin") {
                        assert_eq!(
                            segments.len(),
                            data.len() * if kind == "ohlc" { 2 } else { 3 }
                        );
                        let width = if kind == "thin" {
                            chart.config.wick_width
                        } else {
                            (chart.state.time_scale().bar_spacing() * chart.config.candle_width
                                / 2.0)
                                .max(1.5)
                        };
                        assert!(segments.iter().all(|(_, stroke)| stroke.width == width));
                    } else {
                        assert!(segments.len() > data.len(), "{kind}: patterned stroke");
                        assert!(segments.iter().all(|(_, stroke)| stroke.width == 4.0));
                    }
                }
                _ => {
                    let mesh = painted
                        .iter()
                        .find_map(|shape| match shape {
                            Shape::Mesh(mesh)
                                if mesh.vertices.len() == data.len() * 2
                                    && mesh.vertices[1].color.r() == 0 =>
                            {
                                Some(mesh)
                            }
                            _ => None,
                        })
                        .expect("explicit area gradient");
                    let relative = kind.contains("relative");
                    let inverted = kind.contains("inverted");
                    let top = if relative && !inverted {
                        map.price_to_y(25.0)
                    } else {
                        rect.top()
                    };
                    let bottom = if relative && inverted {
                        map.price_to_y(10.0)
                    } else {
                        rect.bottom()
                    };
                    for pair in mesh.vertices.as_chunks::<2>().0 {
                        assert_eq!(
                            pair[1].pos.y,
                            if inverted { rect.top() } else { rect.bottom() }
                        );
                        for vertex in pair {
                            // Linear alpha interpolation: 200 at the top, 40 at the bottom.
                            let fraction = (vertex.pos.y - top) / (bottom - top);
                            let expected = (200.0 - 160.0 * fraction.clamp(0.0, 1.0)).round() as u8;
                            assert!(
                                vertex.color.a().abs_diff(expected) <= 1,
                                "{kind}: gradient at {:?}",
                                vertex.pos
                            );
                        }
                    }
                }
            }
            #[cfg(feature = "ui")]
            {
                let saved = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
                let mut restored = Chart::new(BarData::from_bars(data.clone()));
                restored
                    .restore(&serde_json::from_str(&saved).unwrap())
                    .unwrap();
                assert_eq!(restored.series().next().unwrap().1, &options);
                let restored_id = restored.restored_series_id(id.0).unwrap();
                assert_eq!(
                    restored.series_pane(restored_id) == Some(PaneId::MAIN),
                    !own_pane
                );
            }
        }
    }
    let mut legacy = serde_json::to_value(SeriesOptions::default()).unwrap();
    for field in [
        "line_style",
        "bars_style",
        "area_style",
        "last_price_animation",
    ] {
        legacy.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(
        serde_json::from_value::<SeriesOptions>(legacy).unwrap(),
        SeriesOptions::default()
    );
}

/// Update pulses are independent of the main series, other added series and pane scales.
/// An expired or hidden pulse requests no repaint; a continuous pulse keeps running.
#[test]
fn added_series_pulses_run_independently_and_stop_repainting() {
    use egui::Shape;

    for chart_type in [
        ChartType::Line,
        ChartType::StepLine,
        ChartType::LineWithMarkers,
        ChartType::Area,
        ChartType::Baseline,
    ] {
        let ctx = egui::Context::default();
        let data = bars(&[10.0, 20.0, 15.0, 25.0]);
        let mut chart = Chart::new(BarData::from_bars(bars(&[1000.0, 2000.0, 1500.0, 2500.0])));
        let mut ids = Vec::new();
        for (at, color) in [Color32::MAGENTA, Color32::GOLD].into_iter().enumerate() {
            let mut saved = serde_json::to_value(SeriesOptions {
                chart_type,
                color,
                up_color: color,
                down_color: color,
                marker_radius: Some(0.0),
                scale: SeriesScale::Separate(at as u64),
                ..Default::default()
            })
            .unwrap();
            saved["last_price_animation"] = serde_json::json!("OnDataUpdate");
            let id = chart.add_series(serde_json::from_value(saved).unwrap());
            chart.set_data(id, data.clone());
            if at == 1 {
                let pane = chart.add_pane();
                chart.move_series_to_pane(id, pane);
            }
            ids.push(id);
        }
        let dots = |painted: &[Shape], color| {
            painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(dot) if dot.radius == 3.0 && dot.fill == color => {
                        Some(dot.center)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
        assert!(dots(&painted, Color32::MAGENTA).is_empty());
        assert!(dots(&painted, Color32::GOLD).is_empty());
        chart.update(
            SeriesId::MAIN,
            bars(&[1000.0, 2000.0, 1500.0, 2400.0]).pop().unwrap(),
        );
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.5, vec![]);
        assert!(dots(&painted, Color32::MAGENTA).is_empty());
        assert!(dots(&painted, Color32::GOLD).is_empty());
        for (step, id) in ids.iter().enumerate() {
            chart.update(*id, bars(&[10.0, 20.0, 15.0, 24.0]).pop().unwrap());
            let painted =
                crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0 + step as f64, vec![]);
            let color = if step == 0 {
                Color32::MAGENTA
            } else {
                Color32::GOLD
            };
            let map = chart.series_price_map(*id).unwrap();
            assert_eq!(
                dots(&painted, color),
                [Pos2::new(
                    chart.index_to_x(3.0).unwrap(),
                    map.price_to_y(24.0)
                )],
                "{chart_type:?}"
            );
            if step == 0 {
                assert!(dots(&painted, Color32::GOLD).is_empty());
            }
        }
        for time in [5.0, 5.1, 5.2] {
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, time, vec![]);
            assert!(dots(&painted, Color32::MAGENTA).is_empty());
            assert!(dots(&painted, Color32::GOLD).is_empty());
        }
        assert!(
            !ctx.has_requested_repaint(),
            "{chart_type:?}: expired update pulses"
        );
        let mut saved = serde_json::to_value(chart.series().next().unwrap().1).unwrap();
        saved["last_price_animation"] = serde_json::json!("Continuous");
        chart.set_series_options(ids[0], serde_json::from_value(saved).unwrap());
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 6.0, vec![]);
        assert_eq!(dots(&painted, Color32::MAGENTA).len(), 1);
        assert!(dots(&painted, Color32::GOLD).is_empty());
        assert!(ctx.has_requested_repaint());
        chart.set_series_visible(ids[0], false);
        for time in [7.0, 7.1, 7.2] {
            frame(&ctx, &mut chart, time, vec![]);
        }
        assert!(!ctx.has_requested_repaint());
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

/// Saved strokes draw independently on main and added series, at their first visible close:
/// (close - base) / base * 100 = 0%, or close / base * 100 = 100.
#[test]
fn relative_base_lines_use_each_series_stroke_and_visibility() {
    for own_pane in [false, true] {
        for mode in [
            PriceScaleMode::Normal,
            PriceScaleMode::Logarithmic,
            PriceScaleMode::Percentage,
            PriceScaleMode::IndexedTo100,
        ] {
            for (pattern, dash, gap) in [
                ("Solid", 0.0, 0.0),
                ("Dashed", 4.0, 4.0),
                ("Dotted", 1.0, 3.0),
                ("LargeDashed", 8.0, 8.0),
                ("SparseDotted", 1.0, 7.0),
            ] {
                let ctx = egui::Context::default();
                let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 90.0])));
                let added = chart.add_series(SeriesOptions {
                    scale: SeriesScale::Separate(2),
                    ..Default::default()
                });
                chart.set_data(added, bars(&[200.0, 180.0, 220.0]));
                if own_pane {
                    let pane = chart.add_pane();
                    assert!(chart.move_series_to_pane(added, pane));
                }
                let cases = [
                    (SeriesId::MAIN, Color32::MAGENTA, 3.0, 100.0),
                    (added, Color32::GOLD, 2.0, 200.0),
                ];
                for (id, color, width, _) in cases {
                    let mut saved = serde_json::to_value(chart.series_style(id).unwrap()).unwrap();
                    saved["base_line_color"] = serde_json::to_value(color).unwrap();
                    saved["base_line_width"] = serde_json::json!(width);
                    saved["base_line_style"] =
                        serde_json::json!(if id == added { "LargeDashed" } else { pattern });
                    saved["base_line_visible"] = serde_json::json!(true);
                    *chart.series_style_mut(id).unwrap() = serde_json::from_value(saved).unwrap();
                    chart.series_price_scale_mut(id).unwrap().options_mut().mode = mode;
                }
                for (at, hidden_line, hidden_series) in [
                    (0.0, None, None),
                    (0.1, Some(SeriesId::MAIN), None),
                    (0.2, Some(added), None),
                    (0.3, None, Some(SeriesId::MAIN)),
                    (0.4, None, Some(added)),
                ] {
                    for (id, ..) in cases {
                        chart.series_style_mut(id).unwrap().base_line_visible =
                            hidden_line != Some(id);
                        assert!(chart.set_series_visible(id, hidden_series != Some(id)));
                    }
                    let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, at, vec![]);
                    for (id, color, width, base) in cases {
                        let (pattern, dash, gap) = if id == added {
                            ("LargeDashed", 8.0, 8.0)
                        } else {
                            (pattern, dash, gap)
                        };
                        let mut segments = Vec::new();
                        for shape in &painted {
                            match shape {
                                egui::Shape::LineSegment { points, stroke }
                                    if stroke.color == color =>
                                {
                                    assert_eq!(stroke.width, width);
                                    segments.push(*points);
                                }
                                egui::Shape::Mesh(mesh)
                                    if mesh.vertices.first().is_some_and(|v| v.color == color) =>
                                {
                                    for dot in mesh.vertices.as_chunks::<4>().0 {
                                        let positions: [Pos2; 4] =
                                            std::array::from_fn(|at| dot[at].pos);
                                        let rect = Rect::from_points(&positions);
                                        assert_eq!(rect.height(), width);
                                        segments.push([
                                            egui::pos2(rect.left(), rect.center().y),
                                            egui::pos2(rect.right(), rect.center().y),
                                        ]);
                                    }
                                }
                                _ => {}
                            }
                        }
                        let visible = matches!(
                            mode,
                            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
                        ) && hidden_line != Some(id)
                            && hidden_series != Some(id);
                        assert_eq!(
                            !segments.is_empty(),
                            visible,
                            "{id:?} {mode:?} {pattern} pane={own_pane}"
                        );
                        if !visible {
                            continue;
                        }
                        let map = chart.series_price_map(id).unwrap();
                        assert_eq!(
                            map.transform().to_scale(base),
                            if mode == PriceScaleMode::Percentage {
                                0.0
                            } else {
                                100.0
                            }
                        );
                        for points in &segments {
                            let tolerance = if dash == 1.0 { 0.51 } else { 0.01 };
                            assert!((points[0].y - map.price_to_y(base)).abs() < tolerance);
                            assert_eq!(points[0].y, points[1].y);
                        }
                        if pattern == "Solid" {
                            assert_eq!(segments.len(), 1);
                        } else {
                            assert!(segments.len() > 2);
                            assert!((segments[0][1].x - segments[0][0].x - dash).abs() < 0.01);
                            assert!((segments[1][0].x - segments[0][1].x - gap).abs() < 0.01);
                        }
                    }
                }
            }
        }
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
        (200.0, Color32::from_rgb(178, 181, 190)),
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

/// A 100..200 range in a 500-pixel plot with 10% scale margins and 49/63 extra
/// pixels uses 100 / (0.8 - 112/500) = 173.611111 scale units. Its upper price is
/// 50 + 49 pixels below the top, and its lower price is 50 + 63 above the bottom.
#[test]
fn autoscale_contributions_reserve_pixels_per_pane_and_scale() {
    use crate::chart::primitives::{Primitive, PrimitivePass, PrimitiveView};
    use crate::model::{AutoscaleInfo, AutoscaleMargins, AutoscaleProvider};

    struct FixedGraphics;
    impl Primitive for FixedGraphics {
        fn autoscale_info(
            &self,
            bars: &[Bar],
            visible: std::ops::Range<usize>,
        ) -> Option<AutoscaleInfo> {
            let values = &bars[visible];
            Some(AutoscaleInfo {
                price_range: (values.first()?.close, values.last()?.close),
                margins: AutoscaleMargins {
                    top: 13.0,
                    bottom: 63.0,
                },
            })
        }

        fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            if pass != PrimitivePass::Above || view.visible.is_empty() {
                return;
            }
            let low = view
                .mapping
                .price
                .price_to_y(view.bars[view.visible.start].close);
            let high = view
                .mapping
                .price
                .price_to_y(view.bars[view.visible.end - 1].close);
            let x = view.mapping.rect.center().x;
            for (y, height) in [(low.min(high) - 13.0, 13.0), (low.max(high), 63.0)] {
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, y), egui::vec2(18.0, height)),
                    0.0,
                    Color32::MAGENTA,
                );
            }
        }
    }

    for placement in ["main", "moved main", "added"] {
        for mode in [
            PriceScaleMode::Normal,
            PriceScaleMode::Logarithmic,
            PriceScaleMode::Percentage,
            PriceScaleMode::IndexedTo100,
        ] {
            for inverted in [false, true] {
                let ctx = egui::Context::default();
                let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 200.0])));
                chart.config.show_volume = false;
                let target = if placement == "added" {
                    let id = chart.add_series(SeriesOptions::default());
                    chart.set_data(id, bars(&[100.0, 200.0]));
                    id
                } else {
                    SeriesId::MAIN
                };
                let pane = if placement == "main" {
                    PaneId::MAIN
                } else {
                    chart.add_pane()
                };
                assert!(chart.move_series_to_pane(target, pane));
                chart.series_style_mut(target).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(|_, default| {
                        Some(AutoscaleInfo {
                            price_range: default?,
                            margins: AutoscaleMargins {
                                top: 37.0,
                                bottom: 11.0,
                            },
                        })
                    }));
                let sibling = chart.add_series(SeriesOptions::default());
                chart.set_data(sibling, bars(&[100.0, 200.0]));
                assert!(chart.move_series_to_pane(sibling, pane));
                chart.series_style_mut(sibling).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(|_, default| {
                        Some(AutoscaleInfo {
                            price_range: default?,
                            margins: AutoscaleMargins {
                                top: 49.0,
                                bottom: 17.0,
                            },
                        })
                    }));
                let independent = chart.add_series(SeriesOptions {
                    scale: SeriesScale::Separate(7),
                    ..Default::default()
                });
                chart.set_data(independent, bars(&[100.0, 200.0]));
                assert!(chart.move_series_to_pane(independent, pane));
                chart.series_style_mut(independent).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(|_, default| {
                        Some(AutoscaleInfo {
                            price_range: default?,
                            margins: AutoscaleMargins {
                                top: 7.0,
                                bottom: 19.0,
                            },
                        })
                    }));
                let primitive = chart.attach_primitive(target, FixedGraphics).unwrap();
                for id in [target, independent] {
                    let options = chart.series_price_scale_mut(id).unwrap().options_mut();
                    options.mode = mode;
                    options.invert_scale = inverted;
                    options.scale_margins = crate::scales::PriceScaleMargins {
                        top: 0.1,
                        bottom: 0.1,
                    };
                }
                for (step, (height, last)) in [(900.0, 200.0), (650.0, 400.0), (900.0, 200.0)]
                    .into_iter()
                    .enumerate()
                {
                    chart.set_data(SeriesId::MAIN, bars(&[100.0, last]));
                    for id in [target, sibling, independent] {
                        chart.set_data(id, bars(&[100.0, last]));
                    }
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(1440.0, height),
                            )),
                            time: Some(step as f64),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                        },
                    );
                    let painted: Vec<_> = output
                        .shapes
                        .iter()
                        .map(|shape| shape.shape.clone())
                        .collect();
                    output.drop_without_applying_deltas();
                    let map = chart.series_price_map(target).unwrap();
                    let (top, bottom) = map.extent();
                    let plot_height = bottom - top;
                    let transform = map.transform();
                    let (low, high) = (transform.to_scale(100.0), transform.to_scale(last));
                    // Secondary bar panes already pad their raw span by 10% on each side.
                    let padding = if placement == "added" && mode != PriceScaleMode::Logarithmic {
                        (high - low) * 0.1
                    } else {
                        0.0
                    };
                    let total =
                        (high - low + 2.0 * padding) / (0.8 - 112.0 / f64::from(plot_height));
                    let (lower, upper) = if inverted { (49.0, 63.0) } else { (63.0, 49.0) };
                    let expected = (
                        low - padding - total * (0.1 + lower / f64::from(plot_height)),
                        high + padding + total * (0.1 + upper / f64::from(plot_height)),
                    );
                    assert!(
                        (map.range().min - expected.0).abs() < 1e-8
                            && (map.range().max - expected.1).abs() < 1e-8,
                        "{placement}, {mode:?}, inverted={inverted}: {:?}, expected {expected:?}",
                        map.range()
                    );
                    let ys = crate::widget::test_frames::candle_ys(&painted, Color32::MAGENTA);
                    assert_eq!(ys.len(), 4, "both fixed-size graphics are rendered");
                    assert!(ys.iter().all(|y| *y >= top && *y <= bottom));
                    let isolated = chart.series_price_map(independent).unwrap();
                    let span = (high - low + 2.0 * padding) / (0.8 - 26.0 / f64::from(plot_height));
                    let (lower, upper) = if inverted { (7.0, 19.0) } else { (19.0, 7.0) };
                    assert!(
                        (isolated.range().min
                            - (low - padding - span * (0.1 + lower / f64::from(plot_height))))
                        .abs()
                            < 1e-8
                    );
                    assert!(
                        (isolated.range().max
                            - (high + padding + span * (0.1 + upper / f64::from(plot_height))))
                        .abs()
                            < 1e-8
                    );
                    if placement == "added" {
                        let main = chart.series_price_map(SeriesId::MAIN).unwrap().range();
                        let span = (last - 100.0) / 0.8;
                        assert!((main.min - (100.0 - span * 0.1)).abs() < 1e-8);
                        assert!((main.max - (last + span * 0.1)).abs() < 1e-8);
                    }
                }
                if placement == "main" {
                    chart.set_markers(
                        target,
                        vec![
                            Marker::new(chart.bars(target)[0].time)
                                .at_price(
                                    if inverted { 200.0 } else { 100.0 },
                                    MarkerPos::AtPriceBottom,
                                )
                                .with_shape(crate::model::MarkerShape::Circle)
                                .with_size(8.0),
                        ],
                    );
                    frame(&ctx, &mut chart, 3.0, vec![]);
                    let map = chart.series_price_map(target).unwrap();
                    let (top, bottom) = map.extent();
                    let a = map.price_to_y(100.0);
                    let b = map.price_to_y(200.0);
                    assert!(
                        a.min(b) - top >= (bottom - top) * 0.1 + 49.0 - 0.001,
                        "marker fitting retains the provider's top clearance"
                    );
                    assert!(
                        bottom - a.max(b) >= (bottom - top) * 0.1 + 63.0 - 0.001,
                        "marker fitting retains the primitive's bottom clearance"
                    );
                    chart.set_markers(target, vec![]);
                }
                let range = chart.series_price_scale(target).unwrap().range();
                chart
                    .series_price_scale_mut(target)
                    .unwrap()
                    .set_range(range.min, range.max);
                chart.series_style_mut(target).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(|_, _| {
                        Some(AutoscaleInfo {
                            price_range: (-1e6, 1e6),
                            margins: AutoscaleMargins {
                                top: 300.0,
                                bottom: 300.0,
                            },
                        })
                    }));
                frame(&ctx, &mut chart, 4.0, vec![]);
                assert_eq!(
                    chart.series_price_scale(target).unwrap().range(),
                    range,
                    "manual ranges ignore all contributions"
                );
                chart.series_price_scale_mut(target).unwrap().reset();
                chart.series_style_mut(target).unwrap().autoscale = None;
                chart.detach_primitive(primitive);
                chart.set_series_visible(sibling, false);
                frame(&ctx, &mut chart, 5.0, vec![]);
                let ordinary = chart.series_price_scale(target).unwrap().range();
                assert!(!chart.series_price_scale(target).unwrap().is_manual());
                let transform = chart.series_price_map(target).unwrap().transform();
                let (low, high) = (transform.to_scale(100.0), transform.to_scale(200.0));
                // Secondary bar panes already pad their raw span by 10% on each side.
                let padding = if placement == "added" && mode != PriceScaleMode::Logarithmic {
                    (high - low) * 0.1
                } else {
                    0.0
                };
                let beyond = padding + (high - low + 2.0 * padding) / 0.8 * 0.1;
                assert!(
                    (ordinary.min - (low - beyond)).abs() < 1e-8
                        && (ordinary.max - (high + beyond)).abs() < 1e-8,
                    "hidden and detached contributors release their space: \
                     {placement}, {mode:?}, inverted={inverted}: {ordinary:?}"
                );
                for (step, margins) in [
                    AutoscaleMargins::default(),
                    AutoscaleMargins {
                        top: f32::NAN,
                        bottom: -5.0,
                    },
                    AutoscaleMargins {
                        top: f32::INFINITY,
                        bottom: f32::NEG_INFINITY,
                    },
                    AutoscaleMargins {
                        top: 1e6,
                        bottom: 1e6,
                    },
                ]
                .into_iter()
                .enumerate()
                {
                    chart.series_style_mut(target).unwrap().autoscale =
                        Some(AutoscaleProvider::with_margins(move |_, default| {
                            Some(AutoscaleInfo {
                                price_range: default?,
                                margins,
                            })
                        }));
                    frame(&ctx, &mut chart, 6.0 + step as f64, vec![]);
                    assert_eq!(chart.series_price_scale(target).unwrap().range(), ordinary);
                }
                chart.set_data(target, bars(&[100.0, 100.0]));
                chart
                    .series_price_scale_mut(target)
                    .unwrap()
                    .options_mut()
                    .scale_margins = crate::scales::PriceScaleMargins {
                    top: 0.0,
                    bottom: 0.0,
                };
                chart.series_style_mut(target).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(|_, default| {
                        Some(AutoscaleInfo {
                            price_range: default?,
                            margins: AutoscaleMargins {
                                top: 50.0,
                                bottom: 20.0,
                            },
                        })
                    }));
                frame(&ctx, &mut chart, 10.0, vec![]);
                let map = chart.series_price_map(target).unwrap();
                assert!(
                    map.range().max > map.range().min,
                    "flat data still has usable pixel space"
                );
                let (top, bottom) = map.extent();
                let y = map.price_to_y(100.0);
                assert!(y >= top + 50.0 - 0.001 && y <= bottom - 20.0 + 0.001);
                assert!(
                    (y - (top + 50.0 + bottom - 20.0) * 0.5).abs() < 0.001,
                    "a flat value stays centred in the remaining screen space"
                );
            }
        }
    }
}

/// A 40-point pitch with 60%/80% widths gives half-widths of 12/16 points. Mapping
/// -10..10 onto y=20..220 puts zero at 120 and +/-5 at 70/170. Flat and tiny bodies
/// retain the painter's one-point height, above the base when the close equals it.
#[test]
fn column_hits_expand_by_the_series_tolerance_and_stay_in_the_plot() {
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 5])));
    let id = chart.add_series(SeriesOptions {
        chart_type: ChartType::Columns,
        columns_base: Some(0.0),
        ..Default::default()
    });
    frame(&ctx, &mut chart, 0.0, vec![]);
    let rect = Rect::from_min_max(Pos2::new(20.0, 20.0), Pos2::new(420.0, 220.0));
    chart.state.time_scale_mut().set_width(rect.width());
    chart.state.time_scale_mut().set_bar_spacing(40.0);
    chart.state.time_scale_mut().set_right_offset(0.0);
    let x = chart
        .state
        .time_scale()
        .idx_to_coord_precise(2.0, rect.left(), rect.width());
    for (width, half_width) in [(0.6, 12.0), (0.8, 16.0)] {
        chart.config.candle_width = width;
        for inverted in [false, true] {
            let map = PriceMap::new(
                Default::default(),
                crate::scales::PriceRange::new(-10.0, 10.0),
                rect.top(),
                rect.bottom(),
                inverted,
            );
            for (base, close, normal_tip, inverted_tip) in [
                (Some(0.0), 5.0, 70.0, 170.0),
                (Some(0.0), -5.0, 170.0, 70.0),
                (Some(0.0), 0.0, 119.0, 119.0),
                (Some(0.0), 0.01, 119.0, 121.0),
                (Some(0.0), -0.01, 121.0, 119.0),
                (None, 5.0, 70.0, 170.0),
                (None, -10.0, 219.0, 20.0),
            ] {
                chart.set_data(id, vec![bars(&[close; 5])[2].clone()]);
                chart.price_series[0].options.columns_base = base;
                let tip: f32 = if inverted { inverted_tip } else { normal_tip };
                let base = if base.is_some() { 120.0 } else { 220.0 };
                let (top, bottom) = (tip.min(base), tip.max(base));
                for tolerance in [0.0, 2.0, 5.0] {
                    chart.series_style_mut(id).unwrap().hit_tolerance = tolerance;
                    let item = &chart.price_series[0];
                    for (edge, outward) in [
                        (
                            Pos2::new(x - half_width, (top + bottom) / 2.0),
                            egui::vec2(-1.0, 0.0),
                        ),
                        (
                            Pos2::new(x + half_width, (top + bottom) / 2.0),
                            egui::vec2(1.0, 0.0),
                        ),
                        (Pos2::new(x, top), egui::vec2(0.0, -1.0)),
                        (Pos2::new(x, bottom), egui::vec2(0.0, 1.0)),
                    ] {
                        for distance in [-0.25, 0.0, 0.25, 2.0, 2.25, 5.0, 5.25] {
                            let pos = edge + outward * distance;
                            let expected =
                                (distance <= tolerance && rect.contains(pos)).then_some(2);
                            assert_eq!(
                                chart.hit_price_series(item, pos, rect, map),
                                expected,
                                "width={width}, close={close}, base={base}, inverted={inverted}, tolerance={tolerance}, pos={pos:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// At a six-point pitch, 80% columns have 2.4-point half-widths. Padding of 5 or 20
/// can reach past adjacent times; the other columns stay 25 points away vertically.
/// A one-point history must not need an extrapolated time to pick its visible body.
#[test]
fn column_padding_searches_past_neighbours_and_single_point_times() {
    let rect = Rect::from_min_max(Pos2::new(20.0, 20.0), Pos2::new(420.0, 220.0));
    let map = PriceMap::new(
        Default::default(),
        crate::scales::PriceRange::new(-10.0, 10.0),
        rect.top(),
        rect.bottom(),
        false,
    );
    for (len, index) in [(25, 12), (1, 0)] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&vec![100.0; len])));
        chart.config.candle_width = 0.8;
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Columns,
            columns_base: Some(0.0),
            ..Default::default()
        });
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.state.time_scale_mut().set_width(rect.width());
        chart.state.time_scale_mut().set_bar_spacing(6.0);
        chart
            .state
            .time_scale_mut()
            .set_right_offset(if len == 1 { 30.0 } else { 0.0 });
        let x =
            chart
                .state
                .time_scale()
                .idx_to_coord_precise(index as f64, rect.left(), rect.width());
        for close in [5.0, -5.0] {
            let mut values = vec![-close; len];
            values[index] = close;
            chart.set_data(id, bars(&values));
            let y = if close > 0.0 { 95.0 } else { 145.0 };
            for tolerance in [5.0, 20.0] {
                chart.series_style_mut(id).unwrap().hit_tolerance = tolerance;
                for side in [-1.0, 1.0] {
                    for distance in [tolerance - 0.25, tolerance + 0.25] {
                        let pos = Pos2::new(x + side * (2.4 + distance), y);
                        assert!(rect.contains(pos));
                        assert_eq!(
                            chart.hit_price_series(&chart.price_series[0], pos, rect, map),
                            (distance <= tolerance).then_some(index),
                            "len={len}, close={close}, tolerance={tolerance}, side={side}, distance={distance}"
                        );
                    }
                }
            }
        }
    }
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
    for key in [SeriesScale::Right, SeriesScale::Separate(2)] {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        assert!(chart.set_series_scale(SeriesId::MAIN, key));
        let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(800.0, 400.0));
        assert_eq!(
            chart.calculate_layout_rects(widget).price_rect.left(),
            widget.left() + chart.config.padding,
            "the primary right axis needs no left gutter: {key:?}"
        );
    }
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
                // The labels beside the plot, not the pane legend's value inside it.
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("pane:")
                        && !(plot.left()..=plot.right()).contains(&text.pos.x) =>
                {
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
/// that order on every frame, in the price pane and in another pane, including after restoring a
/// serialized chart.
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

    #[cfg(feature = "ui")]
    {
        let snapshot = chart.snapshot().unwrap();
        chart.restore(&snapshot).unwrap();
        for id in &mut ids[1..] {
            *id = chart.restored_series_id(id.0).unwrap();
            chart.set_data(*id, data.clone());
        }
        assert_eq!(chart.series_order(ids[1]), Some(1), "restored pane order");
        assert_eq!(chart.series_order(ids[2]), Some(0), "restored pane order");
    }

    // A secondary pane paints and selects in its own order too, and a study placed there
    // keeps its slot.
    let pane = chart.series_pane(ids[1]).unwrap();
    let study = chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
        "Pane study",
        |bars| vec![crate::studies::IndicatorValue::Single(50.0); bars.len()],
    )));
    assert!(chart.move_series_to_pane(study, pane));
    frame(&ctx, &mut chart, 10.0, vec![]);
    for (step, (order, members)) in [(1, [1, 2]), (0, [2, 1])].into_iter().enumerate() {
        assert!(chart.set_series_order(ids[2], order));
        assert_eq!(chart.series_order(ids[2]), Some(order));
        assert_eq!(
            chart.pane_series(pane),
            [ids[members[0]], ids[members[1]], study]
        );
        chart.clear_selection();
        let time = 11.0 + step as f64;
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, time, vec![]);
        let paths: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path) if path.points.len() == data.len() => colors
                    .iter()
                    .position(|color| path.stroke.color == egui::epaint::ColorMode::Solid(*color))
                    .filter(|index| *index != 0)
                    .map(|index| (index, path.points[49])),
                _ => None,
            })
            .collect();
        let drawn: Vec<_> = paths.iter().map(|(index, _)| *index).collect();
        assert_eq!(drawn, members, "pane paint at step {step}");
        let pos = paths[1].1;
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
            Some(ChartElementId::Series(ids[members[1]])),
            "pane selection at step {step}"
        );
    }
    assert!(chart.move_series_to_pane(ids[2], PaneId::MAIN));
    assert_eq!(chart.series_order(ids[0]), Some(0));
    assert_eq!(chart.series_order(ids[2]), Some(1));
    assert!(chart.set_series_visible(ids[2], false));
    frame(&ctx, &mut chart, 20.0, vec![]);
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
        let original_scale = chart.series_price_scale(other).unwrap();
        let shared = chart
            .shared_price_scale(chart.series_scale(other).unwrap())
            .unwrap();
        assert_eq!(shared.range(), original_scale.range());
        assert_eq!(shared.options(), original_scale.options());
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

/// Saved axis layout changes columns without changing source units or manual bounds.
/// Restore still waits for the host's bars; an attached, visible source keeps its axis.
#[cfg(feature = "ui")]
#[test]
fn saved_pane_axes_preserve_independent_values_and_columns() {
    use crate::scales::CustomPriceFormatter;
    use std::sync::Arc;
    for own_pane in [false, true] {
        for side in ["Left", "Right"] {
            let ctx = egui::Context::default();
            let data = bars(&[100.0; 30]);
            let mut chart = Chart::new(BarData::from_bars(data));
            chart.config.show_volume = false;
            chart.config.show_plus_button = false;
            let pane = if own_pane {
                chart.add_pane()
            } else {
                PaneId::MAIN
            };
            let mut ids = Vec::new();
            for (key, values, bounds) in [
                (SeriesScale::Separate(7), [10.0, 20.0], [0.0, 40.0]),
                (SeriesScale::Separate(8), [1200.0, 2400.0], [1000.0, 3000.0]),
            ] {
                let id = chart.add_series(SeriesOptions {
                    scale: key,
                    ..Default::default()
                });
                chart.move_series_to_pane(id, pane);
                chart.set_data(id, bars(&values.repeat(15)));
                chart
                    .series_price_scale_mut(id)
                    .unwrap()
                    .set_range(bounds[0], bounds[1]);
                ids.push(id);
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let mut saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            saved["panes"][usize::from(own_pane)]["axes"] = serde_json::json!({
                "side": side, "hide_unused": true,
            });
            chart
                .restore(&serde_json::from_value(saved).unwrap())
                .unwrap();
            let mut restored = Vec::new();
            for (at, old) in ids.iter().enumerate() {
                let id = chart.restored_series_id(old.0).unwrap();
                let prefix = if at == 0 { "small" } else { "large" };
                chart.set_series_formatter(
                    id,
                    Some(Arc::new(CustomPriceFormatter::new(move |value| {
                        format!("{prefix}:{value:.0}")
                    }))),
                );
                chart.set_data(
                    id,
                    bars(
                        &if at == 0 {
                            [10.0, 20.0]
                        } else {
                            [1200.0, 2400.0]
                        }
                        .repeat(15),
                    ),
                );
                restored.push(id);
            }
            // Let automatic width measurement settle before comparing columns.
            for time in [0.1, 0.2, 0.3] {
                frame(&ctx, &mut chart, time, vec![]);
            }
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.4, vec![]);
            let plot = chart.get_rendered_price_rect();
            let mut column_x = Vec::new();
            for prefix in ["small:", "large:"] {
                let labels: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with(prefix)
                                && (text.pos.x < plot.left() || text.pos.x > plot.right()) =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!labels.is_empty(), "{own_pane} {side}: {prefix}");
                assert!(
                    labels
                        .iter()
                        .all(|text| (text.pos.x < plot.left()) == (side == "Left")),
                    "{own_pane} {side}"
                );
                // Right labels have a fixed leading inset. Left labels have a fixed trailing inset.
                column_x.push(if side == "Right" {
                    labels[0].pos.x
                } else {
                    labels[0].pos.x + labels[0].galley.size().x
                });
            }
            assert!(
                (column_x[0] - column_x[1]).abs() >= 1.0,
                "{own_pane} {side}: {column_x:?}"
            );
            for (at, id) in restored.iter().enumerate() {
                let map = chart.series_price_map(*id).unwrap();
                let (low, high, value, fraction) = if at == 0 {
                    (0.0, 40.0, 20.0, 0.5)
                } else {
                    (1000.0, 3000.0, 2400.0, 0.7)
                };
                assert_eq!((map.range().min, map.range().max), (low, high));
                let (top, bottom) = map.extent();
                // Linear interpolation: 20/40 = 1/2; (2400-1000)/(3000-1000) = 7/10.
                assert!(
                    (map.price_to_y(value) - (bottom - fraction * (bottom - top))).abs() < 0.001
                );
            }
        }
    }
}

/// Pane commands preserve membership and values, record one undo step, and retain the
/// unused-axis policy through hide/show, empty history, pane moves and serialized layouts.
#[test]
fn pane_axis_commands_enumerate_sources_and_restore_layout() {
    use crate::model::{AxisSide, PaneAxisOptions};
    use crate::widget::PaneScale;
    for placement in 0..4 {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        chart.config.show_volume = false;
        if placement == 3 {
            chart.set_series_scale(SeriesId::MAIN, SeriesScale::Separate(12));
        }
        let pane = if matches!(placement, 0 | 3) {
            PaneId::MAIN
        } else {
            chart.add_pane()
        };
        if placement == 2 {
            assert!(chart.move_series_to_pane(SeriesId::MAIN, pane));
        }
        let other = chart.add_pane();
        chart.set_preserve_empty_pane(other, true);
        let mut ids = Vec::new();
        for key in [
            SeriesScale::Left,
            SeriesScale::Left,
            SeriesScale::Separate(8),
            SeriesScale::Overlay(3),
        ] {
            let id = chart.add_series(SeriesOptions {
                scale: key,
                ..Default::default()
            });
            assert!(chart.move_series_to_pane(id, pane));
            chart.set_data(
                id,
                bars(
                    &[if key == SeriesScale::Separate(8) {
                        1000.0
                    } else {
                        10.0
                    }; 30],
                ),
            );
            ids.push(id);
        }
        let left = [
            PaneScale::Named(SeriesScale::Left),
            PaneScale::Named(SeriesScale::Separate(8)),
        ];
        assert_eq!(chart.pane_scales(pane, AxisSide::Left), left);
        // Listed axes lead to their scale; absent axes and independent studies have none.
        assert!(std::ptr::eq(
            chart.pane_scale(pane, left[0]).unwrap(),
            chart.series_price_scale(ids[0]).unwrap()
        ));
        assert!(
            chart
                .pane_scale(pane, PaneScale::Named(SeriesScale::Separate(99)))
                .is_none()
        );
        assert!(chart.pane_scale(pane, PaneScale::Study(ids[0])).is_none());
        assert!(chart.pane_scales(other, AxisSide::Right).is_empty());
        assert!(chart.pane_scales(PaneId(999), AxisSide::Left).is_empty());
        assert!(!chart.place_pane_scales(PaneId(999), AxisSide::Left));
        assert!(!chart.merge_pane_scales(PaneId(999), AxisSide::Right));
        let assignment = chart.series_scale(ids[0]);
        chart
            .series_price_scale_mut(ids[0])
            .unwrap()
            .set_range(0.0, 20.0);
        chart
            .series_price_scale_mut(ids[0])
            .unwrap()
            .set_price_precision(Some(4));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let initial = chart.pane_axis_options(pane).unwrap();
        for side in [AxisSide::Left, AxisSide::Right] {
            assert!(chart.user_action(|chart| chart.place_pane_scales(pane, side)));
            assert!(chart.pane_scales(pane, side).contains(&left[0]));
            assert!(
                chart
                    .pane_scales(
                        pane,
                        if side == AxisSide::Left {
                            AxisSide::Right
                        } else {
                            AxisSide::Left
                        }
                    )
                    .is_empty()
            );
            assert_eq!(
                chart.pane_axis_options(other),
                Some(PaneAxisOptions::default())
            );
            assert!(chart.undo());
            assert_eq!(chart.pane_axis_options(pane), Some(initial));
            assert!(chart.redo());
            let separate = chart.pane_axis_options(pane).unwrap();
            assert!(chart.user_action(|chart| chart.merge_pane_scales(pane, side)));
            let target = chart.series_scale(ids[0]).unwrap();
            assert_eq!(chart.pane_scales(pane, side), [PaneScale::Named(target)]);
            assert_eq!(
                chart.format_series_price(ids[0], 10.0).as_deref(),
                Some("10.0000")
            );
            assert_eq!(chart.bars(ids[0])[0].close, 10.0);
            assert_eq!(chart.bars(ids[2])[0].close, 1000.0);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let merged = chart.series_price_scale(ids[0]).unwrap();
            if placement == 1 {
                assert!(merged.is_manual());
                assert_eq!((merged.range().min, merged.range().max), (0.0, 20.0));
            } else {
                assert!(merged.range().contains(10.0) && merged.range().contains(1000.0));
            }
            assert!(chart.undo());
            assert_eq!(chart.pane_axis_options(pane), Some(separate));
            assert_eq!(chart.series_scale(ids[0]), assignment);
            let range = chart.series_price_scale(ids[0]).unwrap().range();
            assert_eq!((range.min, range.max), (0.0, 20.0));
            assert!(chart.redo());
            assert_eq!(chart.pane_scales(pane, side), [PaneScale::Named(target)]);
            assert!(chart.undo());
            assert!(chart.undo());
            assert_eq!(chart.pane_axis_options(pane), Some(initial));
        }
        // An independent study has its own axis identity in the main series' pane.
        if placement != 1 {
            let study = chart.add_indicator(Box::new(
                crate::studies::CustomIndicator::new("independent", |_| {
                    vec![crate::studies::IndicatorValue::Single(5.0); 30]
                })
                .with_scale(crate::studies::StudyScale::Independent),
            ));
            if placement == 2 {
                chart.move_series_to_pane(study, pane);
            }
            assert!(
                chart
                    .pane_scales(pane, AxisSide::Left)
                    .contains(&PaneScale::Study(study))
            );
            assert!(chart.user_action(|chart| chart.merge_pane_scales(pane, AxisSide::Right)));
            assert_eq!(chart.pane_scales(pane, AxisSide::Right).len(), 1);
            assert!(chart.indicators().find(|(id, _)| *id == study).is_some_and(
                |(_, study)| matches!(study.scale(), crate::studies::StudyScale::Shared(_))
            ));
            assert!(chart.undo());
            assert!(
                chart
                    .pane_scales(pane, AxisSide::Left)
                    .contains(&PaneScale::Study(study))
            );
            chart.remove_series(study);
        }
        chart.place_pane_scales(pane, AxisSide::Right);
        for id in &ids {
            chart.set_series_visible(*id, false);
        }
        if placement != 1 {
            chart.set_series_visible(SeriesId::MAIN, false);
        }
        assert!(chart.pane_scales(pane, AxisSide::Right).is_empty());
        #[cfg(feature = "ui")]
        if placement != 1 {
            chart.place_pane_scales(pane, AxisSide::Left);
            let settings = crate::widget::settings::ChartSettingsState::from_chart(&chart);
            assert!(
                settings.scales_and_lines.price_scale_left,
                "hidden sources retain their configured side"
            );
            let axis_visibility = (chart.config.show_left_axis, chart.config.show_right_axis);
            settings.apply_to_chart(&mut chart).unwrap();
            assert_eq!(
                (chart.config.show_left_axis, chart.config.show_right_axis),
                axis_visibility,
                "pane placement must not move another pane's axes"
            );
            assert_eq!(
                chart.pane_axis_options(pane).unwrap().side,
                Some(AxisSide::Left)
            );
            chart.place_pane_scales(pane, AxisSide::Right);
        }
        let options = chart.pane_axis_options(pane).unwrap();
        assert!(options.hide_unused);
        chart.set_pane_axis_options(
            pane,
            PaneAxisOptions {
                hide_unused: false,
                ..options
            },
        );
        assert!(!chart.pane_scales(pane, AxisSide::Right).is_empty());
        #[cfg(feature = "ui")]
        {
            let main_assignment = chart.series_scale(SeriesId::MAIN);
            let saved = chart
                .snapshot()
                .unwrap_or_else(|error| panic!("placement {placement}: {error}"));
            let saved = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
            chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
            chart.restore(&saved).unwrap();
            assert_eq!(chart.series_scale(SeriesId::MAIN), main_assignment);
            for id in &mut ids {
                *id = chart.restored_series_id(id.0).unwrap();
            }
        }
        let pane = chart.series_pane(ids[0]).unwrap();
        assert!(!chart.pane_axis_options(pane).unwrap().hide_unused);
        chart.set_pane_axis_options(pane, options);
        assert!(chart.pane_scales(pane, AxisSide::Right).is_empty());
        chart.set_data(ids[0], Vec::new());
        assert!(chart.set_series_visible(ids[0], true));
        assert_eq!(chart.pane_scales(pane, AxisSide::Right), [left[0]]);
        chart.set_data(ids[0], bars(&[10.0; 30]));
        assert_eq!(chart.pane_scales(pane, AxisSide::Right), [left[0]]);
        assert!(chart.move_series_to_pane(ids[0], other));
        assert!(chart.pane_scales(pane, AxisSide::Right).is_empty());
        assert_eq!(chart.pane_scales(other, AxisSide::Left), [left[0]]);
        assert!(chart.move_series_to_pane(ids[0], pane));
        assert_eq!(chart.pane_scales(pane, AxisSide::Right), [left[0]]);
    }

    #[cfg(feature = "ui")]
    {
        // A study-only axis has no saved bar-series record to carry its options or range.
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        chart.set_series_scale(SeriesId::MAIN, SeriesScale::Overlay(40));
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
        let key = SeriesScale::Separate(9);
        assert!(
            chart
                .indicator_mut(study)
                .unwrap()
                .set_scale(crate::studies::StudyScale::Shared(key))
        );
        // Hosts can edit a study and save before a frame creates its named scale.
        assert!(
            chart
                .pane_scale(PaneId::MAIN, PaneScale::Named(key))
                .is_none()
        );
        let pending = chart.snapshot().unwrap();
        let mut restored = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        restored.restore(&pending).unwrap();
        assert_eq!(
            restored.pane_scales(PaneId::MAIN, AxisSide::Left),
            [PaneScale::Named(key)]
        );
        let scale = chart
            .pane_scale_mut(PaneId::MAIN, PaneScale::Named(key))
            .unwrap();
        scale.set_range(80.0, 120.0);
        scale.set_price_precision(Some(3));
        scale.options_mut().invert_scale = true;
        assert!(chart.merge_pane_scales(PaneId::MAIN, AxisSide::Right));
        let saved =
            serde_json::from_str(&serde_json::to_string(&chart.snapshot().unwrap()).unwrap())
                .unwrap();
        let mut restored = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        restored.restore(&saved).unwrap();
        let scale = restored.pane_price_scale(PaneId::MAIN, key);
        assert!(scale.is_manual() && scale.options().invert_scale);
        assert_eq!((scale.range().min, scale.range().max), (80.0, 120.0));
        assert_eq!(
            scale.map(Default::default(), 0.0, 1.0).price_precision(),
            Some(3)
        );
        assert_eq!(
            restored.pane_scales(PaneId::MAIN, AxisSide::Right),
            [PaneScale::Named(key)]
        );
    }
}

/// A margin-only request (prices left out, as the hook allows) while no bar is in view: the
/// main scale has nothing to fit, so it keeps the range it had, as `PriceScale::autoscale`
/// documents for bounds that are not finite. With the default 10% margins and 40 points at each
/// edge of a plot h points tall, bars from 10 to 20 fit a span of 10 / (0.8 - 80 / h), with
/// 0.1 + 40 / h of it beyond each price.
#[test]
fn margin_only_request_keeps_the_main_range_with_no_bar_in_view() {
    use crate::chart::primitives::Primitive;
    use crate::model::{AutoscaleInfo, AutoscaleMargins};

    struct Header;
    impl Primitive for Header {
        fn autoscale_info(&self, _: &[Bar], _: std::ops::Range<usize>) -> Option<AutoscaleInfo> {
            Some(AutoscaleInfo {
                price_range: (f64::NAN, f64::NAN),
                margins: AutoscaleMargins {
                    top: 40.0,
                    bottom: 40.0,
                },
            })
        }
    }
    let ctx = egui::Context::default();
    let mut chart = crate::widget::test_frames::chart(&[10.0], &[20.0]);
    chart.attach_primitive(SeriesId::MAIN, Header).unwrap();
    frame(&ctx, &mut chart, 0.0, vec![]);
    frame(&ctx, &mut chart, 0.5, vec![]);
    let map = chart.series_price_map(SeriesId::MAIN).unwrap();
    let height = f64::from(map.extent().1 - map.extent().0);
    let beyond = 10.0 / (0.8 - 80.0 / height) * (0.1 + 40.0 / height);
    let fitted = chart.state().price_scale().range();
    assert!(
        (fitted.min - (10.0 - beyond)).abs() < 1e-8 && (fitted.max - (20.0 + beyond)).abs() < 1e-8,
        "observed {fitted:?}"
    );
    // Scroll into the whitespace after the last bar; no bar remains in view.
    chart.scroll_to_position(300.0, false);
    for step in 1..=4 {
        frame(&ctx, &mut chart, step as f64, vec![]);
        let (start, end) = chart.state().visible_range();
        assert_eq!(start, end, "no bar in view");
        assert_eq!(chart.state().price_scale().range(), fitted, "frame {step}");
    }
    chart.scroll_to_position(0.0, false);
    frame(&ctx, &mut chart, 5.0, vec![]);
    assert_eq!(chart.state().price_scale().range(), fitted, "back in view");
}

/// A request too large for the plot keeps the fit the plot has without it, as the
/// `AutoscaleMargins` docs promise, and that fit makes room for markers. With zero fractional
/// margins a circle of size 1 placed above a price reaches 8 + 8 + 2 = 18 points above it (offset,
/// half size and stroke), so the fit puts the top of the fitted prices 18 points below the top of
/// the plot. That is the high, 20, in the main series' pane. An added series' pane first pads its
/// span of 10 by 10% at each side, so there it is 21, and the padding alone leaves 20 only
/// 150 / 12 = 12.5 points below the top of this pane: too little for the marker.
#[test]
fn oversized_margin_request_keeps_the_marker_fit() {
    use crate::model::{AutoscaleInfo, AutoscaleMargins, AutoscaleProvider, MarkerShape};

    for (placement, ceiling) in [("main", 20.0), ("added", 21.0)] {
        for request in [None, Some(1e6_f32)] {
            let ctx = egui::Context::default();
            let mut chart = crate::widget::test_frames::chart(&[10.0], &[10.0, 20.0]);
            chart.config.show_volume = false;
            let last = chart.bars(SeriesId::MAIN)[99].time;
            let target = if placement == "added" {
                let id = chart.add_series(SeriesOptions::default());
                let data = chart.bars(SeriesId::MAIN).to_vec();
                chart.set_data(id, data);
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(id, pane));
                assert!(chart.set_pane_height(pane, 150.0));
                id
            } else {
                SeriesId::MAIN
            };
            chart
                .series_price_scale_mut(target)
                .unwrap()
                .options_mut()
                .scale_margins = crate::scales::PriceScaleMargins {
                top: 0.0,
                bottom: 0.0,
            };
            chart.set_markers(
                target,
                vec![
                    Marker::new(last)
                        .at_price(20.0, MarkerPos::AtPriceTop)
                        .with_shape(MarkerShape::Circle),
                ],
            );
            if let Some(pixels) = request {
                chart.series_style_mut(target).unwrap().autoscale =
                    Some(AutoscaleProvider::with_margins(move |_, default| {
                        Some(AutoscaleInfo {
                            price_range: default?,
                            margins: AutoscaleMargins {
                                top: pixels,
                                bottom: pixels,
                            },
                        })
                    }));
            }
            for step in 0..3 {
                frame(&ctx, &mut chart, step as f64, vec![]);
            }
            let map = chart.series_price_map(target).unwrap();
            let gap = map.price_to_y(ceiling) - map.extent().0;
            assert!(
                (gap - 18.0).abs() < 0.01,
                "{placement}, request {request:?}: observed {gap}, extent {:?}",
                map.extent()
            );
        }
    }
}

/// A margin-only request holds on the scale its contributor shares while that scale is fitted.
/// The main series' bars fit the main scale here, so with the default 10% margins a request of
/// 40 points at the top puts the high, 20, 0.1 h + 40 below the top of the plot,
/// whether the primitive belongs to the main series or to an added series on the same scale
/// that has no bar yet.
#[test]
fn margin_only_request_of_an_added_series_reaches_the_main_scale() {
    use crate::chart::primitives::Primitive;
    use crate::model::{AutoscaleInfo, AutoscaleMargins};

    struct Header;
    impl Primitive for Header {
        fn autoscale_info(&self, _: &[Bar], _: std::ops::Range<usize>) -> Option<AutoscaleInfo> {
            Some(AutoscaleInfo {
                price_range: (f64::NAN, f64::NAN),
                margins: AutoscaleMargins {
                    top: 40.0,
                    bottom: 0.0,
                },
            })
        }
    }
    let mut gaps = Vec::new();
    for owner in ["main", "added"] {
        let ctx = egui::Context::default();
        let mut chart = crate::widget::test_frames::chart(&[10.0], &[20.0]);
        chart.config.show_volume = false;
        let added = chart.add_series(SeriesOptions::default());
        let target = if owner == "main" {
            SeriesId::MAIN
        } else {
            added
        };
        chart.attach_primitive(target, Header).unwrap();
        for step in 0..3 {
            frame(&ctx, &mut chart, step as f64, vec![]);
        }
        let map = chart.series_price_map(SeriesId::MAIN).unwrap();
        let (top, bottom) = map.extent();
        gaps.push(map.price_to_y(20.0) - top - (0.1 * (bottom - top) + 40.0));
    }
    assert!(gaps.iter().all(|gap| gap.abs() < 0.01), "observed {gaps:?}");
}

/// The main axis reports the width of its placed side, a chart with both price axes off stays
/// without them after a placement, and own-scale volume never takes an axis of the initial pane.
#[test]
fn placed_axes_report_their_side_and_respect_hidden_chart_axes() {
    use crate::model::AxisSide;
    use crate::scales::PriceScaleId;
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
    chart.config.show_volume = false;
    assert!(chart.place_pane_scales(PaneId::MAIN, AxisSide::Left));
    frame(&ctx, &mut chart, 0.0, vec![]);
    assert!(chart.price_axis_width(PriceScaleId::Left) > 0.0);
    assert_eq!(chart.price_axis_width(PriceScaleId::Right), 0.0);
    chart.config.show_left_axis = false;
    chart.config.show_right_axis = false;
    assert!(chart.pane_scales(PaneId::MAIN, AxisSide::Left).is_empty());

    #[cfg(feature = "studies")]
    {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
        chart.config.show_volume = false;
        chart.add_indicator(Box::new(crate::studies::Volume::new(20)));
        let other = chart.add_pane();
        assert!(chart.move_series_to_pane(SeriesId::MAIN, other));
        assert!(chart.pane_scales(PaneId::MAIN, AxisSide::Right).is_empty());
        frame(&ctx, &mut chart, 0.1, vec![]);
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
        let rows = chart.pane_layout.panes[0].rect.y_range();
        let plot = chart.get_rendered_price_rect();
        assert!(
            !painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.pos.x > plot.right() && rows.contains(text.pos.y)
            )),
            "own-scale volume paints no axis labels"
        );
    }
}

/// A locked axis keeps its units per bar, span * spacing / height, whichever axis shows it, and
/// an unlocked scale keeps its range. The main axis is the control: a range of 10 inside margins
/// of 10% spans 12.5, and twice the spacing at the same height halves it. A scale of a secondary
/// pane has no margins and fits 10% of padding, so it spans 12.
#[test]
fn named_axis_lock_keeps_units_per_bar() {
    use crate::widget::PaneScale;
    use PriceScaleMode::{Normal, Percentage};
    use SeriesScale::{Left, Right, Separate};
    for (case, secondary, key, mode, hand) in [
        ("left", false, Left, Normal, Some(12.5)),
        ("additional", false, Separate(1), Normal, Some(12.5)),
        ("left percent", false, Left, Percentage, None),
        ("pane left", true, Left, Normal, Some(12.0)),
        ("pane right", true, Right, Normal, Some(12.0)),
        ("pane left percent", true, Left, Percentage, None),
    ] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0].repeat(100))));
        chart.config.show_volume = false;
        // More bars than fit: zooming moves the first visible value, a percent scale's reference.
        let prices: Vec<f64> = match mode {
            Normal => [10.0, 20.0].repeat(100),
            _ => (0..200).map(|at| 10.0 + f64::from(at)).collect(),
        };
        let other = chart.add_pane();
        let pane = if secondary { other } else { PaneId::MAIN };
        for scale in [key, Separate(9)] {
            let id = chart.add_series(SeriesOptions {
                scale,
                ..Default::default()
            });
            chart.set_data(id, bars(&prices));
            assert!(chart.move_series_to_pane(id, pane), "{case}");
        }
        let axis = PaneScale::Named(key);
        chart.pane_scale_mut(pane, axis).unwrap().options_mut().mode = mode;
        let unlocked = PaneScale::Named(Separate(9));
        let fixed = crate::scales::PriceRange::new(0.0, 100.0);
        chart
            .pane_scale_mut(pane, unlocked)
            .unwrap()
            .set_range(fixed.min, fixed.max);
        frame(&ctx, &mut chart, 0.0, vec![]);

        let height = |chart: &Chart| {
            if secondary {
                chart.pane_height(pane).unwrap()
            } else {
                chart.get_rendered_price_rect().height()
            }
        };
        let (spacing, offset) = chart.get_time_scale_state();
        let main_height = chart.get_rendered_price_rect().height();
        chart
            .price_scale_mut()
            .set_bar_ratio_locked(true, spacing, main_height);
        let before = height(&chart);
        chart
            .pane_scale_mut(pane, axis)
            .unwrap()
            .set_bar_ratio_locked(true, spacing, before);
        let range = |chart: &Chart| chart.pane_scale(pane, axis).unwrap().range();
        let span = |range: crate::scales::PriceRange| range.max - range.min;
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-4 * b.abs().max(1.0);
        let locked = range(&chart);
        if let Some(hand) = hand {
            assert!(near(span(locked), hand), "{case}: {locked:?}");
        }
        assert!(
            near(span(chart.state.price_scale().range()), 12.5),
            "{case}"
        );

        frame(&ctx, &mut chart, 0.1, vec![]);
        assert!(near(span(range(&chart)), span(locked)), "{case}: a jump");

        chart.apply_synced_time_scale(spacing * 2.0, offset);
        frame(&ctx, &mut chart, 0.2, vec![]);
        // Narrower axis labels widen the plot, which moves the spacing of a percent scale a little.
        let (zoomed, _) = chart.get_time_scale_state();
        assert!(zoomed > spacing * 1.5, "{case}: {spacing} -> {zoomed}");
        assert_eq!(height(&chart), before, "{case}");
        let zoom = f64::from(spacing / zoomed);
        let halved = range(&chart);
        assert!(
            near(span(halved), span(locked) * zoom),
            "{case}: {halved:?}"
        );
        if let Some(hand) = hand {
            assert!(near(span(halved), hand / 2.0), "{case}: {halved:?}");
        }
        let centre = |range: crate::scales::PriceRange| (range.min + range.max) / 2.0;
        assert!(near(centre(halved), centre(locked)), "{case}: {halved:?}");
        assert!(
            near(span(chart.state.price_scale().range()), 12.5 * zoom),
            "{case}"
        );

        chart.set_pane_stretch_factor(other, 3.0);
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_ne!(height(&chart), before, "{case}: the pane was resized");
        assert!(
            near(
                span(range(&chart)) * f64::from(chart.get_time_scale_state().0 / height(&chart)),
                span(locked) * f64::from(spacing / before),
            ),
            "{case}: {:?}",
            range(&chart)
        );
        assert_eq!(
            chart.pane_scale(pane, unlocked).unwrap().range(),
            fixed,
            "{case}"
        );
    }
}

/// A study assigned to a named scale in a secondary pane has no scale before the frame that
/// creates it, as documented and as in the initial pane, rather than the pane's right scale.
#[test]
fn unpainted_study_axis_in_a_secondary_pane_has_no_scale() {
    use crate::model::AxisSide;
    use crate::widget::PaneScale;
    let ctx = egui::Context::default();
    let mut chart = Chart::new(BarData::from_bars(bars(&[100.0; 30])));
    chart.config.show_volume = false;
    let study = chart.add_indicator(Box::new(
        crate::studies::CustomIndicator::new("lower", |_| {
            vec![crate::studies::IndicatorValue::Single(5.0); 30]
        })
        .with_overlay(false),
    ));
    let pane = chart.series_pane(study).unwrap();
    assert_ne!(pane, PaneId::MAIN);
    assert!(
        chart.set_indicator_scale(study, crate::studies::StudyScale::Shared(SeriesScale::Left))
    );
    let left = PaneScale::Named(SeriesScale::Left);
    assert_eq!(chart.pane_scales(pane, AxisSide::Left), [left]);
    assert!(chart.pane_scale(pane, left).is_none());

    frame(&ctx, &mut chart, 0.0, vec![]);
    let right = chart.pane_price_scale(pane, SeriesScale::Right);
    assert!(!std::ptr::eq(chart.pane_scale(pane, left).unwrap(), right));
}

/// An added high-low series paints every bar in its `color`, so its last-value tag takes that
/// colour too, as the main high-low series' last-price label takes its bar colour. An added
/// candles series still tags its last value by the bar's direction.
#[test]
fn added_high_low_series_tags_its_last_value_in_its_bar_colour() {
    let start = "2024-01-01T00:00:00Z"
        .parse::<chrono::DateTime<Utc>>()
        .unwrap();
    // A falling bar, then a rising last bar.
    let data: Vec<Bar> = [(13.0, 17.0, 8.5, 11.0), (10.0, 18.0, 8.0, 14.0)]
        .iter()
        .enumerate()
        .map(|(i, &(o, h, l, c))| Bar::new(start + TimeDelta::minutes(i as i64), o, h, l, c, 1.0))
        .collect();
    let (color, up, down) = (
        Color32::from_rgb(1, 2, 101),
        Color32::from_rgb(1, 100, 2),
        Color32::from_rgb(101, 2, 3),
    );
    for (chart_type, tagged) in [(ChartType::HighLow, color), (ChartType::Candles, up)] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(data.clone()));
        let id = chart.add_series(SeriesOptions {
            chart_type,
            color,
            up_color: up,
            down_color: down,
            ..Default::default()
        });
        chart.set_data(id, data.clone());
        frame(&ctx, &mut chart, 0.0, vec![]);
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let tag = chart
            .series_last_value(id, PriceLineSource::LastBar)
            .map(|value| value.color);
        assert_eq!(tag, Some(tagged), "{chart_type:?} last-value tag colour");
        if chart_type == ChartType::HighLow {
            let plot = chart.get_rendered_price_rect();
            let bars_in = |fill: Color32| {
                painted
                    .iter()
                    .filter(|shape| {
                        matches!(shape, egui::Shape::Rect(r)
                            if r.fill == fill && plot.contains(r.rect.center()))
                    })
                    .count()
            };
            assert_eq!((bars_in(color), bars_in(up)), (2, 0), "high-low bars");
        }
    }
}
