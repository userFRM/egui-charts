use super::super::test_frames::{candle_ys, frame, shapes};
use super::*;
use crate::data::{DepthPage, FeedError, HistoryPage, Sink, Subscription, TradePage};
use crate::model::{Bar, BarData, ChartType};
use crate::widget::ChartBuilder;
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Resolve(String),
    History(HistoryRequest),
    Subscribe(SubscriptionId, String, Timeframe, bool),
    Unsubscribe(SubscriptionId),
    Trades(TradeRequest),
    Snapshot(SubscriptionId),
    Depth(DepthRequest),
}

/// A feed that records every call and keeps the replies and sinks for the test to answer.
#[derive(Default)]
struct Script {
    calls: Mutex<Vec<Call>>,
    resolves: Mutex<Vec<Reply<Instrument>>>,
    pages: Mutex<Vec<Reply<HistoryPage>>>,
    trades: Mutex<Vec<Reply<TradePage>>>,
    depth: Mutex<Vec<Reply<DepthPage>>>,
    sinks: Mutex<Vec<Sink>>,
    /// `unsubscribe` drops the replies it keeps.
    drop_replies: AtomicBool,
    capabilities: Mutex<Vec<Reply<FeedCapabilities>>>,
}

impl Script {
    fn calls(&self) -> Vec<Call> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
    fn resolve(&self) -> Reply<Instrument> {
        self.resolves.lock().unwrap().remove(0)
    }
    fn page(&self) -> Reply<HistoryPage> {
        self.pages.lock().unwrap().remove(0)
    }
    fn sink(&self) -> Sink {
        self.sinks.lock().unwrap().last().unwrap().clone()
    }
}

impl Datafeed for Script {
    fn capabilities(&self, reply: Reply<FeedCapabilities>) {
        self.capabilities.lock().unwrap().push(reply);
    }
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        self.calls
            .lock()
            .unwrap()
            .push(Call::Resolve(symbol.into()));
        self.resolves.lock().unwrap().push(reply);
    }
    fn resolve_converted(&self, symbol: &str, conversion: &str, reply: Reply<Instrument>) {
        let call = Call::Resolve(format!("{symbol} in {conversion}"));
        self.calls.lock().unwrap().push(call);
        self.resolves.lock().unwrap().push(reply);
    }
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        self.calls.lock().unwrap().push(Call::History(request));
        self.pages.lock().unwrap().push(reply);
    }
    fn subscribe(&self, subscription: Subscription) {
        let Subscription {
            id,
            symbol,
            timeframe,
            extended_hours,
            sink,
            ..
        } = subscription;
        let call = Call::Subscribe(id, symbol, timeframe, extended_hours);
        self.calls.lock().unwrap().push(call);
        self.sinks.lock().unwrap().push(sink);
    }
    fn unsubscribe(&self, id: SubscriptionId) {
        self.calls.lock().unwrap().push(Call::Unsubscribe(id));
        if self.drop_replies.load(Ordering::Relaxed) {
            self.resolves.lock().unwrap().clear();
            self.pages.lock().unwrap().clear();
            self.trades.lock().unwrap().clear();
        }
    }
    fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
        self.calls.lock().unwrap().push(Call::Trades(request));
        self.trades.lock().unwrap().push(reply);
    }
    fn depth_history(&self, request: DepthRequest, reply: Reply<DepthPage>) {
        self.calls.lock().unwrap().push(Call::Depth(request));
        self.depth.lock().unwrap().push(reply);
    }
    fn request_depth_snapshot(&self, id: SubscriptionId) {
        self.calls.lock().unwrap().push(Call::Snapshot(id));
    }
}

/// The feed-wide capabilities are known before a symbol resolves, asked once however often
/// the symbol or interval changes, also while the answer is outstanding, and an empty list
/// stays distinct from an unreported one.
#[test]
fn capabilities_arrive_before_the_symbol_resolves() {
    let feed = Arc::new(Script::default());
    let reported = FeedCapabilities::new()
        .with_timeframes([])
        .with_exchanges(["NYSE".to_owned()]);
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    chart.set_symbol("B");
    frames.run(&mut chart);
    assert_eq!(chart.feed_capabilities(), None);
    feed.capabilities
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(reported.clone()));
    frames.run(&mut chart);
    assert_eq!(chart.feed_capabilities(), Some(&reported));
    assert_eq!(chart.feed_capabilities().unwrap().symbol_types, None);
    assert_eq!(chart.feed_capabilities().unwrap().timeframes, Some(vec![]));
    assert!(feed.resolves.lock().unwrap().len() == 2, "still resolving");
    chart.set_symbol("C");
    chart.set_timeframe(Timeframe::Min5);
    frames.run(&mut chart);
    assert!(feed.capabilities.lock().unwrap().is_empty(), "asked again");
}

/// An empty window names where older bars continue, and the next request starts there
/// instead of repeating itself; a continuation that does not lie before the request stops
/// paging with an error; without one the empty page ends history.
#[test]
fn empty_bar_windows_continue_at_the_named_boundary() {
    let all = bars(2000);
    let oldest = all[1900].time;
    for (name, continuation, next, complete, stopped) in [
        (
            "continues",
            Some(oldest - TimeDelta::days(3)),
            true,
            false,
            false,
        ),
        ("ends", None, false, true, false),
        ("repeats", Some(oldest), false, false, true),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.page().send(page(all[1900..].to_vec(), false));
        frames.run(&mut chart);
        frames.run(&mut chart);
        feed.calls();
        let mut window = HistoryPage::new(Vec::new(), false);
        window.continue_before = continuation;
        feed.page().send(Ok(window));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let calls = feed.calls();
        let expected: Vec<_> = continuation
            .filter(|_| next)
            .map(|at| history("A", Timeframe::Min1, Some(at)))
            .into_iter()
            .collect();
        assert_eq!(calls, expected, "{name}");
        let state = chart.feed_state(SeriesId::MAIN).unwrap();
        assert_eq!(state.history_complete, complete, "{name}");
        assert_eq!(
            matches!(state.last_error, Some(FeedError::InvalidData(_))),
            stopped,
            "{name}"
        );
    }
}

/// Older trades follow the view once something consumes them; an empty window continues at
/// its named boundary, and a continuation that does not advance ends the paging.
#[test]
fn older_trades_follow_the_view_without_repeating_a_cursor() {
    let all = bars(1000);
    let oldest = all[999].time;
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(all.clone(), false));
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let asked = |feed: &Script| {
        feed.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Trades(request) => Some(request),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(asked(&feed)[0].before, None);
    let trades = (0..5)
        .map(|i| {
            let mut trade = Tick::new(oldest + TimeDelta::seconds(i), 100.0, 1.0, None);
            trade.id = Some(i as u64);
            trade
        })
        .collect();
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(trades, false)));
    frames.run(&mut chart);
    let requests = asked(&feed);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].before, Some(oldest));
    assert!(
        !chart
            .feed_state(SeriesId::MAIN)
            .unwrap()
            .trade_history_complete
    );

    // Inside the view, which reaches back further, so the boundary is asked for at once.
    let cursor = oldest - TimeDelta::minutes(30);
    let window = TradePage::new(Vec::new(), false).with_continue_before(cursor);
    feed.trades.lock().unwrap().remove(0).send(Ok(window));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let requests = asked(&feed);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].before, Some(cursor));

    let stuck = TradePage::new(Vec::new(), false).with_continue_before(cursor);
    feed.trades.lock().unwrap().remove(0).send(Ok(stuck));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert!(asked(&feed).is_empty(), "no repeated cursor");
    let state = chart.feed_state(SeriesId::MAIN).unwrap();
    assert!(state.trade_history_complete);
    assert!(matches!(state.last_error, Some(FeedError::InvalidData(_))));
}

/// A changed conversion resolves and loads the symbol again with the choice in every request,
/// and a reply to the previous choice, however late, is not shown.
#[test]
fn conversion_reloads_and_ignores_replies_to_the_previous_choice() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    let (stale, stale_resolve) = (feed.page(), feed.resolve());
    feed.calls();
    chart.set_conversion(Some("EUR".into()));
    frames.run(&mut chart);
    let calls = feed.calls();
    assert_eq!(calls[0], Call::Resolve("A in EUR".into()));
    let Call::History(request) = &calls[1] else {
        panic!("{calls:?}");
    };
    assert_eq!(request.conversion.as_deref(), Some("EUR"));

    stale.send(page(bars(5), false));
    stale_resolve.send(Ok(Instrument::new("A").with_currency("USD")));
    frames.run(&mut chart);
    assert!(chart.bars(SeriesId::MAIN).is_empty(), "previous choice");
    assert_eq!(chart.instrument(), None, "previous choice");
    feed.resolve()
        .send(Ok(Instrument::new("A").with_currency("EUR")));
    feed.page().send(page(bars(3), false));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 3);
    assert_eq!(
        chart.instrument().and_then(|i| i.currency.as_deref()),
        Some("EUR")
    );
    let calls = feed.calls();
    let Some(Call::Trades(trades)) = calls.iter().find(|c| matches!(c, Call::Trades(_))) else {
        panic!("{calls:?}");
    };
    assert_eq!(trades.conversion.as_deref(), Some("EUR"));
}

/// Scanners retain no empty managers; marked symbols survive revisits until forgotten.
#[test]
fn inactive_symbol_state_can_be_forgotten() {
    let mut chart = Chart::new(crate::model::BarData::from_bars(bars(2)));
    for i in 0..10_000 {
        chart.set_symbol(format!("SYMBOL-{i}"));
        chart.drain_events().for_each(drop);
    }
    #[cfg(feature = "drawings")]
    assert!(chart.symbol_drawings.is_empty());
    #[cfg(not(feature = "drawings"))]
    assert!(chart.symbol_markers.is_empty());
    chart.set_symbol("marked");
    chart.set_markers(
        SeriesId::MAIN,
        vec![crate::model::Marker::new(bars(1)[0].time)],
    );
    chart.set_symbol("other");
    chart.set_symbol("marked");
    assert_eq!(chart.markers(SeriesId::MAIN).len(), 1);
    assert!(!chart.forget_symbol("marked"));
    chart.set_symbol("other");
    assert!(chart.forget_symbol("marked"));
    chart.set_symbol("marked");
    assert!(chart.markers(SeriesId::MAIN).is_empty());
}

/// `count` one-minute bars from 00:00 on 15 March 2024, closing at 100 plus their index.
fn bars(count: usize) -> Vec<Bar> {
    let start = "2024-03-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    (0..count)
        .map(|i| {
            let close = 100.0 + i as f64;
            let time = start + TimeDelta::minutes(i as i64);
            Bar::new(time, close, close, close, close, 1.0)
        })
        .collect()
}

fn page(bars: Vec<Bar>, end_of_history: bool) -> Result<HistoryPage, FeedError> {
    Ok(HistoryPage::new(bars, end_of_history))
}

fn history(symbol: &str, timeframe: Timeframe, before: Option<DateTime<Utc>>) -> Call {
    Call::History(HistoryRequest {
        symbol: symbol.into(),
        timeframe,
        clock: BarClock::new(timeframe),
        before,
        since: None,
        count: 1000,
        extended_hours: false,
        conversion: None,
    })
}

/// Frames at 60 Hz, each a sixtieth of a second after the last.
struct Frames {
    ctx: egui::Context,
    time: f64,
}

impl Frames {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            time: 0.0,
        }
    }
    fn run(&mut self, chart: &mut Chart) {
        self.time += 1.0 / 60.0;
        frame(&self.ctx, chart, self.time, Vec::new());
    }
}

fn fed(feed: &Arc<Script>, symbol: Option<&str>) -> Chart {
    let builder = ChartBuilder::new().with_feed(feed.clone());
    match symbol {
        Some(symbol) => builder.with_symbol(symbol).build(),
        None => builder.build(),
    }
}

/// Changing symbols fits the new prices, with or without a feed. Repeating a symbol,
/// replacing its bars or reloading its feed preserves the user's manual range.
/// Initial ratio-lock configuration still applies to the first frame of supplied bars.
#[test]
fn symbol_changes_reset_manual_price_scaling() {
    for with_feed in [false, true] {
        for locked in [false, true] {
            let feed = Arc::new(Script::default());
            let mut chart = if with_feed {
                fed(&feed, Some("A"))
            } else {
                let mut chart = ChartBuilder::new()
                    .with_config(crate::config::ChartConfig {
                        lock_price_to_bar_ratio: locked,
                        ..Default::default()
                    })
                    .with_symbol("A")
                    .build();
                chart.set_data(SeriesId::MAIN, bars(6));
                chart
            };
            let mut frames = Frames::new();
            frames.run(&mut chart);
            if with_feed {
                feed.page().send(page(bars(6), true));
            } else {
                assert_eq!(chart.state.price_scale().bar_ratio_locked(), locked);
            }
            frames.run(&mut chart);
            chart.price_scale_mut().set_range(80.0, 120.0);
            chart.config_mut().lock_price_to_bar_ratio = locked;
            frames.run(&mut chart);
            chart.state.push_zoom_state();
            let manual = chart.state.price_scale().range();
            chart.set_symbol("A");
            if with_feed {
                chart.reload_feed();
                frames.run(&mut chart);
                feed.page().send(page(bars(6), true));
            } else {
                chart.set_data(SeriesId::MAIN, bars(6));
            }
            frames.run(&mut chart);
            assert!(chart.state.price_scale().is_manual());
            assert_eq!(chart.state.price_scale().range(), manual);

            chart.set_symbol("B");
            frames.run(&mut chart);
            let next = bars(6)
                .into_iter()
                .map(|bar| Bar::new(bar.time, 1000.0, 1050.0, 1000.0, 1050.0, 1.0))
                .collect();
            if with_feed {
                feed.page().send(page(next, true));
            } else {
                chart.set_data(SeriesId::MAIN, next);
            }
            frames.run(&mut chart);
            assert!(!chart.state.price_scale().is_manual());
            assert!(!chart.state.price_scale().bar_ratio_locked());
            assert!(!chart.state.has_zoom_history());
            let range = chart.state.price_scale().range();
            // 50 / (1 - 0.1 - 0.1) = 62.5; each margin adds 6.25.
            assert!((range.min - 993.75).abs() < 1e-10, "{range:?}");
            assert!((range.max - 1056.25).abs() < 1e-10, "{range:?}");
        }
    }
}

/// Undoing a user's symbol navigation asks the feed for the earlier symbol again, and an
/// answer still owed for the symbol left behind changes nothing.
#[test]
fn undone_symbol_navigation_reloads_the_symbol_and_ignores_stale_replies() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(bars(6), true));
    frames.run(&mut chart);
    chart.user_action(|chart| chart.set_symbol("B"));
    frames.run(&mut chart);
    let stale = feed.page();
    feed.calls();
    assert!(chart.undo());
    frames.run(&mut chart);
    assert!(feed.calls().contains(&history("A", Timeframe::Min1, None)));
    stale.send(page(bars(3), true));
    feed.page().send(page(bars(6), true));
    frames.run(&mut chart);
    assert_eq!(chart.symbol(), "A");
    assert_eq!(chart.bars(SeriesId::MAIN), bars(6));
    assert!(chart.redo());
    frames.run(&mut chart);
    assert!(feed.calls().contains(&history("B", Timeframe::Min1, None)));
}

/// A full retention window stops paging without claiming the feed has no more history.
/// Time and trade-count bars both keep their newest slots across per-frame tick batches.
#[test]
fn bar_limits_bound_feed_history_and_ticks() {
    use crate::widget::ChartEvent;
    use std::num::NonZeroUsize;
    for (timeframe, end, limit_after_load) in [
        (Timeframe::Min1, false, false),
        ("1t".parse().unwrap(), false, false),
        ("2t".parse().unwrap(), false, false),
        (Timeframe::Min1, true, false),
        (Timeframe::Min1, true, true),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(timeframe);
        chart.set_chart_type(ChartType::VolumeFootprint);
        if !limit_after_load {
            chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(3));
        }
        let mut frames = Frames::new();
        frames.run(&mut chart);
        chart.drain_events().for_each(drop);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), end));
        frames.run(&mut chart);
        if limit_after_load {
            chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(3));
        }
        assert_eq!(chart.bars(SeriesId::MAIN), &bars(5)[2..]);
        assert_eq!(
            chart
                .drain_events()
                .filter(|event| matches!(
                    event,
                    ChartEvent::DataLoaded { .. } | ChartEvent::DataUpdated { .. }
                ))
                .collect::<Vec<_>>(),
            if limit_after_load {
                vec![
                    ChartEvent::DataLoaded {
                        series: SeriesId::MAIN,
                        bars: 5,
                    },
                    ChartEvent::DataUpdated {
                        series: SeriesId::MAIN,
                        bars: 3,
                    },
                ]
            } else {
                vec![ChartEvent::DataLoaded {
                    series: SeriesId::MAIN,
                    bars: 3,
                }]
            },
            "history eviction must not report a live update"
        );
        assert!(!chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
        assert!(
            feed.pages.lock().unwrap().is_empty(),
            "full window must not page"
        );
        chart.set_bar_limit(SeriesId::MAIN, None);
        chart.set_start_idx(0);
        frames.run(&mut chart);
        assert_eq!(
            feed.pages.lock().unwrap().len(),
            1,
            "clearing the limit resumes paging"
        );
        // This page was already in flight when the limit was restored.
        chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(3));
        feed.page()
            .send(page(if end { Vec::new() } else { bars(2) }, end));
        frames.run(&mut chart);
        assert!(!chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
        let sink = feed.sink();
        let start = bars(5)[4].time;
        for batch in 0..10 {
            for i in 0..10 {
                let at = batch * 10 + i + 1;
                sink.push_tick(Tick::new(
                    start + TimeDelta::minutes(at),
                    1000.0 + at as f64,
                    1.0,
                    None,
                ));
            }
            frames.run(&mut chart);
            assert_eq!(chart.bars(SeriesId::MAIN).len(), 3, "{timeframe}");
            assert!(
                chart.from_ticks.len() <= 3,
                "tick provenance must also be evicted"
            );
        }
        assert_eq!(chart.bars(SeriesId::MAIN).last().unwrap().close, 1100.0);
        assert_eq!(chart.trades(SeriesId::MAIN).count(), 100);
        assert_eq!(
            chart.feed_link.as_ref().unwrap().footprint.is_some(),
            !timeframe.is_trade_based(),
            "execution charts retain candles when the interval has no time slots"
        );
    }
}

/// A full limit of 16 bars evicts down to 16 - 16 / 8 = 14 when a new bar overflows it, so a
/// stream restarts the studies on one new bar in three instead of on every one. Between those
/// evictions every study follows a new bar in the bar's frame, and after them scroll-back stays
/// paused although the series holds fewer bars than the limit.
#[cfg(feature = "studies")]
#[test]
fn a_full_bar_limit_evicts_in_chunks_and_keeps_paging_paused() {
    use std::num::NonZeroUsize;
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(16));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    feed.page().send(page(bars(16), false));
    chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
    chart.add_indicator(Box::new(crate::studies::EMA::new(2)));
    // Restarted studies return one per frame: volume, SMA and EMA take three.
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    let sink = feed.sink();
    let start = bars(1)[0].time;
    let mut held = Vec::new();
    let mut previous = chart.bars(SeriesId::MAIN).len();
    for minute in 16..22 {
        sink.push_tick(Tick::new(
            start + TimeDelta::minutes(minute),
            200.0,
            1.0,
            None,
        ));
        frames.run(&mut chart);
        let len = chart.bars(SeriesId::MAIN).len();
        // Only an eviction, which shrinks the series, restarts the studies.
        if len >= previous {
            assert!(
                chart
                    .indicators()
                    .all(|(_, study)| study.values().len() == len),
                "minute {minute}: a study without values"
            );
        }
        held.push(len);
        previous = len;
        for _ in 0..3 {
            frames.run(&mut chart);
        }
    }
    assert_eq!(held, [14, 15, 16, 14, 15, 16]);
    assert!(
        feed.pages.lock().unwrap().is_empty(),
        "an eviction must not resume paging"
    );
}

/// Evicted trades leave the footprint: its bars equal those summed from the held trades, delta
/// extrema included, and the bar the eviction cut is not offered as complete.
#[test]
fn footprint_follows_trade_eviction() {
    use crate::model::OrderSide::{Buy, Sell};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(5), false));
    frames.run(&mut chart);
    let start = bars(5)[4].time;
    let sink = feed.sink();
    // 100 s of trades span two minute bars; the first 10 leave the store.
    for range in [0..TRADE_CAPACITY, TRADE_CAPACITY..TRADE_CAPACITY + 10] {
        for i in range {
            let side = [Buy, Buy, Sell][i % 3];
            let ts = start + TimeDelta::milliseconds(i as i64);
            sink.push_tick(Tick::new(ts, 100.0 + (i % 5) as f64, 1.0, Some(side)));
        }
        frames.run(&mut chart);
    }
    let mut summed = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    chart
        .trades(SeriesId::MAIN)
        .for_each(|t| _ = summed.push(t));
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap();
    assert_eq!(flow.bars(), summed.bars());
    assert_eq!(flow.complete_from(), Some(start + TimeDelta::minutes(1)));
}

/// Bars from the first execution on show merged candles while their cells would not be readable.
#[test]
fn footprint_draws_candles_from_the_first_execution_at_the_minimum_spacing() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(1), false));
    frames.run(&mut chart);
    let start = bars(1)[0].time;
    let sink = feed.sink();
    for i in 0..2000 {
        let price = match i {
            1900 => 500.0,
            1950 => 20.0,
            _ => 100.0,
        };
        let ts = start + TimeDelta::minutes(i);
        sink.push_tick(Tick::new(
            ts,
            price,
            1.0,
            Some(crate::model::OrderSide::Buy),
        ));
    }
    frames.run(&mut chart);
    chart.apply_synced_time_scale(0.5, 0.0);
    frames.run(&mut chart);
    let painted = shapes(&frames.ctx, &mut chart, frames.time, Vec::new());
    let config = chart.config();
    let ys = [config.bullish_color, config.bearish_color].map(|color| candle_ys(&painted, color));
    let ys = ys.concat();
    let map = chart.get_chart_mapping();
    for price in [500.0, 20.0] {
        let y = map.price_to_y(price);
        assert!(ys.iter().any(|at| (at - y).abs() < 1.0), "{price}: {y}");
    }
}

/// Range bars follow the order of the supplied executions, not only the OHLC of the bars they
/// fall in, and the OHLC approximation stands in while the chart holds none.
#[test]
fn range_bars_are_built_from_the_held_trades() {
    use crate::model::RangeBarSource::{Executions, OhlcApproximation};
    let start = bars(1)[0].time;
    // Both paths open at 100, high 104, low 96 and close at 100: up first, then down first.
    let paths = [[100.0, 104.0, 96.0, 100.0], [100.0, 96.0, 104.0, 100.0]];
    let mut closes = Vec::new();
    for prices in paths {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(crate::model::ChartType::Range);
        chart.set_chart_type_params(crate::model::ChartTypeParams {
            range_size: 4.0,
            ..Default::default()
        });
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        let mut history = bars(1);
        (history[0].high, history[0].low) = (104.0, 96.0);
        feed.page().send(page(history, true));
        frames.run(&mut chart);
        assert_eq!(chart.range_bar_source(), Some(OhlcApproximation));
        let sink = feed.sink();
        for (i, price) in prices.into_iter().enumerate() {
            sink.push_tick(Tick::new(
                start + TimeDelta::seconds(i as i64),
                price,
                1.0,
                None,
            ));
        }
        frames.run(&mut chart);
        assert_eq!(chart.range_bar_source(), Some(Executions));
        closes.push(
            chart
                .state
                .shown()
                .iter()
                .map(|bar| bar.close)
                .collect::<Vec<_>>(),
        );
        // A replay cursor after the second trade leaves only its first range bar.
        chart.set_replay_time(Some(start + TimeDelta::seconds(2)));
        frames.run(&mut chart);
        assert_eq!(chart.range_bar_source(), Some(Executions));
        let shown: Vec<_> = chart.state.shown().iter().map(|bar| bar.close).collect();
        assert_eq!(shown, [closes[closes.len() - 1][0]]);
    }
    assert_eq!(
        closes,
        [[104.0, 100.0, 96.0, 100.0], [96.0, 100.0, 104.0, 100.0]]
    );
}

#[test]
fn replay_reset_removes_future_data_from_the_chart() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    #[cfg(feature = "studies")]
    let studies: Vec<_> = {
        let factory = crate::studies::IndicatorFactory::new();
        ["Quoted Spread", "Trade Rate", "Book Size"]
            .map(|name| chart.add_indicator(factory.create(name).unwrap()))
            .into()
    };
    let mut frames = Frames::new();
    frames.run(&mut chart);
    let mut instrument = Instrument::new("A");
    instrument.tick_size = Some(0.25);
    feed.resolve().send(Ok(instrument));
    feed.page().send(page(bars(10), true));
    frames.run(&mut chart);
    let sink = feed.sink();
    let time = bars(1)[0].time;
    let future = time + TimeDelta::minutes(20);
    sink.set_server_time(future);
    sink.push_tick(Tick::new(future, 120.0, 9.0, None).with_id(7));
    let quote = |ts, bid, ask| crate::model::Level1Quote {
        ts,
        symbol: "A".into(),
        bid,
        ask,
        bid_size: 1.0,
        ask_size: 1.0,
    };
    sink.push_quote(quote(future, 119.0, 121.0));
    sink.advance_time(future + TimeDelta::seconds(1));
    sink.push_depth_snapshot(
        1,
        OrderBook {
            ts: future,
            symbol: "A".into(),
            bids: vec![],
            asks: vec![],
        },
    );
    frames.run(&mut chart);
    assert_eq!(chart.trades(SeriesId::MAIN).count(), 1);
    assert!(chart.order_book(SeriesId::MAIN).is_some());
    sink.reset_replay(time);
    sink.push_tick(Tick::new(time, 100.0, 2.0, None).with_id(7));
    sink.push_quote(quote(time, 99.0, 100.0));
    sink.advance_time(time + TimeDelta::seconds(1));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 1);
    assert_eq!(
        (
            chart.bars(SeriesId::MAIN)[0].close,
            chart.bars(SeriesId::MAIN)[0].volume
        ),
        (100.0, 2.0)
    );
    assert_eq!(
        chart
            .trades(SeriesId::MAIN)
            .map(|t| t.price)
            .collect::<Vec<_>>(),
        [100.0]
    );
    assert!(chart.order_book(SeriesId::MAIN).is_none());
    #[cfg(feature = "studies")]
    {
        for _ in 0..8 {
            frames.run(&mut chart);
        }
        for (id, expected) in studies.into_iter().zip([Some(4.0), Some(1.0), None]) {
            let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
            let values = study.order_flow_input().unwrap().values();
            assert!(values.keys().all(|at| *at == time), "{}", study.name());
            assert_eq!(study.values().value(0, 0), expected, "{}", study.name());
            if study.name() == "Trade Rate" {
                assert_eq!(study.values().value(2, 0), Some(1.0 / 3.0));
            }
        }
    }
}

#[test]
fn executions_link_existing_tape_fills_without_counting_twice() {
    use crate::data::{OrderEvent, RestingOrder};
    use crate::model::OrderSide::{Buy, Sell};
    for (timeframe, separate_frames) in [
        (Timeframe::Min1, false),
        (Timeframe::Min1, true),
        ("2v".parse().unwrap(), false),
        ("2v".parse().unwrap(), true),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(timeframe);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        feed.page().send(page(vec![], true));
        frames.run(&mut chart);
        let sink = feed.sink();
        let time = bars(1)[0].time;
        let trade = Tick::new(time, 100.0, 2.0, Some(Buy)).with_id(42);
        sink.set_server_time(time);
        sink.push_order_snapshot(1, time, vec![RestingOrder::new(8, Sell, 100.0, 5.0)]);
        sink.push_tick(trade.clone());
        if separate_frames {
            frames.run(&mut chart);
        }
        sink.push_order_event(
            2,
            time,
            OrderEvent::Execute {
                id: 8,
                size: 2.0,
                trade: Some(trade.with_aggressor_order_id(9)),
            },
        );
        frames.run(&mut chart);
        let trades: Vec<_> = chart.trades(SeriesId::MAIN).collect();
        assert_eq!(trades.len(), 1);
        assert_eq!(
            (trades[0].resting_order_id, trades[0].aggressor_order_id),
            (Some(8), Some(9)),
            "separate frames: {separate_frames}"
        );
        assert_eq!(chart.bars(SeriesId::MAIN)[0].volume, 2.0);
        assert_eq!(chart.order_book(SeriesId::MAIN).unwrap().asks[0].size, 3.0);
        assert_eq!(
            chart.combined_trades(SeriesId::MAIN)[0].aggressor_order_id,
            Some(9)
        );
    }
}

#[test]
fn depth_paging_ends_independently_and_drops_old_settings_replies() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    let mut instrument = Instrument::new("A");
    instrument.tick_size = Some(0.25);
    feed.resolve().send(Ok(instrument));
    feed.page().send(page(bars(10), true));
    frames.run(&mut chart);
    let old = feed.depth.lock().unwrap().remove(0);
    chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
    frames.run(&mut chart);
    old.send(Ok(DepthPage::new(
        vec![crate::data::DepthColumn {
            time: Utc::now(),
            span: TimeDelta::seconds(1),
            ..Default::default()
        }],
        false,
    )));
    feed.depth
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(DepthPage::new(vec![], false)));
    feed.calls();
    frames.run(&mut chart);
    chart.set_start_idx(0);
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert!(feed.calls().iter().all(|c| !matches!(c, Call::Depth(_))));
    assert_eq!(
        chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .count(),
        0
    );
    assert!(chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
}

/// Depth pages follow the visible time and stop without older columns, unless an empty
/// window names where they continue.
#[test]
fn depth_pages_follow_visible_time_and_stop_without_older_columns() {
    for (name, offset, span, end, continuation, expected) in [
        ("overlapping page", Some(899), 120, false, None, vec![900]),
        ("final page", Some(899), 60, true, None, vec![899, 900]),
        ("empty page", None, 60, false, None, vec![900]),
        ("repeated page", Some(900), 60, false, None, vec![900]),
        ("empty window", None, 60, false, Some(880), vec![900]),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        let mut instrument = Instrument::new("A");
        instrument.tick_size = Some(0.25);
        feed.resolve().send(Ok(instrument));
        let bars = bars(1000);
        feed.page().send(page(bars.clone(), true));
        frames.run(&mut chart);
        let column = |index: usize, seconds| crate::data::DepthColumn {
            time: bars[index].time,
            span: TimeDelta::seconds(seconds),
            bids: vec![(400, 2.0)],
            ..Default::default()
        };
        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 950.0,
            right: 999.0,
        });
        feed.depth
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(DepthPage::new(vec![column(900, 60)], false)));
        frames.run(&mut chart);
        feed.calls();
        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 850.0,
            right: 899.0,
        });
        frames.run(&mut chart);
        frames.run(&mut chart);
        let calls = feed.calls();
        assert_eq!(calls.len(), 1, "{name}: {calls:?}");
        assert!(
            matches!(&calls[0], Call::Depth(r) if r.before == Some(bars[900].time)),
            "{name}: {calls:?}"
        );
        let mut answer = DepthPage::new(offset.map(|i| column(i, span)).into_iter().collect(), end);
        answer.continue_before = continuation.map(|i| bars[i].time);
        feed.depth.lock().unwrap().remove(0).send(Ok(answer));
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        let calls = feed.calls();
        match continuation {
            Some(i) => assert!(
                matches!(&calls[..], [Call::Depth(r)] if r.before == Some(bars[i].time)),
                "{name}: {calls:?}"
            ),
            None => assert!(calls.is_empty(), "{name}: paging did not stop"),
        }
        let held: Vec<_> = chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .map(|c| (c.time, c.span, c.bids.clone()))
            .collect();
        assert_eq!(
            held,
            expected
                .into_iter()
                .map(|i| (bars[i].time, TimeDelta::seconds(60), vec![(400, 2.0)]))
                .collect::<Vec<_>>(),
            "{name}"
        );
    }
}

#[test]
fn extended_hours_reload_history_paging_and_subscription_together() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    let mut previous = None;
    for extended in [false, true, false] {
        chart.set_extended_hours(extended);
        assert_eq!(chart.extended_hours(), extended);
        frames.run(&mut chart);
        let calls = feed.calls();
        if let Some(previous) = previous {
            assert!(calls.contains(&Call::Unsubscribe(previous)));
        }
        assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_none() && request.extended_hours == extended)));
        feed.page().send(page(bars(1000), false));
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let id = feed.sink().id;
        let mut calls = feed.calls();
        assert!(calls.contains(&Call::Subscribe(id, "A".into(), Timeframe::Min1, extended)));
        chart.set_extended_hours(extended);
        frames.run(&mut chart);
        assert!(
            feed.calls().is_empty(),
            "an unchanged choice must keep the stream"
        );
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        calls.extend(feed.calls());
        assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_some() && request.extended_hours == extended)), "{extended}: {calls:?}");
        feed.page().send(page(vec![], true));
        frames.run(&mut chart);
        previous = Some(id);
    }
}

/// A daily-only instrument: the minute page asked for before it was known is dropped, no
/// intraday history is asked for once it is known, and an answer allowing the interval again
/// asks for it.
#[test]
fn intervals_the_instrument_rules_out_are_not_requested() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    let daily = || {
        Instrument::new("A").with_supported_timeframes([
            Timeframe::Day1,
            Timeframe::Week1,
            Timeframe::Month1,
        ])
    };
    let ruled_out = |chart: &Chart| {
        let state = chart.feed_state(SeriesId::MAIN).unwrap();
        chart.bars(SeriesId::MAIN).is_empty()
            && !state.loading_history
            && state.last_error == Some(FeedError::NotSupported)
    };
    frames.run(&mut chart);
    assert!(feed.calls().contains(&history("A", Timeframe::Min1, None)));
    feed.resolve().send(Ok(daily()));
    feed.page().send(page(bars(10), false));
    frames.run(&mut chart);
    assert!(ruled_out(&chart));
    assert!(
        feed.calls().is_empty(),
        "no subscription for a ruled-out interval"
    );

    chart.set_timeframe(Timeframe::Min5);
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [Call::Resolve("A".into())]);
    feed.resolve().send(Ok(daily()));
    frames.run(&mut chart);
    assert!(ruled_out(&chart));
    assert!(feed.calls().is_empty());

    chart.reload_feed();
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [Call::Resolve("A".into())]);
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let calls = feed.calls();
    assert_eq!(calls[0], history("A", Timeframe::Min5, None));
    assert!(matches!(calls[1], Call::Subscribe(..)), "{calls:?}");
}

/// Resolving an exchange zone must realign history, live aggregation and the countdown
/// once, then keep the subscription stable through ordinary repaints.
#[cfg(feature = "timezones")]
#[test]
fn calendar_reload_uses_the_resolved_exchange_zone() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.chart_options.time_scale.timezone =
        crate::scales::TimezoneMode::Exchange(crate::model::TimezoneId::utc());
    chart.set_timeframe(Timeframe::Hour4);
    let mut frames = Frames::new();
    let zone = crate::model::TimezoneId::named("America/New_York").unwrap();
    let expected =
        BarClock::new(Timeframe::Hour4).with_timezone("America/New_York".parse().unwrap());
    frames.run(&mut chart);
    feed.page().send(page(Vec::new(), true));
    feed.resolve()
        .send(Ok(Instrument::new("A").with_time_zone(zone.clone())));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(chart.bar_clock(), expected);
    assert!(
        feed.calls()
            .iter()
            .any(|call| matches!(call, Call::History(request) if request.clock == expected))
    );
    feed.page().send(page(Vec::new(), true));
    feed.resolve()
        .send(Ok(Instrument::new("A").with_time_zone(zone)));
    frames.run(&mut chart);
    let sink = feed.sink();
    let time = "2024-03-10T07:30:00Z".parse().unwrap();
    sink.push_tick(Tick::new(time, 100.0, 2.0, None));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN)[0].time, expected.bar_open(time));
    assert_eq!(chart.feed_link.as_ref().unwrap().clock, expected);
    feed.calls();
    frames.run(&mut chart);
    assert!(feed.calls().is_empty());
}

#[test]
fn scroll_to_time_shows_the_nearest_bar_and_loads_older_history() {
    let all = bars(3000);
    // Exact match, nearer right/left neighbour, tie (earlier wins), and both ends.
    for (seconds, expected) in [
        (30000, 500),
        (30040, 501),
        (30020, 500),
        (30030, 500),
        (-60, 0),
        (180000, 2999),
    ] {
        let mut chart = Chart::new(BarData::from_bars(all.clone()));
        let mut frames = Frames::new();
        chart.set_visible_bars(100);
        frames.run(&mut chart);
        let spacing = chart.state.time_scale().bar_spacing();
        chart.enable_tracking_mode();
        chart.scroll_to_time(all[0].time + TimeDelta::seconds(seconds));
        frames.run(&mut chart);
        let range = chart.state.time_scale().visible_logical_range();
        assert!(
            ((range.left + range.right) / 2.0 - expected as f32).abs() < 0.01,
            "{seconds}: {range:?}"
        );
        assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
        assert!(!chart.is_tracking_mode_active());
    }
    let mut empty = Chart::new(BarData::new());
    empty.scroll_to_time(all[0].time);
    Frames::new().run(&mut empty);

    for (target, expected, exhaust) in [
        (all[300].time, 300, false),
        (all[0].time - TimeDelta::days(1), 0, true),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        // Navigation requested even before the first page remains pending.
        chart.scroll_to_time(target);
        frames.run(&mut chart);
        feed.calls();
        feed.page().send(page(all[2000..].to_vec(), false));
        frames.run(&mut chart);
        frames.run(&mut chart);
        // Scrolling to an unloaded time bounds each page by it.
        let ranged = |before: DateTime<Utc>| {
            let Call::History(mut request) = history("A", Timeframe::Min1, Some(before)) else {
                unreachable!()
            };
            request.since = Some(target);
            Call::History(request)
        };
        assert_eq!(feed.calls(), [ranged(all[2000].time)]);
        feed.page().send(page(all[1000..2000].to_vec(), false));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [ranged(all[1000].time)]);
        feed.page().send(page(all[..1000].to_vec(), exhaust));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let range = chart.state.time_scale().visible_logical_range();
        assert!(
            ((range.left + range.right) / 2.0 - expected as f32).abs() < 0.01,
            "{range:?}"
        );
        assert!(feed.calls().is_empty());
    }

    // Moving the view while an older page is outstanding cancels the target.
    fn play(chart: &mut Chart, frames: &mut Frames, events: Vec<egui::Event>) {
        for event in events {
            frames.time += 1.0 / 60.0;
            frame(&frames.ctx, chart, frames.time, vec![event]);
        }
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    let drag = |chart: &mut Chart, frames: &mut Frames| {
        let start = chart.last_rendered_price_rect.center();
        let mut events = vec![egui::Event::PointerMoved(start), button(start, true)];
        events.extend(
            (1..=10).map(|i| egui::Event::PointerMoved(start - egui::vec2(40.0 * i as f32, 0.0))),
        );
        events.push(button(start - egui::vec2(400.0, 0.0), false));
        play(chart, frames, events);
    };
    let end_key = |chart: &mut Chart, frames: &mut Frames| {
        let centre = chart.last_rendered_price_rect.center();
        let end = egui::Event::Key {
            key: egui::Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let click = [egui::Event::PointerMoved(centre), button(centre, true)];
        play(
            chart,
            frames,
            [&click[..], &[button(centre, false), end]].concat(),
        );
    };
    let box_zoom = |chart: &mut Chart, _: &mut Frames| {
        let rect = chart.last_rendered_price_rect;
        let start = rect.right_top() + egui::vec2(-100.0, 10.0);
        let end = rect.right_bottom() - egui::vec2(10.0, 10.0);
        assert!(chart.execute_box_zoom(start, end, rect, rect.width()));
    };
    let moves: [fn(&mut Chart, &mut Frames); 4] =
        [|chart, _| chart.set_start_idx(600), drag, end_key, box_zoom];
    for (row, user_move) in moves.into_iter().enumerate() {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.page().send(page(all[2000..].to_vec(), false));
        frames.run(&mut chart);
        chart.scroll_to_time(all[300].time);
        frames.run(&mut chart);
        frames.run(&mut chart);
        user_move(&mut chart, &mut frames);
        for _ in 0..5 {
            frames.run(&mut chart);
        }
        let range = chart.state.time_scale().visible_logical_range();
        assert!((range.left + range.right) / 2.0 > 25.0, "{row}: {range:?}");
    }
}

#[test]
fn transient_requests_back_off_and_keep_the_failed_page_boundary() {
    for (error, delay) in [
        (FeedError::Network("offline".into()), 1.0),
        (FeedError::Unanswered, 1.0),
        (
            FeedError::RateLimited {
                retry_after: Some(std::time::Duration::from_secs(7)),
            },
            7.0,
        ),
        (FeedError::RateLimited { retry_after: None }, 1.0),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.page().send(page(bars(1000), false));
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        feed.calls();
        let expected = history("A", Timeframe::Min1, Some(bars(1)[0].time));
        feed.page().send(Err(error.clone()));
        frames.run(&mut chart);
        let failed_at = frames.time;
        frames.time = failed_at + delay - 0.1;
        frames.run(&mut chart);
        assert!(feed.calls().is_empty(), "early retry: {error}");
        frames.time = failed_at + delay;
        frames.run(&mut chart);
        assert_eq!(
            feed.calls(),
            std::slice::from_ref(&expected),
            "missing retry: {error}"
        );
        feed.page()
            .send(Err(FeedError::Network("still offline".into())));
        frames.run(&mut chart);
        let second = frames.time;
        frames.time = second + 1.5;
        frames.run(&mut chart);
        assert!(feed.calls().is_empty(), "backoff did not double");
        frames.time = second + 2.0;
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [expected]);
        feed.page().send(page(bars(1000), true));
        frames.run(&mut chart);
        assert!(chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
    }
    // A failed resolve retries independently and a changed key cancels its deadline.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.calls();
    feed.resolve()
        .send(Err(FeedError::Network("offline".into())));
    feed.page().send(page(bars(10), true));
    frames.run(&mut chart);
    frames.time += 1.0;
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [Call::Resolve("A".into())]);
    feed.resolve()
        .send(Err(FeedError::Network("offline".into())));
    frames.run(&mut chart);
    chart.set_symbol("B");
    frames.run(&mut chart);
    feed.calls();
    frames.time += 60.0;
    frames.run(&mut chart);
    assert!(feed.calls().is_empty(), "old symbol retry survived");
    // A host can use an arbitrarily long rate limit, including Duration::MAX.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.calls();
    feed.page().send(Err(FeedError::RateLimited {
        retry_after: Some(std::time::Duration::MAX),
    }));
    frames.run(&mut chart);
    frames.time += 3600.0;
    frames.run(&mut chart);
    assert!(feed.calls().is_empty(), "long rate limit resumed early");
}

/// Guards the feed contract of the chart: nothing blocks or calls the feed outside `show`,
/// each request is issued once, answers and pushes reach the bars, a push from another thread
/// wakes the UI once per burst, order flow is asked for and reaches the volume bubbles and
/// the footprint chart type while it streams, scroll-back ends on every end signal, a permanent error stops requests until the symbol
/// or timeframe changes, a change releases the old subscription and ignores whatever comes
/// for it, and a dropped chart unsubscribes without waking itself.
#[test]
fn feed_lifecycle() {
    let main = SeriesId::MAIN;
    let mut frames = Frames::new();

    // a. `build` calls nothing; the first frames ask once for the instrument and a page.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    assert_eq!(feed.calls(), [], "a: build");
    frames.run(&mut chart);
    frames.run(&mut chart);
    let first_page = history("A", Timeframe::Min1, None);
    assert_eq!(feed.calls(), [Call::Resolve("A".into()), first_page], "a");

    assert!(
        !chart
            .drain_events()
            .any(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. })),
        "starting history must not report a completed load"
    );
    // b. The page reaches the bars and notifies the host once.
    feed.page().send(page(bars(1000), false));
    frames.run(&mut chart);
    assert_eq!(chart.bars(main), bars(1000), "b");
    assert_eq!(
        chart
            .drain_events()
            .filter(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. }))
            .collect::<Vec<_>>(),
        [super::super::ChartEvent::DataLoaded {
            series: main,
            bars: 1000
        }]
    );
    assert!(!chart.feed_state(main).unwrap().loading_history, "b");

    // c. The instrument subscribes and asks for trades once; a burst from another thread
    // wakes the root viewport once and reaches the forming bar; a lost book asks for one
    // snapshot.
    let instrument = Instrument::new("A").with_tick_size(0.5);
    feed.resolve().send(Ok(instrument.clone()));
    frames.run(&mut chart);
    let id = feed.sink().id;
    let trades = Call::Trades(TradeRequest {
        symbol: "A".into(),
        before: None,
        since: None,
        count: TRADE_CAPACITY,
        conversion: None,
    });
    let subscribe = Call::Subscribe(id, "A".into(), Timeframe::Min1, false);
    assert_eq!(feed.calls(), [subscribe, trades], "c");
    assert_eq!(chart.instrument(), Some(&instrument), "c");
    feed.sink().symbols_changed();
    feed.sink().symbols_changed();
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(
        chart.feed_state(main).unwrap().symbol_catalog_revision,
        1,
        "c: catalog notification coalesces"
    );
    chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
    chart.set_chart_type(ChartType::VolumeFootprint);
    frames.run(&mut chart);
    let woken = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&woken);
    frames
        .ctx
        .set_repaint_observer(move |info| seen.lock().unwrap().push(info.viewport_id));
    let forming = bars(1001).pop().unwrap();
    let sink = feed.sink();
    let ts = forming.time + TimeDelta::seconds(30);
    std::thread::spawn(move || {
        // The last trade repeats an id: it is counted, not folded.
        for id in (0..1000).chain([999]) {
            sink.push_tick(Tick::new(ts, 7.0, 1.0, None).with_id(id));
        }
    })
    .join()
    .unwrap();
    assert_eq!(*woken.lock().unwrap(), [ViewportId::ROOT], "c: one wake");
    frames.run(&mut chart);
    let ticks = Bar::new(forming.time, 7.0, 7.0, 7.0, 7.0, 1000.0);
    assert_eq!(chart.bars(main).last(), Some(&ticks), "c: forming bar");
    assert_eq!(chart.trades(main).count(), 1000, "c: trades");
    let bubbles = chart
        .feed_link
        .as_ref()
        .and_then(|link| link.bubbles.as_ref());
    let shown = bubbles.unwrap().between(forming.time, forming.time);
    let shown = Vec::from_iter(shown.map(|bubble| (bubble.price, bubble.volume)));
    assert_eq!(shown, [(7.0, 1000.0)], "c: the trades reach the bubbles");
    let footprint = chart
        .feed_link
        .as_ref()
        .and_then(|link| link.footprint.as_ref());
    let row = footprint.and_then(|flow| flow.bar_at(forming.time)?.levels.get(&14));
    assert_eq!(
        row.map(|row| row.volume()),
        Some(1000.0),
        "c: and the footprint"
    );
    let frozen = chart.feed_link.as_ref().unwrap().frozen();
    let row = frozen
        .footprint
        .as_ref()
        .and_then(|flow| flow.bar_at(forming.time)?.levels.get(&14));
    assert_eq!(
        row.map(|row| row.volume()),
        Some(1000.0),
        "c: image footprint"
    );
    let shown = frozen
        .bubbles
        .as_ref()
        .unwrap()
        .between(forming.time, forming.time);
    assert_eq!(shown.count(), 1, "c: image bubbles");
    assert!(frozen.paused, "c: an image never loads");
    chart.set_chart_type(ChartType::Candles);
    assert_eq!(chart.feed_state(main).unwrap().repeated_ticks, 1, "c");
    let empty = OrderBook {
        ts,
        symbol: "A".into(),
        bids: Vec::new(),
        asks: Vec::new(),
    };
    feed.sink().push_depth_delta(5, &empty);
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [Call::Snapshot(id)], "c: snapshot");
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [], "c: one snapshot");

    let quote = |time, symbol: &str, bid| crate::model::Level1Quote {
        ts: time,
        symbol: symbol.into(),
        bid,
        ask: 8.0,
        bid_size: 1.0,
        ask_size: 2.0,
    };
    feed.sink().push_quote(quote(ts, "A", 6.0));
    feed.sink()
        .push_quote(quote(ts + TimeDelta::seconds(1), "B", 4.0));
    frames.run(&mut chart);
    assert_eq!(
        chart.quote.as_ref().map(|q| q.bid),
        Some(6.0),
        "c: quote reaches legend"
    );
    feed.sink()
        .push_quote(quote(ts - TimeDelta::seconds(1), "A", 5.0));
    frames.run(&mut chart);
    assert_eq!(
        chart.quote.as_ref().map(|q| q.bid),
        Some(6.0),
        "c: older frame quote ignored"
    );
    feed.sink()
        .push_quote(quote(ts + TimeDelta::seconds(1), "B", 4.0));
    frames.run(&mut chart);
    assert_eq!(
        chart.quote.as_ref().map(|q| q.bid),
        Some(6.0),
        "c: other symbol ignored"
    );

    // h. Bars appended while the view is scrolled back leave the visible bars where they are,
    // and so do updates of the forming bar stamped later inside its slot.
    chart.state.time_scale_mut().set_right_offset(-50.0);
    frames.run(&mut chart);
    let first_visible = |chart: &Chart| chart.bars(main)[chart.state.visible_range().0].time;
    let shown = first_visible(&chart);
    for bar in bars(1004).drain(1001..) {
        feed.sink().push_bar(bar);
    }
    frames.run(&mut chart);
    assert_eq!(chart.bars(main).len(), 1004, "h");
    assert_eq!(first_visible(&chart), shown, "h: the view moved");
    let mut forming = bars(1004).pop().unwrap();
    for seconds in [10, 20] {
        forming.time += TimeDelta::seconds(seconds);
        feed.sink().push_bar(forming.clone());
        frames.run(&mut chart);
    }
    assert_eq!(chart.bars(main).len(), 1004, "h: forming");
    assert_eq!(first_visible(&chart), shown, "h: forming bar");

    // d. Near the oldest bar the chart asks once for the page before it; a page with nothing
    // older ends scroll-back and names the feed in a warning.
    struct Warnings;
    thread_local! {
        static WARNINGS: std::cell::RefCell<Option<Vec<String>>> = const {
            std::cell::RefCell::new(None)
        };
    }
    impl log::Log for Warnings {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() == log::Level::Warn
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                WARNINGS.with_borrow_mut(|warnings| {
                    if let Some(warnings) = warnings {
                        warnings.push(record.args().to_string());
                    }
                });
            }
        }
        fn flush(&self) {}
    }
    log::set_logger(&Warnings).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    WARNINGS.set(Some(Vec::new()));
    chart.set_start_idx(0);
    for _ in 0..5 {
        frames.run(&mut chart);
    }
    let oldest = chart.bars(main)[0].time;
    let older = history("A", Timeframe::Min1, Some(oldest));
    assert_eq!(feed.calls(), [older], "d");
    feed.page().send(page(bars(1005)[400..].to_vec(), false));
    for _ in 0..5 {
        frames.run(&mut chart);
    }
    assert_eq!(
        chart.bars(main).len(),
        1004,
        "d: bars beyond before are dropped"
    );
    assert_eq!(feed.calls(), [], "d: a repeated page ends scroll-back");
    assert!(chart.feed_state(main).unwrap().history_complete, "d");
    assert_eq!(
        WARNINGS.take().unwrap(),
        ["the feed of A sent bars at or after the requested before; those bars were dropped"],
        "d: the warning names the feed"
    );

    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    let held = bars(1000)[500..].to_vec();
    feed.page().send(page(held.clone(), false));
    frames.run(&mut chart);
    chart.set_start_idx(0);
    for _ in 0..5 {
        frames.run(&mut chart);
    }
    assert_eq!(
        feed.calls().last(),
        Some(&history("A", Timeframe::Min1, Some(held[0].time))),
        "d: invalid page request"
    );
    let mut invalid = bars(500);
    for bar in &mut invalid {
        bar.high = bar.low - 1.0;
    }
    WARNINGS.set(Some(Vec::new()));
    feed.page().send(page(invalid, false));
    for _ in 0..5 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.bars(main), held, "d: invalid older bars are dropped");
    assert_eq!(feed.calls(), [], "d: invalid older bars end scroll-back");
    assert!(chart.feed_state(main).unwrap().history_complete, "d");
    assert_eq!(
        WARNINGS.take().unwrap(),
        [
            "dropped 500 invalid bars",
            "the feed of A sent no valid bar older than the oldest one held; scroll-back ended",
        ],
        "d: the invalid page warning names the feed"
    );

    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.page().send(page(bars(1000), true));
    frames.run(&mut chart);
    chart.set_start_idx(0);
    for _ in 0..5 {
        frames.run(&mut chart);
    }
    assert_eq!(feed.calls().len(), 2, "d: end of history");

    // e. A permanently failed page stops scroll-back, also through a setter called every frame with the
    // value it has; a live error does not.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.page().send(page(bars(1000), false));
    frames.run(&mut chart);
    chart.set_start_idx(0);
    frames.run(&mut chart);
    frames.run(&mut chart);
    feed.calls();
    feed.page()
        .send(Err(FeedError::InvalidData("reset".into())));
    frames.run(&mut chart);
    let error = Some(FeedError::InvalidData("reset".into()));
    assert_eq!(chart.feed_state(main).unwrap().last_error, error, "e");
    for _ in 0..10 {
        chart.set_timeframe(Timeframe::Min1);
        frames.run(&mut chart);
    }
    assert_eq!(feed.calls(), [], "e: no retry");
    assert_eq!(chart.bars(main), bars(1000), "e: bars kept");

    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    feed.page().send(page(bars(1000), false));
    frames.run(&mut chart);
    feed.sink().report_error(FeedError::Network("reset".into()));
    frames.run(&mut chart);
    assert_eq!(
        chart.feed_state(main).unwrap().last_error,
        Some(FeedError::Network("reset".into())),
        "e: live"
    );
    chart.set_start_idx(0);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let calls = feed.calls();
    let older = history("A", Timeframe::Min1, Some(bars(1)[0].time));
    assert_eq!(calls.last(), Some(&older), "e: a live error stops nothing");

    // f. A change of symbol and timeframe between two frames is one load at the next frame,
    // which releases the old subscription and drops its trades and bubbles; what arrives for
    // the old one changes nothing.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_price_decimals(3)));
    frames.run(&mut chart);
    let precision = |chart: &Chart| {
        chart
            .state
            .price_scale()
            .map(crate::scales::PriceTransform::default(), 0.0, 100.0)
            .price_precision()
    };
    assert_eq!(precision(&chart), Some(3), "f: supplied precision");
    let (old_sink, old_page) = (feed.sink(), feed.page());
    chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
    old_sink.push_tick(Tick::new(Utc::now(), 7.0, 1.0, None));
    frames.run(&mut chart);
    assert_eq!(chart.trades(main).count(), 1, "f: a trade of the old key");
    feed.calls();
    chart.set_symbol("B");
    chart.set_timeframe(Timeframe::Hour1);
    assert_eq!(feed.calls(), [], "f: no call from a setter");
    frames.run(&mut chart);
    let load = [
        Call::Unsubscribe(old_sink.id),
        Call::Resolve("B".into()),
        history("B", Timeframe::Hour1, None),
    ];
    assert_eq!(feed.calls(), load, "f");
    assert!(!old_sink.is_active(), "f: old sink");
    chart.drain_events().for_each(drop);
    old_sink.push_bar(bars(1)[0].clone());
    old_page.send(page(bars(10), false));
    frames.run(&mut chart);
    assert_eq!(chart.bars(main), [], "f: nothing of the old key");
    assert!(
        !chart
            .drain_events()
            .any(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. }))
    );
    assert_eq!(precision(&chart), None, "f: old precision cleared");
    let bubbles = chart
        .feed_link
        .as_ref()
        .and_then(|link| link.bubbles.as_ref());
    let all = bubbles
        .unwrap()
        .between(DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC);
    assert_eq!(all.count(), 0, "f: no bubble of the old key");
    for _ in 0..3 {
        chart.set_timeframe(Timeframe::Hour1);
        frames.run(&mut chart);
    }
    assert_eq!(feed.calls(), [], "f: an unchanged value reloads nothing");

    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    chart.set_symbol("B");
    frames.run(&mut chart);
    let calls = feed.calls();
    assert!(
        !calls.iter().any(|call| matches!(call, Call::Subscribe(..))),
        "f: a late resolve subscribes nothing: {calls:?}"
    );

    // g. Without a symbol the feed is never called. Bars the host sets keep one per slot.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, None);
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [], "g");
    let mut set = bars(2);
    let mut later = set[0].clone();
    later.time += TimeDelta::seconds(20);
    set.insert(1, later.clone());
    chart.set_data(main, set);
    assert_eq!(chart.bars(main), [later, bars(2)[1].clone()], "g: slot");

    // i. A chart dropped inside an input handler, while the feed drops its replies in
    // `unsubscribe`, unsubscribes and returns.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let sink = feed.sink();
    feed.calls();
    feed.drop_replies.store(true, Ordering::Relaxed);
    frames.ctx.input(move |_| drop(chart));
    assert_eq!(feed.calls(), [Call::Unsubscribe(sink.id)], "i");
    assert!(!sink.is_active(), "i: sink");

    // j. Labelling a chart without a feed keeps its bars.
    let mut chart = Chart::new(BarData::from_bars(bars(10)));
    chart.set_symbol("X");
    chart.set_timeframe(Timeframe::Hour1);
    frames.run(&mut chart);
    assert_eq!(chart.bars(main), bars(10), "j");

    // k. Scroll-back counts elements; older history adds none to a trimmed build.
    for (size, asks) in [(1.0, true), (1.0 / 16.0, false)] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(crate::model::ChartType::Renko);
        chart.set_chart_type_params(crate::model::ChartTypeParams {
            renko_brick_size: size,
            ..Default::default()
        });
        frames.run(&mut chart);
        feed.page().send(page(bars(1000), false));
        frames.run(&mut chart);
        feed.calls();
        chart.set_start_idx(0);
        for _ in 0..5 {
            frames.run(&mut chart);
        }
        let older = history("A", Timeframe::Min1, Some(bars(1)[0].time));
        assert_eq!(feed.calls().contains(&older), asks, "k: brick {size}");
    }
    // Removing the main series keeps its reference history even when playback resumes.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.page().send(page(bars(1000), true));
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let id = feed.sink().id;
    feed.calls();
    assert!(chart.remove_series(main));
    assert_eq!(feed.calls(), [Call::Unsubscribe(id)]);
    chart.set_feed_paused(false);
    frames.run(&mut chart);
    assert!(
        feed.calls().is_empty(),
        "removed main cannot restart its feed"
    );
    assert_eq!(chart.state.data().bars, bars(1000));
}

/// Redoing a main series removal before the reload started by its undo answers leaves the
/// remaining series the time axis the first removal kept.
#[test]
fn redo_during_the_undo_reload_keeps_the_time_axis() {
    let main = SeriesId::MAIN;
    let mut frames = Frames::new();
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    frames.run(&mut chart);
    feed.page().send(page(bars(1000), false));
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let other = chart.add_series(crate::widget::SeriesOptions::default());
    chart.set_data(other, bars(1000));
    frames.run(&mut chart);
    assert!(chart.user_action(|chart| chart.remove_series(main)));
    frames.run(&mut chart);
    assert!(chart.visible_logical_range().is_some(), "removed");
    assert!(chart.undo());
    frames.run(&mut chart);
    assert!(chart.redo());
    frames.run(&mut chart);
    while let Some(reply) = feed.pages.lock().unwrap().pop() {
        reply.send(page(bars(1000), false));
    }
    frames.run(&mut chart);
    assert!(chart.visible_logical_range().is_some(), "redone");
}
#[cfg(feature = "studies")]
#[test]
fn stream_studies_keep_every_quote_and_book_loss_between_frames() {
    use crate::model::{Level1Quote, OrderBookLevel};
    use crate::studies::{IndicatorFactory, InputValue};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let factory = IndicatorFactory::new();
    let names = [
        "Quoted Spread",
        "Book Size",
        "Trade Rate",
        "Levels Traded Through",
    ];
    let ids: Vec<_> = names
        .iter()
        .map(|name| chart.add_indicator(factory.create(name).unwrap()))
        .collect();
    let mut volume = factory.create("Trade Rate").unwrap();
    volume
        .set_input("volume", InputValue::Toggle(true))
        .unwrap();
    let volume_id = chart.add_indicator(volume);
    let mut near = factory.create("Book Size").unwrap();
    near.set_input("levels", InputValue::Length(1)).unwrap();
    let near_id = chart.add_indicator(near);
    let mut distance = factory.create("Book Size").unwrap();
    distance
        .set_input("distance", InputValue::Number(1.0))
        .unwrap();
    let distance_id = chart.add_indicator(distance);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(bars(3), true));
    let mut instrument = Instrument::new("A");
    instrument.tick_size = Some(1.0);
    feed.resolve().send(Ok(instrument));
    frames.run(&mut chart);
    chart.feed_link.as_mut().unwrap().trades = Trades::new(3);
    let sink = feed.sink();
    let start = bars(1)[0].time;
    let at = |second| start + TimeDelta::seconds(second);
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(
            vec![Tick::new(at(2), 100.0, 3.0, None).with_id(1)],
            true,
        )));
    for (second, bid, ask) in [(0, 99.0, 101.0), (10, 100.0, 103.0), (20, 98.0, 102.0)] {
        sink.push_quote(Level1Quote {
            ts: at(second),
            symbol: "A".into(),
            bid,
            ask,
            bid_size: 1.0,
            ask_size: 1.0,
        });
    }
    let level = |price, size| OrderBookLevel {
        price,
        size,
        order_cnt: None,
    };
    let book = |second, bids, asks| OrderBook {
        ts: at(second),
        symbol: "A".into(),
        bids,
        asks,
    };
    sink.push_depth_snapshot(
        1,
        book(
            0,
            vec![level(99.0, 2.0), level(98.0, 3.0)],
            vec![level(101.0, 7.0), level(102.0, 11.0)],
        ),
    );
    // A sequence gap erases the first book even though it was valid earlier in this bar.
    sink.push_depth_delta(3, &book(20, vec![], vec![]));
    for _ in 0..12 {
        frames.run(&mut chart);
    }
    assert!(
        chart
            .indicators()
            .find(|(id, _)| *id == ids[1])
            .unwrap()
            .1
            .values()
            .value(0, 0)
            .is_none()
    );
    sink.push_depth_snapshot(
        4,
        book(
            30,
            vec![level(98.0, 6.0), level(97.0, 9.0)],
            vec![level(102.0, 8.0), level(103.0, 12.0)],
        ),
    );
    // No quotes in bar 1. The first quote after loss seeds the path rather than counting
    // an unobserved jump from the old inside market.
    sink.push_quote(Level1Quote {
        ts: at(125),
        symbol: "A".into(),
        bid: 105.0,
        ask: 107.0,
        bid_size: 1.0,
        ask_size: 1.0,
    });
    for (id, (second, size)) in [(0, 2.0), (2, 3.0), (5, 5.0), (65, 7.0)]
        .into_iter()
        .enumerate()
    {
        sink.push_tick(Tick::new(at(second), 100.0, size, None).with_id(id as u64));
    }
    sink.push_tick(Tick::new(at(65), 100.0, 7.0, None).with_id(3));
    for _ in 0..20 {
        frames.run(&mut chart);
    }
    let expected = [
        (
            ids[0],
            [[Some(4.0), Some(3.0), Some(4.0)], [None; 3], [Some(2.0); 3]],
        ),
        (
            ids[1],
            [[Some(15.0), Some(20.0), Some(-5.0)], [None; 3], [None; 3]],
        ),
        (
            near_id,
            [[Some(6.0), Some(8.0), Some(-2.0)], [None; 3], [None; 3]],
        ),
        (
            distance_id,
            [[Some(15.0), Some(20.0), Some(-5.0)], [None; 3], [None; 3]],
        ),
        (
            ids[2],
            [
                [Some(3.0), Some(3.0 / 60.0), Some(1.0 / 3.0)],
                [Some(1.0), Some(1.0 / 60.0), Some(1.0 / 3.0)],
                [None; 3],
            ],
        ),
        (
            volume_id,
            [
                [Some(10.0), Some(10.0 / 60.0), Some(5.0 / 3.0)],
                [Some(7.0), Some(7.0 / 60.0), Some(7.0 / 3.0)],
                [None; 3],
            ],
        ),
        (
            ids[3],
            [[Some(3.0), Some(3.0), Some(6.0)], [None; 3], [Some(0.0); 3]],
        ),
    ];
    for (id, rows) in expected {
        let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
        for (bar, row) in rows.into_iter().enumerate() {
            for (line, want) in row.into_iter().enumerate() {
                assert_eq!(
                    study.values().value(line, bar),
                    want,
                    "{} bar {bar} line {line}",
                    study.name()
                );
            }
        }
    }
    sink.advance_time(at(68));
    for _ in 0..4 {
        frames.run(&mut chart);
    }
    for id in [ids[2], volume_id] {
        let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
        assert_eq!(study.values().value(2, 1), Some(0.0), "idle instant rate");
    }
    let fresh = chart.add_indicator(factory.create("Trade Rate").unwrap());
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    let (_, study) = chart.indicators().find(|(id, _)| *id == fresh).unwrap();
    assert_eq!(
        study.values().value(0, 0),
        Some(2.0),
        "new rate replays the two retained bar-0 executions"
    );
    let (_, study) = chart.indicators().find(|(id, _)| *id == ids[2]).unwrap();
    assert_eq!(
        study.values().value(0, 0),
        Some(3.0),
        "existing rate keeps the execution already evicted from raw history"
    );
    assert_eq!(
        study.values().value(2, 1),
        Some(0.0),
        "adding another study preserves the idle clock"
    );
    let band = crate::data::OrderFlowInput::new(crate::data::OrderFlowKind::BookSize {
        levels: 10,
        distance: 0.3,
    });
    band.push_book(
        &Timeframe::Min1.into(),
        at(130),
        Some(&book(
            130,
            vec![level(0.4, 2.0), level(0.1, 3.0)],
            vec![level(0.7, 7.0), level(1.0, 11.0)],
        )),
    );
    assert_eq!(
        band.values()[&at(120)],
        [5.0, 18.0, -13.0],
        "decimal price-distance endpoints are inclusive"
    );
    let input = chart
        .indicators()
        .find(|(id, _)| *id == ids[2])
        .unwrap()
        .1
        .order_flow_input()
        .unwrap();
    input.clear();
    input.push_trade(&chart.bar_clock(), &Tick::new(at(125), 100.0, 1.0, None));
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    let (_, study) = chart.indicators().find(|(id, _)| *id == ids[2]).unwrap();
    assert_eq!(
        study.values().value(0, 0),
        None,
        "clearing an input removes earlier plot values"
    );
    assert_eq!(study.values().value(0, 1), None);
    assert_eq!(study.values().value(0, 2), Some(1.0));

    let copy = study.clone_box();
    let copy_id = chart.add_indicator(copy);
    frames.run(&mut chart);
    sink.push_tick(Tick::new(at(130), 100.0, 2.0, None).with_id(4));
    sink.advance_time(at(134));
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    for id in [ids[2], copy_id] {
        let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
        assert_eq!(
            study.values().value(0, 2),
            Some(2.0),
            "shared sources count once"
        );
        assert_eq!(
            study.values().value(2, 2),
            Some(0.0),
            "heartbeat follows queued trades"
        );
    }
    let fresh = chart.add_indicator(factory.create("Trade Rate").unwrap());
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    let (_, study) = chart.indicators().find(|(id, _)| *id == fresh).unwrap();
    assert_eq!(
        study.values().value(2, 2),
        Some(0.0),
        "new sources retain the idle clock"
    );
}

/// A new bar that crosses no input drop updates order-flow studies from that bar: closed bars
/// keep their values and no order-flow study blanks for a frame.
/// Hand values: one buy of 1 in each of 11,000 one-minute bars. Each input holds all 11,000,
/// so the CVD of bar 10,999 is 11,000 and a 10-bar flow sum is 10. Bar 11,000 is the 11,001st:
/// the input drops its oldest 1,001 and the CVD starts again at bar 1,001, 9,999 at bar 10,999.
/// Bar 11,001 crosses no boundary, so both studies update on its first frame and bar 10,999
/// keeps 9,999 and 10.
#[cfg(feature = "studies")]
#[test]
fn order_flow_studies_update_incrementally_between_input_drops() {
    use crate::data::OrderFlowKind;
    use crate::model::OrderSide;
    use crate::studies::OrderFlowStudy;
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let ids = [
        OrderFlowKind::FilteredCvd {
            min: 0.0,
            max: f64::MAX,
        },
        OrderFlowKind::FlowSum { bars: 10 },
    ]
    .map(|kind| chart.add_indicator(Box::new(OrderFlowStudy::new(kind))));
    let all = bars(11_002);
    let buy = |bar: &Bar| Tick::new(bar.time, bar.close, 1.0, Some(OrderSide::Buy));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(all[..11_000].to_vec(), true));
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    let trades = TradePage::new(all[..11_000].iter().map(buy).collect(), true);
    feed.trades.lock().unwrap().remove(0).send(Ok(trades));
    let record = |chart: &Chart| {
        ids.map(|id| {
            let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
            (study.values().len(), study.values().value(0, 10_999))
        })
    };
    for _ in 0..30 {
        frames.run(&mut chart);
    }
    let before = record(&chart);
    feed.sink().push_tick(buy(&all[11_000]));
    for _ in 0..10 {
        frames.run(&mut chart);
    }
    let dropped = record(&chart);
    feed.sink().push_tick(buy(&all[11_001]));
    frames.run(&mut chart);
    let first = record(&chart);
    assert_eq!(
        [before, dropped, first],
        [
            [(11_000, Some(11_000.0)), (11_000, Some(10.0))],
            [(11_001, Some(9_999.0)), (11_001, Some(10.0))],
            [(11_002, Some(9_999.0)), (11_002, Some(10.0))],
        ]
    );
}
