//! Synthetic executions delivered through the same feed contract as host market data.

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
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
use super::orders::Orders;
use crate::sampledata::{
    DemoSymbol, SAMPLE_LOGO, dividend_date, dividend_factor, latest_open_end, session_bars, us_equity_hours,
    us_equity_session, us_equity_zone,
};
use egui_charts::ui::timeframe_toolbar::SessionType;

const RETAINED_TRADES: usize = 100_000;

/// Orders added or cancelled in the book after each trade.
const CHURN_PER_TRADE: usize = 6;

/// The instrument whose feed sends individual orders instead of aggregate depth.
pub(super) const ORDER_LEVEL_SYMBOL: &str = "ESZ2026";

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
    /// The ex-date and price factor stay fixed for the life of the feed.
    dividend: Option<(DateTime<Utc>, f64)>,
    subscription: Option<Subscription>,
    signals: Vec<Events<SignalEvent>>,
    options: Vec<(OptionsVolumeQuery, OptionsVolumeSink)>,
    metrics: Vec<MetricSubscription>,
    /// The order book of the subscription, around the last price it was shown.
    market: Option<Market>,
    /// The individual orders behind `market`, on [`ORDER_LEVEL_SYMBOL`] only.
    orders: Option<Orders>,
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
            dividend: DemoSymbol::has_hours(symbol)
                .then(|| (dividend_date(symbol, now), dividend_factor(symbol))),
            subscription: None,
            signals: Vec::new(),
            options: Vec::new(),
            metrics: Vec::new(),
            market: None,
            orders: None,
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
            dividend: sim.dividend,
            subscription: None,
            signals: Vec::new(),
            options: Vec::new(),
            metrics: Vec::new(),
            market: None,
            orders: None,
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

    /// The same synthetic metric history supplied through the feed contract.
    pub(super) fn metric_snapshot(&self) -> Vec<MetricPoint> {
        metric_points(&self.state())
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
            .with_native_timeframes([Timeframe::Min1]);
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

    /// The simulation clock, which stands still while the feed is paused.
    pub(super) fn now(&self) -> DateTime<Utc> {
        self.state().now
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
/// other catalog symbols. The quotes follow the feed's simulation clock, which the pause freezes.
/// The demo has no background worker, so the controller publishes once per frame. Repeating a
/// quote only wakes the UI, so it is pushed again only when a displayed value changes.
pub(super) struct DemoQuotes {
    live: Arc<LiveFeed>,
    stream: Mutex<Option<(Vec<String>, Events<(String, Quote)>)>>,
    /// The last quote pushed per symbol, with times at display precision.
    last_pushed: Mutex<BTreeMap<String, Quote>>,
}

impl DemoQuotes {
    pub(super) fn new(live: Arc<LiveFeed>) -> Self {
        Self {
            live,
            stream: Mutex::default(),
            last_pushed: Mutex::default(),
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
        let mut last_pushed = self.last_pushed.lock().unwrap_or_else(|e| e.into_inner());
        for symbol in symbols {
            let quote = if symbol.eq_ignore_ascii_case(&chart) {
                self.live.quote(now)
            } else {
                watchlist_quote(symbol, now)
            };
            let Some(quote) = quote else {
                continue;
            };
            // Panels use the whole quote, but details shows both update times only to the minute.
            let mut pushed = quote.clone();
            pushed.time = pushed.time.duration_trunc(TimeDelta::minutes(1)).unwrap();
            if let Some(extended) = &mut pushed.extended {
                extended.time = extended.time.duration_trunc(TimeDelta::minutes(1)).unwrap();
            }
            if last_pushed.get(symbol) == Some(&pushed) {
                continue;
            }
            last_pushed.insert(symbol.clone(), pushed);
            events.push((symbol.clone(), quote));
        }
    }
}

impl Quotes for DemoQuotes {
    fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>) {
        // A new stream starts from the current quotes, whatever was pushed to the one before.
        self.last_pushed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.push(symbols, &events, self.live.now());
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
        let Some((dividend, _)) = self.dividend else {
            return self.history.clone();
        };
        session_bars(
            &self.symbol,
            &self.history,
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
            let bars = self.history.partition_point(|bar| bar.time < before);
            let (time, price) = self
                .trades
                .range(..trades)
                .next_back()
                .map(|trade| (trade.ts, trade.price))
                .or_else(|| self.history[..bars].last().map(|bar| (bar.time, bar.close)))?;
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
                self.orders =
                    order_level(&self.symbol, &market, trade.ts, self.tick_size, self.random);
                push_snapshot(sub, &market, self.orders.as_ref(), trade.ts);
                market
            });
            let (sequence, delta) = market.trade(trade.ts, price, side, size);
            match &mut self.orders {
                Some(orders) => orders.apply(&sub.sink, delta, Some((side, price))),
                None => sub.sink.push_depth_delta(sequence, delta),
            }
            for _ in 0..CHURN_PER_TRADE {
                let (sequence, delta) = market.churn(trade.ts);
                match &mut self.orders {
                    Some(orders) => orders.apply(&sub.sink, delta, None),
                    None => sub.sink.push_depth_delta(sequence, delta),
                }
            }
            sub.sink.push_quote(market.quote(trade.ts));
        }
        if trade.ts < self.retain_from {
            match self.history.last_mut() {
                Some(bar)
                    if trade.ts >= bar.time && trade.ts - bar.time < TimeDelta::minutes(1) =>
                {
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
        reply.send(Ok(HistoryPage::new(bars[start..end].to_vec(), start == 0)));
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
        sim.orders = None;
        if let Some(last) = sim.shown_ticks(sim.now, subscription.extended_hours) {
            let market = Market::new(&sim.symbol, sim.tick_size, last, sim.random);
            sim.orders = order_level(&sim.symbol, &market, sim.now, sim.tick_size, sim.random);
            push_snapshot(&subscription, &market, sim.orders.as_ref(), sim.now);
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
            sim.orders = None;
        }
    }

    fn request_depth_snapshot(&self, id: SubscriptionId) {
        let sim = self.state();
        if let (Some(sub), Some(market)) = (&sim.subscription, &sim.market)
            && sub.id == id
        {
            push_snapshot(sub, market, sim.orders.as_ref(), sim.now);
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

    /// A trade after the regular close, at 17:30 New York time, is on the tape only with extended
    /// hours: the window closes at 20:00, not at the regular close.
    #[test]
    fn a_post_market_trade_is_shown_only_with_extended_hours() {
        let now = "2026-10-23T22:00:00Z".parse().unwrap();
        let feed = LiveFeed::new("SAMPLE", now);
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
                let feed = LiveFeed::new(symbol, now);
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
                let feed = LiveFeed::new(symbol, now.parse().unwrap());
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
            let feed = LiveFeed::new(symbol, "2026-10-26T15:00:00Z".parse().unwrap());
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
                let feed = LiveFeed::new(symbol, now.parse().unwrap());
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
                                    sim.shown_history(extended)
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
        let now = "2026-10-23T13:29:59Z".parse().unwrap();
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

    /// Guards the two kinds of demo feed: the futures contract keeps the chart's book of
    /// individual orders without one rejected event, with orders in both demo size tiers and of
    /// both recent and older placement, and the other instruments keep aggregate depth.
    #[test]
    fn the_futures_contract_streams_individual_orders_and_the_others_aggregate_depth() {
        use egui_charts::data::OrderFilter;

        let start: DateTime<Utc> = "2026-10-23T14:00:00Z".parse().unwrap();
        for (symbol, order_level) in [(ORDER_LEVEL_SYMBOL, true), ("BTCUSD", false)] {
            let feed = Arc::new(LiveFeed::new(symbol, start));
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol(symbol)
                .with_initial_bars(100)
                .build();
            let ctx = egui::Context::default();
            for step in 0..=240 {
                feed.advance(start + TimeDelta::milliseconds(500 * step));
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    chart.show(ui);
                })
                .drop_without_applying_deltas();
            }
            let state = chart.feed_state(SeriesId::MAIN).unwrap();
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
        let live = Arc::new(LiveFeed::new("AAPL", now));
        live.set_paused(true);
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
        let live = Arc::new(LiveFeed::new("AAPL", now));
        live.set_paused(true);
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
                    (watchlist_quote(symbol, now).map(|q| q.last)
                        != watchlist_quote(symbol, at).map(|q| q.last))
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
            watchlist_quote(symbol, moved).map(|quote| quote.last),
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
