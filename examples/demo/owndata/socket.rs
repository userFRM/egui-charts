//! One WebSocket connection: the page's own `WebSocket` in the browser, a blocking client on a
//! thread of its own natively. Text in, text out; nothing here knows about profiles.

/// What a socket reports, in order. `Closed` is the last event of a socket.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SocketEvent {
    /// The connection is open and messages can be sent.
    Open,
    /// A text message, or a binary message that holds UTF-8 text.
    Text(String),
    /// A binary message of this many bytes that is not UTF-8 text.
    Binary(usize),
    /// The connection ended or could not be opened. `reason` is written for the user: the
    /// close code and text the server sent, or what failed (the address, TLS, the status of
    /// the handshake). A browser gives no cause for a failed connection, only code 1006.
    Closed { reason: String },
}

/// A connection that is open or being opened. Dropping it closes the connection without
/// waiting; no event is started afterwards.
pub(crate) struct Socket {}

impl Socket {
    /// Starts connecting to `url` (`ws://` or `wss://`), offering `protocols` as subprotocols.
    ///
    /// Every outcome arrives through `on_event`, also a URL that cannot be opened at all.
    /// `on_event` is never called from inside this function, from [`Socket::send`] or from the
    /// drop. Natively it runs on the socket's own thread; in the browser as a task of the page.
    pub(crate) fn open(
        _url: &str,
        _protocols: &[String],
        _on_event: impl FnMut(SocketEvent) + Send + 'static,
    ) -> Self {
        unimplemented!("part B")
    }

    /// Sends `text` as one text message. Only an open socket sends: before `Open` and after
    /// `Closed` the text is dropped.
    pub(crate) fn send(&self, _text: &str) {
        unimplemented!("part B")
    }
}
