//! HTTP symbol metadata, column history and polling subscriptions (feature `http`).
//!
//! See the builder guide's HTTP endpoint contract. The application supplies the server and
//! its data. Requests run off the UI thread on native targets and through browser fetch on
//! wasm32; HTTPS, redirects and response framing are handled by the HTTP client.

mod protocol;
#[cfg(test)]
mod tests;

use super::{
    Datafeed, FeedCapabilities, FeedError, FeedStatus, HistoryPage, HistoryRequest, Reply,
    Subscription, SubscriptionId,
};
use crate::model::{Instrument, SymbolType};
use protocol::{Columns, Config, Symbol, history_url, url};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REFRESH: Duration = Duration::from_secs(5);

/// A reusable feed for a server exposing `/config`, `/symbols`, `/search` and `/history`.
///
/// Implements both [`Datafeed`] and [`SymbolSearch`](crate::services::SymbolSearch), so the same `Arc` can supply chart data
/// and the host's symbol search dialog. Subscriptions refresh bars every five seconds,
/// retry recoverable failures with backoff and stop on unsubscribe or chart drop. An HTTP
/// request already in flight may finish, but its answer is discarded after cancellation.
///
/// ```
/// # #[cfg(feature = "widget")] {
/// use egui_charts::{ChartBuilder, data::http::HttpDatafeed};
/// use std::sync::Arc;
/// let feed = Arc::new(HttpDatafeed::new("https://data.example.com"));
/// let chart = ChartBuilder::new().with_symbol("SAMPLE").with_feed(feed).build();
/// # }
/// ```
pub struct HttpDatafeed {
    base: String,
    subscriptions: Mutex<HashMap<SubscriptionId, Arc<AtomicBool>>>,
}

impl HttpDatafeed {
    /// Connects to the endpoint root at `address`, which may include a path prefix.
    /// A trailing slash is optional. Browser servers must allow the application's origin.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            base: address.into().trim_end_matches('/').to_owned(),
            subscriptions: Mutex::new(HashMap::new()),
        }
    }
}

impl Datafeed for HttpDatafeed {
    fn capabilities(&self, reply: Reply<FeedCapabilities>) {
        get(
            url(&self.base, "config", &[]),
            String::new(),
            move |result| {
                reply.send(result.and_then(Config::into_capabilities));
            },
        );
    }

    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        get(
            url(&self.base, "symbols", &[("symbol", symbol.to_owned())]),
            symbol.to_owned(),
            move |result| reply.send(result.and_then(Symbol::into_instrument)),
        );
    }

    fn resolve_converted(&self, symbol: &str, conversion: &str, reply: Reply<Instrument>) {
        get(
            url(
                &self.base,
                "symbols",
                &[
                    ("symbol", symbol.to_owned()),
                    ("conversion", conversion.to_owned()),
                ],
            ),
            symbol.to_owned(),
            move |result| reply.send(result.and_then(Symbol::into_instrument)),
        );
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let target = match history_url(&self.base, &request) {
            Ok(target) => target,
            Err(error) => return reply.send(Err(error)),
        };
        get(target, request.symbol.clone(), move |result| {
            reply.send(result.and_then(|columns: Columns| columns.into_page(&request)));
        });
    }

    fn subscribe(&self, subscription: Subscription) {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(old) = self
            .subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(subscription.id, cancelled.clone())
        {
            old.store(true, Ordering::Release);
        }
        Poll {
            base: self.base.clone(),
            subscription,
            cancelled,
            since: None,
            retry: REFRESH,
        }
        .fetch();
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        if let Some(cancelled) = self
            .subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
        {
            cancelled.store(true, Ordering::Release);
        }
    }
}

impl Drop for HttpDatafeed {
    fn drop(&mut self) {
        for cancelled in self
            .subscriptions
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
        {
            cancelled.store(true, Ordering::Release);
        }
    }
}

impl HttpDatafeed {
    pub(crate) fn search_symbols(
        &self,
        text: String,
        symbol_type: Option<SymbolType>,
        exchange: Option<String>,
        offset: usize,
        limit: usize,
        reply: Reply<Vec<Instrument>>,
    ) {
        let target = url(
            &self.base,
            "search",
            &[
                ("query", text),
                (
                    "type",
                    symbol_type.map(|kind| kind.to_string()).unwrap_or_default(),
                ),
                ("exchange", exchange.unwrap_or_default()),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
            ],
        );
        get(target, String::new(), move |result| {
            reply.send(result.and_then(|symbols: Vec<Symbol>| {
                symbols
                    .into_iter()
                    .take(limit)
                    .map(Symbol::into_instrument)
                    .collect()
            }));
        });
    }
}

fn get<T: DeserializeOwned>(
    target: String,
    symbol: String,
    answer: impl FnOnce(Result<T, FeedError>) + Send + 'static,
) {
    ehttp::fetch(ehttp::Request::get(target), move |result| {
        answer(
            result
                .map_err(FeedError::Network)
                .and_then(|response| decode(&symbol, response)),
        );
    });
}

fn decode<T: DeserializeOwned>(symbol: &str, response: ehttp::Response) -> Result<T, FeedError> {
    match response.status {
        200..=299 => serde_json::from_slice(&response.bytes)
            .map_err(|error| FeedError::InvalidData(error.to_string())),
        404 => Err(FeedError::SymbolNotFound(symbol.to_owned())),
        429 => Err(FeedError::RateLimited {
            retry_after: response.headers.get("retry-after").and_then(|value| {
                value
                    .trim()
                    .parse()
                    .ok()
                    .map(Duration::from_secs)
                    .or_else(|| {
                        chrono::DateTime::parse_from_rfc2822(value)
                            .ok()
                            .map(|time| {
                                (time.with_timezone(&chrono::Utc) - chrono::Utc::now())
                                    .to_std()
                                    .unwrap_or(Duration::ZERO)
                            })
                    })
            }),
        }),
        408 | 500..=599 => Err(FeedError::Network(format!("HTTP {}", response.status))),
        _ => Err(FeedError::InvalidData(format!("HTTP {}", response.status))),
    }
}

struct Poll {
    base: String,
    subscription: Subscription,
    cancelled: Arc<AtomicBool>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    retry: Duration,
}

impl Poll {
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && self.subscription.sink.is_active()
    }

    fn fetch(self) {
        if !self.active() {
            return;
        }
        let request = HistoryRequest {
            symbol: self.subscription.symbol.clone(),
            timeframe: self.subscription.timeframe,
            clock: self.subscription.clock.clone(),
            before: None,
            since: self.since,
            count: 500,
            extended_hours: self.subscription.extended_hours,
            conversion: self.subscription.conversion.clone(),
        };
        let target = match history_url(&self.base, &request) {
            Ok(target) => target,
            Err(error) => return self.subscription.sink.report_error(error),
        };
        self.subscription.sink.set_status(FeedStatus::Connecting);
        get(target, request.symbol.clone(), move |result| {
            let result = result.and_then(|columns: Columns| columns.into_page(&request));
            if let Some((poll, delay)) = self.received(result) {
                later(
                    delay,
                    poll.cancelled.clone(),
                    Box::new(move || poll.fetch()),
                );
            }
        });
    }

    fn received(mut self, result: Result<HistoryPage, FeedError>) -> Option<(Self, Duration)> {
        if !self.active() {
            return None;
        }
        let sink = &self.subscription.sink;
        let result = result.and_then(|page| {
            if page.bars.len() == 500 && self.since.is_some_and(|since| page.bars[0].time > since) {
                Err(FeedError::InvalidData(
                    "live history window exceeded; reload history".into(),
                ))
            } else {
                Ok(page)
            }
        });
        let delay = match result {
            Ok(page) => {
                for bar in page.bars {
                    self.since = Some(bar.time);
                    sink.push_bar(bar);
                }
                sink.set_status(FeedStatus::Live);
                self.retry = REFRESH;
                REFRESH
            }
            Err(error) => {
                let recoverable = error.is_recoverable();
                let delay = match &error {
                    FeedError::RateLimited {
                        retry_after: Some(delay),
                    } => (*delay).max(self.retry),
                    _ => self.retry,
                };
                sink.set_status(FeedStatus::Disconnected);
                sink.report_error(error);
                if !recoverable {
                    return None;
                }
                self.retry = (self.retry * 2).min(Duration::from_secs(60));
                delay
            }
        };
        Some((self, delay))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn later(delay: Duration, cancelled: Arc<AtomicBool>, action: Box<dyn FnOnce() + Send>) {
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        while !cancelled.load(Ordering::Acquire) {
            let remaining = delay.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return action();
            }
            std::thread::sleep(remaining.min(Duration::from_secs(1)));
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn later(delay: Duration, cancelled: Arc<AtomicBool>, action: Box<dyn FnOnce() + Send>) {
    use wasm_bindgen::{JsCast as _, closure::Closure};
    if let Some(window) = web_sys::window() {
        // Release cancelled retries promptly, even after a long Retry-After.
        let wait = delay.min(Duration::from_secs(1));
        let millis =
            wait.as_millis() as i32 + i32::from(!wait.subsec_nanos().is_multiple_of(1_000_000));
        let callback = Closure::once_into_js(move || {
            if !cancelled.load(Ordering::Acquire) {
                if delay > wait {
                    later(delay - wait, cancelled, action);
                } else {
                    action();
                }
            }
        });
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            millis,
        );
    }
}
