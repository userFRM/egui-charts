//! The sink's footprint producer is independent of the chart's displayed copy. Keep its
//! forming allocation and raw rows, including rows no longer recoverable from the tape.

use super::*;
use crate::data::{footprint::PackedFootprintState, inbox::TradeBars};

pub(crate) struct PackedFootprintInput {
    footprint: Option<PackedFootprintState>,
    trade_bars: TradeBars,
    last: Option<DateTime<Utc>>,
    basis: SizeBasis,
    tick_size: Option<f64>,
    revision: u64,
}

pub(crate) struct FootprintGuard<'a>(MutexGuard<'a, State>);

impl OrderFlowInput {
    pub(crate) fn try_checkpoint_footprint(&self) -> Option<FootprintGuard<'_>> {
        self.0.try_lock().ok().map(FootprintGuard)
    }
}

impl FootprintGuard<'_> {
    pub(crate) fn revision(&self) -> u64 {
        self.0.revision
    }

    pub(crate) fn eligible(&self, clock: &BarClock) -> bool {
        let s = &self.0;
        let unused = |w: &Window| {
            w.events.is_empty() && w.sum == [0.0; 2] && w.now.is_none() && w.lost_until.is_none()
        };
        matches!(s.source, Source::Footprint)
            && s.plain_checkpoint_state()
            && !s.paged_rate
            && s.cut.is_none()
            && s.bars.is_empty()
            && unused(&s.window)
            && unused(&s.instant)
            && s.extremes.lost_until.is_none()
            && s.replay.is_none()
            && s.footprint.as_ref().is_none_or(|flow| {
                flow.live.clock() == clock
                    && Some(flow.live.tick_size()) == s.tick_size
                    && flow.live.price_step() == flow.live.tick_size()
                    && flow.checkpoint_eligible()
            })
    }

    /// All nested capture allocations. Plain TradeBars has no heap of its own.
    pub(crate) fn heap_bytes(&self) -> Option<usize> {
        self.0
            .footprint
            .as_ref()
            .map_or(Some(0), FootprintState::checkpoint_bytes)
    }

    pub(crate) fn snapshot(&self) -> PackedFootprintInput {
        let s = &self.0;
        PackedFootprintInput {
            footprint: s.footprint.as_ref().map(FootprintState::pack_checkpoint),
            trade_bars: s.trade_bars.clone(),
            last: s.last,
            basis: s.basis,
            tick_size: s.tick_size,
            revision: s.revision,
        }
    }

    pub(crate) fn compatible(&self, saved: &PackedFootprintInput) -> bool {
        self.0.tick_size == saved.tick_size
            && self.0.basis == saved.basis
            && saved
                .footprint
                .as_ref()
                .zip(self.0.footprint.as_ref())
                .is_none_or(|(source, dest)| source.compatible(dest))
    }

    /// Restore inside the existing handle after all guards pass. Reset readers at a fresh
    /// revision: edits and drops preceding it need no entries in the new change map.
    pub(crate) fn restore(&mut self, saved: &PackedFootprintInput, clock: &BarClock) {
        let revision = self.0.revision.max(saved.revision).wrapping_add(1);
        *self.0 = State {
            footprint: saved.footprint.as_ref().map(|flow| flow.restore(clock)),
            trade_bars: saved.trade_bars.clone(),
            last: saved.last,
            basis: saved.basis,
            tick_size: saved.tick_size,
            revision,
            reset_revision: revision,
            ..State::new(Source::Footprint)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;
    use std::num::NonZeroU64;

    fn clock() -> BarClock {
        BarClock::new(Timeframe::Volume(NonZeroU64::new(8).unwrap()))
    }

    #[test]
    fn producer_without_counted_rows_keeps_its_grouping() {
        let clock = clock();
        let source = OrderFlowInput::footprint();
        let target = OrderFlowInput::footprint();
        source.set_tick_size(Some(0.25));
        let open = DateTime::from_timestamp(100, 0).unwrap();
        let later = DateTime::from_timestamp(101, 0).unwrap();
        let planned = source.revision();
        source.push_trade(&clock, &Tick::new(open, 2.0, 0.0, None));
        let saved = {
            let guard = source.try_checkpoint_footprint().unwrap();
            assert!(source.try_checkpoint_footprint().is_none());
            assert_ne!(planned, guard.revision());
            assert!(guard.eligible(&clock));
            guard.snapshot()
        };
        assert!(
            !target
                .try_checkpoint_footprint()
                .unwrap()
                .compatible(&saved)
        );
        target.set_tick_size(Some(0.25));
        {
            let mut guard = target.try_checkpoint_footprint().unwrap();
            assert!(guard.eligible(&clock) && guard.compatible(&saved));
            guard.restore(&saved, &clock);
        }
        for input in [&source, &target] {
            input.push_trade(&clock, &Tick::new(later, 3.0, 10.0, Some(OrderSide::Buy)));
            let rows = input.footprints_from(open).bars;
            assert_eq!(rows[&open].ask, 8.0);
            assert_eq!(rows[&later].ask, 2.0);
        }
    }

    #[test]
    fn producer_rejects_an_unaccounted_pending_order() {
        let input = OrderFlowInput::footprint();
        let planned = input.revision();
        input.set_tick_size(Some(0.25));
        input.set_tick_size(None);
        assert_ne!(planned, input.revision());
        let guard = input.try_checkpoint_footprint().unwrap();
        assert!(guard.eligible(&clock()));
        assert_eq!(guard.heap_bytes(), Some(0));
        drop(guard);

        let at = DateTime::from_timestamp(100, 0).unwrap();
        input
            .state()
            .pending
            .sizes
            .add(&Tick::new(at, 2.0, 1.0, None));
        assert!(!input.try_checkpoint_footprint().unwrap().eligible(&clock()));
    }
}
