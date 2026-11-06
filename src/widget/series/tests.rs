use super::*;
#[cfg(feature = "drawings")]
use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
use crate::studies::{Indicator, IndicatorValue, SMA};
#[cfg(all(test, feature = "ui"))]
use crate::ui::top_toolbar::SettingsMenu;
use crate::widget::test_frames::frame;
use chrono::{DateTime, TimeDelta, Utc};
use std::time::Duration;

/// Bars span 100..110 and the overlay reaches 120: 10% of the pane above and below adds an
/// eighth of the span on each side, 97.5..122.5. Every line of a study counts and non-finite
/// values do not; without eligible values, or with a volume profile, the bars alone give
/// 98.75..111.25. Relative modes use close 110.
#[test]
fn autoscale_includes_only_visible_overlay_values() {
    use crate::scales::PriceScaleMode;
    use crate::studies::{CustomIndicator, VolProfileVisible};
    use crate::widget::test_frames::chart;

    for (mode, fitted, bars_only) in [
        (PriceScaleMode::Normal, (97.5, 122.5), (98.75, 111.25)),
        (
            PriceScaleMode::Percentage,
            (-125.0 / 11.0, 125.0 / 11.0),
            (-112.5 / 11.0, 12.5 / 11.0),
        ),
        (
            PriceScaleMode::IndexedTo100,
            (975.0 / 11.0, 1225.0 / 11.0),
            (987.5 / 11.0, 1112.5 / 11.0),
        ),
    ] {
        for case in [
            "single",
            "multiple",
            "excluded",
            "hidden",
            "plot_hidden",
            "interval_hidden",
            "pane",
            "short",
            "profile",
            "independent",
            "no_scale",
            "displaced_future",
            "main_hidden",
            "price_only",
            "price_only_shared",
            "price_only_shared_other",
            "price_only_restored",
            "price_only_excluded",
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config_mut().right_axis_scale_mode = mode;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.set_visible_bars(20);
            chart.set_start_idx(40);
            frame(&ctx, &mut chart, 0.02, Vec::new());
            let (start, end) = chart.state.visible_range();
            assert!(start > 0 && end < 100);
            if case == "displaced_future" {
                chart.set_start_idx(85);
                frame(&ctx, &mut chart, 0.025, Vec::new());
                chart.state.time_scale_mut().set_right_offset(8.0);
                assert!(
                    chart
                        .state
                        .time_scale()
                        .visible_logical_range()
                        .to_strict_range()
                        .1
                        > 104
                );
            }
            let study = CustomIndicator::new("Overlay", move |bars| {
                let len = if case == "short" { start } else { bars.len() };
                (0..len)
                    .map(|i| {
                        if case == "displaced_future" {
                            if i + 1 == bars.len() {
                                IndicatorValue::Single(120.0)
                            } else {
                                IndicatorValue::None
                            }
                        } else if i < start {
                            IndicatorValue::Single(-10_000.0)
                        } else if i >= end {
                            IndicatorValue::Single(10_000.0)
                        } else if case == "multiple" {
                            IndicatorValue::Multiple(vec![
                                105.0,
                                120.0,
                                f64::NAN,
                                f64::INFINITY,
                                f64::NEG_INFINITY,
                            ])
                        } else {
                            IndicatorValue::Single(120.0)
                        }
                    })
                    .collect()
            })
            .with_line_count(if case == "multiple" { 5 } else { 1 })
            .with_overlay(case != "pane")
            .with_offsets(vec![if case == "displaced_future" { 5 } else { 0 }])
            .with_scale(match case {
                "independent" => crate::studies::StudyScale::Independent,
                "no_scale" => crate::studies::StudyScale::None,
                "pane" => crate::studies::StudyScale::Independent,
                "price_only_shared" => {
                    crate::studies::StudyScale::Shared(crate::model::SeriesScale::Right)
                }
                "price_only_shared_other" => {
                    crate::studies::StudyScale::Shared(crate::model::SeriesScale::Left)
                }
                _ => crate::studies::StudyScale::Main,
            });
            let study: Box<dyn Indicator> = if case == "profile" {
                Box::new(VolProfileVisible::default())
            } else {
                Box::new(study)
            };
            if case == "interval_hidden" {
                chart.set_timeframe(crate::model::Timeframe::Min1);
            }
            let id = chart.add_indicator(study);
            if matches!(case, "plot_hidden" | "interval_hidden") {
                let study = chart.indicator_mut(id).unwrap();
                let mut settings = study.settings();
                if case == "plot_hidden" {
                    settings.plots[0].visible = false;
                } else {
                    settings
                        .hidden_intervals
                        .push(crate::model::Timeframe::Min1);
                }
                study.apply_settings(&[], settings).unwrap();
            }
            chart.set_indicator_visible(id, case != "hidden");
            assert!(
                chart.set_indicator_autoscale(
                    id,
                    !matches!(case, "excluded" | "price_only_excluded")
                )
            );
            chart.set_main_series_visible(case != "main_hidden");
            if case.starts_with("price_only") {
                #[cfg(feature = "ui")]
                {
                    let mut menu = SettingsMenu::new();
                    menu.settings.scale_price_chart_only = true;
                    chart.update_config(menu.apply_to_config(chart.config().clone()));
                }
                #[cfg(not(feature = "ui"))]
                {
                    chart.config_mut().scale_price_chart_only = true;
                }
                if matches!(case, "price_only_restored" | "price_only_excluded") {
                    frame(&ctx, &mut chart, 0.03, Vec::new());
                    chart.config_mut().scale_price_chart_only = false;
                }
                assert!(
                    chart
                        .indicators()
                        .find(|(study, _)| *study == id)
                        .unwrap()
                        .1
                        .is_visible()
                );
            }
            frame(&ctx, &mut chart, 0.04, Vec::new());
            if case == "price_only_shared_other" {
                let study = chart
                    .indicators()
                    .find(|(study, _)| *study == id)
                    .unwrap()
                    .1;
                let map =
                    crate::chart::rendering::study_mapping(study, &chart.get_chart_mapping()).price;
                assert!(map.range().contains(map.transform().to_scale(120.0)));
            }
            let expected = if case == "main_hidden" {
                let value = crate::scales::PriceTransform { mode, base: 110.0 }.to_scale(120.0);
                (value - value.abs() * 0.00125, value + value.abs() * 0.00125)
            } else if matches!(
                case,
                "single" | "multiple" | "displaced_future" | "price_only_restored"
            ) {
                fitted
            } else {
                bars_only
            };
            let range = chart.price_scale_mut().range();
            assert!(
                (range.min - expected.0).abs() < 1e-10 && (range.max - expected.1).abs() < 1e-10,
                "{mode:?} {case}: {range:?}, expected {expected:?}"
            );
        }
    }
}

/// A flat bar at `close`, stamped `seconds` after 10:00.
fn bar(seconds: i64, close: f64) -> Bar {
    let t = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::seconds(seconds);
    Bar::new(t, close, close, close, close, 1.0)
}

/// Streaming keeps the configured live offset, or the same scrolled-back element.
#[test]
fn appended_bars_keep_the_view() {
    use crate::model::{ChartType, ChartTypeParams};
    for (name, kind, offset, limit, expected) in [
        (
            "candles at the live edge",
            ChartType::Candles,
            None,
            None,
            [20.0; 3],
        ),
        (
            "renko at the live edge",
            ChartType::Renko,
            None,
            None,
            [20.0; 3],
        ),
        (
            "limited candles at the live edge",
            ChartType::Candles,
            None,
            Some(100),
            [20.0; 3],
        ),
        (
            "limited candles scrolled back",
            ChartType::Candles,
            Some(-30.0),
            Some(100),
            [-31.0, -32.0, -33.0],
        ),
        (
            "limited renko scrolled back",
            ChartType::Renko,
            Some(-30.0),
            Some(100),
            [-32.0, -34.0, -36.0],
        ),
        (
            "renko scrolled back",
            ChartType::Renko,
            Some(-30.0),
            None,
            [-32.0, -34.0, -36.0],
        ),
    ] {
        let options = crate::config::ChartOptions {
            time_scale: crate::config::TimeScaleOptions {
                right_offset: 20.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // Every bar after the first adds two bricks of 10. Brick 167 stays at the right edge.
        let mut chart = Chart::new(BarData::from_bars(
            (0..100)
                .map(|i| bar(i * 60, 120.0 + 20.0 * i as f64))
                .collect(),
        ))
        .with_chart_options(options);
        chart.set_chart_type(kind);
        chart.set_chart_type_params(ChartTypeParams {
            renko_brick_size: 10.0,
            ..Default::default()
        });
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.set_bar_limit(SeriesId::MAIN, limit.and_then(NonZeroUsize::new));
        if let Some(offset) = offset {
            chart.state.time_scale_mut().set_right_offset(offset);
        }
        let mut got = Vec::new();
        for i in 100..103 {
            chart.update(SeriesId::MAIN, bar(i * 60, 120.0 + 20.0 * i as f64));
            frame(&ctx, &mut chart, i as f64, Vec::new());
            got.push(chart.state.time_scale().right_offset());
        }
        assert_eq!(got, expected, "{name}");
        if limit.is_some() {
            // The first new bar overflows the limit of 100 and evicts down to
            // 100 - 100 / 8 = 88; the other two are kept.
            assert_eq!(
                chart.bars(SeriesId::MAIN).len(),
                90,
                "{name}: retained bars"
            );
        }
    }
}

/// Only filling a whitespace tail may use the whitespace shift policy. Ordinary updates keep
/// the offset and pending start; appends still follow the new-bar policy. Without following,
/// one appended bar subtracts one from the offset to keep the same time at the right edge,
/// also when a window of the same length moves forward by that bar.
#[test]
fn bar_updates_obey_live_edge_policies() {
    for (replace, rolling) in [(false, false), (true, false), (true, true)] {
        for shift in [false, true] {
            for allow_whitespace in [false, true] {
                for offset in [19.0, -1.5, -2.0, -30.0] {
                    for (name, old_whitespace, new_whitespace, close, append) in [
                        ("close rises", false, false, 11.0, false),
                        ("close falls", false, false, 9.0, false),
                        ("close unchanged", false, false, 10.0, false),
                        ("whitespace unchanged", true, true, 10.0, false),
                        ("whitespace filled", true, false, 11.0, false),
                        ("value removed", false, true, 10.0, false),
                        ("value appended", false, false, 11.0, true),
                        ("whitespace appended", true, true, 10.0, true),
                    ] {
                        let options = crate::config::ChartOptions {
                            time_scale: crate::config::TimeScaleOptions {
                                right_offset: 20.0,
                                shift_visible_range_on_new_bar: shift,
                                allow_shift_visible_range_on_whitespace_replacement:
                                    allow_whitespace,
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        let mut bars: Vec<_> = (0..100).map(|i| bar(i * 60, 10.0)).collect();
                        if old_whitespace {
                            bars[99] = Bar::whitespace(bars[99].time);
                        }
                        let mut chart = Chart::new(BarData::from_bars(bars.clone()))
                            .with_chart_options(options);
                        chart.scroll_to_position(offset, false);
                        chart.set_start_idx(40);
                        let mut incoming = bar(if append { 6000 } else { 5940 }, close);
                        if new_whitespace {
                            incoming = Bar::whitespace(incoming.time);
                        }
                        if replace {
                            if append {
                                if rolling {
                                    bars.remove(0);
                                }
                                bars.push(incoming);
                            } else {
                                bars[99] = incoming;
                            }
                            chart.set_data(SeriesId::MAIN, bars);
                        } else {
                            chart.update(SeriesId::MAIN, incoming);
                        }
                        let follow = shift
                            && offset >= -1.5
                            && (append || old_whitespace && !new_whitespace && allow_whitespace);
                        let expected = if follow {
                            20.0
                        } else if append {
                            offset - 1.0
                        } else {
                            offset
                        };
                        assert_eq!(
                            chart.state.time_scale().right_offset(),
                            expected,
                            "{name}: replace {replace}, rolling {rolling}, shift {shift}, whitespace {allow_whitespace}, offset {offset}"
                        );
                        assert_eq!(
                            chart.pending_start_idx,
                            (!follow).then_some(40),
                            "{name}: pending start"
                        );
                    }
                }
            }
        }
    }
    // Replacements that bring a newer last bar without growing the series, a frame after each.
    // The right offset stays at its default of 2.5, which a fixed right edge limits to 0.
    let window = |start: i64, len: i64| -> Vec<Bar> {
        (start..start + len).map(|i| bar(i * 60, 10.0)).collect()
    };
    for (name, fix_right_edge, limit, replacements, expected) in [
        (
            "rolling window",
            false,
            None,
            (1..=5).map(|start| window(start, 100)).collect::<Vec<_>>(),
            2.5,
        ),
        (
            "rolling window, fixed right edge",
            true,
            None,
            (1..=5).map(|start| window(start, 100)).collect(),
            0.0,
        ),
        (
            "bar limit fed the whole history",
            false,
            NonZeroUsize::new(100),
            (101..=105).map(|len| window(0, len)).collect(),
            2.5,
        ),
        (
            "window replaced by a later window",
            false,
            None,
            vec![window(150, 100)],
            2.5,
        ),
    ] {
        let options = crate::config::ChartOptions {
            time_scale: crate::config::TimeScaleOptions {
                fix_right_edge,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut chart = Chart::new(BarData::from_bars(window(0, 100))).with_chart_options(options);
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.set_bar_limit(SeriesId::MAIN, limit);
        for (step, bars) in replacements.into_iter().enumerate() {
            chart.set_data(SeriesId::MAIN, bars);
            frame(&ctx, &mut chart, 1.0 + step as f64, Vec::new());
            assert_eq!(
                chart.state.time_scale().right_offset(),
                expected,
                "{name}: replacement {step}"
            );
        }
    }
}

/// Limits and explicit removal operate on retained replay history and independent series.
/// A limit of 8 evicts down to 8 - 8 / 8 = 7 bars whenever a series holds more:
/// `set_bar_limit` and `set_data` keep the newest 7 of 9 bars, the first update fills the
/// limit again and the second evicts down to the 7 bars from 10:04 to 10:10.
#[test]
fn bar_limits_cover_replay_and_added_series() {
    use crate::widget::SeriesOptions;
    let initial: Vec<_> = (0..9).map(|i| bar(i * 60, i as f64 + 1.0)).collect();
    let kept: Vec<_> = (4..11).map(|i| bar(i * 60, i as f64 + 1.0)).collect();
    for replay in [false, true] {
        for main in [false, true] {
            let mut chart = Chart::new(BarData::from_bars(initial.clone()));
            let id = if main {
                SeriesId::MAIN
            } else {
                chart.add_series(SeriesOptions::default())
            };
            chart.set_data(id, initial.clone());
            if replay {
                chart.set_replay_time(Some(bar(120, 0.0).time));
            }
            chart.set_bar_limit(id, NonZeroUsize::new(8));
            chart.set_data(id, initial.clone());
            chart.update(id, bar(540, 10.0));
            chart.update(id, bar(600, 11.0));
            if replay {
                chart.set_replay_time(None);
            }
            assert_eq!(chart.bars(id), kept, "replay {replay}, main {main}");
            assert_eq!(chart.remove_oldest_bars(id, 0), []);
            if replay {
                chart.set_replay_time(Some(bar(601, 0.0).time));
            }
            assert_eq!(chart.remove_oldest_bars(id, usize::MAX), kept);
            if replay {
                chart.set_replay_time(None);
            }
            assert!(chart.bars(id).is_empty());
            chart.set_bar_limit(id, None);
            chart.set_data(id, initial.clone());
            assert_eq!(chart.bars(id), initial);
            assert!(chart.bar_limit(id).is_none());
        }
    }
}

/// Guards the wiring of every bar update: the merged bars, the studies, the revision that span
/// caches are keyed on and the count of invalid bars must describe the same series. Studies
/// updated from a bar after the first change keep stale values, and ones not updated after a
/// removal keep values for removed bars; a revision bumped on every update rebuilds the caches
/// each tick, and one kept on an insert or a removal leaves them stale; out-of-order bars must
/// not be dropped, nor invalid bars drawn; a drawing must stay on its bar when bars are
/// inserted in front of it.
#[test]
#[cfg(feature = "drawings")]
fn updates_reach_bars_studies_and_revision() {
    enum Action {
        Update(Bar),
        SetData(Vec<Bar>),
        Limit(usize),
        RemoveOldest(usize),
    }
    use Action::{Limit, RemoveOldest, SetData, Update};
    // Five one-minute bars closing at 1 to 5.
    let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
    let mut invalid = bar(300, 6.0);
    invalid.high = 5.0;
    let series = |bars: &[(i64, f64)]| {
        bars.iter()
            .map(|&(t, close)| bar(t, close))
            .collect::<Vec<_>>()
    };
    // Expected bars, then the SMA(2) of their closes, which has no value on the first bar.
    #[rustfmt::skip]
    let cases = [
        ("retained window slides on append", vec![Limit(3), Update(bar(300, 6.0))],
            series(&[(180, 4.0), (240, 5.0), (300, 6.0)]),
            vec![None, Some(4.5), Some(5.5)], true, 0),
        ("oldest bars removed", vec![RemoveOldest(2)],
            series(&[(120, 3.0), (180, 4.0), (240, 5.0)]),
            vec![None, Some(3.5), Some(4.5)], true, 0),
        ("replacement respects limit", vec![Limit(4), SetData(start.clone())],
            start[1..].to_vec(),
            vec![None, Some(2.5), Some(3.5), Some(4.5)], true, 0),
        ("last-bar update", vec![Update(bar(240, 6.0))],
            series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 6.0)]),
            vec![None, Some(1.5), Some(2.5), Some(3.5), Some(5.0)], false, 0),
        ("append", vec![Update(bar(300, 6.0))],
            series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
            vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5), Some(5.5)], false, 0),
        ("correction", vec![Update(bar(120, 9.0))],
            series(&[(0, 1.0), (60, 2.0), (120, 9.0), (180, 4.0), (240, 5.0)]),
            vec![None, Some(1.5), Some(5.5), Some(6.5), Some(4.5)], true, 0),
        ("out of order", vec![Update(bar(90, 7.0))],
            series(&[(0, 1.0), (60, 2.0), (90, 7.0), (120, 3.0), (180, 4.0), (240, 5.0)]),
            vec![None, Some(1.5), Some(4.5), Some(5.0), Some(3.5), Some(4.5)], true, 0),
        ("invalid", vec![Update(invalid.clone()), SetData([start.clone(), vec![invalid]].concat())],
            start.clone(),
            vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5)], false, 2),
        ("replace", vec![SetData(series(&[(120, 3.0), (0, 1.0), (240, 5.0), (60, 2.0), (180, 4.0), (120, 8.0)]))],
            series(&[(0, 1.0), (60, 2.0), (120, 8.0), (180, 4.0), (240, 5.0)]),
            vec![None, Some(1.5), Some(5.0), Some(6.0), Some(4.5)], true, 0),
        ("replay step", vec![SetData([start.clone(), vec![bar(300, 6.0)]].concat())],
            series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
            vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5), Some(5.5)], false, 0),
        ("truncate", vec![SetData(start[..4].to_vec())],
            start[..4].to_vec(),
            vec![None, Some(1.5), Some(2.5), Some(3.5)], true, 0),
        ("sorted with a repeated time", vec![SetData(series(&[(0, 1.0), (60, 2.0), (120, 3.0), (120, 8.0), (180, 4.0)]))],
            series(&[(0, 1.0), (60, 2.0), (120, 8.0), (180, 4.0)]),
            vec![None, Some(1.5), Some(5.0), Some(6.0)], true, 0),
        ("correction, then append", vec![Update(bar(120, 9.0)), Update(bar(300, 6.0))],
            series(&[(0, 1.0), (60, 2.0), (120, 9.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
            vec![None, Some(1.5), Some(5.5), Some(6.5), Some(4.5), Some(5.5)], true, 0),
    ];
    let ctx = egui::Context::default();
    for (name, actions, bars, sma, revised, rejected) in cases {
        let mut chart = Chart::new(BarData::from_bars(start.clone()));
        let id = chart.add_indicator(Box::new(SMA::new(2)));
        // A drawing anchored on the bar at 10:03, the fourth.
        let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
        drawing.chart_points.push(ChartPoint::new(3.0, 4.0));
        chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        let revision = chart.state.revision();
        for action in actions {
            match action {
                Update(bar) => chart.update(SeriesId::MAIN, bar),
                SetData(bars) => chart.set_data(SeriesId::MAIN, bars),
                Limit(count) => chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(count)),
                RemoveOldest(count) => {
                    assert_eq!(
                        chart.remove_oldest_bars(SeriesId::MAIN, count),
                        start[..count]
                    );
                }
            }
        }
        frame(&ctx, &mut chart, 0.0, Vec::new());
        assert_eq!(chart.bars(SeriesId::MAIN), bars, "{name}");
        let anchor = chart.drawings_mut().drawings[0].chart_points[0].bar_idx as usize;
        assert_eq!(
            chart.bars(SeriesId::MAIN)[anchor].time,
            start[3].time,
            "{name}: drawing"
        );
        let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
        let values: Vec<Option<f64>> = study
            .values()
            .to_rows()
            .iter()
            .map(|value| match value {
                IndicatorValue::Single(value) => Some(*value),
                _ => None,
            })
            .collect();
        assert_eq!(values, sma, "{name}: SMA");
        assert_eq!(
            chart.state.revision() != revision,
            revised,
            "{name}: revision"
        );
        let feed = chart.feed_state(SeriesId::MAIN).unwrap();
        assert_eq!(feed.rejected_bars, rejected, "{name}: rejected");
    }
}

/// Merges retain anchor times, interpolating gaps and using the most common edge spacing.
/// After the inserts, 30 s occurs most: -60 / 30 = -2 and 7 + (360 - 240) / 30 = 11.
#[test]
#[cfg(feature = "drawings")]
fn inserted_bars_preserve_drawing_times_including_future_anchors() {
    let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
    let live = |seconds| (bar(seconds, 1.0), Origin::Live);
    #[rustfmt::skip]
    let cases = [
        ("inserts at two positions", vec![live(90), live(100), live(210)],
            vec![-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 6.0], vec![-2.0, 0.0, 1.0, 4.0, 5.0, 7.0, 11.0]),
        ("append", vec![live(300)], vec![3.0, 6.0], vec![3.0, 6.0]),
        ("history in front", vec![(bar(-60, 1.0), Origin::History)], vec![-1.0, 0.0], vec![0.0, 1.0]),
    ];
    for (name, incoming, points, moved) in cases {
        let mut chart = Chart::new(BarData::from_bars(start.clone()));
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = points.iter().map(|&at| ChartPoint::new(at, 1.0)).collect();
        chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        chart.apply(incoming);
        let points = &chart.drawings().drawings[0].chart_points;
        let after: Vec<f32> = points.iter().map(|point| point.bar_idx).collect();
        assert_eq!(after, moved, "{name}");
    }
}

/// A handle moves, restyles and removes only its own line, on the main series and on an
/// added series. Replacing a series' lines or removing the series retires its handles, and a
/// study takes no line. The moved line's tag and title use their own colours. `price_lines`
/// lists the current lines in drawing order after each edit, and each handle it returns
/// resolves through `price_line`.
#[test]
fn price_line_handles_change_only_their_own_line() {
    use crate::model::PriceLine;
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Shape};

    let listed = |chart: &Chart, series| {
        let lines: Vec<_> = chart
            .price_lines(series)
            .map(|(id, line)| (id, line.clone()))
            .collect();
        for (id, line) in &lines {
            assert_eq!(chart.price_line(*id), Some(line));
        }
        lines
    };
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0, 120.0]);
    chart.config.show_volume = false;
    assert!(listed(&chart, SeriesId::MAIN).is_empty());
    let kept = PriceLine::new(105.0, Color32::GOLD);
    let kept_id = chart.add_price_line(SeriesId::MAIN, kept.clone()).unwrap();
    let moved = chart
        .add_price_line(SeriesId::MAIN, PriceLine::new(112.0, Color32::RED))
        .unwrap();
    assert_eq!(
        listed(&chart, SeriesId::MAIN)
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [kept_id, moved]
    );
    let (fill, text, title) = (
        Color32::from_rgb(10, 20, 30),
        Color32::from_rgb(200, 210, 220),
        Color32::from_rgb(1, 2, 3),
    );
    let restyled = PriceLine::new(115.0, Color32::BLUE)
        .with_axis_label_colors(fill, text)
        .with_title("Target")
        .with_title_color(title);
    assert!(chart.update_price_line(moved, restyled.clone()));
    assert_eq!(chart.price_line(kept_id), Some(&kept));
    assert_eq!(chart.price_line(moved), Some(&restyled));
    assert_eq!(
        listed(&chart, SeriesId::MAIN),
        [(kept_id, kept.clone()), (moved, restyled.clone())]
    );

    frame(&ctx, &mut chart, 0.0, Vec::new());
    let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
    let y = chart.last_price_map.price_to_y(115.0);
    let tag = painted
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(tag) if tag.fill == fill => Some(tag.rect),
            _ => None,
        })
        .expect("the moved line's tag");
    assert!((tag.center().y - y).abs() < 1.0);
    assert!(painted.iter().any(|shape| matches!(shape,
        Shape::Text(t) if t.galley.text() == "Target"
            && t.galley.job.sections[0].format.color == title)));
    assert!(painted.iter().any(|shape| matches!(shape,
        Shape::Text(t) if t.galley.text() == "115.00"
            && t.galley.job.sections[0].format.color == text)));

    assert!(chart.remove_price_line(kept_id));
    assert!(!chart.remove_price_line(kept_id));
    assert!(!chart.update_price_line(kept_id, kept.clone()));
    assert_eq!(chart.price_line(moved), Some(&restyled));
    assert_eq!(listed(&chart, SeriesId::MAIN), [(moved, restyled.clone())]);
    chart.set_price_lines(SeriesId::MAIN, vec![kept.clone()]);
    assert_eq!(chart.price_line(moved), None);
    let reset = listed(&chart, SeriesId::MAIN);
    assert_eq!(reset.len(), 1);
    assert_ne!(reset[0].0, moved);

    let series = chart.add_series(crate::widget::SeriesOptions::default());
    let line = chart.add_price_line(series, kept.clone()).unwrap();
    assert!(chart.update_price_line(line, restyled.clone()));
    assert_eq!(chart.price_line(line), Some(&restyled));
    assert_eq!(listed(&chart, series), [(line, restyled.clone())]);
    assert_eq!(listed(&chart, SeriesId::MAIN), reset);
    assert!(chart.remove_series(series));
    assert_eq!(chart.price_line(line), None);
    assert!(listed(&chart, series).is_empty());
    let study = chart.add_indicator(Box::new(SMA::new(3)));
    assert!(listed(&chart, study).is_empty());
    assert_eq!(chart.add_price_line(study, kept), None);
}

/// A price-anchored marker in view widens the fit unless its options say otherwise, and one
/// out of view never does. Each layer paints the main series' marker and its text over the
/// main series' candles, the text on a background, and the marker under or over a series
/// added after it and a price line.
#[test]
fn marker_options_fit_and_layer_the_markers() {
    use crate::model::{MarkerOptions, MarkerPos, MarkerShape, MarkerZOrder, PriceLine};
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Shape};

    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.set_theme_override(Some(theme.clone()));
        chart.config.show_volume = false;
        chart.set_visible_bars(20);
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        let series = chart.add_series(crate::widget::SeriesOptions {
            color: Color32::GREEN,
            ..Default::default()
        });
        chart.set_data(series, bars.clone());
        chart.set_price_lines(
            SeriesId::MAIN,
            vec![PriceLine::new(105.0, Color32::BLUE).with_style(crate::model::LineStyle::Solid)],
        );
        let marker = |time, price| {
            Marker::new(time)
                .with_shape(MarkerShape::Circle)
                .with_color(Color32::RED)
                .with_text("Fill")
                .at_price(price, MarkerPos::AtPriceMiddle)
        };
        let markers = vec![marker(bars[95].time, 150.0), marker(bars[0].time, 300.0)];
        chart.set_markers(SeriesId::MAIN, markers.clone());
        assert_eq!(chart.markers(SeriesId::MAIN).len(), markers.len());
        assert!(chart.markers(series).is_empty());

        for (auto_scale, z_order) in [
            (false, MarkerZOrder::Normal),
            (true, MarkerZOrder::Normal),
            (true, MarkerZOrder::AboveSeries),
            (true, MarkerZOrder::Top),
        ] {
            chart.set_marker_options(
                SeriesId::MAIN,
                MarkerOptions {
                    auto_scale,
                    z_order,
                },
            );
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
            let max = chart.state.price_scale().range().max;
            assert_eq!(max > 150.0, auto_scale, "{max}");
            assert!(max < 300.0, "{max}");
            if !auto_scale {
                continue;
            }
            let line = |color| {
                let at = |shape: &Shape| match shape {
                    Shape::LineSegment { stroke, .. } => stroke.color == color,
                    Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
                    _ => false,
                };
                (
                    painted.iter().position(at).unwrap(),
                    painted.iter().rposition(at).unwrap(),
                )
            };
            let mark = painted
                .iter()
                .position(|shape| matches!(shape, Shape::Circle(c) if c.fill == Color32::RED))
                .unwrap();
            let (series_line, price_line) = (line(Color32::GREEN), line(Color32::BLUE));
            let candles = painted
                .iter()
                .rposition(|shape| {
                    matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                    && chart.get_rendered_price_rect().contains_rect(r.rect))
                })
                .unwrap();
            let text = painted
                .iter()
                .position(|shape| matches!(shape, Shape::Text(t) if t.galley.text() == "Fill"))
                .unwrap();
            assert!(candles < mark && candles + 1 < text, "{z_order:?}");
            assert!(
                matches!(&painted[text - 1], Shape::Rect(r) if r.fill == theme.chart.bg && r.rect.contains_rect(painted[text].visual_bounding_rect())),
                "{z_order:?}: the text has a background"
            );
            assert!(
                match z_order {
                    MarkerZOrder::Normal => mark < series_line.0,
                    MarkerZOrder::AboveSeries => series_line.1 < mark && mark < price_line.0,
                    MarkerZOrder::Top => price_line.1 < mark,
                },
                "{z_order:?}: marker {mark}, series {series_line:?}, price line {price_line:?}"
            );
        }
    }
}

/// Autoscaling fits the painted shape, outline and multiline text at both edges, at the
/// configured size, on shared and independent scales and in a separate pane. Manual ranges,
/// disabled autoscaling, hidden series and markers outside the horizontal view keep their fit.
/// Flat data and flat line scales also fit oversized above-bar markers.
#[test]
fn marker_autoscale_keeps_shapes_and_text_inside_each_pane() {
    use crate::model::{MarkerOptions, MarkerPos, MarkerShape, SeriesScale};
    use crate::scales::{PriceScaleMargins, PriceScaleMode};
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Shape};

    for placement in ["main", "shared", "independent", "pane", "flat", "line"] {
        for mode in [
            PriceScaleMode::Normal,
            PriceScaleMode::Logarithmic,
            PriceScaleMode::Percentage,
            PriceScaleMode::IndexedTo100,
        ] {
            for inverted in [false, true] {
                let ctx = egui::Context::default();
                let flat = placement == "flat";
                let large = flat || placement == "line";
                let mut chart = chart(&[if flat { 110.0 } else { 100.0 }], &[110.0]);
                chart.config.show_volume = false;
                chart.config.show_ohlc_info = false;
                chart.config.right_axis_scale_mode = mode;
                let bars = chart.bars(SeriesId::MAIN).to_vec();
                let id = if matches!(placement, "main" | "flat") {
                    SeriesId::MAIN
                } else {
                    let id = chart.add_series(crate::widget::SeriesOptions {
                        chart_type: if placement == "line" {
                            crate::model::ChartType::Line
                        } else {
                            crate::model::ChartType::Candles
                        },
                        scale: if matches!(placement, "independent" | "line") {
                            SeriesScale::Left
                        } else {
                            SeriesScale::Right
                        },
                        ..Default::default()
                    });
                    chart.set_data(id, bars.clone());
                    if placement == "pane" {
                        let pane = chart.add_pane();
                        assert!(chart.move_series_to_pane(id, pane));
                        chart.set_pane_stretch_factor(pane, 3.0);
                    }
                    id
                };
                let options = chart.series_price_scale_mut(id).unwrap().options_mut();
                options.mode = mode;
                options.invert_scale = inverted;
                options.scale_margins = PriceScaleMargins {
                    top: 0.0,
                    bottom: 0.0,
                };
                frame(&ctx, &mut chart, 0.0, Vec::new());
                chart.set_visible_bars(40);
                chart.set_start_idx(40);
                frame(&ctx, &mut chart, 0.1, Vec::new());
                let baseline = chart.series_price_scale(id).unwrap().range();
                let mut marks = Vec::new();
                for (i, shape) in [
                    MarkerShape::Circle,
                    MarkerShape::Square,
                    MarkerShape::ArrowUp,
                    MarkerShape::ArrowDown,
                    MarkerShape::TriangleUp,
                    MarkerShape::TriangleDown,
                    MarkerShape::Diamond,
                    MarkerShape::Star,
                    MarkerShape::Cross,
                    MarkerShape::Flag,
                ]
                .into_iter()
                .enumerate()
                {
                    for (edge, price, position) in [
                        (0, 110.0, MarkerPos::AboveBar),
                        (1, 100.0, MarkerPos::BelowBar),
                        (2, 110.0, MarkerPos::AtPriceTop),
                        (3, 100.0, MarkerPos::AtPriceBottom),
                        (4, 110.0, MarkerPos::AtPriceMiddle),
                        (5, 100.0, MarkerPos::AtPriceMiddle),
                    ] {
                        if large && position != MarkerPos::AboveBar {
                            continue;
                        }
                        marks.push(
                            Marker::new(bars[45 + i * 2].time)
                                .with_shape(shape)
                                .with_color(Color32::RED)
                                .with_size(if large { 10.0 } else { 2.0 })
                                .with_text(if large {
                                    format!("edge {i}/{edge}")
                                } else {
                                    format!("edge {i}/{edge}\nannotation")
                                })
                                .at_price(price, position),
                        );
                    }
                }
                chart.set_markers(id, marks.clone());
                chart.set_marker_options(
                    id,
                    MarkerOptions {
                        auto_scale: false,
                        ..Default::default()
                    },
                );
                frame(&ctx, &mut chart, 0.2, Vec::new());
                assert_eq!(chart.series_price_scale(id).unwrap().range(), baseline);
                chart.set_marker_options(id, MarkerOptions::default());
                frame(&ctx, &mut chart, 0.3, Vec::new());
                let painted = shapes(&ctx, &mut chart, 0.4, Vec::new());
                let pane = chart.series_pane(id).unwrap();
                let rect = if pane == crate::widget::PaneId::MAIN {
                    chart.get_rendered_price_rect()
                } else {
                    chart
                        .pane_layout
                        .panes
                        .iter()
                        .find(|p| p.id == pane)
                        .unwrap()
                        .rect
                };
                let mut labels = 0;
                let mut bounds = Vec::new();
                let mut in_marker = false;
                for shape in &painted {
                    let marker = match shape {
                        Shape::Circle(c) => c.fill == Color32::RED,
                        Shape::Rect(r) => r.fill == Color32::RED || r.stroke.color == Color32::RED,
                        Shape::Path(p) => {
                            p.fill == Color32::RED
                                || p.stroke.color == egui::epaint::ColorMode::Solid(Color32::RED)
                        }
                        Shape::LineSegment { stroke, .. } => stroke.color == Color32::RED,
                        Shape::Text(t) if t.galley.text().starts_with("edge ") => {
                            labels += 1;
                            true
                        }
                        _ => false,
                    };
                    in_marker |= marker;
                    if in_marker {
                        bounds.push(shape.visual_bounding_rect());
                    }
                    if matches!(shape, Shape::Text(t) if t.galley.text().starts_with("edge ")) {
                        in_marker = false;
                    }
                }
                assert_eq!(
                    labels,
                    marks.len(),
                    "{placement} {mode:?} inverted {inverted}"
                );
                for bounds in bounds {
                    assert!(
                        bounds.top() >= rect.top() - 0.01
                            && bounds.bottom() <= rect.bottom() + 0.01,
                        "{placement} {mode:?} inverted {inverted}: {bounds:?} outside {rect:?}"
                    );
                }
                let fitted = chart.series_price_scale(id).unwrap().range();
                frame(&ctx, &mut chart, 0.5, Vec::new());
                assert_eq!(
                    chart.series_price_scale(id).unwrap().range(),
                    fitted,
                    "no frame-to-frame growth"
                );
                chart.set_marker_options(
                    id,
                    MarkerOptions {
                        auto_scale: false,
                        ..Default::default()
                    },
                );
                frame(&ctx, &mut chart, 0.51, Vec::new());
                assert_eq!(chart.series_price_scale(id).unwrap().range(), baseline);
                chart.set_marker_options(id, MarkerOptions::default());
                let manual = if baseline.min == baseline.max {
                    crate::scales::PriceRange::new(baseline.min - 0.01, baseline.max + 0.01)
                } else {
                    baseline
                };
                chart
                    .series_price_scale_mut(id)
                    .unwrap()
                    .set_range(manual.min, manual.max);
                frame(&ctx, &mut chart, 0.6, Vec::new());
                assert_eq!(chart.series_price_scale(id).unwrap().range(), manual);
                chart.series_price_scale_mut(id).unwrap().reset();
                chart.set_markers(
                    id,
                    vec![
                        Marker::new(bars[0].time)
                            .at_price(1e6, MarkerPos::AtPriceMiddle)
                            .with_size(100.0)
                            .with_text("offscreen"),
                    ],
                );
                frame(&ctx, &mut chart, 0.7, Vec::new());
                assert_eq!(chart.series_price_scale(id).unwrap().range(), baseline);
                if placement != "pane" {
                    let main_baseline = chart.state.price_scale().range();
                    chart.set_markers(id, marks);
                    chart.set_series_visible(id, false);
                    frame(&ctx, &mut chart, 0.8, Vec::new());
                    assert_eq!(chart.state.price_scale().range(), main_baseline);
                    assert_eq!(chart.series_price_scale(id).unwrap().range(), baseline);
                }
            }
        }
    }
}

/// A live update that raises or lowers the close of a stored bar marks its new close in the
/// rise or the fall colour, replacing the bar's previous mark, on the main and on an added
/// series. An unchanged close, a new bar and replaced history add no mark, and a mark goes
/// once its duration has passed.
#[test]
fn change_markers_mark_rises_and_falls_of_the_close() {
    use crate::model::ChangeMarkers;
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Pos2, Shape};

    let (up, down) = (Color32::from_rgb(1, 200, 1), Color32::from_rgb(200, 1, 1));
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0]);
    chart.config.show_volume = false;
    let options = ChangeMarkers {
        up_color: up,
        down_color: down,
        duration: Duration::from_secs(600),
    };
    chart.set_change_markers(SeriesId::MAIN, Some(options));
    let series = chart.add_series(crate::widget::SeriesOptions::default());
    let bars = chart.bars(SeriesId::MAIN).to_vec();
    chart.set_data(series, bars.clone());
    chart.set_change_markers(series, Some(options));
    let last = bars.last().unwrap().time;
    let bar = |close: f64| Bar::new(last, 100.0, close.max(110.0), 100.0, close, 1.0);
    let marks = |chart: &mut Chart| {
        shapes(&ctx, chart, 0.0, Vec::new())
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Circle(c) if c.radius == 4.0 && (c.fill == up || c.fill == down) => {
                    Some((c.center, c.fill))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    marks(&mut chart);
    let at = |chart: &Chart, close| {
        Pos2::new(
            chart.get_chart_mapping().idx_to_x(99),
            chart.last_price_map.price_to_y(close),
        )
    };
    for (close, color) in [(108.0, down), (109.0, up), (109.0, up)] {
        chart.update(SeriesId::MAIN, bar(close));
        let painted = marks(&mut chart);
        assert_eq!(painted, vec![(at(&chart, close), color)], "close {close}");
    }
    chart.update(
        SeriesId::MAIN,
        Bar::new(last + TimeDelta::days(1), 1.0, 2.0, 1.0, 2.0, 1.0),
    );
    assert_eq!(marks(&mut chart).len(), 1, "a new bar is not a change");
    chart.update(series, bar(120.0));
    assert_eq!(
        marks(&mut chart).iter().filter(|mark| mark.1 == up).count(),
        2
    );
    chart.set_data(SeriesId::MAIN, bars.clone());
    chart.set_data(series, bars);
    assert!(
        marks(&mut chart).is_empty(),
        "replaced history drops the marks"
    );
    chart.set_change_markers(
        SeriesId::MAIN,
        Some(ChangeMarkers {
            duration: Duration::ZERO,
            ..options
        }),
    );
    chart.update(SeriesId::MAIN, bar(105.0));
    assert!(
        marks(&mut chart).is_empty(),
        "an expired mark is not painted"
    );
}
