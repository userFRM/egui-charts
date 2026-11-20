//! A drained live tape. Page boundaries and unheld executions are not represented.

use super::{DateTime, Tick, Trades, Utc, next_revision};

pub(crate) struct PackedTrades {
    ticks: Vec<Tick>,
    capacity: usize,
    id_count: usize,
    lost_until: Option<DateTime<Utc>>,
    front_reached: bool,
    live_lost: bool,
}

impl Trades {
    pub(crate) fn checkpoint_eligible(&self) -> bool {
        self.paged == 0
            && self.paged_until.is_none()
            && self.paged_without_id_until.is_none()
            && self.retained_until.is_none()
            && self.first_retained_until.is_none()
            && self.left_out.is_empty()
            && self.left_out_at.is_none()
            && self.unheld_live.is_none()
            && self.trades.iter().is_sorted_by_key(|trade| trade.ts)
    }

    /// Additional heap requested during capture, excluding the inline `PackedTrades` value.
    pub(crate) fn checkpoint_bytes(&self) -> Option<usize> {
        self.trades.len().checked_mul(size_of::<Tick>())
    }

    /// Call after checking eligibility. Copy directly from the ring into its final allocation;
    /// making the ring contiguous or cloning its id index would require additional storage.
    pub(crate) fn pack_checkpoint(&self) -> PackedTrades {
        let mut ticks = Vec::with_capacity(self.trades.len());
        let (head, tail) = self.trades.as_slices();
        ticks.extend_from_slice(head);
        ticks.extend_from_slice(tail);
        PackedTrades {
            ticks,
            capacity: self.capacity,
            id_count: self.ids.len(),
            lost_until: self.lost_until,
            front_reached: self.front_reached,
            live_lost: self.live_lost,
        }
    }
}

impl PackedTrades {
    pub(crate) fn compatible(&self, dest: &Trades) -> bool {
        self.capacity == dest.capacity
    }

    /// Restore into an eligible, compatible tape after all components have passed validation.
    pub(crate) fn restore(&self, dest: &mut Trades) {
        dest.trades.clear();
        dest.trades.extend(self.ticks.iter().cloned());
        dest.ids.clear();
        dest.ids.reserve(self.id_count);
        // Stamps are internal to the index. Rebase them at the restored ring's front.
        dest.front = 0;
        for (at, trade) in self.ticks.iter().enumerate() {
            if let Some(id) = trade.id {
                dest.ids.insert(id, at);
            }
        }
        dest.lost_until = self.lost_until;
        dest.front_reached = self.front_reached;
        dest.live_lost = self.live_lost;
        // A reusable checkpoint replaces the tape even when it restores the same executions.
        dest.revision = next_revision();
        dest.generation = dest.revision;
        // Restored rows are the baseline. Only later stream appends count as new executions.
        dest.page_revision = dest.generation;
        dest.live_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(id: u64, second: i64) -> Tick {
        Tick::new(DateTime::from_timestamp(second, 0).unwrap(), 1., 1., None).with_id(id)
    }

    #[test]
    fn restores_a_wrapped_tape_and_rebuilds_only_held_ids() {
        let mut source = Trades::new(3);
        source.prepend(Vec::new(), &mut |_, _| ());
        for id in 10..13 {
            source.push(trade(id, 0));
        }
        let mut queued = source.empty();
        for id in 0..9 {
            queued.push(trade(id, 0));
        }
        source.append(queued, &mut |_, _| ());
        assert!(source.checkpoint_eligible());
        assert_eq!(source.checkpoint_bytes(), Some(3 * size_of::<Tick>()));
        let saved = source.pack_checkpoint();
        let mut dest = source.empty();
        dest.push(trade(99, 0));
        assert!(saved.compatible(&dest));
        assert!(!saved.compatible(&Trades::new(2)));
        for _ in 0..2 {
            let old = dest.generation;
            saved.restore(&mut dest);
            assert_ne!(dest.generation, old);
            assert!(dest.front_reached && dest.live_lost);
            assert_eq!(dest.lost_until, source.lost_until);
            assert!(dest.iter().eq(source.iter()));
            assert!(!dest.holds(99));
            dest.push(trade(7, 1).with_resting_order_id(42));
            dest.push(trade(0, 1));
            assert_eq!(dest.trades[0].resting_order_id, Some(42));
            assert_eq!(
                dest.iter().filter_map(|t| t.id).collect::<Vec<_>>(),
                [7, 8, 0]
            );
            assert!(!dest.holds(6));
        }
    }

    #[test]
    fn restored_rows_are_not_new_stream_progress() {
        let mut tape = Trades::new(2);
        let mut queued = tape.empty();
        queued.push(trade(1, 0));
        tape.append(queued, &mut |_, _| ());
        let mut cursor = 0;
        let (old_page, first) = tape.progress(&mut cursor);
        assert_eq!(first, 0);

        let saved = tape.pack_checkpoint();
        saved.restore(&mut tape);
        cursor = 0; // Consumers reset their cursor on a new generation.
        let (page, first) = tape.progress(&mut cursor);
        assert_ne!(page, old_page);
        assert_eq!(first, 1);

        let mut queued = tape.empty();
        queued.push(trade(2, 1));
        tape.append(queued, &mut |_, _| ());
        assert_eq!(tape.progress(&mut cursor), (page, 1));
        assert_eq!(tape.progress(&mut cursor), (page, 2));
    }

    #[test]
    fn refuses_page_evidence_even_after_its_trades_are_evicted() {
        let mut tape = Trades::new(1);
        tape.prepend(vec![trade(1, 0)], &mut |_, _| ());
        tape.push(trade(2, 1));
        assert_eq!(tape.paged, 0);
        assert!(!tape.checkpoint_eligible());
        let mut unordered = Trades::new(2);
        unordered.push(trade(2, 1));
        unordered.push(trade(1, 0));
        assert!(!unordered.checkpoint_eligible());
    }
}
