//! One WebSocket connection: the page's own `WebSocket` in the browser, a blocking client on a
//! thread of its own natively. Text in, text out; nothing here knows about profiles.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::Socket;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::Socket;

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

/// The reason of a close that the server asked for: its code, and its text when it sent one.
fn closed_by_server(code: u16, text: &str) -> String {
    if text.is_empty() {
        format!("closed by the server (code {code})")
    } else {
        format!("closed by the server (code {code}): {text}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fmt::Display;
    use std::io::{self, Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::Duration;

    use tungstenite::client::IntoClientRequest as _;
    use tungstenite::http::Uri;
    use tungstenite::{ClientRequestBuilder, Error, HandshakeError, Message, WebSocket};

    use super::super::{Failure, endpoint};
    use super::{SocketEvent, closed_by_server};

    /// The longest wait for one address to connect and for each read and write of the handshake.
    const PATIENCE: Duration = Duration::from_secs(10);
    /// How long a read waits for data before the thread looks at its queue and its flag again.
    const TICK: Duration = Duration::from_millis(250);

    /// A plain or a TLS stream behind one type.
    trait Wire: Read + Write {}
    impl<T: Read + Write> Wire for T {}

    /// A connection that is open or being opened. Dropping it closes the connection without
    /// waiting; no event is started afterwards.
    pub(crate) struct Socket {
        /// Set by the drop. The thread reads it before every event and at every tick.
        closed: Arc<AtomicBool>,
        /// Set by the thread while the connection is open.
        is_open: Arc<AtomicBool>,
        /// The texts the thread sends at its next tick.
        outgoing: Sender<String>,
    }

    impl Socket {
        /// Starts connecting to `url` (`ws://` or `wss://`), offering `protocols` as subprotocols.
        ///
        /// Every outcome arrives through `on_event`, also a URL that cannot be opened at all.
        /// `on_event` is never called from inside this function, from [`Socket::send`] or from the
        /// drop. Natively it runs on the socket's own thread; in the browser as a task of the page.
        pub(crate) fn open(
            url: &str,
            protocols: &[String],
            on_event: impl FnMut(SocketEvent) + Send + 'static,
        ) -> Self {
            let closed = Arc::new(AtomicBool::new(false));
            let is_open = Arc::new(AtomicBool::new(false));
            let (outgoing, queue) = mpsc::channel();
            let (url, protocols) = (url.to_owned(), protocols.to_vec());
            let flags = (closed.clone(), is_open.clone());
            std::thread::spawn(move || run(&url, &protocols, &flags.0, &flags.1, &queue, on_event));
            Self {
                closed,
                is_open,
                outgoing,
            }
        }

        /// Sends `text` as one text message. Only an open socket sends: before `Open` and after
        /// `Closed` the text is dropped.
        pub(crate) fn send(&self, text: &str) {
            if self.is_open.load(Ordering::Acquire) {
                // The thread is gone when this fails: the text is lost with the connection.
                let _ = self.outgoing.send(text.to_owned());
            }
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::Release);
        }
    }

    /// The life of one connection, on its own thread.
    fn run(
        url: &str,
        protocols: &[String],
        closed: &AtomicBool,
        is_open: &AtomicBool,
        queue: &Receiver<String>,
        mut on_event: impl FnMut(SocketEvent),
    ) {
        let ended = match connect(url, protocols, closed) {
            Ok(mut socket) => {
                is_open.store(true, Ordering::Release);
                let ended = serve(&mut socket, closed, queue, &mut on_event);
                is_open.store(false, Ordering::Release);
                if ended.is_none() {
                    // The socket was dropped: take leave of the server and report nothing.
                    let _ = socket.close(None);
                    let _ = socket.flush();
                }
                ended
            }
            Err(reason) => Some(reason),
        };
        // The flag is read before every event, so none starts after the drop.
        if let Some(reason) = ended
            && !closed.load(Ordering::Acquire)
        {
            // A message of the client library can repeat the address, and a key with it.
            let reason = reason.replace(url, endpoint(url));
            on_event(SocketEvent::Closed { reason });
        }
    }

    /// Opens the connection: the request, the TCP connection, TLS for `wss`, the handshake and
    /// the idle tick. An error is the reason the socket reports.
    fn connect(
        url: &str,
        protocols: &[String],
        closed: &AtomicBool,
    ) -> Result<WebSocket<Box<dyn Wire>>, String> {
        // Printed, an address without a path gets the `/` that a browser sends. The library
        // sends the target as it stands in the address: `ws://host?key=..` would ask for `?key=..`.
        let uri = url.parse::<Uri>().map_err(unusable)?.to_string();
        let uri = uri.parse::<Uri>().map_err(unusable)?;
        let tls = match uri.scheme_str() {
            Some("ws") => false,
            Some("wss") => true,
            _ => return Err(unusable("it starts with neither ws:// nor wss://")),
        };
        let request = protocols
            .iter()
            .fold(
                ClientRequestBuilder::new(uri.clone()),
                |request, protocol| request.with_sub_protocol(protocol),
            )
            .into_client_request()
            .map_err(unusable)?;
        // An IPv6 address stands in brackets in a URL and without them everywhere else.
        let host = uri.host().unwrap_or_default();
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });

        let tcp = dial(host, port, closed).map_err(cannot)?;
        if closed.load(Ordering::Acquire) {
            // Dropped while connecting: nothing more is sent, and this reason is never reported.
            return Err(String::new());
        }
        let tick = tcp.try_clone().map_err(cannot)?;
        let stream: Box<dyn Wire> = if tls {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            // The provider is named here so that the demo does not depend on a process-wide
            // default.
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(failed)?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let name = rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(failed)?;
            let connection =
                rustls::ClientConnection::new(Arc::new(config), name).map_err(failed)?;
            Box::new(rustls::StreamOwned::new(connection, tcp))
        } else {
            Box::new(tcp)
        };
        let unanswered = || "no answer to the handshake within 10 seconds".to_owned();
        let (socket, _) = tungstenite::client(request, stream).map_err(|error| match error {
            HandshakeError::Failure(Error::Http(response)) => match response.status().as_u16() {
                // The words of a refused request, so that a wrong key reads the same everywhere.
                status @ (401 | 403) => Failure::Status(status).to_string(),
                status => format!("handshake refused (HTTP {status})"),
            },
            // A read that timed out interrupts the handshake on Unix and fails it on Windows.
            HandshakeError::Interrupted(_) => unanswered(),
            HandshakeError::Failure(Error::Io(error)) if idle(&error) => unanswered(),
            HandshakeError::Failure(error) => failed(error),
        })?;
        // From here a read that times out is the idle tick of the loop. That is safe: the frame
        // reader keeps a partly read frame for the next call, and so does the TLS layer.
        tick.set_read_timeout(Some(TICK)).map_err(lost)?;
        Ok(socket)
    }

    /// The first address of `host` that answers, with the timeouts that bound the handshake.
    fn dial(host: &str, port: u16, closed: &AtomicBool) -> io::Result<TcpStream> {
        let mut tcp = Err(io::Error::other("the host has no address"));
        for address in (host, port).to_socket_addrs()? {
            if closed.load(Ordering::Acquire) {
                break;
            }
            tcp = TcpStream::connect_timeout(&address, PATIENCE);
            if tcp.is_ok() {
                break;
            }
        }
        let tcp = tcp?;
        tcp.set_nodelay(true)?;
        tcp.set_read_timeout(Some(PATIENCE))?;
        tcp.set_write_timeout(Some(PATIENCE))?;
        Ok(tcp)
    }

    /// Reports the open and every message and sends what is queued, until the connection ends,
    /// which gives the reason, or the socket is dropped, which gives `None`.
    fn serve(
        socket: &mut WebSocket<Box<dyn Wire>>,
        closed: &AtomicBool,
        queue: &Receiver<String>,
        on_event: &mut impl FnMut(SocketEvent),
    ) -> Option<String> {
        if !closed.load(Ordering::Acquire) {
            on_event(SocketEvent::Open);
        }
        while !closed.load(Ordering::Acquire) {
            for text in queue.try_iter() {
                if let Err(error) = socket.send(Message::text(text)) {
                    return Some(lost(error));
                }
            }
            let event = match socket.read() {
                Ok(Message::Text(text)) => SocketEvent::Text(text.as_str().to_owned()),
                Ok(Message::Binary(bytes)) => match std::str::from_utf8(&bytes) {
                    Ok(text) => SocketEvent::Text(text.to_owned()),
                    Err(_) => SocketEvent::Binary(bytes.len()),
                },
                Ok(Message::Close(frame)) => {
                    // The library queued the reply to the close frame; this sends it.
                    let _ = socket.flush();
                    // A close frame without a code counts as 1005, as a browser reports it.
                    return Some(frame.map_or_else(
                        || closed_by_server(1005, ""),
                        |frame| closed_by_server(frame.code.into(), &frame.reason),
                    ));
                }
                // The library answers a ping itself on this or the next call.
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                // The idle tick: nothing arrived for `TICK`.
                Err(Error::Io(error)) if idle(&error) => continue,
                Err(error) => return Some(lost(error)),
            };
            if !closed.load(Ordering::Acquire) {
                on_event(event);
            }
        }
        None
    }

    /// Whether a read or a write ended because its timeout passed: `WouldBlock` on Unix,
    /// `TimedOut` on Windows.
    fn idle(error: &io::Error) -> bool {
        matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        )
    }

    // The reasons of a socket that ends: sentence parts that follow the name of the endpoint.

    fn unusable(error: impl Display) -> String {
        format!("not a usable WebSocket address ({error})")
    }

    fn cannot(error: impl Display) -> String {
        format!("cannot connect ({error})")
    }

    fn failed(error: impl Display) -> String {
        format!("handshake failed ({error})")
    }

    fn lost(error: impl Display) -> String {
        format!("connection lost ({error})")
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast as _, JsValue};
    use web_sys::js_sys;

    use super::super::{after, endpoint};
    use super::{SocketEvent, closed_by_server};

    /// A socket of the page and the handlers it calls, which live as long as the row.
    struct Entry {
        socket: web_sys::WebSocket,
        _on_open: Closure<dyn FnMut()>,
        _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        _on_close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    }

    thread_local! {
        /// The page's sockets and their handlers by number. They are not `Send`, so they stay
        /// here and `Socket` holds the number: the session and the feed are then the same code
        /// and the same `Send` types on both targets.
        static SOCKETS: RefCell<HashMap<u64, Entry>> = RefCell::default();
    }

    /// The number of the next row.
    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// A connection that is open or being opened. Dropping it closes the connection without
    /// waiting; no event is started afterwards.
    pub(crate) struct Socket {
        /// The row of `SOCKETS` that holds the page's socket, if the page could create one.
        number: u64,
        /// Set by the drop, for the one event that no handler of the page's socket reports.
        closed: Arc<AtomicBool>,
    }

    impl Socket {
        /// Starts connecting to `url` (`ws://` or `wss://`), offering `protocols` as subprotocols.
        ///
        /// Every outcome arrives through `on_event`, also a URL that cannot be opened at all.
        /// `on_event` is never called from inside this function, from [`Socket::send`] or from the
        /// drop. Natively it runs on the socket's own thread; in the browser as a task of the page.
        pub(crate) fn open(
            url: &str,
            protocols: &[String],
            mut on_event: impl FnMut(SocketEvent) + Send + 'static,
        ) -> Self {
            let number = NEXT.fetch_add(1, Ordering::Relaxed);
            let closed = Arc::new(AtomicBool::new(false));
            let created = if protocols.is_empty() {
                web_sys::WebSocket::new(url)
            } else {
                let offered: js_sys::Array = protocols.iter().map(JsValue::from).collect();
                web_sys::WebSocket::new_with_str_sequence(url, &offered)
            };
            let socket = match created {
                Ok(socket) => socket,
                Err(error) => {
                    // The page may not open this address, or a subprotocol is not a token.
                    let message = error
                        .dyn_ref::<js_sys::Error>()
                        .map_or_else(|| format!("{error:?}"), |error| error.message().into());
                    let reason = format!("not a usable WebSocket address ({message})")
                        .replace(url, endpoint(url));
                    // In a later task: `on_event` is never called from inside `open`.
                    after(Duration::ZERO, closed.clone(), move || {
                        on_event(SocketEvent::Closed { reason });
                    });
                    return Self { number, closed };
                }
            };
            socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

            // No error handler: a browser fires a close event after every error, and the error
            // event carries no cause.
            let on_event = Rc::new(RefCell::new(on_event));
            let opened = Rc::new(Cell::new(false));
            let (report, flag) = (on_event.clone(), opened.clone());
            let on_open = Closure::<dyn FnMut()>::new(move || {
                flag.set(true);
                (*report.borrow_mut())(SocketEvent::Open);
            });
            let report = on_event.clone();
            let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |message: web_sys::MessageEvent| {
                    let data = message.data();
                    let event = match data.as_string() {
                        Some(text) => SocketEvent::Text(text),
                        None => match String::from_utf8(js_sys::Uint8Array::new(&data).to_vec()) {
                            Ok(text) => SocketEvent::Text(text),
                            Err(error) => SocketEvent::Binary(error.as_bytes().len()),
                        },
                    };
                    (*report.borrow_mut())(event);
                },
            );
            let address = url.to_owned();
            let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(
                move |close: web_sys::CloseEvent| {
                    let reason = match close.code() {
                        1006 if opened.get() => "connection lost".to_owned(),
                        1006 => "could not connect. The browser gives no cause: check the \
                                 address, that the server accepts this page, and the key if it \
                                 travels in the address or as a subprotocol"
                            .to_owned(),
                        code => closed_by_server(code, &close.reason()),
                    };
                    // A text of the server can repeat the address, and a key with it.
                    let reason = reason.replace(&address, endpoint(&address));
                    (*on_event.borrow_mut())(SocketEvent::Closed { reason });
                },
            );
            socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
            let entry = Entry {
                socket,
                _on_open: on_open,
                _on_message: on_message,
                _on_close: on_close,
            };
            SOCKETS.with_borrow_mut(|sockets| sockets.insert(number, entry));
            Self { number, closed }
        }

        /// Sends `text` as one text message. Only an open socket sends: before `Open` and after
        /// `Closed` the text is dropped.
        pub(crate) fn send(&self, text: &str) {
            SOCKETS.with_borrow(|sockets| {
                if let Some(entry) = sockets.get(&self.number)
                    && entry.socket.ready_state() == web_sys::WebSocket::OPEN
                {
                    // A send that fails is a text lost with the connection.
                    let _ = entry.socket.send_with_str(text);
                }
            });
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::Release);
            let number = self.number;
            SOCKETS.with_borrow(|sockets| {
                if let Some(entry) = sockets.get(&number) {
                    entry.socket.set_onopen(None);
                    entry.socket.set_onmessage(None);
                    entry.socket.set_onclose(None);
                    let _ = entry.socket.close();
                }
            });
            // The row goes in a later task: this drop can run inside one of the row's own
            // handlers, and a handler must not be freed while it runs.
            after(Duration::ZERO, Arc::default(), move || {
                SOCKETS.with_borrow_mut(|sockets| sockets.remove(&number));
            });
        }
    }
}
