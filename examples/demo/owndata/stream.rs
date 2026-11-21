//! The stream of one subscription: a socket kept open, the connect and keep-alive messages of
//! the profile, every message split into its records, each record sent to its route, and the
//! connection opened again after it drops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;

use super::after;
use super::profile::{RouteKind, Stream, at, fill, fill_url, text};
use super::socket::{Socket, SocketEvent};

/// The wait before the first reconnect, and before the one after a connection that settled.
const FIRST_RETRY: Duration = Duration::from_secs(1);
/// The longest wait before a reconnect.
const LONGEST_RETRY: Duration = Duration::from_secs(60);
/// A connection that was open for this long has settled: the waits start again.
const SETTLED_AFTER: TimeDelta = TimeDelta::seconds(10);

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
/// session applies to every message. The dialog calls it again for the messages of its
/// log when the user edits the routes, so the log always shows what the feed would do.
pub(crate) fn route(stream: &Stream, message: &str) -> Vec<Routed> {
    // A message that is not JSON is one record: its own text.
    let root = serde_json::from_str(message).unwrap_or_else(|_| Value::String(message.to_owned()));
    let records = match at(&root, &stream.records) {
        Some(Value::Array(records)) => records.iter().collect(),
        Some(record @ Value::Object(_)) => vec![record],
        // Nothing to split at the path, as in a status message: the message is the record.
        _ => vec![&root],
    };
    let routed = |record: &Value| {
        let route = if stream.route_by.is_empty() {
            (!stream.routes.is_empty()).then_some(0)
        } else {
            // The record's own value, then the one the message names once at its root, then
            // the record itself when it is a bare value. A record without one takes the route
            // whose value is empty.
            let value = at(record, &stream.route_by)
                .and_then(text)
                .or_else(|| at(&root, &stream.route_by).and_then(text))
                .or_else(|| text(record))
                .unwrap_or_default();
            stream.routes.iter().position(|route| route.value == value)
        };
        match route {
            Some(route) if matches!(stream.routes[route].kind, RouteKind::Ignore) => {
                Routed::Ignored { route }
            }
            Some(route) => Routed::Record {
                route,
                record: record.clone(),
            },
            None => Routed::Unrouted {
                record: record.clone(),
            },
        }
    };
    records.into_iter().map(routed).collect()
}

/// The stream of one subscription. Dropping it closes the socket and cancels its timers
/// without waiting; no event is started afterwards.
pub(crate) struct Session {
    shared: Arc<Shared>,
}

/// What a session, the handler of its socket and its timers share.
struct Shared {
    stream: Stream,
    values: Vec<(String, String)>,
    /// Set by the drop. Read with `on_event` locked, immediately before every call.
    closed: AtomicBool,
    /// The caller's callback. The drop never locks it.
    on_event: Mutex<Box<dyn FnMut(StreamEvent) + Send>>,
    /// Everything that changes. Never locked while `on_event` runs.
    core: Mutex<Core>,
}

struct Core {
    /// The connection in use or being opened.
    socket: Option<Socket>,
    /// True once a socket of this session has opened: the next open is a reconnect.
    opened: bool,
    /// When the connection in use opened.
    open_since: Option<DateTime<Utc>>,
    /// The wait before the next reconnect.
    retry_in: Duration,
    /// Cancels the timer in flight: the keep-alive while open, the reconnect while closed.
    timer: Arc<AtomicBool>,
}

impl Session {
    /// Opens the stream described by `stream`. `values` fill its templates: `symbol`,
    /// `interval` (the value the service expects) and one pair per key.
    ///
    /// `on_event` is never called from inside this function or from the drop. Natively it runs
    /// on a thread the session owns; in the browser as a task of the page.
    pub(crate) fn open(
        stream: Stream,
        values: Vec<(String, String)>,
        on_event: impl FnMut(StreamEvent) + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            stream,
            values,
            closed: AtomicBool::new(false),
            on_event: Mutex::new(Box::new(on_event)),
            core: Mutex::new(Core {
                socket: None,
                opened: false,
                open_since: None,
                retry_in: FIRST_RETRY,
                timer: Arc::default(),
            }),
        });
        shared.connect();
        Self { shared }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        let mut core = self.shared.core();
        core.timer.store(true, Ordering::Release);
        let socket = core.socket.take();
        drop(core);
        drop(socket);
    }
}

impl Shared {
    /// The state that changes. A poisoned lock is entered anyway.
    fn core(&self) -> MutexGuard<'_, Core> {
        self.core.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The values of the templates, borrowed for `fill` and `fill_url`.
    fn values(&self) -> Vec<(&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect()
    }

    /// Calls the caller, unless the session was dropped. Nothing else touches `on_event`.
    fn report(&self, event: StreamEvent) {
        let mut on_event = self.on_event.lock().unwrap_or_else(|e| e.into_inner());
        if !self.closed.load(Ordering::Acquire) {
            on_event(event);
        }
    }

    /// Opens the next socket and stores it, unless the session was dropped. `core` stays
    /// locked meanwhile: the socket's thread can report the open before `Socket::open` has
    /// returned, and its handler must find the socket it sends the connect messages through.
    fn connect(self: &Arc<Self>) {
        let mut core = self.core();
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let values = self.values();
        let url = fill_url(&self.stream.url, &values);
        let offered = |protocol: &String| fill(protocol, &values);
        let protocols: Vec<String> = self.stream.protocols.iter().map(offered).collect();
        // Weak: the socket lives in `core`, so a strong capture would be a cycle.
        let session = Arc::downgrade(self);
        core.socket = Some(Socket::open(&url, &protocols, move |event| {
            if let Some(shared) = session.upgrade() {
                shared.handle(event);
            }
        }));
    }

    /// What the session does with an event of its socket.
    fn handle(self: &Arc<Self>, event: SocketEvent) {
        match event {
            SocketEvent::Open => {
                let mut core = self.core();
                if let Some(socket) = &core.socket {
                    let values = self.values();
                    for message in &self.stream.connect {
                        socket.send(&fill(message, &values));
                    }
                }
                core.open_since = Some(Utc::now());
                if !self.stream.keep_alive.is_empty() {
                    core.timer = Arc::default();
                    self.keep_alive(core.timer.clone());
                }
                let again = std::mem::replace(&mut core.opened, true);
                drop(core);
                self.report(StreamEvent::Open { again });
            }
            SocketEvent::Text(text) => {
                let records = route(&self.stream, &text);
                self.report(StreamEvent::Message { text, records });
            }
            SocketEvent::Binary(len) => {
                let reason = format!("a binary message of {len} bytes that is not text");
                self.report(StreamEvent::Unreadable { reason });
            }
            SocketEvent::Closed { reason } => {
                let mut core = self.core();
                // The keep-alive stops, and the timer is now the wait before the reconnect.
                core.timer.store(true, Ordering::Release);
                core.timer = Arc::default();
                let timer = core.timer.clone();
                let socket = core.socket.take();
                // Wall time: `Instant` is not available in the browser. A clock that was set
                // back makes a long connection look short, which only lengthens the wait.
                let settled = core
                    .open_since
                    .take()
                    .is_some_and(|since| Utc::now() - since >= SETTLED_AFTER);
                let retry_in = if settled { FIRST_RETRY } else { core.retry_in };
                core.retry_in = (retry_in * 2).min(LONGEST_RETRY);
                drop(core);
                drop(socket);
                self.report(StreamEvent::Closed { reason, retry_in });
                let session = Arc::downgrade(self);
                after(retry_in, timer, move || {
                    if let Some(shared) = session.upgrade() {
                        shared.connect();
                    }
                });
            }
        }
    }

    /// Arms the next keep-alive message: after the period it is sent and the one after it is
    /// armed, until `timer` is set at the close or at the drop.
    fn keep_alive(self: &Arc<Self>, timer: Arc<AtomicBool>) {
        let every = Duration::from_secs(self.stream.keep_alive_every.max(1).into());
        let session = Arc::downgrade(self);
        after(every, timer.clone(), move || {
            let Some(shared) = session.upgrade() else {
                return;
            };
            let core = shared.core();
            // Read with `core` locked, where the close and the drop set it: the socket of
            // `core` is still the one this timer was armed for.
            if timer.load(Ordering::Acquire) {
                return;
            }
            if let Some(socket) = &core.socket {
                socket.send(&fill(&shared.stream.keep_alive, &shared.values()));
            }
            drop(core);
            shared.keep_alive(timer);
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use eframe::egui;
    use egui_charts::data::FeedStatus;
    use egui_charts::model::{OrderSide, Tick, Timeframe};
    use egui_charts::widget::{Chart, ChartBuilder, SeriesId};
    use serde_json::json;
    use tungstenite::handshake::server::{Callback, ErrorResponse, Request, Response};
    use tungstenite::protocol::CloseFrame;
    use tungstenite::{Message, WebSocket};

    use super::super::feed::MappedFeed;
    use super::super::profile::{
        BarFields, ConditionRule, Route, RouteKind, Segment, Stream, TimeSpec, TimeUnit,
        TradeFields,
    };
    use super::{Routed, Session, StreamEvent, route};

    /// One step of a scripted connection.
    pub(crate) enum Step {
        /// Wait for this many text messages and record them.
        Receive(usize),
        /// Send a text message.
        Send(&'static str),
        /// Do nothing for this long.
        Wait(Duration),
        /// Record every further text message until the client closes the connection.
        UntilClosed,
        /// Close with this code and a close frame.
        Close(u16),
        /// End the connection without a close frame, as a lost network does.
        Drop,
        /// Wait until the test calls [`Peer::release`]. A test uses it to send the next message
        /// only after it has seen the chart reach a state.
        Hold,
    }

    /// Notes the target a handshake asks for and accepts the handshake.
    struct Asked<'a>(&'a Mutex<Vec<String>>);

    impl Callback for Asked<'_> {
        fn on_request(
            self,
            request: &Request,
            response: Response,
        ) -> Result<Response, ErrorResponse> {
            self.0.lock().unwrap().push(request.uri().to_string());
            Ok(response)
        }
    }

    /// A scripted WebSocket server on a local port. Connection `n` runs `scripts[n]`, one
    /// connection at a time; the listener closes after the last script.
    pub(crate) struct Peer {
        /// `ws://127.0.0.1:PORT`.
        pub(crate) url: String,
        asked: Arc<Mutex<Vec<String>>>,
        received: Arc<Mutex<Vec<Vec<String>>>>,
        ended: Arc<AtomicUsize>,
        held: mpsc::Sender<()>,
    }

    impl Peer {
        pub(crate) fn start(scripts: Vec<Vec<Step>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let asked = Arc::new(Mutex::new(Vec::new()));
            let targets = asked.clone();
            let received = Arc::new(Mutex::new(Vec::new()));
            let log = received.clone();
            let ended = Arc::new(AtomicUsize::new(0));
            let over = ended.clone();
            let (held, released) = mpsc::channel();
            std::thread::spawn(move || {
                // The scripts come first in the pair, so that no connection is accepted after
                // the last one.
                for (script, connection) in scripts.into_iter().zip(listener.incoming()) {
                    let mut socket =
                        tungstenite::accept_hdr(connection.unwrap(), Asked(&targets)).unwrap();
                    log.lock().unwrap().push(Vec::new());
                    let record = |socket: &mut WebSocket<TcpStream>, mut wanted: usize| {
                        while wanted > 0 {
                            match socket.read() {
                                Ok(Message::Text(text)) => {
                                    log.lock()
                                        .unwrap()
                                        .last_mut()
                                        .unwrap()
                                        .push(text.to_string());
                                    wanted -= 1;
                                }
                                Ok(_) => {}
                                Err(_) => return,
                            }
                        }
                    };
                    for step in script {
                        match step {
                            Step::Receive(count) => record(&mut socket, count),
                            Step::Send(text) => socket.send(Message::text(text)).unwrap(),
                            Step::Wait(time) => std::thread::sleep(time),
                            Step::UntilClosed => record(&mut socket, usize::MAX),
                            Step::Close(code) => {
                                let frame = CloseFrame {
                                    code: code.into(),
                                    reason: "".into(),
                                };
                                let _ = socket.close(Some(frame));
                                record(&mut socket, usize::MAX);
                            }
                            Step::Drop => break,
                            Step::Hold => {
                                if released.recv().is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    // Dropping the socket here ends the connection; without a `Close` step no
                    // close frame was sent.
                    drop(socket);
                    over.fetch_add(1, Ordering::Release);
                }
            });
            Self {
                url,
                asked,
                received,
                ended,
                held,
            }
        }

        /// The target each handshake asked for, such as `/stream?key=k`, one per connection in
        /// the order they arrived.
        pub(crate) fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        /// The text messages received so far, one list per connection in the order they arrived.
        pub(crate) fn received(&self) -> Vec<Vec<String>> {
            self.received.lock().unwrap().clone()
        }

        /// How many connections have ended: their scripts are over and their sockets are closed.
        pub(crate) fn ended(&self) -> usize {
            self.ended.load(Ordering::Acquire)
        }

        /// Lets the script pass its next `Hold`. A release that comes first is kept for it.
        pub(crate) fn release(&self) {
            self.held.send(()).unwrap();
        }
    }

    #[test]
    fn messages_split_into_routed_records() {
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let to = |value: &str, kind| Route {
            value: value.to_owned(),
            kind,
        };
        // A status route, a keep-alive answer, trades, a route whose value is a number, and one
        // without a value for the records that have no `type`.
        let stream = Stream {
            records: key("data"),
            route_by: key("type"),
            routes: vec![
                to("status", RouteKind::Ignore),
                to("pong", RouteKind::Ignore),
                to("trade", RouteKind::Trades(TradeFields::default())),
                to("7", RouteKind::Bars(BarFields::default())),
                to("", RouteKind::Ignore),
            ],
            ..Stream::default()
        };
        let record = |route, record| Routed::Record { route, record };
        let whole = r#"[{"type":"trade","p":1,"s":1,"t":1},{"type":"status"}]"#;
        let rows: Vec<(&str, Vec<Routed>)> = vec![
            // 1: nothing at `data`, so the message is the record.
            (
                r#"{"type":"status","message":"authenticated"}"#,
                vec![Routed::Ignored { route: 0 }],
            ),
            // 2: two records without a `type`, routed by the root's.
            (
                r#"{"type":"trade","data":[{"p":"101.5","s":3,"t":1794407400123},{"p":101.25,"s":1,"t":1794407400456}]}"#,
                vec![
                    record(2, json!({"p": "101.5", "s": 3, "t": 1794407400123_i64})),
                    record(2, json!({"p": 101.25, "s": 1, "t": 1794407400456_i64})),
                ],
            ),
            // 3: an empty list holds no record.
            (r#"{"type":"trade","data":[]}"#, vec![]),
            // 4: no route has the value `quote`.
            (
                r#"{"type":"quote","data":[{"b":101.0}]}"#,
                vec![Routed::Unrouted {
                    record: json!({"b": 101.0}),
                }],
            ),
            // 5 and 6: text that is not JSON, and a bare JSON string, are routed by their own text.
            ("pong", vec![Routed::Ignored { route: 1 }]),
            (r#""pong""#, vec![Routed::Ignored { route: 1 }]),
            // 7: an object at the path is one record, with a `type` of its own.
            (
                r#"{"data":{"type":"trade","p":101.5,"s":2,"t":1794407400789}}"#,
                vec![record(
                    2,
                    json!({"type": "trade", "p": 101.5, "s": 2, "t": 1794407400789_i64}),
                )],
            ),
            // 8: a number is compared as JSON prints it.
            (
                r#"{"type":7,"data":[{"o":1}]}"#,
                vec![record(3, json!({"o": 1}))],
            ),
            // 9: an answer to a subscribe or a ping that names no `type` anywhere.
            (
                r#"{"method":"pong","time_in":"2026-11-11T14:33:00.000000Z"}"#,
                vec![Routed::Ignored { route: 4 }],
            ),
            // 10: the root is a list, so `data` leads nowhere and the list is one record, which
            // has no `type`.
            (whole, vec![Routed::Ignored { route: 4 }]),
        ];
        for (message, expected) in rows {
            assert_eq!(route(&stream, message), expected, "{message}");
        }

        // With the root as the place of the records, the list of row 10 is split.
        let rooted = Stream {
            records: vec![],
            ..stream.clone()
        };
        assert_eq!(
            route(&rooted, whole),
            vec![
                record(2, json!({"type": "trade", "p": 1, "s": 1, "t": 1})),
                Routed::Ignored { route: 0 },
            ],
        );
        // Without a routing field every record goes to the first route, and without a route
        // nowhere.
        let unrouted = Stream {
            route_by: vec![],
            ..stream.clone()
        };
        assert_eq!(
            route(&unrouted, r#"{"data":[{"x":1}]}"#),
            vec![Routed::Ignored { route: 0 }],
        );
        let bare = Stream {
            routes: vec![],
            ..unrouted
        };
        assert_eq!(
            route(&bare, r#"{"data":[{"x":1}]}"#),
            vec![Routed::Unrouted {
                record: json!({"x": 1}),
            }],
        );
    }

    #[test]
    fn a_session_logs_in_keeps_alive_and_reconnects() {
        const LOGIN: &str = r#"{"action":"auth","key":"s3cret-7f3a"}"#;
        const SUBSCRIBE: &str = r#"{"action":"subscribe","symbol":"ABC","interval":"1m"}"#;
        const PING: &str = r#"{"action":"ping","symbol":"ABC"}"#;
        const STATUS: &str = r#"{"type":"status","message":"authenticated"}"#;
        const TRADES: &str = r#"{"type":"trade","data":[{"p":101.75,"s":3,"t":1794407710000},{"p":99.75,"s":2,"t":1794407715000}]}"#;
        let peer = Peer::start(vec![
            vec![
                Step::Receive(2),
                Step::Send(STATUS),
                // Longer than two idle ticks of the native client. The message after it must
                // still arrive, and no close may be reported in between.
                Step::Wait(Duration::from_millis(600)),
                Step::Send(TRADES),
                // The first keep-alive, one second after the open.
                Step::Receive(1),
                Step::Close(1008),
            ],
            vec![Step::Receive(2), Step::Drop],
        ]);
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let stream = Stream {
            // No path: the handshake must ask for `/`, as a browser does for this address.
            url: format!("{}?key={{key}}", peer.url),
            connect: vec![
                r#"{"action":"auth","key":"{key}"}"#.to_owned(),
                r#"{"action":"subscribe","symbol":"{symbol}","interval":"{interval}"}"#.to_owned(),
            ],
            keep_alive: r#"{"action":"ping","symbol":"{symbol}"}"#.to_owned(),
            keep_alive_every: 1,
            records: key("data"),
            route_by: key("type"),
            routes: vec![
                Route {
                    value: "status".to_owned(),
                    kind: RouteKind::Ignore,
                },
                Route {
                    value: "trade".to_owned(),
                    kind: RouteKind::Trades(TradeFields::default()),
                },
            ],
            ..Stream::default()
        };
        let values = [
            ("symbol", "ABC"),
            ("interval", "1m"),
            ("key", "s3cret-7f3a"),
        ]
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .to_vec();

        // Every event with the moment it arrived.
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        let session = Session::open(stream, values, move |event| {
            log.lock().unwrap().push((Instant::now(), event));
        });
        let start = Instant::now();
        while events.lock().unwrap().len() < 6 {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "{:?}",
                events.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        // The session would open a third socket two seconds from now. Dropping it cancels that.
        drop(session);

        let events = events.lock().unwrap().clone();
        let trade = |price: f64, size: u64, time: i64| Routed::Record {
            route: 1,
            record: json!({"p": price, "s": size, "t": time}),
        };
        assert_eq!(events[0].1, StreamEvent::Open { again: false });
        assert_eq!(
            events[1].1,
            StreamEvent::Message {
                text: STATUS.to_owned(),
                records: vec![Routed::Ignored { route: 0 }],
            },
        );
        assert_eq!(
            events[2].1,
            StreamEvent::Message {
                text: TRADES.to_owned(),
                records: vec![
                    trade(101.75, 3, 1794407710000),
                    trade(99.75, 2, 1794407715000),
                ],
            },
        );
        assert_eq!(
            events[3].1,
            StreamEvent::Closed {
                reason: "closed by the server (code 1008)".to_owned(),
                retry_in: Duration::from_secs(1),
            },
        );
        // The keep-alive leaves on time although nothing arrives after the trades, so the close
        // that answers it comes long before the ten seconds a read waits during the handshake.
        let answered = events[3].0 - events[0].0;
        assert!(answered < Duration::from_secs(5), "{answered:?}");
        assert_eq!(events[4].1, StreamEvent::Open { again: true });
        let StreamEvent::Closed { reason, retry_in } = &events[5].1 else {
            panic!("{:?}", events[5].1);
        };
        assert!(reason.starts_with("connection lost"), "{reason}");
        assert_eq!(*retry_in, Duration::from_secs(2));
        // The wait is kept, not only reported: the socket is not opened again before it has passed.
        assert!(events[4].0 - events[3].0 >= Duration::from_secs(1));

        assert_eq!(peer.asked(), ["/?key=s3cret-7f3a", "/?key=s3cret-7f3a"]);
        let received = peer.received();
        assert_eq!(received[0][..3], [LOGIN, SUBSCRIBE, PING]);
        assert!(received[0][3..].iter().all(|text| text == PING));
        assert_eq!(received[1], [LOGIN, SUBSCRIBE]);
    }

    /// A chart on the mapped feed, with history from the scripted server and trades from the
    /// scripted peer. A trade that arrives while a page is being fetched is dropped by rule, so
    /// every message of the peer waits behind a `Hold`, which the test releases only once the
    /// chart shows the state the message builds on. Each trade folded into a bar also reaches the
    /// chart's trades and the tape of time and sales, once. The fixture is one-minute bars of
    /// 11 November 2026: 14:35 UTC is second 1,794,407,700, and a trade gives its time in
    /// milliseconds.
    #[test]
    fn a_stream_updates_the_last_bar_and_fills_the_gap() {
        use super::super::feed::tests::{Server, Shown, pages, profile, shown, until};
        // History rows as the service sends them: its newest three at the start, what it holds
        // later, and two of another symbol in the slots of 14:37 and 14:38.
        const J3: &str = r#"{"t":1794407580,"o":100.75,"h":103.0,"l":100.5,"c":102.5,"v":1500}"#;
        const J4: &str = r#"{"t":1794407640,"o":102.5,"h":102.75,"l":101.0,"c":101.25,"v":800}"#;
        const J5: &str = r#"{"t":1794407700,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"v":700}"#;
        const G6: &str = r#"{"t":1794407760,"o":100.5,"h":101.0,"l":100.25,"c":100.75,"v":40}"#;
        const G7: &str = r#"{"t":1794407820,"o":100.75,"h":101.25,"l":100.5,"c":101.0,"v":25}"#;
        const G7F: &str = r#"{"t":1794407820,"o":100.75,"h":101.5,"l":100.5,"c":101.25,"v":60}"#;
        const G8: &str = r#"{"t":1794407880,"o":101.25,"h":101.5,"l":101.0,"c":101.25,"v":10}"#;
        const X0: &str = r#"{"t":1794407820,"o":50.0,"h":50.5,"l":49.75,"c":50.25,"v":100}"#;
        const X1: &str = r#"{"t":1794407880,"o":50.25,"h":50.75,"l":50.0,"c":50.5,"v":80}"#;
        // The messages of the stream.
        const LOGIN: &str = r#"{"action":"auth","key":"s3cret-7f3a"}"#;
        const ABC: &str = r#"{"action":"subscribe","symbol":"ABC","interval":"1m"}"#;
        const XYZ: &str = r#"{"action":"subscribe","symbol":"XYZ","interval":"1m"}"#;
        const STATUS: &str = r#"{"type":"status","message":"authenticated"}"#;
        const IN_LAST_BAR: &str = r#"{"type":"trade","data":[{"p":"101.75","s":3,"t":1794407710000,"side":"buy"},{"p":99.75,"s":2,"t":1794407715000,"side":"sell"},{"p":120.0,"s":50,"t":1794407716000,"c":"X"},{"p":98.0,"s":4,"t":1794407717000,"c":"T"}]}"#;
        const NEW_SLOT: &str = r#"{"type":"trade","data":[{"p":100.5,"s":4,"t":1794407765000}]}"#;
        const AFTER_FILL: &str = r#"{"type":"trade","data":[{"p":101.5,"s":5,"t":1794407850000}]}"#;
        const AFTER_RESUME: &str =
            r#"{"type":"trade","data":[{"p":101.75,"s":2,"t":1794407900000}]}"#;
        const OTHER_SYMBOL: &str =
            r#"{"type":"trade","data":[{"p":51.0,"s":5,"t":1794407920000}]}"#;
        // The bars the chart shows.
        const B3: Shown = (1794407580, Some([100.75, 103.0, 100.5, 102.5, 1500.0]));
        const B4: Shown = (1794407640, Some([102.5, 102.75, 101.0, 101.25, 800.0]));
        const B5: Shown = (1794407700, Some([101.25, 101.5, 100.0, 100.25, 700.0]));
        const B5A: Shown = (1794407700, Some([101.25, 101.75, 99.75, 99.75, 709.0]));
        const T6: Shown = (1794407760, Some([100.5, 100.5, 100.5, 100.5, 4.0]));
        const F6: Shown = (1794407760, Some([100.5, 101.0, 100.25, 100.75, 40.0]));
        const F7: Shown = (1794407820, Some([100.75, 101.25, 100.5, 101.0, 25.0]));
        const F7C: Shown = (1794407820, Some([100.75, 101.5, 100.5, 101.5, 30.0]));
        const R7: Shown = (1794407820, Some([100.75, 101.5, 100.5, 101.25, 60.0]));
        const R8: Shown = (1794407880, Some([101.25, 101.5, 101.0, 101.25, 10.0]));
        const R8D: Shown = (1794407880, Some([101.25, 101.75, 101.0, 101.75, 12.0]));
        const Y0: Shown = (1794407820, Some([50.0, 50.5, 49.75, 50.25, 100.0]));
        const Y1: Shown = (1794407880, Some([50.25, 50.75, 50.0, 50.5, 80.0]));
        const Y1E: Shown = (1794407880, Some([50.25, 51.0, 50.0, 51.0, 85.0]));
        // The trades the chart and time and sales receive: time in milliseconds, price, size and
        // side. The ignored trade at 120.0 is in neither.
        type Fed = (i64, f64, f64, Option<OrderSide>);
        const P1: Fed = (1794407710000, 101.75, 3.0, Some(OrderSide::Buy));
        const P2: Fed = (1794407715000, 99.75, 2.0, Some(OrderSide::Sell));
        const P4: Fed = (1794407717000, 98.0, 4.0, None);
        const P6: Fed = (1794407765000, 100.5, 4.0, None);
        const P8: Fed = (1794407850000, 101.5, 5.0, None);
        const P9: Fed = (1794407920000, 51.0, 5.0, None);
        let fed = |trades: Vec<Tick>| -> Vec<Fed> {
            let row = |t: &Tick| (t.ts.timestamp_millis(), t.price, t.size, t.side);
            trades.iter().map(row).collect()
        };

        let list = |rows: &[&str]| format!("[{}]", rows.join(","));
        let server = Server::start(pages(vec![
            list(&[J3, J4, J5]),
            "[]".to_owned(),
            list(&[G6, G7]),
            list(&[G6, G7F, G8]),
            "[]".to_owned(),
            list(&[X0, X1]),
            "[]".to_owned(),
        ]));
        let peer = Peer::start(vec![
            vec![
                Step::Receive(2),
                Step::Hold,
                Step::Send(STATUS),
                Step::Send(IN_LAST_BAR),
                Step::Hold,
                Step::Send(NEW_SLOT),
                Step::Hold,
                Step::Drop,
            ],
            vec![
                Step::Receive(2),
                Step::Hold,
                Step::Send(AFTER_FILL),
                Step::UntilClosed,
            ],
            vec![
                Step::Receive(2),
                Step::Hold,
                Step::Send(AFTER_RESUME),
                Step::UntilClosed,
            ],
            vec![
                Step::Receive(2),
                Step::Hold,
                Step::Send(OTHER_SYMBOL),
                Step::UntilClosed,
            ],
        ]);

        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let mut profile = profile(&server.origin);
        profile.keys = vec!["key".to_owned()];
        profile.stream = Some(Stream {
            url: format!("{}/{{key}}/stream?key={{key}}", peer.url),
            connect: vec![
                r#"{"action":"auth","key":"{key}"}"#.to_owned(),
                r#"{"action":"subscribe","symbol":"{symbol}","interval":"{interval}"}"#.to_owned(),
            ],
            records: key("data"),
            route_by: key("type"),
            routes: vec![
                Route {
                    value: "status".to_owned(),
                    kind: RouteKind::Ignore,
                },
                Route {
                    value: "trade".to_owned(),
                    kind: RouteKind::Trades(TradeFields {
                        time: TimeSpec::Instant {
                            field: key("t"),
                            unit: TimeUnit::Milliseconds,
                        },
                        price: key("p"),
                        size: key("s"),
                        condition: key("c"),
                        conditions: vec![
                            ("T".to_owned(), ConditionRule::VolumeOnly),
                            ("X".to_owned(), ConditionRule::Ignore),
                        ],
                        side: key("side"),
                        buy: "buy".to_owned(),
                        sell: "sell".to_owned(),
                    }),
                },
            ],
            ..Stream::default()
        });
        profile.tick_size = Some(0.25);
        let secrets = vec![("key".to_owned(), "s3cret-7f3a".to_owned())];
        let feed = Arc::new(MappedFeed::new(profile, secrets));
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("ABC")
            .with_timeframe(Timeframe::Min1)
            .with_initial_bars(3)
            .build();
        let ctx = egui::Context::default();
        let state = |chart: &Chart| {
            let state = chart.feed_state(SeriesId::MAIN);
            state.cloned().unwrap_or_default()
        };
        // Waits until history has ended and the chart shows exactly `bars`.
        let see = |chart: &mut Chart, what: &str, bars: &[Shown]| {
            until(&ctx, chart, what, |chart| {
                state(chart).history_complete && shown(chart) == bars
            });
        };

        // 1. History. The instrument carries the profile's price step.
        see(&mut chart, "history", &[B3, B4, B5]);
        assert_eq!(chart.instrument().and_then(|i| i.tick_size), Some(0.25));
        // 2. Trades in the slot of the newest bar continue that bar. Its open stays the one of
        // history, 101.75, sent as text, is the high, 99.75 is the low and the close, and the
        // volume is 700 + 3 + 2 + 4: the ignored trade counts nothing, and the volume only
        // trade at 98.0 counts its size without moving a price.
        peer.release();
        see(&mut chart, "trades in the last bar", &[B3, B4, B5A]);
        // 3. The first trade of the next slot opens the next bar, at its price and with its size.
        peer.release();
        see(&mut chart, "a new slot", &[B3, B4, B5A, T6]);
        let trades = fed(chart.trades(SeriesId::MAIN).cloned().collect());
        assert_eq!(trades, [P1, P2, P4, P6]);
        let tape = feed.drain_tape("ABC", Some(Timeframe::Min1));
        assert_eq!(fed(tape), [P1, P2, P4, P6]);

        // 4. The connection is lost. The chart says so with the endpoint, and without the key
        // that travels in the address: in its path the key is shown by its name.
        peer.release();
        until(&ctx, &mut chart, "the drop", |chart| {
            state(chart).last_error.is_some()
        });
        assert_eq!(state(&chart).status, Some(FeedStatus::Disconnected));
        let error = state(&chart).last_error.unwrap().to_string();
        let lost = format!(
            "network error: {}/{{key}}/stream: connection lost",
            peer.url
        );
        assert!(error.starts_with(&lost), "{error}");
        // The session connects again after a second and the feed loads what was missed, from the
        // slot of the newest bar it knows: the final state of the bar that was forming takes the
        // place of the bar the one trade built, and the bar after it is new.
        see(&mut chart, "the gap filled", &[B3, B4, B5A, F6, F7]);
        assert_eq!(state(&chart).status, Some(FeedStatus::Live));
        // 5. The next trade continues the newest bar of the fill: 101.5 is its high and its
        // close, and the volume is 25 + 5.
        peer.release();
        see(
            &mut chart,
            "a trade after the fill",
            &[B3, B4, B5A, F6, F7C],
        );
        // The bars the fill loaded, from 14:36:05 to 14:37:30, bring no trades.
        let trades = fed(chart.trades(SeriesId::MAIN).cloned().collect());
        assert_eq!(trades, [P1, P2, P4, P6, P8]);

        // 6. A paused chart has no socket: the peer sees the connection end during the pause.
        chart.set_feed_paused(true);
        until(&ctx, &mut chart, "the pause to close the socket", |_| {
            peer.ended() == 2
        });
        // Resuming loads the latest page and subscribes again. The chart drops its last bar and
        // keeps the others: the row of 14:37 takes the place of the dropped bar and the row of
        // 14:38 is new.
        chart.set_feed_paused(false);
        see(&mut chart, "the resume", &[B3, B4, B5A, F6, R7, R8]);
        // 7. The next trade continues the newest bar of that page: 101.75 is its high and its
        // close, and the volume is 10 + 2.
        peer.release();
        see(
            &mut chart,
            "a trade after the resume",
            &[B3, B4, B5A, F6, R7, R8D],
        );

        // 8. Another symbol: its own history and its own subscription, nothing of the old.
        chart.set_symbol("XYZ");
        see(&mut chart, "the other symbol", &[Y0, Y1]);
        // 9. Its trade continues its newest bar, in the slot the old symbol's newest bar had:
        // 51.0 is the high and the close, and the volume is 80 + 5.
        peer.release();
        see(&mut chart, "its trade", &[Y0, Y1E]);
        // The chart keeps the trades of the shown symbol only, and so does time and sales: the
        // trades of steps 5 and 7, still in the tape, are of the old symbol.
        assert_eq!(fed(chart.trades(SeriesId::MAIN).cloned().collect()), [P9]);
        assert_eq!(fed(feed.drain_tape("XYZ", Some(Timeframe::Min1))), [P9]);

        let lines: Vec<String> = server.seen().iter().map(|seen| seen.line.clone()).collect();
        assert_eq!(lines.len(), 7, "{lines:?}");
        // The first load, the gap fill and the load after the resume end at the system clock.
        for latest in [0, 2, 3] {
            assert!(
                lines[latest].starts_with("GET /bars?symbol=ABC&interval=1m&to="),
                "{lines:?}"
            );
            assert!(lines[latest].ends_with("&limit=3"), "{lines:?}");
        }
        // Scroll-back asks before the oldest bar held, also after the resume.
        assert_eq!(
            lines[1],
            "GET /bars?symbol=ABC&interval=1m&to=1794407580&limit=3"
        );
        assert_eq!(lines[4], lines[1]);
        assert!(
            lines[5].starts_with("GET /bars?symbol=XYZ&interval=1m&to="),
            "{lines:?}"
        );
        assert_eq!(
            lines[6],
            "GET /bars?symbol=XYZ&interval=1m&to=1794407820&limit=3"
        );
        // One socket at a time, each opened with the login and the subscription of its symbol.
        assert_eq!(
            peer.received(),
            [[LOGIN, ABC], [LOGIN, ABC], [LOGIN, ABC], [LOGIN, XYZ]]
        );
    }

    /// Time and sales lists a trade once while a study reads the chart's symbol on the chart's
    /// interval, as Volume Delta does on a one-minute chart: the study's subscription has a
    /// socket of its own, which receives the trade again. A second chart on the feed stands for
    /// the study.
    #[test]
    fn a_trade_is_listed_once_while_a_study_streams_the_same_interval() {
        use super::super::feed::tests::{Server, profile, shown, until};
        // The bar of midnight on 11 November 2026, which the trade ten seconds later continues.
        const BAR: &str = r#"[{"t":1794355200,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"v":700}]"#;
        const TRADE: &str = r#"{"type":"trade","data":[{"p":101.75,"s":3,"t":1794355210000}]}"#;
        let server = Server::start(|_, seen| {
            // A request for bars before the bar held gets none, which ends history.
            let older = seen.line.contains("&to=1794355200&");
            let page = if older { "[]" } else { BAR };
            (200, page.to_owned())
        });
        // One socket at a time: the first gets the trade and is closed, then the second gets the
        // trade too, and the first one's new socket waits behind it.
        let peer = Peer::start(vec![
            vec![
                Step::Receive(1),
                Step::Hold,
                Step::Send(TRADE),
                Step::Close(1000),
            ],
            vec![Step::Receive(1), Step::Send(TRADE), Step::UntilClosed],
        ]);
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let mut profile = profile(&server.origin);
        profile.stream = Some(Stream {
            url: peer.url.clone(),
            connect: vec![r#"{"symbol":"{symbol}"}"#.to_owned()],
            records: key("data"),
            route_by: key("type"),
            routes: vec![Route {
                value: "trade".to_owned(),
                kind: RouteKind::Trades(TradeFields {
                    time: TimeSpec::Instant {
                        field: key("t"),
                        unit: TimeUnit::Milliseconds,
                    },
                    price: key("p"),
                    size: key("s"),
                    ..TradeFields::default()
                }),
            }],
            ..Stream::default()
        });
        let feed = Arc::new(MappedFeed::new(profile, Vec::new()));
        let open = || {
            ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("ABC")
                .with_timeframe(Timeframe::Min1)
                .with_initial_bars(3)
                .build()
        };
        let (mut chart, mut study) = (open(), open());
        let ctx = egui::Context::default();
        // Whether the newest bar of a chart holds this volume.
        let holds = |volume: f64| {
            move |chart: &Chart| {
                let newest = shown(chart).last().and_then(|(_, bar)| *bar);
                newest.is_some_and(|bar| bar[4] == volume)
            }
        };
        until(&ctx, &mut chart, "the chart's bar", holds(700.0));
        until(&ctx, &mut study, "the study's bar", holds(700.0));
        peer.release();
        // Both subscriptions fold the trade: 700 + 3.
        until(&ctx, &mut chart, "the chart's trade", holds(703.0));
        until(&ctx, &mut study, "the study's trade", holds(703.0));
        let row = |t: &Tick| (t.ts.timestamp_millis(), t.price);
        let tape = feed.drain_tape("ABC", Some(Timeframe::Min1));
        let listed: Vec<_> = tape.iter().map(row).collect();
        assert_eq!(listed, [(1794355210000, 101.75)]);
    }

    /// A study that loads the history of the chart's symbol on the chart's interval, as Volume
    /// Delta does on a one-minute chart, leaves the bar the chart's stream is forming as it was.
    /// The service's pages end at the last closed bar, so only the stream knows the minute that
    /// is forming. A second chart on the feed stands for the study.
    #[test]
    fn a_study_page_on_the_chart_interval_keeps_the_forming_bar() {
        use super::super::feed::tests::{Server, Shown, profile, shown, until};
        // The closed bar of 23:59 on 10 November 2026, the newest the service holds.
        const BAR: &str = r#"[{"t":1794355140,"o":100.0,"h":100.5,"l":99.5,"c":100.25,"v":500}]"#;
        const CLOSED: Shown = (1794355140, Some([100.0, 100.5, 99.5, 100.25, 500.0]));
        // Two trades of the minute of midnight before the study loads, and after it a third one
        // of that minute and one of the next.
        const BEFORE: &str = r#"{"type":"trade","data":[{"p":101.0,"s":3,"t":1794355210000},{"p":102.0,"s":2,"t":1794355220000}]}"#;
        const AFTER: &str = r#"{"type":"trade","data":[{"p":100.5,"s":4,"t":1794355230000},{"p":100.75,"s":1,"t":1794355265000}]}"#;
        const NEXT: Shown = (1794355260, Some([100.75, 100.75, 100.75, 100.75, 1.0]));
        let server = Server::start(|_, seen| {
            // A request for bars before the closed bar gets none, which ends history.
            let older = seen.line.contains("&to=1794355140&");
            let page = if older { "[]" } else { BAR };
            (200, page.to_owned())
        });
        // The chart's socket. The study's waits behind it and never opens.
        let peer = Peer::start(vec![vec![
            Step::Receive(1),
            Step::Hold,
            Step::Send(BEFORE),
            Step::Hold,
            Step::Send(AFTER),
            Step::UntilClosed,
        ]]);
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let mut profile = profile(&server.origin);
        profile.stream = Some(Stream {
            url: peer.url.clone(),
            connect: vec![r#"{"symbol":"{symbol}"}"#.to_owned()],
            records: key("data"),
            route_by: key("type"),
            routes: vec![Route {
                value: "trade".to_owned(),
                kind: RouteKind::Trades(TradeFields {
                    time: TimeSpec::Instant {
                        field: key("t"),
                        unit: TimeUnit::Milliseconds,
                    },
                    price: key("p"),
                    size: key("s"),
                    ..TradeFields::default()
                }),
            }],
            ..Stream::default()
        });
        let feed = Arc::new(MappedFeed::new(profile, Vec::new()));
        let open = || {
            ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("ABC")
                .with_timeframe(Timeframe::Min1)
                .with_initial_bars(3)
                .build()
        };
        let complete = |chart: &Chart| {
            let state = chart.feed_state(SeriesId::MAIN);
            state.is_some_and(|state| state.history_complete)
        };
        let ctx = egui::Context::default();
        let mut chart = open();
        until(&ctx, &mut chart, "the chart's history", |chart| {
            complete(chart) && shown(chart) == [CLOSED]
        });
        peer.release();
        let forming: Shown = (1794355200, Some([101.0, 102.0, 101.0, 102.0, 5.0]));
        until(&ctx, &mut chart, "the forming bar", |chart| {
            shown(chart) == [CLOSED, forming]
        });
        let mut study = open();
        until(&ctx, &mut study, "the study's history", |study| {
            complete(study) && shown(study) == [CLOSED]
        });
        peer.release();
        until(&ctx, &mut chart, "the next minute", |chart| {
            shown(chart).last() == Some(&NEXT)
        });
        // The minute of midnight holds its three trades: 3 + 2 + 4.
        let midnight: Shown = (1794355200, Some([101.0, 102.0, 100.5, 100.5, 9.0]));
        assert_eq!(shown(&chart), [CLOSED, midnight, NEXT]);
    }

    /// Every ready profile reads its service's history rows, bar messages and trade messages,
    /// in the form the service documents them, into the chart's bars and trades. The service's
    /// hosts are swapped for local ones, so nothing leaves the machine. The fixtures are
    /// one-minute bars of 11 November 2026 from 14:33 UTC (second 1,794,407,580) and daily bars
    /// of the 9th to the 11th; every expected bar and request is worked out by hand.
    #[test]
    fn ready_profiles_chart_recorded_history_and_stream() {
        use super::super::feed::tests::{Server, Shown, pages, shown, until};
        use super::super::profile::Profile;
        use super::super::store;

        /// A ready profile on recorded answers. The pages answer the history requests in order;
        /// the script runs the peer's one connection, each `Hold` until the bars before it are
        /// shown.
        struct Ready {
            name: &'static str,
            symbol: &'static str,
            timeframe: Timeframe,
            pages: Vec<String>,
            script: Vec<Step>,
            /// The bars once history has ended, then after each release of the peer.
            bars: Vec<Vec<Shown>>,
            /// What the chart and the tape hold at the end: time in ms, price, size, side.
            trades: Vec<(i64, f64, f64, Option<OrderSide>)>,
            tick_size: f64,
            /// The start and the end of the first request line, whose bound comes from the
            /// clock.
            first: (&'static str, &'static str),
            /// Every later request line.
            asked: Vec<&'static str>,
            /// The handshake's target and the messages the peer received.
            target: &'static str,
            received: Vec<&'static str>,
        }

        // Binance: rows of twelve columns, times in milliseconds, values as text.
        const R3: &str = r#"[1794407580000,"100750.00","103000.00","100500.00","102500.00","1.50000000",1794407639999,"153375.00000000",120,"0.75000000","76687.50000000","0"]"#;
        const R4: &str = r#"[1794407640000,"102500.00","102750.00","101000.00","101250.00","0.75000000",1794407699999,"76312.50000000",64,"0.25000000","25437.50000000","0"]"#;
        const R5: &str = r#"[1794407700000,"101250.00","101500.00","100000.00","100250.00","0.50000000",1794407759999,"50437.50000000",41,"0.25000000","25218.75000000","0"]"#;
        const KLINE: &str = r#"{"stream":"btcusdt@kline_1m","data":{"e":"kline","E":1794407711000,"s":"BTCUSDT","k":{"t":1794407700000,"T":1794407759999,"s":"BTCUSDT","i":"1m","f":4100,"L":4180,"o":"101250.00","c":"99750.00","h":"101750.00","l":"99750.00","v":"0.75000000","n":81,"x":false,"q":"75687.50000000","V":"0.25000000","Q":"25312.50000000","B":"0"}}}"#;
        // `m` is true when the buyer was the maker: the trade sold into the bid.
        const SELL: &str = r#"{"stream":"btcusdt@aggTrade","data":{"e":"aggTrade","E":1794407712001,"s":"BTCUSDT","a":2001,"p":"100000.00","q":"0.12500000","f":4181,"l":4182,"T":1794407712000,"m":true,"M":true}}"#;
        const BUY: &str = r#"{"stream":"btcusdt@aggTrade","data":{"e":"aggTrade","E":1794407713001,"s":"BTCUSDT","a":2002,"p":"102000.00","q":"0.25000000","f":4183,"l":4183,"T":1794407713000,"m":false,"M":true}}"#;
        const B3: Shown = (
            1794407580,
            Some([100750.0, 103000.0, 100500.0, 102500.0, 1.5]),
        );
        const B4: Shown = (
            1794407640,
            Some([102500.0, 102750.0, 101000.0, 101250.0, 0.75]),
        );
        const B5: Shown = (
            1794407700,
            Some([101250.0, 101500.0, 100000.0, 100250.0, 0.5]),
        );
        // The kline takes the place of the history bar.
        const K5: Shown = (
            1794407700,
            Some([101250.0, 101750.0, 99750.0, 99750.0, 0.75]),
        );
        // The trades continue the kline's bar: the sell at 100000 is the close, volume 0.875;
        // the buy at 102000 is the high and the close, volume 1.125. Continuing the history
        // bar would give the low 100000 and the volume 0.875.
        const K5T: Shown = (
            1794407700,
            Some([101250.0, 102000.0, 99750.0, 102000.0, 1.125]),
        );

        // Coinbase Exchange: daily rows of time in seconds, low, high, open, close and volume,
        // newest first, the 9th to the 11th of November.
        const C9: &str = "[1794182400,97000,99500,98000,99000,120.5]";
        const C10: &str = "[1794268800,98500,101000,99000,100500,98.25]";
        const C11: &str = "[1794355200,99500,101500,100500,101000,75.5]";
        // The trade before the subscription, already in the history bar.
        const LAST_MATCH: &str = r#"{"type":"last_match","trade_id":29001,"maker_order_id":"ac928c66-ca53-498f-9c13-a110027a60e8","taker_order_id":"132fb6ae-456b-4654-b4e0-d681ac05cea1","side":"buy","size":"0.50000000","price":"95000.00","product_id":"BTC-USD","sequence":61001,"time":"2026-11-11T14:35:11.000000Z"}"#;
        // `side` is the side of the resting order: a `sell` match is a buy.
        const SOLD: &str = r#"{"type":"match","trade_id":29002,"maker_order_id":"5b8e0a3c-1d6f-4f0e-9a7b-2c4d6e8f0a1b","taker_order_id":"7c9e1b4d-2e70-4a1f-8b6c-3d5e7f9a1b2c","side":"sell","size":"0.25000000","price":"102000.00","product_id":"BTC-USD","sequence":61002,"time":"2026-11-11T14:35:12.000000Z"}"#;
        const BOUGHT: &str = r#"{"type":"match","trade_id":29003,"maker_order_id":"8d0f2c5e-3f81-4b2a-9c7d-4e6f8a0b2c3d","taker_order_id":"9e1a3d6f-4a92-4c3b-8d8e-5f7a9b1c3d4e","side":"buy","size":"0.12500000","price":"100000.00","product_id":"BTC-USD","sequence":61003,"time":"2026-11-11T14:35:13.000000Z"}"#;
        const D9: Shown = (
            1794182400,
            Some([98000.0, 99500.0, 97000.0, 99000.0, 120.5]),
        );
        const D10: Shown = (
            1794268800,
            Some([99000.0, 101000.0, 98500.0, 100500.0, 98.25]),
        );
        const D11: Shown = (
            1794355200,
            Some([100500.0, 101500.0, 99500.0, 101000.0, 75.5]),
        );
        // The buy at 102000 is the high, the sell at 100000 the close; volume 75.875. The last
        // match at 95000 would be the low.
        const D11T: Shown = (
            1794355200,
            Some([100500.0, 102000.0, 99500.0, 100000.0, 75.875]),
        );

        // Kraken: rows under the pair's name, time in seconds, then open, high, low, close,
        // vwap, volume and count; values as text. `last` beside them is not a row.
        const OHLC: &str = r#"{"error":[],"result":{"BTC/USD":[[1794407580,"100750.0","103000.0","100500.0","102500.0","101900.0","1.50000000",120],[1794407640,"102500.0","102750.0","101000.0","101250.0","101800.0","0.75000000",64],[1794407700,"101250.0","101500.0","100000.0","100250.0","100700.0","0.50000000",41]],"last":1794407640}}"#;
        // `side` is the taker's.
        const TRADES: &str = r#"{"channel":"trade","type":"update","data":[{"symbol":"BTC/USD","side":"sell","price":100000.0,"qty":0.125,"ord_type":"market","trade_id":4665906,"timestamp":"2026-11-11T14:35:12.000000Z"},{"symbol":"BTC/USD","side":"buy","price":102000.0,"qty":0.25,"ord_type":"limit","trade_id":4665907,"timestamp":"2026-11-11T14:35:13.000000Z"}]}"#;
        // The trades continue the history bar: the sell at 100000 is the close, the buy at
        // 102000 the high and the close; volume 0.875.
        const B5T: Shown = (
            1794407700,
            Some([101250.0, 102000.0, 100000.0, 102000.0, 0.875]),
        );

        let rows = [
            Ready {
                name: "Binance spot (public)",
                symbol: "BTCUSDT",
                timeframe: Timeframe::Min1,
                // At or before `endTime`, so the older page holds the row that opens at it. That
                // row is the symbol's first: the page ends history.
                pages: vec![format!("[{R3},{R4},{R5}]"), format!("[{R3}]")],
                script: vec![
                    Step::Hold,
                    Step::Send(KLINE),
                    Step::Hold,
                    Step::Send(SELL),
                    Step::Send(BUY),
                    Step::UntilClosed,
                ],
                bars: vec![vec![B3, B4, B5], vec![B3, B4, K5], vec![B3, B4, K5T]],
                trades: vec![
                    (1794407712000, 100000.0, 0.125, Some(OrderSide::Sell)),
                    (1794407713000, 102000.0, 0.25, Some(OrderSide::Buy)),
                ],
                tick_size: 0.01,
                first: (
                    "GET /api/v3/klines?symbol=BTCUSDT&interval=1m&endTime=",
                    "&limit=3",
                ),
                // Before the oldest bar held, in milliseconds; the chart asks 600 bars at a time.
                asked: vec![
                    "GET /api/v3/klines?symbol=BTCUSDT&interval=1m&endTime=1794407580000&limit=600",
                ],
                target: "/stream?streams=btcusdt@kline_1m/btcusdt@aggTrade",
                received: vec![],
            },
            Ready {
                name: "Coinbase Exchange (public)",
                symbol: "BTC-USD",
                // Daily, so that two empty older windows of 299 days reach past a year and end
                // history.
                timeframe: Timeframe::Day1,
                // Newest first. `end` is inclusive, so the older page holds the row that opens at
                // it; that row is the symbol's first.
                pages: vec![
                    format!("[{C11},{C10},{C9}]"),
                    format!("[{C9}]"),
                    "[]".to_owned(),
                ],
                script: vec![
                    Step::Receive(1),
                    Step::Hold,
                    Step::Send(LAST_MATCH),
                    Step::Send(SOLD),
                    Step::Send(BOUGHT),
                    Step::UntilClosed,
                ],
                bars: vec![vec![D9, D10, D11], vec![D9, D10, D11T]],
                trades: vec![
                    (1794407712000, 102000.0, 0.25, Some(OrderSide::Buy)),
                    (1794407713000, 100000.0, 0.125, Some(OrderSide::Sell)),
                ],
                tick_size: 0.01,
                first: ("GET /products/BTC-USD/candles?granularity=86400&start=", ""),
                // Windows of 299 days, in seconds: at most 300 candles with both ends counted.
                asked: vec![
                    "GET /products/BTC-USD/candles?granularity=86400&start=1768348800&end=1794182400",
                    "GET /products/BTC-USD/candles?granularity=86400&start=1742515200&end=1768348800",
                ],
                target: "/",
                received: vec![
                    r#"{"type":"subscribe","product_ids":["BTC-USD"],"channels":["matches"]}"#,
                ],
            },
            Ready {
                name: "Kraken (public)",
                symbol: "BTC/USD",
                timeframe: Timeframe::Min1,
                // Kraken sends its newest bars and no older ones, so the chart asks once.
                pages: vec![OHLC.to_owned()],
                script: vec![
                    Step::Receive(1),
                    Step::Hold,
                    Step::Send(TRADES),
                    Step::UntilClosed,
                ],
                bars: vec![vec![B3, B4, B5], vec![B3, B4, B5T]],
                trades: vec![
                    (1794407712000, 100000.0, 0.125, Some(OrderSide::Sell)),
                    (1794407713000, 102000.0, 0.25, Some(OrderSide::Buy)),
                ],
                tick_size: 0.01,
                first: (
                    "GET /0/public/OHLC?pair=BTC%2FUSD&interval=1&assetVersion=1",
                    "",
                ),
                asked: vec![],
                target: "/v2",
                received: vec![
                    r#"{"method":"subscribe","params":{"channel":"trade","symbol":["BTC/USD"]}}"#,
                ],
            },
        ];

        for row in rows {
            let server = Server::start(pages(row.pages));
            let peer = Peer::start(vec![row.script]);
            let named = |ready: &Profile| ready.name == row.name;
            let mut profile = store::ready().into_iter().find(named).unwrap();
            // The service's address is kept after its host, which is the local one.
            let local = |url: &mut String, origin: &str| {
                let rest = url.split_once("://").unwrap().1;
                let path = rest.find('/').map_or("", |at| &rest[at..]);
                *url = format!("{origin}{path}");
            };
            local(&mut profile.history.as_mut().unwrap().url, &server.origin);
            local(&mut profile.stream.as_mut().unwrap().url, &peer.url);
            let feed = Arc::new(MappedFeed::new(profile, Vec::new()));
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol(row.symbol)
                .with_timeframe(row.timeframe)
                .with_initial_bars(3)
                .build();
            let ctx = egui::Context::default();
            for (step, bars) in row.bars.iter().enumerate() {
                if step > 0 {
                    peer.release();
                }
                let what = format!("{}, bars {step}", row.name);
                until(&ctx, &mut chart, &what, |chart| {
                    let state = chart.feed_state(SeriesId::MAIN);
                    state.is_some_and(|state| state.history_complete) && shown(chart) == *bars
                });
            }
            let fed = |trades: Vec<Tick>| -> Vec<(i64, f64, f64, Option<OrderSide>)> {
                let row = |t: &Tick| (t.ts.timestamp_millis(), t.price, t.size, t.side);
                trades.iter().map(row).collect()
            };
            let held = fed(chart.trades(SeriesId::MAIN).cloned().collect());
            assert_eq!(held, row.trades, "{}", row.name);
            let tape = fed(feed.drain_tape(row.symbol, Some(row.timeframe)));
            assert_eq!(tape, row.trades, "{}", row.name);
            let step = chart
                .instrument()
                .and_then(|instrument| instrument.tick_size);
            assert_eq!(step, Some(row.tick_size), "{}", row.name);

            let lines: Vec<String> = server.seen().iter().map(|seen| seen.line.clone()).collect();
            let (start, end) = row.first;
            let first = lines.first().map(String::as_str).unwrap_or_default();
            assert!(
                first.starts_with(start) && first.ends_with(end),
                "{lines:?}"
            );
            assert_eq!(lines[1..], row.asked, "{}", row.name);
            assert_eq!(peer.asked(), [row.target], "{}", row.name);
            assert_eq!(peer.received(), [row.received], "{}", row.name);
        }
    }
}
