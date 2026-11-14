//! Own data: a visitor's REST history and WebSocket stream, mapped to bars and trades by a saved
//! profile and served to the chart through the library's `Datafeed` trait.
//!
//! `profile` is the saved description. `feed` answers history and owns the bars of every
//! subscription: it reads REST through `http`, lays responses out with `shape`, reads times with
//! `time` and builds bars with `bars`. `stream` keeps one socket per subscription open through
//! `socket` and hands its records to the feed. `dialog`, `tree` and `store` are the user
//! interface and the saved state. `app` is the application's side: it enters and leaves own data
//! and decides what stays tied to the synthetic session.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use egui_charts::data::FeedError;

pub(crate) mod app;
pub(crate) mod bars;
pub(crate) mod dialog;
pub(crate) mod feed;
pub(crate) mod http;
pub(crate) mod profile;
pub(crate) mod shape;
pub(crate) mod socket;
pub(crate) mod store;
pub(crate) mod stream;
pub(crate) mod time;
pub(crate) mod tree;

/// The values of the keys a profile names, as `(name, value)`. They are kept apart from the
/// profile so that a saved or exported profile never holds one.
pub(crate) type Secrets = Vec<(String, String)>;

/// Why a request to a configured endpoint produced no data. The dialog shows it after the
/// endpoint; the feed turns it into the error the chart reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// No response arrived: the connection failed or timed out or, in a browser, the response
    /// was withheld under the cross-origin rules. A browser does not tell these apart.
    Refused(String),
    /// The server answered with a status outside 200 to 299.
    Status(u16),
    /// A response arrived, but it is not JSON or the mapping finds no row or field to read.
    Unreadable(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(reason) if cfg!(target_arch = "wasm32") => write!(
                f,
                "no response ({reason}). The server is unreachable, or it does not allow this \
                 page's origin or these request headers (cross-origin rules)"
            ),
            Self::Refused(reason) => write!(f, "no response ({reason})"),
            Self::Status(status @ (401 | 403)) => write!(
                f,
                "rejected as unauthorised (HTTP {status}): check the key and where it is sent"
            ),
            Self::Status(status) => write!(f, "HTTP {status}"),
            Self::Unreadable(reason) => write!(f, "unreadable response: {reason}"),
        }
    }
}

impl Failure {
    /// The error the chart reports for a history request on `symbol`. Statuses map as in the
    /// library's HTTP feed, so the chart retries what can succeed later and stops on the rest.
    pub(crate) fn feed_error(&self, symbol: &str) -> FeedError {
        match self {
            Self::Status(404) => FeedError::SymbolNotFound(symbol.to_owned()),
            Self::Status(429) => FeedError::RateLimited { retry_after: None },
            Self::Refused(_) | Self::Status(408 | 500..=599) => {
                FeedError::Network(self.to_string())
            }
            Self::Status(_) | Self::Unreadable(_) => FeedError::InvalidData(self.to_string()),
        }
    }
}

/// `url` without its query and fragment: the name of an endpoint in an error, a log line or a
/// notification. A key passed in the query is therefore never shown.
pub(crate) fn endpoint(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

/// `text` with the value of every key of `secrets` written as its `{name}`, as typed and as an
/// address encodes it. An endpoint keeps its path, and a key can travel there.
pub(crate) fn conceal(text: &str, secrets: &Secrets) -> String {
    let mut text = text.to_owned();
    for (name, value) in secrets.iter().filter(|(_, value)| !value.is_empty()) {
        let named = format!("{{{name}}}");
        text = text.replace(value.as_str(), &named);
        let encoded = profile::fill_url("{v}", &[("v", value)]);
        if encoded != *value {
            text = text.replace(&encoded, &named);
        }
    }
    text
}

/// Runs `action` after `delay` unless `cancelled` is set first. It serves the deadline of a
/// request, the keep-alive of a stream and the wait before a reconnect. A cancelled wait is
/// released within a second. `action` runs on a thread of its own natively and as a browser task
/// on the web, never inside this call.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn after(
    delay: Duration,
    cancelled: Arc<AtomicBool>,
    action: impl FnOnce() + Send + 'static,
) {
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

/// Runs `action` after `delay` unless `cancelled` is set first. It serves the deadline of a
/// request, the keep-alive of a stream and the wait before a reconnect. A cancelled wait is
/// released within a second. `action` runs on a thread of its own natively and as a browser task
/// on the web, never inside this call.
#[cfg(target_arch = "wasm32")]
pub(crate) fn after(
    delay: Duration,
    cancelled: Arc<AtomicBool>,
    action: impl FnOnce() + Send + 'static,
) {
    use wasm_bindgen::{JsCast as _, closure::Closure};
    let Some(window) = web_sys::window() else {
        return;
    };
    // Wake at least once a second so that a cancelled wait releases what it holds.
    let wait = delay.min(Duration::from_secs(1));
    let callback = Closure::once_into_js(move || {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        if delay > wait {
            after(delay - wait, cancelled, action);
        } else {
            action();
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        callback.unchecked_ref(),
        wait.as_millis() as i32,
    );
}
