//! The stream of one subscription: a socket kept open, the connect and keep-alive messages of
//! the profile, every message split into its records, each record sent to its route, and the
//! connection opened again after it drops.

use std::time::Duration;

use serde_json::Value;

use super::profile::Stream;

/// What a session reports, in order and one at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StreamEvent {
    /// The socket is open and the connect messages have been sent. `again` is true for every
    /// open after the first of this session: records may have been missed since the `Closed`
    /// before it, and the feed fills that gap from history.
    Open { again: bool },
    /// One message, as received, and its records in the order of the message.
    Message { text: String, records: Vec<Routed> },
    /// A message the session cannot read: binary data that is not UTF-8 text, as a compressed
    /// stream sends. `reason` says so with the size.
    Unreadable { reason: String },
    /// The connection ended or could not be opened; `reason` is the socket's. The session
    /// opens it again after `retry_in`.
    Closed { reason: String, retry_in: Duration },
}

/// One record of a message and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Routed {
    /// A record of route `route` of `Stream::routes`, whose kind is bars or trades.
    Record { route: usize, record: Value },
    /// A record of route `route`, whose kind is `Ignore`.
    Ignored { route: usize },
    /// A record that matched no route.
    Unrouted { record: Value },
}

/// The records of one message and their routes, in message order: the split and the routing a
/// session applies to every message (1.3.4). The dialog calls it again for the messages of its
/// log when the user edits the routes, so the log always shows what the feed would do.
pub(crate) fn route(_stream: &Stream, _message: &str) -> Vec<Routed> {
    unimplemented!("part B")
}

/// The stream of one subscription. Dropping it closes the socket and cancels its timers
/// without waiting; no event is started afterwards.
pub(crate) struct Session {}

impl Session {
    /// Opens the stream described by `stream`. `values` fill its templates: `symbol`,
    /// `interval` (the value the service expects) and one pair per key.
    ///
    /// `on_event` is never called from inside this function or from the drop. Natively it runs
    /// on a thread the session owns; in the browser as a task of the page.
    pub(crate) fn open(
        _stream: Stream,
        _values: Vec<(String, String)>,
        _on_event: impl FnMut(StreamEvent) + Send + 'static,
    ) -> Self {
        unimplemented!("part B")
    }
}
