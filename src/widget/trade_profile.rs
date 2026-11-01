//! Display settings and timestamp-based edits for execution profiles.

use super::frame::session_ranges;
use super::{Chart, SeriesId};
use crate::chart::overlays::TradeProfileSettings;
use crate::model::{BarClock, SessionProvider};
use crate::studies::TradeProfileRanges;
use chrono::{DateTime, Utc};
use std::ops::{Range, RangeInclusive};

#[derive(Clone, Default)]
pub(super) struct TradeProfiles {
    settings: TradeProfileSettings,
    context: Option<(String, BarClock)>,
    spans: Vec<RangeInclusive<DateTime<Utc>>>,
    splits: Vec<DateTime<Utc>>,
}

impl Chart {
    /// Settings used when drawing [`crate::model::ChartType::SessionVolumeProfile`].
    pub fn trade_profile_settings(&self) -> &TradeProfileSettings {
        &self.trade_profiles.settings
    }

    /// Changes the session profile display without changing its executions or ranges.
    pub fn set_trade_profile_settings(&mut self, settings: TradeProfileSettings) {
        self.trade_profiles.settings = settings;
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

    pub(crate) fn profile_clock(&self) -> BarClock {
        let mut clock = self.bar_clock();
        if let Some(zone) = self
            .instrument()
            .filter(|i| i.session.is_none())
            .and_then(|i| i.time_zone.as_ref())
        {
            clock = clock.with_timezone(zone.clone());
        }
        clock
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
