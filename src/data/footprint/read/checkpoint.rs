//! A drained volume footprint. Copy sealed source bars as well as the mutable suffix so
//! the checkpoint never pins a live tree, an `Arc` allocation or a prepared page owner.

use super::super::{Footprint, FootprintBar, FootprintLevel, SizeBasis};
use super::FootprintState;
use crate::data::inbox::TradeBars;
use crate::model::{BarClock, Timeframe, next_revision};
use chrono::{DateTime, Utc};
use std::{num::NonZeroU64, sync::Arc};

type Times = (DateTime<Utc>, DateTime<Utc>);

#[derive(Debug)]
struct PackedBar {
    time: DateTime<Utc>,
    end: usize, // exclusive end in the shared row buffer
    bid: f64,
    ask: f64,
    unknown: f64,
    min_delta: f64,
    max_delta: f64,
}

#[derive(Debug)]
pub(crate) struct PackedFootprintState {
    lots: NonZeroU64,
    tick_size: f64,
    ticks_per_row: u32,
    max_bars: usize,
    max_rows: usize,
    bars: Vec<PackedBar>,
    levels: Vec<FootprintLevel>,
    closed: usize,
    trade_bars: TradeBars,
    trade_starts: Vec<Times>,
    last: Option<DateTime<Utc>>,
    incomplete_through: Option<DateTime<Utc>>,
    missing: Vec<Times>,
    missing_bars: Vec<Times>,
    covered_from: Option<DateTime<Utc>>,
}

impl FootprintState {
    pub(crate) fn checkpoint_eligible(&self) -> bool {
        let flow = &self.live;
        // This chart uses sessionless volume bars and unfiltered fills: no exchange-session
        // placement cache and no per-order pending fills/undo records can contribute to it.
        // A prepared owner can retain arbitrarily more than its visible rows.
        matches!(flow.clock.timeframe(), Some(Timeframe::Volume(_)))
            && !flow.clock.has_session()
            && flow.placement.is_empty()
            && flow.basis == SizeBasis::Fill
            && flow.min_size == 0.0
            && flow.max_size == f64::MAX
            && flow.pending.sizes.checkpoint_empty()
            && flow.pending.fills.is_empty()
            && flow.pending.undo.is_empty()
            && flow.trade_bars.is_plain()
            && self.pages.is_empty()
            && flow.partial_slots.is_none()
            && flow.prepared.is_none()
    }

    /// All capture allocations beyond the inline `PackedFootprintState`. The accumulator
    /// maintains both row counts, including the newest bar even when it exceeds its budget.
    pub(crate) fn checkpoint_bytes(&self) -> Option<usize> {
        let flow = &self.live;
        let bars = self.closed.len().checked_add(flow.bars.len())?;
        let levels = self.closed_rows.checked_add(flow.rows)?;
        let times = flow
            .trade_starts
            .len()
            .checked_add(flow.missing.len())?
            .checked_add(flow.missing_bars.len())?;
        let bytes = bars
            .checked_mul(size_of::<PackedBar>())?
            .checked_add(levels.checked_mul(size_of::<FootprintLevel>())?)?
            .checked_add(times.checked_mul(size_of::<Times>())?)?;
        (bytes <= isize::MAX as usize).then_some(bytes)
    }

    /// After admission, copy each row directly into its final allocation. Keep the closed
    /// prefix distinct: its bars must still be sealed and subject to source retention.
    pub(crate) fn pack_checkpoint(&self) -> PackedFootprintState {
        let flow = &self.live;
        let Some(Timeframe::Volume(lots)) = flow.clock.timeframe() else {
            unreachable!("admitted volume footprint")
        };
        let mut bars = Vec::with_capacity(self.closed.len() + flow.bars.len());
        let mut levels = Vec::with_capacity(self.closed_rows + flow.rows);
        for bar in self
            .closed
            .iter()
            .map(Arc::as_ref)
            .chain(flow.bars.values())
        {
            levels.extend(bar.levels.values().cloned());
            bars.push(PackedBar {
                time: bar.time,
                end: levels.len(),
                bid: bar.bid,
                ask: bar.ask,
                unknown: bar.unknown,
                min_delta: bar.min_delta,
                max_delta: bar.max_delta,
            });
        }
        PackedFootprintState {
            lots,
            tick_size: flow.tick_size,
            ticks_per_row: flow.ticks_per_row,
            max_bars: flow.max_bars,
            max_rows: flow.max_rows,
            bars,
            levels,
            closed: self.closed.len(),
            trade_bars: flow.trade_bars.clone(),
            trade_starts: flow
                .trade_starts
                .iter()
                .map(|(&at, &ts)| (at, ts))
                .collect(),
            last: flow.last,
            incomplete_through: flow.incomplete_through,
            missing: flow.missing.clone(),
            missing_bars: flow.missing_bars.clone(),
            covered_from: flow.covered_from,
        }
    }
}

impl PackedFootprintState {
    /// Both sides are eligible, and the outer chart key validates the instrument and clock.
    pub(crate) fn compatible(&self, dest: &FootprintState) -> bool {
        let flow = &dest.live;
        flow.clock.timeframe() == Some(Timeframe::Volume(self.lots))
            && flow.tick_size == self.tick_size
            && flow.ticks_per_row == self.ticks_per_row
            && flow.max_bars == self.max_bars
            && flow.max_rows == self.max_rows
    }

    pub(crate) fn restore(&self, clock: &BarClock) -> FootprintState {
        let mut closed = Vec::with_capacity(self.closed);
        let mut bars = std::collections::BTreeMap::new();
        let mut from = 0;
        for (index, saved) in self.bars.iter().enumerate() {
            let bar = FootprintBar {
                time: saved.time,
                levels: self.levels[from..saved.end]
                    .iter()
                    .map(|row| (row.level, row.clone()))
                    .collect(),
                bid: saved.bid,
                ask: saved.ask,
                unknown: saved.unknown,
                min_delta: saved.min_delta,
                max_delta: saved.max_delta,
            };
            if index < self.closed {
                closed.push(Arc::new(bar));
            } else {
                bars.insert(bar.time, bar);
            }
            from = saved.end;
        }
        let closed_rows = self.bars[..self.closed].last().map_or(0, |bar| bar.end);
        FootprintState {
            live: Footprint {
                clock: clock.clone(),
                step: self.tick_size * f64::from(self.ticks_per_row),
                tick_size: self.tick_size,
                ticks_per_row: self.ticks_per_row,
                bars,
                trade_bars: self.trade_bars.clone(),
                trade_starts: self.trade_starts.iter().copied().collect(),
                rows: self.levels.len() - closed_rows,
                last: self.last,
                min_size: 0.0,
                max_size: f64::MAX,
                basis: SizeBasis::Fill,
                pending: Default::default(),
                max_bars: self.max_bars,
                max_rows: self.max_rows,
                incomplete_through: self.incomplete_through,
                missing: self.missing.clone(),
                partial_slots: None,
                missing_bars: self.missing_bars.clone(),
                placement: Default::default(),
                covered_from: self.covered_from,
                prepared: None,
                revision: next_revision(),
            },
            pages: Vec::new(),
            closed,
            closed_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrderSide, Tick};

    #[test]
    fn packed_rows_release_sealed_owners_and_restore_the_mutable_suffix() {
        let clock = BarClock::new(Timeframe::Volume(NonZeroU64::new(10).unwrap()));
        let mut state = FootprintState::new(Footprint::new(clock.clone(), 0.5, 2).unwrap());
        let time = DateTime::from_timestamp(5, 0).unwrap();
        let fill = |price, size, side| Tick::new(time, price, size, Some(side));
        for tick in [
            fill(10.0, 6.0, OrderSide::Buy),
            fill(11.0, 7.0, OrderSide::Sell),
        ] {
            state.live.push(&tick).unwrap();
        }
        state.seal();
        let owner = Arc::downgrade(&state.closed[0]);
        let first = (*state.closed[0]).clone();
        let tail = state.live.bars.clone();
        let starts = state.live.trade_starts.clone();
        let gap = (DateTime::from_timestamp(1, 0).unwrap(), time);
        state.live.add_missing(gap.0, gap.1);
        state.live.covered_from = Some(gap.0);
        state.live.incomplete_through = Some(gap.0);
        assert!(state.checkpoint_eligible());
        // Two bar headers, three distinct (bar, price) rows, two exchange-time starts,
        // and the missing interval in both exchange times and bar opens.
        assert_eq!(
            state.checkpoint_bytes(),
            Some(
                2 * size_of::<PackedBar>()
                    + 3 * size_of::<FootprintLevel>()
                    + 4 * size_of::<Times>()
            )
        );
        let mark = state.pack_checkpoint();
        assert!(state.retain(1, 1));
        assert!(owner.upgrade().is_none());
        let mut restored = mark.restore(&clock);
        assert_eq!(restored.closed.len(), 1);
        assert_eq!(*restored.closed[0], first);
        assert_eq!(restored.live.bars, tail);
        assert_eq!(restored.live.trade_starts, starts);
        assert_eq!(restored.closed_rows, 2);
        assert_eq!(restored.live.rows, 1);
        assert_eq!(restored.live.covered_from, Some(gap.0));
        assert_eq!(restored.live.incomplete_through, Some(gap.0));
        assert_eq!(restored.live.missing, [gap]);
        assert_eq!(restored.live.missing_bars, state.live.missing_bars);

        // The mutable suffix has 3 lots and continues the saved allocation, not a new bar.
        let next = fill(12.0, 8.0, OrderSide::Buy);
        state.live.push(&next).unwrap();
        restored.live.push(&next).unwrap();
        assert_eq!(restored.live.bars, state.live.bars);
        assert_eq!(restored.live.bars.len(), 2);
    }
}
