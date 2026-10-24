//! Synthetic executions delivered through the same feed contract as host market data.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{
    Datafeed, FeedError, FeedStatus, HistoryPage, HistoryRequest, Reply, Subscription,
    SubscriptionId, SymbolDetails, aggregate_trades, resample,
};
use egui_charts::model::{Bar, BarClock, Instrument, Level1Quote, Tick, Timeframe, TradeSide};
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use crate::sampledata::{DemoSymbol, sample_session, session_bars};
use egui_charts::ui::timeframe_toolbar::SessionType;
use egui_charts::ui::widget_bar::WatchlistQuote;

const RETAINED_TRADES: usize = 100_000;

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
            *sim = Self::new(symbol, sim.now)
                .0
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sim.paused = paused;
            sim.continuous = continuous;
            sim.adjusted = adjusted;
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
            aggregate_trades(&trades, clock.clone()).expect("generated executions")
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
                .shown_trade(
                    &Tick::new(now, sim.price_ticks as f64 * sim.tick_size, 0.0, None),
                    sub.extended_hours,
                )
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

/// A synthetic watchlist quote: a slow deterministic swing around the catalog price of `symbol`,
/// which stands for the previous close. `None` for symbols outside the catalog.
pub(super) fn watchlist_quote(symbol: &str, now: DateTime<Utc>) -> Option<WatchlistQuote> {
    let spec = DemoSymbol::catalog()
        .iter()
        .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))?;
    let (tick_size, price_decimals) = if spec.start_price < 10.0 {
        (0.0001, 4)
    } else {
        (0.01, 2)
    };
    let t = now.timestamp_millis() as f64 / 1000.0;
    let phase = spec.seed as f64;
    let swing = 0.02 * (t / 900.0 + phase).sin() + 0.002 * (t / 7.0 + 2.0 * phase).sin();
    Some(WatchlistQuote {
        last: (spec.start_price * (1.0 + swing) / tick_size).round() * tick_size,
        prev_close: spec.start_price,
        price_decimals: Some(price_decimals),
    })
}

impl Simulation {
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
            && let Some(shown) = self.shown_trade(&trade, sub.extended_hours)
        {
            sub.sink.push_tick(shown);
            sub.sink.push_quote(self.quote(trade.ts));
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

impl Datafeed for LiveFeed {
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
        if sim
            .shown_trade(
                &Tick::new(sim.now, sim.price_ticks as f64 * sim.tick_size, 0.0, None),
                subscription.extended_hours,
            )
            .is_some()
        {
            subscription.sink.advance_time(sim.now);
            subscription.sink.push_quote(sim.quote(sim.now));
        }
        subscription.sink.set_status(FeedStatus::Live);
        sim.subscription = Some(subscription);
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        let mut sim = self.state();
        if sim.subscription.as_ref().is_some_and(|sub| sub.id == id) {
            sim.subscription = None;
        }
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
        let volume = 0.5 + (spec.seed % 10) as f64 * 0.35;
        let kind = match spec.exchange {
            "Crypto" => "Cryptocurrency",
            "Index" => "Index",
            "Forex" => "Currency pair",
            "Synthetic" => "Fund",
            _ => "Stock",
        };
        let pair = |label: &str, value: String| (label.to_owned(), value);
        reply.send(Ok(SymbolDetails::new(
            vec![
                pair(
                    "52-week range",
                    format!("{} - {}", price(0.82), price(1.17)),
                ),
                pair("Average volume (30D)", format!("{volume:.2}M")),
            ],
            vec![
                pair("Type", kind.into()),
                pair("Source", "Synthetic data generated by the demo".into()),
            ],
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::ChartBuilder;
    use egui_charts::chart::selection::SeriesId;
    use std::sync::Arc;

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
