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
    if source == target || (from == to && matches!(from, Timeframe::Ticks(_))) {
        return Some(bars.to_vec());
    }
    if matches!(from, Timeframe::Ticks(_)) || matches!(to, Timeframe::Ticks(_)) {
        return None;
    }
    // Weeks do not nest in months or in fixed multi-week intervals, and a month has no fixed length.
    if matches!(from, Timeframe::Week1 | Timeframe::Month1) {
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
            _ => resampled.push(Bar {
                time: slot,
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
/// Bars outside the selected hours and holidays are omitted. No bars are invented across gaps,
/// and the last output includes all available inputs even while its period is forming.
/// Daily and longer inputs already carry their trading date as a calendar label. Pass timeframes
/// for UTC labels, or matching source and target [`BarClock`] values to retain another calendar.
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
    Some(
        resample_periods(bars, from, to, instrument, extended_hours)?
            .into_iter()
            .map(|(_, bar)| bar)
            .collect(),
    )
}

pub(crate) fn resample_periods(
    bars: &[Bar],
    from: impl Into<BarClock>,
    to: impl Into<BarClock>,
    instrument: &Instrument,
    extended_hours: bool,
) -> Option<Vec<(std::ops::Range<usize>, Bar)>> {
    use crate::model::{ExchangeSessionProvider, TimezoneId};

    let zone = instrument.time_zone.clone().unwrap_or_else(TimezoneId::utc);
    let (source, target) = (from.into(), to.into());
    let (from, to) = (source.timeframe()?, target.timeframe()?);
    if (instrument.session.is_none()
        && (instrument.time_zone.is_none() || to.duration_ms() < Timeframe::Day1.duration_ms()))
        || from.duration_ms() >= Timeframe::Day1.duration_ms()
        || matches!(from, Timeframe::Ticks(_))
        || matches!(to, Timeframe::Ticks(_))
    {
        let output = resample(bars, source.clone(), target.clone())?;
        let mut start = 0;
        return Some(
            output
                .into_iter()
                .map(|bar| {
                    let end = start
                        + bars[start..].partition_point(|input| {
                            target.bar_open(source.bar_open(input.time)) == bar.time
                        });
                    let range = start..end;
                    start = end;
                    (range, bar)
                })
                .collect(),
        );
    }
    let mut session = instrument.session.clone();
    if !extended_hours && let Some(session) = &mut session {
        // Keep an empty regular schedule distinct from an explicitly continuous session.
        if session.windows.is_empty() && !session.extended_windows.is_empty() {
            return Some(Vec::new());
        }
        session.extended_windows.clear();
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
        let local = BarClock::new(Timeframe::Day1).with_timezone(chrono_tz::America::New_York);
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
        ];
        for (from, to, bars, expected) in cases {
            assert_eq!(resample(&bars, from, to), expected, "{from} to {to}");
        }
        let local = BarClock::new(Timeframe::Hour4).with_timezone(chrono_tz::America::New_York);
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
}
