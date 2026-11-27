//! In-memory continuation of a Trade Rate input on volume bars. The live maps and deque
//! capacities are not part of the checkpoint; each saved buffer requests exactly its length.

use super::*;
use crate::data::inbox::TradeBars;

type BarEntry = (DateTime<Utc>, ([f64; 3], u64));
type Change = (DateTime<Utc>, u64);
type Event = (DateTime<Utc>, [f64; 2]);

#[derive(Debug)]
struct PackedWindow {
    events: Vec<Event>,
    sum: [f64; 2],
    lost_until: Option<DateTime<Utc>>,
    now: Option<DateTime<Utc>>,
}

impl PackedWindow {
    fn capture(window: &Window) -> Self {
        let (front, back) = window.events.as_slices();
        let mut events = Vec::with_capacity(window.events.len());
        events.extend_from_slice(front);
        events.extend_from_slice(back);
        Self {
            events,
            sum: window.sum,
            lost_until: window.lost_until,
            now: window.now,
        }
    }

    fn restore(&self) -> Window {
        Window {
            events: self.events.clone().into(),
            sum: self.sum,
            lost_until: self.lost_until,
            now: self.now,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ReplayRow {
    cutoff: DateTime<Utc>,
    open: DateTime<Utc>,
    row: Option<[f64; 3]>,
}

#[derive(Debug)]
pub(crate) struct PackedTradeRate {
    kind: OrderFlowKind,
    basis: SizeBasis,
    tick_size: Option<f64>,
    bars: Vec<BarEntry>,
    changes: Vec<Change>,
    window: PackedWindow,
    instant: PackedWindow,
    trade_bars: TradeBars,
    paged_rate: bool,
    cut: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
    revision: u64,
    // Rebuilding after missing executions marks all auxiliary windows with the same loss.
    auxiliary_loss: Option<DateTime<Utc>>,
    replay: Option<ReplayRow>,
}

/// Holds the source stable across admission and copying (or compatibility and restore).
pub(crate) struct RateGuard<'a>(MutexGuard<'a, State>);

impl OrderFlowInput {
    /// Never block on an input already held by this chart or another checkpoint operation.
    pub(crate) fn try_checkpoint_rate(&self) -> Option<RateGuard<'_>> {
        self.0.try_lock().ok().map(RateGuard)
    }
}

impl RateGuard<'_> {
    fn kind(&self) -> Option<OrderFlowKind> {
        match self.0.source {
            Source::Fixed(kind @ OrderFlowKind::TradeRate { .. }) => Some(kind),
            _ => None,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.0.revision
    }

    /// No nested owner may be cloned by `TradeBars::clone`, and no state of a different
    /// flow aggregation or a time-based placement may be lost on restore.
    pub(crate) fn eligible(&self) -> bool {
        let s = &self.0;
        self.kind().is_some()
            && s.plain_checkpoint_state()
            && s.footprint.is_none()
            && s.replay
                .as_ref()
                .is_none_or(|r| r.footprint.is_none() && r.book.is_none())
    }

    /// All nested heap requests; the caller sizes its own storage of `PackedTradeRate`.
    pub(crate) fn heap_bytes(&self) -> Option<usize> {
        let s = &self.0;
        let bars = s.bars.len().checked_mul(size_of::<BarEntry>())?;
        let changes = s.changes.len().checked_mul(size_of::<Change>())?;
        let events = s.window.events.len().checked_add(s.instant.events.len())?;
        let total = bars
            .checked_add(changes)?
            .checked_add(events.checked_mul(size_of::<Event>())?)?;
        (total <= isize::MAX as usize).then_some(total)
    }

    /// Called only after *all* guards have passed admission. No live map, pending
    /// allocation or temporary buffer is cloned to get at its entries.
    pub(crate) fn snapshot(&self) -> PackedTradeRate {
        let s = &self.0;
        let kind = self.kind().expect("admitted Trade Rate input");
        let mut bars = Vec::with_capacity(s.bars.len());
        bars.extend(s.bars.iter().map(|(&open, &row)| (open, row)));
        let mut changes = Vec::with_capacity(s.changes.len());
        changes.extend(s.changes.iter().map(|(&open, &rev)| (open, rev)));
        PackedTradeRate {
            kind,
            basis: s.basis,
            tick_size: s.tick_size,
            bars,
            changes,
            window: PackedWindow::capture(&s.window),
            instant: PackedWindow::capture(&s.instant),
            trade_bars: s.trade_bars.clone(),
            paged_rate: s.paged_rate,
            cut: s.cut,
            last: s.last,
            revision: s.revision,
            auxiliary_loss: s.extremes.lost_until,
            replay: s.replay.as_ref().map(|r| ReplayRow {
                cutoff: r.cutoff,
                open: r.open,
                row: r.row,
            }),
        }
    }

    pub(crate) fn compatible(&self, saved: &PackedTradeRate) -> bool {
        self.kind() == Some(saved.kind)
            && self.0.basis == saved.basis
            && self.0.tick_size == saved.tick_size
    }

    /// All guards have been validated. Mark every reader for a full refresh, even if this
    /// is a different input with an older revision, or a second restore of the same mark.
    pub(crate) fn restore(&mut self, saved: &PackedTradeRate) {
        let revision = self.0.revision.max(saved.revision).wrapping_add(1);
        *self.0 = State {
            source: Source::Fixed(saved.kind),
            tick_rule: None,
            trade_bars: saved.trade_bars.clone(),
            paged_rate: saved.paged_rate,
            cut: saved.cut,
            bars: saved.bars.iter().copied().collect(),
            last: saved.last,
            inside: None,
            window: saved.window.restore(),
            instant: saved.instant.restore(),
            extremes: Extremes {
                lost_until: saved.auxiliary_loss,
                ..Extremes::default()
            },
            distinct: std::array::from_fn(|_| Distinct {
                lost_until: saved.auxiliary_loss,
                ..Distinct::default()
            }),
            last_price: [None; 2],
            revision,
            reset_revision: revision,
            dropped_revision: 0,
            changes: saved.changes.iter().copied().collect(),
            filter: None,
            basis: saved.basis,
            pending: PendingParts::default(),
            footprint: None,
            books: BTreeMap::new(),
            book_rows: 0,
            tick_size: saved.tick_size,
            replay: saved.replay.map(|r| Replay {
                cutoff: r.cutoff,
                open: r.open,
                row: r.row,
                footprint: None,
                book: None,
            }),
            placement: Placement::default(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;
    use std::num::NonZeroU64;

    #[test]
    fn reverting_rate_configuration_does_not_revive_an_old_plan() {
        let input = OrderFlowInput::new(OrderFlowKind::TradeRate {
            volume: false,
            seconds: 5,
            instant: 1,
        });
        let planned = input.try_checkpoint_rate().unwrap().revision();
        input.set_tick_size(Some(0.25));
        input.set_tick_size(None);
        assert_ne!(input.try_checkpoint_rate().unwrap().revision(), planned);
    }

    #[test]
    fn locked_snapshot_preserves_grouping_and_different_window_cutoffs() {
        let kind = OrderFlowKind::TradeRate {
            volume: true,
            seconds: 4,
            instant: 1,
        };
        let source = OrderFlowInput::new(kind);
        let target = OrderFlowInput::new(kind);
        let clock = BarClock::new(Timeframe::Volume(NonZeroU64::new(8).unwrap()));
        let at = |ms| DateTime::from_timestamp_millis(ms).unwrap();
        let push = |input: &OrderFlowInput, ms, size| {
            input.push_trade(&clock, &Tick::new(at(ms), 100.0, size, None));
        };
        push(&source, 0, 2.0);
        push(&source, 1000, 3.0);
        let saved = {
            let guard = source.try_checkpoint_rate().unwrap();
            assert!(source.try_checkpoint_rate().is_none());
            assert!(guard.eligible());
            // Two events in the rolling window, just the newest in the instant window.
            assert_eq!(
                guard.heap_bytes(),
                Some(size_of::<BarEntry>() + size_of::<Change>() + 3 * size_of::<Event>())
            );
            guard.snapshot()
        };
        {
            let mut guard = target.try_checkpoint_rate().unwrap();
            assert!(guard.eligible() && guard.compatible(&saved));
            guard.restore(&saved);
        }
        for input in [&source, &target] {
            push(input, 1500, 5.0);
            assert_eq!(input.values()[&at(0)], [8.0, 2.5, 8.0]);
            assert_eq!(input.values()[&at(1500)], [2.0, 2.5, 8.0]);
            input.advance_time(&clock, at(2250));
            assert_eq!(input.values()[&at(1500)], [2.0, 2.5, 5.0]);
        }
    }
}
