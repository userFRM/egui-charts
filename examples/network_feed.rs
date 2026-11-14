//! # Network feed
//!
//! Uses `data::http::HttpDatafeed` with a local server of generated column history and live
//! bars. The server fails every fourth history request to demonstrate recoverable errors.
//!
//! Run natively with `cargo run --features http --example network_feed`.
//! For the browser, run the server with `cargo run --features http --example network_feed --
//! --serve`, then `trunk serve --example network_feed --features http --dist
//! target/network_feed examples/network_feed.html` and open <http://127.0.0.1:8080>.
//! Point `HttpDatafeed::new` at your own server using the builder guide's endpoint contract.

#[cfg(not(feature = "http"))]
fn main() {
    eprintln!("Run this example with --features http");
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
fn main() -> eframe::Result {
    app::run()
}

#[cfg(all(feature = "http", target_arch = "wasm32"))]
fn main() {
    app::run();
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
mod support;

#[cfg(feature = "http")]
mod app {
    #[cfg(not(target_arch = "wasm32"))]
    use super::support;

    use egui_charts::ChartBuilder;
    use egui_charts::data::http::HttpDatafeed;
    use egui_charts::model::Timeframe;
    use egui_charts::theme::Theme;
    use egui_charts::widget::Chart;
    use std::sync::Arc;

    /// The one symbol the backend serves.
    const SYMBOL: &str = "SAMPLE";

    /// Where `--serve` listens and where the browser build looks for the backend.
    const SERVE_ADDRESS: &str = "127.0.0.1:8090";

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn run() -> eframe::Result {
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
    pub(super) fn run() {
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
                .with_feed(Arc::new(HttpDatafeed::new(base)))
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

    /// The backend: generated sample data for `SYMBOL` over plain HTTP, a thread per connection.
    #[cfg(not(target_arch = "wasm32"))]
    mod server {
        use super::{SYMBOL, support};
        use chrono::{DateTime, DurationRound, TimeDelta, Utc};
        use egui_charts::model::Bar;
        use rand::RngExt;
        use serde_json::json;
        use std::io::{self, BufRead, BufReader, Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        /// The most bars one history answer carries; fewer than asked is fine for the chart.
        const MAX_PAGE: usize = 5_000;

        /// Serves `/config`, `/symbols`, `/search` and `/history` for `SYMBOL` on `listener` until the process
        /// ends.
        pub fn serve(listener: TcpListener) {
            let now = Utc::now().duration_trunc(TimeDelta::minutes(1)).unwrap();
            let bars = Arc::new(Mutex::new(support::sample_bars(
                20_000,
                100.0,
                TimeDelta::minutes(1),
                now,
            )));
            std::thread::spawn({
                let bars = bars.clone();
                move || loop {
                    {
                        let mut bars = bars.lock().unwrap();
                        let time = Utc::now().duration_trunc(TimeDelta::minutes(1)).unwrap();
                        let last = bars.last_mut().unwrap();
                        let price = (last.close
                            * (1.0 + rand::rng().random_range(-0.0005..0.0005))
                            * 100.0)
                            .round()
                            / 100.0;
                        if last.time == time {
                            last.high = last.high.max(price);
                            last.low = last.low.min(price);
                            last.close = price;
                            last.volume += 1.0;
                        } else {
                            let open = last.close;
                            bars.push(Bar::new(
                                time,
                                open,
                                open.max(price),
                                open.min(price),
                                price,
                                1.0,
                            ));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            let requests = Arc::new(AtomicUsize::new(0));
            for connection in listener.incoming().flatten() {
                let (bars, requests) = (bars.clone(), requests.clone());
                std::thread::spawn(move || {
                    // A failed connection only ends its own exchange.
                    let _ = handle(connection, &bars, &requests);
                });
            }
        }

        fn handle(
            connection: TcpStream,
            bars: &Mutex<Vec<Bar>>,
            requests: &AtomicUsize,
        ) -> io::Result<()> {
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
            let symbol = json!({
                "name": SYMBOL, "description": "Sample data", "exchange": "DEMO",
                "type": "stock", "minmov": 1, "pricescale": 100, "volume_precision": 0,
                "timezone": "UTC", "session": "24x7", "supported_resolutions": ["1"],
                "data_status": "streaming"
            });
            let body = match path {
                "/config" => json!({
                    "supported_resolutions": ["1"], "exchanges": [{"value": "DEMO"}],
                    "symbols_types": [{"value": "stock"}]
                }),
                "/symbols" if param(query, "symbol") == Some(SYMBOL) => symbol,
                "/search" => {
                    let matches = param(query, "query").is_none_or(|text| SYMBOL.contains(text));
                    if matches { json!([symbol]) } else { json!([]) }
                }
                "/history" if param(query, "symbol") == Some(SYMBOL) => {
                    return history(&connection, query, bars, requests);
                }
                _ => return respond(&connection, "404 Not Found", "{}"),
            };
            respond(&connection, "200 OK", &body.to_string())
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
            bars: &Mutex<Vec<Bar>>,
            requests: &AtomicUsize,
        ) -> io::Result<()> {
            if requests.fetch_add(1, Ordering::Relaxed) % 4 == 3 {
                eprintln!("history request failed on purpose; the chart retries");
                return respond(connection, "503 Service Unavailable", "{}");
            }
            let time = |name| match param(query, name) {
                None => Some(None),
                Some(seconds) => seconds
                    .parse::<f64>()
                    .ok()
                    .filter(|seconds| seconds.is_finite())
                    .and_then(|seconds| DateTime::from_timestamp_millis((seconds * 1_000.0) as i64))
                    .map(Some),
            };
            let count = param(query, "countback").and_then(|count| count.parse::<usize>().ok());
            let (Some(count), Some(before), Some(since)) = (count, time("to"), time("from")) else {
                return respond(connection, "400 Bad Request", "{}");
            };
            if param(query, "resolution") != Some("1") {
                return respond(connection, "400 Bad Request", "{}");
            }
            let page = {
                let bars = bars.lock().unwrap();
                let (bars, end_of_history) = page(&bars, before, since, count.min(MAX_PAGE));
                json!({
                    "s": if bars.is_empty() { "no_data" } else { "ok" },
                    "t": bars.iter().map(|bar| bar.time.timestamp()).collect::<Vec<_>>(),
                    "o": bars.iter().map(|bar| bar.open).collect::<Vec<_>>(),
                    "h": bars.iter().map(|bar| bar.high).collect::<Vec<_>>(),
                    "l": bars.iter().map(|bar| bar.low).collect::<Vec<_>>(),
                    "c": bars.iter().map(|bar| bar.close).collect::<Vec<_>>(),
                    "v": bars.iter().map(|bar| bar.volume).collect::<Vec<_>>(),
                    "end_of_history": end_of_history
                })
            };
            respond(connection, "200 OK", &page.to_string())
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
    }
    #[cfg(test)]
    mod tests {
        use super::server;
        use chrono::{DateTime, TimeDelta, TimeZone, Utc};
        use egui_charts::model::Bar;

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
    }
}
