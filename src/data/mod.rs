//! Data source abstraction for flexible data fetching.
//!
//! This module defines the [`DataSource`] trait -- the primary integration point
//! for connecting external market data to the chart engine.  Any struct that
//! implements `DataSource` can feed OHLCV bars into the chart, whether the data
//! comes from a WebSocket stream, a REST API, a local CSV file, or an in-memory
//! buffer.
//!
//! # Architecture
//!
//! ```text
//!  ┌─────────────┐       ┌──────────────┐       ┌──────────────┐
//!  │  WebSocket   │       │  REST API    │       │  CSV / File  │
//!  └──────┬───────┘       └──────┬───────┘       └──────┬───────┘
//!         │                      │                      │
//!         └──────────┬───────────┴──────────┬───────────┘
//!                    │                      │
//!               impl DataSource        impl DataSource
//!                    │                      │
//!                    └──────────┬───────────┘
//!                               │
//!                      ┌────────▼────────┐
//!                      │   Chart Engine  │
//!                      └─────────────────┘
//! ```
//!
//! # Implementing a DataSource
//!
//! At minimum, implement [`symbols`](DataSource::symbols),
//! [`subscribe`](DataSource::subscribe), [`unsubscribe`](DataSource::unsubscribe),
//! [`poll`](DataSource::poll), [`fetch_historical`](DataSource::fetch_historical)
//! and [`get_timeframe`](DataSource::get_timeframe). A source that serves history
//! also returns `true` from [`supports_historical`](DataSource::supports_historical);
//! the default is `false`, and the chart then never calls `fetch_historical`. The
//! other methods default to not offering search, marks or server time.
//!
//! # Provided types
//!
//! | Type                     | Purpose                                          |
//! |--------------------------|--------------------------------------------------|
//! | [`DataSourceError`]      | Error enum for all data source operations         |
//! | [`DataUpdate`]           | Enum of possible updates returned by `poll()`     |
//! | [`HistoricalDataRequest`]| Parameters for a historical data fetch            |
//! | [`SymbolSearchResult`]   | Single result from a symbol search                |
//! | [`PaginatedSearchResult`]| Paginated wrapper around search results           |
//! | [`BarMark`]              | Event marker attached to a specific bar           |
//! | [`TimescaleMark`]        | Event marker on the time axis                     |

pub use crate::model::Bar;
pub use crate::model::Timeframe;
use crate::model::{Instrument, Tick};
use chrono::{DateTime, Utc};
use std::error::Error;
use std::fmt;
use std::time::Duration;

pub(crate) mod depth;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`Chart` does not consume a `Datafeed` yet")
)]
pub(crate) mod inbox;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`Chart` does not consume a `Datafeed` yet")
)]
pub(crate) mod merge;
mod resample;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`Chart` does not consume a `Datafeed` yet")
)]
pub(crate) mod trades;

pub use inbox::{Reply, Sink};
pub use resample::resample;

/// Error types for data source operations.
///
/// All fallible [`DataSource`] methods return this error type.  Use
/// [`is_recoverable`](DataSourceError::is_recoverable) to decide whether an
/// automatic retry is appropriate.
#[derive(Debug, Clone)]
pub enum DataSourceError {
    /// Failed to connect to data source
    ConnError(String),
    /// Failed to fetch data
    FetchError(String),
    /// Symbol not found
    SymbolNotFound(String),
    /// Invalid time range
    InvalidTimeRange(String),
    /// IO error (cannot be cloned, stores error message)
    IoError(String),
    /// Parse error
    ParseError(String),
    /// Request was rate limited
    RateLimited,
    /// Feature not supported by this data source
    NotSupported(String),
}

impl fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataSourceError::ConnError(msg) => write!(f, "Conn error: {msg}"),
            DataSourceError::FetchError(msg) => write!(f, "Fetch error: {msg}"),
            DataSourceError::SymbolNotFound(msg) => write!(f, "Symbol not found: {msg}"),
            DataSourceError::InvalidTimeRange(msg) => write!(f, "Invalid time range: {msg}"),
            DataSourceError::IoError(msg) => write!(f, "IO error: {msg}"),
            DataSourceError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            DataSourceError::RateLimited => write!(f, "Rate limited"),
            DataSourceError::NotSupported(msg) => write!(f, "Not supported: {msg}"),
        }
    }
}

impl Error for DataSourceError {}

impl From<std::io::Error> for DataSourceError {
    fn from(err: std::io::Error) -> Self {
        DataSourceError::IoError(err.to_string())
    }
}

impl DataSourceError {
    /// Check if this error is recoverable (can retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            DataSourceError::ConnError(_)
                | DataSourceError::FetchError(_)
                | DataSourceError::IoError(_)
        )
    }

    /// Check if this error is a conn-related error
    pub fn is_conn_error(&self) -> bool {
        matches!(self, DataSourceError::ConnError(_))
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        match self {
            DataSourceError::ConnError(msg)
            | DataSourceError::FetchError(msg)
            | DataSourceError::SymbolNotFound(msg)
            | DataSourceError::InvalidTimeRange(msg)
            | DataSourceError::IoError(msg)
            | DataSourceError::ParseError(msg)
            | DataSourceError::NotSupported(msg) => msg,
            DataSourceError::RateLimited => "Rate limited",
        }
    }
}

/// An incremental data update returned by [`DataSource::poll`].
///
/// The chart engine pattern-matches on these variants to decide how to merge
/// incoming data into its internal bar buffer.
#[derive(Debug, Clone)]
pub enum DataUpdate {
    /// New bars appended to the right edge (live / streaming data).
    NewBars {
        /// Symbol the bars belong to.
        symbol: String,
        /// One or more new bars, ordered oldest-first.
        bars: Vec<Bar>,
    },

    /// Historical bars prepended to the left edge (lazy-loaded older data).
    HistoricalBars {
        /// Symbol the bars belong to.
        symbol: String,
        /// Older bars, ordered oldest-first.
        bars: Vec<Bar>,
    },

    /// Full dataset replacement (initial load or symbol/timeframe change).
    FullDataset {
        /// Symbol the dataset belongs to.
        symbol: String,
        /// Complete bar series, ordered oldest-first.
        bars: Vec<Bar>,
    },

    /// The set of available symbols has changed.
    SymbolsChanged(Vec<String>),

    /// Connection status changed (`true` = connected, `false` = disconnected).
    ConnStatus(bool),
}

/// Parameters for a historical data fetch via [`DataSource::fetch_historical`].
///
/// The chart engine creates this request when the user scrolls left beyond the
/// currently loaded data range and the data source supports historical fetching.
#[derive(Debug, Clone)]
pub struct HistoricalDataRequest {
    /// Symbol to fetch data for.
    pub symbol: String,
    /// Timeframe (bar interval) to fetch.
    pub timeframe: Timeframe,
    /// End timestamp in Unix milliseconds (exclusive) -- fetch bars *before* this point.
    pub end_ts_millis: i64,
    /// Maximum number of bars to return.
    pub limit: usize,
}

/// Symbol search result
#[derive(Debug, Clone)]
pub struct SymbolSearchResult {
    /// Symbol name (e.g., "AAPL")
    pub symbol: String,
    /// Full symbol name with exchange (e.g., "NASDAQ:AAPL")
    pub full_name: String,
    /// Symbol description
    pub description: String,
    /// Exchange name
    pub exchange: String,
    /// Symbol type (stock, crypto, forex, etc.)
    pub symbol_type: String,
}

/// Bar mark (event on a bar)
#[derive(Debug, Clone)]
pub struct BarMark {
    /// Bar timestamp (Unix milliseconds)
    pub time: i64,
    /// Mark color
    pub color: String,
    /// Mark label text
    pub label: String,
    /// Mark tooltip text
    pub tooltip: String,
}

/// Timescale mark (event on the timescale)
#[derive(Debug, Clone)]
pub struct TimescaleMark {
    /// Mark timestamp (Unix milliseconds)
    pub time: i64,
    /// Mark color
    pub color: String,
    /// Mark label
    pub label: String,
    /// Mark tooltip
    pub tooltip: String,
}

/// Paginated symbol search result
#[derive(Debug, Clone)]
pub struct PaginatedSearchResult {
    /// Search results for this page
    pub results: Vec<SymbolSearchResult>,
    /// Total number of matching results
    pub total: usize,
    /// Current offset (number of results skipped)
    pub offset: usize,
    /// Whether there are more results available
    pub has_more: bool,
}

/// Data source trait -- the primary integration point for feeding market data
/// into the chart engine.
///
/// Implementors supply OHLCV bars from any source (WebSocket, REST, CSV, etc.).
/// The chart engine calls methods on this trait to subscribe to symbols, poll for
/// live updates, and request historical data when the user scrolls back in time.
///
/// # Required methods
///
/// | Method              | Purpose                                         |
/// |---------------------|-------------------------------------------------|
/// | `symbols`           | List available instrument identifiers            |
/// | `subscribe`         | Begin receiving live data for a symbol           |
/// | `unsubscribe`       | Stop receiving live data for a symbol            |
/// | `poll`              | Non-blocking check for new data updates          |
/// | `fetch_historical`  | Load older bars when the user scrolls left       |
/// | `get_timeframe`     | Timeframe a symbol was subscribed with           |
///
/// # Optional capabilities
///
/// Override the `supports_*` / `get_*` method pairs to enable:
/// - **Symbol search** -- typeahead search across instruments
/// - **Bar marks** -- event annotations on individual bars
/// - **Timescale marks** -- event annotations on the time axis
/// - **Server time** -- accurate exchange clock for countdown widget
///
/// # Thread safety
///
/// `DataSource` requires `Send` so it can be owned by the chart state which may
/// be passed across threads.  If your implementation wraps a connection handle
/// that is not `Send`, consider using a channel-based architecture.
pub trait DataSource: Send {
    /// Returns the list of symbol identifiers available from this data source.
    fn symbols(&self) -> Vec<String>;

    /// Subscribe to live updates for `symbol` at the given `timeframe`.
    ///
    /// After subscribing, subsequent calls to [`poll`](DataSource::poll) should
    /// return [`DataUpdate::NewBars`] or [`DataUpdate::FullDataset`] for this
    /// symbol.
    fn subscribe(&mut self, symbol: String, timeframe: Timeframe) -> Result<(), DataSourceError>;

    /// Unsubscribe from live updates for `symbol`.
    fn unsubscribe(&mut self, symbol: String) -> Result<(), DataSourceError>;

    /// Non-blocking poll for data updates since the last call.
    ///
    /// The chart engine calls this on every frame.  Return an empty `Vec` when
    /// there is nothing new.
    fn poll(&mut self) -> Vec<DataUpdate>;

    /// Fetch historical bars older than the data already loaded.
    ///
    /// Called when the user scrolls left beyond the currently loaded range, and
    /// when the chart is built or its symbol or timeframe changes. The chart
    /// calls it synchronously on the UI thread, so a slow implementation stalls
    /// rendering; fetch in the background and return what is already loaded.
    fn fetch_historical(
        &mut self,
        request: HistoricalDataRequest,
    ) -> Result<Vec<Bar>, DataSourceError>;

    /// Whether this data source supports [`fetch_historical`](DataSource::fetch_historical).
    ///
    /// Return `false` (default) if the data source only provides a fixed dataset.
    fn supports_historical(&self) -> bool {
        false
    }

    /// Whether the data source is currently connected and able to deliver data.
    ///
    /// Defaults to `true`. Override to reflect real connection state for
    /// WebSocket / network-based sources.
    fn is_connected(&self) -> bool {
        true
    }

    /// Returns the timeframe the given symbol was subscribed with, or `None` if
    /// the symbol is not currently subscribed.
    fn get_timeframe(&self, symbol: &str) -> Option<Timeframe>;

    /// Search for symbols matching `_user_input`.
    ///
    /// Override together with [`supports_symbol_search`](DataSource::supports_symbol_search)
    /// to enable the symbol-search dialog in the UI.
    fn search_symbols(
        &self,
        _user_input: &str,
        _exchange: &str,
        _symbol_type: &str,
        _max_records: usize,
    ) -> Result<Vec<SymbolSearchResult>, DataSourceError> {
        Ok(Vec::new())
    }

    /// Whether this data source supports the symbol search API.
    fn supports_symbol_search(&self) -> bool {
        false
    }

    /// Returns event marks attached to bars in the time range `[_from, _to]`
    /// (Unix milliseconds).
    fn get_marks(
        &self,
        _symbol: &str,
        _from: i64,
        _to: i64,
    ) -> Result<Vec<BarMark>, DataSourceError> {
        Ok(Vec::new())
    }

    /// Whether this data source provides bar marks.
    fn supports_marks(&self) -> bool {
        false
    }

    /// Returns event marks on the timescale in the range `[_from, _to]`
    /// (Unix milliseconds).
    fn get_timescale_marks(
        &self,
        _symbol: &str,
        _from: i64,
        _to: i64,
    ) -> Result<Vec<TimescaleMark>, DataSourceError> {
        Ok(Vec::new())
    }

    /// Whether this data source provides timescale marks.
    fn supports_timescale_marks(&self) -> bool {
        false
    }

    /// Returns the exchange server time as a Unix timestamp (seconds).
    ///
    /// Used by the countdown-to-next-bar widget.  Defaults to the local
    /// system clock.
    fn get_server_time(&self) -> Result<i64, DataSourceError> {
        Ok(chrono::Utc::now().timestamp())
    }

    /// Whether this data source provides authoritative server time.
    fn supports_server_time(&self) -> bool {
        false
    }
}

/// `Send + Sync` on native targets. No bound on wasm32, where feeds hold JavaScript handles.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSend for T {}
/// `Send + Sync` on native targets. No bound on wasm32, where feeds hold JavaScript handles.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// Source of instrument metadata, history and live updates. No chart consumes it yet;
/// `TradingChart` reads a [`DataSource`].
///
/// Methods are called on the UI thread and must return promptly. Do the work elsewhere and answer
/// through the [`Reply`] or [`Sink`] you were given, from any thread, runtime or browser task.
///
/// ```
/// use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
/// use egui_charts::model::Bar;
///
/// /// Bars held in memory, oldest first. Answers inline, without live updates.
/// struct InMemory(Vec<Bar>);
///
/// impl Datafeed for InMemory {
///     fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
///         let before = |bar: &Bar| request.before.is_none_or(|before| bar.time < before);
///         let end = self.0.partition_point(before);
///         let start = end.saturating_sub(request.count);
///         reply.send(Ok(HistoryPage {
///             bars: self.0[start..end].to_vec(),
///             end_of_history: start == 0,
///         }));
///     }
/// }
/// ```
pub trait Datafeed: MaybeSend + 'static {
    /// Describes `symbol`. The default reports [`Instrument::new`] of the symbol.
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol)));
    }

    /// Up to `request.count` bars that open before `request.before`, or the latest bars when it
    /// is `None`.
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>);

    /// Starts pushing live updates into `subscription.sink` until `unsubscribe`. The default does
    /// nothing, which gives a history-only chart.
    fn subscribe(&self, subscription: Subscription) {
        let _ = subscription;
    }

    /// Stops what `subscribe` started. Pushes that arrive afterwards are ignored. The default does
    /// nothing; a producer can then poll [`Sink::is_active`] to know when to stop.
    fn unsubscribe(&self, id: SubscriptionId) {
        let _ = id;
    }

    /// Up to `request.count` trades older than `request.before`, or the latest trades when it is
    /// `None`, so order flow survives a reload. The default answers [`FeedError::NotSupported`].
    fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
        let _ = request;
        reply.send(Err(FeedError::NotSupported));
    }

    /// The order book of subscription `id` was lost to a gap, an unusable update or a book deeper
    /// than the limit: push a new snapshot with [`Sink::push_depth_snapshot`], and keep pushing
    /// deltas while it is fetched. The snapshot may be older than the latest delta: the chart
    /// holds the deltas pushed while the book waits and applies those numbered above the
    /// snapshot. Called once per loss: after an unusable snapshot the chart does not ask
    /// again, and the book stays empty until the feed pushes a usable one or the subscription
    /// restarts. The default does nothing, for feeds without depth.
    fn request_depth_snapshot(&self, id: SubscriptionId) {
        let _ = id;
    }
}

/// A request for one page of history.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HistoryRequest {
    pub symbol: String,
    pub timeframe: Timeframe,
    /// Only bars that open strictly before this instant. `None` asks for the latest bars.
    pub before: Option<DateTime<Utc>>,
    /// Bars wanted. Fewer is fine; the chart asks again as the user scrolls back.
    pub count: usize,
}

/// One page of history. A page without bars ends scroll-back like `end_of_history` does, so a
/// feed that pages by time window must skip empty windows such as weekends itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryPage {
    /// Any order, each stamped with its open or any instant inside it: a bar stamped with its
    /// close time lands in the next bar. A bar of a day or longer covers a UTC period of
    /// [`BarClock`](crate::model::BarClock), such as a UTC day, a week from Monday or a calendar
    /// month: stamp it inside that period, for example at 00:00 UTC on its trading date. A
    /// session open on the evening before can fall in the period of the bar before, and one of
    /// the two bars is then dropped. The chart sorts, deduplicates and validates them.
    pub bars: Vec<Bar>,
    /// Nothing exists before the oldest returned bar (or before `request.before` when `bars` is
    /// empty).
    pub end_of_history: bool,
}

/// A request for one page of trades.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TradeRequest {
    pub symbol: String,
    /// Only trades strictly before this instant. `None` asks for the latest trades.
    pub before: Option<DateTime<Utc>>,
    /// Trades wanted. Fewer is fine.
    pub count: usize,
}

/// One page of trades. A page without trades ends scroll-back like `end_of_history` does.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TradePage {
    /// Oldest first; trades sharing a time keep their order. They feed order flow only: bars come
    /// from [`Datafeed::history`]. The chart drops invalid trades and takes as many as its trade
    /// store has room for, up to the oldest trade it holds: later ones are left to the live stream.
    /// A trade with an [`id`](Tick::id) the chart already holds is dropped, so history and live
    /// trades join exactly when both carry ids. Without ids, only trades older than every held
    /// trade are taken, and live trades at or before the newest trade a page added are dropped:
    /// trades at the boundary instant may be lost, never counted twice.
    pub trades: Vec<Tick>,
    /// Nothing exists before the oldest returned trade.
    pub end_of_history: bool,
}

impl TradePage {
    /// A page of `trades`, the last one when `end_of_history`.
    pub fn new(trades: Vec<Tick>, end_of_history: bool) -> Self {
        Self {
            trades,
            end_of_history,
        }
    }
}

/// A live subscription: push its updates into `sink` until [`Datafeed::unsubscribe`] gets `id`.
#[non_exhaustive]
pub struct Subscription {
    pub id: SubscriptionId,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub sink: Sink,
}

/// Identifies a subscription. Unique within the process, so one feed can serve many charts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

/// Connection state of a live subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeedStatus {
    Connecting,
    Live,
    Disconnected,
}

/// Why a feed or another service could not answer.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FeedError {
    /// The feed or service does not offer this request.
    NotSupported,
    SymbolNotFound(String),
    RateLimited {
        retry_after: Option<Duration>,
    },
    Network(String),
    InvalidData(String),
    /// The feed or service dropped a [`Reply`] without answering.
    Unanswered,
    Other(String),
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSupported => f.write_str("not supported"),
            Self::SymbolNotFound(symbol) => write!(f, "symbol not found: {symbol}"),
            Self::RateLimited {
                retry_after: Some(after),
            } => write!(f, "rate limited, retry after {after:?}"),
            Self::RateLimited { retry_after: None } => f.write_str("rate limited"),
            Self::Network(message) => write!(f, "network error: {message}"),
            Self::InvalidData(message) => write!(f, "invalid data: {message}"),
            Self::Unanswered => f.write_str("no answer"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl Error for FeedError {}

/// What the chart knows about one series' feed, for the application to show or log.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FeedState {
    /// Last status pushed by the feed. `None` without a live subscription.
    pub status: Option<FeedStatus>,
    pub loading_history: bool,
    pub history_complete: bool,
    pub last_error: Option<FeedError>,
    /// Bars and ticks dropped by validation: non-finite values, a high or low on the wrong side of
    /// the open and close, negative volume, a tick that would overflow its bar's volume, a time
    /// too far ahead of the feed's clock (see `Sink::set_server_time`).
    pub rejected_bars: u64,
    /// Trades older than the forming bar, dropped.
    pub late_ticks: u64,
    /// Trades whose id is among those of the last trades the subscription folded, as many as its
    /// trade store holds, dropped: a feed replaying its last trades after a reconnect.
    pub repeated_ticks: u64,
    /// Order book snapshots and deltas dropped by validation: a non-finite price or size, a
    /// negative size. Each one clears the book until the feed pushes a new snapshot.
    pub rejected_depth: u64,
}
