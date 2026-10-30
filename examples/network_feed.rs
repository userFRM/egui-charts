//! # Network feed
//!
//! A `Datafeed` over HTTP. `resolve` and `history` fetch JSON from a backend and answer their
//! `Reply` when the response arrives, after the call has returned; live trades arrive as
//! server-sent events and go into the subscription's `Sink`. HTTP failures become `FeedError`
//! values, and the chart retries the recoverable ones.
//!
//! The example starts its own backend: a small server of generated sample data on a local
//! port. It fails every fourth history request on purpose, so scrolling back shows the chart
//! retrying.
//!
//! Run natively:
//! ```sh
//! cargo run --example network_feed
//! ```
//!
//! Run in the browser, with the backend on port 8090 in one terminal and Trunk in another, then
//! open <http://127.0.0.1:8080>:
//! ```sh
//! cargo run --example network_feed -- --serve
//! trunk serve --example network_feed --dist target/network_feed examples/network_feed.html
//! ```
//!
//! To connect your backend, keep `NetworkFeed` and replace the URLs, the JSON types and the two
//! `http_get` functions with your HTTP client.

#[cfg(not(target_arch = "wasm32"))]
mod support; // uses rand, a native-only dev-dependency

use egui_charts::ChartBuilder;
use egui_charts::data::{
    Datafeed, FeedError, HistoryPage, HistoryRequest, Reply, Sink, Subscription,
};
use egui_charts::model::{Bar, Instrument, Tick, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::widget::Chart;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use client::{get, stream};

/// The one symbol the backend serves.
const SYMBOL: &str = "SAMPLE";

/// Where `--serve` listens and where the browser build looks for the backend.
const SERVE_ADDRESS: &str = "127.0.0.1:8090";

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use std::net::TcpListener;

    if std::env::args().any(|arg| arg == "--serve") {
        let listener = TcpListener::bind(SERVE_ADDRESS)
            .unwrap_or_else(|error| panic!("cannot listen on {SERVE_ADDRESS}: {error}"));
        println!("serving sample data on http://{SERVE_ADDRESS}");
        server::serve(listener);
        return Ok(());
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free local port");
    let base = format!("http://{}", listener.local_addr().expect("a bound address"));
    std::thread::spawn(move || server::serve(listener));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "egui-charts: network feed",
        options,
        Box::new(|_cc| Ok(Box::new(NetworkFeedApp::new(base)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    let canvas: eframe::web_sys::HtmlCanvasElement = eframe::web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
        .get_element_by_id("chart")
        .expect("the page's canvas")
        .unchecked_into();
    wasm_bindgen_futures::spawn_local(async move {
        eframe::WebRunner::new()
            .start(
                canvas,
                Default::default(),
                Box::new(|_cc| {
                    Ok(Box::new(NetworkFeedApp::new(format!(
                        "http://{SERVE_ADDRESS}"
                    ))))
                }),
            )
            .await
            .expect("the chart starts");
    });
}

struct NetworkFeedApp {
    chart: Chart,
}

impl NetworkFeedApp {
    fn new(base: String) -> Self {
        let chart = ChartBuilder::new()
            .with_symbol(SYMBOL)
            .with_timeframe(Timeframe::Min1)
            .with_feed(Arc::new(NetworkFeed { base }))
            .with_initial_bars(500)
            .build();
        Self { chart }
    }
}

impl eframe::App for NetworkFeedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui_charts::theme::apply_to_egui(ui.ctx(), &Theme::dark());
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        egui::CentralPanel::default().show(ui, |ui| {
            // Asks the feed for what the chart needs and applies what it sent.
            self.chart.show(ui);
        });
    }
}

/// The answer of `/resolve`.
#[derive(Serialize, Deserialize)]
struct SymbolInfo {
    symbol: String,
    description: String,
    tick_size: f64,
}

/// The answer of `/history`.
#[derive(Serialize, Deserialize)]
struct Page {
    bars: Vec<Bar>,
    end_of_history: bool,
}

/// The parts of an HTTP response the feed reads.
struct Response {
    status: u16,
    retry_after: Option<Duration>,
    body: String,
}

/// A `Datafeed` that asks the backend at `base`, such as `http://127.0.0.1:8090`.
struct NetworkFeed {
    base: String,
}

// shortcut: symbols go into the URLs unescaped, enough for `SAMPLE`; escape them for a real
// backend.
impl Datafeed for NetworkFeed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        let url = format!("{}/resolve?symbol={symbol}", self.base);
        get(url, symbol.to_owned(), reply, |info: SymbolInfo| {
            Instrument::new(info.symbol)
                .with_description(info.description)
                .with_tick_size(info.tick_size)
        });
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        // The sample backend keeps one-minute bars.
        if request.timeframe != Timeframe::Min1 {
            return reply.send(Err(FeedError::NotSupported));
        }
        let mut url = format!(
            "{}/history?symbol={}&count={}",
            self.base, request.symbol, request.count
        );
        if let Some(before) = request.before {
            url += &format!("&before={}", before.timestamp_millis());
        }
        if let Some(since) = request.since {
            url += &format!("&since={}", since.timestamp_millis());
        }
        get(url, request.symbol, reply, |page: Page| {
            HistoryPage::new(page.bars, page.end_of_history)
        });
    }

    fn subscribe(&self, subscription: Subscription) {
        let url = format!("{}/stream?symbol={}", self.base, subscription.symbol);
        stream(url, subscription.symbol, subscription.sink);
    }
}

/// The error an HTTP status reports, `None` for success. A server error is transient, so it
/// maps to a recoverable `Network` error that the chart retries.
fn status_error(symbol: &str, status: u16, retry_after: Option<Duration>) -> Option<FeedError> {
    match status {
        200..=299 => None,
        404 => Some(FeedError::SymbolNotFound(symbol.to_owned())),
        429 => Some(FeedError::RateLimited { retry_after }),
        500..=599 => Some(FeedError::Network(format!("HTTP {status}"))),
        _ => Some(FeedError::InvalidData(format!("HTTP {status}"))),
    }
}

/// The decoded body of a successful response, or the error it reports.
fn answer<T: DeserializeOwned>(symbol: &str, response: Response) -> Result<T, FeedError> {
    match status_error(symbol, response.status, response.retry_after) {
        Some(error) => Err(error),
        None => serde_json::from_str(&response.body)
            .map_err(|error| FeedError::InvalidData(error.to_string())),
    }
}

/// A `Retry-After` value in seconds. shortcut: the HTTP date form reads as none, and the chart
/// then backs off on its own.
fn retry_after(value: &str) -> Option<Duration> {
    value.trim().parse().ok().map(Duration::from_secs)
}

/// Pushes the trade of one event, or reports what could not be read.
fn push(sink: &Sink, data: &str) {
    match serde_json::from_str::<Tick>(data) {
        Ok(tick) => sink.push_tick(tick),
        Err(error) => sink.report_error(FeedError::InvalidData(error.to_string())),
    }
}

/// HTTP over `std::net`, each request on its own thread.
#[cfg(not(target_arch = "wasm32"))]
mod client {
    use super::{Response, push, retry_after, status_error};
    use egui_charts::data::{FeedError, FeedStatus, Reply, Sink};
    use serde::de::DeserializeOwned;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    /// How long a connection or a read may stall before it counts as a network failure.
    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Answers `reply` from a GET of `url`, decoded as `W` and converted by `into`, off the UI
    /// thread.
    pub fn get<W: DeserializeOwned, T: Send + 'static>(
        url: String,
        symbol: String,
        reply: Reply<T>,
        into: impl FnOnce(W) -> T + Send + 'static,
    ) {
        // shortcut: a thread per request; with an async runtime, spawn a task on it instead.
        std::thread::spawn(move || {
            reply.send(
                http_get(&url)
                    .and_then(|response| super::answer(&symbol, response))
                    .map(into),
            );
        });
    }

    /// Streams trades into `sink` until the subscription ends, reconnecting after a second when
    /// the connection fails and giving up on an error retrying cannot fix.
    pub fn stream(url: String, symbol: String, sink: Sink) {
        std::thread::spawn(move || {
            while sink.is_active() {
                sink.set_status(FeedStatus::Connecting);
                let result = read_events(&url, &symbol, &sink);
                sink.set_status(FeedStatus::Disconnected);
                if let Err(error) = result {
                    let fatal = !error.is_recoverable();
                    sink.report_error(error);
                    if fatal {
                        break;
                    }
                }
                // shortcut: a fixed reconnect delay; back off for a real backend.
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }

    /// Pushes the trades of one connection until it ends or the chart ends the subscription.
    fn read_events(url: &str, symbol: &str, sink: &Sink) -> Result<(), FeedError> {
        let network = |error: io::Error| FeedError::Network(error.to_string());
        let mut reader = request(url).map_err(network)?;
        let (status, retry_after) = read_head(&mut reader).map_err(network)?;
        if let Some(error) = status_error(symbol, status, retry_after) {
            return Err(error);
        }
        sink.set_status(FeedStatus::Live);
        for line in reader.lines() {
            // Checked before each line, so the producer stops within one event of the end.
            if !sink.is_active() {
                return Ok(());
            }
            if let Some(data) = line.map_err(network)?.strip_prefix("data: ") {
                push(sink, data);
            }
        }
        Err(FeedError::Network("the event stream ended".into()))
    }

    /// The response to a GET of `url`. Every transport failure is a `Network` error, which the
    /// chart retries.
    fn http_get(url: &str) -> Result<Response, FeedError> {
        let read = || -> io::Result<Response> {
            let mut reader = request(url)?;
            let (status, retry_after) = read_head(&mut reader)?;
            let mut body = String::new();
            reader.read_to_string(&mut body)?;
            Ok(Response {
                status,
                retry_after,
                body,
            })
        };
        read().map_err(|error| FeedError::Network(error.to_string()))
    }

    /// Connects to the host of an `http://host:port/path?query` URL and sends a GET for the rest.
    /// shortcut: plain HTTP/1.1 without TLS, chunked bodies or redirects, enough for the local
    /// backend; use your HTTP client for a real one.
    fn request(url: &str) -> io::Result<BufReader<TcpStream>> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidInput, format!("bad URL {url}"));
        let (host, path) = url
            .strip_prefix("http://")
            .and_then(|rest| rest.split_once('/'))
            .ok_or_else(invalid)?;
        let address = host.to_socket_addrs()?.next().ok_or_else(invalid)?;
        let mut connection = TcpStream::connect_timeout(&address, TIMEOUT)?;
        connection.set_read_timeout(Some(TIMEOUT))?;
        write!(
            connection,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )?;
        Ok(BufReader::new(connection))
    }

    /// Reads the status line and the headers, returning the status and `Retry-After`.
    fn read_head(reader: &mut impl BufRead) -> io::Result<(u16, Option<Duration>)> {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let status = line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("bad status {line:?}"))
            })?;
        let mut delay = None;
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                return Ok((status, delay));
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("retry-after")
            {
                delay = retry_after(value);
            }
        }
    }
}

/// HTTP with the browser's `fetch` and `EventSource`, each request in a browser task.
#[cfg(target_arch = "wasm32")]
mod client {
    use super::{Response, push, retry_after};
    use eframe::wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
    use egui_charts::data::{FeedError, FeedStatus, Reply, Sink};
    use serde::de::DeserializeOwned;
    use wasm_bindgen_futures::JsFuture;

    /// Answers `reply` from a GET of `url`, decoded as `W` and converted by `into`.
    pub fn get<W: DeserializeOwned, T: Send + 'static>(
        url: String,
        symbol: String,
        reply: Reply<T>,
        into: impl FnOnce(W) -> T + Send + 'static,
    ) {
        wasm_bindgen_futures::spawn_local(async move {
            reply.send(
                http_get(&url)
                    .await
                    .and_then(|response| super::answer(&symbol, response))
                    .map(into),
            );
        });
    }

    /// Streams trades into `sink` until the subscription ends. The browser reconnects a dropped
    /// connection by itself.
    pub fn stream(url: String, _symbol: String, sink: Sink) {
        let source = match web_sys::EventSource::new(&url) {
            Ok(source) => source,
            Err(error) => return sink.report_error(FeedError::Network(format!("{error:?}"))),
        };
        sink.set_status(FeedStatus::Connecting);
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new({
            let (sink, source) = (sink.clone(), source.clone());
            move |event: web_sys::MessageEvent| {
                if !sink.is_active() {
                    return source.close();
                }
                sink.set_status(FeedStatus::Live);
                push(&sink, &event.data().as_string().unwrap_or_default());
            }
        });
        let on_error = Closure::<dyn FnMut()>::new({
            let source = source.clone();
            move || {
                if !sink.is_active() {
                    return source.close();
                }
                sink.set_status(FeedStatus::Disconnected);
            }
        });
        source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        // shortcut: the handlers stay allocated after the source closes, a few bytes per symbol
        // change.
        on_message.forget();
        on_error.forget();
    }

    /// The response to a GET of `url`. A failed request is a `Network` error, which the chart
    /// retries. shortcut: fetch has no timeout here; add an `AbortController` for a real backend.
    async fn http_get(url: &str) -> Result<Response, FeedError> {
        let network = |error: JsValue| FeedError::Network(format!("{error:?}"));
        let window =
            web_sys::window().ok_or_else(|| FeedError::Other("no browser window".into()))?;
        let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(network)?
            .unchecked_into();
        let body = JsFuture::from(response.text().map_err(network)?)
            .await
            .map_err(network)?;
        Ok(Response {
            status: response.status(),
            retry_after: response
                .headers()
                .get("Retry-After")
                .ok()
                .flatten()
                .as_deref()
                .and_then(retry_after),
            body: body.as_string().unwrap_or_default(),
        })
    }
}

/// The backend: generated sample data for `SYMBOL` over plain HTTP, a thread per connection.
#[cfg(not(target_arch = "wasm32"))]
mod server {
    use super::{Page, SYMBOL, SymbolInfo, support};
    use chrono::{DateTime, DurationRound, TimeDelta, Utc};
    use egui_charts::model::{Bar, OrderSide, Tick};
    use rand::RngExt;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// The most bars one history answer carries; fewer than asked is fine for the chart.
    const MAX_PAGE: usize = 5_000;

    /// Serves `/resolve`, `/history` and `/stream` for `SYMBOL` on `listener` until the process
    /// ends.
    pub fn serve(listener: TcpListener) {
        let now = Utc::now().duration_trunc(TimeDelta::minutes(1)).unwrap();
        let bars: Arc<[Bar]> =
            support::sample_bars(20_000, 100.0, TimeDelta::minutes(1), now).into();
        let requests = Arc::new(AtomicUsize::new(0));
        for connection in listener.incoming().flatten() {
            let (bars, requests) = (bars.clone(), requests.clone());
            std::thread::spawn(move || {
                // A failed connection only ends its own exchange.
                let _ = handle(connection, &bars, &requests);
            });
        }
    }

    fn handle(connection: TcpStream, bars: &[Bar], requests: &AtomicUsize) -> io::Result<()> {
        // Trust boundary: the head is read with a bounded buffer. All of it is read, because
        // closing a socket with unread request bytes can reset the response before it is read.
        let mut reader = BufReader::new((&connection).take(8 * 1024));
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let mut header = String::new();
        while reader.read_line(&mut header)? > 2 {
            header.clear();
        }

        let target = request_line.split_whitespace().nth(1).unwrap_or_default();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if param(query, "symbol") != Some(SYMBOL) {
            return respond(&connection, "404 Not Found", "{}");
        }
        match path {
            "/resolve" => {
                let info = SymbolInfo {
                    symbol: SYMBOL.to_owned(),
                    description: "Sample data".to_owned(),
                    tick_size: 0.01,
                };
                respond(&connection, "200 OK", &serde_json::to_string(&info)?)
            }
            "/history" => history(&connection, query, bars, requests),
            "/stream" => events(&connection, bars.last().map_or(100.0, |bar| bar.close)),
            _ => respond(&connection, "404 Not Found", "{}"),
        }
    }

    /// The value of `name` in a query string. shortcut: no percent-decoding, enough for the
    /// sample symbol and numbers.
    fn param<'q>(query: &'q str, name: &str) -> Option<&'q str> {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find_map(|(key, value)| (key == name).then_some(value))
    }

    fn history(
        connection: &TcpStream,
        query: &str,
        bars: &[Bar],
        requests: &AtomicUsize,
    ) -> io::Result<()> {
        if requests.fetch_add(1, Ordering::Relaxed) % 4 == 3 {
            eprintln!("history request failed on purpose; the chart retries");
            return respond(connection, "503 Service Unavailable", "{}");
        }
        let time = |name| match param(query, name) {
            None => Some(None),
            Some(millis) => millis
                .parse()
                .ok()
                .and_then(DateTime::from_timestamp_millis)
                .map(Some),
        };
        let count = param(query, "count").and_then(|count| count.parse::<usize>().ok());
        let (Some(count), Some(before), Some(since)) = (count, time("before"), time("since"))
        else {
            return respond(connection, "400 Bad Request", "{}");
        };
        let (bars, end_of_history) = page(bars, before, since, count.min(MAX_PAGE));
        let page = Page {
            bars: bars.to_vec(),
            end_of_history,
        };
        respond(connection, "200 OK", &serde_json::to_string(&page)?)
    }

    /// The newest `count` bars of `[since, before)` and whether none older remain, as
    /// `Datafeed::history` asks.
    pub(super) fn page(
        bars: &[Bar],
        before: Option<DateTime<Utc>>,
        since: Option<DateTime<Utc>>,
        count: usize,
    ) -> (&[Bar], bool) {
        let end = bars.partition_point(|bar| before.is_none_or(|before| bar.time < before));
        let first = bars
            .partition_point(|bar| since.is_some_and(|since| bar.time < since))
            .min(end);
        let start = end.saturating_sub(count).max(first);
        (&bars[start..end], start == 0)
    }

    fn respond(mut connection: &TcpStream, status: &str, body: &str) -> io::Result<()> {
        write!(
            connection,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// Server-sent events of random trades around `price`, four a second, until the client
    /// leaves.
    fn events(mut connection: &TcpStream, mut price: f64) -> io::Result<()> {
        connection.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\
              Access-Control-Allow-Origin: *\r\n\r\n",
        )?;
        let mut rng = rand::rng();
        loop {
            price *= 1.0 + rng.random_range(-0.0005..0.0005);
            let side = [OrderSide::Buy, OrderSide::Sell][rng.random_range(0..2)];
            let tick = Tick::new(
                Utc::now(),
                (price * 100.0).round() / 100.0,
                rng.random_range(0.1..5.0),
                Some(side),
            );
            write!(connection, "data: {}\n\n", serde_json::to_string(&tick)?)?;
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta, TimeZone, Utc};

    #[test]
    fn history_pages_honour_before_since_and_count() {
        let t = |minutes| Utc.timestamp_opt(0, 0).unwrap() + TimeDelta::minutes(minutes);
        let bars: Vec<_> = (0..5)
            .map(|m| Bar::new(t(m), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        let half_past = t(2) + TimeDelta::seconds(30);
        // (before, since, count) -> (bars, end_of_history), from the `Datafeed::history` contract.
        let cases: [(Option<DateTime<Utc>>, Option<DateTime<Utc>>, usize, _, bool); 7] = [
            (None, None, 2, 3..5, false),
            (Some(t(3)), None, 2, 1..3, false),
            (Some(t(2)), None, 5, 0..2, true),
            (Some(t(0)), None, 5, 0..0, true),
            (Some(half_past), None, 1, 2..3, false),
            (None, Some(t(3)), 5, 3..5, false),
            (Some(t(4)), Some(t(1)), 2, 2..4, false),
        ];
        for (before, since, count, range, end) in cases {
            assert_eq!(
                server::page(&bars, before, since, count),
                (&bars[range], end),
                "before {before:?}, since {since:?}, count {count}"
            );
        }
        assert_eq!(server::page(&[], None, None, 5), (&[][..], true));
    }

    #[test]
    fn http_failures_map_to_feed_errors() {
        let response = |status, retry_after, body: &str| Response {
            status,
            retry_after,
            body: body.to_owned(),
        };
        let end_of_history =
            |response| answer::<Page>(SYMBOL, response).map(|page| page.end_of_history);

        let body = r#"{"bars":[],"end_of_history":true}"#;
        assert_eq!(end_of_history(response(200, None, body)), Ok(true));
        assert!(matches!(
            end_of_history(response(200, None, "{")),
            Err(FeedError::InvalidData(_))
        ));
        let two_seconds = Some(Duration::from_secs(2));
        for (status, retry_after, error) in [
            (404, None, FeedError::SymbolNotFound(SYMBOL.to_owned())),
            (
                429,
                two_seconds,
                FeedError::RateLimited {
                    retry_after: two_seconds,
                },
            ),
            (503, None, FeedError::Network("HTTP 503".to_owned())),
            (400, None, FeedError::InvalidData("HTTP 400".to_owned())),
        ] {
            assert_eq!(
                end_of_history(response(status, retry_after, "")),
                Err(error),
                "HTTP {status}"
            );
        }

        assert_eq!(retry_after(" 2"), two_seconds);
        assert_eq!(retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }
}
