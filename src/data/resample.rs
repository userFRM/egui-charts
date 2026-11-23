//! Bars of a coarser interval built from bars of a finer one.

use crate::model::{Bar, BarClock, Instrument, Timeframe};
use chrono::TimeDelta;

/// Builds bars of interval `to` from `bars` of interval `from`, which must be in time order.
/// Pass timeframes for UTC boundaries, or [`BarClock`] values for a selected calendar timezone.
/// Prices and volumes are expected to be finite, as stored series keep them.
///
/// Each output bar gathers the consecutive input bars that fall in one bar of
/// the target [`BarClock`] and is stamped with that bar's open time. It takes the open
/// of the first input bar, the highest high, the lowest low, the close of the last input bar and
/// the summed volume. A period without input bars has no output bar. When `from == to`, the bars
/// are returned unchanged when the clocks also share a timezone.
///
/// Returns `None` when `from` bars do not nest in `to` bars: weekly and monthly bars into any
/// other interval, or an input bar that would span two output bars, as a 5-minute bar opening at
/// 00:05 does in 7-minute bars. Also returns `None` when the bars of one slot do not combine into a
/// valid bar, as when their volumes add up to infinity.
///
/// ```
/// use chrono::{DateTime, TimeDelta, Utc};
/// use egui_charts::data::{Bar, Timeframe, resample};
///
/// let start: DateTime<Utc> = "2024-03-15T10:00:00Z".parse().unwrap();
/// let minutes: Vec<Bar> = (0..10)
///     .map(|i| {
///         let price = 100.0 + i as f64;
///         Bar::new(start + TimeDelta::minutes(i), price, price + 1.0, price - 1.0, price + 0.5, 10.0)
///     })
///     .collect();
///
/// let bars = resample(&minutes, Timeframe::Min1, Timeframe::Min5).unwrap();
/// assert_eq!(bars.len(), 2);
/// let last = &bars[1];
/// assert_eq!(last.time, start + TimeDelta::minutes(5));
/// assert_eq!(
///     (last.open, last.high, last.low, last.close, last.volume),
///     (105.0, 110.0, 104.0, 109.5, 50.0)
/// );
/// ```
pub fn resample(
    bars: &[Bar],
    from: impl Into<BarClock>,
    to: impl Into<BarClock>,
) -> Option<Vec<Bar>> {
    let (source, target) = (from.into(), to.into());
    let (from, to) = (source.timeframe()?, target.timeframe()?);
    if source == target || (from == to && from.is_trade_based()) {
        return Some(bars.to_vec());
    }
    if from.is_trade_based() || to.is_trade_based() {
        return None;
    }
    // Weeks do not nest in months or in fixed multi-week intervals, and a month has no fixed length.
    if matches!(
        from,
        Timeframe::Week1 | Timeframe::Month1 | Timeframe::Weeks(_) | Timeframe::Months(_)
    ) {
        return None;
    }
    let mut resampled: Vec<Bar> = Vec::new();
    for bar in bars {
        let open = source.bar_open(bar.time);
        // The next open rather than `open + from`: the last bar of a day is short when `from`
        // does not divide a day.
        let end = source.next_open(open)?;
        let slot = target.bar_open(open);
        if target.bar_open(end - TimeDelta::milliseconds(1)) != slot {
            return None;
        }
        match resampled.last_mut() {
            Some(last) if last.time == slot => *last = super::merge::fold(last, bar)?,
            // The colours and payload of a source bar are not those of the bar it falls in.
            _ => resampled.push(Bar {
                time: slot,
                colors: None,
                payload: None,
                ..bar.clone()
            }),
        }
    }
    Some(resampled)
}

/// Builds a coarser interval using the instrument's exchange trading hours and dates.
///
/// Intraday inputs must be stamped at their actual open, including a shortened final bar.
/// Intraday target slots start at the first selected session window of each trading date;
/// daily, weekly and monthly targets use that trading date, including overnight sessions.
/// A target of two or more whole days, such as `3D`, holds that many trading dates of the selected
/// hours, counted from 1970-01-01 as a [`BarClock`] with the session counts them, and carries the
/// date of the first; a target clock with a session of its own counts that session's dates.
/// Bars outside the selected hours and holidays are omitted. No bars are invented across gaps,
/// and the last output includes all available inputs even while its period is forming.
/// Daily and longer inputs already carry their trading date as a calendar label. Pass timeframes
/// for UTC labels, or matching source and target [`BarClock`] values to retain another calendar.
/// With `extended_hours`, both extended and overnight windows participate; see
/// [`resample_with_session_hours`] to leave the overnight windows out.
/// Exchange time determines the trading date; the target clock determines its output label.
///
/// As with [`resample`], returns `None` if an input straddles target slots or folded volume
/// overflows. The host retains the base history and calls again after live updates or corrections.
/// Native source intervals can be reported through [`Instrument::native_timeframes`].
pub fn resample_with_session(
    bars: &[Bar],
    from: impl Into<BarClock>,
    to: impl Into<BarClock>,
    instrument: &Instrument,
    extended_hours: bool,
) -> Option<Vec<Bar>> {
    resample_with_session_hours(bars, from, to, instrument, extended_hours, extended_hours)
}

/// As [`resample_with_session`], for the hours a [`HistoryRequest`](super::HistoryRequest)
/// selects: regular hours without `extended_hours`, extended hours without the
/// [overnight windows](crate::model::Session::overnight_windows) with `extended_hours` alone,
/// and all hours with both. `overnight_hours` without `extended_hours` selects regular hours.
///
/// ```
/// use chrono::{NaiveTime, Weekday::*};
/// use egui_charts::data::{Bar, HistoryRequest, resample_with_session_hours};
/// use egui_charts::model::{Instrument, Session, SessionWindow, Timeframe, TimezoneId};
///
/// let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
/// let days = [Mon, Tue, Wed, Thu, Fri];
/// let session = Session::new([SessionWindow::new(days, at(9, 30), at(16, 0))])
///     .with_extended_windows([SessionWindow::new(days, at(4, 0), at(9, 30))])
///     .with_overnight_windows([SessionWindow::new(days, at(20, 0), at(4, 0))]);
/// let instrument = Instrument::new("XYZ")
///     .with_time_zone(TimezoneId::utc())
///     .with_session(session);
/// // An overnight minute and a pre-market minute of Monday's trading date.
/// let minutes = ["2024-03-10T20:00:00Z", "2024-03-11T04:00:00Z"]
///     .map(|time| Bar::new(time.parse().unwrap(), 1.0, 1.0, 1.0, 1.0, 1.0));
/// let mut request = HistoryRequest::new("XYZ", "45min".parse::<Timeframe>().unwrap(), 500);
/// request.extended_hours = true;
/// for (overnight_hours, opens) in [
///     // Extended hours count from the 04:00 pre-market open.
///     (false, &["2024-03-11T04:00:00Z"][..]),
///     // All hours count from the 20:00 overnight open, so 04:00 is in the 03:30 bar.
///     (true, &["2024-03-10T20:00:00Z", "2024-03-11T03:30:00Z"][..]),
/// ] {
///     request.overnight_hours = overnight_hours;
///     let bars = resample_with_session_hours(
///         &minutes,
///         Timeframe::Min1,
///         request.clock.clone(),
///         &instrument,
///         request.extended_hours,
///         request.overnight_hours,
///     )
///     .unwrap();
///     let times: Vec<String> = bars.iter()
///         .map(|bar| bar.time.format("%FT%TZ").to_string())
///         .collect();
///     assert_eq!(times, opens);
/// }
/// ```
pub fn resample_with_session_hours(
    bars: &[Bar],
    from: impl Into<BarClock>,
    to: impl Into<BarClock>,
    instrument: &Instrument,
    extended_hours: bool,
    overnight_hours: bool,
) -> Option<Vec<Bar>> {
    Some(
        resample_periods(bars, from, to, instrument, extended_hours, overnight_hours)?
            .into_iter()
            .map(|(_, bar)| bar)
            .collect(),
    )
}

/// As [`resample_with_session_hours`], with the range of `bars` each output bar was built from.
/// The ranges are in order and never empty, and each kept bar is in exactly one.
pub(crate) fn resample_periods(
    bars: &[Bar],
    from: impl Into<BarClock>,
    to: impl Into<BarClock>,
    instrument: &Instrument,
    extended_hours: bool,
    overnight_hours: bool,
) -> Option<Vec<(std::ops::Range<usize>, Bar)>> {
    use crate::model::{ExchangeSessionProvider, TimezoneId};

    let zone = instrument.time_zone.clone().unwrap_or_else(TimezoneId::utc);
    let (mut source, mut target) = (from.into(), to.into());
    if let Some(session) = &instrument.session {
        // Bars of several days count the trading dates of the selected hours.
        source = source.with_trading_dates_of(&zone, session, extended_hours, overnight_hours);
        target = target.with_trading_dates_of(&zone, session, extended_hours, overnight_hours);
    }
    let (from, to) = (source.timeframe()?, target.timeframe()?);
    if (instrument.session.is_none()
        && (instrument.time_zone.is_none() || to.duration_ms() < Timeframe::Day1.duration_ms()))
        || from.duration_ms() >= Timeframe::Day1.duration_ms()
        || from.is_trade_based()
        || to.is_trade_based()
    {
        let output = resample(bars, source.clone(), target.clone())?;
        // As many output bars as input bars leaves one input bar to each. Equal clocks return
        // the bars unchanged, also those off the clock's grid, which the count below misses.
        if output.len() == bars.len() {
            return Some(
                output
                    .into_iter()
                    .enumerate()
                    .map(|(i, bar)| (i..i + 1, bar))
                    .collect(),
            );
        }
        let mut start = 0;
        return Some(
            output
                .into_iter()
                .map(|bar| {
                    // The run `resample` folded into this bar, counted as it folds: bar by bar.
                    let end = start
                        + bars[start..]
                            .iter()
                            .take_while(|input| {
                                target.bar_open(source.bar_open(input.time)) == bar.time
                            })
                            .count();
                    let range = start..end;
                    start = end;
                    (range, bar)
                })
                .collect(),
        );
    }
    let mut session = instrument.session.clone();
    if let Some(session) = &mut session {
        session.select_hours(extended_hours, extended_hours && overnight_hours);
    }
    let provider = ExchangeSessionProvider::new(zone, session);
    let opens = provider.opens(bars.iter().map(|bar| bar.time));
    let slot = |time| {
        let n = opens.partition_point(|(open, _)| *open <= time);
        let (open, date) = *opens.get(n.checked_sub(1)?)?;
        if to.duration_ms() < Timeframe::Day1.duration_ms() {
            let elapsed = (time - open).num_milliseconds();
            open.checked_add_signed(TimeDelta::milliseconds(
                elapsed - elapsed.rem_euclid(to.duration_ms()),
            ))
        } else {
            target.open_on_date(date)
        }
    };
    let mut output: Vec<(std::ops::Range<usize>, Bar)> = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        if !provider.is_open(bar.time, true) {
            continue;
        }
        let period = slot(bar.time)?;
        let mut end = if instrument.session.is_none() {
            source.next_open(source.bar_open(bar.time))?
        } else {
            bar.time.checked_add_signed(from.duration())?
        };
        if let Some(close) = provider.session_close(bar.time, true) {
            end = end.min(close);
        }
        if slot(end.checked_sub_signed(TimeDelta::nanoseconds(1))?)? != period {
            return None;
        }
        match output.last_mut() {
            Some((range, last)) if last.time == period => {
                *last = super::merge::fold(last, bar)?;
                range.end = i + 1;
            }
            _ => output.push((
                i..i + 1,
                Bar {
                    time: period,
                    colors: None,
                    payload: None,
                    ..bar.clone()
                },
            )),
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn bar(time: &str, [open, high, low, close, volume]: [f64; 5]) -> Bar {
        let time: DateTime<Utc> = time.parse().unwrap();
        Bar::new(time, open, high, low, close, volume)
    }

    /// Guards #3758: on an instrument with a session a bar of several whole days holds that many
    /// trading dates of the selected hours, counted from 1970-01-01, and carries the date of the
    /// first, and live updates join the same bars on the chart's clock.
    #[test]
    fn resamples_multi_day_intervals_by_trading_dates() {
        use crate::model::{OverrideWindow, Session, SessionOverride, SessionWindow, TimezoneId};
        use chrono::{NaiveDate, NaiveTime, TimeDelta, Weekday::*};
        type Row = (&'static str, Session, &'static [u32], &'static [(u32, f64)]);
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let nov = |day| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let midnight = |day| nov(day).and_time(at(0, 0)).and_utc();
        let weekdays = Session::new([SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            at(9, 30),
            at(16, 0),
        )]);
        let three_days: Timeframe = "3D".parse().unwrap();
        // Monday 9 November 2026 is day 20766 after 1970-01-01 and weekday 14832 since then, both
        // multiples of three, so a run starts there on every row. Volumes are 2^(day - 9).
        #[rustfmt::skip]
        let rows: [Row; 3] = [
            ("weekdays", weekdays.clone(), &[9, 10, 11, 12, 13, 16, 17, 18, 19, 20, 23, 24, 25, 26, 27],
                &[(9, 7.0), (12, 152.0), (17, 1792.0), (20, 51200.0), (25, 458752.0)]),
            // A holiday listed twice drops the 11th; the half day on the 20th still trades.
            ("closures", weekdays.clone().with_holidays([nov(11), nov(11)]).with_overrides([
                SessionOverride::new(nov(20)).with_windows([OverrideWindow::new(at(9, 30), at(13, 0))]),
            ]), &[10, 12, 13, 16, 17, 18, 19, 20, 23, 24, 25, 26, 27],
                &[(9, 10.0), (13, 400.0), (18, 3584.0), (23, 114688.0), (26, 393216.0)]),
            // Open every day: the calendar periods of today.
            ("every day", Session::new([]), &[10, 11, 12, 13, 14, 15, 16, 17],
                &[(9, 6.0), (12, 56.0), (15, 448.0)]),
        ];
        for (name, session, days, expected) in rows {
            let instrument = Instrument::new("DAYS")
                .with_time_zone(TimezoneId::utc())
                .with_session(session.clone());
            let input: Vec<Bar> = (days.iter())
                .map(|&day| {
                    let volume = f64::from(1u32 << (day - 9));
                    Bar::new(
                        midnight(day) + TimeDelta::minutes(570),
                        1.0,
                        1.0,
                        1.0,
                        1.0,
                        volume,
                    )
                })
                .collect();
            let bars =
                resample_with_session(&input, Timeframe::Hour1, three_days, &instrument, false)
                    .unwrap();
            let got: Vec<_> = bars.iter().map(|bar| (bar.time, bar.volume)).collect();
            let want: Vec<_> = (expected.iter())
                .map(|&(day, volume)| (midnight(day), volume))
                .collect();
            assert_eq!(got, want, "{name}");
            // The chart's clock places live updates and boundaries in the same bars.
            let clock =
                BarClock::new(three_days).with_session(TimezoneId::utc(), session.clone(), false);
            for bar in &input {
                let open = want
                    .iter()
                    .rev()
                    .find(|(open, _)| *open <= bar.time)
                    .unwrap()
                    .0;
                assert_eq!(clock.instant_open(bar.time), open, "{name} {}", bar.time);
            }
            for pair in want.windows(2) {
                let (open, next) = (pair[0].0, pair[1].0);
                assert_eq!(clock.next_open(open), Some(next), "{name} {open}");
                assert_eq!(
                    clock.bar_open(next - TimeDelta::milliseconds(1)),
                    open,
                    "{name} {next}"
                );
            }
        }
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn resamples_selected_exchange_sessions() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday};
        let clock = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let instrument = Instrument::new("SESSION")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap())
            .with_session(
                Session::new([SessionWindow::new(
                    [Weekday::Fri, Weekday::Mon],
                    clock(9, 30),
                    clock(16, 0),
                )])
                .with_extended_windows([SessionWindow::new(
                    [Weekday::Fri, Weekday::Mon],
                    clock(8, 30),
                    clock(9, 30),
                )])
                .with_holidays(["2024-03-18".parse().unwrap()]),
            );
        let input = vec![
            bar("2024-03-15T12:30:00Z", [9.0, 10.0, 8.0, 9.0, 1.0]),
            bar("2024-03-15T13:30:00Z", [10.0, 13.0, 9.0, 12.0, 2.0]),
            bar("2024-03-15T14:30:00Z", [12.0, 15.0, 11.0, 14.0, 3.0]),
            bar("2024-03-15T15:30:00Z", [14.0, 16.0, 10.0, 11.0, 4.0]),
            bar("2024-03-15T19:30:00Z", [11.0, 12.0, 7.0, 8.0, 5.0]),
            bar("2024-03-18T13:30:00Z", [8.0, 9.0, 6.0, 7.0, 6.0]),
        ];
        for (extended, expected) in [
            (
                false,
                vec![
                    bar("2024-03-15T13:30:00Z", [10.0, 15.0, 9.0, 14.0, 5.0]),
                    bar("2024-03-15T15:30:00Z", [14.0, 16.0, 10.0, 11.0, 4.0]),
                    bar("2024-03-15T19:30:00Z", [11.0, 12.0, 7.0, 8.0, 5.0]),
                ],
            ),
            (
                true,
                vec![
                    bar("2024-03-15T12:30:00Z", [9.0, 13.0, 8.0, 12.0, 3.0]),
                    bar("2024-03-15T14:30:00Z", [12.0, 16.0, 10.0, 11.0, 7.0]),
                    bar("2024-03-15T18:30:00Z", [11.0, 12.0, 7.0, 8.0, 5.0]),
                ],
            ),
        ] {
            assert_eq!(
                resample_with_session(
                    &input,
                    Timeframe::Hour1,
                    "2h".parse::<Timeframe>().unwrap(),
                    &instrument,
                    extended
                ),
                Some(expected)
            );
        }
        // Only the override opens regular hours; the weekly pre-market must stay closed.
        let date = "2024-03-15".parse().unwrap();
        let extended_only = Session::new([]).with_extended_windows([SessionWindow::new(
            [Weekday::Fri],
            clock(8, 30),
            clock(9, 30),
        )]);
        for (session, expected) in [
            (extended_only.clone(), vec![]),
            (
                extended_only.with_overrides([crate::model::SessionOverride::new(date)
                    .with_windows([crate::model::OverrideWindow::new(
                        clock(9, 30),
                        clock(16, 0),
                    )])]),
                vec![bar("2024-03-15T00:00:00Z", [10.0, 16.0, 7.0, 8.0, 14.0])],
            ),
        ] {
            let instrument = instrument.clone().with_session(session);
            assert_eq!(
                resample_with_session(
                    &input,
                    Timeframe::Hour1,
                    Timeframe::Day1,
                    &instrument,
                    false
                ),
                Some(expected),
            );
        }
        let continuous = Instrument::new("CONTINUOUS")
            .with_time_zone(TimezoneId::utc())
            .with_session(Session::new([]));
        let last = bar("2024-03-15T23:55:00Z", [10.0, 13.0, 9.0, 12.0, 2.0]);
        assert_eq!(
            resample_with_session(
                &[last],
                "7min".parse::<Timeframe>().unwrap(),
                Timeframe::Day1,
                &continuous,
                false,
            ),
            Some(vec![bar(
                "2024-03-15T00:00:00Z",
                [10.0, 13.0, 9.0, 12.0, 2.0]
            )])
        );
        #[cfg(feature = "widget")]
        {
            let mut chart = crate::widget::ChartBuilder::new().build();
            chart.set_timeframe("2h".parse().unwrap());
            chart.set_instrument(Some(instrument.clone()));
            assert_eq!(chart.bar_clock().bar_open(input[1].time), input[1].time);
            chart.set_extended_hours(true);
            assert_eq!(chart.bar_clock().bar_open(input[1].time), input[0].time);
        }
        let overnight = Instrument::new("OVERNIGHT")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap())
            .with_session(Session::new([SessionWindow::new(
                [Weekday::Mon, Weekday::Tue],
                clock(18, 0),
                clock(17, 0),
            )]));
        let input = vec![
            bar("2024-03-10T22:00:00Z", [10.0, 13.0, 9.0, 12.0, 2.0]),
            bar("2024-03-11T03:00:00Z", [12.0, 15.0, 11.0, 14.0, 3.0]),
            bar("2024-03-11T20:00:00Z", [14.0, 16.0, 10.0, 11.0, 4.0]),
            bar("2024-03-11T21:00:00Z", [99.0, 99.0, 99.0, 99.0, 1.0]),
            bar("2024-03-11T22:00:00Z", [11.0, 12.0, 7.0, 8.0, 5.0]),
        ];
        assert_eq!(
            resample_with_session(&input, Timeframe::Hour1, Timeframe::Day1, &overnight, false),
            Some(vec![
                bar("2024-03-11T00:00:00Z", [10.0, 16.0, 9.0, 11.0, 9.0]),
                bar("2024-03-12T00:00:00Z", [11.0, 12.0, 7.0, 8.0, 5.0]),
            ])
        );
        // Date-labelled daily inputs must not be read as instants on the preceding New York day.
        let monday = vec![bar("2024-03-11T00:00:00Z", [10.0, 13.0, 9.0, 12.0, 2.0])];
        assert_eq!(
            resample_with_session(
                &monday,
                Timeframe::Day1,
                Timeframe::Week1,
                &overnight,
                false
            ),
            Some(monday.clone())
        );
        // Tokyo midnight is on the preceding UTC date; its trading date still labels March 11.
        let tokyo =
            Instrument::new("TOKYO").with_time_zone(TimezoneId::named("Asia/Tokyo").unwrap());
        assert_eq!(
            resample_with_session(&monday, Timeframe::Hour1, Timeframe::Day1, &tokyo, false),
            Some(monday)
        );
        let local =
            BarClock::new(Timeframe::Day1).with_timezone("America/New_York".parse().unwrap());
        let selected =
            resample_with_session(&input, Timeframe::Hour1, local, &overnight, false).unwrap();
        assert_eq!(
            selected.iter().map(|bar| bar.time).collect::<Vec<_>>(),
            [
                "2024-03-11T04:00:00Z".parse::<DateTime<Utc>>().unwrap(),
                "2024-03-12T04:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            ]
        );
    }

    /// Guards the OHLCV aggregation, the alignment of output bars to `BarClock`, and the refusal of
    /// intervals whose bars would be split across two output bars.
    #[test]
    fn resamples_into_bar_clock_slots() {
        let min7: Timeframe = "7min".parse().unwrap();
        #[rustfmt::skip]
        let cases = [
            // The seven-minute bar that opens at 23:55 is short: the next one opens at midnight.
            (Timeframe::Min1, min7, vec![
                bar("2024-03-15T23:54:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 13.0, 10.0, 12.0, 2.0]),
                bar("2024-03-15T23:56:00Z", [12.0, 15.0, 11.0, 14.0, 3.0]),
                bar("2024-03-15T23:57:00Z", [14.0, 14.0, 8.0, 9.0, 4.0]),
                bar("2024-03-15T23:58:00Z", [9.0, 11.0, 9.0, 10.0, 5.0]),
                bar("2024-03-15T23:59:00Z", [10.0, 12.0, 10.0, 11.0, 6.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 11.0, 10.0, 10.0, 7.0]),
                bar("2024-03-16T00:01:00Z", [10.0, 13.0, 10.0, 12.0, 8.0]),
            ], Some(vec![
                bar("2024-03-15T23:48:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 15.0, 8.0, 11.0, 20.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ])),
            // The short 23:55 bar ends at midnight, so it fits in the day.
            (min7, Timeframe::Day1, vec![
                bar("2024-03-15T23:48:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 15.0, 8.0, 11.0, 20.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ], Some(vec![
                bar("2024-03-15T00:00:00Z", [10.0, 15.0, 8.0, 11.0, 21.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ])),
            // The 00:05 bar runs to 00:10, across the 00:07 boundary.
            (Timeframe::Min5, min7, vec![
                bar("2024-03-15T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
                bar("2024-03-15T00:05:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
            // The 02:00 bar is missing: the 00:00 bar is built from the three present.
            (Timeframe::Hour1, Timeframe::Hour4, vec![
                bar("2024-03-15T00:00:00Z", [20.0, 22.0, 19.0, 21.0, 100.0]),
                bar("2024-03-15T01:00:00Z", [21.0, 25.0, 20.0, 24.0, 200.0]),
                bar("2024-03-15T03:00:00Z", [24.0, 24.0, 18.0, 19.0, 300.0]),
                bar("2024-03-15T04:00:00Z", [19.0, 20.0, 17.0, 18.0, 400.0]),
            ], Some(vec![
                bar("2024-03-15T00:00:00Z", [20.0, 25.0, 18.0, 19.0, 600.0]),
                bar("2024-03-15T04:00:00Z", [19.0, 20.0, 17.0, 18.0, 400.0]),
            ])),
            // Saturday and Sunday belong to the week that opened on Monday the 11th.
            (Timeframe::Day1, Timeframe::Week1, vec![
                bar("2024-03-16T00:00:00Z", [30.0, 31.0, 29.0, 30.0, 1.0]),
                bar("2024-03-17T00:00:00Z", [30.0, 33.0, 30.0, 32.0, 2.0]),
                bar("2024-03-18T00:00:00Z", [32.0, 34.0, 31.0, 33.0, 3.0]),
                bar("2024-03-19T00:00:00Z", [33.0, 33.0, 28.0, 29.0, 4.0]),
            ], Some(vec![
                bar("2024-03-11T00:00:00Z", [30.0, 33.0, 29.0, 32.0, 3.0]),
                bar("2024-03-18T00:00:00Z", [32.0, 34.0, 28.0, 29.0, 7.0]),
            ])),
            // Two volumes that add up to infinity give no valid bar.
            (Timeframe::Min1, Timeframe::Min5, vec![
                bar("2024-03-15T00:00:00Z", [1.0, 1.0, 1.0, 1.0, f64::MAX]),
                bar("2024-03-15T00:01:00Z", [1.0, 1.0, 1.0, 1.0, f64::MAX]),
            ], None),
            // Some weeks end in the next month, so weekly bars never build monthly ones, even a week inside one month.
            (Timeframe::Week1, Timeframe::Month1, vec![
                bar("2024-03-04T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
            ("2w".parse::<Timeframe>().unwrap(), "3month".parse::<Timeframe>().unwrap(), vec![
                bar("2024-03-04T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
            ("3month".parse::<Timeframe>().unwrap(), "6month".parse::<Timeframe>().unwrap(), vec![
                bar("2024-01-01T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
        ];
        for (from, to, bars, expected) in cases {
            assert_eq!(resample(&bars, from, to), expected, "{from} to {to}");
        }
        let Some(zone) = crate::model::TimezoneId::named("America/New_York") else {
            return;
        };
        let local = BarClock::new(Timeframe::Hour4).with_timezone(zone);
        // Local midnight to 04:00 lasts three hours on the spring clock change.
        let input = vec![
            bar("2024-03-10T05:00:00Z", [100.0, 101.0, 99.0, 100.0, 2.0]),
            bar("2024-03-10T07:59:00Z", [102.0, 104.0, 101.0, 103.0, 3.0]),
            bar("2024-03-10T08:00:00Z", [103.0, 105.0, 102.0, 104.0, 4.0]),
        ];
        assert_eq!(
            resample(&input, Timeframe::Min1, local),
            Some(vec![
                bar("2024-03-10T05:00:00Z", [100.0, 104.0, 99.0, 103.0, 5.0]),
                bar("2024-03-10T08:00:00Z", [103.0, 105.0, 102.0, 104.0, 4.0]),
            ])
        );
    }

    /// Each period names the input bars it was built from: every bar once, none left out. Equal
    /// clocks keep each bar as its own period, off the clock's grid or twice at one time, and a
    /// period ends where the next bar leaves it even when a later bar returns to it.
    #[test]
    fn periods_name_the_bars_they_hold() {
        // (source interval, bar times, (bars in the period, period time))
        type Row = (
            Timeframe,
            &'static [&'static str],
            &'static [(usize, &'static str)],
        );
        #[rustfmt::skip]
        let rows: [Row; 3] = [
            (Timeframe::Hour4, &["2024-03-15T01:00:00Z", "2024-03-15T05:00:00Z"],
                &[(1, "2024-03-15T01:00:00Z"), (1, "2024-03-15T05:00:00Z")]),
            (Timeframe::Hour4, &["2024-03-15T04:00:00Z", "2024-03-15T04:00:00Z"],
                &[(1, "2024-03-15T04:00:00Z"), (1, "2024-03-15T04:00:00Z")]),
            (Timeframe::Hour1, &[
                "2024-03-15T00:00:00Z", "2024-03-15T04:00:00Z", "2024-03-15T00:00:00Z",
                "2024-03-15T01:00:00Z", "2024-03-15T02:00:00Z", "2024-03-15T03:00:00Z",
                "2024-03-15T03:00:00Z",
            ], &[(1, "2024-03-15T00:00:00Z"), (1, "2024-03-15T04:00:00Z"), (5, "2024-03-15T00:00:00Z")]),
        ];
        for (from, times, expected) in rows {
            let input: Vec<Bar> = times.iter().map(|time| bar(time, [1.0; 5])).collect();
            let periods = resample_periods(
                &input,
                from,
                Timeframe::Hour4,
                &Instrument::new("PERIODS"),
                true,
                true,
            )
            .unwrap();
            let got: Vec<_> = periods
                .iter()
                .map(|(range, bar)| (range.clone(), bar.time))
                .collect();
            let mut start = 0;
            let expected: Vec<_> = expected
                .iter()
                .map(|&(len, time)| {
                    start += len;
                    (start - len..start, time.parse::<DateTime<Utc>>().unwrap())
                })
                .collect();
            assert_eq!(got, expected, "{from} to 4h, {times:?}");
        }
    }

    /// Guards the three hour choices a chart requests on a session with overnight windows:
    /// intraday slots count from the first selected window of the trading date, as the
    /// chart's clock does, and bars outside the selected hours stay out of every period.
    #[test]
    fn resamples_each_chart_hours_choice() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let days = [Mon, Tue, Wed, Thu, Fri];
        let session = Session::new([SessionWindow::new(days, at(9, 30), at(16, 0))])
            .with_extended_windows([
                SessionWindow::new(days, at(4, 0), at(9, 30)),
                SessionWindow::new(days, at(16, 0), at(20, 0)),
            ])
            .with_overnight_windows([SessionWindow::new(days, at(20, 0), at(4, 0))]);
        let instrument = Instrument::new("OVERNIGHT")
            .with_time_zone(TimezoneId::utc())
            .with_session(session.clone());
        // Monday's trading date opens on Sunday at 20:00 with overnight hours, else at 04:00
        // with extended hours and at 09:30 with regular hours. Each volume is a distinct bit.
        let input: Vec<Bar> = [
            ("2024-03-10T20:00:00Z", 1.0),
            ("2024-03-11T03:59:00Z", 2.0),
            ("2024-03-11T04:00:00Z", 4.0),
            ("2024-03-11T04:44:00Z", 8.0),
            ("2024-03-11T04:45:00Z", 16.0),
            ("2024-03-11T05:30:00Z", 32.0),
            ("2024-03-11T10:00:00Z", 256.0),
            ("2024-03-11T19:59:00Z", 64.0),
            // Tuesday's overnight window.
            ("2024-03-11T20:00:00Z", 128.0),
        ]
        .into_iter()
        .map(|(time, volume)| bar(time, [1.0, 1.0, 1.0, 1.0, volume]))
        .collect();
        let min45: Timeframe = "45min".parse().unwrap();
        type Row = (
            (bool, bool),
            &'static [(&'static str, f64)],
            &'static [(&'static str, f64)],
        );
        #[rustfmt::skip]
        let rows: [Row; 4] = [
            ((false, false), &[("2024-03-11T09:30:00Z", 256.0)], &[("2024-03-11T00:00:00Z", 256.0)]),
            // Overnight hours need extended hours, as in a request.
            ((false, true), &[("2024-03-11T09:30:00Z", 256.0)], &[("2024-03-11T00:00:00Z", 256.0)]),
            ((true, false), &[
                ("2024-03-11T04:00:00Z", 12.0), ("2024-03-11T04:45:00Z", 16.0),
                ("2024-03-11T05:30:00Z", 32.0), ("2024-03-11T10:00:00Z", 256.0),
                ("2024-03-11T19:45:00Z", 64.0),
            ], &[("2024-03-11T00:00:00Z", 380.0)]),
            ((true, true), &[
                ("2024-03-10T20:00:00Z", 1.0), ("2024-03-11T03:30:00Z", 6.0),
                ("2024-03-11T04:15:00Z", 24.0), ("2024-03-11T05:00:00Z", 32.0),
                ("2024-03-11T09:30:00Z", 256.0), ("2024-03-11T19:15:00Z", 64.0),
                ("2024-03-11T20:00:00Z", 128.0),
            ], &[("2024-03-11T00:00:00Z", 383.0), ("2024-03-12T00:00:00Z", 128.0)]),
        ];
        for ((extended, overnight), intraday, daily) in rows {
            for (to, expected) in [(min45, intraday), (Timeframe::Day1, daily)] {
                let bars = resample_with_session_hours(
                    &input,
                    Timeframe::Min1,
                    to,
                    &instrument,
                    extended,
                    overnight,
                )
                .unwrap();
                let got: Vec<_> = bars.iter().map(|bar| (bar.time, bar.volume)).collect();
                let expected: Vec<_> = expected
                    .iter()
                    .map(|&(time, volume)| (time.parse::<DateTime<Utc>>().unwrap(), volume))
                    .collect();
                assert_eq!(got, expected, "{extended} {overnight} {to}");
            }
            // The chart's clock for the same choice opens the same intraday bars.
            let clock = BarClock::new(min45).with_session_hours(
                TimezoneId::utc(),
                session.clone(),
                extended,
                overnight,
            );
            for &(time, _) in intraday {
                let time: DateTime<Utc> = time.parse().unwrap();
                assert_eq!(clock.bar_open(time), time, "{extended} {overnight}");
            }
        }
    }
}
