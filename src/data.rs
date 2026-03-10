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
//! [`poll`](DataSource::poll), [`fetch_historical`](DataSource::fetch_historical),
//! and [`get_timeframe`](DataSource::get_timeframe).  The remaining methods have
//! sensible defaults that opt out of optional capabilities (search, marks, server
//! time).
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
use std::error::Error;
use std::fmt;

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
    /// Called when the user scrolls left beyond the currently loaded range.
    /// The implementation may block or use internal async machinery -- the chart
    /// engine will not call this from the UI thread if it would block.
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
