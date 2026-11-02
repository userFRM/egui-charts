use super::*;
use crate::model::{BarData, LineStyle};
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
            crate::chart::renderers::study_mapping(study, &chart.get_chart_mapping()).price
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

/// Bars can be popped from, and looked up in, any bar series; live changes are reported
/// once per series in a row; the draw order can be changed.
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
        (minute(2), MismatchDirection::Exact, Some(1)),
        (minute(1), MismatchDirection::Exact, None),
        (minute(1), MismatchDirection::NearestLeft, Some(0)),
        (minute(1), MismatchDirection::NearestRight, Some(1)),
        (minute(-1), MismatchDirection::NearestLeft, None),
        (minute(5), MismatchDirection::NearestRight, None),
    ] {
        assert_eq!(
            chart.bar_index_at(id, time, direction),
            expected,
            "{time} {direction:?}"
        );
    }
    for ((left, right), bars, bars_before, bars_after) in [
        ((1.0, 3.0), 1..2, 1.0, 1.0),
        // Two bars of whitespace before the first bar.
        ((-2.0, 2.4), 0..2, -2.0, 1.0),
        // Five bars of whitespace after the last bar, at 4.
        ((2.6, 9.0), 2..3, 2.0, -5.0),
        ((0.6, 1.4), 1..1, 1.0, 2.0),
    ] {
        assert_eq!(
            chart.bars_in_logical_range(id, LogicalRange::new(left, right)),
            Some(BarsInRange {
                bars,
                bars_before,
                bars_after
            }),
            "{left}..{right}"
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
    assert!(chart.set_series_order(other, 0));
    assert_eq!(
        chart.series().map(|(id, _)| id).collect::<Vec<_>>(),
        [other, id]
    );
    assert_eq!(chart.series_order(id), Some(1));
}
