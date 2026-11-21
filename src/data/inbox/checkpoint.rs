//! Checkpoint boundary between the chart and its feed producers. A successful check keeps the
//! same lock until the chart finishes sizing, copying, or restoring its portion of the state.

use super::{Inbox, Live, State, TradeBars};
use crate::data::{OrderFlowInput, SubscriptionId};
use crate::model::Timeframe;
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::MutexGuard;

#[derive(Debug)]
pub(crate) enum SinkCheckpointError {
    NotReady,
    Unsupported,
}

/// The queued grouping and live grouping coincide at a drain. One plain grouping and one packed
/// sequence of deduplication IDs suffice; the membership index need not be copied on capture.
#[derive(Debug)]
pub(crate) struct SinkCheckpoint {
    grouping: TradeBars,
    last_close: Option<f64>,
    empty: bool,
    server_time: Option<DateTime<Utc>>,
    ids: Vec<u64>,
    capacity: usize,
    timeframe: Timeframe,
    tick_size: Option<f64>,
}

pub(crate) struct SinkCheckpointGuard<'a> {
    state: MutexGuard<'a, State>,
    id: SubscriptionId,
    timeframe: Timeframe,
    server_time: Option<DateTime<Utc>>,
}

impl Inbox {
    pub(crate) fn checkpoint_guard(
        &self,
        id: SubscriptionId,
    ) -> Result<SinkCheckpointGuard<'_>, SinkCheckpointError> {
        let state = self.state();
        // A heartbeat can queue time without waking a chart that has no stream studies. A replay
        // rebuild can remain open across drains. Neither is a drain boundary.
        if state.woken
            || !state.answers.is_empty()
            || state
                .subscriptions
                .values()
                .any(|live| live.updates.time.is_some() || live.rebuilt_bars != 0)
        {
            return Err(SinkCheckpointError::NotReady);
        }
        let live = state
            .subscriptions
            .get(&id)
            .ok_or(SinkCheckpointError::NotReady)?;
        let Some(timeframe @ Timeframe::Volume(_)) = live.clock.timeframe() else {
            return Err(SinkCheckpointError::Unsupported);
        };
        if state.subscriptions.len() != 1
            || live.non_trade
            || live.clock.has_session()
            || live.sampler.is_some()
            || live.orders.is_some()
            || live
                .tick_size
                .is_some_and(|size| !size.is_finite() || size <= 0.0)
            || live.checkpoint_grouping().is_none()
        {
            return Err(SinkCheckpointError::Unsupported);
        }
        // Preserve a server clock at this execution, not its old offset from wall time: the
        // checkpoint might be restored much later. A feed using system time keeps that mode.
        let server_time = if live.offset.is_zero() {
            None
        } else {
            Some(
                Utc::now()
                    .checked_add_signed(live.offset)
                    .ok_or(SinkCheckpointError::Unsupported)?,
            )
        };
        Ok(SinkCheckpointGuard {
            state,
            id,
            timeframe,
            server_time,
        })
    }
}

impl SinkCheckpointGuard<'_> {
    fn live(&self) -> &Live {
        &self.state.subscriptions[&self.id]
    }

    pub(crate) fn revision(&self) -> u64 {
        self.state.revision
    }

    pub(crate) fn inputs(&self) -> &[OrderFlowInput] {
        &self.live().study_inputs
    }

    /// Capacity requested during capture, beyond the inline `SinkCheckpoint`.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.live().taken.order.len() * size_of::<u64>()
    }

    pub(crate) fn capture(&self) -> SinkCheckpoint {
        let live = self.live();
        let mut ids = Vec::with_capacity(live.taken.order.len());
        let (first, second) = live.taken.order.as_slices();
        ids.extend_from_slice(first);
        ids.extend_from_slice(second);
        SinkCheckpoint {
            grouping: live
                .queued_from
                .as_ref()
                .expect("checked drained grouping")
                .clone(),
            last_close: live.last_close,
            empty: live.empty,
            server_time: self.server_time,
            ids,
            capacity: live.taken.capacity,
            timeframe: self.timeframe,
            tick_size: live.tick_size,
        }
    }

    pub(crate) fn compatible(&self, saved: &SinkCheckpoint) -> bool {
        let live = self.live();
        self.timeframe == saved.timeframe
            && live.tick_size == saved.tick_size
            && live.taken.capacity == saved.capacity
    }

    /// Called after all chart and feed compatibility checks, while producers still cannot push.
    pub(crate) fn restore(&mut self, saved: &SinkCheckpoint) {
        let live = self
            .state
            .subscriptions
            .get_mut(&self.id)
            .expect("guarded subscription");
        live.restore_grouping(&saved.grouping);
        live.last_close = saved.last_close;
        live.empty = saved.empty;
        live.offset = saved
            .server_time
            .map_or(TimeDelta::zero(), |time| time - Utc::now());
        live.taken.order.clear();
        live.taken.order.extend(saved.ids.iter().copied());
        live.taken.ids.clear();
        live.taken.ids.extend(saved.ids.iter().copied());
        self.state.changed();
    }
}
