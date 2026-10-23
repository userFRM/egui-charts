//! Synthetic executions delivered through the same feed contract as host market data.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{
    Datafeed, FeedStatus, HistoryPage, HistoryRequest, Reply, Subscription, SubscriptionId,
    aggregate_trades, resample,
};
use egui_charts::model::{Bar, BarClock, Instrument, Level1Quote, Tick, Timeframe, TradeSide};
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use crate::sampledata::{DemoSymbol, generate_sample_bars, session_bars};
use egui_charts::ui::timeframe_toolbar::SessionType;

const RETAINED_TRADES: usize = 100_000;

/// Driven by the UI timer on both native and web; there is no background worker.
pub(super) struct LiveFeed(Mutex<Simulation>);

struct Simulation {
    symbol: String,
    tick_size: f64,
    price_ticks: i64,
    random: u64,
    id: u64,
    trades: VecDeque<Tick>,
    history: Vec<Bar>,
    next_trade: DateTime<Utc>,
    now: DateTime<Utc>,
    paused: bool,
    session: SessionType,
    adjusted: bool,
    subscription: Option<Subscription>,
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
        let mut history = generate_sample_bars(symbol, 1000, Timeframe::Min1);
        let shift = start - history.last().expect("generated history").time - TimeDelta::minutes(1);
        for bar in &mut history {
            bar.time += shift;
            for price in [&mut bar.open, &mut bar.high, &mut bar.low, &mut bar.close] {
                *price = (*price / tick_size).round() * tick_size;
            }
        }
        let mut sim = Simulation {
            symbol: symbol.into(),
            tick_size,
            price_ticks: (history.last().unwrap().close / tick_size).round() as i64,
            random: 0x5eed,
            id: 0,
            trades: VecDeque::new(),
            history,
            next_trade: start,
            now,
            paused: false,
            session: SessionType::Full24x7,
            adjusted: false,
            subscription: None,
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

    pub(super) fn set_symbol(&self, symbol: &str) {
        let mut sim = self.state();
        if sim.symbol != symbol {
            let paused = sim.paused;
            let session = sim.session;
            let adjusted = sim.adjusted;
            *sim = Self::new(symbol, sim.now)
                .0
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sim.paused = paused;
            sim.session = session;
            sim.adjusted = adjusted;
        }
    }

    pub(super) fn set_view(&self, session: SessionType, adjusted: bool) {
        let mut sim = self.state();
        sim.session = session;
        sim.adjusted = adjusted;
    }

    pub(super) fn snapshot(&self, timeframe: Timeframe, clock: BarClock) -> Vec<Bar> {
        let sim = self.state();
        let trades: Vec<_> = sim.trades.iter().cloned().collect();
        if timeframe.duration_ms() < Timeframe::Min1.duration_ms() {
            return aggregate_trades(&trades, clock).expect("generated executions");
        }
        let mut minutes = sim.history.clone();
        minutes.extend(aggregate_trades(&trades, Timeframe::Min1).expect("generated executions"));
        sim.backfill(&minutes, timeframe, clock)
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
        if second_changed && let Some(sub) = &sim.subscription {
            sub.sink.set_server_time(now);
            if sim
                .shown_trade(&Tick::new(
                    now,
                    sim.price_ticks as f64 * sim.tick_size,
                    0.0,
                    None,
                ))
                .is_some()
            {
                sub.sink.advance_time(now);
                sub.sink.push_quote(sim.quote(now));
            }
        }
        let to_trade = (sim.next_trade - now).to_std().unwrap_or_default();
        let to_second = std::time::Duration::from_millis(
            (1000 - now.timestamp_millis().rem_euclid(1000)) as u64,
        );
        to_trade.min(to_second)
    }
}

impl Simulation {
    fn backfill(&self, minutes: &[Bar], timeframe: Timeframe, clock: BarClock) -> Vec<Bar> {
        let bars = resample(minutes, Timeframe::Min1, clock.clone()).unwrap_or_default();
        let Some(first) = bars.first() else {
            return bars;
        };
        // Older synthetic OHLC bars extend coarse views beyond the retained executions.
        // Walk actual clock boundaries backwards, including calendar months and clock changes.
        let mut older = generate_sample_bars(
            &self.symbol,
            1000usize.saturating_sub(bars.len()),
            Timeframe::Min1,
        );
        if older.is_empty() {
            return bars;
        }
        let factor = first.open / older.last().unwrap().close;
        let mut end = first.time;
        let mut start = older.len();
        for i in (0..older.len()).rev() {
            if end.checked_sub_signed(timeframe.duration()).is_none() {
                break;
            }
            let Some(before) = end.checked_sub_signed(TimeDelta::milliseconds(1)) else {
                break;
            };
            let open = clock.bar_open(before);
            if open >= end {
                break;
            }
            let bar = &mut older[i];
            bar.time = open;
            for price in [&mut bar.open, &mut bar.high, &mut bar.low, &mut bar.close] {
                *price = (*price * factor / self.tick_size).round() * self.tick_size;
            }
            bar.volume *= (end - open).num_milliseconds() as f64 / 60_000.0;
            end = open;
            start = i;
        }
        older.drain(..start);
        older.extend(bars);
        older
    }

    fn shown_trade(&self, trade: &Tick) -> Option<Tick> {
        let bar = Bar::new(
            trade.ts,
            trade.price,
            trade.price,
            trade.price,
            trade.price,
            trade.size,
        );
        session_bars(&self.symbol, &[bar], self.session, self.adjusted)
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

    fn quote(&self, ts: DateTime<Utc>) -> Level1Quote {
        Level1Quote {
            ts,
            symbol: self.symbol.clone(),
            bid: (self.price_ticks - 1) as f64 * self.tick_size,
            ask: (self.price_ticks + 1) as f64 * self.tick_size,
            bid_size: 100.0,
            ask_size: 100.0,
        }
    }

    fn trade(&mut self) {
        let step = (self.random() % 11) as i64 - 5;
        self.price_ticks = (self.price_ticks + step).max(1);
        let u = (self.random() % 1000 + 1) as f64 / 1001.0;
        let size = (10.0 / u.sqrt()).round();
        let side = if step >= 0 {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        };
        let trade = Tick::new(
            self.next_trade,
            self.price_ticks as f64 * self.tick_size,
            size,
            Some(side),
        )
        .with_id(self.id);
        self.id += 1;
        if let Some(sub) = &self.subscription
            && let Some(shown) = self.shown_trade(&trade)
        {
            sub.sink.push_tick(shown);
            sub.sink.push_quote(self.quote(trade.ts));
        }
        self.trades.push_back(trade);
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
        // Bursts near 20/s alternate with quieter stretches and occasional half-second pauses.
        let delay = 50
            + self.random() % 100
            + if self.random().is_multiple_of(13) {
                400
            } else {
                0
            };
        self.next_trade += TimeDelta::milliseconds(delay as i64);
    }
}

impl Datafeed for LiveFeed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        let spec = DemoSymbol::find(symbol);
        self.set_symbol(symbol);
        let sim = self.state();
        let mut instrument = Instrument::new(symbol)
            .with_description(format!("{} (synthetic)", spec.company_name))
            .with_exchange(spec.exchange)
            .with_price_decimals(if sim.tick_size < 0.01 { 4 } else { 2 });
        instrument.tick_size = Some(sim.tick_size);
        reply.send(Ok(instrument));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let sim = self.state();
        // The retained executions are replayed through the subscription, including its partial
        // final count bar. OHLC history cannot reconstruct a count or a smaller time interval.
        let bars = if request.timeframe.duration_ms() >= Timeframe::Min1.duration_ms() {
            sim.backfill(
                &session_bars(&sim.symbol, &sim.history, sim.session, sim.adjusted),
                request.timeframe,
                request.clock.clone(),
            )
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
            if let Some(trade) = sim.shown_trade(trade) {
                subscription.sink.push_tick(trade);
            }
        }
        subscription.sink.advance_time(sim.now);
        subscription.sink.push_quote(sim.quote(sim.now));
        subscription.sink.set_status(FeedStatus::Live);
        sim.subscription = Some(subscription);
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        let mut sim = self.state();
        if sim.subscription.as_ref().is_some_and(|sub| sub.id == id) {
            sim.subscription = None;
        }
    }
}
