//! The same complete-bar view over owned accumulation and immutable prepared pages.

use super::{Footprint, FootprintBarView};
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
        let (mut left, mut right) = (0, self.end);
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
            Bound::Unbounded => 0,
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

#[derive(Clone, Copy)]
pub(crate) struct FootprintView<'a> {
    flow: &'a Footprint,
    pages: &'a [PreparedPage],
}
impl<'a> From<&'a Footprint> for FootprintView<'a> {
    fn from(flow: &'a Footprint) -> Self {
        Self { flow, pages: &[] }
    }
}
impl<'a> FootprintView<'a> {
    #[cfg(feature = "widget")]
    pub(crate) fn clock(self) -> &'a crate::model::BarClock {
        self.flow.clock()
    }
    pub(crate) fn tick_size(self) -> f64 {
        self.flow.tick_size()
    }
    pub(crate) fn price_step(self) -> f64 {
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
            .or_else(|| self.flow.bars.first_key_value().map(|(time, _)| *time))
    }
    #[cfg(feature = "widget")]
    pub(crate) fn bar_at(self, time: DateTime<Utc>) -> Option<FootprintBarView<'a>> {
        let time = self.flow.clock.bar_open(time);
        if let Some(bar) = self.flow.bars.get(&time) {
            return Some(bar.into());
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
        shared.chain(
            self.flow
                .bars
                .range((start, end))
                .map(|(_, bar)| bar.into()),
        )
    }
    /// Walk ordered chart bars once; restart the range for a gap or a reordered selection.
    #[cfg(feature = "widget")]
    pub(crate) fn matching(
        self,
        bars: &[crate::model::Bar],
    ) -> impl Iterator<Item = Option<FootprintBarView<'a>>> {
        let mut rows = self.bars((Bound::Unbounded, Bound::Unbounded)).peekable();
        let mut previous = None;
        bars.iter().map(move |bar| {
            let open = self.flow.clock.bar_open(bar.time);
            let restart = previous.is_none_or(|last| open < last);
            if !restart && rows.peek().is_some_and(|row| row.time < open) {
                rows.next();
            }
            if restart || rows.peek().is_some_and(|row| row.time < open) {
                rows = self
                    .bars((Bound::Included(open), Bound::Unbounded))
                    .peekable();
            }
            previous = Some(open);
            rows.peek().filter(|row| row.time == open).copied()
        })
    }
    #[cfg(feature = "widget")]
    pub(crate) fn complete_at(self, open: DateTime<Utc>) -> bool {
        self.flow.complete_at(open)
    }
    pub(crate) fn complete_bars(
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
    #[cfg(feature = "widget")]
    pub(crate) fn complete_from(self) -> Option<DateTime<Utc>> {
        self.bars((self.flow.complete_start(), Bound::Unbounded))
            .next()
            .map(|bar| bar.time)
    }
    pub(crate) fn span_has_gap(self, from: DateTime<Utc>, through: Option<DateTime<Utc>>) -> bool {
        self.flow.span_has_gap(from, through)
    }
    #[cfg(feature = "studies")]
    pub(crate) fn span_is_complete(self, first: DateTime<Utc>, last: DateTime<Utc>) -> bool {
        let (first, last) = (
            self.flow.clock.bar_open(first),
            self.flow.clock.bar_open(last),
        );
        first <= last
            && self.first_time().is_some_and(|held| held <= first)
            && self.flow.complete_at(first)
            && self.flow.complete_at(last)
            && !self.flow.span_has_gap(first, Some(last))
    }
}

/// Prepared owners stay outside the mutable footprint and its public tree accessor.
#[cfg(feature = "widget")]
#[derive(Clone, Debug)]
pub(crate) struct FootprintState {
    pub(crate) live: Footprint,
    pages: Vec<PreparedPage>,
}
#[cfg(feature = "widget")]
impl FootprintState {
    pub(crate) fn new(live: Footprint) -> Self {
        Self {
            live,
            pages: Vec::new(),
        }
    }
    pub(crate) fn view(&self) -> FootprintView<'_> {
        FootprintView {
            flow: &self.live,
            pages: &self.pages,
        }
    }
    /// The original tree API is available only when it describes the entire input.
    pub(crate) fn owned(&self) -> Option<&Footprint> {
        self.pages.is_empty().then_some(&self.live)
    }

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
            source,
        };
        Ok(Self {
            live,
            pages: vec![page],
        })
    }

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
            if page.end == 0 {
                self.pages.pop();
            }
        }
        self.live.prepare_history(before, trades, from_start);
    }
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
    fn shared_pages_reject_invalid_bar_order_clock_and_numbers() {
        for invalid in 0..4 {
            let mut bars = bars(0..2);
            match invalid {
                0 => bars.swap(0, 1),
                1 => bars[1].time = bars[0].time,
                2 => bars[0].time += TimeDelta::seconds(1),
                _ => bars[0].unknown = f64::NAN,
            }
            assert!(
                FootprintState::from_page(
                    &request(),
                    FootprintBars::Shared(Arc::new(Native::new(bars))),
                    false
                )
                .is_err(),
                "case {invalid}"
            );
        }
    }
}
