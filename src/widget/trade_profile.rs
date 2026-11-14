//! Display settings and timestamp-based edits for execution profiles.

use super::frame::session_ranges;
use super::{Chart, SeriesId};
use crate::chart::overlays::TradeProfileSettings;
use crate::model::{
    BarClock, ChartType, ProfileSession, Session, SessionProvider, SessionWindow, Timeframe,
    TimezoneId,
};
use crate::studies::TradeProfileRanges;
use chrono::{DateTime, Utc, Weekday};
use std::ops::{Range, RangeInclusive};

#[derive(Clone, Default)]
pub(super) struct TradeProfiles {
    context: Option<(String, BarClock)>,
    spans: Vec<RangeInclusive<DateTime<Utc>>>,
    splits: Vec<DateTime<Utc>>,
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
    /// Merge and split edits retain UTC boundaries across history eviction and prepending.
    pub fn trade_profile_ranges(&self) -> Option<TradeProfileRanges> {
        self.execution_profile_ranges(0..self.bars(SeriesId::MAIN).len())
    }

    /// Merges the indexed execution profile with its next neighbour. Returns false if absent.
    pub fn merge_trade_profile(&mut self, index: usize) -> bool {
        let Some(ranges) = self.trade_profile_ranges() else {
            return false;
        };
        let Some(pair) = ranges.ranges().get(index..).and_then(|r| r.get(..2)) else {
            return false;
        };
        let bars = self.bars(SeriesId::MAIN);
        let end = bars.partition_point(|bar| bar.time < pair[1].end);
        let Some(last) = end.checked_sub(1).and_then(|i| bars.get(i)) else {
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
        let span = range.start..=bars[end - 1].time;
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

    /// The calendar that divides the chart into profile sessions, as the settings choose it.
    pub(crate) fn profile_clock(&self) -> BarClock {
        let mut clock = self.bar_clock();
        let instrument = self.instrument();
        let exchange_zone = || {
            instrument
                .and_then(|i| i.time_zone.clone())
                .unwrap_or_else(TimezoneId::utc)
        };
        // The profile settings divide only the session volume profile chart: the market profile
        // chart and its menu keep the chart's calendar.
        let selection = match self.chart_type {
            ChartType::SessionVolumeProfile => &self.trade_profile_settings().session,
            _ => &ProfileSession::Chart,
        };
        // Daily and longer bars carry a trading date rather than a time of day to divide or re-zone.
        let intraday =
            self.timeframe.unwrap_or_default().duration_ms() < Timeframe::Day1.duration_ms();
        match selection {
            ProfileSession::Custom(custom) if intraday => {
                use Weekday::*;
                let zone = custom.zone.clone().unwrap_or_else(exchange_zone);
                let every_day = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
                let session =
                    Session::new([SessionWindow::new(every_day, custom.start, custom.end)]);
                clock.with_session(zone, session, false)
            }
            // Without instrument hours there is nothing to select among, so the chart's calendar stays.
            selection @ (ProfileSession::Regular | ProfileSession::Extended)
                if instrument.is_some_and(|i| i.session.is_some()) =>
            {
                let session = instrument.and_then(|i| i.session.clone()).unwrap();
                let extended = *selection == ProfileSession::Extended;
                clock.with_session(exchange_zone(), session, extended)
            }
            _ => {
                if let Some(zone) = instrument
                    .filter(|i| i.session.is_none())
                    .and_then(|i| i.time_zone.as_ref())
                {
                    clock = clock.with_timezone(zone.clone());
                }
                clock
            }
        }
    }

    fn profile_edits_current(&self) -> bool {
        self.trade_profiles
            .context
            .as_ref()
            .is_some_and(|(symbol, clock)| {
                symbol == self.symbol() && *clock == self.profile_clock()
            })
    }

    fn prepare_profile_edits(&mut self) {
        if !self.profile_edits_current() {
            self.reset_trade_profile_ranges();
            self.trade_profiles.context = Some((self.symbol().to_owned(), self.profile_clock()));
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
        let clock = self.profile_clock();
        let sessions = clock.sessions();
        let mut natural = session_ranges(bars, shown.clone(), &clock)?;
        let current = self.profile_edits_current();
        if current {
            let first = natural.ranges().first()?.start;
            let end = natural.ranges().last()?.end;
            let original = shown.clone();
            for merged in &self.trade_profiles.spans {
                if *merged.start() < end
                    && (*merged.end() >= first
                        || sessions
                            .session_break_between(merged.end(), &first)
                            .is_none())
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
                natural = session_ranges(bars, shown, &clock)?;
            }
        }
        if !current {
            return Some(natural);
        }
        let mut ranges: Vec<Range<DateTime<Utc>>> = Vec::new();
        for range in natural.ranges() {
            let merged = self.trade_profiles.spans.iter().find(|r| {
                *r.start() < range.end
                    && (*r.end() >= range.start
                        || sessions
                            .session_break_between(r.end(), &range.start)
                            .is_none())
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

#[cfg(all(test, feature = "timezones"))]
mod tests {
    use crate::chart::overlays::TradeProfileSettings;
    use crate::model::{
        Bar, BarData, ChartType, CustomProfileSession, Instrument, ProfileSession, Session,
        SessionWindow, Timeframe, TimezoneId,
    };
    use crate::widget::Chart;
    use chrono::{DateTime, NaiveTime, TimeDelta, Utc, Weekday};

    fn hourly(instrument: Instrument, session: ProfileSession) -> Chart {
        // Hourly bars from Friday 2024-03-08 20:00 UTC across the New York clock change early
        // on Sunday March 10.
        let start: DateTime<Utc> = "2024-03-08T20:00:00Z".parse().unwrap();
        let bars = (0..80)
            .map(|i| Bar::new(start + TimeDelta::hours(i), 100.0, 102.0, 99.0, 101.0, 1.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_chart_type(ChartType::SessionVolumeProfile);
        chart.set_timeframe(Timeframe::Hour1);
        chart.set_instrument(Some(instrument));
        chart.set_trade_profile_settings(TradeProfileSettings {
            session,
            ..Default::default()
        });
        chart
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
        let custom = ProfileSession::Custom(CustomProfileSession {
            start: at(18),
            end: at(17),
            zone: None,
        });
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
        other.zone = Some(TimezoneId::utc());
        let chart = hourly(exchange.clone(), ProfileSession::Custom(other));
        assert_eq!(starts(&chart)[..2], ["08 20:00", "09 18:00"]);
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

    /// Daily and longer bars profile by trading date whatever the custom hours' zone: a Paris
    /// instrument's bars stamped at Paris midnight keep their Monday-to-Friday weeks, and its
    /// monthly bars their calendar years.
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
            ProfileSession::Custom(CustomProfileSession {
                start: at(9, 0),
                end: at(17, 30),
                zone: Some(TimezoneId::named(zone).unwrap()),
            })
        };
        let observed = [
            (
                "day chart",
                held(&days, Timeframe::Day1, ProfileSession::Chart),
            ),
            ("day UTC", held(&days, Timeframe::Day1, custom("UTC"))),
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
            ("day New York", vec![5, 5]),
            ("month chart", vec![2, 2]),
            ("month UTC", vec![2, 2]),
        ];
        assert!(observed == expected, "bars per profile: {observed:?}");
    }
}
