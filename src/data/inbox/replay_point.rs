//! A subscription's state partway through a replay rebuild, so that a later seek can continue
//! from it instead of the first event. The chart drains a rebuild as one batch whose reset
//! discards everything it held, so this state and that of the connected order-flow inputs are
//! all a rebuild produces.

use super::{Live, Sink};
use crate::data::OrderFlowInput;
use crate::data::OrderQueue;
use crate::data::depth_history::{DepthColumn, DepthHistory};
use crate::data::merge::Origin;
use crate::data::order_flow::{InputLock, InputPoint};
use crate::model::{Bar, OrderBookLevel, Tick};
use chrono::{DateTime, Utc};

/// One subscription's inbox state and its connected order-flow inputs partway through a replay
/// rebuild, before the chart drained any of it.
pub(crate) struct RebuildPoint {
    live: Live,
    /// In the order of `live.study_inputs`.
    inputs: Vec<InputPoint>,
    bytes: usize,
}

/// Why a subscription did not continue from a point.
pub(crate) enum RestoreRefused {
    /// Its interval, tick size, trade capacity, depth history or connected studies differ: the
    /// point can never apply again.
    Incompatible,
    /// A chart frame changed it during the rebuild, or an input was locked: try another time.
    Busy,
}

impl RebuildPoint {
    /// Estimated size, as [`Sink::capture_rebuild`] measured it against its budget.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Sink {
    /// The state after the events pushed since `start_rebuild`, when it fits in `budget` bytes.
    /// `None` outside a clean rebuild, while an input is locked, or when it does not fit.
    pub(crate) fn capture_rebuild(&self, budget: usize) -> Option<RebuildPoint> {
        let state = self.inbox.state();
        let live = state.subscriptions.get(&self.id)?;
        if !live.rebuild_clean || !live.updates.reset || live.rebuilt_bars != usize::MAX {
            return None;
        }
        // The sink holds the inbox lock: never wait for an input the chart holds.
        let locks = (live.study_inputs.iter())
            .map(OrderFlowInput::try_lock_replay_point)
            .collect::<Option<Vec<_>>>()?;
        let bytes = (locks.iter()).fold(live.rebuild_bytes(), |sum, lock| {
            sum.saturating_add(lock.bytes())
        });
        (bytes <= budget).then(|| RebuildPoint {
            live: live.clone(),
            inputs: locks.iter().map(InputLock::capture).collect(),
            bytes,
        })
    }

    /// Replaces the state `start_rebuild` just reset with `point`, so the next push continues
    /// it. Wakes the chart as a push does.
    pub(crate) fn restore_rebuild(&self, point: &RebuildPoint) -> Result<(), RestoreRefused> {
        let mut result = Err(RestoreRefused::Busy);
        self.inbox.push(|state| {
            if let Some(live) = state.subscriptions.get_mut(&self.id) {
                result = live.restore_rebuild(point);
            }
            result.is_ok()
        });
        result
    }
}

impl Live {
    fn restore_rebuild(&mut self, point: &RebuildPoint) -> Result<(), RestoreRefused> {
        let saved = &point.live;
        if saved.symbol != self.symbol
            || saved.clock != self.clock
            || saved.tick_size != self.tick_size
            || saved.taken.capacity != self.taken.capacity
            || saved.sampler.as_ref().map(|s| &s.config) != self.sampler.as_ref().map(|s| &s.config)
            || saved.study_inputs.len() != self.study_inputs.len()
            || !(saved.study_inputs.iter().zip(&self.study_inputs))
                .all(|(saved, input)| saved.same_source(input))
        {
            return Err(RestoreRefused::Incompatible);
        }
        if !self.rebuild_clean || self.rebuilt_bars != usize::MAX {
            return Err(RestoreRefused::Busy);
        }
        {
            let mut locks = (self.study_inputs.iter())
                .map(OrderFlowInput::try_lock_replay_point)
                .collect::<Option<Vec<_>>>()
                .ok_or(RestoreRefused::Busy)?;
            if !locks
                .iter()
                .zip(&point.inputs)
                .all(|(lock, saved)| lock.compatible(saved))
            {
                return Err(RestoreRefused::Incompatible);
            }
            for (lock, saved) in locks.iter_mut().zip(&point.inputs) {
                lock.restore(saved);
            }
        }
        // The feed clock was set for this seek; the saved one belongs to an earlier one.
        let offset = self.offset;
        self.clone_from(saved);
        self.offset = offset;
        Ok(())
    }

    /// Estimated bytes of a clone: the inline state and the bars, executions, ids, book levels,
    /// resting orders and depth columns it copies, without allocator overhead. Depth columns are
    /// shared, so this counts them more than once.
    fn rebuild_bytes(&self) -> usize {
        let updates = &self.updates;
        let queues = [self.orders.as_ref(), updates.orders.as_ref()];
        let book = updates.depth.as_ref().map_or(0, |book| {
            (book.bids.len() + book.asks.len()) * size_of::<OrderBookLevel>()
        });
        let columns = (updates.depth_columns.iter())
            .chain(updates.replay_depth.iter().flat_map(DepthHistory::columns))
            .map(|column| {
                size_of::<DepthColumn>()
                    + (column.bids.len() + column.asks.len()) * size_of::<(i64, f64)>()
                    + (column.largest_bid_orders.len() + column.largest_ask_orders.len())
                        * size_of::<f64>()
            });
        [
            size_of::<Live>(),
            updates.bars.len() * size_of::<(DateTime<Utc>, (Bar, Origin))>(),
            updates.trades.len() * (size_of::<Tick>() + size_of::<(u64, usize)>()),
            updates.repeats.len() * size_of::<u64>(),
            self.taken.order.len() * 2 * size_of::<u64>(),
            self.book.clone_bytes(),
            queues
                .into_iter()
                .flatten()
                .map(OrderQueue::clone_bytes)
                .sum(),
            book,
        ]
        .into_iter()
        .chain(columns)
        .fold(0, usize::saturating_add)
    }
}
