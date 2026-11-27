//! An input's data at a replay rebuild point, which a later seek of the same subscription
//! restores before it pushes the events after that point.

use super::*;

/// The data of one source at a rebuild point; its configuration stays with the source.
pub(crate) struct InputPoint(State);

/// Holds a source stable while the sink sizes and copies it, or checks and restores it.
pub(crate) struct InputLock<'a>(MutexGuard<'a, State>);

impl OrderFlowInput {
    /// `None` while another thread holds the source: the sink must not wait for it under the
    /// inbox lock.
    pub(crate) fn try_lock_replay_point(&self) -> Option<InputLock<'_>> {
        match self.0.try_lock() {
            Ok(state) => Some(InputLock(state)),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                Some(InputLock(poisoned.into_inner()))
            }
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
}

impl InputLock<'_> {
    /// Estimated bytes of a capture, as `Live::rebuild_bytes` counts: the inline state and the
    /// rows, events and levels it copies.
    pub(crate) fn bytes(&self) -> usize {
        let s = &*self.0;
        let distinct = s.distinct.iter().map(|d| {
            d.events.len() * size_of::<(DateTime<Utc>, u64)>()
                + d.counts.len() * size_of::<(u64, usize)>()
        });
        #[cfg(feature = "widget")]
        let footprint = (s.footprint.as_ref()).map_or(Some(0), FootprintState::checkpoint_bytes);
        // Only the chart connects footprint sources.
        #[cfg(not(feature = "widget"))]
        let footprint = s.footprint.is_none().then_some(0);
        [
            size_of::<State>(),
            s.bars.len() * size_of::<(DateTime<Utc>, ([f64; 3], u64))>(),
            s.changes.len() * size_of::<(DateTime<Utc>, u64)>(),
            (s.window.events.len() + s.instant.events.len())
                * size_of::<(DateTime<Utc>, [f64; 2])>(),
            (s.extremes.high.len() + s.extremes.low.len()) * size_of::<(DateTime<Utc>, f64)>(),
            s.pending.parts.len() * size_of::<(Tick, DateTime<Utc>, f64, bool)>(),
            s.pending.rows.len() * size_of::<Option<([f64; 3], u64)>>(),
            s.books.len() * size_of::<(DateTime<Utc>, BarBook)>()
                + s.book_rows * size_of::<OrderBookLevel>(),
            footprint.unwrap_or(usize::MAX),
        ]
        .into_iter()
        .chain(distinct)
        .fold(0, usize::saturating_add)
    }

    pub(crate) fn capture(&self) -> InputPoint {
        InputPoint((*self.0).clone())
    }

    /// Same aggregation, order filter, size basis and tick size.
    pub(crate) fn compatible(&self, point: &InputPoint) -> bool {
        let (s, saved) = (&*self.0, &point.0);
        s.source == saved.source
            && s.filter == saved.filter
            && s.basis == saved.basis
            && s.tick_size == saved.tick_size
    }

    /// As [`OrderFlowInput::clear`], then the data of `point`: readers start again from the new
    /// revision, as after a reset.
    pub(crate) fn restore(&mut self, point: &InputPoint) {
        let state = &mut *self.0;
        state.clear();
        let kept = std::mem::replace(state, point.0.clone());
        state.source = kept.source;
        state.filter = kept.filter;
        state.basis = kept.basis;
        state.tick_size = kept.tick_size;
        state.replay = kept.replay;
        state.revision = kept.revision;
        state.reset_revision = kept.reset_revision;
        state.dropped_revision = kept.dropped_revision;
        state.changes = kept.changes;
    }
}
