//! The same complete-bar view over owned accumulation and immutable prepared pages.

use super::{Footprint, FootprintBar, FootprintBarView};
use crate::data::FootprintSource;
use chrono::{DateTime, Utc};
use std::{
    fmt,
    ops::{Bound, RangeBounds},
    sync::Arc,
};

#[derive(Clone)]
struct PreparedPage {
    source: Arc<dyn FootprintSource>,
    first: DateTime<Utc>,
    end: usize,
    start: usize,
}
impl fmt::Debug for PreparedPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedPage")
            .field("owner", &Arc::as_ptr(&self.source))
            .field("first", &self.first)
            .field("bars", &self.end)
            .finish()
    }
}
impl PreparedPage {
    fn partition(&self, before: impl Fn(DateTime<Utc>) -> bool) -> usize {
        let (mut left, mut right) = (self.start, self.end);
        while left < right {
            let middle = left + (right - left) / 2;
            if before(self.source.bar(middle).time) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }
    fn range(
        &self,
        start: Bound<DateTime<Utc>>,
        end: Bound<DateTime<Utc>>,
    ) -> std::ops::Range<usize> {
        let first = match start {
            Bound::Unbounded => self.start,
            Bound::Included(time) => self.partition(|at| at < time),
            Bound::Excluded(time) => self.partition(|at| at <= time),
        };
        let end = match end {
            Bound::Unbounded => self.end,
            Bound::Included(time) => self.partition(|at| at <= time),
            Bound::Excluded(time) => self.partition(|at| at < time),
        };
        first.min(end)..end
    }
}

/// Borrowed executions across owned bars and immutable prepared history, without copying rows.
/// Construct from an owned [`Footprint`] with `(&footprint).into()`. Chart drawing callbacks
/// receive the same view over their feed's shared history. Available with `widget` or `studies`.
#[derive(Clone, Copy, Debug)]
pub struct FootprintView<'a> {
    pub(super) flow: &'a Footprint,
    pages: &'a [PreparedPage],
    closed: &'a [Arc<FootprintBar>],
}
impl<'a> From<&'a Footprint> for FootprintView<'a> {
    fn from(flow: &'a Footprint) -> Self {
        Self {
            flow,
            pages: &[],
            closed: &[],
        }
    }
}
impl<'a> FootprintView<'a> {
    pub(crate) fn clock(self) -> &'a crate::model::BarClock {
        self.flow.clock()
    }
    /// Instrument tick size used by the price-level keys.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub fn tick_size(self) -> f64 {
        self.flow.tick_size()
    }
    /// Price width of one grouped row.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub fn price_step(self) -> f64 {
        self.flow.price_step()
    }
    #[cfg(feature = "widget")]
    pub(crate) fn level_of(self, price: f64) -> Option<i64> {
        self.flow.level_of(price)
    }
    #[cfg(all(feature = "widget", feature = "studies"))]
    pub(crate) fn revision(self) -> u64 {
        self.flow.revision()
    }
    pub(crate) fn first_time(self) -> Option<DateTime<Utc>> {
        self.pages
            .first()
            .map(|page| page.first)
            .or_else(|| self.closed.first().map(|bar| bar.time))
            .or_else(|| self.flow.bars.first_key_value().map(|(time, _)| *time))
    }
    pub(crate) fn bar_at(self, time: DateTime<Utc>) -> Option<FootprintBarView<'a>> {
        let time = self.flow.clock.bar_open(time);
        if let Some(bar) = self.flow.bars.get(&time) {
            return Some(bar.into());
        }
        if let Ok(index) = self.closed.binary_search_by_key(&time, |bar| bar.time) {
            return Some(self.closed[index].as_ref().into());
        }
        let page = self.pages.get(
            self.pages
                .partition_point(|page| page.first <= time)
                .checked_sub(1)?,
        )?;
        let index = page.partition(|at| at < time);
        (index < page.end)
            .then(|| page.source.bar(index))
            .filter(|bar| bar.time == time)
    }
    pub(crate) fn bars(
        self,
        range: impl RangeBounds<DateTime<Utc>>,
    ) -> impl DoubleEndedIterator<Item = FootprintBarView<'a>> {
        let (start, end) = (range.start_bound().cloned(), range.end_bound().cloned());
        let first_page = match start {
            Bound::Unbounded => 0,
            Bound::Included(time) | Bound::Excluded(time) => self
                .pages
                .partition_point(|page| page.first <= time)
                .saturating_sub(1),
        };
        let last_page = match end {
            Bound::Unbounded => self.pages.len(),
            Bound::Included(time) => self.pages.partition_point(|page| page.first <= time),
            Bound::Excluded(time) => self.pages.partition_point(|page| page.first < time),
        };
        let shared = self.pages[first_page.min(last_page)..last_page]
            .iter()
            .flat_map(move |page| {
                page.range(start, end)
                    .map(move |index| page.source.bar(index))
            });
        let first = self.closed.partition_point(|bar| match start {
            Bound::Unbounded => false,
            Bound::Included(time) => bar.time < time,
            Bound::Excluded(time) => bar.time <= time,
        });
        let last = self.closed.partition_point(|bar| match end {
            Bound::Unbounded => true,
            Bound::Included(time) => bar.time <= time,
            Bound::Excluded(time) => bar.time < time,
        });
        shared
            .chain(
                self.closed[first.min(last)..last]
                    .iter()
                    .map(|bar| bar.as_ref().into()),
            )
            .chain(
                self.flow
                    .bars
                    .range((start, end))
                    .map(|(_, bar)| bar.into()),
            )
    }
    /// Match aligned opens directly; use the clock for other timestamps and seek across gaps.
    #[cfg(feature = "widget")]
    pub(crate) fn matching(
        self,
        bars: &[crate::model::Bar],
    ) -> impl Iterator<Item = Option<FootprintBarView<'a>>> {
        let mut rows = self.bars((Bound::Unbounded, Bound::Unbounded)).peekable();
        let mut previous = None;
        let mut matched: Option<FootprintBarView<'a>> = None;
        bars.iter().map(move |bar| {
            let open = if matched.is_some_and(|row| row.time == bar.time)
                || rows.peek().is_some_and(|row| row.time == bar.time)
            {
                bar.time
            } else {
                self.flow.clock.bar_open(bar.time)
            };
            if previous == Some(open) {
                return matched;
            }
            if previous.is_some_and(|last| open < last)
                || rows.peek().is_some_and(|row| row.time < open)
            {
                rows = self
                    .bars((Bound::Included(open), Bound::Unbounded))
                    .peekable();
            }
            previous = Some(open);
            matched = rows.next_if(|row| row.time == open);
            matched
        })
    }
    #[cfg(feature = "widget")]
    pub(crate) fn complete_at(self, open: DateTime<Utc>) -> bool {
        self.flow.complete_at(open)
    }
    /// Complete bars opening in `range`, including prepared bars before a missing join.
    /// The range may still contain gaps; check [`span_is_complete`](Self::span_is_complete)
    /// before treating a calculation over it as complete. Panics for invalid bounds as
    /// [`std::collections::BTreeMap::range`] does.
    pub fn complete_bars(
        self,
        range: impl RangeBounds<DateTime<Utc>>,
    ) -> impl DoubleEndedIterator<Item = FootprintBarView<'a>> {
        self.bars(range)
            .filter(move |bar| self.flow.complete_at(bar.time))
    }
    #[cfg(feature = "widget")]
    pub(crate) fn complete_bar_at(self, time: DateTime<Utc>) -> Option<FootprintBarView<'a>> {
        self.bar_at(time)
            .filter(|bar| self.flow.complete_at(bar.time))
    }
    pub(crate) fn complete_from(self) -> Option<DateTime<Utc>> {
        self.bars((self.flow.complete_start(), Bound::Unbounded))
            .next()
            .map(|bar| bar.time)
    }
    pub(crate) fn span_has_gap(self, from: DateTime<Utc>, through: Option<DateTime<Utc>>) -> bool {
        self.flow.span_has_gap(from, through)
    }
    /// Whether all executions are known in the bars containing `first` through `last`, inclusive.
    /// Quiet slots are complete; a missing join or a span before held history is not.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub fn span_is_complete(self, first: DateTime<Utc>, last: DateTime<Utc>) -> bool {
        self.span_is_complete_from(self.first_time(), first, last)
    }
    pub(crate) fn span_is_complete_from(
        self,
        covered_from: Option<DateTime<Utc>>,
        first: DateTime<Utc>,
        last: DateTime<Utc>,
    ) -> bool {
        let (first, last) = (
            self.flow.clock.bar_open(first),
            self.flow.clock.bar_open(last),
        );
        first <= last
            && covered_from.is_some_and(|held| held <= first)
            && self.flow.complete_at(first)
            && self.flow.complete_at(last)
            && !self.span_has_gap(first, Some(last))
    }

    /// Sum complete bars containing `first` through `last`, inclusive, into an owned profile.
    /// As in [`Footprint::profile`], incomplete bars are omitted and reversed bounds give an
    /// empty profile. This allocates the summed price rows, without copying the input history.
    #[cfg(any(feature = "widget", feature = "studies"))]
    pub fn profile(self, first: DateTime<Utc>, last: DateTime<Utc>) -> super::FootprintBar {
        let (first, last) = (
            self.flow.clock.bar_open(first),
            self.flow.clock.bar_open(last),
        );
        let mut profile = super::FootprintBar::empty(first);
        if first <= last {
            for bar in self.complete_bars(first..=last) {
                profile.absorb_grouped(bar, 1);
            }
        }
        profile
    }
}

/// Shared history stays outside the mutable footprint and its public tree accessor.
#[derive(Clone, Debug)]
pub(crate) struct FootprintState {
    pub(crate) live: Footprint,
    pages: Vec<PreparedPage>,
    closed: Vec<Arc<FootprintBar>>,
    closed_rows: usize,
}
impl FootprintState {
    pub(crate) fn new(live: Footprint) -> Self {
        Self {
            live,
            pages: Vec::new(),
            closed: Vec::new(),
            closed_rows: 0,
        }
    }
    pub(crate) fn view(&self) -> FootprintView<'_> {
        FootprintView {
            flow: &self.live,
            pages: &self.pages,
            closed: &self.closed,
        }
    }

    /// Move an owned prepared prefix into a shared owner when another consumer attaches.
    #[cfg(feature = "widget")]
    pub(crate) fn share_prepared(&mut self) {
        let Some(before) = self.live.prepared_before() else {
            return;
        };
        let open = self.live.clock.bar_open(before);
        if self
            .live
            .bars
            .first_key_value()
            .is_none_or(|(at, _)| *at >= open)
        {
            return;
        }
        let tail = self.live.bars.split_off(&open);
        let bars: Vec<_> = std::mem::replace(&mut self.live.bars, tail)
            .into_values()
            .collect();
        self.live.rows -= bars.iter().map(|bar| bar.levels.len()).sum::<usize>();
        self.pages.push(PreparedPage {
            first: bars[0].time,
            end: bars.len(),
            start: 0,
            source: Arc::new(bars),
        });
    }

    /// Share admitted history and its boundary seed, without copying the live suffix.
    #[cfg(feature = "widget")]
    pub(crate) fn prepared_snapshot(&self) -> Option<Self> {
        self.live.prepared_before()?;
        let live = Footprint {
            clock: self.live.clock.clone(),
            bars: Default::default(),
            trade_bars: Default::default(),
            trade_starts: Default::default(),
            rows: 0,
            missing: self.live.missing.clone(),
            prepared: self.live.prepared.clone(),
            ..self.live
        };
        Some(Self {
            live,
            pages: self.pages.clone(),
            closed: Vec::new(),
            closed_rows: 0,
        })
    }

    /// Move finished source bars without copying price rows; only the newest can change.
    pub(crate) fn seal(&mut self) {
        while self.live.bars.len() > 1 {
            let (_, bar) = self.live.bars.pop_first().unwrap();
            self.live.rows -= bar.levels.len();
            self.closed_rows += bar.levels.len();
            self.closed.push(Arc::new(bar));
        }
    }

    pub(crate) fn over_budget(&self, bars: usize, rows: usize) -> bool {
        self.closed.len() + self.live.bars.len() > bars || self.closed_rows + self.live.rows > rows
    }

    /// Source retention must not follow the independently retained prepared pages.
    pub(crate) fn first_source_time(&self) -> Option<DateTime<Utc>> {
        self.closed
            .first()
            .map(|bar| bar.time)
            .or_else(|| self.live.bars.first_key_value().map(|(time, _)| *time))
    }

    /// The mutable source has the same retention as before; prepared owners are independent.
    pub(crate) fn retain(&mut self, bars: usize, rows: usize) -> bool {
        let mut count = 0;
        while count < self.closed.len()
            && (self.closed.len() - count + self.live.bars.len() > bars
                || self.closed_rows + self.live.rows > rows)
        {
            let bar = &self.closed[count];
            self.closed_rows -= bar.levels.len();
            self.live.trade_starts.remove(&bar.time);
            #[cfg(feature = "widget")]
            if let Some(before) = self.live.prepared_before() {
                let end = self
                    .live
                    .clock
                    .next_open(bar.time)
                    .unwrap_or(DateTime::<Utc>::MAX_UTC);
                self.live.add_missing(before, end);
            } else {
                self.live.incomplete_through = self.live.incomplete_through.max(Some(bar.time));
            }
            #[cfg(not(feature = "widget"))]
            {
                self.live.incomplete_through = self.live.incomplete_through.max(Some(bar.time));
            }
            count += 1;
        }
        self.closed.drain(..count);
        count > 0
    }

    /// Capture rows without copying history or the mutable accumulator's trade-start index.
    pub(crate) fn snapshot(&self, from: DateTime<Utc>, before: Option<DateTime<Utc>>) -> Self {
        let mut live = Footprint {
            clock: self.live.clock.clone(),
            bars: self
                .live
                .bars
                .range(from..)
                .take_while(|(time, _)| before.is_none_or(|end| **time < end))
                .map(|(time, bar)| (*time, bar.clone()))
                .collect(),
            trade_bars: Default::default(),
            trade_starts: Default::default(),
            missing: self.live.missing.clone(),
            #[cfg(feature = "widget")]
            prepared: None,
            ..self.live
        };
        live.rows = live.bars.values().map(|bar| bar.levels.len()).sum();
        let pages = self
            .pages
            .iter()
            .filter_map(|page| {
                let end = before.map_or(page.end, |end| page.partition(|at| at < end));
                let start = page.partition(|at| at < from).min(end);
                (start < end).then(|| PreparedPage {
                    start,
                    end,
                    first: page.source.bar(start).time,
                    ..page.clone()
                })
            })
            .collect();
        let first = self.closed.partition_point(|bar| bar.time < from);
        let end = before.map_or(self.closed.len(), |end| {
            self.closed.partition_point(|bar| bar.time < end)
        });
        let closed = self.closed[first.min(end)..end].to_vec();
        let closed_rows = closed.iter().map(|bar| bar.levels.len()).sum();
        Self {
            live,
            pages,
            closed,
            closed_rows,
        }
    }

    pub(crate) fn insert_partial(&mut self, bar: FootprintBar) {
        self.live.rows += bar.levels.len();
        self.live.bars.insert(bar.time, bar);
    }

    #[cfg(feature = "widget")]
    pub(crate) fn drop_through<'a>(
        &mut self,
        through: DateTime<Utc>,
        held: impl IntoIterator<Item = &'a crate::model::Tick>,
    ) {
        // The existing accumulator owns the exchange-time index even for sealed bars.
        self.live.drop_through(through, held);
        if let Some(cut) = self.live.incomplete_through {
            let end = self.closed.partition_point(|bar| bar.time <= cut);
            for bar in self.closed.drain(..end) {
                self.closed_rows -= bar.levels.len();
            }
        }
    }

    /// The original tree API is available only when it describes the entire input.
    #[cfg(feature = "widget")]
    pub(crate) fn owned(&self) -> Option<&Footprint> {
        (self.pages.is_empty() && self.closed.is_empty()).then_some(&self.live)
    }

    #[cfg(feature = "widget")]
    pub(crate) fn from_page(
        request: &crate::data::FootprintRequest,
        mut bars: crate::data::FootprintBars,
        retain: bool,
    ) -> Result<Self, crate::data::FeedError> {
        use crate::data::{FeedError, FootprintBars};
        if !retain && let FootprintBars::Owned(bars) = bars {
            return Footprint::from_bars(
                request.clock.clone(),
                request.tick_size,
                request.ticks_per_row,
                bars,
            )
            .map(Self::new);
        }
        let live = Footprint::from_bars(
            request.clock.clone(),
            request.tick_size,
            request.ticks_per_row,
            [],
        )?;
        if bars.is_empty() {
            return Ok(Self::new(live));
        }
        if let FootprintBars::Owned(bars) = &mut bars {
            bars.sort_unstable_by_key(|bar| bar.time);
        }
        let (mut previous, mut trades) = (None, 0);
        for bar in bars.iter() {
            if previous.is_some_and(|time| time >= bar.time) {
                return Err(FeedError::InvalidData(format!(
                    "footprint bar at {} is unordered or supplied more than once",
                    bar.time
                )));
            }
            super::validate_snapshot_bar(&live, bar, &mut trades)?;
            previous = Some(bar.time);
        }
        let source: Arc<dyn FootprintSource> = match bars {
            FootprintBars::Owned(bars) => Arc::new(bars),
            FootprintBars::Shared(source) => source,
        };
        let page = PreparedPage {
            first: source.bar(0).time,
            end: source.len(),
            start: 0,
            source,
        };
        Ok(Self {
            live,
            pages: vec![page],
            closed: Vec::new(),
            closed_rows: 0,
        })
    }

    #[cfg(feature = "widget")]
    pub(crate) fn prepare_history(
        &mut self,
        before: DateTime<Utc>,
        trades: &crate::data::trades::Trades,
        from_start: bool,
    ) {
        let open = self.live.clock.bar_open(before);
        if let Some(page) = self.pages.last_mut()
            && page.source.bar(page.end - 1).time == open
        {
            let source = page.source.bar(page.end - 1);
            let mut bar = super::FootprintBar::empty(open);
            bar.absorb_grouped(source, 1);
            self.live.rows += bar.levels.len();
            self.live.bars.insert(open, bar);
            page.end -= 1;
            if page.end == page.start {
                self.pages.pop();
            }
        }
        self.live.prepare_history(before, trades, from_start);
    }
    #[cfg(feature = "widget")]
    pub(crate) fn prepend_prepared(&mut self, mut page: Self) {
        debug_assert!(self.pages.is_empty() || page.live.bars.is_empty());
        page.pages.append(&mut self.pages);
        self.pages = page.pages;
        self.live.prepend_prepared(page.live);
    }
}

#[cfg(all(test, feature = "widget"))]
mod tests {
    use super::*;
    use crate::data::{
        FootprintBar, FootprintBars, FootprintLevel, FootprintRequest, FootprintRows,
    };
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use chrono::TimeDelta;

    struct Native(Vec<(FootprintBar, Vec<FootprintLevel>)>);
    impl Native {
        fn new(bars: Vec<FootprintBar>) -> Self {
            Self(
                bars.into_iter()
                    .map(|mut bar| {
                        let rows = std::mem::take(&mut bar.levels).into_values().collect();
                        (bar, rows)
                    })
                    .collect(),
            )
        }
    }
    impl FootprintSource for Native {
        fn len(&self) -> usize {
            self.0.len()
        }
        fn bar(&self, index: usize) -> FootprintBarView<'_> {
            let (meta, rows) = &self.0[index];
            FootprintBarView {
                levels: FootprintRows::Slice(rows),
                ..meta.into()
            }
        }
    }
    fn at(minute: i64) -> DateTime<Utc> {
        "2026-01-05T00:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::minutes(minute)
    }
    fn request() -> FootprintRequest {
        FootprintRequest {
            symbol: "A".into(),
            timeframe: Timeframe::Min1,
            clock: BarClock::new(Timeframe::Min1),
            tick_size: 1.0,
            ticks_per_row: 1,
            before: None,
            since: None,
            count: 10,
            extended_hours: false,
            adjusted: false,
            conversion: None,
        }
    }
    fn bars(minutes: std::ops::Range<i64>) -> Vec<FootprintBar> {
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for minute in minutes {
            for (price, size, side) in [
                (100.0, 2.0, Some(OrderSide::Sell)),
                (101.0, 5.0, Some(OrderSide::Buy)),
                (103.0, 3.0, None),
            ] {
                flow.push(&Tick::new(at(minute), price, size, side))
                    .unwrap();
            }
        }
        flow.bars.into_values().collect()
    }

    #[test]
    fn read_snapshot_bounds_shared_pages_without_copying_their_rows() {
        let owner = Arc::new(Native::new(bars(0..5)));
        let flow =
            FootprintState::from_page(&request(), FootprintBars::Shared(owner.clone()), false)
                .unwrap();
        for (from, before, expected) in [
            (at(0), None, vec![0, 1, 2, 3, 4]),
            (at(1), Some(at(4)), vec![1, 2, 3]),
            (at(1) + TimeDelta::seconds(1), Some(at(4)), vec![2, 3]),
            (at(5), None, vec![]),
            (at(0), Some(at(0)), vec![]),
        ] {
            let read = flow.snapshot(from, before);
            assert_eq!(
                read.view().bars(..).map(|bar| bar.time).collect::<Vec<_>>(),
                expected.iter().copied().map(at).collect::<Vec<_>>()
            );
            for minute in expected {
                let FootprintRows::Slice(rows) = read.view().bar_at(at(minute)).unwrap().levels
                else {
                    panic!("snapshot copied prepared rows");
                };
                assert_eq!(rows.as_ptr(), owner.0[minute as usize].1.as_ptr());
            }
        }
    }

    #[test]
    fn shared_pages_retain_rows_and_copy_only_the_partial_boundary() {
        let owner = Arc::new(Native::new(bars(0..3)));
        let weak = Arc::downgrade(&owner);
        let first_row = owner.0[0].1.as_ptr();
        let mut first =
            FootprintState::from_page(&request(), FootprintBars::Shared(owner.clone()), false)
                .unwrap();
        let mut second =
            FootprintState::from_page(&request(), FootprintBars::Shared(owner.clone()), false)
                .unwrap();
        let tape = crate::data::trades::Trades::new(10);
        first.prepare_history(at(2) + TimeDelta::seconds(30), &tape, true);
        second.prepare_history(at(3), &tape, true);
        assert_eq!(first.live.bars.len(), 1);
        assert_eq!(first.live.rows, 3);
        assert!(second.live.bars.is_empty());
        assert_eq!(second.live.rows, 0);
        for flow in [&first, &second] {
            let FootprintRows::Slice(rows) = flow.view().bar_at(at(0)).unwrap().levels else {
                panic!("history was copied")
            };
            assert_eq!(rows.as_ptr(), first_row);
        }
        first
            .live
            .push(&Tick::new(at(2) + TimeDelta::seconds(40), 101.0, 7.0, None))
            .unwrap();
        assert_eq!(first.view().bar_at(at(2)).unwrap().unknown, 10.0);
        assert_eq!(second.view().bar_at(at(2)).unwrap().unknown, 3.0);
        let frozen = first.clone();
        drop(owner);
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_some());
        assert_eq!(frozen.view().bar_at(at(0)).unwrap().volume(), 10.0);
        drop(frozen);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn mixed_pages_use_one_complete_bar_view_across_missing_joins() {
        let mut flow = FootprintState::from_page(
            &request(),
            FootprintBars::Shared(Arc::new(Native::new(bars(0..2)))),
            false,
        )
        .unwrap();
        let tape = crate::data::trades::Trades::new(10);
        flow.prepare_history(at(2), &tape, true);
        flow.live.push(&Tick::new(at(2), 100.0, 7.0, None)).unwrap();
        flow.live
            .push(&Tick::new(at(4), 100.0, 11.0, None))
            .unwrap();
        flow.live.add_missing(at(2), at(4));
        let older =
            FootprintState::from_page(&request(), FootprintBars::Owned(bars(-1..0)), true).unwrap();
        flow.prepend_prepared(older);
        let view = flow.view();
        assert_eq!(
            view.bars(..).map(|b| b.time).collect::<Vec<_>>(),
            [-1, 0, 1, 2, 4].map(at)
        );
        assert_eq!(
            view.bars(at(0)..at(2)).map(|b| b.time).collect::<Vec<_>>(),
            [at(0), at(1)]
        );
        assert_eq!(
            view.complete_bars(..).map(|b| b.time).collect::<Vec<_>>(),
            [-1, 0, 1, 4].map(at)
        );
        assert_eq!(view.complete_from(), Some(at(4)));
        assert!(view.complete_bar_at(at(2)).is_none());
        assert!(flow.owned().is_none());
        let selection: Vec<_> = [0, 1, 1, 4, 2, -1, 10]
            .map(|minute| {
                crate::model::Bar::new(
                    at(minute) + TimeDelta::seconds(30),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    0.0,
                )
            })
            .into();
        assert_eq!(
            view.matching(&selection)
                .map(|bar| bar.map(|b| b.time))
                .collect::<Vec<_>>(),
            [
                Some(at(0)),
                Some(at(1)),
                Some(at(1)),
                Some(at(4)),
                Some(at(2)),
                Some(at(-1)),
                None
            ]
        );
        let owned = Footprint::from_bars(request().clock, 1.0, 1, bars(-1..5)).unwrap();
        let times: Vec<_> = (-2..7)
            .flat_map(|minute| [at(minute), at(minute) + TimeDelta::seconds(30)])
            .collect();
        // Repeated, reordered and non-opening timestamps must agree with independent lookups.
        let selection: Vec<_> = times
            .iter()
            .flat_map(|&first| {
                times
                    .iter()
                    .flat_map(move |&second| [first, first, second, first])
            })
            .map(|time| crate::model::Bar::new(time, 100.0, 100.0, 100.0, 100.0, 0.0))
            .collect();
        for view in [view, (&owned).into()] {
            assert_eq!(
                view.matching(&selection).collect::<Vec<_>>(),
                selection
                    .iter()
                    .map(|bar| view.bar_at(bar.time))
                    .collect::<Vec<_>>()
            );
        }

        #[cfg(feature = "studies")]
        {
            let ranges = crate::studies::TradeProfileRanges::new(vec![at(-1)..at(5)]).unwrap();
            let profiles = crate::studies::trade_profiles_with_rows_read(
                view,
                &ranges,
                crate::model::ProfileRows::Feed,
            );
            assert_eq!(profiles[0].volume.volume(), 41.0);
            assert!(!profiles[0].complete);
            assert_eq!(profiles[0].developing_poc.len(), 4);
            assert_eq!(profiles[0].developing_value_area_read(view, 0.7).len(), 4);
        }
    }

    #[test]
    fn shared_pages_reject_unordered_or_duplicate_bars_and_rows() {
        let invalid = |change: fn(&mut Native)| {
            let mut source = Native::new(bars(0..2));
            change(&mut source);
            FootprintState::from_page(&request(), FootprintBars::Shared(Arc::new(source)), false)
        };
        let bar_error = "invalid data: footprint bar at 2026-01-05 00:00:00 UTC is unordered or supplied more than once";
        let row_error = "invalid data: footprint bar at 2026-01-05 00:00:00 UTC has unordered or duplicate price rows";
        for (result, expected) in [
            (invalid(|source| source.0.swap(0, 1)), bar_error),
            (
                invalid(|source| source.0[1].0.time = source.0[0].0.time),
                bar_error,
            ),
            (invalid(|source| source.0[0].1.swap(0, 1)), row_error),
            (
                invalid(|source| source.0[0].1[1].level = source.0[0].1[0].level),
                row_error,
            ),
        ] {
            assert_eq!(result.unwrap_err().to_string(), expected);
        }
    }
}
