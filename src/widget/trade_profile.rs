//! Display settings and timestamp-based edits for execution profiles.

use super::frame::session_ranges;
use super::{Chart, SeriesId};
use crate::chart::overlays::TradeProfileSettings;
use crate::model::{Bar, BarClock, ChartType, ProfileSession, SessionProvider};
use crate::studies::TradeProfileRanges;
use chrono::{DateTime, Utc};
use std::borrow::Cow;
use std::ops::{Range, RangeInclusive};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct TradeProfiles {
    /// The symbol and calendar the edits were made under, `None` before the first edit.
    context: Option<EditContext>,
    spans: Vec<RangeInclusive<DateTime<Utc>>>,
    splits: Vec<DateTime<Utc>>,
}

/// The symbol and calendar edits were made under, with the choice that divides its sessions
/// further: the extended hours and the segment choices share a calendar.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct EditContext {
    symbol: String,
    clock: BarClock,
    division: ProfileSession,
}

impl TradeProfiles {
    /// Whether there are no merges and no splits.
    #[cfg(feature = "ui")]
    pub(super) fn is_empty(&self) -> bool {
        self.spans.is_empty() && self.splits.is_empty()
    }

    /// Whether these are edits the chart makes: splits in time order, each once, and merged
    /// spans that end no earlier than they start and neither overlap nor touch. A split given
    /// twice or out of order leaves no profile to draw, and so no menu to reset it from.
    #[cfg(feature = "ui")]
    pub(super) fn is_valid(&self) -> bool {
        let mut spans: Vec<_> = self.spans.iter().collect();
        spans.sort_unstable_by_key(|span| *span.start());
        self.splits.windows(2).all(|pair| pair[0] < pair[1])
            && spans.iter().all(|span| span.start() <= span.end())
            && spans.windows(2).all(|pair| pair[0].end() < pair[1].start())
    }
}

impl Chart {
    /// Settings used when drawing [`crate::model::ChartType::SessionVolumeProfile`].
    pub fn trade_profile_settings(&self) -> &TradeProfileSettings {
        &self.chart_type_params.profile
    }

    /// Changes the session profile display without changing its executions or ranges.
    pub fn set_trade_profile_settings(&mut self, settings: TradeProfileSettings) {
        self.chart_type_params.profile = settings;
    }

    /// Current execution-profile ranges, in time order, over the loaded main bars.
    /// Custom sessions that keep shared hours in both may overlap.
    /// Merge and split edits retain UTC boundaries across history eviction and prepending.
    pub fn trade_profile_ranges(&self) -> Option<TradeProfileRanges> {
        self.execution_profile_ranges(0..self.bars(SeriesId::MAIN).len())
    }

    /// Merges the indexed execution profile with its next neighbour. Returns false if absent.
    pub fn merge_trade_profile(&mut self, index: usize) -> bool {
        let Some(ranges) = self.trade_profile_ranges() else {
            return false;
        };
        if ranges.ranges().windows(2).any(|r| r[0].end > r[1].start) {
            return false;
        }
        let Some(pair) = ranges.ranges().get(index..).and_then(|r| r.get(..2)) else {
            return false;
        };
        let bars = self.bars(SeriesId::MAIN);
        let end = bars.partition_point(|bar| bar.time < pair[1].end);
        // Restored splits can leave both profiles without a bar.
        let Some(last) = end
            .checked_sub(1)
            .and_then(|i| bars.get(i))
            .filter(|bar| bar.time >= pair[0].start)
        else {
            return false;
        };
        // End at a held bar, so future sessions cannot join this composite accidentally.
        let merged = pair[0].start..=last.time;
        self.prepare_profile_edits();
        self.trade_profiles
            .splits
            .retain(|t| !(merged.start() < t && merged.contains(t)));
        self.retain_profile_span(merged);
        true
    }

    /// Splits the indexed execution profile at a loaded bar's timestamp or clock-slot open.
    /// Endpoints and times between bars leave the ranges unchanged.
    pub fn split_trade_profile(&mut self, index: usize, time: DateTime<Utc>) -> bool {
        let Some(ranges) = self.trade_profile_ranges() else {
            return false;
        };
        if ranges.ranges().windows(2).any(|r| r[0].end > r[1].start) {
            return false;
        }
        let bars = self.bars(SeriesId::MAIN);
        let Some(bar) = bars.get(bars.partition_point(|bar| bar.time < time)) else {
            return false;
        };
        let Some(range) = ranges.ranges().get(index) else {
            return false;
        };
        let open = self.profile_clock().bar_open(bar.time);
        if (time != bar.time && time != open) || !(range.start < open && open < range.end) {
            return false;
        }
        let end = bars.partition_point(|bar| bar.time < range.end);
        // A restored split can leave the profile without a bar.
        let Some(last) = end
            .checked_sub(1)
            .and_then(|i| bars.get(i))
            .filter(|bar| bar.time >= range.start)
        else {
            return false;
        };
        let span = range.start..=last.time;
        self.prepare_profile_edits();
        // A split needs its original session start even after the first candles are evicted.
        self.retain_profile_span(span);
        self.trade_profiles.splits.push(open);
        self.trade_profiles.splits.sort_unstable();
        self.trade_profiles.splits.dedup();
        true
    }

    /// Restores the natural session boundaries for execution profiles.
    pub fn reset_trade_profile_ranges(&mut self) {
        self.trade_profiles.spans.clear();
        self.trade_profiles.splits.clear();
    }

    /// Whether the current symbol and calendar have execution profile merges or splits.
    #[cfg(feature = "ui")]
    pub(crate) fn has_trade_profile_edits(&self) -> bool {
        self.profile_edits_current() && !self.trade_profiles.is_empty()
    }

    /// The calendar that divides the chart into profile sessions, as the settings choose it.
    pub(crate) fn profile_clock(&self) -> BarClock {
        self.profile_division().0
    }

    /// The calendar that divides the chart into profile sessions and the choice that keeps the
    /// bars of their hours, as [`Chart::profile_hours`] resolves the session volume profile's
    /// choice.
    fn profile_division(&self) -> (BarClock, Cow<'_, ProfileSession>) {
        // The profile settings divide only the session volume profile chart: other chart types
        // keep the chart's calendar here, and the market profile chart resolves its own hours in
        // `tpo_chart_division`.
        let selection = match self.chart_type {
            ChartType::SessionVolumeProfile => &self.trade_profile_settings().session,
            _ => &ProfileSession::Chart,
        };
        self.profile_hours(selection)
    }

    fn profile_edits_current(&self) -> bool {
        let (clock, selection) = self.profile_division();
        self.trade_profiles.context.as_ref().is_some_and(|edited| {
            edited.symbol == self.symbol() && edited.clock == clock && edited.division == *selection
        })
    }

    fn prepare_profile_edits(&mut self) {
        if !self.profile_edits_current() {
            self.reset_trade_profile_ranges();
            let (clock, selection) = self.profile_division();
            let context = EditContext {
                symbol: self.symbol().to_owned(),
                clock,
                division: selection.into_owned(),
            };
            self.trade_profiles.context = Some(context);
        }
    }

    fn retain_profile_span(&mut self, mut span: RangeInclusive<DateTime<Utc>>) {
        for range in &self.trade_profiles.spans {
            if range.end() >= span.start() && range.start() <= span.end() {
                span = (*range.start()).min(*span.start())..=(*range.end()).max(*span.end());
            }
        }
        self.trade_profiles
            .spans
            .retain(|r| r.end() < span.start() || r.start() > span.end());
        self.trade_profiles.spans.push(span);
    }

    pub(crate) fn execution_profile_ranges(
        &self,
        mut shown: Range<usize>,
    ) -> Option<TradeProfileRanges> {
        let bars = self.bars(SeriesId::MAIN);
        let (clock, selection) = self.profile_division();
        let sessions = clock.sessions();
        // Each segment of a day is a profile of its own, so a merged span takes in the later bars
        // of its own segment only.
        let segment = |time: &DateTime<Utc>| {
            (*selection == ProfileSession::Segments).then(|| sessions.market_session(*time))
        };
        let joins = |end: &DateTime<Utc>, start: &DateTime<Utc>| {
            sessions.session_break_between(end, start).is_none() && segment(end) == segment(start)
        };
        let natural = |shown| match &*selection {
            ProfileSession::Custom(custom) => custom_ranges(
                bars,
                shown,
                &clock,
                custom,
                self.regular_hours_clock().as_ref(),
            ),
            _ => profiled(
                bars,
                session_ranges(bars, shown, &clock)?,
                &clock,
                &selection,
            ),
        };
        let mut ranges = natural(shown.clone())?;
        let current = self.profile_edits_current()
            && !ranges.ranges().windows(2).any(|r| r[0].end > r[1].start);
        if current {
            let first = ranges.ranges().first()?.start;
            let end = ranges.ranges().last()?.end;
            let original = shown.clone();
            for merged in &self.trade_profiles.spans {
                if *merged.start() < end && (*merged.end() >= first || joins(merged.end(), &first))
                {
                    shown.start = shown
                        .start
                        .min(bars.partition_point(|b| b.time < *merged.start()));
                    shown.end = shown
                        .end
                        .max(bars.partition_point(|b| b.time <= *merged.end()));
                }
            }
            if shown != original {
                ranges = natural(shown)?;
            }
        }
        if !current || ranges.ranges().windows(2).any(|r| r[0].end > r[1].start) {
            return Some(ranges);
        }
        let natural = ranges;
        let mut ranges: Vec<Range<DateTime<Utc>>> = Vec::new();
        for range in natural.ranges() {
            let merged = self.trade_profiles.spans.iter().find(|r| {
                *r.start() < range.end && (*r.end() >= range.start || joins(r.end(), &range.start))
            });
            if let Some(last) = ranges.last_mut()
                && merged.is_some_and(|r| last.end > *r.start() && last.start <= *r.end())
            {
                last.end = range.end;
            } else {
                ranges.push(merged.map_or(range.start, |r| range.start.min(*r.start()))..range.end);
            }
        }
        let ranges = ranges
            .into_iter()
            .flat_map(|range| {
                let mut start = range.start;
                self.trade_profiles
                    .splits
                    .iter()
                    .copied()
                    .filter(move |t| range.start < *t && *t < range.end)
                    .chain([range.end])
                    .map(move |end| {
                        let part = start..end;
                        start = end;
                        part
                    })
            })
            .filter(|range| range.end > clock.bar_open(bars[0].time))
            .collect();
        TradeProfileRanges::new(ranges).ok()
    }
}

/// Whole custom parts touching the view, padded to include an overnight session across a clock change.
fn custom_ranges(
    bars: &[Bar],
    shown: Range<usize>,
    clock: &BarClock,
    custom: &crate::model::CustomProfileSessions,
    regular: Option<&BarClock>,
) -> Option<TradeProfileRanges> {
    let visible = bars.get(shown.clone())?;
    let first = visible.first()?.time;
    let last = visible.last()?.time;
    let padding = chrono::TimeDelta::days(2);
    let first = first
        .checked_sub_signed(padding)
        .unwrap_or(DateTime::<Utc>::MIN_UTC);
    let last = last
        .checked_add_signed(padding)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    let slice = bars.partition_point(|b| b.time < first)..bars.partition_point(|b| b.time <= last);
    let ranges = custom
        .parts(
            bars,
            slice,
            clock.timeframe().unwrap_or_default(),
            regular,
            true,
            None,
        )
        .into_iter()
        .filter(|p| p.bars.end > shown.start && p.bars.start < shown.end)
        .map(|p| {
            clock.bar_open(bars[p.bars.start].time)
                ..bars
                    .get(p.bars.end)
                    .map_or(DateTime::<Utc>::MAX_UTC, |b| clock.bar_open(b.time))
        })
        .collect();
    if custom.overlap == crate::model::SessionOverlap::InBoth {
        TradeProfileRanges::overlapping(ranges).ok()
    } else {
        TradeProfileRanges::new(ranges).ok()
    }
}

/// `ranges`, whole sessions of `clock` over `bars`, cut into the parts `selection` profiles
/// ([`ProfileSession::profiled`]): each from the open of its first bar to the open of the bar
/// after it, or to its session's end after the session's last bar.
fn profiled(
    bars: &[Bar],
    ranges: TradeProfileRanges,
    clock: &BarClock,
    selection: &ProfileSession,
) -> Option<TradeProfileRanges> {
    if *selection == ProfileSession::Chart {
        return Some(ranges);
    }
    let sessions = clock.sessions();
    let mut parts = Vec::new();
    for range in ranges.ranges() {
        let first = bars.partition_point(|bar| bar.time < range.start);
        let end = bars.partition_point(|bar| bar.time < range.end);
        for part in selection.profiled(bars, first..end, &sessions, true) {
            let close = match bars.get(part.end).filter(|_| part.end < end) {
                Some(after) => clock.bar_open(after.time),
                None => range.end,
            };
            parts.push(clock.bar_open(bars[part.start].time)..close);
        }
    }
    parts.retain(|part| part.start < part.end);
    TradeProfileRanges::new(parts).ok()
}

#[cfg(all(test, feature = "timezones"))]
mod tests {
    use crate::chart::overlays::TradeProfileSettings;
    use crate::model::{
        Bar, BarData, ChartType, CustomProfileSession, CustomProfileSessions, Instrument,
        ProfileSession, Session, SessionOverlap, SessionWindow, Timeframe, TimezoneId,
    };
    use crate::widget::Chart;
    use chrono::{DateTime, NaiveTime, TimeDelta, Timelike, Utc, Weekday};

    fn hourly(instrument: Instrument, session: ProfileSession) -> Chart {
        bars_every(60, 80, instrument, session)
    }

    fn bars_every(
        minutes: i64,
        count: i64,
        instrument: Instrument,
        session: ProfileSession,
    ) -> Chart {
        // Friday evening through the New York clock change on Sunday March 10.
        let start: DateTime<Utc> = "2024-03-08T20:00:00Z".parse().unwrap();
        let bars = (0..count)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(minutes * i),
                    100.0,
                    102.0,
                    99.0,
                    101.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_chart_type(ChartType::SessionVolumeProfile);
        chart.set_timeframe(if minutes == 30 {
            Timeframe::Min30
        } else {
            Timeframe::Hour1
        });
        chart.set_instrument(Some(instrument));
        chart.set_trade_profile_settings(TradeProfileSettings {
            session,
            ..Default::default()
        });
        chart
    }

    fn custom_windows() -> CustomProfileSessions {
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        CustomProfileSessions::new([
            CustomProfileSession::new(at(8, 0), at(12, 30)),
            CustomProfileSession::new(at(12, 0), at(16, 0)),
        ])
    }

    fn ranges(chart: &Chart) -> Vec<String> {
        let bars = chart.bars(crate::widget::SeriesId::MAIN);
        chart
            .trade_profile_ranges()
            .unwrap()
            .ranges()
            .iter()
            .map(|r| {
                let start = bars.partition_point(|bar| bar.time < r.start);
                let end = bars.partition_point(|bar| bar.time < r.end);
                assert_eq!(r.start, chart.profile_clock().bar_open(bars[start].time));
                format!(
                    "{}-{}",
                    bars[start].time.format("%d %H:%M"),
                    bars.get(end)
                        .map_or(r.end, |bar| bar.time)
                        .format("%d %H:%M")
                )
            })
            .collect()
    }

    /// The 12:00 bar belongs to both windows: keep it twice or give it its own profile.
    #[test]
    fn custom_sessions_share_or_split_their_common_hours() {
        for (overlap, expected) in [
            (
                SessionOverlap::InBoth,
                vec![
                    "08 20:00-08 21:00",
                    "09 13:00-09 18:00",
                    "09 17:00-09 21:00",
                    "10 12:00-10 17:00",
                    "10 16:00-10 20:00",
                    "11 12:00-11 17:00",
                    "11 16:00-11 20:00",
                ],
            ),
            (
                SessionOverlap::OwnProfile,
                vec![
                    "08 20:00-08 21:00",
                    "09 13:00-09 17:00",
                    "09 17:00-09 18:00",
                    "09 18:00-09 21:00",
                    "10 12:00-10 16:00",
                    "10 16:00-10 17:00",
                    "10 17:00-10 20:00",
                    "11 12:00-11 16:00",
                    "11 16:00-11 17:00",
                    "11 17:00-11 20:00",
                ],
            ),
        ] {
            let mut custom = custom_windows();
            custom.overlap = overlap;
            let mut chart = hourly(us_equity_hours(), ProfileSession::Custom(custom));
            assert_eq!(ranges(&chart), expected, "{overlap:?}");
            // The session study uses the same independent windows and counts every kept bar.
            use crate::studies::{Indicator, InputValue, VolProfileSession};
            let mut study = VolProfileSession::new(24);
            study.set_timeframe(Timeframe::Hour1);
            study.set_instrument(chart.instrument());
            for (id, value) in [
                ("sessions", InputValue::Choice(3)),
                ("hours_only", InputValue::Toggle(true)),
                ("custom_sessions", InputValue::Choice(1)),
                ("session_start", InputValue::Length(480)),
                ("session_end", InputValue::Length(750)),
                ("session_2_start", InputValue::Length(720)),
                ("session_2_end", InputValue::Length(960)),
                (
                    "session_overlap",
                    InputValue::Choice(usize::from(overlap == SessionOverlap::InBoth)),
                ),
            ] {
                study.set_input(id, value).unwrap();
            }
            let bars = chart.bars(crate::widget::SeriesId::MAIN);
            study.calculate(bars);
            let parts = study.volume_profiles(0..bars.len());
            let observed: Vec<_> = parts
                .iter()
                .map(|p| {
                    assert!(
                        (p.profile.volume().iter().sum::<f64>() - p.bars.len() as f64).abs() < 1e-9
                    );
                    format!(
                        "{}-{}",
                        bars[p.bars.start].time.format("%d %H:%M"),
                        bars[p.bars.end].time.format("%d %H:%M")
                    )
                })
                .collect();
            assert_eq!(observed, expected, "session study {overlap:?}");
            study
                .set_input("session_2_end", InputValue::Length(15 * 60))
                .unwrap();
            study.calculate(bars);
            let edited = study.volume_profiles(0..bars.len());
            assert_eq!(
                edited.len(),
                if overlap == SessionOverlap::InBoth {
                    6
                } else {
                    9
                }
            );
            let total: f64 = edited.iter().flat_map(|p| p.profile.volume()).sum();
            assert!(
                (total
                    - if overlap == SessionOverlap::InBoth {
                        24.0
                    } else {
                        21.0
                    })
                .abs()
                    < 1e-9,
                "edited window rebuilds its cached bars"
            );

            if overlap == SessionOverlap::InBoth {
                let mut shared = chart.trade_profile_ranges().unwrap();
                assert!(!shared.merge_next(1));
                assert!(!shared.split(1, "2024-03-09T15:00:00Z".parse().unwrap()));
                assert!(!chart.merge_trade_profile(1));
                assert!(!chart.split_trade_profile(1, "2024-03-09T15:00:00Z".parse().unwrap()));
                assert_eq!(ranges(&chart), expected);
                // A later history fill can introduce shared bars after an unambiguous merge.
                let full = chart.bars(crate::widget::SeriesId::MAIN).to_vec();
                let zone = TimezoneId::named("America/New_York").unwrap();
                let without_shared = full
                    .iter()
                    .filter(|bar| bar.time.with_timezone(&zone).hour() != 12)
                    .cloned()
                    .collect();
                chart.set_data(crate::widget::SeriesId::MAIN, without_shared);
                assert!(chart.merge_trade_profile(1));
                chart.set_data(crate::widget::SeriesId::MAIN, full);
                assert!(chart.profile_edits_current());
                assert_eq!(
                    ranges(&chart),
                    expected,
                    "shared bars keep their natural profiles despite earlier edits"
                );
            }
        }
    }

    /// Clipping removes weekends and Monday pre-market, without requiring instrument hours.
    #[test]
    fn custom_sessions_clip_to_the_trading_hours() {
        for (clip, hours, expected) in [
            (
                false,
                true,
                vec![
                    "09 13:00-09 17:30",
                    "10 12:00-10 16:30",
                    "11 12:00-11 16:30",
                ],
            ),
            (true, true, vec!["11 13:30-11 16:30"]),
            (
                true,
                false,
                vec![
                    "09 13:00-09 17:30",
                    "10 12:00-10 16:30",
                    "11 12:00-11 16:30",
                ],
            ),
        ] {
            let mut custom = custom_windows();
            custom.sessions.truncate(1);
            custom.trading_hours_only = clip;
            let instrument = if hours {
                us_equity_hours()
            } else {
                Instrument::new("X").with_time_zone(TimezoneId::named("America/New_York").unwrap())
            };
            let chart = bars_every(30, 160, instrument, ProfileSession::Custom(custom));
            assert_eq!(
                ranges(&chart),
                expected,
                "clip {clip} instrument hours {hours}"
            );
            use crate::studies::{Indicator, InputValue, VolProfileSession};
            let mut study = VolProfileSession::new(24);
            study.set_timeframe(Timeframe::Min30);
            study.set_instrument(chart.instrument());
            for (id, value) in [
                ("sessions", InputValue::Choice(3)),
                ("hours_only", InputValue::Toggle(true)),
                ("session_start", InputValue::Length(480)),
                ("session_end", InputValue::Length(750)),
                ("clip_hours", InputValue::Toggle(clip)),
            ] {
                study.set_input(id, value).unwrap();
            }
            let bars = chart.bars(crate::widget::SeriesId::MAIN);
            study.calculate(bars);
            let observed: Vec<_> = study
                .volume_profiles(0..bars.len())
                .iter()
                .map(|p| {
                    format!(
                        "{}-{}",
                        bars[p.bars.start].time.format("%d %H:%M"),
                        bars[p.bars.end].time.format("%d %H:%M")
                    )
                })
                .collect();
            assert_eq!(observed, expected, "session study clipping");
        }
    }

    /// New York hours on weekdays: regular from 09:30 to 16:00, extended from 04:00 to 09:30 and
    /// from 16:00 to 20:00.
    fn us_equity_hours() -> Instrument {
        use Weekday::*;
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let week = [Mon, Tue, Wed, Thu, Fri];
        Instrument::new("X")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap())
            .with_session(
                Session::new([SessionWindow::new(week, at(9, 30), at(16, 0))])
                    .with_extended_windows([
                        SessionWindow::new(week, at(4, 0), at(9, 30)),
                        SessionWindow::new(week, at(16, 0), at(20, 0)),
                    ]),
            )
    }

    fn starts(chart: &Chart) -> Vec<String> {
        let ranges = chart.trade_profile_ranges().unwrap();
        ranges
            .ranges()
            .iter()
            .map(|r| r.start.format("%d %H:%M").to_string())
            .collect()
    }

    /// An overnight custom session opens at its own wall-clock time on both sides of a clock
    /// change, whatever the instrument's own hours say.
    #[test]
    fn custom_sessions_open_at_their_local_time_across_a_clock_change() {
        let new_york = TimezoneId::named("America/New_York").unwrap();
        let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let custom = ProfileSession::Custom(
            CustomProfileSession {
                start: at(18),
                end: at(17),
                zone: None,
            }
            .into(),
        );
        // Monday daytime hours in New York: only the custom session can explain the starts.
        let exchange = Instrument::new("X")
            .with_time_zone(new_york)
            .with_session(Session::new([SessionWindow::new(
                [Weekday::Mon],
                at(9),
                at(15),
            )]));
        let chart = hourly(exchange.clone(), custom.clone());
        // 18:00 is 23:00 UTC in winter and 22:00 UTC after the change.
        assert_eq!(
            starts(&chart),
            ["08 20:00", "08 23:00", "09 23:00", "10 22:00", "11 22:00"]
        );
        // Its own zone overrides the instrument's.
        let ProfileSession::Custom(mut other) = custom.clone() else {
            unreachable!()
        };
        other.sessions[0].zone = Some(TimezoneId::utc());
        let chart = hourly(exchange.clone(), ProfileSession::Custom(other));
        assert_eq!(starts(&chart)[..2], ["08 20:00", "09 18:00"]);
        let mut event_chart = chart;
        for interval in [
            Timeframe::Ticks(1.try_into().unwrap()),
            Timeframe::Volume(10.try_into().unwrap()),
            Timeframe::Range(4.try_into().unwrap()),
            Timeframe::Delta(5.try_into().unwrap()),
        ] {
            event_chart.set_timeframe(interval);
            assert_eq!(
                starts(&event_chart)[..2],
                ["08 20:00", "09 18:00"],
                "{interval:?}"
            );
        }

        // Other chart types keep the chart's calendar, so their menus find the same sessions.
        let mut other_type = hourly(exchange.clone(), ProfileSession::Chart);
        let plain = other_type.profile_clock();
        other_type.set_trade_profile_settings(TradeProfileSettings {
            session: custom,
            ..Default::default()
        });
        other_type.set_chart_type(ChartType::TimePriceOpportunity);
        assert_eq!(other_type.profile_clock(), plain);
    }

    /// Daily and longer bars profile by trading date whatever the custom hours' zone, and keep
    /// every bar under regular hours: a Paris instrument's bars stamped at Paris midnight keep
    /// their Monday-to-Friday weeks, and its monthly bars their calendar years.
    #[test]
    fn custom_zone_keeps_daily_profiles_on_their_trading_weeks() {
        use Weekday::*;
        let paris = TimezoneId::named("Europe/Paris").unwrap();
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let exchange = Instrument::new("X")
            .with_time_zone(paris)
            .with_session(Session::new([SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri],
                at(9, 0),
                at(17, 30),
            )]));
        // Paris midnight is 23:00 UTC the day before in winter (CET).
        let stamped = |times: &[&str]| -> Vec<Bar> {
            times
                .iter()
                .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 1.0))
                .collect()
        };
        // Trading dates Mon 4 to Fri 8 and Mon 11 to Fri 15 March 2024.
        let days = stamped(&[
            "2024-03-03T23:00:00Z",
            "2024-03-04T23:00:00Z",
            "2024-03-05T23:00:00Z",
            "2024-03-06T23:00:00Z",
            "2024-03-07T23:00:00Z",
            "2024-03-10T23:00:00Z",
            "2024-03-11T23:00:00Z",
            "2024-03-12T23:00:00Z",
            "2024-03-13T23:00:00Z",
            "2024-03-14T23:00:00Z",
        ]);
        // November and December 2023, January and February 2024.
        let months = stamped(&[
            "2023-10-31T23:00:00Z",
            "2023-11-30T23:00:00Z",
            "2023-12-31T23:00:00Z",
            "2024-01-31T23:00:00Z",
        ]);
        let held = |bars: &[Bar], timeframe, session: ProfileSession| {
            let mut chart = Chart::new(BarData::from_bars(bars.to_vec()));
            chart.set_chart_type(ChartType::SessionVolumeProfile);
            chart.set_timeframe(timeframe);
            chart.set_instrument(Some(exchange.clone()));
            chart.set_trade_profile_settings(TradeProfileSettings {
                session,
                ..Default::default()
            });
            let ranges = chart.trade_profile_ranges().unwrap();
            ranges
                .ranges()
                .iter()
                .map(|r| bars.iter().filter(|b| r.contains(&b.time)).count())
                .collect::<Vec<_>>()
        };
        let custom = |zone: &str| {
            ProfileSession::Custom(
                CustomProfileSession {
                    start: at(9, 0),
                    end: at(17, 30),
                    zone: Some(TimezoneId::named(zone).unwrap()),
                }
                .into(),
            )
        };
        let observed = [
            (
                "day chart",
                held(&days, Timeframe::Day1, ProfileSession::Chart),
            ),
            ("day UTC", held(&days, Timeframe::Day1, custom("UTC"))),
            (
                "day regular",
                held(&days, Timeframe::Day1, ProfileSession::Regular),
            ),
            (
                "day New York",
                held(&days, Timeframe::Day1, custom("America/New_York")),
            ),
            (
                "month chart",
                held(&months, Timeframe::Month1, ProfileSession::Chart),
            ),
            ("month UTC", held(&months, Timeframe::Month1, custom("UTC"))),
        ];
        let expected = [
            ("day chart", vec![5, 5]),
            ("day UTC", vec![5, 5]),
            ("day regular", vec![5, 5]),
            ("day New York", vec![5, 5]),
            ("month chart", vec![2, 2]),
            ("month UTC", vec![2, 2]),
        ];
        assert!(observed == expected, "bars per profile: {observed:?}");
    }

    /// Every choice but the trading day profiles only the bars of its hours, each part ending at
    /// the open of the bar after it: regular hours end Friday's profile at the 21:00 UTC close
    /// and start Monday's at the 13:30 open, and extended hours end Friday's at the 01:00
    /// post-market close. A merge of two segments leaves the next one apart and stays with its
    /// choice. Without extended hours the segment choices keep the chart's calendar.
    #[test]
    fn segment_sessions_profile_each_part_of_the_day_apart() {
        let exchange = us_equity_hours();
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let ranges = |chart: &Chart| -> Vec<String> {
            let ranges = chart.trade_profile_ranges().unwrap();
            let format = |time: DateTime<Utc>| time.format("%d %H:%M").to_string();
            (ranges.ranges().iter())
                .map(|r| format!("{}-{}", format(r.start), format(r.end)))
                .collect()
        };
        // In UTC, Friday's regular hours end at 21:00 and its post-market at 01:00; on Monday,
        // after the clock change, the pre-market runs from 08:00, the regular hours from 13:30
        // and the post-market from 20:00 to 00:00.
        let segments = [
            "08 20:00-08 21:00",
            "08 21:00-09 01:00",
            "11 08:00-11 14:00",
            "11 14:00-11 20:00",
            "11 20:00-12 00:00",
        ];
        for (session, expected) in [
            (ProfileSession::Segments, &segments[..]),
            (ProfileSession::PreMarket, &segments[2..3]),
            (ProfileSession::PostMarket, &[segments[1], segments[4]][..]),
            // Regular hours alone bucket the hours from their 09:30 open, so Friday's 20:00 bar
            // opens at 19:30; extended hours close at 20:00 New York time.
            (
                ProfileSession::Regular,
                &["08 19:30-08 21:00", "11 13:30-11 20:00"][..],
            ),
            (
                ProfileSession::Extended,
                &["08 20:00-09 01:00", "11 08:00-12 00:00"][..],
            ),
        ] {
            let chart = hourly(exchange.clone(), session.clone());
            assert_eq!(ranges(&chart), expected, "{session:?}");
        }
        let mut merged = hourly(exchange, ProfileSession::Segments);
        assert!(merged.merge_trade_profile(2));
        assert_eq!(
            ranges(&merged),
            [segments[0], segments[1], "11 08:00-11 20:00", segments[4]]
        );
        // On the same calendar, the post-market choice starts from its own profiles.
        merged.set_trade_profile_settings(TradeProfileSettings {
            session: ProfileSession::PostMarket,
            ..Default::default()
        });
        assert_eq!(ranges(&merged), [segments[1], segments[4]]);
        let regular = Instrument::new("X")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap())
            .with_session(Session::new([SessionWindow::new(
                [Weekday::Mon],
                at(9, 0),
                at(15, 0),
            )]));
        assert_eq!(
            ranges(&hourly(regular.clone(), ProfileSession::Segments)),
            ranges(&hourly(regular, ProfileSession::Chart))
        );
    }

    /// Each hours choice of the Time Price Opportunity study and the TPO chart type profiles
    /// only the bars of its hours, counting its periods from their open, on both sides of the
    /// New York clock change: custom hours from 08:00 to 12:30 in the exchange zone give one
    /// profile a day, and the trading day split by extended hours one per segment, whether the
    /// chart type's settings choose the segments or were saved with Split extended hours.
    #[test]
    fn tpo_study_hours_profile_only_their_bars() {
        use crate::model::to_tpo_profiles;
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        // (hours, split, profiles as first bar and period letters); periods are the bar's hour.
        type Case = (usize, bool, usize, bool, i64, &'static [&'static str]);
        let rows: [Case; 9] = [
            (1, false, 1, false, 60, &["08 20:00 F", "11 14:00 ABCDEF"]),
            (
                2,
                false,
                1,
                false,
                60,
                &["09 13:00 ABCDE", "10 12:00 ABCDE", "11 12:00 ABCDE"],
            ),
            (3, false, 1, false, 60, &["11 08:00 ABCDEF"]),
            (4, false, 1, false, 60, &["08 21:00 ABCD", "11 20:00 ABCD"]),
            (
                0,
                true,
                1,
                false,
                60,
                &[
                    "08 20:00 F",
                    "08 21:00 ABCD",
                    "11 08:00 ABCDEF",
                    "11 14:00 ABCDEF",
                    "11 20:00 ABCD",
                ],
            ),
            (
                2,
                false,
                2,
                false,
                60,
                &[
                    "08 20:00 D",
                    "09 13:00 ABCDE",
                    "09 17:00 ABCD",
                    "10 12:00 ABCDE",
                    "10 16:00 ABCD",
                    "11 12:00 ABCDE",
                    "11 16:00 ABCD",
                ],
            ),
            (
                2,
                false,
                2,
                true,
                60,
                &[
                    "08 20:00 D",
                    "09 13:00 ABCD",
                    "09 17:00 A",
                    "09 18:00 BCD",
                    "10 12:00 ABCD",
                    "10 16:00 A",
                    "10 17:00 BCD",
                    "11 12:00 ABCD",
                    "11 16:00 A",
                    "11 17:00 BCD",
                ],
            ),
            (
                2,
                false,
                1,
                false,
                30,
                &[
                    "09 13:00 ABCDEFGHI",
                    "10 12:00 ABCDEFGHI",
                    "11 12:00 ABCDEFGHI",
                ],
            ),
            (2, false, 1, true, 30, &["11 13:30 ABCDEF"]),
        ];
        let time = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        for (hours, split, count, own_or_clip, minutes, expected) in rows {
            let mut chart = bars_every(
                minutes,
                4800 / minutes,
                us_equity_hours(),
                ProfileSession::Chart,
            );
            let mut study = TimePriceOpportunity::default();
            for (id, value) in [
                ("hours", InputValue::Choice(hours)),
                ("split_sessions", InputValue::Toggle(split)),
                ("session_start", InputValue::Length(8 * 60)),
                ("session_end", InputValue::Length(12 * 60 + 30)),
                ("custom_sessions", InputValue::Choice(count - 1)),
                ("session_2_start", InputValue::Length(12 * 60)),
                ("session_2_end", InputValue::Length(16 * 60)),
                (
                    "session_overlap",
                    InputValue::Choice(usize::from(!own_or_clip)),
                ),
                ("clip_hours", InputValue::Toggle(count == 1 && own_or_clip)),
                ("period", InputValue::Length(minutes as usize)),
            ] {
                study.set_input(id, value).unwrap();
            }
            // As the chart draws the study, on rows of one price unit.
            let config = study.tpo().unwrap();
            let (clock, mut config) = chart.tpo_division(config);
            config.tick_size = 1.0;
            // The chart type with the same choice, and the segments chosen without the legacy flag.
            let mut params = crate::model::ChartTypeParams {
                tpo_session: match hours {
                    1 => ProfileSession::Regular,
                    2 => ProfileSession::Custom(CustomProfileSessions {
                        sessions: [
                            CustomProfileSession::new(time(8, 0), time(12, 30)),
                            CustomProfileSession::new(time(12, 0), time(16, 0)),
                        ][..count]
                            .to_vec(),
                        overlap: if own_or_clip {
                            SessionOverlap::OwnProfile
                        } else {
                            SessionOverlap::InBoth
                        },
                        trading_hours_only: count == 1 && own_or_clip,
                    }),
                    3 => ProfileSession::PreMarket,
                    4 => ProfileSession::PostMarket,
                    _ => ProfileSession::Chart,
                },
                tpo_split_sessions: split,
                tpo_period_minutes: minutes as u32,
                ..Default::default()
            };
            chart.set_chart_type(ChartType::TimePriceOpportunity);
            chart.set_chart_type_params(params.clone());
            let mut divisions = vec![("study", (clock.clone(), config.clone()))];
            divisions.push(("chart type", chart.tpo_chart_division()));
            if split {
                params.tpo_session = ProfileSession::Segments;
                params.tpo_split_sessions = false;
                chart.set_chart_type_params(params);
                divisions.push(("chart type segments", chart.tpo_chart_division()));
            }
            for (owner, (clock, mut config)) in divisions {
                config.tick_size = 1.0;
                let observed: Vec<_> =
                    to_tpo_profiles(chart.bars(crate::widget::SeriesId::MAIN), &config, &clock)
                        .iter()
                        .map(|profile| {
                            let mut periods: Vec<_> = (profile.letters.iter())
                                .map(|l| (l.period_idx, l.letter))
                                .collect();
                            periods.sort_unstable();
                            periods.dedup();
                            let letters: String = periods.into_iter().map(|(_, l)| l).collect();
                            format!("{} {letters}", profile.session_date.format("%d %H:%M"))
                        })
                        .collect();
                assert_eq!(observed, expected, "{owner}, hours {hours} split {split}");
            }
            if count == 2 {
                let weekly = crate::model::TPOConfig {
                    profile_period: crate::model::TPOProfilePeriod::Week,
                    ..config.clone()
                };
                let profiles =
                    to_tpo_profiles(chart.bars(crate::widget::SeriesId::MAIN), &weekly, &clock);
                let dates: Vec<_> = profiles
                    .iter()
                    .map(|p| p.session_date.format("%d %H:%M").to_string())
                    .collect();
                let expected_dates = if own_or_clip {
                    vec!["08 20:00", "11 12:00"]
                } else {
                    vec!["08 20:00", "09 13:00", "11 12:00", "11 16:00"]
                };
                assert_eq!(dates, expected_dates, "weekly session layers");
                assert_eq!(
                    profiles.iter().map(|p| p.letters.len()).sum::<usize>(),
                    if own_or_clip { 25 * 4 } else { 28 * 4 }
                );
            }
            let saved = study.tpo().unwrap().clone();
            study
                .set_input("custom_sessions", InputValue::Choice(0))
                .unwrap();
            study
                .set_input("custom_sessions", InputValue::Choice(count - 1))
                .unwrap();
            assert_eq!(study.tpo(), Some(&saved), "inactive window values survive");
            let inputs = study.inputs();
            let second = inputs
                .iter()
                .find(|input| input.id == "session_2_start")
                .unwrap();
            assert_eq!(second.is_shown(&inputs), hours == 2 && count == 2);
        }
        // Week profiles keep the chart's hours whatever the one-day choice.
        let mut chart = bars_every(60, 80, us_equity_hours(), ProfileSession::Chart);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        chart.set_chart_type_params(crate::model::ChartTypeParams {
            tpo_profile_period: crate::model::TPOProfilePeriod::Week,
            tpo_session: ProfileSession::PreMarket,
            ..Default::default()
        });
        assert_eq!(chart.tpo_chart_division().1.session, ProfileSession::Chart);
    }

    /// Custom hours keep whole TPO periods on the grid of the period length: a period that opens
    /// before the custom start is left out, the last one keeps its bars past the custom end, and
    /// clipped hours use the grid of the regular hours. Bar `i` opens at `5 * i` minutes past
    /// midnight and trades only `100 + i`, so prices read as times.
    #[test]
    fn tpo_custom_hours_keep_whole_periods_on_their_grid() {
        use crate::model::to_tpo_profiles;
        use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
        use Weekday::*;
        let start: DateTime<Utc> = "2024-03-05T00:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..288)
            .map(|i| Bar::from_value(start + TimeDelta::minutes(5 * i), 100.0 + i as f64))
            .collect();
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let regular = Session::new([SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            at(9, 30),
            at(16, 0),
        )]);
        // (start, end, period, clip) in minutes past midnight.
        let rows = [
            (8 * 60, 12 * 60 + 30, 240, false),
            (9 * 60 + 45, 12 * 60, 60, false),
            (8 * 60 + 10, 12 * 60 + 30, 30, false),
            (9 * 60 + 45, 12 * 60, 60, true),
            (8 * 60 + 10, 8 * 60 + 25, 30, false),
        ];
        let mut observed = Vec::new();
        for (from, to, period, clip) in rows {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_timeframe(Timeframe::Min5);
            chart.set_instrument(Some(
                Instrument::new("X")
                    .with_time_zone(TimezoneId::utc())
                    .with_session(regular.clone()),
            ));
            let mut study = TimePriceOpportunity::default();
            for (id, value) in [
                ("hours", InputValue::Choice(2)),
                ("session_start", InputValue::Length(from)),
                ("session_end", InputValue::Length(to)),
                ("clip_hours", InputValue::Toggle(clip)),
                ("period", InputValue::Length(period)),
            ] {
                study.set_input(id, value).unwrap();
            }
            let (clock, mut config) = chart.tpo_division(study.tpo().unwrap());
            config.tick_size = 1.0;
            let mut row = Vec::new();
            for profile in
                to_tpo_profiles(chart.bars(crate::widget::SeriesId::MAIN), &config, &clock)
            {
                row.push(format!(
                    "{} {}-{}",
                    profile.session_date.format("%H:%M"),
                    profile.profile_low,
                    profile.profile_high
                ));
                let mut periods = std::collections::BTreeMap::new();
                for l in &profile.letters {
                    let (_, _, lo, hi) = periods
                        .entry(l.period_idx)
                        .or_insert((l.letter, l.ts, l.price, l.price));
                    *lo = l.price.min(*lo);
                    *hi = l.price.max(*hi);
                }
                for (letter, ts, lo, hi) in periods.into_values() {
                    row.push(format!("{letter} {} {lo}-{hi}", ts.format("%H:%M")));
                }
            }
            observed.push(row.join(", "));
        }
        assert_eq!(
            observed,
            [
                "08:00 196-291, A 08:00 196-243, B 12:00 244-291",
                "10:00 220-243, A 10:00 220-231, B 11:00 232-243",
                "08:30 202-249, A 08:30 202-207, B 09:00 208-213, C 09:30 214-219, \
                 D 10:00 220-225, E 10:30 226-231, F 11:00 232-237, G 11:30 238-243, \
                 H 12:00 244-249",
                "10:30 226-249, A 10:30 226-237, B 11:30 238-249",
                "",
            ]
        );
    }
}
