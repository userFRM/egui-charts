//! Historical and streaming market data contracts, aggregation and order flow data.
//!
//! Implement [`Datafeed`] to connect a chart to your backend, or use the data
//! containers and aggregation functions independently of a widget.
//!
//! # Connecting a feed
//!
//! Attach a feed with `ChartBuilder::with_feed`. Only [`Datafeed::history`] is
//! required. The chart calls the feed on the UI thread; implementations should
//! return promptly, answer each request once through [`Reply`] and push live updates
//! into the [`Sink`] of a [`Subscription`]. Replies and pushes can come from another
//! thread, runtime or browser task. No network connection or data provider is built in.
//!
//! [`Subscription::clock`] and [`HistoryRequest::clock`] carry the same bar boundaries
//! so history and streaming aggregation agree. Symbol search is a separate
//! [`SymbolSearch`](crate::services::SymbolSearch) service.
//!
//! # Order flow and derived data
//!
//! Push executions, quotes and book updates through [`Sink`]. [`Footprint`] holds
//! executed volume at price; [`DepthHistory`] holds the book over time.
//! [`ReplayClock`] coordinates host-supplied recorded events. [`resample`] aggregates
//! bars into a longer interval, and [`VolumeProfile`] describes volume by price.
//!
//! The optional feed methods provide auxiliary study data and records for host UI,
//! including news, calendars, hotlists and screener rows. Unsupported requests use
//! [`FeedError::NotSupported`].

pub use crate::model::Bar;
pub use crate::model::Timeframe;
use crate::model::{BarClock, Instrument, Tick};
use chrono::{DateTime, Utc};
use std::error::Error;
use std::fmt;
use std::time::Duration;

pub(crate) mod depth;
mod depth_history;
mod footprint;
#[cfg_attr(
    not(any(test, feature = "widget")),
    expect(dead_code, reason = "only the chart widget consumes a feed")
)]
pub(crate) mod inbox;
mod market;
mod metrics;
pub use metrics::{MetricPage, MetricPoint, MetricRequest, MetricSeries, MetricSubscription};
#[cfg_attr(
    not(any(test, feature = "widget")),
    expect(dead_code, reason = "only the chart widget consumes a feed")
)]
pub(crate) mod merge;
mod options_volume;
mod order_flow;
mod orders;
mod recording;
mod replay;
mod resample;
mod targets;
mod volume_profile;
pub use depth_history::{DepthColumn, DepthHistory, DepthHistoryConfig};
pub use footprint::{Footprint, FootprintBar, FootprintLevel};
pub use market::{
    CalendarEvent, CalendarKind, CalendarQuery, HotlistEntry, HotlistQuery, Importance, NewsQuery,
    NewsStory, ScreenerColumn, ScreenerColumnSet, ScreenerFilter, ScreenerOp, ScreenerQuery,
    ScreenerRow,
};
#[cfg(feature = "studies")]
pub(crate) use options_volume::OptionsVolumeSource;
pub use options_volume::{OptionsVolume, OptionsVolumeQuery, OptionsVolumeSink};
#[cfg_attr(
    not(any(test, feature = "widget")),
    expect(dead_code, reason = "only the chart widget consumes a feed")
)]
pub(crate) mod trades;

pub use inbox::{Reply, Sink, aggregate_trades, aggregate_trades_with_tick_size};
pub use order_flow::{OrderFlowInput, OrderFlowKind};
pub use orders::{
    OrderEvent, OrderFilter, OrderFilterUnavailable, OrderQueue, OrderSizeTier, OrderSizeTiers,
    RestingOrder,
};
pub use recording::{RecordingReader, RecordingWriter};
pub use replay::{MarketEvent, ReplayClock, ReplayEvent};
#[cfg(feature = "studies")]
pub(crate) use resample::resample_periods;
pub use resample::{resample, resample_with_session};
pub use targets::PriceTargets;
pub use trades::{CombinedTrade, combine_trades};
pub use volume_profile::{ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig};

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

/// Source of instrument metadata, history and live updates for a chart, given to it with
/// `ChartBuilder::with_feed`.
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
    ///
    /// Also called when the chart is dropped, which an application may do while egui holds the
    /// lock of its `Context`, for example inside `Context::input` or `Ui::input`. A push into a
    /// [`Sink`] or an answer through a [`Reply`] may wake a chart, and waking waits for that lock
    /// until the application's closure returns. So:
    ///
    /// - Return without waiting for a thread that pushes or answers: do not join it, and do not
    ///   take a lock it holds while it pushes or answers.
    /// - Answer or drop here only the replies of the chart that held `id`, and push into no other
    ///   subscription's `Sink`: another chart would be woken on this thread, which holds the
    ///   lock. A feed that cannot tell which chart a `Reply` is for keeps its replies and answers
    ///   them as usual; an answer for a dropped chart is never read.
    ///
    /// When the chart held the last reference to the feed, the feed's own `Drop` runs right
    /// after this call and must not wait for such a thread either.
    fn unsubscribe(&self, id: SubscriptionId) {
        let _ = id;
    }

    /// Up to `request.count` trades older than `request.before`, or the latest trades when it is
    /// `None`, so order flow survives a reload. The default answers [`FeedError::NotSupported`].
    fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
        let _ = request;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Push complete options-volume snapshots for the underlying and inclusive expiry range.
    /// Each push replaces the previous figures, including an empty snapshot. Sum contracts
    /// across the requested expiries at each strike. Stop when `sink.is_active()` is false.
    /// The default supplies none; the library never fetches options data itself.
    fn options_volume(&self, query: OptionsVolumeQuery, sink: OptionsVolumeSink) {
        let _ = (query, sink);
    }

    /// One page of recorded depth on the requested grid. The default answers
    /// [`FeedError::NotSupported`]; live-only feeds need not implement this method.
    fn depth_history(&self, request: DepthRequest, reply: Reply<DepthPage>) {
        let _ = request;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Metrics offered for `symbol`, in display order. The default offers none.
    /// The host publishes the values; the library computes no instrument metrics.
    fn metrics(&self, symbol: &str, reply: Reply<Vec<MetricSeries>>) {
        let _ = symbol;
        reply.send(Ok(Vec::new()));
    }

    /// Metric history. The default answers [`FeedError::NotSupported`].
    fn metric_history(&self, request: MetricRequest, reply: Reply<MetricPage>) {
        let _ = request;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Start publishing metric points until [`Self::unsubscribe`]. The default does nothing.
    fn subscribe_metric(&self, subscription: MetricSubscription) {
        let _ = subscription;
    }

    /// Analyst price targets for `symbol`. The default answers [`FeedError::NotSupported`],
    /// which leaves the chart without targets and shows no error.
    fn price_targets(&self, symbol: &str, reply: Reply<crate::data::PriceTargets>) {
        let _ = symbol;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Key statistics and the company or contract profile of `symbol`, for the symbol details
    /// panel. The default answers [`FeedError::NotSupported`], and the panel then shows neither.
    fn symbol_details(&self, symbol: &str, reply: Reply<SymbolDetails>) {
        let _ = symbol;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Up to `query.limit` stories, newest first, for the news and highlights panels. Answer
    /// [`FeedError::NotSupported`] (the default) to have the panels say the feed has none.
    fn news(&self, query: NewsQuery, reply: Reply<Vec<NewsStory>>) {
        let _ = query;
        reply.send(Err(FeedError::NotSupported));
    }

    /// The events of one calendar that pass `query`'s filters, in time order, for the calendar
    /// panel. Answer [`FeedError::NotSupported`] (the default) to have the panel say the feed has
    /// none.
    fn calendar(&self, query: CalendarQuery, reply: Reply<Vec<CalendarEvent>>) {
        let _ = query;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Up to `query.limit` symbols that meet every filter of `query`, sorted as it asks, each
    /// with the values of its columns, for the screener. The feed does the screening. Answer
    /// [`FeedError::InvalidData`] for a filter it cannot apply, and [`FeedError::NotSupported`]
    /// (the default) to have the panel say the feed has none.
    fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
        let _ = query;
        reply.send(Err(FeedError::NotSupported));
    }

    /// Up to `query.limit` symbols of the hotlist `query.list`, in the list's order, for the
    /// hotlists panel. Answer [`FeedError::NotSupported`] (the default) to have the panel say the
    /// feed has none.
    fn hotlist(&self, query: HotlistQuery, reply: Reply<Vec<HotlistEntry>>) {
        let _ = query;
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
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// Bar interval requested from the feed.
    pub timeframe: Timeframe,
    /// Calendar boundaries selected by the chart, shared by historical and streaming aggregation.
    pub clock: BarClock,
    /// Only bars that open strictly before this instant. `None` asks for the latest bars.
    pub before: Option<DateTime<Utc>>,
    /// Bars wanted. Fewer is fine; the chart asks again as the user scrolls back.
    pub count: usize,
    /// Include the instrument's extended trading hours, as well as regular hours.
    pub extended_hours: bool,
}

/// One page of history. A page without bars ends scroll-back like `end_of_history` does, so a
/// feed that pages by time window must skip empty windows such as weekends itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryPage {
    /// Any order, each stamped with its open or any instant inside it: a bar stamped with its
    /// close time lands in the next bar. A bar of a day or longer covers a calendar period of
    /// [`HistoryRequest::clock`], such as a day, a week from Monday or a calendar
    /// month, in UTC by default: stamp it inside that period, for example at its local midnight. A
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
    /// Instrument identifier used by the host.
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

/// One page of recorded depth for a symbol, using the same price and time grid as live columns.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DepthRequest {
    /// Instrument requested.
    pub symbol: String,
    /// Columns opening strictly before this instant, or the latest columns with `None`.
    pub before: Option<DateTime<Utc>>,
    /// Maximum columns wanted.
    pub count: usize,
    /// Cadence, row grouping and limits of the receiving store.
    pub config: DepthHistoryConfig,
    /// Price of one tick, finite and positive.
    pub tick_size: f64,
}

impl DepthRequest {
    /// Requests the latest `count` columns using `config` and the instrument's `tick_size`.
    pub fn new(
        symbol: impl Into<String>,
        count: usize,
        config: DepthHistoryConfig,
        tick_size: f64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            before: None,
            count,
            config,
            tick_size,
        }
    }
}

/// Recorded depth columns. Empty pages end paging, including when `end_of_history` is false.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DepthPage {
    /// Columns on the requested grid; held live columns win on overlap. Gaps remain gaps.
    pub columns: Vec<DepthColumn>,
    /// Nothing exists before the oldest returned column.
    pub end_of_history: bool,
}

impl DepthPage {
    /// A page of `columns`, the last one when `end_of_history`.
    pub fn new(columns: Vec<DepthColumn>, end_of_history: bool) -> Self {
        Self {
            columns,
            end_of_history,
        }
    }
}

/// Facts about one symbol as labelled values, already formatted for display, in the order to
/// show them. Supply only what is known: the panel shows a section only when it has values.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SymbolDetails {
    /// Key statistics, such as `("Market capitalization", "2.95T USD")`.
    pub statistics: Vec<(String, String)>,
    /// The company or contract profile, such as `("Sector", "Technology")`.
    pub profile: Vec<(String, String)>,
    /// A page about the instrument, linked beside its name.
    pub url: Option<String>,
    /// What the instrument is, such as `"Technology • Software"`, shown under its name.
    pub category: Option<String>,
}

impl SymbolDetails {
    /// Details made of `statistics` and `profile`.
    pub fn new(statistics: Vec<(String, String)>, profile: Vec<(String, String)>) -> Self {
        Self {
            statistics,
            profile,
            url: None,
            category: None,
        }
    }

    /// Sets [`url`](Self::url).
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets [`category`](Self::category).
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

/// A live subscription: push its updates into `sink` until [`Datafeed::unsubscribe`] gets `id`.
#[non_exhaustive]
pub struct Subscription {
    /// Identifier passed back to the feed when unsubscribing.
    pub id: SubscriptionId,
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// Bar interval requested from the feed.
    pub timeframe: Timeframe,
    /// The same calendar boundaries supplied to history requests.
    pub clock: BarClock,
    /// Include the instrument's extended trading hours, as well as regular hours.
    pub extended_hours: bool,
    /// Destination for updates; stop publishing when it becomes inactive.
    pub sink: Sink,
}

/// Identifies a subscription. Unique within the process, so one feed can serve many charts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

/// Connection state of a live subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeedStatus {
    /// A connection attempt is in progress.
    Connecting,
    /// The feed reports live data.
    Live,
    /// The connection is closed.
    Disconnected,
}

/// Why a feed or another service could not answer.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FeedError {
    /// The feed or service does not offer this request.
    NotSupported,
    /// The service could not find the requested instrument.
    SymbolNotFound(String),
    /// The service has limited the request rate.
    RateLimited {
        /// Suggested delay before retrying, when supplied by the service.
        retry_after: Option<Duration>,
    },
    /// A transport error with a service-supplied message.
    Network(String),
    /// Invalid input or data, described by the service.
    InvalidData(String),
    /// The feed or service dropped a [`Reply`] without answering.
    Unanswered,
    /// A service error outside the other categories.
    Other(String),
}

impl FeedError {
    /// Whether retrying the same request can succeed without changing its inputs.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::RateLimited { .. } | Self::Unanswered
        )
    }
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
    /// Catalog-change notifications received for this subscription, for host search refreshes.
    pub symbol_catalog_revision: u64,
    /// Whether a historical page is currently being requested.
    pub loading_history: bool,
    /// Whether the feed has reported that no older history remains.
    pub history_complete: bool,
    /// Most recent feed error, when present.
    pub last_error: Option<FeedError>,
    /// Bars and ticks dropped by validation: non-finite values, a high or low on the wrong side of
    /// the open and close, negative volume, a tick that would overflow its bar's volume, a time
    /// too far ahead of the feed's clock (see `Sink::set_server_time`), or pending bar slots
    /// evicted after overflow (also reported in `last_error`).
    pub rejected_bars: u64,
    /// Trades older than the last accepted execution or forming bar, dropped.
    pub late_ticks: u64,
    /// Trades whose id is among those of the last trades the subscription folded, as many as its
    /// trade store holds, dropped: a feed replaying its last trades after a reconnect.
    pub repeated_ticks: u64,
    /// Order book snapshots and deltas dropped by validation: a non-finite price or size, a
    /// negative size. Each one clears the book until the feed pushes a new snapshot.
    pub rejected_depth: u64,
}
