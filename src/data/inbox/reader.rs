//! The reading end of a sink outside a chart, for hosts that test or forward a feed.

use super::{Inbox, Sink};
use crate::data::{FeedError, FeedStatus, SubscriptionId};
use crate::model::{Bar, BarClock, Level1Quote, OrderBook};
use std::sync::Arc;

/// A [`Sink`] outside a chart and what is pushed into it, folded by the rules a chart's sink
/// follows: to test a [`Datafeed`](crate::data::Datafeed) or to forward its pushes. Dropping
/// the reader ends the subscription: [`Sink::is_active`] turns false and later pushes are
/// ignored.
pub struct SinkReader {
    inbox: Arc<Inbox>,
    id: SubscriptionId,
}

impl std::fmt::Debug for SinkReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SinkReader")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl SinkReader {
    /// A sink for `symbol` whose trades fold into bars of `clock` and whose book keys prices by
    /// integer ticks of `tick_size`, as for an instrument with that
    /// [`tick_size`](crate::model::Instrument::tick_size). It ignores quotes of another symbol,
    /// and a range interval needs a finite tick size above zero, or its trades are rejected.
    pub fn new(
        symbol: impl Into<String>,
        clock: impl Into<BarClock>,
        tick_size: Option<f64>,
    ) -> (Self, Sink) {
        let inbox = Arc::new(Inbox::default());
        // As many trades as a chart keeps, so repeated trade ids are recognised as a chart does.
        let sink = inbox.subscribe(symbol.into(), clock.into(), 100_000, tick_size);
        let id = sink.id;
        (Self { inbox, id }, sink)
    }

    /// What was pushed since the last call. Empty when nothing was.
    pub fn take(&self) -> SinkUpdates {
        // The inbox holds this one subscription while the reader lives.
        let Some((_, updates)) = self.inbox.drain().updates.pop() else {
            return SinkUpdates::default();
        };
        SinkUpdates {
            bars: updates.bars.into_values().map(|(bar, _)| bar).collect(),
            book: updates.depth,
            quote: updates.quote,
            status: updates.status,
            error: updates.error,
        }
    }
}

impl Drop for SinkReader {
    fn drop(&mut self) {
        self.inbox.unsubscribe(self.id);
    }
}

/// What a feed pushed into the sink of a [`SinkReader`] since the last
/// [`take`](SinkReader::take).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SinkUpdates {
    /// The bar of each slot the pushes changed, oldest first; the last one is the forming bar. A
    /// bar built from trades or a heartbeat is stamped with its slot's open, a pushed bar keeps
    /// its time, and a metric point arrives as a bar whose prices are its value.
    pub bars: Vec<Bar>,
    /// The order book after the depth pushes, when they changed it.
    pub book: Option<OrderBook>,
    /// The latest accepted top-of-book quote.
    pub quote: Option<Level1Quote>,
    /// The latest connection status.
    pub status: Option<FeedStatus>,
    /// The latest stream error: one the feed reported, a rejected quote, or bars dropped over
    /// the limit of 100,000 pending slots.
    pub error: Option<FeedError>,
}
