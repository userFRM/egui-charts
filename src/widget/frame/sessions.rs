use super::Chart;
use std::borrow::Cow;

impl Chart {
    /// The session break dividers in the chart's colour, width and pattern.
    pub(in crate::widget) fn session_break_renderer(
        &self,
    ) -> crate::chart::rendering::SessionBreakRenderer {
        crate::chart::rendering::SessionBreakRenderer::new(
            crate::chart::rendering::SessionBreakRenderConfig {
                line_color: self.config.session_break_color,
                line_width: self.config.session_break_width,
                style: self.config.session_break_style,
            },
        )
    }

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

    /// The calendar that divides the bars into profiles of the hours `selection` picks, and the
    /// choice that then keeps only the bars of those hours
    /// ([`ProfileSession::profiled`](crate::model::ProfileSession::profiled)): `selection` on
    /// intraday bars, and [`ProfileSession::Chart`](crate::model::ProfileSession::Chart) on daily
    /// and longer bars, which carry a trading date rather than a time of day. Regular and
    /// extended hours still divide daily bars by their trading dates. The other choices on daily
    /// bars, and a choice without the instrument hours it reads, keep the chart's calendar, in
    /// the instrument's zone when it has no hours.
    pub(crate) fn profile_hours<'a>(
        &self,
        selection: &'a crate::model::ProfileSession,
    ) -> (
        crate::model::BarClock,
        Cow<'a, crate::model::ProfileSession>,
    ) {
        use crate::model::{ProfileSession, Timeframe, TimezoneId};
        let mut clock = self.bar_clock();
        let instrument = self.instrument();
        // Daily and longer bars carry a trading date rather than a time of day to divide or re-zone.
        let intraday =
            self.timeframe.unwrap_or_default().duration_ms() < Timeframe::Day1.duration_ms();
        let zone = instrument
            .and_then(|i| i.time_zone.clone())
            .unwrap_or_else(TimezoneId::utc);
        let hours = instrument.and_then(|i| i.session.as_ref());
        if intraday && let ProfileSession::Custom(custom) = selection {
            if hours.is_none() {
                clock = clock.with_timezone(zone.clone());
            }
            return (
                clock,
                Cow::Owned(ProfileSession::Custom(custom.with_zone(&zone))),
            );
        }
        let by_time_of_day = !matches!(
            selection,
            ProfileSession::Regular | ProfileSession::Extended
        );
        if (intraday || !by_time_of_day)
            && let Some(divided) = selection.session_clock(&clock, &zone, hours)
        {
            // Daily and longer bars keep every bar of their trading dates.
            let kept = if intraday {
                selection
            } else {
                &ProfileSession::Chart
            };
            return (divided, Cow::Borrowed(kept));
        }
        // Without instrument hours there is nothing to select among, so the chart's calendar stays.
        if let Some(zone) = instrument
            .filter(|i| i.session.is_none())
            .and_then(|i| i.time_zone.as_ref())
        {
            clock = clock.with_timezone(zone.clone());
        }
        (clock, Cow::Borrowed(&ProfileSession::Chart))
    }

    /// The instrument's regular hours on this interval, absent when it supplies no hours.
    pub(crate) fn regular_hours_clock(&self) -> Option<crate::model::BarClock> {
        let instrument = self.instrument()?;
        let hours = instrument.session.as_ref()?;
        let zone = instrument
            .time_zone
            .clone()
            .unwrap_or_else(crate::model::TimezoneId::utc);
        Some(
            crate::model::BarClock::new(self.timeframe.unwrap_or_default()).with_session(
                zone,
                hours.clone(),
                false,
            ),
        )
    }

    /// Resolve a TPO study's custom zones and the regular calendar it may clip to.
    pub(crate) fn tpo_division(
        &self,
        config: &crate::model::TPOConfig,
    ) -> (crate::model::BarClock, crate::model::TPOConfig) {
        let (clock, session) = self.profile_hours(&config.session);
        let config = crate::model::TPOConfig {
            session: session.into_owned(),
            regular_hours: self.regular_hours_clock(),
            ..config.clone()
        };
        (clock, config)
    }

    /// The calendar and settings the TPO chart type draws with: its parameters at the
    /// instrument's tick, their hours resolved as a TPO study's are.
    pub(crate) fn tpo_chart_division(&self) -> (crate::model::BarClock, crate::model::TPOConfig) {
        let tick = self
            .instrument()
            .and_then(|instrument| instrument.tick_size);
        self.tpo_division(&self.chart_type_params.tpo_config(tick))
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

    /// The TPO chart type's hours choice reaches its painted profiles: pre-market only keeps
    /// the six bars from 04:00 to 09:00 New York time, and two custom windows from 05:00 to 08:00
    /// and 12:00 to 14:00 profile their bars apart, each counting its periods from its own open.
    #[test]
    fn tpo_chart_type_profiles_the_hours_it_chooses() {
        use crate::model::{
            Bar, BarData, ChartType, Instrument, Session, SessionWindow, Timeframe, TimezoneId,
        };
        use chrono::{NaiveTime, Weekday::*};
        let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let week = [Mon, Tue, Wed, Thu, Fri];
        let instrument = Instrument::new("NQ")
            .with_tick_size(1.0)
            .with_time_zone(TimezoneId::fixed_offset(-5 * 3600).unwrap())
            .with_session(
                Session::new([SessionWindow::new(week, at(10), at(16))]).with_extended_windows([
                    SessionWindow::new(week, at(4), at(10)),
                    SessionWindow::new(week, at(16), at(20)),
                ]),
            );
        // Hourly bars from 04:00 New York time on a Tuesday, bar i trading only 100 + i.
        let start: chrono::DateTime<chrono::Utc> = "2024-03-05T09:00:00Z".parse().unwrap();
        let custom = serde_json::json!({"Custom": {"sessions": [
            {"start": "05:00:00", "end": "08:00:00", "zone": null},
            {"start": "12:00:00", "end": "14:00:00", "zone": null}
        ]}});
        // (saved hours, painted letters with their rows)
        let rows: [(serde_json::Value, &[(&str, i64)]); 2] = [
            (
                serde_json::json!("PreMarket"),
                &[
                    ("A", 100),
                    ("B", 101),
                    ("C", 102),
                    ("D", 103),
                    ("E", 104),
                    ("F", 105),
                ],
            ),
            (
                custom,
                &[("A", 101), ("A", 108), ("B", 102), ("B", 109), ("C", 103)],
            ),
        ];
        for (hours, expected) in rows {
            let bars = (0..16)
                .map(|i| {
                    let price = 100.0 + i as f64;
                    let time = start + chrono::Duration::hours(i);
                    Bar::new(time, price, price, price, price, 1.0)
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Hour1);
            chart.set_instrument(Some(instrument.clone()));
            chart.config_mut().show_ohlc_info = false;
            chart.set_chart_type(ChartType::TimePriceOpportunity);
            let saved =
                serde_json::json!({"tpo_row_ticks": 1, "tpo_letters": true, "tpo_session": hours});
            chart.set_chart_type_params(serde_json::from_value(saved).unwrap());
            let ctx = egui::Context::default();
            shapes(&ctx, &mut chart, 0.0, vec![]);
            chart.price_scale_mut().set_range(90.0, 130.0);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let map = chart.get_chart_mapping();
            let mut letters: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.galley.text().len() == 1
                            && text.galley.text().chars().all(|c| c.is_ascii_uppercase()) =>
                    {
                        // Off the rows, such as the legend's symbol badge, is no profile letter.
                        let y = text.visual_bounding_rect().center().y;
                        let row = (90..=130).find(|&p| (map.price_to_y(p as f64) - y).abs() < 1.0);
                        Some((text.galley.text().to_owned(), row?))
                    }
                    _ => None,
                })
                .collect();
            letters.sort_unstable();
            let expected: Vec<_> = expected
                .iter()
                .map(|&(letter, row)| (letter.to_owned(), row))
                .collect();
            assert_eq!(letters, expected, "hours {hours}");
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn tpo_rows_match_in_the_chart_type_and_the_study() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe};
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        // The bars span 4.5, 18 ticks of 0.25: 0.225 ticks per row over 80 rows, rounded up to
        // rows of 5 ticks (1.25). A holds the row of 5000, B the rows of 5000 to 5002.5, C the row
        // of 5002.5 and D the row of 5003.75, so the rows of 5001.25 and 5003.75 hold one TPO each
        // and both paths highlight two single prints.
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T14:30:00Z".parse().unwrap();
        let single_print = crate::tokens::DESIGN_TOKENS.semantic.tpo.single_print;
        for study in [false, true] {
            let bars = [
                (5000.0, 5001.0),
                (5000.0, 5003.5),
                (5002.5, 5003.5),
                (5003.75, 5004.5),
            ]
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

    /// Automatic rows come from the bars, so every vertical zoom paints the same profile. Bars 0
    /// to 12 span 6, 24 ticks of 0.25: 0.3 ticks per row over 80 rows, rounded up to rows of 5
    /// ticks (1.25). Bar 0 covers the rows of 5000 to 5005 and bars 1 to 12 the row of 5003.75,
    /// which holds 13 of the 17 TPOs and so is the whole value area, with 3 TPOs below and 1 above.
    #[test]
    fn automatic_tpo_rows_keep_the_profile_at_every_vertical_zoom() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe};
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T14:30:00Z".parse().unwrap();
        let bars = (0..13)
            .map(|i| {
                let (low, high) = if i == 0 {
                    (5000.0, 5006.0)
                } else {
                    (5004.0, 5004.5)
                };
                let time = start + chrono::Duration::minutes(30 * i);
                Bar::new(time, low, high, low, high, 1.0)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_instrument(Some(Instrument::new("ES").with_tick_size(0.25)));
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        chart.set_chart_type_params(
            serde_json::from_value(serde_json::json!({"tpo_statistics": true})).unwrap(),
        );
        let ctx = egui::Context::default();
        shapes(&ctx, &mut chart, 0.0, vec![]);
        for (low, high) in [(4999.0, 5007.0), (4990.0, 5016.0), (4950.0, 5060.0)] {
            chart.price_scale_mut().set_range(low, high);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let texts: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            // The statistics table's value area and TPO counts around the point of control.
            for value in ["5003.75 / 5003.75", "3 / 1"] {
                assert!(texts.contains(&value), "visible {low}..{high}: {texts:?}");
            }
        }
    }

    /// Rows of four 0.25 ticks: day one counts 2,2 (POC 100, VA 100..101);
    /// day two counts 2,3,2 (POC 111, VA 110..112). Only day two tags the scale.
    #[test]
    fn tpo_level_tags_mark_only_the_latest_profile() {
        use crate::model::{Bar, BarData, ChartType, ChartTypeParams, Instrument, Timeframe};
        use crate::widget::SeriesId;
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T14:30:00Z".parse().unwrap();
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let mut baseline = Vec::new();
            for tags in [false, true] {
                let bars = [
                    (0, 100.25, 101.0, 100.0, 100.75),
                    (30, 100.25, 101.0, 100.0, 100.75),
                    (1440, 110.5, 112.0, 110.0, 111.0),
                    (1470, 111.0, 112.0, 110.0, 111.5),
                    (1500, 111.0, 111.0, 111.0, 111.0),
                ]
                .into_iter()
                .map(|(m, o, h, l, c)| {
                    Bar::new(start + chrono::Duration::minutes(m), o, h, l, c, 1.0)
                })
                .collect();
                let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
                chart.set_timeframe(Timeframe::Min30);
                chart.set_instrument(Some(Instrument::new("ES").with_tick_size(0.25)));
                chart.set_chart_type(ChartType::TimePriceOpportunity);
                chart.set_chart_type_params(ChartTypeParams {
                    tpo_row_ticks: 4,
                    tpo_level_tags: tags,
                    ..Default::default()
                });
                chart.config_mut().show_volume = false;
                chart.config_mut().show_symbol_last_val = false;
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
                let plot = chart.get_rendered_price_rect();
                let mut tag_rects: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(r)
                            if r.rect.left() >= plot.right() - 1.0
                                && [theme.chart.axis_text, theme.chart.axis_text_secondary]
                                    .contains(&r.fill) =>
                        {
                            Some((r.rect, r.fill))
                        }
                        _ => None,
                    })
                    .collect();
                // The scale's existing controls can use the caption colours too.
                if !tags {
                    baseline = tag_rects;
                    continue;
                }
                tag_rects.retain(|tag| !baseline.contains(tag));
                // Exactly three newest-profile tags leave none for the earlier profile.
                assert_eq!(tag_rects.len(), 3, "background {:?}", theme.chart.bg);
                let map = chart.series_price_map(SeriesId::MAIN).unwrap();
                for (price, color) in [
                    (111.0, theme.chart.axis_text),
                    (112.0, theme.chart.axis_text_secondary),
                    (110.0, theme.chart.axis_text_secondary),
                ] {
                    assert!(
                        tag_rects.iter().any(|(r, fill)| *fill == color
                            && r.y_range().contains(map.price_to_y(price))),
                        "missing {price}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn tpo_study_draws_the_table_of_its_statistics_input() {
        use crate::model::{Bar, BarData, ChartType, TPOStatisticsRows, Timeframe};
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T02:00:00Z".parse().unwrap();
        let mut rows = TPOStatisticsRows::default();
        let rows: Vec<_> = rows
            .switches()
            .into_iter()
            .map(|(name, label, shown)| (name, label, *shown))
            .chain([
                ("value_area_pair", "VAL / VAH", true),
                ("tpo_pair", "TPO below / above", true),
            ])
            .collect();
        // Both halves of a pair on share one row; the other half alone draws its own.
        let pairs = [
            ("value_area_high", "value_area_low", "value_area_pair"),
            ("tpos_above", "tpos_below", "tpo_pair"),
        ];
        // Painted count of each row's label; `POC` is also a level caption.
        let paint = |study: bool, statistics: bool, flipped: Option<(&str, bool)>| {
            // One session wide enough to leave the table room before the plot's edge.
            let bars = (0..40)
                .map(|i| {
                    let time = start + chrono::Duration::minutes(30 * i);
                    Bar::new(time, 100.0, 102.0, 99.0, 101.0, 1.0)
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min30);
            if study {
                let mut tpo = TimePriceOpportunity::default();
                tpo.set_input("statistics", InputValue::Toggle(statistics))
                    .unwrap();
                if let Some((name, show)) = flipped {
                    tpo.set_input(&format!("statistics_{name}"), InputValue::Toggle(show))
                        .unwrap();
                }
                chart.add_indicator(Box::new(tpo));
            } else {
                chart.set_chart_type(ChartType::TimePriceOpportunity);
                let mut saved = serde_json::json!({"tpo_statistics": statistics});
                if let Some((name, show)) = flipped {
                    saved["tpo_statistics_rows"] = serde_json::json!({ name: show });
                }
                chart.set_chart_type_params(serde_json::from_value(saved).unwrap());
            }
            let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
            rows.iter()
                .map(|(_, label, _)| {
                    painted
                        .iter()
                        .filter(|s| matches!(s, Shape::Text(t) if t.galley.text() == *label))
                        .count()
                })
                .collect::<Vec<_>>()
        };
        for study in [false, true] {
            let rotation = rows
                .iter()
                .position(|(name, ..)| *name == "rotation_factor");
            for (statistics, tables) in [(false, 0), (true, 1)] {
                let counts = paint(study, statistics, None);
                assert_eq!(counts[rotation.unwrap()], tables, "statistics {statistics}");
            }
            let defaults = paint(study, true, None);
            let index = |name: &str| rows.iter().position(|(row, ..)| *row == name).unwrap();
            for (name, _, shown) in rows.iter().filter(|(name, ..)| !name.ends_with("_pair")) {
                let counts = paint(study, true, Some((name, !shown)));
                let mut expected = defaults.clone();
                match pairs.iter().find(|(a, b, _)| [a, b].contains(&name)) {
                    Some((a, b, pair)) => {
                        // Defaults draw the pair's row; switching one half off leaves the other.
                        expected[index(pair)] -= 1;
                        expected[index(if a == name { b } else { a })] += 1;
                    }
                    None if *shown => expected[index(name)] -= 1,
                    None => expected[index(name)] += 1,
                }
                assert_eq!(counts, expected, "{name}, study {study}");
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn tpo_settings_change_the_painted_profile_in_the_chart_type_and_the_study() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe};
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        use crate::theme::ThemeExt;
        let start: chrono::DateTime<chrono::Utc> = "2024-01-02T02:00:00Z".parse().unwrap();
        let paint = |study: bool, edits: &[(&str, InputValue)]| {
            let bars = (0..40)
                .map(|i| {
                    let (low, high) = match i {
                        0 => (100.0, 101.0),
                        1 => (101.0, 103.0),
                        _ => (99.0, 103.0),
                    };
                    Bar::new(
                        start + chrono::Duration::minutes(30 * i),
                        100.0_f64.clamp(low, high),
                        high,
                        low,
                        102.0_f64.clamp(low, high),
                        1.0,
                    )
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min30);
            chart.set_instrument(Some(Instrument::new("X").with_tick_size(1.0)));
            if study {
                let mut tpo = TimePriceOpportunity::default();
                tpo.set_input("row_ticks", InputValue::Length(1)).unwrap();
                tpo.set_input("display", InputValue::Choice(2)).unwrap();
                for (id, value) in edits {
                    tpo.set_input(id, value.clone()).unwrap();
                }
                chart.add_indicator(Box::new(tpo));
            } else {
                chart.set_chart_type(ChartType::TimePriceOpportunity);
                let mut saved = serde_json::json!({"tpo_row_ticks":1, "tpo_letters":true, "tpo_midpoint":false});
                for (id, value) in edits {
                    let InputValue::Toggle(show) = value else {
                        panic!("chart rows use toggles")
                    };
                    saved[format!("tpo_{id}")] = (*show).into();
                }
                chart.set_chart_type_params(serde_json::from_value(saved).unwrap());
            }
            let ctx = egui::Context::default();
            shapes(&ctx, &mut chart, 0.0, vec![]);
            chart.price_scale_mut().set_range(90.0, 115.0);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            (painted, chart.get_chart_mapping(), ctx.active_theme().chart)
        };
        for study in [false, true] {
            for setting in [
                "default",
                "poc",
                "value_area_levels",
                "initial_balance",
                "open_price",
                "close_price",
                "poor_high",
                "poor_low",
                "volume_profile",
            ] {
                let show = matches!(
                    setting,
                    "close_price" | "poor_high" | "poor_low" | "volume_profile"
                );
                let edits = if setting == "default" {
                    vec![]
                } else {
                    vec![(setting, InputValue::Toggle(show))]
                };
                let (painted, map, chart) = paint(study, &edits);
                let caption = |name| {
                    painted
                        .iter()
                        .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == name))
                };
                assert_eq!(caption("POC"), setting != "poc", "{setting}, study {study}");
                assert_eq!(
                    (caption("VAH"), caption("VAL")),
                    (
                        setting != "value_area_levels",
                        setting != "value_area_levels"
                    ),
                    "{setting}, study {study}"
                );
                let balance = painted.iter().filter(|s| matches!(s, Shape::Rect(r) if r.rect.width() == 2.0 && r.fill == crate::tokens::DESIGN_TOKENS.semantic.tpo.initial_balance)).count();
                assert_eq!(
                    balance,
                    usize::from(setting != "initial_balance"),
                    "{setting}, study {study}"
                );
                let markers: Vec<_> = painted
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Path(p) if p.points.len() == 3 && p.points[1].x == p.points[2].x => {
                            Some(p)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    markers.len(),
                    usize::from(setting != "open_price") + usize::from(setting == "close_price"),
                    "{setting}, study {study}"
                );
                for (key, price) in [("poor_high", 103.0), ("poor_low", 99.0)] {
                    let ticks = painted.iter().filter(|s| matches!(s, Shape::LineSegment { points, .. } if points[0].y == map.price_to_y(price) && points[1].x - points[0].x == 8.0)).count();
                    assert_eq!(
                        ticks,
                        usize::from(setting == key),
                        "{setting}, study {study}, {key}"
                    );
                }
                let color = chart.axis_text_secondary;
                let dim = chart.bg.lerp_to_gamma(color, 0.45);
                let volume = painted.iter().filter(|s| matches!(s, Shape::Rect(r) if (r.fill == color || r.fill == dim) && r.rect.width() > 2.0 && r.rect.width() <= 40.0)).count();
                assert_eq!(
                    volume,
                    if setting == "volume_profile" { 5 } else { 0 },
                    "{setting}, study {study}"
                );
                if setting == "close_price" {
                    let close = markers.last().unwrap();
                    assert_eq!(close.points[0].y, map.price_to_y(102.0));
                    assert!(close.points[0].x > markers[0].points[2].x);
                    assert_eq!(close.fill, chart.candle_up);
                }
            }
        }
        let yellow = egui::Color32::from_rgb(255, 200, 0);
        for opacity in [0, 45, 100] {
            let (painted, map, chart) = paint(
                true,
                &[
                    ("coloring", InputValue::Choice(1)),
                    ("block_color", InputValue::Color(yellow)),
                    ("display", InputValue::Choice(1)),
                    ("outside_value_area_opacity", InputValue::Length(opacity)),
                ],
            );
            let cells: Vec<_> = painted
                .iter()
                .filter_map(|s| match s {
                    Shape::Rect(r) if r.rect.width() > 2.0 && r.rect.width() < 24.0 => Some(r),
                    _ => None,
                })
                .collect();
            // Counts at 99..=103 are 38,39,40,39,39: the value area covers 100..=103.
            assert!(cells.iter().any(|r| r.fill == yellow));
            let outside = cells
                .iter()
                .find(|r| (r.rect.center().y - map.price_to_y(99.0)).abs() < 1.0)
                .unwrap();
            assert_eq!(
                outside.fill,
                chart.bg.lerp_to_gamma(yellow, opacity as f32 / 100.0)
            );
        }
        for (minutes, low, high) in [(60, 100.0, 103.0), (30, 100.0, 101.0)] {
            let (painted, map, _) = paint(
                true,
                &[("initial_balance_minutes", InputValue::Length(minutes))],
            );
            let balance = painted
                .iter()
                .find_map(|s| match s {
                    Shape::Rect(r)
                        if r.rect.width() == 2.0
                            && r.fill
                                == crate::tokens::DESIGN_TOKENS.semantic.tpo.initial_balance =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                (balance.top(), balance.bottom()),
                (map.price_to_y(high), map.price_to_y(low))
            );
        }
        let (painted, _, _) = paint(
            true,
            &[("initial_balance_extensions", InputValue::Toggle(true))],
        );
        assert_eq!(painted.iter().filter(|s| matches!(s, Shape::LineSegment { points, stroke } if points[1].x - points[0].x == 10.0 && stroke.color == crate::tokens::DESIGN_TOKENS.semantic.tpo.initial_balance)).count(), 4);
        let letter_x = |split| {
            let (painted, map, _) = paint(true, &[("split_periods", InputValue::Toggle(split))]);
            let x = |letter: &str, row: Option<f64>| {
                painted
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Text(t) if t.galley.text() == letter => {
                            let center = t.pos + t.galley.size() * 0.5;
                            row.is_none_or(|price| (center.y - map.price_to_y(price)).abs() < 1.0)
                                .then_some(center.x)
                        }
                        _ => None,
                    })
                    .min_by(f32::total_cmp)
                    .unwrap()
            };
            (x("B", None), x("B", Some(101.0)) - x("A", Some(101.0)))
        };
        let (packed, column) = letter_x(false);
        let split = letter_x(true).0;
        assert!(
            (split - packed - column).abs() < 0.001,
            "packed {packed}, split {split}, column {column}"
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn configured_sessions_reach_breaks_and_extended_shading() {
        use crate::model::{Bar, BarData, Timeframe};
        const PREMARKET: egui::Color32 = egui::Color32::from_rgba_premultiplied(40, 170, 90, 30);
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
        // `night` is the overnight switch of an instrument with overnight windows.
        for (shade, custom, metadata, night, pre, post, defaults, expected) in [
            (true, false, false, None, true, true, false, vec![2]),
            (false, false, false, None, true, true, false, vec![2]),
            (true, true, false, None, true, true, false, vec![1, 6]),
            (true, false, true, None, true, true, false, vec![3]),
            (true, false, true, Some(true), true, true, false, vec![1, 7]),
            (true, false, true, None, false, true, false, vec![3]),
            (true, false, true, None, true, false, false, vec![3]),
            (true, false, true, None, true, true, true, vec![3]),
            (false, false, true, None, true, true, true, vec![3]),
            (true, false, true, Some(true), true, true, true, vec![1, 7]),
            (true, false, true, Some(false), true, true, true, vec![1, 7]),
        ] {
            let overnight = night.is_some();
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
                            ])
                            .with_overnight_windows(if overnight {
                                vec![SessionWindow::new(weekdays, clock(20, 0), clock(5, 0))]
                            } else {
                                vec![]
                            }),
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
            if !defaults {
                chart.config.overnight_hours_color =
                    egui::Color32::from_rgba_unmultiplied(170, 90, 40, 30);
                chart.config.premarket_color = Some(PREMARKET);
                chart.config.postmarket_color = None;
            }
            let mut settings = crate::widget::settings::ChartSettingsState::from_chart(&chart);
            settings
                .session_background
                .as_mut()
                .unwrap()
                .show_extended_hours = shade;
            settings.apply_to_chart(&mut chart).unwrap();
            chart.config.shade_premarket = pre;
            chart.config.shade_postmarket = post;
            chart.config.shade_overnight = night != Some(false);
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
                        if stroke.color == chart.config.session_break_color =>
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
            // Defaults use distinct phase colours at round(255 * 0.08) = 20 alpha; explicit
            // pre-market and shared post-market colours remain independent of those defaults.
            for (color, spans) in [
                (
                    if defaults {
                        egui::Color32::from_rgba_unmultiplied(255, 152, 0, 20)
                    } else {
                        PREMARKET
                    },
                    if shade && pre {
                        vec![(if metadata { 3 } else { 2 }, 4)]
                    } else {
                        vec![]
                    },
                ),
                (
                    if defaults {
                        egui::Color32::from_rgba_unmultiplied(41, 98, 255, 20)
                    } else {
                        chart.config.extended_hours_color
                    },
                    if shade && post {
                        vec![(0, 1), (5, 7)]
                    } else {
                        vec![]
                    },
                ),
            ] {
                let fills: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect) if rect.fill == color => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                let expected: Vec<_> = spans
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
                    .collect();
                assert_eq!(fills, expected, "{color:?} pre={pre} post={post}");
            }
            // The default is #D500F9 at round(255 * 0.08) = 20 alpha, premultiplied.
            let overnight_color = if defaults {
                egui::Color32::from_rgba_premultiplied(17, 0, 20, 20)
            } else {
                egui::Color32::from_rgba_unmultiplied(170, 90, 40, 30)
            };
            let overnight_fills: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == overnight_color => Some(rect.rect),
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = if shade && night == Some(true) {
                [(1, 3), (7, 8)]
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
            assert_eq!(overnight_fills, expected);
        }
    }

    /// Session breaks paint their colour and width as set at each new session of an hourly
    /// chart, in the price pane and in a study pane, none on daily, weekly and monthly charts,
    /// and shade no session.
    #[test]
    fn session_breaks_paint_as_configured_without_shading() {
        use crate::model::{Bar, BarData, LineStyle, Timeframe};
        use egui::{Color32, Stroke};
        let start: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        // Fifty bars stay in view. Days start at hourly bars 24 and 48; the longer charts cross
        // weeks, months and years, which they drew before.
        for (timeframe, step, expected) in [
            (Timeframe::Hour1, chrono::Duration::hours(1), vec![24, 48]),
            (Timeframe::Day1, chrono::Duration::days(1), vec![]),
            (Timeframe::Week1, chrono::Duration::weeks(1), vec![]),
            (Timeframe::Month1, chrono::Duration::days(30), vec![]),
        ] {
            let bars = (0..50)
                .map(|i| Bar::new(start + step * i, 100.0, 102.0, 99.0, 101.0, 1.0))
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(timeframe);
            chart.config.show_volume = false;
            chart.config.show_grid = false;
            chart.config.show_session_breaks = true;
            chart.config.session_break_style = LineStyle::Solid;
            chart.config.session_break_color = Color32::RED;
            chart.config.session_break_width = 2.0;
            #[cfg(feature = "studies")]
            chart.add_indicator(Box::new(crate::studies::RSI::default()));
            let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let expected: Vec<_> = expected
                .iter()
                .map(|index| (map.idx_to_x(*index), Stroke::new(2.0, Color32::RED)))
                .collect();
            let check_breaks = |pane, plot: egui::Rect| {
                assert!(plot.height() > 20.0, "{timeframe:?} {pane} {plot:?}");
                let breaks: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::LineSegment { points, stroke }
                            if points[0].x == points[1].x
                                && points[0].y == plot.top()
                                && points[1].y == plot.bottom() =>
                        {
                            Some((points[0].x, *stroke))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(breaks, expected, "{timeframe:?} {pane}");
            };
            check_breaks("price", chart.last_rendered_price_rect);
            #[cfg(feature = "studies")]
            check_breaks("study", chart.last_rendered_indicator_panes[0].chart_rect);
            let shading = chart.config.background_color.gamma_multiply(0.94);
            assert!(
                !painted
                    .iter()
                    .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == shading)),
                "{timeframe:?}"
            );
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
        // Over 241 quarter-point ticks, rows of 10 ticks make 25 rows and rows of 11 make 23,
        // equally close to the 24 requested, so the taller 11. Keys
        // floor(400/11)=36 through floor(640/11)=58 span 98.875..162.125, with POC 100.25.
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
                    let edges = if kind == ChartType::SessionVolumeProfile {
                        [98.875, 162.125]
                    } else {
                        [99.875, 162.375]
                    };
                    for price in edges {
                        assert!(
                            map.price
                                .range()
                                .contains(map.price.transform().to_scale(price)),
                            "{kind:?} {mode:?} letters={letters} count={count} width={width}: {price}"
                        );
                    }
                    if count <= 8 {
                        let range = map.price.range();
                        // The span takes 82% of the pane, with 8% below and 10% above.
                        // Grouped execution rows span 98.875..162.125. TPO rows of 2.5 (the 440 ticks of
                        // the bars over 80 rows, 5.5 rounded up to 10) end at 162.375.
                        let (low, high) = if kind == ChartType::SessionVolumeProfile {
                            (98.875 - 63.25 / 0.82 * 0.08, 162.125 + 63.25 / 0.82 * 0.1)
                        } else {
                            (99.875 - 62.5 / 0.82 * 0.08, 162.375 + 62.5 / 0.82 * 0.1)
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
                let poc = if kind == ChartType::SessionVolumeProfile {
                    "POC 100.25"
                } else {
                    "POC 100.00"
                };
                assert!(
                    painted.iter().any(|s| matches!(&s.shape, Shape::Text(t)
                    if t.galley.text().starts_with(&session) && t.galley.text().contains(poc) && t.galley.text().contains("VAH")
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
                            // Flat data uses 1% of its price, so the margin span is 1 / 0.82.
                            crate::scales::PriceRange::new(
                                100.0 - 1.0 / 0.82 * 0.08,
                                100.0 + 1.0 / 0.82 * 0.1,
                            ),
                        ),
                        // Rows of 1.0 (62.1 over 80 rows, no tick) cover 8.5..71.5.
                        (
                            [8.9, 71.0],
                            crate::scales::PriceRange::new(
                                8.5 - 63.0 / 0.82 * 0.08,
                                71.5 + 63.0 / 0.82 * 0.1,
                            ),
                        ),
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
                        let range = chart.state.price_scale().range();
                        assert!(
                            (range.min - expected.min).abs() < 1e-9
                                && (range.max - expected.max).abs() < 1e-9,
                            "automatic rows converge and preserve flat-session bounds: {prices:?}, {range:?}, expected {expected:?}"
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
