use super::Chart;

impl Chart {
    /// Resolve the timeframe used to pick a session-break granularity.
    ///
    /// The chart's timeframe is the primary source. Without one, the cadence
    /// is inferred from the visible bars by taking the median gap between
    /// consecutive timestamps and snapping it to the nearest preset, so session
    /// breaks work even when the host never set a timeframe. Falls back to the
    /// default (1-minute) when there are too few bars to measure.
    pub(super) fn resolve_session_timeframe(
        &self,
        visible_data: &[crate::model::Bar],
    ) -> crate::model::Timeframe {
        use crate::model::Timeframe;

        if let Some(tf) = self.timeframe {
            return tf;
        }

        // Infer from the median inter-bar gap (robust to session-edge jumps).
        if visible_data.len() < 2 {
            return Timeframe::default();
        }
        // An even sample of the gaps keeps a zoomed-out frame from sorting all of them.
        let mut gaps_ms: Vec<i64> = visible_data
            .windows(2)
            .step_by((visible_data.len() / 1024).max(1))
            .map(|w| (w[1].time - w[0].time).num_milliseconds())
            .filter(|&g| g > 0)
            .collect();
        if gaps_ms.is_empty() {
            return Timeframe::default();
        }
        gaps_ms.sort_unstable();
        let median = gaps_ms[gaps_ms.len() / 2];

        // Snap to the closest preset by duration.
        Timeframe::all()
            .into_iter()
            .min_by_key(|tf| (tf.duration_ms() - median).abs())
            .unwrap_or_default()
    }
}

/// The sessions of `clock` that the bars `shown` of `bars` belong to, each from the open of its
/// first bar to the next session's.
#[cfg(feature = "studies")]
pub(in crate::widget) fn session_ranges(
    bars: &[crate::model::Bar],
    shown: std::ops::Range<usize>,
    clock: &crate::model::BarClock,
) -> Option<crate::studies::TradeProfileRanges> {
    use crate::model::SessionProvider;
    let in_view = bars.get(shown.clone())?;
    let (first, last) = (in_view.first()?, in_view.last()?);
    let sessions = clock.sessions();
    let starts: Vec<_> = std::iter::once(bars[..shown.start].partition_point(|bar| {
        sessions
            .session_break_between(&bar.time, &first.time)
            .is_some()
    }))
    .chain(
        sessions
            .session_breaks(in_view)
            .into_iter()
            .map(|(i, _)| shown.start + i),
    )
    .collect();
    let end = shown.end
        + bars[shown.end..].partition_point(|bar| {
            sessions
                .session_break_between(&last.time, &bar.time)
                .is_none()
        });
    let close = bars
        .get(end)
        .map_or(chrono::DateTime::<chrono::Utc>::MAX_UTC, |bar| {
            clock.bar_open(bar.time)
        });
    let ends = starts
        .iter()
        .skip(1)
        .map(|&next| clock.bar_open(bars[next].time));
    let ranges = starts
        .iter()
        .zip(ends.chain([close]))
        .map(|(&start, end)| clock.bar_open(bars[start].time)..end)
        .collect();
    crate::studies::TradeProfileRanges::new(ranges).ok()
}

#[cfg(test)]
mod tests {
    use crate::widget::test_frames::shapes;
    #[cfg(feature = "studies")]
    use egui::Event;
    use egui::Shape;
    #[cfg(any(feature = "timezones", feature = "studies"))]
    use egui::{Pos2, Rect};

    #[test]
    fn tpo_days_follow_the_instrument_zone_without_trading_hours() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe, TimezoneId};
        // Half-hourly bars from 02:00 to 08:00 UTC, across midnight in New York at 05:00 UTC
        let start: chrono::DateTime<chrono::Utc> = "2024-03-05T02:00:00Z".parse().unwrap();
        let bars = (0..13)
            .map(|i| {
                let time = start + chrono::Duration::minutes(30 * i);
                Bar::new(time, 100.0, 102.0, 99.0, 101.0, 1.0)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let new_york = TimezoneId::fixed_offset(-5 * 3600).unwrap();
        chart.set_instrument(Some(Instrument::new("FX").with_time_zone(new_york)));
        let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
        // One initial balance bar beside each profile
        let initial_balance = crate::tokens::DESIGN_TOKENS.semantic.tpo.initial_balance;
        let profiles = painted
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect(rect)
                    if rect.fill == initial_balance && rect.rect.width() == 2.0)
            })
            .count();
        assert_eq!(profiles, 2);
    }

    #[cfg(feature = "studies")]
    #[test]
    fn tpo_rows_follow_the_instrument_tick_in_the_chart_type_and_the_study() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe};
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        // A spans 5000.00 to 5000.25 and B 5000.25 to 5000.50. On rows of one 0.25 tick, A alone
        // holds 5000.00 and B alone 5000.50, so the profile highlights two single prints.
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T14:30:00Z".parse().unwrap();
        let single_print = crate::tokens::DESIGN_TOKENS.semantic.tpo.single_print;
        for study in [false, true] {
            let bars = [(5000.0, 5000.25), (5000.25, 5000.5)]
                .into_iter()
                .enumerate()
                .map(|(i, (low, high))| {
                    let time = start + chrono::Duration::minutes(30 * i as i64);
                    Bar::new(time, low, high, low, high, 1.0)
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min30);
            chart.set_instrument(Some(Instrument::new("ES").with_tick_size(0.25)));
            if study {
                let mut tpo = TimePriceOpportunity::default();
                tpo.set_input("single_prints", InputValue::Toggle(true))
                    .unwrap();
                chart.add_indicator(Box::new(tpo));
            } else {
                chart.set_chart_type(ChartType::TimePriceOpportunity);
                chart.set_chart_type_params(crate::model::ChartTypeParams {
                    tpo_single_prints: true,
                    ..Default::default()
                });
            }
            let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
            let highlighted = painted
                .iter()
                .filter(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == single_print))
                .count();
            assert_eq!(highlighted, 2, "study {study}");
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn tpo_study_draws_the_table_of_its_statistics_input() {
        use crate::model::{Bar, BarData, Timeframe};
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T02:00:00Z".parse().unwrap();
        for (statistics, tables) in [(false, 0), (true, 1)] {
            // One session wide enough to leave the table room before the plot's edge.
            let bars = (0..40)
                .map(|i| {
                    let time = start + chrono::Duration::minutes(30 * i);
                    Bar::new(time, 100.0, 102.0, 99.0, 101.0, 1.0)
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min30);
            let mut tpo = TimePriceOpportunity::default();
            tpo.set_input("statistics", InputValue::Toggle(statistics))
                .unwrap();
            chart.add_indicator(Box::new(tpo));
            let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
            let drawn = painted
                .iter()
                .filter(|s| matches!(s, Shape::Text(t) if t.galley.text() == "Rotation"))
                .count();
            assert_eq!(drawn, tables, "statistics {statistics}");
        }
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn configured_sessions_reach_breaks_and_extended_shading() {
        use crate::model::{Bar, BarData, Timeframe};
        let times = [
            "2024-03-08T23:00:00Z",
            "2024-03-11T07:59:00Z",
            "2024-03-11T08:00:00Z",
            "2024-03-11T13:00:00Z",
            "2024-03-11T13:30:00Z",
            "2024-03-11T20:00:00Z",
            "2024-03-11T23:59:00Z",
            "2024-03-12T00:00:00Z",
        ];
        for (shade, custom, metadata, expected) in [
            (true, false, false, vec![2]),
            (false, false, false, vec![2]),
            (true, true, false, vec![1, 6]),
            (true, false, true, vec![3]),
        ] {
            let ctx = egui::Context::default();
            let bars = times
                .iter()
                .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 1.0))
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Hour1);
            if metadata {
                use crate::model::{Instrument, Session, SessionWindow, TimezoneId};
                use chrono::{NaiveTime, Weekday::*};
                let weekdays = [Mon, Tue, Wed, Thu, Fri];
                let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
                // The instrument opens pre-market at 05:00, independent of the display zone.
                chart.set_instrument(Some(
                    Instrument::new("SESSION")
                        .with_time_zone(TimezoneId::named("America/New_York").unwrap())
                        .with_session(
                            Session::new([SessionWindow::new(
                                weekdays,
                                clock(9, 30),
                                clock(16, 0),
                            )])
                            .with_extended_windows([
                                SessionWindow::new(weekdays, clock(5, 0), clock(9, 30)),
                                SessionWindow::new(weekdays, clock(16, 0), clock(20, 0)),
                            ]),
                        ),
                ));
                chart.chart_options.time_scale.timezone =
                    crate::scales::TimezoneMode::Timezone("Europe/Zurich".parse().unwrap());
            } else {
                chart.config.session_config = Some(crate::config::SessionConfig {
                    show_premarket: true,
                    show_postmarket: true,
                    ..crate::config::SessionConfig::nyse()
                });
            }
            chart.config.show_session_breaks = true;
            chart.config.session_break_style = crate::model::LineStyle::Solid;
            chart.config.session_break_color = egui::Color32::from_rgb(201, 37, 180);
            chart.config.extended_hours_color =
                egui::Color32::from_rgba_unmultiplied(90, 40, 170, 30);
            chart.config.show_extended_hours = shade;
            if custom {
                chart.set_session_provider(Some(std::sync::Arc::new(
                    crate::model::DailySessionProvider::new(21, 0),
                )));
            }
            let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let lines: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == chart.config.session_break_color.gamma_multiply(0.6) =>
                    {
                        Some(points[0].x)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                lines,
                expected
                    .iter()
                    .map(|index| map.idx_to_x(*index))
                    .collect::<Vec<_>>()
            );
            let fills: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == chart.config.extended_hours_color => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = if shade {
                [(0, 1), (if metadata { 3 } else { 2 }, 4), (5, 7)]
                    .into_iter()
                    .map(|(first, end)| {
                        Rect::from_min_max(
                            Pos2::new(map.idx_to_x(first) - map.bar_spacing / 2.0, map.rect.top()),
                            Pos2::new(
                                map.idx_to_x(end - 1) + map.bar_spacing / 2.0,
                                map.rect.bottom(),
                            ),
                        )
                        .intersect(map.rect)
                    })
                    .collect()
            } else {
                vec![]
            };
            assert_eq!(fills, expected);
        }
    }

    /// Session volume profiles sum whole sessions, from the first bar of a session that is only
    /// partly shown up to the open of the next one, and the last session holds the live bar.
    #[cfg(feature = "studies")]
    #[test]
    fn session_ranges_cover_the_whole_sessions_shown() {
        use crate::model::{Bar, BarClock, Timeframe};
        let open = chrono::DateTime::parse_from_rfc3339("2026-01-05T20:00:00Z")
            .unwrap()
            .to_utc();
        let hour = |h| open + chrono::TimeDelta::hours(h);
        let bars: Vec<_> = (0..9)
            .map(|h| Bar::new(hour(h), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        let clock = BarClock::new(Timeframe::Hour1);
        let max = chrono::DateTime::<chrono::Utc>::MAX_UTC;
        for (shown, expected) in [
            (2..5, Some(vec![hour(0)..hour(4), hour(4)..max])),
            (0..2, Some(vec![hour(0)..hour(4)])),
            (6..6, None),
        ] {
            let ranges = super::session_ranges(&bars, shown.clone(), &clock);
            let ranges = ranges.map(|ranges| ranges.ranges().to_vec());
            assert_eq!(ranges, expected, "{shown:?}");
        }
    }

    /// A visible closing bar still draws the full distribution. Autoscale fits its row edges,
    /// range edits select the same distributions as painting, and manual cropping retains levels.
    #[cfg(feature = "studies")]
    #[test]
    fn session_distributions_fit_their_rows_and_keep_cropped_levels_readable() {
        use crate::chart::overlays::{FootprintMode, TradeProfileSettings};
        use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
        use crate::model::{Bar, ChartType, Instrument, OrderSide, Tick, Timeframe};
        use std::sync::Arc;
        struct Feed(Vec<Bar>);
        impl Datafeed for Feed {
            fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
                reply.send(Ok(Instrument::new(symbol).with_tick_size(0.25)));
            }
            fn subscribe(&self, subscription: Subscription) {
                for bar in &self.0 {
                    subscription.sink.push_tick(Tick::new(
                        bar.time,
                        bar.close,
                        bar.volume,
                        Some(OrderSide::Buy),
                    ));
                }
            }
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                reply.send(Ok(HistoryPage::new(self.0.clone(), true)));
            }
        }
        let open: chrono::DateTime<chrono::Utc> = "2026-01-05T00:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..16)
            .map(|i| {
                let time =
                    open + chrono::TimeDelta::days(i / 8) + chrono::TimeDelta::minutes(i % 8 * 30);
                let price = [100.0, 100.0, 100.0, 100.0, 110.0, 130.0, 150.0, 160.0]
                    [i as usize % 8]
                    - if i < 8 { 50.0 } else { 0.0 };
                Bar::new(
                    time,
                    price,
                    price,
                    price,
                    price,
                    if i % 8 < 4 { 9.0 } else { 1.0 },
                )
            })
            .collect();
        // On the last day 100 holds 36 of 40 volume units, and four of eight TPOs.
        // Its 0.25 row spans 99.875..100.125; the last row spans 159.875..160.125.
        for (kind, mode, letters) in [
            (
                ChartType::SessionVolumeProfile,
                FootprintMode::BidAsk,
                false,
            ),
            (
                ChartType::SessionVolumeProfile,
                FootprintMode::Volume,
                false,
            ),
            (ChartType::SessionVolumeProfile, FootprintMode::Delta, false),
            (
                ChartType::SessionVolumeProfile,
                FootprintMode::DeltaVolume,
                false,
            ),
            (
                ChartType::TimePriceOpportunity,
                FootprintMode::Volume,
                false,
            ),
            (ChartType::TimePriceOpportunity, FootprintMode::Volume, true),
        ] {
            for (width, theme) in [800.0, 1440.0].into_iter().flat_map(|width| {
                [crate::theme::Theme::light(), crate::theme::Theme::dark()]
                    .map(|theme| (width, theme))
            }) {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::ChartBuilder::new()
                    .with_symbol("SESSION")
                    .with_timeframe(Timeframe::Min30)
                    .with_feed(Arc::new(Feed(bars.clone())))
                    .build();
                for _ in 0..3 {
                    shapes(&ctx, &mut chart, 0.0, vec![]);
                }
                chart.set_instrument(Some(Instrument::new("SESSION").with_tick_size(0.25)));
                chart.set_chart_type(kind);
                chart.set_chart_type_params(crate::model::ChartTypeParams {
                    tpo_letters: letters,
                    ..Default::default()
                });
                chart.set_trade_profile_settings(TradeProfileSettings {
                    mode,
                    ..Default::default()
                });
                let draw = |chart: &mut crate::widget::Chart, pointer: Option<Pos2>| {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            events: pointer.into_iter().map(Event::PointerMoved).collect(),
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(width, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                        },
                    );
                    output.textures_delta.clear();
                    output.shapes
                };
                let mut last_day: Option<crate::scales::PriceRange> = None;
                for (count, start) in [(16, 0), (8, 8), (1, 15)] {
                    chart.set_visible_bars(count);
                    chart.set_start_idx(start);
                    draw(&mut chart, None);
                    let map = chart.get_chart_mapping();
                    for price in [99.875, 160.125] {
                        assert!(
                            map.price
                                .range()
                                .contains(map.price.transform().to_scale(price)),
                            "{kind:?} {mode:?} letters={letters} count={count} width={width}: {price}"
                        );
                    }
                    if count <= 8 {
                        let range = map.price.range();
                        // At 10% margins each side: 60.25 / 0.8 = 75.3125 for executed rows;
                        // TPO rows of 2 span 99.875..161.875, so 62 / 0.8 = 77.5.
                        let (low, high) = if kind == ChartType::SessionVolumeProfile {
                            (92.34375, 167.65625)
                        } else {
                            (92.125, 169.625)
                        };
                        assert!(
                            (range.min - low).abs() < 1e-9 && (range.max - high).abs() < 1e-9,
                            "{kind:?} fits its rows with one margin: {range:?}"
                        );
                        if let Some(previous) = last_day {
                            assert_eq!(
                                range, previous,
                                "the same session keeps its price range when time narrows"
                            );
                        }
                        last_day = Some(range);
                    }
                }
                if kind == ChartType::SessionVolumeProfile {
                    assert!(chart.merge_trade_profile(0));
                    draw(&mut chart, None);
                    assert!(
                        chart.state.price_scale().range().contains(49.875),
                        "merged earlier session is fitted"
                    );
                    assert!(chart.split_trade_profile(0, bars[8].time));
                    draw(&mut chart, None);
                    assert!(
                        chart.state.price_scale().range().min > 90.0,
                        "offscreen split profile is excluded"
                    );
                } else {
                    let mut params = chart.chart_type_params().clone();
                    params.tpo_composites = vec![bars[0].time..=bars[15].time];
                    chart.set_chart_type_params(params);
                    draw(&mut chart, None);
                    assert!(
                        chart.state.price_scale().range().contains(49.875),
                        "TPO composite is fitted"
                    );
                }
                chart.price_scale_mut().set_range(150.0, 162.0);
                let painted = draw(&mut chart, None);
                assert_eq!(
                    chart.state.price_scale().range(),
                    crate::scales::PriceRange::new(150.0, 162.0)
                );
                let session = chart.format_time(
                    bars[if kind == ChartType::SessionVolumeProfile {
                        8
                    } else {
                        0
                    }]
                    .time,
                );
                assert!(
                    painted.iter().any(|s| matches!(&s.shape, Shape::Text(t)
                    if t.galley.text().starts_with(&session) && t.galley.text().contains("POC 100.00") && t.galley.text().contains("VAH")
                        && t.galley.text().contains("VAL") && t.galley.text().contains("below"))),
                    "cropped {kind:?} retains its central levels"
                );
                let readout = painted
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text().starts_with(&session) => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let volume_bottom = painted
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text().starts_with("Vol") => {
                            Some(text.pos.y + text.galley.size().y)
                        }
                        _ => None,
                    })
                    .fold(0.0, f32::max);
                assert!(
                    readout.pos.y > volume_bottom,
                    "cropped profile context clears the legend"
                );
                chart.set_visible_bars(1);
                chart.set_start_idx(0);
                draw(&mut chart, None);
                assert_eq!(
                    chart.state.price_scale().range(),
                    crate::scales::PriceRange::new(150.0, 162.0)
                );
                chart.apply_synced_time_scale(2.0, 0.0);
                let compact = draw(&mut chart, None);
                let hint = compact
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text() == "i" => {
                            Some(text.pos + text.galley.size() * 0.5)
                        }
                        _ => None,
                    })
                    .expect("compact cropped profiles advertise their hover readout");
                ctx.all_styles_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
                let mut hovered = Vec::new();
                for frame in 0..3 {
                    let mut corrected = bars[14].clone();
                    corrected.volume += frame as f64 + 1.0;
                    chart.update(crate::widget::SeriesId::MAIN, corrected);
                    hovered = draw(&mut chart, Some(hint));
                }
                assert!(
                    hovered.iter().any(|s| matches!(&s.shape, Shape::Text(text)
                        if text.galley.text().contains("POC")
                            && text.galley.text().contains("below"))),
                    "compact cropped levels remain readable over row tooltips: {kind:?} {mode:?} width={width} dark={}",
                    theme.dark
                );
                if kind == ChartType::TimePriceOpportunity {
                    for (prices, expected) in [
                        (
                            [100.0, 100.0],
                            crate::scales::PriceRange::new(99.875, 100.125),
                        ),
                        // Rows of 2 cover 7..73; rows of 2.5 cover 8.75..71.25. Starting over
                        // would alternate between spans 66/0.8 = 82.5 and 62.5/0.8 = 78.125.
                        ([8.9, 71.0], crate::scales::PriceRange::new(-1.25, 81.25)),
                    ] {
                        let mut chart =
                            crate::widget::Chart::new(crate::model::BarData::from_bars(
                                bars[8..]
                                    .iter()
                                    .enumerate()
                                    .map(|(i, bar)| Bar::from_value(bar.time, prices[i % 2]))
                                    .collect(),
                            ));
                        chart.set_chart_type(kind);
                        chart.set_chart_type_params(crate::model::ChartTypeParams {
                            tpo_letters: letters,
                            ..Default::default()
                        });
                        chart.set_visible_bars(8);
                        chart.set_start_idx(0);
                        draw(&mut chart, None);
                        assert_eq!(
                            chart.state.price_scale().range(),
                            expected,
                            "automatic rows converge and preserve flat-session bounds: {prices:?}"
                        );
                    }
                }
            }
        }
    }

    /// A session that opens before the first complete footprint bar counts only the bars from
    /// it and is marked partial, so the bar the store cut, drawn as a candle, counts nowhere.
    #[cfg(feature = "studies")]
    #[test]
    fn session_profile_counts_only_complete_footprint_bars() {
        use crate::data::Footprint;
        use crate::model::OrderSide::{Buy, Sell};
        use crate::model::{Bar, BarClock, Tick, Timeframe};
        let t0: chrono::DateTime<chrono::Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let m = chrono::TimeDelta::minutes(1);
        let trades = [
            Tick::new(t0 + m, 100.0, 5.0, Some(Sell)),
            Tick::new(
                t0 + m + chrono::TimeDelta::seconds(1),
                101.0,
                3.0,
                Some(Buy),
            ),
            Tick::new(t0 + m * 2, 102.0, 2.0, Some(Buy)),
        ];
        let clock = BarClock::new(Timeframe::Min1);
        let mut flow = Footprint::new(clock.clone(), 1.0, 1).unwrap();
        for trade in &trades {
            flow.push(trade).unwrap();
        }
        // The store dropped the sell of 5, so the 10:01 bar keeps only the buy of 3.
        flow.drop_through(trades[0].ts, &trades[1..]);
        let bars: Vec<_> = (0..3)
            .map(|i| Bar::new(t0 + m * i, 100.0, 102.0, 100.0, 101.0, 10.0))
            .collect();
        // As the frame splits candles from bars drawn from executions.
        let candles = bars.partition_point(|bar| bar.time < flow.complete_from().unwrap());
        let ranges = super::session_ranges(&bars, candles..bars.len(), &clock).unwrap();
        let profile = &crate::studies::trade_profiles(&flow, &ranges)[0];
        assert_eq!(
            (profile.volume.volume(), profile.volume.poc()),
            (2.0, Some(102))
        );
        assert!(!profile.complete);
    }
}
