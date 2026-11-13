use crate::chart::series::SeriesSettings;
use crate::scales::PriceScaleMode;
#[cfg(feature = "icons")]
use crate::widget::navigation::tests::navigation_press;
use crate::widget::test_frames::{chart, frame, shapes};
use egui::{Event, Pos2, Rect, Shape};

#[test]
fn automatic_price_precision_reaches_every_axis_tag() {
    use crate::model::{Bar, BarData, Level1Quote, PriceLine, Timeframe};
    use crate::studies::{CustomIndicator, IndicatorValue};
    let start = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .to_utc();
    let bars = (0..100)
        .map(|i| {
            let base = 1.0850 + 0.0002 * (i % 10) as f64;
            Bar::new(
                start + chrono::Duration::hours(i),
                base,
                base + 0.0004,
                base - 0.0004,
                base + 0.00012,
                1.0,
            )
        })
        .collect();
    let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
    chart.set_timeframe(Timeframe::Hour1);
    chart.config.show_volume = false;
    chart.config.show_high_low = true;
    chart.config.show_bid_ask = true;
    chart.config.show_symbol_prev_close = true;
    chart.config.show_extended_session_price = true;
    chart.config.price_axis.allow_label_overlap = true;
    chart.config.price_axis.show_study_names = true;
    chart.config.show_quote_boxes = false;
    chart.price_scale_mut().set_range(1.0845, 1.0873);
    chart.set_quote(Some(Level1Quote {
        symbol: "TEST".into(),
        ts: start + chrono::Duration::hours(99),
        bid: 1.08652,
        ask: 1.08702,
        bid_size: 1.0,
        ask_size: 1.0,
    }));
    chart.set_session_quote(Some(crate::model::quote::SessionQuote::new(1.08512, "EXT")));
    chart.add_price_line(
        crate::widget::SeriesId::MAIN,
        PriceLine::new(1.08532, egui::Color32::GOLD),
    );
    chart.add_indicator(Box::new(
        CustomIndicator::new("Study", |bars| {
            vec![IndicatorValue::Single(1.08572); bars.len()]
        })
        .with_overlay(true),
    ));
    let series = chart.add_series(crate::widget::SeriesOptions::default());
    chart.set_data(
        series,
        (0..100)
            .map(|i| {
                let close = 1.08632;
                Bar::new(
                    start + chrono::Duration::hours(i),
                    close,
                    close,
                    close,
                    close,
                    1.0,
                )
            })
            .collect(),
    );
    let ctx = egui::Context::default();
    shapes(&ctx, &mut chart, 0.0, vec![]);
    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
    let labels: Vec<_> = painted
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) if text.pos.x >= chart.last_rendered_price_rect.right() => {
                Some(text.galley.text())
            }
            _ => None,
        })
        .collect();
    // A 0.00025 tick step uses six decimals. These literal prices come from the bars above.
    for expected in [
        "1.086920\n01:00:00",
        "EXT 1.085120",
        "1.085320",
        "Study 1.085720",
        "1.086120",
        "High 1.087200",
        "Low 1.084600",
        "1.086520",
        "1.087020",
        "1.086320",
    ] {
        assert!(labels.contains(&expected), "missing {expected}: {labels:?}");
    }
}

#[test]
fn price_labels_and_lines_are_independent_on_either_axis() {
    use crate::model::{Level1Quote, Timeframe};
    // `series` sets the main series' own label and price line, which the chart-wide
    // switches override.
    for left in [false, true] {
        for (label, line, countdown, series) in [
            (true, true, true, true),
            (true, false, false, true),
            (false, true, false, true),
            (false, false, false, true),
            (true, true, false, false),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0, 120.0]);
            chart.set_timeframe(Timeframe::Min1);
            chart.config.show_right_axis = !left;
            chart.config.show_left_axis = left;
            chart.config.show_volume = false;
            chart.config.show_symbol_labels = false;
            chart.config.show_ohlc_info = false;
            chart.config.show_ohlc_change = false;
            chart.config.show_ohlc_change_percent = false;
            chart.config.show_symbol_last_val = label;
            chart.config.show_price_line = line;
            chart.config.show_symbol_prev_close = label;
            chart.config.show_prev_close_line = line;
            chart.config.show_countdown = countdown;
            chart.config.show_bid_ask = label;
            chart.main_series_style.last_value_visible = series;
            chart.main_series_style.price_line_visible = series;
            let bar = chart.state.data().bars.last().unwrap();
            chart.set_quote(Some(Level1Quote {
                symbol: "TEST".into(),
                ts: bar.time + chrono::Duration::seconds(30),
                bid: 119.0,
                ask: 121.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            chart.config.show_quote_boxes = false;
            chart.set_session_quote(Some(crate::model::quote::SessionQuote::new(
                115.0, "EXTENDED",
            )));
            chart.set_price_lines(
                crate::chart::series::SeriesId::MAIN,
                vec![crate::model::PriceLine::new(115.0, egui::Color32::GOLD)],
            );
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.last_rendered_price_rect;
            let custom_tag = painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Rect(tag) if tag.fill == egui::Color32::GOLD => Some(tag.rect),
                    _ => None,
                })
                .expect("custom price line retains its axis label");
            assert!(if left {
                custom_tag.right() <= plot.left()
            } else {
                custom_tag.left() >= plot.right()
            });
            let session_text = painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text().starts_with("EXTENDED") => Some(text),
                    _ => None,
                })
                .expect("session tag");
            assert!(
                session_text.pos.x
                    >= plot.left() - chart.config.left_axis_width.max(chart.measured_axis_width),
                "session label is clipped on the left"
            );
            for name in ["Bid", "Ask"] {
                let badge = painted.iter().find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text() == name => {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                });
                assert_eq!(badge.is_some(), label);
                if let Some(badge) = badge {
                    assert!(
                        ctx.content_rect().contains_rect(badge)
                            && if left {
                                badge.center().x >= plot.left()
                            } else {
                                badge.center().x <= plot.right()
                            },
                        "{name} stays inside the plot on either axis"
                    );
                }
            }
            let close_y = chart.last_price_map.price_to_y(120.0);
            let prev_y = chart.last_price_map.price_to_y(110.0);
            let price_tags: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect)
                        if rect.fill == chart.config.bullish_color
                            && (rect.rect.center().y - close_y).abs() < 1.0
                            && rect.rect.width() > 20.0 =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                !price_tags.is_empty(),
                label && series,
                "{left} {label} {line}"
            );
            for tag in price_tags {
                assert!(if left {
                    tag.right() <= plot.left()
                } else {
                    tag.left() >= plot.right()
                });
            }
            for (y, color, shown) in [
                (close_y, chart.config.bullish_color, line && series),
                (
                    prev_y,
                    crate::theme::ThemeExt::active_theme(&ctx).ui.text_secondary,
                    line,
                ),
            ] {
                let has_line = painted.iter().any(|shape| match shape {
                    Shape::LineSegment { points, stroke } => {
                        stroke.color == color
                            && (points[0].y - y).abs() < 0.01
                            && (points[1].y - y).abs() < 0.01
                    }
                    Shape::Mesh(mesh) => {
                        mesh.vertices.len() > 8
                            && mesh.vertices.iter().all(|vertex| {
                                vertex.color == color && (vertex.pos.y - y).abs() <= 1.5
                            })
                    }
                    _ => false,
                });
                assert_eq!(
                    has_line, shown,
                    "line at {y}: left={left} label={label} line={line}"
                );
            }
            assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text().contains("00:30"))), countdown);
        }
    }
}

/// Line tags use the anchor, stay clear of the last price and follow this frame's drag.
/// Bar 51 is 1,700,000,000 + 51 * 86,400 = Jan 4, 2024 at 22:13:20 UTC;
/// bar 61 is ten days later. Yellow's contrast with white is 1.22, so its text is black.
#[cfg(feature = "drawings")]
#[test]
fn line_drawings_tag_their_price_and_time_on_the_axes() {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType as T, TimeframeVisibility};
    use egui::{Color32, Modifiers, PointerButton, vec2};
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0, 120.0]);
    chart.config.show_symbol_last_val = true;
    for (id, tool, bar, price, color) in [
        (1, T::HorizontalLine, 90.0, 105.37, [255, 235, 59, 128]),
        (2, T::HorizontalRay, 51.0, 112.63, [11, 102, 35, 255]),
        (3, T::CrossLine, 61.0, 108.41, [101, 31, 147, 255]),
        (4, T::VerticalLine, 51.0, 100.0, [23, 57, 181, 255]),
        (5, T::HorizontalLine, 90.0, 116.29, [200, 0, 0, 255]),
        (6, T::HorizontalLine, 90.0, 102.71, [200, 0, 0, 255]),
        (7, T::HorizontalLine, 90.0, 118.13, [200, 0, 0, 255]),
        (8, T::VerticalLine, 71.0, 100.0, [200, 0, 0, 255]),
        (9, T::CrossLine, 81.0, 103.59, [9, 9, 9, 255]),
        (10, T::HorizontalRay, 1000.0, 114.71, [200, 0, 0, 255]),
        (11, T::HorizontalLine, 90.0, 120.0, [140, 70, 20, 255]),
    ] {
        let mut drawing = Drawing::with_color(id, tool, color);
        drawing.chart_points = vec![ChartPoint::new(bar, price)];
        match id {
            5 | 9 => drawing.price_axis_label = false,
            8 => drawing.time_axis_label = false,
            6 => drawing.visible = false,
            7 => drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]),
            _ => {}
        }
        chart.drawings_mut().drawings.push(drawing);
    }
    frame(&ctx, &mut chart, 0.0, vec![]);
    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
    let plot = chart.last_rendered_price_rect;
    let price_texts: Vec<_> = painted
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) if text.pos.x >= plot.right() => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    for (label, shown) in [
        ("105.37", true),
        ("112.63", true),
        ("108.41", true),
        ("116.29", false),
        ("102.71", false),
        ("118.13", false),
        ("103.59", false),
        ("114.71", false),
    ] {
        assert_eq!(price_texts.contains(&label), shown, "price {label}");
    }
    let time_texts: Vec<_> = painted
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) if text.pos.y >= plot.bottom() => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    for (label, shown) in [
        ("Jan 04, 2024", true),
        ("Jan 14, 2024", true),
        ("Jan 24, 2024", false),
        ("Feb 03, 2024", true),
    ] {
        assert_eq!(time_texts.contains(&label), shown, "time {label}");
    }
    let price_text = painted
        .iter()
        .find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text() == "105.37" => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(price_text.fallback_color, Color32::BLACK);
    let tag_rect = |painted: &[Shape], color| {
        painted
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(rect) if rect.fill == color && rect.rect.left() >= plot.right() => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("drawing price tag")
    };
    assert!(tag_rect(&painted, Color32::from_rgb(255, 235, 59)).contains(price_text.pos));
    let close_y = chart.last_price_map.price_to_y(120.0);
    let last = painted
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(rect)
                if rect.fill == chart.config.bullish_color
                    && (rect.rect.center().y - close_y).abs() < 1.0
                    && rect.rect.width() > 20.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("last-price tag");
    let line = tag_rect(&painted, Color32::from_rgb(140, 70, 20));
    assert!(line.bottom() <= last.top() + 0.01 || line.top() >= last.bottom() - 0.01);

    let start = Pos2::new(plot.right() - 10.0, chart.last_price_map.price_to_y(105.37));
    frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(start)]);
    frame(
        &ctx,
        &mut chart,
        0.3,
        vec![Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    frame(
        &ctx,
        &mut chart,
        0.4,
        vec![Event::PointerMoved(start + vec2(0.0, 20.0))],
    );
    let painted = shapes(
        &ctx,
        &mut chart,
        0.5,
        vec![Event::PointerMoved(start + vec2(0.0, 60.0))],
    );
    let price = chart.drawings().drawings[0].chart_points[0].price;
    assert!(price < 104.0, "the line moved: {price}");
    assert!(
        (tag_rect(&painted, Color32::from_rgb(255, 235, 59))
            .center()
            .y
            - chart.last_price_map.price_to_y(price))
        .abs()
            < 1.0,
        "tag follows the current frame's anchor"
    );
}

/// On a logarithmic scale 100 lies halfway between 10 and 1,000 (ln 100 is the mean of ln 10 and
/// ln 1,000); a linear mapping puts it a tenth of the way up. A range zoomed by hand before the
/// switch, current or in the zoom history, is in the old mode's units and must not carry over.
#[test]
fn log_mode_reaches_the_rendered_mapping() {
    let ctx = egui::Context::default();
    let mut chart = chart(&[10.0], &[1000.0]);
    frame(&ctx, &mut chart, 0.0, Vec::new());
    chart.state.price_scale_mut().zoom(0.5, 500.0);
    chart.state.push_zoom_state();
    chart.apply_series_settings(&SeriesSettings {
        price_scale_mode: PriceScaleMode::Logarithmic,
        ..SeriesSettings::default()
    });
    chart.state.pop_zoom_state();
    frame(&ctx, &mut chart, 0.1, Vec::new());
    let map = chart.get_chart_mapping();
    let (y10, y100, y1000) = (
        map.price_to_y(10.0),
        map.price_to_y(100.0),
        map.price_to_y(1000.0),
    );
    assert!(
        (y100 - (y10 + y1000) / 2.0).abs() < 0.01,
        "{y10} {y100} {y1000}"
    );
    let (top, bottom) = map.price.extent();
    assert!(
        top < y1000 && y10 < bottom,
        "{y10}..{y1000} outside {top}..{bottom}"
    );
}

/// Axis buttons route their clicks without changing the price range or viewport.
#[cfg(feature = "icons")]
#[test]
fn axis_buttons_route_without_scaling() {
    for (settings, magnet, enabled, left, bottom) in [
        (false, false, true, false, false),
        (true, false, true, false, false),
        (false, true, true, false, false),
        (false, false, false, false, false),
        (false, false, true, true, false),
        (false, false, true, false, true),
    ] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config.show_plus_button = enabled;
        chart.config.show_left_axis = left;
        chart.config.show_right_axis = !left;
        #[cfg(feature = "studies")]
        if bottom {
            chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
        }
        if magnet {
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        let plot = chart.last_rendered_price_rect;
        let hover = Pos2::new(
            plot.center().x,
            if bottom {
                plot.bottom() - 0.5
            } else {
                plot.center().y + 10.0
            },
        );
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(hover)]);
        let map = chart.get_chart_mapping();
        let range = map.price.range();
        let viewport = chart.get_time_scale_state();
        let pos = if settings {
            Pos2::new(
                plot.right() + chart.config.right_axis_width / 2.0,
                plot.bottom() + 14.0,
            )
        } else {
            let (_, tag) = chart.axis_hover.expect("hover tag");
            tag.left_center() - egui::Vec2::new(13.0, 0.0)
        };
        for (step, events) in [
            vec![Event::PointerMoved(pos)],
            vec![navigation_press(pos, true)],
            vec![navigation_press(pos, false)],
        ]
        .into_iter()
        .enumerate()
        {
            frame(&ctx, &mut chart, 0.2 + step as f64 * 0.1, events);
        }
        let expected_price = if magnet {
            100.0
        } else {
            map.y_to_price(hover.y)
        };
        if settings {
            assert!(chart.take_scale_settings_request());
            assert!(!chart.take_scale_settings_request());
        } else if !enabled {
            assert!(chart.take_right_click().is_none());
        } else {
            assert!(
                matches!(chart.take_right_click(), Some(crate::widget::RightClickTarget::Background { price, pos }) if (price - expected_price).abs() < 0.001 && pos.y <= plot.bottom())
            );
        }
        assert_eq!(chart.get_chart_mapping().price.range().min, range.min);
        assert_eq!(chart.get_chart_mapping().price.range().max, range.max);
        assert_eq!(chart.get_time_scale_state(), viewport);
        if enabled && !settings && !magnet && !bottom {
            let tag = chart.axis_hover.unwrap().1;
            let start = tag.center();
            let end = start + egui::vec2(0.0, 30.0);
            for (step, events) in [
                vec![Event::PointerMoved(start)],
                vec![navigation_press(start, true)],
                vec![Event::PointerMoved(end)],
                vec![navigation_press(end, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.6 + step as f64 * 0.1, events);
            }
            assert_ne!(
                chart.get_chart_mapping().price.range(),
                range,
                "drag over the tag"
            );
            assert!(chart.take_right_click().is_none());
        }
    }
}

/// Sixty one-minute slots keep clock labels when a weekend is in view or in the
/// prefetched keys, and when an overnight gap falls inside the view.
#[test]
fn intraday_time_labels_keep_their_density_across_session_gaps() {
    use crate::model::{Bar, BarData, LogicalRange, Timeframe};
    use chrono::{Duration, TimeZone, Utc};
    let bars: Vec<_> = [5, 8, 9]
        .into_iter()
        .flat_map(|day| {
            let open = Utc.with_ymd_and_hms(2024, 1, day, 14, 30, 0).unwrap();
            (0..390).map(move |minute| {
                Bar::new(
                    open + Duration::minutes(minute),
                    100.0,
                    110.0,
                    100.0,
                    110.0,
                    1.0,
                )
            })
        })
        .collect();
    for (name, left, right) in [
        ("Monday only", 420.5, 480.5),
        ("weekend left of the view", 390.5, 450.5),
        ("weekend in view", 360.5, 420.5),
        ("overnight in view", 750.5, 810.5),
    ] {
        let ctx = egui::Context::default();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars.clone()));
        chart.config.show_volume = false;
        chart.set_timeframe(Timeframe::Min1);
        frame(&ctx, &mut chart, 0.0, vec![]);
        assert!(chart.set_visible_logical_range(LogicalRange::new(left, right)));
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.pos.y > plot.bottom() && text.pos.x < plot.right() => {
                    Some(text.galley.text())
                }
                _ => None,
            })
            .collect();
        let clocks = labels.iter().filter(|label| label.contains(':')).count();
        assert!(clocks >= 3, "{name}: {labels:?}");
    }
}

#[test]
fn time_visibility_flags_reach_axis_and_crosshair() {
    use crate::model::{Bar, BarData, Timeframe};
    use chrono::{Duration, TimeZone, Utc};
    let time = Utc.with_ymd_and_hms(2024, 3, 10, 6, 59, 30).unwrap();
    for (seconds, time_visible, seconds_visible, ticks_visible, expected) in [
        (1, true, true, false, "Sun 10 Mar '24 01:59:30"),
        (1, true, false, false, "Sun 10 Mar '24 01:59"),
        (1, false, true, false, "Mar 10, 2024"),
        (1, false, false, false, "Mar 10, 2024"),
        (1, true, true, true, "Sun 10 Mar '24 01:59:30"),
        (60, true, true, true, "Sun 10 Mar '24 01:59"),
        (86_400, true, true, true, "Mar 10, 2024"),
    ] {
        let ctx = egui::Context::default();
        let bars = (0..100)
            .map(|i| {
                Bar::new(
                    time + Duration::seconds((i - 50) * seconds),
                    100.0,
                    102.0,
                    99.0,
                    101.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(if seconds == 1 {
            Timeframe::Sec1
        } else if seconds == 60 {
            Timeframe::Min1
        } else {
            Timeframe::Day1
        });
        chart.chart_options.tooltip.enabled = false;
        // Each change after a frame, as the axis menu makes them, so the time labels follow.
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.chart_options.time_scale.timezone =
            crate::scales::TimezoneMode::Timezone("-05:00".parse().unwrap());
        frame(&ctx, &mut chart, 0.03, vec![]);
        let options = &mut chart.chart_options.time_scale;
        options.time_visible = time_visible;
        options.seconds_visible = seconds_visible;
        options.ticks_visible = ticks_visible;
        frame(&ctx, &mut chart, 0.06, vec![]);
        let map = chart.get_chart_mapping();
        let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
        assert!(
            painted
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == expected)),
            "{seconds} {time_visible} {seconds_visible}"
        );
        let ticks = painted.iter().filter(|shape| matches!(shape, Shape::LineSegment { points, .. } if (points[0].y - map.rect.bottom()).abs() < 0.1 && (points[1].y - points[0].y - 3.0).abs() < 0.1)).count();
        assert_eq!(ticks > 0, ticks_visible, "tick visibility: {seconds}");
    }
}

/// Whitespace on either side labels only the weekdays the daily series contains,
/// including clock changes that move local midnight across UTC midnight.
#[test]
fn whitespace_time_labels_skip_days_without_bars() {
    use crate::model::{Bar, BarData, Timeframe, TimezoneId};
    use crate::scales::{CustomTimeFormatter, TimezoneMode};
    use chrono::{Datelike, Duration, NaiveDate, TimeZone};
    use std::sync::Arc;
    // (first and last local dates, weekdays only, logical right edge or None for a
    // whitespace offset from the last bar, whitespace offset, zone, expected labels)
    for (start, last, weekdays, right, offset, zone, expected) in [
        (
            "2024-12-02",
            "2025-01-30",
            true,
            None,
            6.0,
            "UTC",
            &["Mon 03"][..],
        ),
        (
            "2024-12-02",
            "2025-01-31",
            true,
            None,
            6.0,
            "UTC",
            &["Mon 03"][..],
        ),
        (
            "2024-12-02",
            "2025-01-30",
            false,
            None,
            6.0,
            "UTC",
            &["Sat 01"][..],
        ),
        // Partly before the first bar: bar 0 at the right edge.
        (
            "2024-12-02",
            "2025-01-30",
            true,
            Some(0.0),
            0.0,
            "UTC",
            &["Mon 25"][..],
        ),
        (
            "2024-12-02",
            "2025-01-30",
            false,
            Some(0.0),
            0.0,
            "UTC",
            &["Sun 01"][..],
        ),
        // Wholly before the first bar.
        (
            "2024-12-02",
            "2025-01-30",
            true,
            Some(-3.0),
            0.0,
            "UTC",
            &["Mon 25"][..],
        ),
        (
            "2024-12-02",
            "2025-01-30",
            false,
            Some(-3.0),
            0.0,
            "UTC",
            &["Mon 25"][..],
        ),
        // British summer midnight is 23:00Z on the previous date. The October
        // change lies in whitespace; the March change lies in the final 16 bars.
        (
            "2026-09-01",
            "2026-10-23",
            true,
            None,
            10.0,
            "Europe/London",
            &["Mon 26", "Tue 27", "Wed 28", "Thu 29", "Fri 30"][..],
        ),
        (
            "2026-02-02",
            "2026-04-03",
            true,
            None,
            10.0,
            "Europe/London",
            &["Mon 06", "Tue 07", "Wed 08", "Thu 09", "Fri 10"][..],
        ),
        // The same weekday rule applies before bars that begin after the March change.
        (
            "2026-03-30",
            "2026-05-01",
            true,
            Some(-3.0),
            0.0,
            "Europe/London",
            &["Fri 20", "Mon 23"][..],
        ),
    ] {
        let Some(zone) = TimezoneId::named(zone) else {
            continue;
        };
        let start: NaiveDate = start.parse().unwrap();
        let last: NaiveDate = last.parse().unwrap();
        let bars: Vec<_> = (0..)
            .map(|day| start + Duration::days(day))
            .take_while(|date| *date <= last)
            .filter(|date| !weekdays || date.weekday().number_from_monday() <= 5)
            .map(|date| {
                let time = zone
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .unwrap()
                    .to_utc();
                Bar::new(time, 100.0, 110.0, 100.0, 110.0, 1.0)
            })
            .collect();
        let base = (bars.len() - 1) as f64;
        let ctx = egui::Context::default();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.config.show_volume = false;
        chart.set_timeframe(Timeframe::Day1);
        chart.chart_options.time_scale.timezone = TimezoneMode::Timezone(zone.clone());
        if zone.name() == "Europe/London" {
            chart
                .chart_options
                .time_scale
                .tick_mark_max_character_length = 1;
        }
        chart.set_time_formatter(Some(Arc::new(CustomTimeFormatter::new(|time, _| {
            time.format("%a %d").to_string()
        }))));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let scale = chart.state.time_scale_mut();
        scale.set_bar_spacing(80.0);
        scale.set_right_offset(right.map_or(offset, |right| right - base));
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.pos.y > plot.bottom() && text.pos.x < plot.right() => {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect();
        let row = format!("{zone}, {start}..{last}, {right:?}: {labels:?}");
        let weekend = labels
            .iter()
            .any(|label| label.starts_with("Sat") || label.starts_with("Sun"));
        assert!(!(weekdays && weekend), "{row}");
        for expected in expected {
            assert!(
                labels.iter().any(|label| label == expected),
                "{expected}: {row}"
            );
        }
    }
}

/// Daily bars put bold day and month labels on the axis; 20 characters space them at
/// least 200 points apart, which the default 8 (80 points) does not.
#[test]
fn time_scale_options_style_and_space_the_axis_labels() {
    for (bold, border, characters) in [(true, false, 8), (false, true, 8), (true, false, 20)] {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0], &[105.0]);
        chart.config.show_volume = false;
        let options = &mut chart.chart_options.time_scale;
        options.allow_bold_labels = bold;
        options.border_visible = border;
        options.border_color = Some(egui::Color32::RED);
        options.tick_mark_max_character_length = characters;
        frame(&ctx, &mut chart, 0.0, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.pos.y > plot.bottom() && text.pos.x < plot.right() => {
                    Some(text)
                }
                _ => None,
            })
            .collect();
        let bolds = labels.iter().any(|text| {
            text.galley
                .job
                .sections
                .iter()
                .any(|section| section.format.coords != Default::default())
        });
        assert_eq!(bolds, bold, "{characters} characters");
        let borders = painted.iter().any(|shape| matches!(shape, Shape::LineSegment { points, stroke } if stroke.color == egui::Color32::RED && points[0].y == points[1].y && (points[0].y - plot.bottom()).abs() < 1.0 && points[1].x - points[0].x == plot.width()));
        assert_eq!(borders, border);
        let mut centres: Vec<_> = labels
            .iter()
            .map(|text| text.visual_bounding_rect().center().x)
            .collect();
        centres.sort_by(f32::total_cmp);
        assert!(centres.len() >= 2, "{centres:?}");
        let gap = centres
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .fold(f32::INFINITY, f32::min);
        assert_eq!(gap >= 200.0, characters == 20, "{centres:?}");
    }
}

/// With fewer than two bars in view, on either side of the bars, time labels keep the daily
/// precision of the bars, and the legend, the price axis at its fitted range, the grid, the
/// time axis and the scale settings button stay.
#[test]
fn views_past_the_bars_keep_the_chart_readable() {
    use crate::model::{LineStyle, LogicalRange};
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0]);
    chart.config.show_volume = false;
    // Solid grid lines in a colour nothing else uses are line segments the test can find.
    chart.config.grid_line_style = LineStyle::Solid;
    chart.config.grid_color = egui::Color32::from_rgb(1, 2, 3);
    let grid_color = chart.config.grid_color;
    frame(&ctx, &mut chart, 0.0, vec![]);
    let fitted = chart.get_chart_mapping().price.range();
    // The first bar opens at 2023-11-14 22:13:20 UTC, and the bars are a day apart.
    let first = chart.state.data().bars[0].time;
    // (left, right, bars in view)
    for (row, (left, right, in_view)) in [
        (40.5, 80.5, 42),
        (-30.25, -0.25, 1),
        (-60.0, -20.0, 0),
        (110.0, 150.0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let view = format!("{left}..{right}");
        assert!(chart.set_visible_logical_range(LogicalRange::new(left, right)));
        assert_eq!(chart.state.visible_data().len(), in_view, "{view}");
        assert_eq!(chart.format_time(first), "Nov 14, 2023", "{view}");
        let time = 1.0 + row as f64;
        let painted = shapes(&ctx, &mut chart, time, vec![]);
        let plot = chart.get_rendered_price_rect();
        let texts: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) => Some((text.pos, text.galley.text().to_owned())),
                _ => None,
            })
            .collect();
        let grid = |vertical: bool| {
            painted.iter().any(|shape| {
                matches!(shape,
                Shape::LineSegment { points, stroke }
                    if stroke.color == grid_color && (points[0].x == points[1].x) == vertical)
            })
        };
        assert!(
            texts.iter().any(|(pos, text)| pos.x < plot.right()
                && pos.y < plot.bottom()
                && text.starts_with("O ")),
            "legend: {view}"
        );
        assert!(
            texts
                .iter()
                .any(|(pos, _)| pos.x >= plot.right() && pos.y < plot.bottom()),
            "price axis: {view}"
        );
        assert_eq!(chart.get_chart_mapping().price.range(), fitted, "{view}");
        assert!(grid(false), "price grid: {view}");
        assert!(
            texts
                .iter()
                .any(|(pos, _)| pos.x < plot.right() && pos.y > plot.bottom()),
            "time axis: {view}"
        );
        assert!(grid(true), "time grid: {view}");
        #[cfg(feature = "icons")]
        {
            let corner = Pos2::new(
                plot.right() + chart.config.right_axis_width / 2.0,
                plot.bottom() + 14.0,
            );
            for (step, event) in [
                Event::PointerMoved(corner),
                navigation_press(corner, true),
                navigation_press(corner, false),
                Event::PointerGone,
            ]
            .into_iter()
            .enumerate()
            {
                frame(
                    &ctx,
                    &mut chart,
                    time + 0.1 * (step + 1) as f64,
                    vec![event],
                );
            }
            assert!(
                chart.take_scale_settings_request(),
                "settings button: {view}"
            );
        }
    }
    // A half-point spacing merges pairs. An empty view has the same labels and key
    // count whether the last pair is complete (1,000 bars) or partial (1,001).
    use crate::model::{Bar, BarData};
    use crate::scales::{
        CustomTimeFormatter, HorizontalScale, TickMark, TickMarkLayout, TimeFormatter,
    };
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Recorder {
        keys: Arc<AtomicUsize>,
        formatter: Box<dyn TimeFormatter>,
    }
    impl HorizontalScale for Recorder {
        fn tick_marks(
            &self,
            points: &[(DateTime<Utc>, usize)],
            layout: &TickMarkLayout,
        ) -> Vec<TickMark> {
            self.keys.store(points.len(), Ordering::Relaxed);
            self.formatter.tick_marks(points, layout)
        }
        fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
            self.formatter.as_ref().format_crosshair(time, bar_seconds)
        }
    }
    for (left, right) in [(2427.0, 3854.0), (28131.0, 29558.0)] {
        let mut control = None;
        for count in [1000, 1001] {
            let start = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
            let bars = (0..count)
                .map(|day| Bar::new(start + Duration::days(day), 100.0, 110.0, 100.0, 110.0, 1.0))
                .collect();
            let ctx = egui::Context::default();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.config.show_volume = false;
            let mut options = chart.chart_options().clone();
            options.time_scale.min_bar_spacing = 0.01;
            chart.set_chart_options(options);
            chart.set_time_formatter(Some(Arc::new(CustomTimeFormatter::new(|time, _| {
                time.format("%Y-%m-%d").to_string()
            }))));
            let keys = Arc::new(AtomicUsize::new(0));
            chart.set_horizontal_scale(Some(Arc::new(Recorder {
                keys: keys.clone(),
                formatter: chart.build_time_formatter(),
            })));
            frame(&ctx, &mut chart, 0.0, vec![]);
            assert!(chart.set_visible_logical_range(LogicalRange::new(left, right)));
            assert!(chart.state.visible_data().is_empty());
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.pos.y > plot.bottom() && text.pos.x < plot.right() =>
                    {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            let keys = keys.load(Ordering::Relaxed);
            let row = format!("{count} bars, {left}..{right}, {keys} keys");
            let range = chart.state.time_scale().visible_logical_range();
            assert!(
                (range.left - left).abs() < 0.01 && (range.right - right).abs() < 0.01,
                "{row}: {range:?}"
            );
            assert_eq!(chart.merge_groups.factor(), 2, "{row}");
            assert!(labels.len() >= 3, "{row}: {labels:?}");
            if let Some((expected_keys, expected_labels)) = &control {
                assert_eq!(&labels, expected_labels, "{row}");
                assert!(
                    keys <= *expected_keys + 1,
                    "{row}: {keys} keys vs {expected_keys}"
                );
            } else {
                control = Some((keys, labels));
            }
        }
    }
}

#[test]
fn initial_axis_passes_keep_zoom_and_later_measurements_keep_range() {
    use crate::model::Instrument;
    let ctx = egui::Context::default();
    let mut chart = chart(&[100.0], &[110.0]);
    chart.config.show_volume = false;
    chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(2)));
    let initial_view = chart.get_time_scale_state();
    let input = || egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    };
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::ZERO),
            ..Default::default()
        },
        |ui| {
            chart.show(ui);
        },
    )
    .drop_without_applying_deltas();
    let mut passes = 0;
    ctx.run_ui(input(), |ui| {
        chart.show(ui);
        passes += 1;
        if ui.ctx().current_pass_index() == 0 {
            // Metadata arriving during startup can widen labels on the discarded pass.
            chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(6)));
            ui.ctx().request_discard("instrument metadata arrived");
        }
    })
    .drop_without_applying_deltas();
    assert!(passes > 1);
    assert_eq!(chart.get_time_scale_state(), initial_view);

    ctx.run_ui(input(), |ui| {
        chart.show(ui);
    })
    .drop_without_applying_deltas();
    let width = chart.get_rendered_price_rect().width();
    let spacing = chart.state.time_scale().bar_spacing();
    chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(10)));
    ctx.run_ui(input(), |ui| {
        chart.show(ui);
    })
    .drop_without_applying_deltas();
    let new_width = chart.get_rendered_price_rect().width();
    assert!(new_width < width, "later labels must widen the axis");
    // The plot spans `width - 1` pixels of slots, so both edges stay on the same times.
    let slots = |width: f32, spacing: f32| f64::from(width - 1.0) / f64::from(spacing);
    assert!(
        (slots(new_width, chart.state.time_scale().bar_spacing()) - slots(width, spacing)).abs()
            < 1e-4
    );
}

/// Extra columns share at most half of a narrow widget on either edge. Four 70-point columns
/// need 280 points, so 240 and 320 keep half for the plot and 800 fits all of them.
#[test]
fn extra_axis_columns_leave_narrow_plots_half_the_width() {
    use crate::model::{AxisSide, SeriesScale};
    use crate::widget::SeriesOptions;
    for side in [AxisSide::Left, AxisSide::Right] {
        for (width, plot) in [(240.0, 120.0), (320.0, 160.0), (800.0, 520.0)] {
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config.show_volume = false;
            chart.config.right_axis_width = 70.0;
            chart.config.left_axis_width = 70.0;
            for key in 0..3 {
                chart.add_series(SeriesOptions {
                    scale: SeriesScale::Separate(key),
                    ..Default::default()
                });
            }
            assert!(chart.place_pane_scales(crate::widget::PaneId::MAIN, side));
            let layout = chart
                .calculate_layout_rects(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 450.0)));
            assert_eq!(layout.price_rect.width(), plot, "{side:?} at {width}");
        }
    }
}

#[test]
fn plot_uses_the_interior_and_configured_axis_widths() {
    for (width, height, padding, left, right, studies, left_gutter) in [
        (800.0, 600.0, 0.0, 0.0, 70.0, 0, 0.0),
        (640.0, 450.0, 2.0, 50.0, 80.0, 0, 0.0),
        (45.0, 20.0, 0.0, 0.0, 70.0, 0, 0.0),
        (800.0, 600.0, 8.0, 0.0, 0.0, 0, 0.0),
        (240.0, 450.0, 0.0, 0.0, 70.0, 3, 85.0),
        (320.0, 450.0, 0.0, 50.0, 70.0, 3, 125.0),
        (800.0, 600.0, 0.0, 0.0, 70.0, 1, 70.0),
    ] {
        let mut chart = chart(&[100.0], &[110.0]);
        for _ in 0..studies {
            chart.add_indicator(Box::new(
                crate::studies::CustomIndicator::new("Independent", |_| Vec::new())
                    .with_scale(crate::studies::StudyScale::Independent),
            ));
        }
        chart.config.padding = padding;
        chart.config.show_left_axis = left > 0.0;
        chart.config.left_axis_width = left;
        chart.config.show_right_axis = right > 0.0;
        chart.config.right_axis_width = right;
        let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, height));
        let layout = chart.calculate_layout_rects(widget);
        assert!(widget.contains_rect(layout.price_rect));
        assert_eq!(layout.price_rect.top(), widget.top() + padding);
        assert_eq!(
            layout.price_rect.left(),
            widget.left() + (padding + left_gutter).min(width)
        );
        assert_eq!(
            layout.price_rect.width(),
            (width - 2.0 * padding - right - left_gutter).max(0.0)
        );
        assert_eq!(
            layout.price_rect.height(),
            (height - 2.0 * padding - 35.0).max(0.0)
        );
    }
}

/// A Heikin-Ashi chart whose main series has no bars lays out the slots of its added series. Its
/// price line has no transformed bar to read, whichever bar it follows, and the frame still
/// paints the added series' line.
#[test]
fn heikin_last_visible_price_line_without_main_bars() {
    use crate::model::{Bar, BarData, ChartType, PriceLineSource};
    use crate::widget::{Chart, SeriesId, SeriesOptions};
    let start = chrono::DateTime::UNIX_EPOCH;
    let color = egui::Color32::from_rgb(1, 2, 3);
    for source in [PriceLineSource::LastBar, PriceLineSource::LastVisible] {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::new());
        chart.config_mut().show_volume = false;
        chart.set_chart_type(ChartType::Heikin);
        chart
            .series_style_mut(SeriesId::MAIN)
            .unwrap()
            .price_line_source = source;
        let id = chart.add_series(SeriesOptions {
            color,
            ..Default::default()
        });
        chart.set_data(
            id,
            (0..3)
                .map(|i| {
                    let time = start + chrono::Duration::minutes(15 * i);
                    Bar::new(time, 10.0, 11.0, 9.0, 10.5, 1.0)
                })
                .collect(),
        );
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.05, vec![]);
        chart.fit_content();
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        assert!(
            painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                if path.stroke.color == egui::epaint::ColorMode::Solid(color))),
            "{source:?}: the added series' line is painted"
        );
    }
}
