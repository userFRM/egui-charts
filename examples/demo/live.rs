//! Synthetic executions delivered through the same feed contract as host market data.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{
    CalendarEvent, CalendarQuery, Datafeed, FeedError, FeedStatus, HistoryPage, HistoryRequest,
    HotlistEntry, HotlistQuery, MetricPage, MetricPoint, MetricRequest, MetricSeries,
    MetricSubscription, NewsQuery, NewsStory, OptionsVolume, OptionsVolumeQuery, OptionsVolumeSink,
    Reply, ScreenerQuery, ScreenerRow, Subscription, SubscriptionId, SymbolDetails,
    aggregate_trades, resample,
};
use egui_charts::model::{
    Bar, BarClock, DataStatus, ExtendedHoursQuote, ExtendedSession, Instrument, Level1Quote,
    OrderSide, Quote, Tick, Timeframe,
};
use egui_charts::model::{Direction, Signal, SignalId, SignalShape};
use egui_charts::services::{Events, SignalEvent, Signals};
use egui_charts::widget::{Chart, SeriesId};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use super::market::Market;
use super::orders::Orders;
use super::replay::book_snapshot;
use crate::sampledata::{
    DemoSymbol, SAMPLE_LOGO, dividend_date, dividend_factor, latest_open_end, session_bars,
    us_equity_hours, us_equity_session, us_equity_zone,
};
use egui_charts::ui::timeframe_toolbar::SessionType;

const RETAINED_TRADES: usize = 100_000;

/// Bound initialization work in each scheduled frame, including retained-tick delivery.
const LOAD_BATCH: usize = 512;

/// Orders added or cancelled in the book after each trade.
const CHURN_PER_TRADE: usize = 6;

/// The instrument whose feed sends individual orders instead of aggregate depth.
pub(super) const ORDER_LEVEL_SYMBOL: &str = "ESZ2026";

/// The only demo symbol whose feed answers analyst price targets.
pub(super) const TARGETS_SYMBOL: &str = "SAMPLE";

/// Driven by the UI timer on both native and web; there is no background worker.
pub(super) struct LiveFeed(
    Mutex<Simulation>,
    Arc<egui_charts::services::OrderFlowDetectors>,
    Option<Arc<Mutex<super::feeds::DemoMarket>>>,
);

/// The inputs of the last comparison history a feed loaded; an unchanged key skips the reload.
#[derive(PartialEq, Eq)]
pub(super) struct ComparisonSnapshot {
    timeframe: Timeframe,
    clock: BarClock,
    extended_hours: bool,
    continuous: bool,
    adjusted: bool,
    /// The latest execution id.
    trade: u64,
    /// The open of the latest bar, or the time of the latest execution on trade-based intervals.
    open: DateTime<Utc>,
    /// The series held no bars, as after a restore or a replay window before the first bar.
    empty: bool,
}

struct Simulation {
    symbol: String,
    tick_size: f64,
    cents: bool,
    price_ticks: i64,
    random: u64,
    id: u64,
    /// The id of the first execution not yet handed to the time and sales panel.
    tape: u64,
    trades: VecDeque<Tick>,
    /// Native UTC minutes, including the executions still retained above.
    history: Vec<Bar>,
    /// Daily OHLCV before the retained minute window, without raw executions.
    daily_history: Vec<Bar>,
    volume_by_day: BTreeMap<chrono::NaiveDate, f64>,
    next_trade: DateTime<Utc>,
    retain_from: DateTime<Utc>,
    now: DateTime<Utc>,
    /// Generate retained executions through this initial clock before running live.
    initializing: Option<DateTime<Utc>>,
    /// The next retained execution to deliver to a new subscription.
    bootstrap: Option<usize>,
    history_requests: VecDeque<(HistoryRequest, Reply<HistoryPage>)>,
    paused: bool,
    continuous: bool,
    adjusted: bool,
    /// What this feed last loaded into its comparison series.
    comparison_snapshot: Option<ComparisonSnapshot>,
    /// The ex-date and price factor stay fixed for the life of the feed.
    dividend: Option<(DateTime<Utc>, f64)>,
    subscription: Option<Subscription>,
    /// Each subscription has its own book; one idle stream per symbol preserves reload history.
    streams: Vec<LiveFeed>,
    signals: Vec<Events<SignalEvent>>,
    options: Vec<(OptionsVolumeQuery, OptionsVolumeSink)>,
    metrics: Vec<MetricSubscription>,
    /// The order book of the subscription, around the last price it was shown.
    market: Option<Market>,
    capture: super::replay::Capture,
    /// The individual orders behind `market`, on [`ORDER_LEVEL_SYMBOL`] only.
    orders: Option<Orders>,
}

impl LiveFeed {
    pub(super) fn new(symbol: &str, now: DateTime<Utc>) -> Self {
        let mut feed = Self::generate(symbol, now);
        let market = super::feeds::DemoMarket::new(now, feed.fork());
        feed.2 = Some(Arc::new(Mutex::new(market)));
        feed
    }

    /// Builds an execution source without recursively owning another market.
    pub(super) fn generate(symbol: &str, now: DateTime<Utc>) -> Self {
        let spec = DemoSymbol::find(symbol);
        let tick_size = if spec.start_price < 10.0 {
            0.0001
        } else {
            0.01
        };
        let start = BarClock::new(Timeframe::Hour1).bar_open(now - TimeDelta::hours(2));
        let mut sim = Simulation {
            symbol: symbol.into(),
            tick_size,
            cents: false,
            price_ticks: (spec.start_price / tick_size).round() as i64,
            random: 0x5eed_u64.wrapping_mul(spec.seed),
            id: 0,
            tape: 0,
            trades: VecDeque::new(),
            history: Vec::new(),
            daily_history: crate::sampledata::daily_history(symbol, start - TimeDelta::days(7)),
            volume_by_day: BTreeMap::new(),
            next_trade: start,
            retain_from: start,
            now,
            initializing: Some(now),
            bootstrap: None,
            history_requests: VecDeque::new(),
            paused: false,
            continuous: true,
            adjusted: false,
            comparison_snapshot: None,
            dividend: DemoSymbol::has_hours(symbol)
                .then(|| (dividend_date(symbol, now), dividend_factor(symbol))),
            subscription: None,
            streams: Vec::new(),
            signals: Vec::new(),
            options: Vec::new(),
            metrics: Vec::new(),
            market: None,
            capture: super::replay::Capture::new(),
            orders: None,
        };
        // Older minutes need OHLCV only; generate executions just for the retained window.
        for minute in 0..7 * 24 * 60 {
            let time = start - TimeDelta::days(7) + TimeDelta::minutes(minute);
            let open = sim.price_ticks;
            sim.price_ticks = (open + (sim.random() % 101) as i64 - 50).max(1);
            let high = open.max(sim.price_ticks) + 25 + (sim.random() % 26) as i64;
            let low = (open.min(sim.price_ticks) - 25 - (sim.random() % 26) as i64).max(1);
            let volume = ((4_000 + sim.random() % 8_001) as f64
                * arrival_intensity(time.timestamp()))
            .round();
            sim.history.push(Bar::new(
                time,
                open as f64 * tick_size,
                high as f64 * tick_size,
                low as f64 * tick_size,
                sim.price_ticks as f64 * tick_size,
                volume,
            ));
            *sim.volume_by_day.entry(time.date_naive()).or_default() += volume;
        }
        Self(Mutex::new(sim), Arc::default(), None)
    }

    pub(super) fn detectors(&self) -> Arc<egui_charts::services::OrderFlowDetectors> {
        self.1.clone()
    }

    fn state(&self) -> MutexGuard<'_, Simulation> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn fork(&self) -> Self {
        let detectors = Arc::new(egui_charts::services::OrderFlowDetectors::new());
        detectors.set_settings(self.1.settings());
        let sim = self.state();
        Self(
            Mutex::new(Simulation {
                symbol: sim.symbol.clone(),
                tick_size: sim.tick_size,
                cents: sim.cents,
                price_ticks: sim.price_ticks,
                random: sim.random,
                id: sim.id,
                tape: sim.tape,
                trades: sim.trades.clone(),
                history: sim.history.clone(),
                daily_history: sim.daily_history.clone(),
                volume_by_day: sim.volume_by_day.clone(),
                next_trade: sim.next_trade,
                retain_from: sim.retain_from,
                now: sim.now,
                initializing: sim.initializing,
                bootstrap: None,
                history_requests: VecDeque::new(),
                paused: sim.paused,
                continuous: sim.continuous,
                adjusted: sim.adjusted,
                comparison_snapshot: None,
                dividend: sim.dividend,
                subscription: None,
                streams: Vec::new(),
                signals: Vec::new(),
                options: Vec::new(),
                metrics: Vec::new(),
                market: None,
                capture: super::replay::Capture::new(),
                orders: None,
            }),
            detectors,
            self.2.clone(),
        )
    }

    pub(super) fn view(&self, extended_hours: bool) -> (SessionType, bool) {
        let sim = self.state();
        (sim.session(extended_hours), sim.adjusted)
    }

    pub(super) fn set_symbol(&self, symbol: &str) {
        let mut sim = self.state();
        if sim.symbol != symbol {
            let paused = sim.paused;
            let continuous = sim.continuous;
            let adjusted = sim.adjusted;
            let cents = sim.cents;
            let options = std::mem::take(&mut sim.options);
            let source = sim
                .streams
                .iter()
                .find(|feed| feed.state().symbol == symbol)
                .map(LiveFeed::fork)
                .unwrap_or_else(|| {
                    self.2.as_ref().map_or_else(
                        || Self::generate(symbol, sim.now),
                        |market| {
                            market
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .feed(symbol, sim.now)
                        },
                    )
                });
            let streams = std::mem::take(&mut sim.streams);
            *sim = source
                .0
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sim.paused = paused;
            sim.continuous = continuous;
            sim.adjusted = adjusted;
            sim.set_price_unit(cents);
            sim.options = options;
            sim.streams = streams;
            for (query, sink) in &sim.options {
                sink.push(sim.options_volume(query));
            }
        }
    }

    pub(super) fn set_view(&self, session: SessionType, adjusted: bool) {
        let mut sim = self.state();
        sim.continuous = session == SessionType::Full24x7;
        sim.adjusted = adjusted;
        for feed in &sim.streams {
            if feed.state().subscription.is_none() {
                feed.set_view(session, adjusted);
            }
        }
    }

    /// Changes the synthetic source's price unit; sizes, times and integer tick positions stay put.
    pub(super) fn set_price_unit(&self, cents: bool) {
        let mut sim = self.state();
        sim.set_price_unit(cents);
        for feed in &sim.streams {
            if feed.state().subscription.is_none() {
                feed.set_price_unit(cents);
            }
        }
    }

    /// Each comparison owns a feed. Rebuild its history only when the source or view changes,
    /// or when restoring the chart cleared previously loaded bars.
    pub(super) fn refresh_comparison(&self, chart: &mut Chart, id: SeriesId) {
        // Seed an empty series from cached minutes before retained ticks finish loading.
        if self.loading() && !chart.bars(id).is_empty() {
            return;
        }
        let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
        let clock = chart.bar_clock();
        let extended_hours = chart.extended_hours();
        let mut key = self.comparison_snapshot(chart, id);
        let sim = self.state();
        if sim.comparison_snapshot.as_ref() == Some(&key) {
            return;
        }
        drop(sim);
        let bars = self.snapshot(timeframe, clock, extended_hours);
        chart.set_data(id, bars);
        key.empty = chart.bars(id).is_empty();
        self.state().comparison_snapshot = Some(key);
    }

    /// The source and view inputs shared by ordinary comparisons and arithmetic spreads.
    pub(super) fn comparison_snapshot(&self, chart: &Chart, id: SeriesId) -> ComparisonSnapshot {
        let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
        let clock = chart.bar_clock();
        let extended_hours = chart.extended_hours();
        let sim = self.state();
        ComparisonSnapshot {
            timeframe,
            clock: clock.clone(),
            extended_hours,
            continuous: sim.continuous,
            adjusted: sim.adjusted,
            trade: sim.id,
            open: if timeframe.is_trade_based() {
                sim.trades.back().map_or(sim.now, |trade| trade.ts)
            } else {
                clock.bar_open(sim.now)
            },
            empty: chart.bars(id).is_empty(),
        }
    }

    pub(super) fn snapshot(
        &self,
        timeframe: Timeframe,
        clock: BarClock,
        extended_hours: bool,
    ) -> Vec<Bar> {
        self.snapshot_before(timeframe, clock, extended_hours, None)
    }

    /// History at an exclusive replay cutoff, or the full live source when it is absent.
    pub(super) fn snapshot_before(
        &self,
        timeframe: Timeframe,
        clock: BarClock,
        extended_hours: bool,
        before: Option<DateTime<Utc>>,
    ) -> Vec<Bar> {
        let sim = self.state();
        let now = before.map_or(sim.now, |time| time.min(sim.now));
        let minutes = timeframe.duration_ms() >= 60_000 && timeframe.duration_ms() % 60_000 == 0;
        // The generated ex-date and exchange windows align to UTC minutes. Historical
        // sub-minute exchange offsets retain the execution path for exact boundaries.
        let aligned = sim
            .dividend
            .is_none_or(|(time, _)| time.timestamp().rem_euclid(60) == 0);
        let mut bars = if minutes && aligned {
            let mut bars = sim.shown_history(&sim.history, extended_hours);
            if let Some(time) = before {
                bars.retain(|bar| bar.time + TimeDelta::minutes(1) <= time);
            }
            sim.resampled_history(&bars, timeframe, clock.clone(), sim.adjusted, before)
        } else {
            let trades: Vec<_> = sim
                .trades
                .iter()
                .take_while(|trade| before.is_none_or(|time| trade.ts < time))
                .filter_map(|trade| sim.shown_trade(trade, extended_hours))
                .collect();
            if minutes {
                let mut bars =
                    sim.shown_history(&sim.history[..sim.archived_end()], extended_hours);
                if let Some(time) = before {
                    bars.retain(|bar| bar.time + TimeDelta::minutes(1) <= time);
                }
                bars.extend(
                    aggregate_trades(&trades, Timeframe::Min1).expect("generated executions"),
                );
                sim.resampled_history(&bars, timeframe, clock.clone(), sim.adjusted, before)
            } else {
                egui_charts::data::aggregate_trades_with_tick_size(
                    &trades,
                    clock.clone(),
                    Some(sim.tick_size),
                )
                .expect("generated executions")
            }
        };
        let open = clock.bar_open(now);
        if sim.initializing.is_none()
            && timeframe.duration_ms() > 0
            && sim
                .shown_trade(
                    &Tick::new(now, sim.price_ticks as f64 * sim.tick_size, 0.0, None),
                    extended_hours,
                )
                .is_some()
            && let Some(last) = bars.last()
            && last.time < open
        {
            bars.push(Bar::new(
                open, last.close, last.close, last.close, last.close, 0.0,
            ));
        }
        bars
    }

    pub(super) fn read_recording(&self, events: &mut Vec<egui_charts::data::ReplayEvent>) {
        let sim = self.state();
        if let Some(feed) = sim.streams.iter().find(|feed| {
            let source = feed.state();
            source.symbol == sim.symbol
                && source.cents == sim.cents
                && source.adjusted == sim.adjusted
        }) {
            feed.read_recording(events);
        } else {
            sim.capture.read_into(events);
        }
    }

    pub(super) fn instrument(&self) -> Instrument {
        let sim = self.state();
        let spec = DemoSymbol::find(&sim.symbol);
        let mut instrument = Instrument::new(&sim.symbol)
            .with_description(format!("{} (synthetic)", spec.company_name))
            .with_exchange(spec.exchange)
            .with_currency("USD")
            .with_price_decimals(if sim.tick_size < 0.01 { 4 } else { 2 })
            .with_tick_size(sim.tick_size)
            .with_services([egui_charts::model::FeedService::Depth])
            .with_native_timeframes([Timeframe::Min1, Timeframe::Day1]);
        if sim.cents {
            instrument.currency = None;
            instrument.unit = Some("US cents".into());
        }
        if DemoSymbol::has_hours(&sim.symbol) && !sim.continuous {
            instrument = instrument
                .with_time_zone(us_equity_zone())
                .with_session(us_equity_session());
        }
        if sim.symbol == "SAMPLE" {
            instrument = instrument.with_logo(SAMPLE_LOGO);
        }
        // Index quotes are commonly licensed delayed; the legend marks them.
        if sim.symbol == "SPX" {
            instrument = instrument.with_data_status(DataStatus::Delayed);
        }
        instrument
    }

    /// The executions the subscription received since the last call, at most the latest 10,000,
    /// for the time and sales panel. A new symbol starts with its retained executions.
    pub(super) fn drain_tape(&self) -> Vec<Tick> {
        let mut sim = self.state();
        let Some(extended_hours) = sim.extended_hours() else {
            return Vec::new();
        };
        let start = sim
            .trades
            .partition_point(|trade| trade.id < Some(sim.tape))
            .max(sim.trades.len().saturating_sub(10_000));
        let tape = sim
            .trades
            .range(start..)
            .filter_map(|trade| sim.shown_trade(trade, extended_hours))
            .collect();
        sim.tape = sim.id;
        tape
    }

    /// The last displayed trade and previous session close, on the chart's session and price basis.
    pub(super) fn day_change(&self) -> Option<(f64, f64)> {
        let sim = self.state();
        let extended = sim.extended_hours().unwrap_or(false);
        let latest =
            sim.trades.back().map_or(sim.now, |trade| trade.ts) + TimeDelta::nanoseconds(1);
        let (time, last) = sim.newest_shown(latest, extended)?;
        let midnight = time.date_naive().and_hms_opt(0, 0, 0)?.and_utc();
        let (_, prev_close) = sim.newest_shown(midnight, extended)?;
        Some((last, prev_close))
    }

    /// The last regular-hours price with its time and the regular close of the day before, and a
    /// later trade in an extended window: pre-market before the open, post-market after the close.
    /// `None` for symbols without trading hours.
    pub(super) fn regular_quote(
        &self,
    ) -> Option<(DateTime<Utc>, f64, f64, Option<ExtendedHoursQuote>)> {
        let sim = self.state();
        if !DemoSymbol::has_hours(&sim.symbol) {
            return None;
        }
        let latest =
            sim.trades.back().map_or(sim.now, |trade| trade.ts) + TimeDelta::nanoseconds(1);
        let newest = |before, extended| {
            sim.newest(
                before,
                if extended {
                    SessionType::ETH
                } else {
                    SessionType::RTH
                },
            )
        };
        let (time, last) = newest(latest, false)?;
        let midnight = time.date_naive().and_hms_opt(0, 0, 0)?.and_utc();
        let (_, prev_close) = newest(midnight, false)?;
        let extended = newest(latest, true)
            .filter(|&(t, _)| t > time)
            .map(|(time, last)| ExtendedHoursQuote {
                session: if time.with_timezone(&us_equity_zone()).time()
                    < us_equity_session().windows[0].open
                {
                    ExtendedSession::PreMarket
                } else {
                    ExtendedSession::PostMarket
                },
                last,
                time,
            });
        Some((time, last, prev_close, extended))
    }

    /// The quote of the chart's symbol: the last displayed trade and the previous session close,
    /// or for a symbol with trading hours its regular-hours price and the extended-hours trade
    /// after it, with the session volume and the book's best bid and ask.
    pub(super) fn quote(&self, now: DateTime<Utc>) -> Option<Quote> {
        let mut quote = {
            let sim = self.state();
            Quote::new(
                sim.price_ticks as f64 * sim.tick_size,
                catalog(&sim.symbol)?.start_price * if sim.cents { 100.0 } else { 1.0 },
                now,
            )
        };
        if let Some((last, prev_close)) = self.day_change() {
            (quote.last, quote.prev_close) = (last, prev_close);
        }
        if let Some((time, last, prev_close, extended)) = self.regular_quote() {
            (quote.time, quote.last, quote.prev_close) = (time, last, prev_close);
            quote.extended = extended;
        }
        quote.volume = Some(self.daily_volume().0);
        if let Some(book) = self.state().quote(now) {
            quote = quote.with_bid_ask(book.bid, book.ask);
        }
        Some(quote)
    }

    /// The same retained execution timeline used when a catalog symbol opens in a chart.
    pub(super) fn market_quote(&self, symbol: &str, now: DateTime<Utc>) -> Option<Quote> {
        let active = self.state().symbol.eq_ignore_ascii_case(symbol);
        if active {
            return self.quote(now);
        }
        self.2
            .as_ref()?
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quote(symbol, now)
    }

    /// The volume traded since midnight UTC and the average volume of the earlier whole days.
    /// The synthetic history spans a week, which stands in for the 30 days of the average.
    pub(super) fn daily_volume(&self) -> (f64, f64) {
        let sim = self.state();
        let date = sim.now.date_naive();
        let today = sim.volume_by_day.get(&date).copied().unwrap_or(0.0);
        let first = if sim.archived_end() == 0 {
            sim.trades.front().map(|trade| trade.ts)
        } else {
            sim.history.first().map(|bar| bar.time)
        };
        // Leave out an incomplete first day.
        let skip_partial =
            usize::from(first.is_some_and(|first| first.time() != chrono::NaiveTime::MIN));
        let (sum, count) = sim
            .volume_by_day
            .iter()
            .filter(|(day, _)| **day != date)
            .skip(skip_partial)
            .fold((0.0, 0_usize), |(sum, count), (_, volume)| {
                (sum + volume, count + 1)
            });
        let average = sum / count.max(1) as f64;
        (today, average)
    }

    pub(super) fn paused(&self) -> bool {
        self.state().paused
    }

    pub(super) fn set_paused(&self, paused: bool) {
        self.state().paused = paused;
    }

    /// The simulation clock, which stands still while the feed is paused.
    pub(super) fn now(&self) -> DateTime<Utc> {
        self.state().now
    }

    /// Retained history is still being generated or delivered to an active chart.
    pub(super) fn loading(&self) -> bool {
        let sim = self.state();
        sim.initializing.is_some()
            || sim.bootstrap.is_some()
            || !sim.history_requests.is_empty()
            || sim.streams.iter().any(LiveFeed::loading)
            || self
                .2
                .as_ref()
                .is_some_and(|market| market.lock().unwrap_or_else(|e| e.into_inner()).loading())
    }

    /// Advance only when called by the scheduled frame. Pausing freezes simulation time.
    pub(super) fn advance(&self, now: DateTime<Utc>) -> std::time::Duration {
        let mut sim = self.state();
        let mut to_stream = self.2.as_ref().map_or(std::time::Duration::MAX, |market| {
            market
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .advance(if sim.paused { sim.now } else { now })
        });
        sim.streams.retain(|feed| {
            feed.state()
                .subscription
                .as_ref()
                .is_none_or(|sub| sub.sink.is_active())
        });
        for feed in &sim.streams {
            feed.set_paused(sim.paused);
            to_stream = to_stream.min(feed.advance(now));
        }
        let mut loading = false;
        if let Some(until) = sim.initializing {
            for _ in 0..LOAD_BATCH {
                if sim.next_trade > until {
                    sim.initializing = None;
                    break;
                }
                sim.trade();
            }
            loading = true;
        }
        if sim.initializing.is_none()
            && let Some(start) = sim.bootstrap
        {
            let sub = sim.subscription.take().expect("initializing subscription");
            let adjusted = matches!(
                sub.conversion.as_deref(),
                Some("adjusted" | "cents-adjusted")
            );
            let retained_from = sim.trades.front().map(|trade| sub.clock.bar_open(trade.ts));
            if start == 0 {
                if let Some(last) = sim.shown_ticks(sim.now, sub.extended_hours) {
                    let market = Market::new(&sim.symbol, sim.tick_size, last, sim.random);
                    sim.orders =
                        order_level(&sim.symbol, &market, sim.now, sim.tick_size, sim.random);
                    push_snapshot(&sub, &market, sim.orders.as_ref(), sim.now);
                    sub.sink.push_quote(market.quote(sim.now));
                    sim.market = Some(market);
                }
                let since = sim.history.first().map_or(sim.now, |bar| {
                    sub.clock.bar_open(bar.time - TimeDelta::days(1))
                });
                for bar in sim.history_bars(
                    sub.timeframe,
                    sub.clock.clone(),
                    sub.extended_hours,
                    adjusted,
                    false,
                ) {
                    // Rebuild overlapping archive prefixes after delivering their executions.
                    if bar.time >= since
                        && retained_from.is_none_or(|from| sub.clock.bar_open(bar.time) < from)
                    {
                        sub.sink.push_bar(bar);
                    }
                }
            }
            let end = (start + LOAD_BATCH).min(sim.trades.len());
            let trades: Vec<_> = sim
                .trades
                .range(start..end)
                .filter_map(|trade| sim.shown_trade(trade, sub.extended_hours))
                .collect();
            if let Some(detectors) = &sim.capture.detectors {
                detectors.push_history(&sim.symbol, &trades);
            }
            let last = trades.last().map(|trade| trade.ts);
            for trade in trades {
                sub.sink.push_tick(trade);
            }
            sim.bootstrap = Some(end);
            if sim.initializing.is_none() && end == sim.trades.len() {
                sim.bootstrap = None;
                // Complete the boundary and bars outside a small latest page after their executions arrive.
                if let Some(from) = retained_from {
                    for bar in sim.history_bars(
                        sub.timeframe,
                        sub.clock.clone(),
                        sub.extended_hours,
                        adjusted,
                        true,
                    ) {
                        if sub.clock.bar_open(bar.time) >= from {
                            sub.sink.push_bar(bar);
                        }
                    }
                }
                sub.sink.advance_time(sim.now);
                if let Some(market) = &sim.market {
                    let symbol = sim.symbol.clone();
                    let time = sim.now;
                    let event = book_snapshot(market, sim.orders.as_ref(), time);
                    let quote = market.quote(time);
                    sim.capture.push(&symbol, time, event);
                    sim.capture
                        .push(&symbol, time, egui_charts::data::MarketEvent::Quote(quote));
                }
                sub.sink.set_status(FeedStatus::Live);
            } else if sub.timeframe.duration_ms() > 0
                && let Some(last) = last
                && sim
                    .newest(
                        sim.now + TimeDelta::nanoseconds(1),
                        sim.session(sub.extended_hours),
                    )
                    .is_some_and(|(newest, _)| sub.clock.same_bar(last, newest))
                && let Some(bar) = sim
                    .history_bars(
                        sub.timeframe,
                        sub.clock.clone(),
                        sub.extended_hours,
                        adjusted,
                        true,
                    )
                    .pop()
            {
                // Old executions in the newest slot already belong to its complete history bar.
                sub.sink.push_bar(bar);
            }
            sim.subscription = Some(sub);
            loading = true;
        }
        if sim.initializing.is_none()
            && sim.bootstrap.is_none()
            && let Some((request, reply)) = sim.history_requests.pop_front()
        {
            reply.send(Ok(sim.history_page(&request)));
            return std::time::Duration::ZERO;
        }
        if loading || to_stream.is_zero() {
            return std::time::Duration::ZERO;
        }
        if sim.paused {
            return std::time::Duration::from_secs(1);
        }
        let empty = sim.subscription.as_ref().and_then(|sub| {
            if sub.timeframe.duration_ms() <= 0
                || sim.shown_ticks(sim.now, sub.extended_hours).is_none()
            {
                return None;
            }
            let open = sub.clock.bar_open(sim.now);
            sim.newest(
                sim.now + TimeDelta::nanoseconds(1),
                sim.session(sub.extended_hours),
            )
            .is_some_and(|(time, _)| sub.clock.bar_open(time) < open)
            .then_some(open)
        });
        let trade_id = sim.id;
        let metric_changed = sim.now.timestamp().div_euclid(300) != now.timestamp().div_euclid(300);
        if metric_changed {
            sim.metrics.retain(|sub| sub.sink.is_active());
            for sub in &sim.metrics {
                sub.sink.set_server_time(now);
                sub.sink.push_metric(open_interest(now));
            }
        }
        let second_changed = sim.now.timestamp() != now.timestamp();
        sim.now = now;
        if let Some(sub) = &sim.subscription {
            sub.sink.set_server_time(now);
        }
        // After a suspended browser tab, discard the elapsed silence rather than replaying it.
        if now - sim.next_trade > TimeDelta::seconds(5) {
            sim.next_trade = now;
        }
        while sim.next_trade <= now {
            sim.trade();
        }
        if sim.id != trade_id
            && let (Some(time), Some(sub)) = (empty, &sim.subscription)
            && let Some(bar) = sim
                .history_bars(
                    sub.timeframe,
                    sub.clock.clone(),
                    sub.extended_hours,
                    sim.adjusted,
                    true,
                )
                .into_iter()
                .find(|bar| sub.clock.same_bar(bar.time, time))
        {
            // The first execution replaces the carried open of an empty latest-page candle.
            sub.sink.push_bar(bar);
        }
        if second_changed {
            sim.options.retain(|(_, sink)| sink.is_active());
            for (query, sink) in &sim.options {
                sink.push(sim.options_volume(query));
            }
        }
        if second_changed && let Some(sub) = &sim.subscription {
            sub.sink.set_server_time(now);
            if sim
                .shown_trade(
                    &Tick::new(now, sim.price_ticks as f64 * sim.tick_size, 0.0, None),
                    sub.extended_hours,
                )
                .is_some()
            {
                sub.sink.advance_time(now);
                if let Some(quote) = sim.quote(now) {
                    let sink = sub.sink.clone();
                    let symbol = sim.symbol.clone();
                    sim.capture.push(
                        &symbol,
                        now,
                        egui_charts::data::MarketEvent::Quote(quote.clone()),
                    );
                    sink.push_quote(quote);
                }
            }
        }
        let to_trade = (sim.next_trade - now).to_std().unwrap_or_default();
        let to_second = std::time::Duration::from_millis(
            (1000 - now.timestamp_millis().rem_euclid(1000)) as u64,
        );
        to_trade.min(to_second).min(to_stream)
    }
}

/// The catalog entry of `symbol`, in any case.
fn catalog(symbol: &str) -> Option<&'static DemoSymbol> {
    DemoSymbol::catalog()
        .iter()
        .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
}

/// Decimals of the prices of a catalog symbol.
pub(super) fn price_decimals(symbol: &str) -> Option<u8> {
    Some(if catalog(symbol)?.start_price < 10.0 {
        4
    } else {
        2
    })
}

impl Simulation {
    fn set_price_unit(&mut self, cents: bool) {
        if self.cents == cents {
            return;
        }
        let factor = if cents { 100.0 } else { 0.01 };
        self.cents = cents;
        self.tick_size *= factor;
        for bar in self.history.iter_mut().chain(&mut self.daily_history) {
            bar.open *= factor;
            bar.high *= factor;
            bar.low *= factor;
            bar.close *= factor;
        }
        for trade in &mut self.trades {
            trade.price *= factor;
        }
        self.market = None;
        self.orders = None;
        self.capture = super::replay::Capture::new();
        self.comparison_snapshot = None;
    }

    fn history_page(&self, request: &HistoryRequest) -> HistoryPage {
        let recent = request.before.is_none() && self.initializing.is_none();
        let mut bars = self.history_bars(
            request.timeframe,
            request.clock.clone(),
            request.extended_hours,
            matches!(
                request.conversion.as_deref(),
                Some("adjusted" | "cents-adjusted")
            ),
            recent,
        );
        let open = request.clock.bar_open(self.now);
        if recent
            && request.timeframe.duration_ms() > 0
            && (!request.clock.has_session()
                || self.dividend.is_none()
                || us_equity_hours().is_open(self.now, request.extended_hours))
            && let Some(last) = bars.last()
            && last.time < open
        {
            bars.push(Bar::new(
                open, last.close, last.close, last.close, last.close, 0.0,
            ));
        }
        let cents = request
            .conversion
            .as_deref()
            .is_some_and(|c| c.starts_with("cents"));
        if cents != self.cents {
            let factor = if cents { 100.0 } else { 0.01 };
            for bar in &mut bars {
                bar.open *= factor;
                bar.high *= factor;
                bar.low *= factor;
                bar.close *= factor;
            }
        }
        super::feeds::history_page(&bars, request.since, request.before, request.count)
    }

    fn history_bars(
        &self,
        timeframe: Timeframe,
        clock: BarClock,
        extended_hours: bool,
        adjusted: bool,
        recent: bool,
    ) -> Vec<Bar> {
        let session = if !clock.has_session() {
            SessionType::Full24x7
        } else if extended_hours {
            SessionType::ETH
        } else {
            SessionType::RTH
        };
        if timeframe.duration_ms() < 60_000 || timeframe.duration_ms() % 60_000 != 0 {
            // Trade-based bars keep their grouping anchor in the execution stream.
            if !recent || timeframe.duration_ms() <= 0 {
                return Vec::new();
            }
            let hours = us_equity_hours();
            let trades: Vec<_> = self
                .trades
                .iter()
                .filter(|trade| {
                    self.dividend.is_none()
                        || session == SessionType::Full24x7
                        || hours.is_open(trade.ts, extended_hours)
                })
                .cloned()
                .map(|mut trade| {
                    if adjusted
                        && let Some((dividend, factor)) = self.dividend
                        && trade.ts < dividend
                    {
                        trade.price *= factor;
                    }
                    trade
                })
                .collect();
            return aggregate_trades(&trades, clock).expect("generated executions");
        }
        // Native minutes already fold the retained executions, including their unfinished minute.
        let end = if recent {
            self.history.len()
        } else {
            self.archived_end()
        };
        let source = &self.history[..end];
        let shown = if let Some((dividend, _)) = self.dividend {
            session_bars(&self.symbol, source, session, adjusted, dividend)
        } else {
            source.to_vec()
        };
        self.resampled_history(&shown, timeframe, clock, adjusted, None)
    }

    fn resampled_history(
        &self,
        minutes: &[Bar],
        timeframe: Timeframe,
        clock: BarClock,
        adjusted: bool,
        before: Option<DateTime<Utc>>,
    ) -> Vec<Bar> {
        if timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
            return resample(minutes, Timeframe::Min1, clock).expect("nested minute history");
        }
        let days: Vec<_> = self
            .daily_history
            .iter()
            .filter(|bar| {
                before.is_none_or(|time| {
                    bar.time
                        .date_naive()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        + TimeDelta::days(1)
                        <= time
                })
            })
            .cloned()
            .map(|mut bar| {
                if adjusted
                    && let Some((dividend, factor)) = self.dividend
                    && bar.time < dividend
                {
                    bar.open *= factor;
                    bar.high *= factor;
                    bar.low *= factor;
                    bar.close *= factor;
                }
                bar
            })
            .collect();
        crate::sampledata::calendar_history(&days, minutes, clock)
    }

    /// The displayed symbol's session, also when this is the host rather than its stream.
    fn extended_hours(&self) -> Option<bool> {
        self.subscription
            .as_ref()
            .map(|sub| sub.extended_hours)
            .or_else(|| {
                self.streams.iter().find_map(|feed| {
                    let sim = feed.state();
                    let sub = sim.subscription.as_ref()?;
                    (sub.symbol == self.symbol).then_some(sub.extended_hours)
                })
            })
    }

    fn options_volume(&self, query: &OptionsVolumeQuery) -> Vec<OptionsVolume> {
        if query.symbol != self.symbol || query.expiries.is_empty() {
            return Vec::new();
        }
        let price = self.price_ticks as f64 * self.tick_size;
        let step = 10_f64.powf((price.abs().max(1.0) * 0.002).log10().floor());
        let center = (price / step).round();
        let expiries = (*query.expiries.end() - *query.expiries.start()).num_days() as f64 + 1.0;
        (-12..=12)
            .map(|offset| {
                let weight = (13 - i32::abs(offset)) as f64;
                let live = (self.id % 100) as f64;
                OptionsVolume {
                    strike: (center + offset as f64) * step,
                    calls: expiries * (weight * (30 + offset) as f64 + live),
                    puts: expiries * (weight * (30 - offset) as f64 + live / 2.0),
                }
            })
            .collect()
    }

    fn session(&self, extended_hours: bool) -> SessionType {
        if self.continuous {
            SessionType::Full24x7
        } else if extended_hours {
            SessionType::ETH
        } else {
            SessionType::RTH
        }
    }

    fn archived_end(&self) -> usize {
        self.trades.front().map_or(self.history.len(), |first| {
            let open = BarClock::new(Timeframe::Min1).bar_open(first.ts);
            self.history.partition_point(|bar| bar.time < open)
        })
    }

    fn shown_history(&self, source: &[Bar], extended_hours: bool) -> Vec<Bar> {
        let Some((dividend, _)) = self.dividend else {
            return source.to_vec();
        };
        session_bars(
            &self.symbol,
            source,
            self.session(extended_hours),
            self.adjusted,
            dividend,
        )
    }

    /// The price at `time` on the chart's session and price basis, `None` outside the session.
    fn shown_price(&self, time: DateTime<Utc>, price: f64, extended_hours: bool) -> Option<f64> {
        let Some((dividend, factor)) = self.dividend else {
            return Some(price);
        };
        if !self.continuous && !us_equity_hours().is_open(time, extended_hours) {
            return None;
        }
        Some(if self.adjusted && time < dividend {
            price * factor
        } else {
            price
        })
    }

    fn shown_trade(&self, trade: &Tick, extended_hours: bool) -> Option<Tick> {
        let price = self.shown_price(trade.ts, trade.price, extended_hours)?;
        let mut trade = trade.clone();
        trade.price = price;
        Some(trade)
    }

    /// The newest trade or history close before `before` in `session`. Each closed candidate
    /// moves the bound to the preceding session, including when the stream has gaps.
    fn newest(
        &self,
        mut before: DateTime<Utc>,
        session: SessionType,
    ) -> Option<(DateTime<Utc>, f64)> {
        loop {
            let extended = session == SessionType::ETH;
            if session != SessionType::Full24x7 {
                before = latest_open_end(before, extended);
            }
            let trades = self.trades.partition_point(|trade| trade.ts < before);
            let history = &self.history[..self.archived_end()];
            let bars = history.partition_point(|bar| bar.time < before);
            let (time, price) = self
                .trades
                .range(..trades)
                .next_back()
                .map(|trade| (trade.ts, trade.price))
                .or_else(|| history[..bars].last().map(|bar| (bar.time, bar.close)))?;
            if session == SessionType::Full24x7 || us_equity_hours().is_open(time, extended) {
                return Some((time, price));
            }
            before = time;
        }
    }

    /// The newest price on the chart's session and dividend basis.
    fn newest_shown(
        &self,
        before: DateTime<Utc>,
        extended_hours: bool,
    ) -> Option<(DateTime<Utc>, f64)> {
        let session = if self.dividend.is_some() {
            self.session(extended_hours)
        } else {
            SessionType::Full24x7
        };
        let (time, price) = self.newest(before, session)?;
        Some((time, self.shown_price(time, price, extended_hours)?))
    }

    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    /// The best bid and ask of the subscription's book.
    fn quote(&self, ts: DateTime<Utc>) -> Option<Level1Quote> {
        self.market
            .as_ref()
            .map(|market| market.quote(ts))
            .or_else(|| {
                self.streams.iter().find_map(|feed| {
                    let sim = feed.state();
                    let market = sim.market.as_ref()?;
                    (sim.symbol == self.symbol && sim.cents == self.cents).then(|| market.quote(ts))
                })
            })
    }

    /// The price in ticks a trade at the current price would show at.
    fn shown_ticks(&self, now: DateTime<Utc>, extended_hours: bool) -> Option<i64> {
        let price = self.price_ticks as f64 * self.tick_size;
        let shown = self.shown_trade(&Tick::new(now, price, 0.0, None), extended_hours)?;
        Some((shown.price / self.tick_size).round() as i64)
    }

    fn trade(&mut self) {
        let random_step = (self.random() % 11) as i64 - 5;
        // Six rapid fills periodically sweep the order-level symbol, alternating direction.
        let sweep = self.symbol == ORDER_LEVEL_SYMBOL
            && self.id % 1_000 < 6
            && self
                .capture
                .detectors
                .as_ref()
                .is_some_and(|detectors| detectors.settings().stop_runs.enabled);
        let step = if sweep {
            if (self.id / 1_000).is_multiple_of(2) {
                1
            } else {
                -1
            }
        } else {
            random_step
        };
        self.price_ticks = (self.price_ticks + step).max(1);
        let u = (self.random() % 1000 + 1) as f64 / 1001.0;
        let size = (10.0 / u.sqrt()).round();
        let side = if step >= 0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let trade = Tick::new(
            self.next_trade,
            self.price_ticks as f64 * self.tick_size,
            size,
            Some(side),
        )
        .with_id(self.id);
        self.id += 1;
        *self.volume_by_day.entry(trade.ts.date_naive()).or_default() += trade.size;
        self.signals.retain(Events::is_active);
        if self.initializing.is_none() {
            for events in &self.signals {
                events.push(SignalEvent::Upsert(live_signal(trade.ts, trade.price)));
            }
        }

        if self.initializing.is_none()
            && let Some(sub) = &self.subscription
            && let Some(shown) = self.shown_trade(&trade, sub.extended_hours)
        {
            let price = (shown.price / self.tick_size).round() as i64;
            if self.capture.needs_checkpoint(trade.ts)
                && let Some(market) = &self.market
            {
                self.capture.record(
                    &self.symbol,
                    trade.ts,
                    book_snapshot(market, self.orders.as_ref(), trade.ts),
                );
            }
            self.capture.push(
                &self.symbol,
                shown.ts,
                egui_charts::data::MarketEvent::Trade(shown.clone()),
            );
            sub.sink.push_tick(shown);
            // A subscription made while no trade was shown gets its book with the first one.
            let market = self.market.get_or_insert_with(|| {
                let market = Market::new(&self.symbol, self.tick_size, price, self.random);
                self.orders =
                    order_level(&self.symbol, &market, trade.ts, self.tick_size, self.random);
                push_snapshot(sub, &market, self.orders.as_ref(), trade.ts);
                self.capture.push(
                    &self.symbol,
                    trade.ts,
                    book_snapshot(&market, self.orders.as_ref(), trade.ts),
                );
                market
            });
            let (sequence, delta) = market.trade(trade.ts, price, side, size);
            match &mut self.orders {
                Some(orders) => orders.apply(&sub.sink, delta, Some((side, price))),
                None => {
                    self.capture.push(
                        &self.symbol,
                        trade.ts,
                        egui_charts::data::MarketEvent::DepthDelta {
                            sequence,
                            book: delta.clone(),
                        },
                    );
                    sub.sink.push_depth_delta(sequence, delta);
                }
            }
            for _ in 0..CHURN_PER_TRADE {
                let (sequence, delta) = market.churn(trade.ts);
                match &mut self.orders {
                    Some(orders) => orders.apply(&sub.sink, delta, None),
                    None => {
                        self.capture.push(
                            &self.symbol,
                            trade.ts,
                            egui_charts::data::MarketEvent::DepthDelta {
                                sequence,
                                book: delta.clone(),
                            },
                        );
                        sub.sink.push_depth_delta(sequence, delta);
                    }
                }
            }
            if let Some(orders) = &mut self.orders {
                for (sequence, event) in orders.take_recorded() {
                    self.capture.push(
                        &self.symbol,
                        trade.ts,
                        egui_charts::data::MarketEvent::Order { sequence, event },
                    );
                }
            }
            let quote = market.quote(trade.ts);
            self.capture.push(
                &self.symbol,
                trade.ts,
                egui_charts::data::MarketEvent::Quote(quote.clone()),
            );
            sub.sink.push_quote(quote);
        }
        match self.history.last_mut() {
            Some(bar) if trade.ts >= bar.time && trade.ts - bar.time < TimeDelta::minutes(1) => {
                bar.high = bar.high.max(trade.price);
                bar.low = bar.low.min(trade.price);
                bar.close = trade.price;
                bar.volume += trade.size;
            }
            _ => self.history.push(Bar::new(
                BarClock::new(Timeframe::Min1).bar_open(trade.ts),
                trade.price,
                trade.price,
                trade.price,
                trade.price,
                trade.size,
            )),
        }
        if trade.ts >= self.retain_from {
            self.trades.push_back(trade);
        }
        if self.trades.len() > RETAINED_TRADES {
            // Archive whole minutes so replay never loses the beginning of a time bar.
            let clock = BarClock::new(Timeframe::Min1);
            let oldest = clock.bar_open(self.trades.front().unwrap().ts);
            let end = oldest + TimeDelta::minutes(1);
            while self.trades.front().is_some_and(|trade| trade.ts < end) {
                self.trades.pop_front();
            }
            let archived = self.archived_end();
            if archived > 10_000 {
                for bar in self.history.drain(..archived - 10_000) {
                    let dividend = self.dividend.map(|(time, _)| time);
                    if let Some(day) = self.daily_history.last_mut().filter(|day| {
                        day.time.date_naive() == bar.time.date_naive()
                            && dividend.is_none_or(|time| (day.time < time) == (bar.time < time))
                    }) {
                        day.high = day.high.max(bar.high);
                        day.low = day.low.min(bar.low);
                        day.close = bar.close;
                        day.volume += bar.volume;
                    } else {
                        self.daily_history.push(bar.clone());
                    }
                    let day = bar.time.date_naive();
                    if let Some(volume) = self.volume_by_day.get_mut(&day) {
                        *volume -= bar.volume;
                        if *volume <= 0.0 {
                            self.volume_by_day.remove(&day);
                        }
                    }
                }
            }
        }
        // Keep the price RNG sequence independent of the daily profile and clustered arrivals.
        let delay = 50
            + self.random() % 100
            + if self.random().is_multiple_of(13) {
                400
            } else {
                0
            };
        let intensity = arrival_intensity(self.next_trade.timestamp());
        let delay = if sweep {
            10
        } else {
            (delay as f64 / intensity).round().max(1.0) as i64
        };
        self.next_trade += TimeDelta::milliseconds(delay);
    }
}

/// The same daily activity and two-minute arrival clusters for bars and executions.
fn arrival_intensity(seconds: i64) -> f64 {
    let phase = seconds.rem_euclid(86_400) as f64 / 86_400.0 * std::f64::consts::TAU;
    let session = 1.0 + 0.6 * phase.cos();
    let mut burst = (seconds.div_euclid(120) as u64).wrapping_mul(0x9e3779b97f4a7c15);
    burst = (burst ^ (burst >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    session * (0.25 + 3.0 * ((burst >> 32) as f64 / u32::MAX as f64).powi(3))
}

/// The individual orders behind `market`, a new book of `symbol`, when its feed sends them.
fn order_level(
    symbol: &str,
    market: &Market,
    ts: DateTime<Utc>,
    tick_size: f64,
    seed: u64,
) -> Option<Orders> {
    (symbol == ORDER_LEVEL_SYMBOL).then(|| Orders::new(&market.snapshot(ts).1, tick_size, seed))
}

/// Pushes the whole book of `sub`: its individual orders when the feed sends them, otherwise
/// the aggregate depth. An aggregate push would discard the order queue.
fn push_snapshot(sub: &Subscription, market: &Market, orders: Option<&Orders>, ts: DateTime<Utc>) {
    match orders {
        Some(orders) => {
            let (sequence, orders) = orders.snapshot();
            sub.sink.push_order_snapshot(sequence, ts, orders);
        }
        None => {
            let (sequence, book) = market.snapshot(ts);
            sub.sink.push_depth_snapshot(sequence, book);
        }
    }
}

fn live_signal(time: DateTime<Utc>, price: f64) -> Signal {
    Signal {
        id: SignalId("live".into()),
        shape: SignalShape::Marker {
            time,
            price: Some(price),
            direction: Direction::Neutral,
        },
        category: "Live analytics".into(),
        label: Some("Host tick".into()),
        tooltip: Some("Synthetic analytics updated by the demo host.".into()),
    }
}

impl Signals for LiveFeed {
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
        if self.state().symbol != symbol {
            return;
        }
        self.1.subscribe(symbol, timeframe, events.clone());
        let bars = self.snapshot(timeframe, BarClock::new(timeframe), false);
        if let Some(last) = bars.last() {
            events.push(SignalEvent::Upsert(live_signal(last.time, last.close)));
            let first = &bars[bars.len().saturating_sub(20)];
            events.push(SignalEvent::Upsert(Signal {
                id: SignalId("zone".into()),
                shape: SignalShape::Zone {
                    from: first.time,
                    to: Some(last.time),
                    low: first.low,
                    high: first.high,
                },
                category: "Zones".into(),
                label: Some("Host zone".into()),
                tooltip: Some("A price zone supplied by the demo host.".into()),
            }));
            events.push(SignalEvent::Upsert(Signal {
                id: SignalId("level".into()),
                shape: SignalShape::Level {
                    price: first.high,
                    from: Some(first.time),
                },
                category: "Levels".into(),
                label: Some("Host level".into()),
                tooltip: Some("A level supplied by the demo host.".into()),
            }));
        }
        self.state().signals.push(events);
    }

    fn clicked(&self, id: &SignalId) {
        log::info!("Selected host signal: {}", id.0);
    }
}

fn metric_points(sim: &Simulation) -> Vec<MetricPoint> {
    let oldest = sim.history.first().map_or(sim.now, |bar| bar.time);
    let mut points: Vec<_> = (0..=(sim.now - oldest).num_seconds().max(0) / 300)
        .map(|i| open_interest(oldest + TimeDelta::seconds(i * 300)))
        .collect();
    // Include the developing five-minute point even when the oldest bar has an offset.
    let current = open_interest(sim.now);
    if points.last().is_none_or(|point| point.time != current.time) {
        points.push(current);
    }
    points
}

fn open_interest(time: DateTime<Utc>) -> MetricPoint {
    let slot = time.timestamp().div_euclid(300) * 300;
    MetricPoint {
        time: DateTime::from_timestamp(slot, 0).unwrap(),
        value: 100_000.0 + (slot as f64 / 3600.0).sin() * 8_000.0,
    }
}

impl Datafeed for LiveFeed {
    fn options_volume(&self, query: OptionsVolumeQuery, sink: OptionsVolumeSink) {
        let mut sim = self.state();
        sink.push(sim.options_volume(&query));
        sim.options.push((query, sink));
    }

    fn metrics(&self, symbol: &str, reply: Reply<Vec<MetricSeries>>) {
        let metrics = if symbol == self.state().symbol {
            vec![MetricSeries {
                id: "open_interest".into(),
                name: "Open interest".into(),
                unit: "contracts".into(),
                format: egui_charts::scales::StudyFormat::Volume,
            }]
        } else {
            Vec::new()
        };
        reply.send(Ok(metrics));
    }

    fn metric_history(&self, request: MetricRequest, reply: Reply<MetricPage>) {
        let sim = self.state();
        if request.symbol != sim.symbol || request.metric != "open_interest" {
            reply.send(Err(FeedError::NotSupported));
            return;
        }
        let mut points: Vec<_> = metric_points(&sim)
            .into_iter()
            .filter(|point| request.before.is_none_or(|before| point.time < before))
            .collect();
        let start = points.len().saturating_sub(request.count);
        reply.send(Ok(MetricPage {
            points: points.split_off(start),
            end_of_history: start == 0,
        }));
    }

    fn subscribe_metric(&self, subscription: MetricSubscription) {
        let mut sim = self.state();
        if subscription.symbol != sim.symbol || subscription.metric != "open_interest" {
            return;
        }
        subscription.sink.set_server_time(sim.now);
        subscription.sink.push_metric(open_interest(sim.now));
        sim.metrics.push(subscription);
    }

    fn price_targets(&self, symbol: &str, reply: Reply<egui_charts::data::PriceTargets>) {
        if symbol != TARGETS_SYMBOL {
            reply.send(Err(FeedError::NotSupported));
            return;
        }
        let sim = self.state();
        let close = sim.price_ticks as f64 * sim.tick_size;
        let published = sim.now.date_naive();
        reply.send(Ok(egui_charts::data::PriceTargets {
            high: close * 1.20,
            average: close * 1.08,
            low: close * 0.85,
            analysts: 24,
            published,
            horizon: published + TimeDelta::days(365),
        }));
    }

    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        self.set_symbol(symbol);
        self.set_price_unit(false);
        reply.send(Ok(self.instrument()));
    }

    fn resolve_converted(&self, symbol: &str, conversion: &str, reply: Reply<Instrument>) {
        if !matches!(conversion, "adjusted" | "cents" | "cents-adjusted") {
            reply.send(Err(FeedError::NotSupported));
            return;
        }
        self.set_symbol(symbol);
        self.set_price_unit(conversion.starts_with("cents"));
        reply.send(Ok(self.instrument()));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let mut sim = self.state();
        if let Some(feed) = sim
            .streams
            .iter()
            .find(|feed| feed.state().symbol == request.symbol)
        {
            feed.history(request, reply);
        } else if request.symbol != sim.symbol {
            let now = sim.now;
            sim.streams.push(LiveFeed::new(&request.symbol, now));
            sim.streams.last().unwrap().history(request, reply);
        } else if sim.subscription.is_none()
            || (request.before.is_none() && sim.initializing.is_none())
        {
            // Cached latest pages serve every chart while another subscription backfills executions.
            reply.send(Ok(sim.history_page(&request)));
        } else {
            // Other subscribed requests wait for complete tick bootstrap.
            sim.history_requests.push_back((request, reply));
        }
    }

    fn subscribe(&self, subscription: Subscription) {
        let feed = {
            let sim = self.state();
            sim.streams
                .iter()
                .find(|feed| feed.state().symbol == subscription.symbol)
                .map(|feed| {
                    let copy = feed.fork();
                    copy.state().history_requests =
                        std::mem::take(&mut feed.state().history_requests);
                    copy
                })
        };
        let feed = feed.unwrap_or_else(|| self.fork());
        feed.set_symbol(&subscription.symbol);
        let session = if !subscription.clock.has_session() {
            SessionType::Full24x7
        } else if subscription.extended_hours {
            SessionType::ETH
        } else {
            SessionType::RTH
        };
        feed.set_price_unit(
            subscription
                .conversion
                .as_deref()
                .is_some_and(|c| c.starts_with("cents")),
        );
        feed.set_view(
            session,
            matches!(
                subscription.conversion.as_deref(),
                Some("adjusted" | "cents-adjusted")
            ),
        );
        feed.set_paused(false);
        self.1
            .track(&feed.instrument())
            .expect("synthetic tick size");
        let mut sim = feed.state();
        subscription.sink.set_server_time(sim.now);
        sim.capture = super::replay::Capture::new();
        sim.capture.detectors = Some(self.1.clone());
        sim.bootstrap = Some(0);
        subscription.sink.set_status(FeedStatus::Connecting);
        sim.subscription = Some(subscription);
        drop(sim);
        let mut sim = self.state();
        sim.streams.retain(|other| {
            let other = other.state();
            other.subscription.is_some() || other.symbol != feed.state().symbol
        });
        sim.streams.push(feed);
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        let mut sim = self.state();
        sim.metrics.retain(|sub| sub.id != id);
        let Some(index) = sim.streams.iter().position(|feed| {
            feed.state()
                .subscription
                .as_ref()
                .is_some_and(|sub| sub.id == id)
        }) else {
            return;
        };
        let feed = sim.streams.remove(index);
        let mut source = feed.state();
        if sim
            .streams
            .iter()
            .any(|other| other.state().symbol == source.symbol)
        {
            return;
        }
        source.subscription = None;
        source.bootstrap = None;
        source.market = None;
        source.orders = None;
        source.continuous = sim.continuous;
        source.adjusted = sim.adjusted;
        drop(source);
        sim.streams.push(feed);
    }

    fn request_depth_snapshot(&self, id: SubscriptionId) {
        for feed in &self.state().streams {
            let sim = feed.state();
            if let (Some(sub), Some(market)) = (&sim.subscription, &sim.market)
                && sub.id == id
            {
                push_snapshot(sub, market, sim.orders.as_ref(), sim.now);
            }
        }
    }

    /// The catalog symbols ranked on their synthetic quotes.
    fn hotlist(&self, query: HotlistQuery, reply: Reply<Vec<HotlistEntry>>) {
        reply.send(Ok(super::feeds::hotlist(&query, self)));
    }

    /// The catalog symbols screened on their synthetic quotes.
    fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
        reply.send(super::feeds::screen(&query, self));
    }

    /// Synthetic releases, earnings, dividends and offerings.
    fn calendar(&self, query: CalendarQuery, reply: Reply<Vec<CalendarEvent>>) {
        reply.send(Ok(super::feeds::calendar(&query, Utc::now())));
    }

    /// Synthetic stories about the catalog symbols.
    fn news(&self, query: NewsQuery, reply: Reply<Vec<NewsStory>>) {
        reply.send(Ok(super::feeds::news(&query, Utc::now())));
    }

    /// Synthetic statistics, profile and category derived from the catalog entry of `symbol`.
    fn symbol_details(&self, symbol: &str, reply: Reply<SymbolDetails>) {
        let Some(spec) = DemoSymbol::catalog()
            .iter()
            .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
        else {
            reply.send(Err(FeedError::SymbolNotFound(symbol.into())));
            return;
        };
        let decimals = if spec.start_price < 10.0 { 4 } else { 2 };
        let price = |factor: f64| format!("{:.*}", decimals, spec.start_price * factor);
        let kind = match spec.exchange {
            "Crypto" => "Cryptocurrency",
            "Index" => "Index",
            "Forex" => "Currency pair",
            "CME" => "Futures",
            "Synthetic" => "Fund",
            _ => "Stock",
        };
        let pair = |label: &str, value: String| (label.to_owned(), value);
        reply.send(Ok(SymbolDetails::new(
            vec![pair(
                "52-week range",
                format!("{} - {}", price(0.82), price(1.17)),
            )],
            vec![pair(
                "Source",
                "Synthetic data generated by the demo".into(),
            )],
        )
        .with_url("https://github.com/userFRM/egui-charts/blob/main/examples/demo/sampledata.rs")
        .with_category(kind)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::feeds::DemoQuotes;
    use super::*;
    use egui_charts::ChartBuilder;
    use egui_charts::widget::SeriesId;
    use std::sync::Arc;

    fn ready_feed(symbol: &str, now: DateTime<Utc>) -> LiveFeed {
        let feed = LiveFeed::new(symbol, now);
        for _ in 0..10_000 {
            if !feed.loading() {
                return feed;
            }
            feed.advance(now);
        }
        panic!("synthetic history did not finish loading");
    }

    fn load_chart(feed: &LiveFeed, chart: &mut Chart, ctx: &egui::Context) {
        for _ in 0..4 {
            super::super::tests::finish_feed(feed);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
            if !feed.loading() {
                return;
            }
        }
        panic!("retained executions did not finish loading");
    }

    #[test]
    fn price_unit_changes_include_the_daily_archive() {
        let now = "2026-11-03T15:00:00Z".parse().unwrap();
        let feed = LiveFeed::generate("SAMPLE", now);
        let histories: Vec<_> = [Timeframe::Day1, Timeframe::Week1, Timeframe::Month1]
            .into_iter()
            .map(|timeframe| {
                let bars = feed.snapshot(timeframe, BarClock::new(timeframe), false);
                assert!(bars.len() > 300);
                (timeframe, bars)
            })
            .collect();
        for cents in [true, false] {
            feed.set_price_unit(cents);
            let factor = if cents { 100.0 } else { 1.0 };
            for (timeframe, raw) in &histories {
                let bars = feed.snapshot(*timeframe, BarClock::new(*timeframe), false);
                assert_eq!(raw.len(), bars.len());
                for (usd, converted) in raw.iter().zip(&bars) {
                    assert_eq!((usd.time, usd.volume), (converted.time, converted.volume));
                    for (usd, converted) in
                        [usd.open, usd.high, usd.low, usd.close].into_iter().zip([
                            converted.open,
                            converted.high,
                            converted.low,
                            converted.close,
                        ])
                    {
                        assert!((usd * factor - converted).abs() < 1e-7);
                    }
                }
            }
        }
    }

    #[test]
    fn cached_archive_is_visible_before_bootstrap_and_joins_retained_ticks() {
        use egui_charts::services::AlertFeed;

        let now = "2026-10-24T02:59:59Z".parse().unwrap();
        for (interval, initialized, count) in [
            ("1min", false, 20_000),
            ("45min", false, 20_000),
            ("1D", false, 20_000),
            ("1min", true, 20),
        ] {
            let timeframe = interval.parse().unwrap();
            let feed = Arc::new(if initialized {
                ready_feed("SAMPLE", now)
            } else {
                LiveFeed::new("SAMPLE", now)
            });
            feed.set_paused(true);
            let ctx = egui::Context::default();
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("SAMPLE")
                .with_timeframe(timeframe)
                .with_initial_bars(count)
                .build();
            ctx.run_ui(Default::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
            let comparison = chart.add_comparison("SAMPLE", egui::Color32::BLUE);
            feed.refresh_comparison(&mut chart, comparison);
            assert!(!chart.bars(SeriesId::MAIN).is_empty(), "{interval}");
            if !initialized {
                assert_eq!(chart.bars(SeriesId::MAIN), chart.bars(comparison));
                assert!(
                    feed.loading(),
                    "showing the archive must not finish loading"
                );
            }

            let mut alerts = AlertFeed::new(
                feed.clone(),
                AlertFeed::history_request("SAMPLE", timeframe),
                None,
            );
            for _ in 0..3 {
                assert!(alerts.poll(&ctx, now).is_empty());
                if !initialized {
                    assert!(alerts.bars().is_empty(), "alerts must wait for bootstrap");
                }
            }

            load_chart(&feed, &mut chart, &ctx);
            feed.refresh_comparison(&mut chart, comparison);
            assert!(alerts.poll(&ctx, now).is_empty());
            let expected = feed.snapshot(timeframe, chart.bar_clock(), chart.extended_hours());
            let actual = chart.bars(SeriesId::MAIN);
            assert_eq!(&actual[actual.len() - expected.len()..], expected);
            assert_eq!(chart.bars(comparison), expected);
            // Alert history requests 1,000 bars; subscription bootstrap must not add older days.
            let alerts = alerts.bars();
            assert_eq!(
                &alerts[alerts.len().saturating_sub(1000)..],
                &expected[expected.len().saturating_sub(1000)..]
            );
            let sim = feed.state();
            assert!(sim.trades.front().unwrap().ts > sim.retain_from);
            let open = BarClock::new(Timeframe::Min1).bar_open(now);
            let trades: Vec<_> = sim
                .trades
                .iter()
                .filter(|trade| trade.ts >= open)
                .cloned()
                .collect();
            assert!(!trades.is_empty());
            assert_eq!(
                chart
                    .trades(SeriesId::MAIN)
                    .filter(|trade| trade.ts >= open)
                    .cloned()
                    .collect::<Vec<_>>(),
                trades
            );
            if timeframe == Timeframe::Min1 {
                let last = chart.bars(SeriesId::MAIN).last().unwrap();
                assert_eq!(last.open, trades.first().unwrap().price);
                assert_eq!(last.close, trades.last().unwrap().price);
                assert_eq!(
                    last.high,
                    trades
                        .iter()
                        .map(|trade| trade.price)
                        .fold(f64::NEG_INFINITY, f64::max)
                );
                assert_eq!(
                    last.low,
                    trades
                        .iter()
                        .map(|trade| trade.price)
                        .fold(f64::INFINITY, f64::min)
                );
                assert_eq!(
                    last.volume,
                    trades.iter().map(|trade| trade.size).sum::<f64>()
                );
            }
        }
    }

    #[test]
    fn replay_exit_restores_each_charts_live_edge_before_tick_backfill() {
        let start = "2026-11-03T15:00:58Z".parse().unwrap();
        let feed = Arc::new(ready_feed("SAMPLE", start));
        feed.set_paused(true);
        let ctx = egui::Context::default();
        let frame = |chart: &mut Chart, width| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            )
        };
        let mut charts = Vec::new();
        for timeframe in [
            Timeframe::Min1,
            Timeframe::Min5,
            Timeframe::Day1,
            Timeframe::Sec1,
        ] {
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("SAMPLE")
                .with_timeframe(timeframe)
                .with_initial_bars(2_000)
                .build();
            chart.set_liquidity_heatmap(true);
            load_chart(&feed, &mut chart, &ctx);
            charts.push(chart);
        }
        let now = start + TimeDelta::seconds(1);
        feed.set_paused(false);
        feed.advance(now);
        feed.set_paused(true);
        let held: Vec<_> = charts
            .iter_mut()
            .map(|chart| {
                frame(chart, 1440.0).drop_without_applying_deltas();
                let last = chart.bars(SeriesId::MAIN).last().unwrap().clone();
                let trades: Vec<_> = chart.trades(SeriesId::MAIN).cloned().collect();
                let depth: Vec<_> = chart
                    .depth_history(SeriesId::MAIN)
                    .unwrap()
                    .columns()
                    .cloned()
                    .collect();
                assert!(
                    !depth.is_empty(),
                    "the last minute must have sampled liquidity"
                );
                (last, trades, depth)
            })
            .collect();
        let layout = egui_charts::widget::ChartLayout::default();
        layout.set_replay_time(
            &mut charts.iter_mut().collect::<Vec<_>>(),
            0,
            Some(now - TimeDelta::minutes(30)),
        );
        assert!(charts.iter().all(|chart| chart.replay_time().is_some()));
        layout.set_replay_time(&mut charts.iter_mut().collect::<Vec<_>>(), 0, None);
        // Resolve, load the latest pages and subscribe, without draining the backfill.
        for _ in 0..3 {
            for chart in &mut charts {
                frame(chart, 1440.0).drop_without_applying_deltas();
            }
        }
        feed.advance(now);
        for (chart, (last, trades, depth)) in charts.iter_mut().zip(&held) {
            for theme in [
                egui_charts::theme::Theme::light(),
                egui_charts::theme::Theme::dark(),
            ] {
                chart.set_theme(&ctx, &theme);
                for width in [360.0, 1440.0] {
                    let output = frame(chart, width);
                    assert_eq!(chart.bars(SeriesId::MAIN).last(), Some(last));
                    assert!(chart.order_book(SeriesId::MAIN).is_some());
                    assert!(
                        output.shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Text(text)
                                if text.galley.text() == "BUY"
                                    && shape.clip_rect.contains_rect(text.visual_bounding_rect()))
                        }),
                        "the current quote must be visible on the first resumed frames"
                    );
                    output.drop_without_applying_deltas();
                }
            }
            assert_eq!(
                chart.trades(SeriesId::MAIN).cloned().collect::<Vec<_>>(),
                *trades
            );
            assert_eq!(
                chart
                    .depth_history(SeriesId::MAIN)
                    .unwrap()
                    .columns()
                    .cloned()
                    .collect::<Vec<_>>(),
                *depth
            );
        }
        for _ in 0..RETAINED_TRADES.div_ceil(LOAD_BATCH) + 4 {
            feed.advance(now);
            for (chart, (last, _, _)) in charts.iter_mut().zip(&held) {
                frame(chart, 1440.0).drop_without_applying_deltas();
                assert_eq!(chart.bars(SeriesId::MAIN).last(), Some(last));
            }
            if !feed.loading() {
                break;
            }
        }
        assert!(!feed.loading());
        feed.set_paused(false);
        feed.advance(now + TimeDelta::milliseconds(500));
        for chart in &mut charts {
            frame(chart, 1440.0).drop_without_applying_deltas();
            let expected = feed.snapshot(
                chart.timeframe().unwrap(),
                chart.bar_clock(),
                chart.extended_hours(),
            );
            assert_eq!(chart.bars(SeriesId::MAIN).last(), expected.last());
        }
    }

    #[test]
    fn recordings_follow_symbols_and_survive_replay_pause() {
        let now = "2026-10-23T14:59:59Z".parse().unwrap();
        let feed = Arc::new(ready_feed("SAMPLE", now));
        let detectors = feed.detectors();
        let mut settings = detectors.settings();
        settings.liquidity.enabled = true;
        settings.liquidity.rule.threshold = egui_charts::services::Threshold::Fixed(10.0);
        detectors.set_settings(settings);
        let ctx = egui::Context::default();
        let symbols = ["SAMPLE", ORDER_LEVEL_SYMBOL];
        let mut charts = Vec::new();
        for symbol in symbols {
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol(symbol)
                .with_initial_bars(100)
                .build();
            load_chart(&feed, &mut chart, &ctx);
            charts.push(chart);
        }
        for stream in &feed.state().streams {
            let mut sim = stream.state();
            let symbol = sim.symbol.clone();
            let kind = egui_charts::services::DetectionKind::LargeLiquidity;
            let before = detectors.counts(&symbol, kind, now);
            assert!(before.total > 0);
            let event = book_snapshot(sim.market.as_ref().unwrap(), sim.orders.as_ref(), now);
            sim.capture.record(&symbol, now, event);
            assert_eq!(
                detectors.counts(&symbol, kind, now),
                before,
                "a recording checkpoint must not restart live liquidity detections"
            );
        }
        for second in 1..=420 {
            feed.advance(now + TimeDelta::seconds(second));
        }
        feed.set_paused(true);
        for (symbol, mut chart) in symbols.into_iter().zip(charts) {
            feed.set_symbol(symbol);
            let mut events = Vec::new();
            feed.read_recording(&mut events);
            assert!(matches!(
                events[0].event,
                egui_charts::data::MarketEvent::DepthSnapshot { .. }
                    | egui_charts::data::MarketEvent::OrderSnapshot { .. }
            ));
            assert!(events[0].time > now, "expired chunks must be removed");
            assert!(events.last().unwrap().time - events[0].time <= TimeDelta::minutes(5));
            let mut encoded = egui_charts::data::RecordingWriter::new(Vec::new()).unwrap();
            for event in &events {
                encoded.write(event).unwrap();
            }
            assert!(encoded.get_ref().len() <= 8 * 1024 * 1024);
            assert!(events.windows(2).all(|pair| (pair[0].time, pair[0].sequence) < (pair[1].time, pair[1].sequence)));
            let driver = Arc::new(
                egui_charts::data::ReplayDriver::new(events.clone(), vec![feed.instrument()])
                    .unwrap(),
            );
            driver.seek(events[0].time);
            let mut replay = ChartBuilder::new()
                .with_feed(driver)
                .with_symbol(symbol)
                .build();
            for _ in 0..4 {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    replay.show(ui);
                })
                .drop_without_applying_deltas();
            }
            let book = replay
                .order_book(SeriesId::MAIN)
                .expect("oldest retained point has a book");
            assert!(!book.bids.is_empty() && !book.asks.is_empty());
            let mut newest = None;
            for event in &events {
                assert_eq!(event.symbol, symbol);
                newest = Some(event.time);
            }
            assert!(
                newest.is_some_and(|time| time > now),
                "{symbol} must include recorded live updates"
            );
            chart.set_replay_time(Some(now));
            let mut retained = Vec::new();
            feed.read_recording(&mut retained);
            assert_eq!(
                retained, events,
                "{symbol} must retain its recording when replay unsubscribes"
            );
        }
    }

    /// Resolving and subscribing another symbol must leave the existing histories and streams
    /// on their own symbols and sessions, including reloads across an hour or the regular close.
    #[test]
    fn subscriptions_keep_their_symbols_after_a_third_symbol_resolves() {
        for (now, session) in [
            ("2026-10-23T14:59:59Z", SessionType::Full24x7),
            ("2026-10-23T19:59:59Z", SessionType::ETH),
        ] {
            let now = now.parse().unwrap();
            let feed = Arc::new(ready_feed("SAMPLE", now));
            feed.set_view(session, false);
            let ctx = egui::Context::default();
            let frame = |chart: &mut egui_charts::Chart| {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    chart.show(ui);
                })
                .drop_without_applying_deltas();
            };
            let mut charts = Vec::new();
            let mut references = Vec::new();
            for (symbol, extended) in [
                ("SAMPLE", false),
                ("EURUSD", false),
                (ORDER_LEVEL_SYMBOL, false),
                ("SAMPLE", true),
            ] {
                let mut chart = ChartBuilder::new()
                    .with_feed(feed.clone())
                    .with_symbol(symbol)
                    .with_initial_bars(100)
                    .build();
                chart.set_extended_hours(extended);
                load_chart(&feed, &mut chart, &ctx);
                charts.push(chart);
                let reference = ready_feed(symbol, now);
                reference.set_view(session, false);
                references.push(reference);
            }
            for seconds in [2, 4, 6, 8, 10, 12] {
                if seconds == 12 {
                    charts.pop();
                    references.pop();
                } else if seconds >= 4 {
                    let chart = &mut charts[(seconds / 2 - 2) as usize];
                    chart.reload_feed();
                    load_chart(&feed, chart, &ctx);
                }
                let at = now + TimeDelta::seconds(seconds);
                feed.advance(at);
                for (chart, reference) in charts.iter_mut().zip(&references) {
                    reference.advance(at);
                    frame(chart);
                    let expected = reference.snapshot(
                        chart.timeframe().unwrap(),
                        chart.bar_clock(),
                        chart.extended_hours(),
                    );
                    let actual = chart.bars(SeriesId::MAIN);
                    assert!(!actual.is_empty(), "{}", chart.symbol());
                    assert_eq!(
                        actual,
                        &expected[expected.len() - actual.len()..],
                        "{} at {at}",
                        chart.symbol()
                    );
                    let state = chart.feed_state(SeriesId::MAIN).unwrap();
                    assert_eq!(state.rejected_depth, 0, "{}", chart.symbol());
                    assert_eq!(
                        chart.order_queue(SeriesId::MAIN).is_some(),
                        chart.symbol() == ORDER_LEVEL_SYMBOL
                    );
                }
            }
            drop(charts);
            for seconds in [14, 16, 18] {
                let at = now + TimeDelta::seconds(seconds);
                feed.advance(at);
                for reference in &references {
                    reference.advance(at);
                }
            }
            for reference in references {
                let mut chart = ChartBuilder::new()
                    .with_feed(feed.clone())
                    .with_symbol(reference.instrument().symbol)
                    .with_initial_bars(100)
                    .build();
                load_chart(&feed, &mut chart, &ctx);
                let expected = reference.snapshot(
                    chart.timeframe().unwrap(),
                    chart.bar_clock(),
                    chart.extended_hours(),
                );
                let actual = chart.bars(SeriesId::MAIN);
                assert!(!actual.is_empty(), "{} after reopening", chart.symbol());
                assert_eq!(
                    actual,
                    &expected[expected.len() - actual.len()..],
                    "{} after reopening",
                    chart.symbol()
                );
            }
        }
    }

    #[test]
    fn options_queries_resume_after_switching_back_to_the_underlying() {
        use egui_charts::studies::{Indicator, OptionsVolumeProfile};

        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = ready_feed("SAMPLE", now);
        let expiry = now.date_naive() + TimeDelta::days(30);
        let study = OptionsVolumeProfile::new(
            &feed,
            OptionsVolumeQuery {
                symbol: "SAMPLE".into(),
                expiries: expiry..=expiry,
            },
            1.0,
            || {},
        )
        .unwrap();
        assert!(!study.volume_profiles(0..10).is_empty());
        for (seconds, symbol, shown) in [(1, "AAPL", false), (2, "SAMPLE", true)] {
            feed.set_symbol(symbol);
            feed.advance(now + TimeDelta::seconds(seconds));
            assert_eq!(study.volume_profiles(0..10).is_empty(), !shown, "{symbol}");
        }
    }

    /// A trade after the regular close, at 17:30 New York time, is on the tape only with extended
    /// hours: the window closes at 20:00, not at the regular close.
    #[test]
    fn a_post_market_trade_is_shown_only_with_extended_hours() {
        let now = "2026-10-23T22:00:00Z".parse().unwrap();
        let feed = ready_feed("SAMPLE", now);
        feed.set_view(SessionType::RTH, false);
        let trade = Tick::new("2026-10-23T21:30:00Z".parse().unwrap(), 100.0, 1.0, None);
        assert!(feed.state().shown_trade(&trade, true).is_some());
        assert!(feed.state().shown_trade(&trade, false).is_none());
    }

    /// Adjustment changes older OHLC prices in the live history, while later bars, timestamps
    /// and volumes keep their raw values in either session.
    #[test]
    fn the_dividend_adjustment_shows_in_the_live_history() {
        for (now, stock_date, fund_date) in [
            (
                "2026-03-06T22:00:00Z",
                "2026-03-03T14:53:00Z",
                "2026-03-04T14:31:00Z",
            ),
            (
                "2026-03-09T14:00:00Z",
                "2026-03-04T14:53:00Z",
                "2026-03-05T14:31:00Z",
            ),
        ] {
            let now = now.parse().unwrap();
            for (symbol, factor, dividend) in
                [("MSFT", 0.97, stock_date), ("SAMPLE", 0.98, fund_date)]
            {
                let dividend = dividend.parse::<DateTime<Utc>>().unwrap();
                let feed = ready_feed(symbol, now);
                for (session, extended) in [
                    (SessionType::RTH, false),
                    (SessionType::ETH, true),
                    (SessionType::Full24x7, false),
                ] {
                    let bars = |adjusted| {
                        feed.set_view(session, adjusted);
                        feed.snapshot(Timeframe::Min1, BarClock::new(Timeframe::Min1), extended)
                    };
                    let (raw, adjusted) = (bars(false), bars(true));
                    let split = raw.partition_point(|bar| bar.time < dividend);
                    assert_eq!(raw.len(), adjusted.len(), "{symbol} at {now}");
                    assert!(0 < split && split < raw.len(), "{symbol} at {now}: {split}");
                    assert_eq!(&raw[split..], &adjusted[split..]);
                    for (raw, adjusted) in raw[..split].iter().zip(&adjusted) {
                        assert_eq!((raw.time, raw.volume), (adjusted.time, adjusted.volume));
                        for (raw, adjusted) in [raw.open, raw.high, raw.low, raw.close]
                            .into_iter()
                            .zip([adjusted.open, adjusted.high, adjusted.low, adjusted.close])
                        {
                            assert!((raw * factor - adjusted).abs() < 1e-9);
                        }
                    }
                    assert_eq!(bars(false), raw);
                }
            }
        }
    }

    /// Closed nights and weekends keep the newest extended-session price and its own time.
    #[test]
    fn the_extended_quote_is_the_newest_trade_inside_an_extended_window() {
        for (now, end, session) in [
            (
                "2026-03-08T15:00:00Z",
                "2026-03-07T01:00:00Z",
                ExtendedSession::PostMarket,
            ),
            (
                "2026-03-09T12:00:00Z",
                "2026-03-09T12:00:00Z",
                ExtendedSession::PreMarket,
            ),
        ] {
            let end = end.parse::<DateTime<Utc>>().unwrap();
            for symbol in ["AAPL", "SAMPLE"] {
                let feed = ready_feed(symbol, now.parse().unwrap());
                for view in [SessionType::RTH, SessionType::ETH, SessionType::Full24x7] {
                    for adjusted in [false, true] {
                        feed.set_view(view, adjusted);
                        let (time, _, _, extended) = feed.regular_quote().unwrap();
                        let extended = extended.unwrap();
                        assert!(us_equity_hours().is_open(time, false));
                        assert_eq!(extended.session, session, "{symbol} at {now}");
                        assert!(
                            (end - TimeDelta::minutes(1)..end).contains(&extended.time),
                            "{symbol} at {now}: {}",
                            extended.time
                        );
                        assert!(extended.time > time);
                        assert!(us_equity_hours().is_open(extended.time, true));
                        assert!(!us_equity_hours().is_open(extended.time, false));
                    }
                }
            }
        }
        for symbol in ["AAPL", "SAMPLE", "BTCUSD"] {
            let feed = ready_feed(symbol, "2026-10-26T15:00:00Z".parse().unwrap());
            if matches!(symbol, "AAPL" | "SAMPLE") {
                assert!(feed.regular_quote().unwrap().3.is_none());
            } else {
                assert!(feed.regular_quote().is_none());
            }
        }
    }

    /// A bounded lookup must return the same price as walking the whole raw stream, including
    /// the history fallback when the retained trades are all outside the selected session.
    #[test]
    fn bounded_quote_lookups_keep_the_last_displayed_prices() {
        for now in ["2026-03-06T14:30:00Z", "2026-03-09T08:00:00Z"] {
            for symbol in ["AAPL", "SAMPLE", "BTCUSD"] {
                let feed = ready_feed(symbol, now.parse().unwrap());
                for session in [SessionType::RTH, SessionType::ETH, SessionType::Full24x7] {
                    for adjusted in [false, true] {
                        feed.set_view(session, adjusted);
                        let sim = feed.state();
                        let extended = session == SessionType::ETH;
                        if let Some((dividend, factor)) = sim.dividend {
                            for (time, price) in [
                                (
                                    dividend - TimeDelta::minutes(1),
                                    if adjusted { 100.0 * factor } else { 100.0 },
                                ),
                                (dividend, 100.0),
                            ] {
                                let trade =
                                    Tick::new(time, 100.0, 7.0, Some(OrderSide::Buy)).with_id(42);
                                let mut expected = trade.clone();
                                expected.price = price;
                                assert_eq!(sim.shown_trade(&trade, extended), Some(expected));
                            }
                        }
                        let latest = sim.trades.back().unwrap().ts + TimeDelta::nanoseconds(1);
                        for before in [
                            latest,
                            latest.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc(),
                            latest + TimeDelta::days(7),
                        ] {
                            let expected = sim
                                .trades
                                .iter()
                                .rev()
                                .filter(|trade| trade.ts < before)
                                .find_map(|trade| sim.shown_trade(trade, extended))
                                .map(|trade| (trade.ts, trade.price))
                                .or_else(|| {
                                    sim.shown_history(&sim.history, extended)
                                        .iter()
                                        .rev()
                                        .find(|bar| bar.time < before)
                                        .map(|bar| (bar.time, bar.close))
                                });
                            assert_eq!(
                                sim.newest_shown(before, extended),
                                expected,
                                "{symbol} at {now}, {session:?}, adjusted {adjusted}, before {before}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Minute volume must reflect clustered arrivals and the daily activity profile.
    #[test]
    fn minute_volume_varies_across_a_full_generated_day() {
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = ready_feed("SAMPLE", now);
        let sim = feed.state();
        let end = sim.archived_end();
        let day = &sim.history[end - 1440..end];
        let mean = day.iter().map(|bar| bar.volume).sum::<f64>() / day.len() as f64;
        let variance = day
            .iter()
            .map(|bar| (bar.volume - mean).powi(2))
            .sum::<f64>()
            / day.len() as f64;
        let cv = variance.sqrt() / mean;
        assert!((0.4..1.2).contains(&cv), "minute volume CV: {cv}");
        let trades: Vec<_> = sim.trades.iter().cloned().collect();
        let minutes = aggregate_trades(&trades, Timeframe::Min1).unwrap();
        let counts: Vec<_> = minutes[1..minutes.len() - 1]
            .iter()
            .map(|bar| {
                trades.partition_point(|trade| trade.ts < bar.time + TimeDelta::minutes(1))
                    - trades.partition_point(|trade| trade.ts < bar.time)
            })
            .collect();
        let mean = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
        let variance = counts
            .iter()
            .map(|count| (*count as f64 - mean).powi(2))
            .sum::<f64>()
            / counts.len() as f64;
        assert!(
            variance.sqrt() / mean > 0.3,
            "arrivals must vary, not only trade sizes"
        );
    }

    #[test]
    fn a_subscription_before_the_session_gets_depth_when_trading_starts() {
        let now = "2026-10-23T13:29:59Z".parse().unwrap();
        let feed = Arc::new(ready_feed("SAMPLE", now));
        feed.set_view(SessionType::RTH, false);
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("SAMPLE")
            .with_initial_bars(100)
            .build();
        chart.set_extended_hours(false);
        let ctx = egui::Context::default();
        let frame = |chart: &mut egui_charts::Chart| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        load_chart(&feed, &mut chart, &ctx);
        assert!(chart.order_book(SeriesId::MAIN).is_none());
        feed.advance(now + TimeDelta::seconds(3));
        frame(&mut chart);
        let book = chart
            .order_book(SeriesId::MAIN)
            .expect("the opening trade starts depth");
        assert!(book.best_bid().is_some() && book.best_ask().is_some());
    }

    #[test]
    fn opening_state_gates_synthetic_stop_runs() {
        let now = "2026-10-23T14:00:00Z".parse().unwrap();
        for enabled in [false, true] {
            let feed = LiveFeed::new(ORDER_LEVEL_SYMBOL, now);
            let detectors = feed.detectors();
            let mut settings = detectors.settings();
            settings.stop_runs.enabled = enabled;
            detectors.set_settings(settings);
            let mut sim = feed.state();
            sim.capture.detectors = Some(detectors);
            sim.id = 1_000;
            sim.next_trade = now;
            let mut rapid = true;
            for _ in 0..6 {
                let time = sim.next_trade;
                sim.trade();
                rapid &= sim.next_trade - time == TimeDelta::milliseconds(10);
            }
            assert_eq!(rapid, enabled);
        }
    }

    /// Guards the two kinds of demo feed: the futures contract keeps the chart's book of
    /// individual orders without one rejected event, with orders in both demo size tiers and of
    /// both recent and older placement, and the other instruments keep aggregate depth.
    #[test]
    fn the_futures_contract_streams_individual_orders_and_the_others_aggregate_depth() {
        use egui_charts::data::OrderFilter;

        let start: DateTime<Utc> = "2026-10-23T14:00:00Z".parse().unwrap();
        for (symbol, order_level) in [(ORDER_LEVEL_SYMBOL, true), ("BTCUSD", false)] {
            let feed = Arc::new(LiveFeed::new(symbol, start));
            let detectors = feed.detectors();
            let mut settings = detectors.settings();
            settings.big_trades.enabled = true;
            settings.stop_runs.enabled = true;
            settings.liquidity.enabled = true;
            settings.big_trades.rule.threshold = egui_charts::services::Threshold::Fixed(10.0);
            settings.liquidity.rule.threshold = egui_charts::services::Threshold::Fixed(10.0);
            settings.liquidity.alert = true;
            detectors.set_settings(settings);
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol(symbol)
                .with_initial_bars(100)
                .build();
            let ctx = egui::Context::default();
            // Subscribe before generating history so it includes the enabled stop-run bursts.
            ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
            load_chart(&feed, &mut chart, &ctx);
            for step in 0..=240 {
                feed.advance(start + TimeDelta::milliseconds(500 * step));
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    chart.show(ui);
                })
                .drop_without_applying_deltas();
            }
            let state = chart.feed_state(SeriesId::MAIN).unwrap();
            for kind in [
                egui_charts::services::DetectionKind::BigTrade,
                egui_charts::services::DetectionKind::StopRun,
                egui_charts::services::DetectionKind::LargeLiquidity,
            ] {
                if kind == egui_charts::services::DetectionKind::StopRun && !order_level {
                    continue;
                }
                assert!(
                    detectors.counts(symbol, kind, feed.now()).total > 0,
                    "{symbol} {kind:?}"
                );
            }
            assert!(
                !detectors.take_alerts().is_empty(),
                "live depth reaches the detector service"
            );
            assert_eq!(state.rejected_depth, 0, "{symbol}");
            let queue = chart.order_queue(SeriesId::MAIN);
            assert_eq!(queue.is_some(), order_level, "{symbol}");
            let Some(queue) = queue else { continue };
            let sizes = Vec::from_iter(
                (queue.largest_orders(&OrderFilter::default()).into_iter()).map(|o| o.size),
            );
            let tiers = super::super::ORDER_TIERS;
            assert!(sizes.iter().any(|&s| s >= tiers.large), "large: {sizes:?}");
            assert!(
                sizes
                    .iter()
                    .any(|&s| (tiers.notable..tiers.large).contains(&s)),
                "notable: {sizes:?}"
            );
            for filter in [
                OrderFilter {
                    max_age: Some(TimeDelta::seconds(10)),
                    ..Default::default()
                },
                OrderFilter {
                    min_age: Some(TimeDelta::seconds(60)),
                    ..Default::default()
                },
            ] {
                let book = queue.filtered("", &filter).unwrap();
                assert!(!book.bids.is_empty() && !book.asks.is_empty(), "{filter:?}");
            }
        }
    }

    /// Archiving executions must preserve their price path, volume and the bucket crossing
    /// the archive boundary, including when a calendar or session choice rebuilds the stream.
    #[test]
    fn history_joins_retained_executions_without_changing_ohlcv() {
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = Arc::new(ready_feed("SAMPLE", now));
        feed.set_paused(true);
        {
            let sim = feed.state();
            let end = sim.archived_end();
            let last = &sim.history[end - 1];
            assert!((last.close - sim.trades.front().unwrap().price).abs() <= 0.051);
            let minutes =
                aggregate_trades(&Vec::from_iter(sim.trades.iter().cloned()), Timeframe::Min1)
                    .unwrap();
            for bars in [&sim.history[end - 60..end], &minutes[..60]] {
                let volume = bars.iter().map(|bar| bar.volume).sum::<f64>() / 60.0;
                let range = bars.iter().map(|bar| bar.high - bar.low).sum::<f64>() / 60.0;
                assert!(
                    (4000.0..14000.0).contains(&volume),
                    "volume per minute: {volume}"
                );
                assert!(
                    (0.7..1.5).contains(&range),
                    "price range per minute: {range}"
                );
            }
        }
        {
            let sim = feed.state();
            let first = &sim.history[0];
            assert_eq!(
                [first.open, first.high, first.low, first.close, first.volume],
                [735.0, 735.29, 734.38, 734.66, 2839.0]
            );
        }
        let ctx = egui::Context::default();
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("SAMPLE")
            .with_initial_bars(20_000)
            .build();
        let frame = |chart: &mut egui_charts::Chart| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        let mut options = chart.chart_options().clone();
        options.time_scale.timezone = egui_charts::config::TimezoneMode::nyse();
        chart = chart.with_chart_options(options);
        for interval in [
            "1min", "45min", "4h", "1D", "1W", "1M", "1s", "5s", "10s", "15s", "30s", "45s", "90s",
            "1t", "10t", "100t", "1000t",
        ] {
            let timeframe = interval.parse().unwrap();
            chart.set_timeframe(timeframe);
            load_chart(&feed, &mut chart, &ctx);
            let expected = feed.snapshot(timeframe, chart.bar_clock(), chart.extended_hours());
            assert_eq!(
                chart.bars(SeriesId::MAIN).len(),
                expected.len(),
                "{interval}"
            );
            for (actual, expected) in chart.bars(SeriesId::MAIN).iter().zip(&expected) {
                assert_eq!(actual, expected, "{interval}");
            }
            if interval == "45min" {
                let join = expected
                    .iter()
                    .find(|bar| {
                        bar.time == "2026-10-23T16:45:00Z".parse::<DateTime<Utc>>().unwrap()
                    })
                    .unwrap();
                // Independently folded across the archived and retained executions.
                for (actual, expected) in [join.open, join.high, join.low, join.close, join.volume]
                    .into_iter()
                    .zip([784.98, 787.37, 783.51, 785.34, 387_265.0])
                {
                    assert!((actual - expected).abs() < 1e-9);
                }
            }
        }
        // The chart's session flag, not a separate demo selection, drives both sources.
        feed.set_view(SessionType::RTH, false);
        for extended in [true, false] {
            chart.set_extended_hours(extended);
            chart.set_timeframe(Timeframe::Min1);
            load_chart(&feed, &mut chart, &ctx);
            let expected = feed.snapshot(Timeframe::Min1, chart.bar_clock(), extended);
            assert_eq!(chart.bars(SeriesId::MAIN), expected);
            let last = expected.last().unwrap();
            let previous = expected
                .iter()
                .rev()
                .find(|bar| bar.time.date_naive() < last.time.date_naive())
                .unwrap();
            assert_eq!(feed.day_change(), Some((last.close, previous.close)));
            let hours = us_equity_hours();
            assert!(expected.iter().all(|bar| hours.is_open(bar.time, extended)));
        }
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        feed.advance(now + TimeDelta::seconds(2));
        frame(&mut chart);
        assert_eq!(
            chart.bars(SeriesId::MAIN),
            bars,
            "pause must freeze executions"
        );
        feed.set_paused(false);
        feed.advance(now + TimeDelta::seconds(2));
        frame(&mut chart);
        assert_ne!(
            chart.bars(SeriesId::MAIN),
            bars,
            "resume must deliver executions"
        );
        let filled = {
            let mut sim = feed.state();
            while sim.trades.len() < RETAINED_TRADES {
                sim.trade();
            }
            sim.next_trade
        };
        let midnight = (filled + TimeDelta::days(1))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        for time in [
            filled,
            midnight - TimeDelta::milliseconds(1),
            midnight + TimeDelta::seconds(2),
        ] {
            {
                let mut sim = feed.state();
                sim.now = time;
                sim.next_trade = time;
                sim.trade();
            }
            let expected = {
                let sim = feed.state();
                let mut days = BTreeMap::new();
                for (time, size) in sim.history[..sim.archived_end()]
                    .iter()
                    .map(|bar| (bar.time, bar.volume))
                    .chain(sim.trades.iter().map(|trade| (trade.ts, trade.size)))
                {
                    *days.entry(time.date_naive()).or_insert(0.0) += size;
                }
                let today = days.remove(&time.date_naive()).unwrap_or(0.0);
                let previous: Vec<f64> = days.into_values().skip(1).collect();
                (
                    today,
                    previous.iter().sum::<f64>() / previous.len().max(1) as f64,
                )
            };
            assert_eq!(feed.daily_volume(), expected);
            assert_eq!(feed.quote(time).unwrap().volume, Some(expected.0));
        }
        let whole = ready_feed("BTCUSD", "2026-10-24T02:00:00Z".parse().unwrap());
        {
            let sim = whole.state();
            assert_eq!(sim.archived_end(), 7 * 24 * 60);
            assert_eq!(sim.history[0].time.time(), chrono::NaiveTime::MIN);
        }
        assert_eq!(whole.daily_volume().1, 78_510_388.0 / 7.0);
    }

    /// The controller publishes the subscribed quotes once per frame, and the first push after a
    /// drain wakes the UI. A push that repeats the last quote therefore kept the demo repainting
    /// every frame forever, even paused with nothing on screen changing: #852. An unchanged
    /// market must now leave the UI idle.
    #[test]
    fn an_unchanged_market_lets_the_demo_go_idle() {
        use egui_charts::ui::widget_bar::{Watchlist, WatchlistPanel};
        use std::time::Duration;
        let ctx = egui::Context::default();
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let live = Arc::new(ready_feed("AAPL", now));
        live.set_paused(true);
        for symbol in ["MSFT", "BTCUSD"] {
            live.market_quote(symbol, now);
        }
        super::super::tests::finish_feed(&live);
        let quotes = Arc::new(DemoQuotes::new(live));
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("Watchlist", "MSFT,BTCUSD")]);
        panel.set_service(quotes.clone());
        let mut time = 0.0;
        let mut delays = Vec::new();
        for _ in 0..8 {
            time += 1.0 / 60.0;
            quotes.publish(now);
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
            );
            delays.push(out.viewport_output[&egui::ViewportId::ROOT].repaint_delay);
            out.drop_without_applying_deltas();
        }
        // The first frame subscribes, which pushes the current quotes; egui then paints one more
        // frame to let things settle, and the subscription stamps its quotes from the feed's
        // clock, which differs from the fixed one of this test by one more push.
        assert_eq!(
            delays[0],
            Duration::ZERO,
            "the first frame subscribes and pushes"
        );
        for delay in &delays[2..] {
            assert_eq!(
                *delay,
                Duration::MAX,
                "publishing an unchanged quote only woke the UI"
            );
        }
    }

    /// The deduplication must not swallow a real move: once a subscribed symbol's rounded price
    /// differs from the one pushed before, the push reaches the panel and wakes the UI.
    #[test]
    fn a_changed_quote_still_wakes_the_ui() {
        use egui_charts::ui::widget_bar::{Watchlist, WatchlistPanel};
        use std::time::Duration;
        let ctx = egui::Context::default();
        let now = "2026-10-23T22:07:05Z".parse().unwrap();
        let live = Arc::new(ready_feed("AAPL", now));
        live.set_paused(true);
        for symbol in ["MSFT", "BTCUSD"] {
            live.market_quote(symbol, now);
        }
        super::super::tests::finish_feed(&live);
        let quotes = Arc::new(DemoQuotes::new(live.clone()));
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("Watchlist", "MSFT,BTCUSD")]);
        panel.set_service(quotes.clone());
        let mut time = 0.0;
        let mut frame = |panel: &mut WatchlistPanel, at: DateTime<Utc>| {
            time += 1.0 / 60.0;
            quotes.publish(at);
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
            );
            let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            out.drop_without_applying_deltas();
            delay
        };
        // The first two frames subscribe and settle, the third is idle.
        assert_eq!(frame(&mut panel, now), Duration::ZERO);
        frame(&mut panel, now);
        assert_eq!(
            frame(&mut panel, now),
            Duration::MAX,
            "an unchanged market must settle idle before the move"
        );
        let (moved, symbol) = (1..=120)
            .map(|seconds| now + chrono::Duration::seconds(seconds))
            .find_map(|at| {
                ["MSFT", "BTCUSD"].iter().find_map(|symbol| {
                    (live.market_quote(symbol, now).map(|q| q.last)
                        != live.market_quote(symbol, at).map(|q| q.last))
                    .then_some((at, *symbol))
                })
            })
            .expect("the synthetic market moves within two minutes");
        assert!(
            frame(&mut panel, moved) < Duration::MAX,
            "a changed price must wake the UI"
        );
        assert_eq!(
            panel.quote(symbol).map(|quote| quote.last),
            live.market_quote(symbol, moved).map(|quote| quote.last),
            "the panel shows the price that woke it"
        );

        // After hours the regular price is fixed, but details must receive the new trade and volume.
        live.set_symbol("SAMPLE");
        panel.set_selected("SAMPLE");
        frame(&mut panel, now);
        frame(&mut panel, now);
        assert_eq!(frame(&mut panel, now), Duration::MAX);
        let before = panel.quote("SAMPLE").unwrap().clone();
        live.set_paused(false);
        let after = now + TimeDelta::seconds(2);
        live.advance(after);
        live.set_paused(true);
        let expected = live.quote(after).unwrap();
        assert_eq!(
            (before.last, before.prev_close),
            (expected.last, expected.prev_close)
        );
        assert_ne!(before.extended, expected.extended);
        assert_ne!(before.volume, expected.volume);
        assert!(frame(&mut panel, after) < Duration::MAX);
        assert_eq!(panel.quote("SAMPLE"), Some(&expected));
        frame(&mut panel, after);
        assert_eq!(frame(&mut panel, after), Duration::MAX);

        // A trade timestamp within the displayed minute changes nothing; crossing it must refresh.
        live.state().trades.back_mut().unwrap().ts += TimeDelta::milliseconds(1);
        assert_eq!(frame(&mut panel, after), Duration::MAX);
        live.state().trades.back_mut().unwrap().ts += TimeDelta::minutes(1);
        assert!(frame(&mut panel, after) < Duration::MAX);
        assert_eq!(panel.quote("SAMPLE"), live.quote(after).as_ref());

        // A book update alone must reach the quote used by the order ticket.
        for price in [76_500, 76_501] {
            live.state().market = Some(Market::new("SAMPLE", 0.01, price, 42));
            assert!(frame(&mut panel, after) < Duration::MAX);
            assert_eq!(panel.quote("SAMPLE"), live.quote(after).as_ref());
            frame(&mut panel, after);
            assert_eq!(frame(&mut panel, after), Duration::MAX);
        }

        // Symbols without trading hours stamp every frame, though details only displays hours and
        // minutes.
        live.set_symbol("SPX");
        panel = WatchlistPanel::new(vec![Watchlist::new("Watchlist")]);
        panel.set_service(quotes.clone());
        panel.set_selected("SPX");
        frame(&mut panel, after);
        frame(&mut panel, after);
        assert_eq!(frame(&mut panel, after), Duration::MAX);
        assert_eq!(
            frame(&mut panel, after + TimeDelta::milliseconds(1)),
            Duration::MAX
        );
        let next_minute = after + TimeDelta::minutes(1);
        assert!(frame(&mut panel, next_minute) < Duration::MAX);
        assert_eq!(panel.quote("SPX"), live.quote(next_minute).as_ref());
    }
}
