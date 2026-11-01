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
//! thread, runtime or browser task. The optional `http` adapter connects to a host's server;
//! no market data provider is built in.
//!
//! [`Subscription::clock`] and [`HistoryRequest::clock`] carry the same bar boundaries
//! so history and streaming aggregation agree. Symbol search is a separate
//! [`SymbolSearch`](crate::services::SymbolSearch) service.
//!
//! # Order flow and derived data
//!
//! Push executions, quotes and book updates through [`Sink`]. [`Footprint`] holds
//! executed volume at price; [`DepthHistory`] holds the book over time.
//! [`ReplayDriver`] loads host recordings and drives subscriptions through one [`ReplayClock`].
//! [`resample`] aggregates
//! bars into a longer interval, and [`VolumeProfile`] describes volume by price.
//!
//! The optional feed methods provide auxiliary study data and records for host UI,
//! including news, calendars, hotlists and screener rows. Unsupported requests use
//! [`FeedError::NotSupported`].

pub use crate::model::Bar;
pub use crate::model::FeedService;
pub use crate::model::Timeframe;
use crate::model::{BarClock, Instrument, SymbolType, Tick};
use chrono::{DateTime, Utc};
use std::error::Error;
use std::fmt;
use std::time::Duration;

pub(crate) mod depth;
mod depth_history;
mod footprint;
#[cfg(feature = "http")]
pub mod http;
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
mod study;
mod targets;
mod volume_profile;
pub use depth_history::{DepthColumn, DepthHistory, DepthHistoryConfig};
#[cfg(any(feature = "studies", feature = "detectors"))]
pub(crate) use footprint::meets_threshold;
pub use footprint::{Footprint, FootprintBar, FootprintLevel, NodeSensitivity};
pub use market::{
    CalendarEvent, CalendarKind, CalendarQuery, HotlistEntry, HotlistQuery, Importance, NewsQuery,
    NewsStory, ScreenerColumn, ScreenerColumnSet, ScreenerFilter, ScreenerOp, ScreenerQuery,
    ScreenerRow, ScreenerTextAnswer, ScreenerUniverse,
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
pub use order_flow::{BarBook, OrderFlowBars, OrderFlowInput, OrderFlowKind};
pub use orders::{
    OrderEvent, OrderFilter, OrderFilterUnavailable, OrderQueue, OrderSizeTier, OrderSizeTiers,
    RestingOrder,
};
pub use recording::{RecordingReader, RecordingWriter};
pub use replay::{MarketEvent, ReplayClock, ReplayDriver, ReplayEvent};
#[cfg(feature = "studies")]
pub(crate) use resample::resample_periods;
pub use resample::{resample, resample_with_session};
pub use study::{StudyData, StudyDataRequest};
pub use targets::PriceTargets;
pub use trades::{CombinedTrade, ConsumedOrder, combine_trades};
pub use volume_profile::{
    ProfileDisplay, ProfileSource, StudyProfile, VolumeProfile, VolumeProfileConfig,
};

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
///         let since = |bar: &Bar| request.since.is_some_and(|since| bar.time < since);
///         let first = self.0.partition_point(since).min(end);
///         let start = end.saturating_sub(request.count).max(first);
///         reply.send(Ok(HistoryPage::new(self.0[start..end].to_vec(), start == 0)));
///     }
/// }
/// ```
pub trait Datafeed: MaybeSend + 'static {
    /// What the feed offers as a whole, before any symbol is resolved: the intervals, exchanges
    /// and symbol classes its selectors may offer, and the optional services it supplies. The
    /// chart asks once when it attaches the feed and keeps the answer in
    /// `Chart::feed_capabilities` (requires `widget`). The default
    /// answers [`FeedCapabilities::default`], which reports nothing and so restricts nothing.
    fn capabilities(&self, reply: Reply<FeedCapabilities>) {
        reply.send(Ok(FeedCapabilities::default()));
    }

    /// Describes `symbol`. The default reports [`Instrument::new`] of the symbol.
    /// Fill [`Instrument::profile_resolutions`] to suggest the bars volume profile studies and
    /// drawings read.
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol)));
    }

    /// Describes `symbol` with its prices converted to `conversion`, the currency or unit the
    /// host chose with `Chart::set_conversion` (requires `widget`). Label the
    /// instrument with the converted [`currency`](Instrument::currency) and
    /// [`unit`](Instrument::unit). The library converts nothing: the default answers
    /// [`FeedError::NotSupported`], which shows no data rather than unconverted data under a
    /// converted choice.
    fn resolve_converted(&self, symbol: &str, conversion: &str, reply: Reply<Instrument>) {
        let _ = (symbol, conversion);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Up to `request.count` bars of the range `[request.since, request.before)`, the newest ones
    /// when there are more. Either end left `None` is unbounded: without `before` the range
    /// reaches the latest bar.
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

    /// Up to `request.count` trades of the range `[request.since, request.before)`, the newest
    /// ones when there are more, so order flow survives a reload and follows the view back in
    /// time. The default answers [`FeedError::NotSupported`].
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

    /// As [`screen`](Self::screen), for a query whose `text` describes the screen in words. The
    /// feed interprets the text with the other criteria and answers the rows with an explanation
    /// of how it understood them. Answer [`FeedError::NotSupported`] (the default) to have the
    /// panel say the feed takes no descriptions, and [`FeedError::InvalidData`] for one it cannot
    /// interpret.
    fn screen_text(&self, query: ScreenerQuery, reply: Reply<ScreenerTextAnswer>) {
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

/// What a feed offers as a whole, answered by [`Datafeed::capabilities`]. Each list distinguishes
/// `None`, which the feed does not report and so restricts nothing, from `Some` of an empty
/// list, which it reports as offering none. Use [`declares`](Self::declares) when a service
/// needs explicit confirmation.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FeedCapabilities {
    /// Intervals [`Datafeed::history`] answers for at least some symbol. A symbol's own
    /// [`supported_timeframes`](Instrument::supported_timeframes) narrows them further.
    pub timeframes: Option<Vec<Timeframe>>,
    /// Exchanges whose symbols the feed serves.
    pub exchanges: Option<Vec<String>>,
    /// Classes of symbol the feed serves.
    pub symbol_types: Option<Vec<SymbolType>>,
    /// Optional services the feed supplies for at least some symbol. A symbol's own
    /// [`services`](Instrument::services) narrows them further. Services absent from a
    /// reported list are unavailable.
    pub services: Option<Vec<FeedService>>,
}

impl FeedCapabilities {
    /// Reports no capability, which restricts nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`timeframes`](Self::timeframes).
    pub fn with_timeframes(mut self, timeframes: impl IntoIterator<Item = Timeframe>) -> Self {
        self.timeframes = Some(timeframes.into_iter().collect());
        self
    }

    /// Sets [`exchanges`](Self::exchanges).
    pub fn with_exchanges(mut self, exchanges: impl IntoIterator<Item = String>) -> Self {
        self.exchanges = Some(exchanges.into_iter().collect());
        self
    }

    /// Sets [`symbol_types`](Self::symbol_types).
    pub fn with_symbol_types(mut self, types: impl IntoIterator<Item = SymbolType>) -> Self {
        self.symbol_types = Some(types.into_iter().collect());
        self
    }

    /// Sets [`services`](Self::services).
    pub fn with_services(mut self, services: impl IntoIterator<Item = FeedService>) -> Self {
        self.services = Some(services.into_iter().collect());
        self
    }

    /// Whether to offer the controls of `service`: it is listed, or the feed reports no list.
    pub fn offers(&self, service: FeedService) -> bool {
        self.services
            .as_ref()
            .is_none_or(|services| services.contains(&service))
    }

    /// Whether `service` is explicitly reported for `instrument`. Its own list takes precedence;
    /// without one, the feed's list applies. A reported feed list must also include the service.
    /// With neither list reported, this returns false, unlike [`offers`](Self::offers).
    pub fn declares(&self, service: FeedService, instrument: &Instrument) -> bool {
        self.offers(service)
            && instrument
                .services
                .as_ref()
                .or(self.services.as_ref())
                .is_some_and(|services| services.contains(&service))
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
    /// Only bars that open strictly before this instant: the end of the range is exclusive.
    /// `None` is the initial load and asks for the latest bars.
    pub before: Option<DateTime<Utc>>,
    /// Only bars that open at or after this instant: the start of the range is inclusive. With
    /// `before` it bounds the range as `[since, before)`; answer the newest `count` bars of it.
    /// The chart sets it when it needs bars back to a time it does not hold, such as for
    /// `Chart::scroll_to_time` (requires `widget`). `None` bounds the range
    /// by `count` alone.
    pub since: Option<DateTime<Utc>>,
    /// Bars wanted. Fewer is fine; the chart asks again as the user scrolls back.
    pub count: usize,
    /// Include the instrument's extended trading hours, as well as regular hours.
    pub extended_hours: bool,
    /// The currency or unit the host chose, in which the bars are wanted, as
    /// [`Datafeed::resolve_converted`] described the symbol. `None` asks for the instrument's own.
    pub conversion: Option<String>,
}

/// One page of history. A page without bars ends scroll-back like `end_of_history` does, unless it
/// names where older bars continue with [`continue_before`](Self::continue_before), so a feed that
/// pages by time window can answer an empty window such as a weekend.
///
/// Trades streamed through [`Sink::push_tick`] extend the page's newest bar even when they arrive
/// first. That bar must count only trades the stream did not send, or the feed must hold those
/// trades until the page is answered. Completed page bars replace bars built from ticks.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
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
    /// On a page without bars: the window had none but older bars exist, and the chart asks
    /// again for the bars before this instant. It must be earlier than `request.before`, or
    /// the chart stops paging and reports [`FeedError::InvalidData`] instead of repeating the
    /// request. Ignored on a page with bars, which continues before its oldest bar.
    pub continue_before: Option<DateTime<Utc>>,
}

impl HistoryPage {
    /// A page of `bars`, the last one when `end_of_history`.
    pub fn new(bars: Vec<Bar>, end_of_history: bool) -> Self {
        Self {
            bars,
            end_of_history,
            continue_before: None,
        }
    }

    /// Sets [`continue_before`](Self::continue_before).
    pub fn with_continue_before(mut self, before: DateTime<Utc>) -> Self {
        self.continue_before = Some(before);
        self
    }
}

/// A request for one page of trades.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TradeRequest {
    /// Instrument identifier used by the host.
    pub symbol: String,
    /// Only trades strictly before this instant: the end of the range is exclusive. `None` is the
    /// initial load and asks for the latest trades.
    pub before: Option<DateTime<Utc>>,
    /// Only trades at or after this instant: the start of the range is inclusive; see
    /// [`HistoryRequest::since`]. `None` bounds the range by `count` alone.
    pub since: Option<DateTime<Utc>>,
    /// Trades wanted. Fewer is fine.
    pub count: usize,
    /// The currency or unit the host chose, in which the prices are wanted; see
    /// [`HistoryRequest::conversion`].
    pub conversion: Option<String>,
}

/// One page of trades. A page without trades ends scroll-back like `end_of_history` does, unless
/// it names where older trades continue with [`continue_before`](Self::continue_before).
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
    /// On a page without trades: older trades exist, and the chart asks again for those before
    /// this instant, which must be earlier than `request.before`; see
    /// [`HistoryPage::continue_before`].
    pub continue_before: Option<DateTime<Utc>>,
}

impl TradePage {
    /// A page of `trades`, the last one when `end_of_history`.
    pub fn new(trades: Vec<Tick>, end_of_history: bool) -> Self {
        Self {
            trades,
            end_of_history,
            continue_before: None,
        }
    }

    /// Sets [`continue_before`](Self::continue_before).
    pub fn with_continue_before(mut self, before: DateTime<Utc>) -> Self {
        self.continue_before = Some(before);
        self
    }
}

/// One page of recorded depth for a symbol, using the same price and time grid as live columns.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DepthRequest {
    /// Instrument requested.
    pub symbol: String,
    /// Columns opening strictly before this instant: the end of the range is exclusive. `None`
    /// is the initial load and asks for the latest columns.
    pub before: Option<DateTime<Utc>>,
    /// Columns opening at or after this instant: the start of the range is inclusive; see
    /// [`HistoryRequest::since`]. `None` bounds the range by `count` alone.
    pub since: Option<DateTime<Utc>>,
    /// Maximum columns wanted.
    pub count: usize,
    /// Cadence, row grouping and limits of the receiving store.
    pub config: DepthHistoryConfig,
    /// Price of one tick, finite and positive.
    pub tick_size: f64,
    /// The currency or unit the host chose, in which the prices are wanted; see
    /// [`HistoryRequest::conversion`].
    pub conversion: Option<String>,
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
            since: None,
            count,
            config,
            tick_size,
            conversion: None,
        }
    }
}

/// Recorded depth columns. Empty pages end paging, including when `end_of_history` is false,
/// unless they name where older columns continue with [`continue_before`](Self::continue_before).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DepthPage {
    /// Columns on the requested grid; held live columns win on overlap. Gaps remain gaps.
    pub columns: Vec<DepthColumn>,
    /// Nothing exists before the oldest returned column.
    pub end_of_history: bool,
    /// On a page without columns: older columns exist, and the chart asks again for those
    /// before this instant, which must be earlier than `request.before`; see
    /// [`HistoryPage::continue_before`].
    pub continue_before: Option<DateTime<Utc>>,
}

impl DepthPage {
    /// A page of `columns`, the last one when `end_of_history`.
    pub fn new(columns: Vec<DepthColumn>, end_of_history: bool) -> Self {
        Self {
            columns,
            end_of_history,
            continue_before: None,
        }
    }

    /// Sets [`continue_before`](Self::continue_before).
    pub fn with_continue_before(mut self, before: DateTime<Utc>) -> Self {
        self.continue_before = Some(before);
        self
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
    /// The currency or unit the host chose, in which the updates are wanted; see
    /// [`HistoryRequest::conversion`].
    pub conversion: Option<String>,
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
    /// Whether no older trades remain to be loaded: the feed reported the end, answered nothing
    /// new, does not offer trades or the trade store is full.
    pub trade_history_complete: bool,
    /// Whether no older depth columns remain to be loaded.
    pub depth_history_complete: bool,
    /// Most recent feed error, when present.
    pub last_error: Option<FeedError>,
    /// Bars and ticks dropped by validation: non-finite values, a high or low on the wrong side of
    /// the open and close, negative volume, a tick or history bar that would overflow its slot's
    /// volume, a time too far ahead of the feed's clock (see `Sink::set_server_time`), or pending
    /// bar slots evicted after overflow (also reported in `last_error`).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_services_require_a_report_and_respect_both_lists() {
        use FeedService::{Depth, OrderDepth, Trades};

        for (feed, symbol, service, expected) in [
            (None, None, Depth, false),
            (Some(vec![Depth]), None, Depth, true),
            (Some(vec![Depth]), Some(vec![]), Depth, false),
            (None, Some(vec![Depth]), Depth, true),
            (Some(vec![Trades]), Some(vec![Depth]), Depth, false),
            (
                Some(vec![Depth, OrderDepth]),
                Some(vec![Depth]),
                Depth,
                true,
            ),
            (
                Some(vec![Depth, OrderDepth]),
                Some(vec![Depth]),
                OrderDepth,
                false,
            ),
            (Some(vec![]), Some(vec![Depth]), Depth, false),
            (Some(vec![]), None, Depth, false),
            (None, Some(vec![]), Depth, false),
            (None, Some(vec![Trades]), Depth, false),
            (Some(vec![Depth]), Some(vec![Trades]), Depth, false),
            (Some(vec![Trades]), None, Trades, true),
        ] {
            let mut capabilities = FeedCapabilities::new();
            if let Some(services) = feed {
                capabilities = capabilities.with_services(services);
            }
            let mut instrument = Instrument::new("A");
            if let Some(services) = symbol {
                instrument = instrument.with_services(services);
            }
            assert_eq!(
                capabilities.declares(service, &instrument),
                expected,
                "feed {:?}, symbol {:?}, service {service:?}",
                capabilities.services,
                instrument.services,
            );
        }

        assert!(FeedCapabilities::new().offers(Depth));
        assert!(!FeedCapabilities::new().with_services([]).offers(Depth));
    }
}
