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
use egui_charts::services::{Events, Quotes, SignalEvent, Signals};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use super::market::Market;
use crate::sampledata::{DemoSymbol, sample_session, session_bars};
use egui_charts::ui::timeframe_toolbar::SessionType;

const RETAINED_TRADES: usize = 100_000;

/// Orders added or cancelled in the book after each trade.
const CHURN_PER_TRADE: usize = 6;

/// Driven by the UI timer on both native and web; there is no background worker.
pub(super) struct LiveFeed(Mutex<Simulation>);

struct Simulation {
    symbol: String,
    tick_size: f64,
    price_ticks: i64,
    random: u64,
    id: u64,
    /// The id of the first execution not yet handed to the time and sales panel.
    tape: u64,
    trades: VecDeque<Tick>,
    history: Vec<Bar>,
    next_trade: DateTime<Utc>,
    retain_from: DateTime<Utc>,
    now: DateTime<Utc>,
    paused: bool,
    continuous: bool,
    adjusted: bool,
    subscription: Option<Subscription>,
    signals: Vec<Events<SignalEvent>>,
    options: Vec<(OptionsVolumeQuery, OptionsVolumeSink)>,
    metrics: Vec<MetricSubscription>,
    /// The order book of the subscription, around the last price it was shown.
    market: Option<Market>,
}

impl LiveFeed {
    pub(super) fn new(symbol: &str, now: DateTime<Utc>) -> Self {
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
            price_ticks: (spec.start_price / tick_size).round() as i64,
            random: 0x5eed_u64.wrapping_mul(spec.seed),
            id: 0,
            tape: 0,
            trades: VecDeque::new(),
            history: Vec::new(),
            next_trade: start - TimeDelta::days(7),
            retain_from: start,
            now,
            paused: false,
            continuous: true,
            adjusted: false,
            subscription: None,
            signals: Vec::new(),
            options: Vec::new(),
            metrics: Vec::new(),
            market: None,
        };
        while sim.next_trade <= now {
            sim.trade();
        }
        Self(Mutex::new(sim))
    }

    fn state(&self) -> MutexGuard<'_, Simulation> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn fork(&self) -> Self {
        let sim = self.state();
        Self(Mutex::new(Simulation {
            symbol: sim.symbol.clone(),
            tick_size: sim.tick_size,
            price_ticks: sim.price_ticks,
            random: sim.random,
            id: sim.id,
            tape: sim.tape,
            trades: sim.trades.clone(),
            history: sim.history.clone(),
            next_trade: sim.next_trade,
            retain_from: sim.retain_from,
            now: sim.now,
            paused: sim.paused,
            continuous: sim.continuous,
            adjusted: sim.adjusted,
            subscription: None,
            signals: Vec::new(),
            options: Vec::new(),
            metrics: Vec::new(),
            market: None,
        }))
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
            let options = std::mem::take(&mut sim.options);
            *sim = Self::new(symbol, sim.now)
                .0
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sim.paused = paused;
            sim.continuous = continuous;
            sim.adjusted = adjusted;
            sim.options = options;
            for (query, sink) in &sim.options {
                sink.push(sim.options_volume(query));
            }
        }
    }

    pub(super) fn set_view(&self, session: SessionType, adjusted: bool) {
        let mut sim = self.state();
        sim.continuous = session == SessionType::Full24x7;
        sim.adjusted = adjusted;
    }

    pub(super) fn snapshot(
        &self,
        timeframe: Timeframe,
        clock: BarClock,
        extended_hours: bool,
    ) -> Vec<Bar> {
        let sim = self.state();
        let trades: Vec<_> = sim
            .trades
            .iter()
            .filter_map(|trade| sim.shown_trade(trade, extended_hours))
            .collect();
        let mut bars = if timeframe.duration_ms() < 60_000 || timeframe.duration_ms() % 60_000 != 0
        {
            egui_charts::data::aggregate_trades_with_tick_size(
                &trades,
                clock.clone(),
                Some(sim.tick_size),
            )
            .expect("generated executions")
        } else {
            let mut minutes = sim.shown_history(extended_hours);
            minutes
                .extend(aggregate_trades(&trades, Timeframe::Min1).expect("generated executions"));
            resample(&minutes, Timeframe::Min1, clock.clone()).expect("nested minute history")
        };
        let open = clock.bar_open(sim.now);
        if timeframe.duration_ms() > 0
            && sim
                .shown_trade(
                    &Tick::new(sim.now, sim.price_ticks as f64 * sim.tick_size, 0.0, None),
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

    pub(super) fn instrument(&self) -> Instrument {
        let sim = self.state();
        let spec = DemoSymbol::find(&sim.symbol);
        let mut instrument = Instrument::new(&sim.symbol)
            .with_description(format!("{} (synthetic)", spec.company_name))
            .with_exchange(spec.exchange)
            .with_price_decimals(if sim.tick_size < 0.01 { 4 } else { 2 })
            .with_tick_size(sim.tick_size)
            .with_native_timeframes([Timeframe::Min1]);
        if sim.symbol == "SAMPLE" && !sim.continuous {
            instrument = instrument
                .with_time_zone(egui_charts::model::TimezoneId::utc())
                .with_session(sample_session());
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
        let Some(extended_hours) = sim.subscription.as_ref().map(|sub| sub.extended_hours) else {
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
        let extended = sim.subscription.as_ref()?.extended_hours;
        let (time, last) = sim
            .trades
            .iter()
            .rev()
            .find_map(|trade| sim.shown_trade(trade, extended))
            .map(|trade| (trade.ts, trade.price))
            .or_else(|| {
                sim.shown_history(extended)
                    .last()
                    .map(|bar| (bar.time, bar.close))
            })?;
        let midnight = time.date_naive().and_hms_opt(0, 0, 0)?.and_utc();
        let prev_close = match sim
            .trades
            .iter()
            .rev()
            .filter(|trade| trade.ts < midnight)
            .find_map(|trade| sim.shown_trade(trade, extended))
        {
            Some(trade) => trade.price,
            None => {
                sim.shown_history(extended)
                    .iter()
                    .rev()
                    .find(|bar| bar.time < midnight)?
                    .close
            }
        };
        Some((last, prev_close))
    }

    /// SAMPLE's last regular-hours price with its time and the regular close of the day before,
    /// and a later trade outside regular hours: pre-market in its extended window, post-market
    /// otherwise. `None` for the other symbols, which have no trading hours.
    pub(super) fn regular_quote(
        &self,
    ) -> Option<(DateTime<Utc>, f64, f64, Option<ExtendedHoursQuote>)> {
        let sim = self.state();
        if sim.symbol != "SAMPLE" {
            return None;
        }
        let hours = egui_charts::model::ExchangeSessionProvider::new(
            egui_charts::model::TimezoneId::utc(),
            Some(sample_session()),
        );
        let newest_first = sim
            .trades
            .iter()
            .rev()
            .map(|trade| (trade.ts, trade.price))
            .chain(sim.history.iter().rev().map(|bar| (bar.time, bar.close)));
        let (time, last) = newest_first
            .clone()
            .find(|&(time, _)| hours.is_open(time, false))?;
        let midnight = time.date_naive().and_hms_opt(0, 0, 0)?.and_utc();
        let (_, prev_close) = newest_first
            .clone()
            .find(|&(t, _)| t < midnight && hours.is_open(t, false))?;
        let extended =
            newest_first
                .clone()
                .next()
                .filter(|&(t, _)| t > time)
                .map(|(time, last)| ExtendedHoursQuote {
                    session: if hours.is_open(time, true) {
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
    /// or for SAMPLE its regular-hours price and the extended-hours trade after it, with the
    /// session volume and the book's best bid and ask.
    pub(super) fn quote(&self, now: DateTime<Utc>) -> Option<Quote> {
        let mut quote = watchlist_quote(&self.state().symbol, now)?;
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

    /// The volume traded since midnight UTC and the average volume of the earlier whole days.
    /// The synthetic history spans a week, which stands in for the 30 days of the average.
    pub(super) fn daily_volume(&self) -> (f64, f64) {
        let sim = self.state();
        let mut days = BTreeMap::new();
        let sizes = sim
            .history
            .iter()
            .map(|bar| (bar.time, bar.volume))
            .chain(sim.trades.iter().map(|trade| (trade.ts, trade.size)));
        for (time, size) in sizes {
            *days.entry(time.date_naive()).or_insert(0.0) += size;
        }
        let today = days.remove(&sim.now.date_naive()).unwrap_or(0.0);
        // The first day starts mid-day.
        let whole: Vec<f64> = days.into_values().skip(1).collect();
        let average = whole.iter().sum::<f64>() / whole.len().max(1) as f64;
        (today, average)
    }

    pub(super) fn paused(&self) -> bool {
        self.state().paused
    }

    pub(super) fn set_paused(&self, paused: bool) {
        self.state().paused = paused;
    }

    /// Advance only when called by the scheduled frame. Pausing freezes simulation time.
    pub(super) fn advance(&self, now: DateTime<Utc>) -> std::time::Duration {
        let mut sim = self.state();
        if sim.paused {
            return std::time::Duration::from_secs(1);
        }
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
                    sub.sink.push_quote(quote);
                }
            }
        }
        let to_trade = (sim.next_trade - now).to_std().unwrap_or_default();
        let to_second = std::time::Duration::from_millis(
            (1000 - now.timestamp_millis().rem_euclid(1000)) as u64,
        );
        to_trade.min(to_second)
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

/// A synthetic watchlist quote: a slow deterministic swing around the catalog price of `symbol`,
/// which stands for the previous close. `None` for symbols outside the catalog.
pub(super) fn watchlist_quote(symbol: &str, now: DateTime<Utc>) -> Option<Quote> {
    let spec = catalog(symbol)?;
    let tick_size = if spec.start_price < 10.0 {
        0.0001
    } else {
        0.01
    };
    let t = now.timestamp_millis() as f64 / 1000.0;
    let phase = spec.seed as f64;
    let swing = 0.02 * (t / 900.0 + phase).sin() + 0.002 * (t / 7.0 + 2.0 * phase).sin();
    let last = (spec.start_price * (1.0 + swing) / tick_size).round() * tick_size;
    Some(Quote::new(last, spec.start_price, now))
}

/// The watchlist's quote service: the chart symbol's quote from the live feed, with its regular
/// and extended-hours prices, session volume and best bid and ask, and a synthetic swing for the
/// other catalog symbols. The demo has no background worker, so the controller publishes once
/// per frame.
pub(super) struct DemoQuotes {
    live: Arc<LiveFeed>,
    stream: Mutex<Option<(Vec<String>, Events<(String, Quote)>)>>,
}

impl DemoQuotes {
    pub(super) fn new(live: Arc<LiveFeed>) -> Self {
        Self {
            live,
            stream: Mutex::default(),
        }
    }

    /// Pushes the current quote of every subscribed symbol.
    pub(super) fn publish(&self, now: DateTime<Utc>) {
        let stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((symbols, events)) = stream.as_ref().filter(|(_, e)| e.is_active()) {
            self.push(symbols, events, now);
        }
    }

    fn push(&self, symbols: &[String], events: &Events<(String, Quote)>, now: DateTime<Utc>) {
        let chart = self.live.state().symbol.clone();
        for symbol in symbols {
            let quote = if symbol.eq_ignore_ascii_case(&chart) {
                self.live.quote(now)
            } else {
                watchlist_quote(symbol, now)
            };
            if let Some(quote) = quote {
                events.push((symbol.clone(), quote));
            }
        }
    }
}

impl Quotes for DemoQuotes {
    fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>) {
        self.push(symbols, &events, chrono::Utc::now());
        *self.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some((symbols.to_vec(), events));
    }
}

impl Simulation {
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

    fn shown_history(&self, extended_hours: bool) -> Vec<Bar> {
        session_bars(
            &self.symbol,
            &self.history,
            self.session(extended_hours),
            self.adjusted,
        )
    }

    fn shown_trade(&self, trade: &Tick, extended_hours: bool) -> Option<Tick> {
        if self.symbol != "SAMPLE" || (self.continuous && !self.adjusted) {
            return Some(trade.clone());
        }
        let bar = Bar::new(
            trade.ts,
            trade.price,
            trade.price,
            trade.price,
            trade.price,
            trade.size,
        );
        session_bars(
            &self.symbol,
            &[bar],
            self.session(extended_hours),
            self.adjusted,
        )
        .first()
        .map(|bar| {
            let mut trade = trade.clone();
            trade.price = bar.close;
            trade
        })
    }

    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    /// The best bid and ask of the subscription's book.
    fn quote(&self, ts: DateTime<Utc>) -> Option<Level1Quote> {
        Some(self.market.as_ref()?.quote(ts))
    }

    /// The price in ticks a trade at the current price would show at.
    fn shown_ticks(&self, now: DateTime<Utc>, extended_hours: bool) -> Option<i64> {
        let price = self.price_ticks as f64 * self.tick_size;
        let shown = self.shown_trade(&Tick::new(now, price, 0.0, None), extended_hours)?;
        Some((shown.price / self.tick_size).round() as i64)
    }

    fn trade(&mut self) {
        let step = (self.random() % 11) as i64 - 5;
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
        self.signals.retain(Events::is_active);
        for events in &self.signals {
            events.push(SignalEvent::Upsert(live_signal(trade.ts, trade.price)));
        }

        if let Some(sub) = &self.subscription
            && let Some(shown) = self.shown_trade(&trade, sub.extended_hours)
        {
            let price = (shown.price / self.tick_size).round() as i64;
            sub.sink.push_tick(shown);
            // A subscription made while no trade was shown gets its book with the first one.
            let market = self.market.get_or_insert_with(|| {
                let market = Market::new(&self.symbol, self.tick_size, price, self.random);
                let (sequence, book) = market.snapshot(trade.ts);
                sub.sink.push_depth_snapshot(sequence, book);
                market
            });
            let (sequence, delta) = market.trade(trade.ts, price, side, size);
            sub.sink.push_depth_delta(sequence, delta);
            for _ in 0..CHURN_PER_TRADE {
                let (sequence, delta) = market.churn(trade.ts);
                sub.sink.push_depth_delta(sequence, delta);
            }
            sub.sink.push_quote(market.quote(trade.ts));
        }
        if trade.ts < self.retain_from {
            let open = BarClock::new(Timeframe::Min1).bar_open(trade.ts);
            match self.history.last_mut() {
                Some(bar) if bar.time == open => {
                    bar.high = bar.high.max(trade.price);
                    bar.low = bar.low.min(trade.price);
                    bar.close = trade.price;
                    bar.volume += trade.size;
                }
                _ => self.history.push(Bar::new(
                    open,
                    trade.price,
                    trade.price,
                    trade.price,
                    trade.price,
                    trade.size,
                )),
            }
        } else {
            self.trades.push_back(trade);
        }
        if self.trades.len() > RETAINED_TRADES {
            // Archive whole minutes so replay never loses the beginning of a time bar.
            let clock = BarClock::new(Timeframe::Min1);
            let oldest = clock.bar_open(self.trades.front().unwrap().ts);
            let mut expired = Vec::new();
            while self
                .trades
                .front()
                .is_some_and(|trade| clock.bar_open(trade.ts) == oldest)
            {
                expired.push(self.trades.pop_front().unwrap());
            }
            self.history
                .extend(aggregate_trades(&expired, Timeframe::Min1).expect("generated executions"));
            if self.history.len() > 10_000 {
                self.history.drain(..self.history.len() - 10_000);
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
        let seconds = self.next_trade.timestamp();
        let phase = seconds.rem_euclid(86_400) as f64 / 86_400.0 * std::f64::consts::TAU;
        let session = 1.0 + 0.6 * phase.cos();
        let mut burst = (seconds.div_euclid(120) as u64).wrapping_mul(0x9e3779b97f4a7c15);
        burst = (burst ^ (burst >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        let intensity = session * (0.25 + 3.0 * ((burst >> 32) as f64 / u32::MAX as f64).powi(3));
        let delay = (delay as f64 / intensity).round().max(1.0) as i64;
        self.next_trade += TimeDelta::milliseconds(delay);
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
        let oldest = sim.history.first().map_or(sim.now, |bar| bar.time);
        let mut points: Vec<_> = (0..=(sim.now - oldest).num_seconds().max(0) / 300)
            .map(|i| open_interest(oldest + TimeDelta::seconds(i * 300)))
            .filter(|point| request.before.is_none_or(|before| point.time < before))
            .collect();
        // Include the developing five-minute point even when the oldest bar has an offset.
        let current = open_interest(sim.now);
        if request.before.is_none_or(|before| current.time < before)
            && points.last().is_none_or(|point| point.time != current.time)
        {
            points.push(current);
        }
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
        if symbol != "SAMPLE" {
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
        reply.send(Ok(self.instrument()));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let sim = self.state();
        // The retained executions are replayed through the subscription, including its partial
        // final count bar. OHLC history cannot reconstruct a count or a smaller time interval.
        let bars = if request.timeframe.duration_ms() >= 60_000
            && request.timeframe.duration_ms() % 60_000 == 0
        {
            resample(
                &sim.shown_history(request.extended_hours),
                Timeframe::Min1,
                request.clock.clone(),
            )
            .expect("nested minute history")
        } else {
            Vec::new()
        };
        let end = bars.partition_point(|b| request.before.is_none_or(|before| b.time < before));
        let start = end.saturating_sub(request.count);
        reply.send(Ok(HistoryPage {
            bars: bars[start..end].to_vec(),
            end_of_history: start == 0,
        }));
    }

    fn subscribe(&self, subscription: Subscription) {
        let mut sim = self.state();
        subscription.sink.set_server_time(sim.now);
        for trade in &sim.trades {
            if let Some(trade) = sim.shown_trade(trade, subscription.extended_hours) {
                subscription.sink.push_tick(trade);
            }
        }
        sim.market = None;
        if let Some(last) = sim.shown_ticks(sim.now, subscription.extended_hours) {
            let market = Market::new(&sim.symbol, sim.tick_size, last, sim.random);
            let (sequence, book) = market.snapshot(sim.now);
            subscription.sink.push_depth_snapshot(sequence, book);
            subscription.sink.advance_time(sim.now);
            subscription.sink.push_quote(market.quote(sim.now));
            sim.market = Some(market);
        }
        subscription.sink.set_status(FeedStatus::Live);
        sim.subscription = Some(subscription);
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        let mut sim = self.state();
        sim.metrics.retain(|sub| sub.id != id);
        if sim.subscription.as_ref().is_some_and(|sub| sub.id == id) {
            sim.subscription = None;
            sim.market = None;
        }
    }

    fn request_depth_snapshot(&self, id: SubscriptionId) {
        let sim = self.state();
        if let (Some(sub), Some(market)) = (&sim.subscription, &sim.market)
            && sub.id == id
        {
            let (sequence, book) = market.snapshot(sim.now);
            sub.sink.push_depth_snapshot(sequence, book);
        }
    }

    /// The catalog symbols ranked on their synthetic quotes.
    fn hotlist(&self, query: HotlistQuery, reply: Reply<Vec<HotlistEntry>>) {
        reply.send(Ok(super::feeds::hotlist(&query, Utc::now())));
    }

    /// The catalog symbols screened on their synthetic quotes.
    fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
        reply.send(super::feeds::screen(&query, Utc::now()));
    }

    /// Synthetic releases, earnings, dividends and offerings.
    fn calendar(&self, query: CalendarQuery, reply: Reply<Vec<CalendarEvent>>) {
        reply.send(Ok(super::feeds::calendar(&query, Utc::now())));
    }

    /// Synthetic stories about the catalog symbols.
    fn news(&self, query: NewsQuery, reply: Reply<Vec<NewsStory>>) {
        reply.send(Ok(super::feeds::news(&query, Utc::now())));
    }

    /// Synthetic statistics and profile derived from the catalog entry of `symbol`.
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
            "Synthetic" => "Fund",
            _ => "Stock",
        };
        let pair = |label: &str, value: String| (label.to_owned(), value);
        reply.send(Ok(SymbolDetails::new(
            vec![pair(
                "52-week range",
                format!("{} - {}", price(0.82), price(1.17)),
            )],
            vec![
                pair("Type", kind.into()),
                pair("Source", "Synthetic data generated by the demo".into()),
            ],
        )
        .with_url(
            "https://github.com/userFRM/egui-charts/blob/main/examples/demo/sampledata.rs",
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::ChartBuilder;
    use egui_charts::widget::SeriesId;
    use std::sync::Arc;

    #[test]
    fn options_queries_resume_after_switching_back_to_the_underlying() {
        use egui_charts::studies::{Indicator, OptionsVolumeProfile};

        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = LiveFeed::new("SAMPLE", now);
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

    /// Minute volume must reflect clustered arrivals and the daily activity profile.
    #[test]
    fn minute_volume_varies_across_a_full_generated_day() {
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = LiveFeed::new("SAMPLE", now);
        let sim = feed.state();
        let day = &sim.history[sim.history.len() - 1440..];
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
        let now = "2026-10-23T14:29:59Z".parse().unwrap();
        let feed = Arc::new(LiveFeed::new("SAMPLE", now));
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
        for _ in 0..4 {
            frame(&mut chart);
        }
        assert!(chart.order_book(SeriesId::MAIN).is_none());
        feed.advance(now + TimeDelta::seconds(3));
        frame(&mut chart);
        let book = chart
            .order_book(SeriesId::MAIN)
            .expect("the opening trade starts depth");
        assert!(book.best_bid().is_some() && book.best_ask().is_some());
    }

    /// Archiving executions must preserve their price path, volume and the bucket crossing
    /// the archive boundary, including when a calendar or session choice rebuilds the stream.
    #[test]
    fn history_joins_retained_executions_without_changing_ohlcv() {
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        let feed = Arc::new(LiveFeed::new("SAMPLE", now));
        feed.set_paused(true);
        {
            let sim = feed.state();
            let last = sim.history.last().unwrap();
            assert!((last.close - sim.trades.front().unwrap().price).abs() <= 0.051);
            let minutes =
                aggregate_trades(&Vec::from_iter(sim.trades.iter().cloned()), Timeframe::Min1)
                    .unwrap();
            for bars in [&sim.history[sim.history.len() - 60..], &minutes[..60]] {
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
            for _ in 0..4 {
                frame(&mut chart);
            }
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
                    .zip([763.95, 765.29, 758.36, 759.55, 376_797.0])
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
            for _ in 0..4 {
                frame(&mut chart);
            }
            let expected = feed.snapshot(Timeframe::Min1, chart.bar_clock(), extended);
            assert_eq!(chart.bars(SeriesId::MAIN), expected);
            let last = expected.last().unwrap();
            let previous = expected
                .iter()
                .rev()
                .find(|bar| bar.time.date_naive() < last.time.date_naive())
                .unwrap();
            assert_eq!(feed.day_change(), Some((last.close, previous.close)));
            let hours = egui_charts::model::ExchangeSessionProvider::new(
                egui_charts::model::TimezoneId::utc(),
                Some(sample_session()),
            );
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
    }
}
