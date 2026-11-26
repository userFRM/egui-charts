use super::super::test_frames::{candle_ys, frame, shapes};
use super::*;
use crate::data::{
    DepthPage, FeedError, FootprintPage, FootprintRequest, FootprintView, HistoryPage, Sink,
    Subscription, TradePage,
};
use crate::model::{Bar, BarData, ChartType};
use crate::widget::ChartBuilder;
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "rebinding_tests.rs"]
mod rebinding;

#[path = "partial_tests.rs"]
mod partial;

#[path = "footprint_demand_tests.rs"]
mod footprint_demand;

#[path = "checkpoint_tests.rs"]
mod checkpoint;

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Resolve(String),
    History(HistoryRequest),
    Subscribe(SubscriptionId, String, Timeframe, (bool, bool)),
    Unsubscribe(SubscriptionId),
    Trades(TradeRequest),
    Footprints(FootprintRequest),
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
    footprints: Mutex<Vec<Reply<FootprintPage>>>,
    prepared_footprints: AtomicBool,
    depth: Mutex<Vec<Reply<DepthPage>>>,
    sinks: Mutex<Vec<Sink>>,
    /// `unsubscribe` drops the replies it keeps.
    drop_replies: AtomicBool,
    capabilities: Mutex<Vec<Reply<FeedCapabilities>>>,
    targets: Mutex<Vec<Reply<crate::data::PriceTargets>>>,
    teardown_push: Mutex<Option<Sink>>,
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
            overnight_hours,
            sink,
            ..
        } = subscription;
        let call = Call::Subscribe(id, symbol, timeframe, (extended_hours, overnight_hours));
        self.calls.lock().unwrap().push(call);
        self.sinks.lock().unwrap().push(sink);
    }
    fn unsubscribe(&self, id: SubscriptionId) {
        self.calls.lock().unwrap().push(Call::Unsubscribe(id));
        if let Some(sink) = self.teardown_push.lock().unwrap().as_ref() {
            sink.push_tick(Tick::new(bars(1)[0].time, 999.0, 999.0, None));
        }
        if self.drop_replies.load(Ordering::Relaxed) {
            self.resolves.lock().unwrap().clear();
            self.pages.lock().unwrap().clear();
            self.trades.lock().unwrap().clear();
            self.footprints.lock().unwrap().clear();
        }
    }
    fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
        self.calls.lock().unwrap().push(Call::Trades(request));
        self.trades.lock().unwrap().push(reply);
    }
    fn footprints(&self, request: FootprintRequest, reply: Reply<FootprintPage>) {
        if self.prepared_footprints.load(Ordering::Relaxed) {
            self.calls.lock().unwrap().push(Call::Footprints(request));
            self.footprints.lock().unwrap().push(reply);
        } else {
            reply.send(Err(FeedError::NotSupported));
        }
    }
    fn depth_history(&self, request: DepthRequest, reply: Reply<DepthPage>) {
        self.calls.lock().unwrap().push(Call::Depth(request));
        self.depth.lock().unwrap().push(reply);
    }
    fn request_depth_snapshot(&self, id: SubscriptionId) {
        self.calls.lock().unwrap().push(Call::Snapshot(id));
    }
    fn price_targets(&self, _: &str, reply: Reply<crate::data::PriceTargets>) {
        self.targets.lock().unwrap().push(reply);
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

#[test]
fn instrument_ticks_supply_default_marks_and_preserve_host_overrides() {
    use crate::scales::{PriceScaleMode, PriceTransform};

    let marks = |chart: &mut Chart, mode, height| {
        let scale = chart.price_scale_mut();
        let transform = PriceTransform { mode, base: 6100.0 };
        let (min, max) = if mode == PriceScaleMode::Logarithmic {
            (6104.0_f64.ln(), 6111.0_f64.ln())
        } else {
            (6104.0, 6111.0)
        };
        scale.set_range(min, max);
        scale.map(transform, 0.0, height).marks()
    };
    for feed in [false, true] {
        let ctx = egui::Context::default();
        let script = Arc::new(Script::default());
        let mut chart = if feed {
            fed(&script, Some("A"))
        } else {
            Chart::new(BarData::default())
        };
        if feed {
            chart.pump(&ctx);
            script.resolve().send(Ok(Instrument::new("A")
                .with_tick_size(0.25)
                .with_price_decimals(2)));
            chart.pump(&ctx);
        } else {
            chart.set_instrument(Some(
                Instrument::new("A")
                    .with_tick_size(0.25)
                    .with_price_decimals(2),
            ));
        }
        let prices: Vec<_> = marks(&mut chart, PriceScaleMode::Normal, 800.0)
            .iter()
            .map(|mark| mark.price)
            .collect();
        // A 0.4 nice step rounds up to two quarter ticks, starting at 6104.
        assert_eq!(
            prices,
            (0..=14)
                .map(|i| 6104.0 + i as f64 * 0.5)
                .collect::<Vec<_>>(),
            "feed: {feed}"
        );
        for mode in [PriceScaleMode::Normal, PriceScaleMode::Logarithmic] {
            for height in [500.0, 800.0] {
                let marks = marks(&mut chart, mode, height);
                assert!(marks.len() >= 3, "feed: {feed}, {mode:?}, height: {height}");
                for mark in marks {
                    assert!((mark.price * 4.0 - (mark.price * 4.0).round()).abs() < 1e-8);
                }
            }
        }
        if feed {
            // Feed metadata takes precedence over a later host update.
            chart.set_instrument(Some(Instrument::new("A").with_tick_size(1.0)));
            assert_eq!(marks(&mut chart, PriceScaleMode::Normal, 800.0).len(), 15);
        }

        #[cfg(feature = "ui")]
        {
            let mut saved = chart.snapshot().unwrap();
            saved.price_scale = Some(crate::scales::PriceScale::new(Default::default()));
            chart.restore(&saved).unwrap();
            chart.pump(&ctx);
            assert_eq!(marks(&mut chart, PriceScaleMode::Normal, 800.0).len(), 15);
        }

        for (override_before, step, count) in [(false, 0.2, 18), (true, 1.0, 8)] {
            if override_before {
                chart
                    .price_scale_mut()
                    .options_mut()
                    .tick_marks
                    .min_price_step = Some(step);
            }
            chart.set_instrument(Some(Instrument::new("A").with_tick_size(0.25)));
            if !override_before {
                chart
                    .price_scale_mut()
                    .options_mut()
                    .tick_marks
                    .min_price_step = Some(step);
            }
            assert_eq!(
                marks(&mut chart, PriceScaleMode::Normal, 800.0).len(),
                count
            );
            chart
                .price_scale_mut()
                .options_mut()
                .tick_marks
                .min_price_step = None;
            assert_eq!(marks(&mut chart, PriceScaleMode::Normal, 800.0).len(), 15);
        }

        chart.set_instrument(None);
        if !feed {
            assert!(
                marks(&mut chart, PriceScaleMode::Normal, 800.0)
                    .iter()
                    .any(|mark| (mark.price - 6104.4).abs() < 1e-8)
            );
            chart.set_instrument(Some(Instrument::new("A").with_tick_size(0.25)));
        }
        chart.set_symbol("B");
        assert!(
            marks(&mut chart, PriceScaleMode::Normal, 800.0)
                .iter()
                .any(|mark| (mark.price - 6104.4).abs() < 1e-8)
        );
        if feed {
            chart.pump(&ctx);
            script
                .resolve()
                .send(Ok(Instrument::new("B").with_tick_size(50.0)));
            chart.pump(&ctx);
        } else {
            chart.set_instrument(Some(Instrument::new("B").with_tick_size(50.0)));
        }
        // Instrument price units must not constrain percentage or index units.
        for mode in [PriceScaleMode::Percentage, PriceScaleMode::IndexedTo100] {
            assert!(marks(&mut chart, mode, 800.0).len() >= 15);
        }
    }
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

/// Live executions that fill the trade store leave no room for older ones, so none are asked
/// for: a feed's empty answer to a request for none would read as the start of trade history.
#[test]
fn a_trade_store_filled_live_asks_for_no_older_trades() {
    let all = bars(1000);
    let oldest = all[999].time;
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(all, false));
    feed.resolve().send(Ok(Instrument::new("A")));
    frames.run(&mut chart);
    chart.feed_link.as_mut().unwrap().trades = Trades::new(2);
    feed.calls();
    // The latest page leaves room for older trades; the stream fills it in the same drain.
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(
            vec![Tick::new(oldest, 100.0, 1.0, None)],
            false,
        )));
    for second in 1..3 {
        feed.sink().push_tick(Tick::new(
            oldest + TimeDelta::seconds(second),
            100.0,
            1.0,
            None,
        ));
    }
    frames.run(&mut chart);
    assert_eq!(
        feed.calls()
            .into_iter()
            .filter(|call| matches!(call, Call::Trades(_)))
            .collect::<Vec<_>>(),
        []
    );
    assert!(
        chart
            .feed_state(SeriesId::MAIN)
            .unwrap()
            .trade_history_complete
    );
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

/// An interval change empties the main series until its history arrives, while the added series
/// keep their bars, so a zoomed-out chart lays out their slots with no main bars behind them.
#[test]
fn interval_change_with_an_added_series_paints_while_history_loads() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(bars(1000), true));
    let added = chart.add_series(crate::widget::SeriesOptions::default());
    chart.set_data(added, bars(1000));
    frames.run(&mut chart);
    chart.fit_content();
    frames.run(&mut chart);
    chart.set_timeframe(Timeframe::Min5);
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert!(chart.bars(SeriesId::MAIN).is_empty(), "history is loading");
    // 200 one-minute bars fall into 200 / 5 = 40 five-minute bars.
    feed.page().send(page(bars(200), true));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 40);
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

/// A precision chosen in the chart settings outlasts an interval change, a reload, a pause and
/// resume, a symbol change and a save and restore, while each instrument's decimals arrive
/// beneath it, and "Default" follows the instrument again. A layout saved before a precision
/// could be chosen held the precision shown, which restores as none chosen and gives way to the
/// next instrument's, as it did.
#[test]
fn chosen_precision_survives_interval_symbol_and_reload() {
    use crate::widget::settings::{ChartSettingsState, PrecisionMode};
    let answer = |feed: &Script, symbol: &str, decimals: u8| {
        while !feed.resolves.lock().unwrap().is_empty() {
            let instrument = Instrument::new(symbol).with_price_decimals(decimals);
            feed.resolve().send(Ok(instrument));
        }
    };
    let shown = |chart: &Chart| {
        let precision = ChartSettingsState::from_chart(chart).precision;
        (chart.state.price_scale().price_precision(), precision)
    };
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    answer(&feed, "A", 2);
    frames.run(&mut chart);
    assert_eq!(
        shown(&chart),
        (Some(2), PrecisionMode::Default),
        "never chosen"
    );
    let mut settings = ChartSettingsState::from_chart(&chart);
    settings.precision = PrecisionMode::Decimals5;
    settings.apply_to_chart(&mut chart).unwrap();
    let chosen = (Some(5), PrecisionMode::Decimals5);
    type Step<'a> = (&'a str, &'a dyn Fn(&mut Chart));
    let steps: [Step; 4] = [
        ("interval", &|chart| chart.set_timeframe(Timeframe::Hour1)),
        ("reload", &|chart| chart.reload_feed()),
        ("pause", &|chart| chart.set_feed_paused(true)),
        ("resume", &|chart| chart.set_feed_paused(false)),
    ];
    for (step, change) in steps {
        change(&mut chart);
        frames.run(&mut chart);
        answer(&feed, "A", 2);
        frames.run(&mut chart);
        assert_eq!(shown(&chart), chosen, "{step}");
    }
    chart.set_symbol("B");
    frames.run(&mut chart);
    answer(&feed, "B", 3);
    frames.run(&mut chart);
    assert_eq!(shown(&chart), chosen, "symbol");

    #[cfg(feature = "ui")]
    {
        let saved = chart.snapshot().unwrap();
        let restored_feed = Arc::new(Script::default());
        let mut restored = fed(&restored_feed, Some("B"));
        restored.restore(&saved).unwrap();
        frames.run(&mut restored);
        answer(&restored_feed, "B", 3);
        frames.run(&mut restored);
        assert_eq!(shown(&restored), chosen, "restored");

        // Before, the saved scale held the five decimals shown and no choice.
        let mut older = serde_json::to_value(&saved).unwrap();
        let scale = older["price_scale"].as_object_mut().unwrap();
        assert!(scale.remove("chosen_precision").is_some());
        scale["price_precision"] = 5.into();
        let older: crate::widget::ChartSnapshot = serde_json::from_value(older).unwrap();
        let older_feed = Arc::new(Script::default());
        let mut restored = fed(&older_feed, Some("B"));
        restored.restore(&older).unwrap();
        assert_eq!(
            shown(&restored),
            (Some(5), PrecisionMode::Default),
            "older save"
        );
        frames.run(&mut restored);
        answer(&older_feed, "B", 3);
        frames.run(&mut restored);
        assert_eq!(
            shown(&restored),
            (Some(3), PrecisionMode::Default),
            "older save, fed"
        );

        // A layout saved without its scale holds only the precision shown, in its settings.
        let mut unscaled = serde_json::to_value(&saved).unwrap();
        let scale = unscaled.as_object_mut().unwrap().remove("price_scale");
        assert!(scale.is_some());
        let unscaled: crate::widget::ChartSnapshot = serde_json::from_value(unscaled).unwrap();
        let unscaled_feed = Arc::new(Script::default());
        let mut restored = fed(&unscaled_feed, Some("B"));
        restored.restore(&unscaled).unwrap();
        frames.run(&mut restored);
        answer(&unscaled_feed, "B", 3);
        frames.run(&mut restored);
        assert_eq!(
            shown(&restored),
            (Some(3), PrecisionMode::Default),
            "saved without a scale"
        );
    }

    let mut settings = ChartSettingsState::from_chart(&chart);
    settings.precision = PrecisionMode::Default;
    settings.apply_to_chart(&mut chart).unwrap();
    assert_eq!(shown(&chart), (Some(3), PrecisionMode::Default), "default");
}

/// An instrument quoted in thirty-seconds labels the price scale, the price tag and the legend
/// in them, from a feed and from `set_instrument` alike, while decimals chosen in the chart
/// settings win and "Default" follows the instrument again.
#[test]
fn instrument_price_fraction_labels_the_scale_and_legend() {
    use crate::chart::rendering::axes::price_marks;
    use crate::model::PriceFraction;
    use crate::scales::PriceScaleMode;
    use crate::widget::settings::{ChartSettingsState, PrecisionMode};
    let start = "2024-03-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    let minute = |index: i64| start + TimeDelta::minutes(index);
    let bars = vec![
        Bar::new(minute(0), 117.0, 117.0, 117.0, 117.0, 1.0),
        Bar::new(minute(1), 117.0, 117.0, 117.0, 117.0, 1.0),
        Bar::new(minute(2), 117.25, 117.75, 117.0, 117.5, 1.0),
    ];
    let instrument = Instrument::new("ZB")
        .with_tick_size(1.0 / 32.0)
        .with_price_fraction(PriceFraction::ThirtySeconds);
    let texts = |chart: &mut Chart, frames: &mut Frames| -> Vec<String> {
        frames.time += 1.0 / 60.0;
        shapes(&frames.ctx, chart, frames.time, Vec::new())
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    };
    let precision = |chart: &mut Chart, mode: PrecisionMode| {
        let mut settings = ChartSettingsState::from_chart(chart);
        settings.precision = mode;
        settings.apply_to_chart(chart).unwrap();
    };

    let feed = Arc::new(Script::default());
    let mut from_feed = fed(&feed, Some("ZB"));
    let mut frames = Frames::new();
    frames.run(&mut from_feed);
    feed.resolve().send(Ok(instrument.clone()));
    feed.page().send(page(bars.clone(), true));
    frames.run(&mut from_feed);
    frames.run(&mut from_feed);
    let mut supplied = Chart::new(BarData::from_bars(bars));
    supplied.set_instrument(Some(instrument));
    let mut supplied_frames = Frames::new();
    supplied_frames.run(&mut supplied);
    supplied_frames.run(&mut supplied);

    for (name, chart, frames) in [
        ("feed", &mut from_feed, &mut frames),
        ("set_instrument", &mut supplied, &mut supplied_frames),
    ] {
        let painted = texts(chart, frames);
        assert!(
            painted.iter().any(|text| text == "O 117'08"),
            "{name}: {painted:?}"
        );
        assert!(
            painted.iter().any(|text| text == "117'16"),
            "{name}: {painted:?}"
        );
        let map = chart.series_price_map(SeriesId::MAIN).unwrap();
        let formatter = chart
            .series_label_formatter(SeriesId::MAIN, PriceScaleMode::Normal)
            .unwrap();
        let marks = price_marks(&map, Some(formatter.as_ref()));
        assert!(!marks.is_empty(), "{name}");
        assert!(
            marks
                .iter()
                .all(|mark| (mark.price * 32.0).fract() == 0.0 && mark.label.contains('\'')),
            "{name}: {marks:?}"
        );
        assert!(
            marks.iter().any(|mark| painted.contains(&mark.label)),
            "{name}: {marks:?} {painted:?}"
        );

        precision(chart, PrecisionMode::Decimals3);
        let painted = texts(chart, frames);
        assert!(
            painted.iter().any(|text| text == "O 117.250"),
            "{name}: {painted:?}"
        );
        assert!(
            painted.iter().any(|text| text == "117.500"),
            "{name}: {painted:?}"
        );
        assert!(!painted.iter().any(|text| text == "O 117'08"), "{name}");

        precision(chart, PrecisionMode::Default);
        let painted = texts(chart, frames);
        assert!(
            painted.iter().any(|text| text == "O 117'08"),
            "{name}: {painted:?}"
        );
    }

    supplied.set_instrument(None);
    let painted = texts(&mut supplied, &mut supplied_frames);
    assert!(
        !painted.iter().any(|text| text == "O 117'08"),
        "{painted:?}"
    );
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
        overnight_hours: false,
        adjusted: false,
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

fn prepared_chart() -> (Arc<Script>, Chart, Frames) {
    prepared_chart_with_trade_policy(false)
}

fn prepared_chart_with_trade_policy(on_demand: bool) -> (Arc<Script>, Chart, Frames) {
    let feed = Arc::new(Script::default());
    feed.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = fed(&feed, Some("A"));
    chart.set_trade_history_on_demand(on_demand);
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(5), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    (feed, chart, frames)
}

fn prepared_page(trades: &[Tick], before: DateTime<Utc>, end: bool) -> FootprintPage {
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    for trade in trades {
        flow.push(trade).unwrap();
    }
    FootprintPage::new(flow.bars().values().cloned().collect(), before, end)
}

fn footprint_requests(feed: &Script) -> Vec<FootprintRequest> {
    feed.calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::Footprints(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn prepared_view(chart: &Chart) -> FootprintView<'_> {
    chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .view()
}

#[cfg(feature = "studies")]
#[test]
fn prepared_rows_wait_for_real_executions_to_fill_trade_rate() {
    let (feed, mut chart, mut frames) = prepared_chart_with_trade_policy(true);
    let start = bars(1)[0].time;
    let tape = vec![
        Tick::new(start + TimeDelta::seconds(2), 100.0, 3.0, None),
        Tick::new(start + TimeDelta::seconds(8), 100.0, 5.0, None),
    ];
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            &tape,
            start + TimeDelta::minutes(5),
            true,
        )));
    frames.run(&mut chart);
    assert_eq!(prepared_view(&chart).bar_at(start).unwrap().volume(), 8.0);
    assert!(feed.trades.lock().unwrap().is_empty());

    let factory = crate::studies::IndicatorFactory::new();
    let rate = chart.add_indicator(factory.create("Trade Rate").unwrap());
    let count = |chart: &Chart| {
        let (_, study) = chart.indicators().find(|(id, _)| *id == rate).unwrap();
        study.values().value(0, 0)
    };
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(count(&chart), None);
    assert_eq!(feed.trades.lock().unwrap().len(), 1);
    chart.request_trade_history();
    frames.run(&mut chart);
    assert_eq!(feed.trades.lock().unwrap().len(), 1);
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(tape, true)));
    for _ in 0..3 {
        chart.request_trade_history();
        frames.run(&mut chart);
    }
    assert_eq!(count(&chart), Some(2.0));
    assert!(feed.trades.lock().unwrap().is_empty());
}

#[cfg(feature = "studies")]
#[test]
fn time_bar_trade_rate_requires_coverage_for_each_output() {
    use crate::data::OrderFlowKind;
    use crate::studies::OrderFlowStudy;

    let start = "2025-01-01T00:01:00Z".parse::<DateTime<Utc>>().unwrap();
    let fill = |ts, id| Tick::new(ts, 100.0, 1.0, None).with_id(id);
    let candle = |open, volume| Bar::new(open, 100.0, 100.0, 100.0, 100.0, volume);
    let ordinary = &[(5, 1), (15, 2)][..];
    let tied_open = &[(0, 10), (0, 11), (30, 12)][..];
    for (fills, held, complete) in [
        (ordinary, 2, true),
        (ordinary, TRADE_CAPACITY, false),
        (ordinary, 2, false),
        (tied_open, 2, false),
        (tied_open, 3, true),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(Timeframe::Min1);
        let rate = chart.add_indicator(Box::new(OrderFlowStudy::new(OrderFlowKind::TradeRate {
            volume: false,
            seconds: 60,
            instant: 10,
        })));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);

        let capped = held == TRADE_CAPACITY;
        let mut candles = vec![candle(start, fills.len() as f64)];
        let mut tape = Vec::with_capacity(held);
        let in_first = if capped { 1 } else { held };
        tape.extend(
            fills[fills.len() - in_first..]
                .iter()
                .map(|&(secs, id)| fill(start + TimeDelta::seconds(secs), id)),
        );
        let later = TRADE_CAPACITY - 1;
        if capped {
            let next = start + TimeDelta::minutes(1);
            candles.push(candle(next, later as f64));
            tape.extend(
                (0..later).map(|i| fill(next + TimeDelta::microseconds(i as i64), i as u64 + 3)),
            );
        }
        feed.page().send(page(candles, true));
        let send = |tape, end| {
            let reply = feed.trades.lock().unwrap().pop().unwrap();
            reply.send(Ok(TradePage::new(tape, end)));
        };
        send(tape, complete);
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        let row = |chart: &Chart, i| {
            let study = chart.indicators().find(|(id, _)| *id == rate).unwrap().1;
            [0, 1, 2].map(|line| study.values().value(line, i))
        };
        let total = fills.len() as f64;
        let full = [Some(total), Some(total / 60.0), Some(0.1)];
        if complete {
            assert_eq!(row(&chart, 0), full);
        } else if capped {
            assert_eq!(row(&chart, 0), [None; 3]);
            assert_eq!(
                row(&chart, 1),
                [Some(later as f64), None, Some(later as f64 / 10.0)]
            );
            assert_eq!(chart.trades(SeriesId::MAIN).count(), TRADE_CAPACITY);
        } else {
            // The 10-second window is covered even if an opening fill is missing.
            assert_eq!(row(&chart, 0), [None, None, Some(0.1)]);
            if held == fills.len() {
                // An empty terminal page proves these were the whole history.
                send(Vec::new(), true);
                for _ in 0..3 {
                    frames.run(&mut chart);
                }
                assert_eq!(row(&chart, 0), full);
            }
        }
    }
}

/// Hand counts on a 1-minute chart with candles at 00:01 and 00:02: buys of one lot at
/// 00:01:05 (price 100), 00:01:15 (101) and 00:02:30 (102), each its own aggressor order. A
/// page that does not reach the start of history leaves the 00:01 candle blank, and every
/// window or sum that reaches it, until an empty terminal page proves it whole.
#[cfg(feature = "studies")]
#[test]
fn time_bar_trade_studies_require_coverage_for_each_output() {
    use crate::data::OrderFlowKind;
    use crate::model::OrderSide::Buy;
    use crate::studies::{Indicator, OrderFlowStudy};

    let start = "2025-01-01T00:01:00Z".parse::<DateTime<Utc>>().unwrap();
    let tape = || {
        [(5, 100.0), (15, 101.0), (90, 102.0)]
            .into_iter()
            .zip(1..)
            .map(|((secs, price), id)| {
                Tick::new(start + TimeDelta::seconds(secs), price, 1.0, Some(Buy))
                    .with_id(id)
                    .with_aggressor_order_id(id)
            })
            .collect::<Vec<_>>()
    };
    type Rows = [[Option<f64>; 3]; 2];
    let n = None;
    let s = Some;
    // Price Range over 90 seconds, read from its source: at 00:02:30 the window opens at 00:01:00.
    let range_kind = OrderFlowKind::PriceRange { seconds: 90 };
    let range_complete: Rows = [[s(1.0), s(101.0), s(100.0)], [s(2.0), s(102.0), s(100.0)]];
    let range_partial: Rows = [[n; 3], [n; 3]];
    for complete in [true, false] {
        // Each study, its two candles complete, and the same before a terminal page.
        let studies: [(OrderFlowStudy, Rows, Rows); 7] = [
            (
                OrderFlowStudy::new(OrderFlowKind::AggressorRate {
                    seconds: 60,
                    instant: 10,
                }),
                [
                    [s(2.0), s(2.0 / 60.0), s(0.1)],
                    [s(1.0), s(1.0 / 60.0), s(0.1)],
                ],
                // The 10-second window from 00:01:05 holds every execution after the page start.
                [[n, n, s(0.1)], [s(1.0), s(1.0 / 60.0), s(0.1)]],
            ),
            (
                OrderFlowStudy::new(OrderFlowKind::LevelsExecuted {
                    seconds: 0,
                    reversal: false,
                }),
                [[s(2.0), s(0.0), s(2.0)], [s(1.0), s(0.0), s(1.0)]],
                [[n, n, n], [s(1.0), s(0.0), s(1.0)]],
            ),
            (
                OrderFlowStudy::new(OrderFlowKind::SizeDelta {
                    ranges: [[0.0, 10.0], [10.0, 100.0], [100.0, f64::MAX]],
                }),
                [[s(2.0), s(0.0), s(0.0)], [s(1.0), s(0.0), s(0.0)]],
                [[n, n, n], [s(1.0), s(0.0), s(0.0)]],
            ),
            (
                OrderFlowStudy::new(OrderFlowKind::FlowSum { bars: 2 }),
                [[n, n, n], [s(3.0), s(3.0), n]],
                [[n, n, n], [n, n, n]],
            ),
            (
                OrderFlowStudy::new(OrderFlowKind::SideVolume),
                [[s(2.0), s(0.0), s(2.0)], [s(1.0), s(0.0), s(1.0)]],
                [[n, n, n], [s(1.0), s(0.0), s(1.0)]],
            ),
            (
                OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
                    min: 0.0,
                    max: f64::MAX,
                }),
                [[s(2.0), n, n], [s(3.0), n, n]],
                // The sum starts at the oldest candle with a value.
                [[n, n, n], [s(1.0), n, n]],
            ),
            (
                OrderFlowStudy::execution_delta(false),
                [[s(2.0), s(2.0), n], [s(1.0), s(1.0), n]],
                [[n, n, n], [s(1.0), s(1.0), n]],
            ),
        ];
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(Timeframe::Min1);
        let ids: Vec<_> = (studies.iter())
            .map(|(study, ..)| chart.add_indicator(Box::new(study.clone())))
            .collect();
        let range = chart.add_indicator(Box::new(OrderFlowStudy::new(range_kind)));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        let candle = |open, volume| Bar::new(open, 100.0, 100.0, 100.0, 100.0, volume);
        feed.page().send(page(
            vec![
                candle(start, 2.0),
                candle(start + TimeDelta::minutes(1), 1.0),
            ],
            true,
        ));
        let send = |tape, end| {
            let reply = feed.trades.lock().unwrap().pop().unwrap();
            reply.send(Ok(TradePage::new(tape, end)));
        };
        send(tape(), complete);
        // A replaced series gets one full recompute per frame.
        for _ in 0..10 {
            frames.run(&mut chart);
        }
        let check = |chart: &Chart, done: bool, when: &str| {
            for ((study, whole, partial), id) in studies.iter().zip(&ids) {
                let shown = chart.indicators().find(|(i, _)| i == id).unwrap().1;
                let rows = [0, 1].map(|i| [0, 1, 2].map(|line| shown.values().value(line, i)));
                let want = if done { whole } else { partial };
                assert_eq!(&rows, want, "{} {when}", study.name());
            }
            let shown = chart.indicators().find(|(i, _)| *i == range).unwrap().1;
            let values = shown.order_flow_inputs()[0].values();
            let rows = [0, 1].map(|i| {
                values[&(start + TimeDelta::minutes(i))].map(|v| v.is_finite().then_some(v))
            });
            let want = if done { range_complete } else { range_partial };
            assert_eq!(rows, want, "Price Range {when}");
        };
        check(&chart, complete, "after the first page");
        if !complete {
            send(Vec::new(), true);
            for _ in 0..10 {
                frames.run(&mut chart);
            }
            check(&chart, true, "after the terminal page");
        }
    }
}

fn missing_history_page(
    shared: bool,
    from: DateTime<Utc>,
    before: DateTime<Utc>,
    volume: f64,
    end: bool,
) -> (FootprintPage, *const crate::data::FootprintLevel) {
    use crate::data::FootprintBars;
    let mut page = prepared_page(&[Tick::new(from, 100.0, volume, None)], before, end);
    let FootprintBars::Owned(bars) = page.bars else {
        unreachable!()
    };
    let row = bars[0].levels.get(&100).unwrap() as *const crate::data::FootprintLevel;
    page.bars = if shared {
        FootprintBars::Shared(Arc::new(bars))
    } else {
        FootprintBars::Owned(bars)
    };
    (page, row)
}

/// A provider can traverse an outage without claiming it was quiet. Both surrounding
/// owners arrive through normal replies; no test reaches into the pending request state.
fn missing_history_chart(
    shared: bool,
) -> (
    Arc<Script>,
    Chart,
    Frames,
    [*const crate::data::FootprintLevel; 2],
) {
    let (feed, mut chart, mut frames) = prepared_chart();
    #[cfg(feature = "studies")]
    {
        chart.add_indicator(
            crate::studies::IndicatorFactory::new()
                .create("Vol Profile FR")
                .unwrap(),
        );
        frames.run(&mut chart);
    }
    let start = bars(1)[0].time;
    let at = |minute| start + TimeDelta::minutes(minute);
    assert_eq!(footprint_requests(&feed).len(), 1);
    let (newer, newer_row) = missing_history_page(shared, at(3), at(4), 7.0, false);
    feed.footprints.lock().unwrap().remove(0).send(Ok(newer));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let requests = footprint_requests(&feed);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].before, Some(at(3)));
    let unavailable = FootprintPage::unavailable(at(1), at(3));
    assert!(!unavailable.is_available());
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(unavailable));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let requests = footprint_requests(&feed);
    assert_eq!(requests.len(), 1, "an outage must not block older history");
    assert_eq!(requests[0].before, Some(at(1)));
    let (older, older_row) = missing_history_page(shared, start, at(1), 3.0, true);
    feed.footprints.lock().unwrap().remove(0).send(Ok(older));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert!(footprint_requests(&feed).is_empty());
    (feed, chart, frames, [older_row, newer_row])
}

#[cfg(feature = "studies")]
fn missing_history_input(chart: &Chart) -> crate::data::OrderFlowInput {
    let mut inputs = chart
        .indicators()
        .flat_map(|(_, study)| study.order_flow_inputs())
        .filter(|input| input.is_footprint());
    let input = inputs
        .next()
        .expect("the fixed-range profile has a footprint input")
        .clone();
    assert!(inputs.next().is_none());
    input
}

fn assert_missing_history_rows(
    chart: &Chart,
    owners: [*const crate::data::FootprintLevel; 2],
    complete: bool,
) {
    let start = bars(1)[0].time;
    let last = start + TimeDelta::minutes(3);
    let view = prepared_view(chart);
    assert_eq!(
        view.complete_bars(..)
            .map(|bar| (bar.time, bar.volume()))
            .collect::<Vec<_>>(),
        [(start, 3.0), (last, 7.0)]
    );
    for (time, owner) in [start, last].into_iter().zip(owners) {
        assert_eq!(
            view.bar_at(time).unwrap().levels.get(&100).unwrap()
                as *const crate::data::FootprintLevel,
            owner,
            "admitting coverage must not replace completed price rows"
        );
    }
    assert_eq!(view.span_is_complete(start, last), complete);
    #[cfg(feature = "studies")]
    {
        let read = missing_history_input(chart).footprint_snapshot(start);
        assert_eq!(read.span_is_complete(start, last), complete);
        assert_eq!(
            read.bars(..)
                .map(|bar| (bar.time, bar.volume()))
                .collect::<Vec<_>>(),
            [(start, 3.0), (last, 7.0)]
        );
        for (bar, owner) in read.bars(..).zip(owners) {
            assert_eq!(
                bar.levels.get(&100).unwrap() as *const crate::data::FootprintLevel,
                owner,
                "the chart and study must share admitted owners"
            );
        }
    }
}

fn resume_missing_history(feed: &Script, chart: &mut Chart, frames: &mut Frames) {
    chart.set_feed_paused(true);
    frames.run(chart);
    chart.set_feed_paused(false);
    frames.run(chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(5), true));
    frames.run(chart);
    frames.run(chart);
    let start = bars(1)[0].time;
    let requests = footprint_requests(feed);
    assert_eq!(
        requests.len(),
        1,
        "chart and study coalesce their gap demand"
    );
    assert_eq!(requests[0].since, Some(start + TimeDelta::minutes(1)));
    assert_eq!(requests[0].before, Some(start + TimeDelta::minutes(3)));
}

#[test]
fn unavailable_prepared_history_skips_only_its_span_and_resume_retries_it() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
        assert_missing_history_rows(&chart, owners, false);
        for _ in 0..6 {
            frames.run(&mut chart);
        }
        assert!(footprint_requests(&feed).is_empty());
        assert!(feed.footprints.lock().unwrap().is_empty());
        resume_missing_history(&feed, &mut chart, &mut frames);
        assert_missing_history_rows(&chart, owners, false);
        for _ in 0..6 {
            frames.run(&mut chart);
        }
        assert!(
            footprint_requests(&feed).is_empty(),
            "a pending gap must not issue duplicate requests"
        );
        assert_eq!(feed.footprints.lock().unwrap().len(), 1);
        assert_missing_history_rows(&chart, owners, false);
    }

    // A clipped viewport can answer only the older prefix of an internal gap.
    // Panning right must still request the newer, unanswered suffix.
    for shared in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let start = bars(1)[0].time;
        let at = |minute| start + TimeDelta::minutes(minute);
        footprint_requests(&feed);
        let (newer, _) = missing_history_page(shared, at(3), at(4), 7.0, false);
        feed.footprints.lock().unwrap().remove(0).send(Ok(newer));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].before, Some(at(3)));
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(FootprintPage::unavailable(at(1), at(3))));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].before, Some(at(1)));
        let (older, _) = missing_history_page(shared, start, at(1), 3.0, true);
        feed.footprints.lock().unwrap().remove(0).send(Ok(older));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert!(footprint_requests(&feed).is_empty());

        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 1.0,
            right: 1.0,
        });
        chart.set_feed_paused(true);
        frames.run(&mut chart);
        chart.set_feed_paused(false);
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].since, Some(at(1)));
        assert_eq!(requests[0].before, Some(at(2)));
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(FootprintPage::unavailable(at(1), at(2))));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert!(footprint_requests(&feed).is_empty());
        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 1.0,
            right: 2.0,
        });
        frames.run(&mut chart);
        frames.run(&mut chart);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].since, Some(at(2)));
        assert_eq!(requests[0].before, Some(at(3)));
    }
}

#[test]
fn explicit_reload_replaces_prepared_history_and_requests_the_latest_page() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let start = bars(1)[0].time;
        let (old, _) = missing_history_page(
            shared,
            start + TimeDelta::minutes(3),
            start + TimeDelta::minutes(4),
            7.0,
            true,
        );
        feed.footprints.lock().unwrap().remove(0).send(Ok(old));
        feed.sink()
            .push_tick(Tick::new(start + TimeDelta::minutes(4), 100.0, 9.0, None));
        frames.run(&mut chart);
        frames.run(&mut chart);
        footprint_requests(&feed);

        chart.reload_feed();
        frames.run(&mut chart);
        assert!(chart.bars(SeriesId::MAIN).is_empty());
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        let replacement = bars(2)
            .into_iter()
            .map(|bar| Bar::new(bar.time, 50.0, 50.0, 50.0, 50.0, 2.0))
            .collect::<Vec<_>>();
        feed.page().send(page(replacement.clone(), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(chart.bars(SeriesId::MAIN), replacement);
        assert!(prepared_view(&chart).complete_bars(..).next().is_none());
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].since, None);
        assert_eq!(requests[0].before, None);

        let (fresh, _) =
            missing_history_page(shared, start, start + TimeDelta::minutes(1), 11.0, true);
        feed.footprints.lock().unwrap().remove(0).send(Ok(fresh));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(
            prepared_view(&chart)
                .complete_bars(..)
                .map(|bar| (bar.time, bar.volume()))
                .collect::<Vec<_>>(),
            [(start, 11.0)]
        );
    }
}

/// Losing an undrained bar replaces the subscription and requests the latest page in the same
/// frame, without retaining the incomplete series or reporting a validation failure.
#[test]
fn a_pending_bar_overflow_reloads_the_history() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    feed.page().send(page(bars(3), true));
    frames.run(&mut chart);
    let old = feed.sink();
    feed.calls();
    let start = bars(1)[0].time;
    // 100,001 separate minute slots exceed the 100,000-slot pending limit by one.
    for minute in 0..=100_000 {
        let time = start + TimeDelta::minutes(minute);
        old.push_bar(Bar::new(time, 200.0, 200.0, 200.0, 200.0, 1.0));
    }
    frames.run(&mut chart);
    assert_eq!(
        feed.calls(),
        [
            Call::Unsubscribe(old.id),
            Call::Resolve("A".into()),
            history("A", Timeframe::Min1, None),
        ]
    );
    assert!(!old.is_active());
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    let state = chart.feed_state(SeriesId::MAIN).unwrap();
    assert_eq!(state.last_error, None);
    assert_eq!(state.rejected_bars, 0);
    assert!(state.loading_history);
    feed.resolve().send(Ok(Instrument::new("A")));
    feed.page().send(page(bars(3), true));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN), bars(3));
    assert!(feed.sink().is_active());
}

#[test]
fn confirmed_empty_internal_history_invalidates_coverage_without_fabricating_rows() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
        resume_missing_history(&feed, &mut chart, &mut frames);
        let start = bars(1)[0].time;
        let before = start + TimeDelta::minutes(3);
        let candles = chart.bars(SeriesId::MAIN).to_vec();
        assert_missing_history_rows(&chart, owners, false);
        let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
        // Owned-only exports clone mutable storage; shared exports retain their source.
        let frozen_owners = [start, before].map(|time| {
            prepared_view(&frozen)
                .bar_at(time)
                .unwrap()
                .levels
                .get(&100)
                .unwrap() as *const crate::data::FootprintLevel
        });
        if shared {
            assert_eq!(frozen_owners, owners);
        }
        #[cfg(feature = "studies")]
        let input_and_revision = {
            let input = missing_history_input(&chart);
            // Populate the consumer's cached snapshot before the coverage-only answer.
            assert!(
                !input
                    .footprint_snapshot(start)
                    .span_is_complete(start, before)
            );
            let revision = input.revision();
            (input, revision)
        };
        let empty = FootprintPage::new(Vec::new(), before, false)
            .with_continue_before(start + TimeDelta::minutes(1));
        assert!(empty.is_available());
        feed.footprints.lock().unwrap().remove(0).send(Ok(empty));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_missing_history_rows(&chart, owners, true);
        assert_eq!(chart.bars(SeriesId::MAIN), candles);
        assert!(
            prepared_view(&chart)
                .bar_at(start + TimeDelta::minutes(1))
                .is_none()
        );
        assert!(
            prepared_view(&chart)
                .bar_at(start + TimeDelta::minutes(2))
                .is_none()
        );
        #[cfg(feature = "studies")]
        {
            let (input, revision) = input_and_revision;
            assert_ne!(input.revision(), revision);
            assert!(
                input
                    .footprint_snapshot(start)
                    .span_is_complete(start, before),
                "a cached incomplete profile must observe a coverage-only answer"
            );
        }
        assert!(footprint_requests(&feed).is_empty());
        frames.run(&mut frozen);
        assert!(feed.calls().is_empty(), "exports must never refill a gap");
        assert_missing_history_rows(&frozen, frozen_owners, false);
        drop(chart);
        assert_missing_history_rows(&frozen, frozen_owners, false);
    }
}

#[test]
fn partial_internal_history_answers_advance_the_remaining_bounded_request() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
        resume_missing_history(&feed, &mut chart, &mut frames);
        let start = bars(1)[0].time;
        let at = |minute| start + TimeDelta::minutes(minute);
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(
                FootprintPage::new(Vec::new(), at(3), false).with_continue_before(at(2))
            ));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_missing_history_rows(&chart, owners, false);
        assert!(prepared_view(&chart).span_is_complete(at(2), at(3)));
        assert!(!prepared_view(&chart).span_is_complete(at(1), at(2)));
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].since, Some(at(1)));
        assert_eq!(requests[0].before, Some(at(2)));
        let (answer, row) = missing_history_page(shared, at(1), at(2), 11.0, false);
        feed.footprints.lock().unwrap().remove(0).send(Ok(answer));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let view = prepared_view(&chart);
        assert!(view.span_is_complete(start, at(3)));
        assert_eq!(
            view.complete_bars(..)
                .map(|bar| bar.volume())
                .collect::<Vec<_>>(),
            [3.0, 11.0, 7.0]
        );
        for (time, owner) in [start, at(1), at(3)]
            .into_iter()
            .zip([owners[0], row, owners[1]])
        {
            assert_eq!(
                view.bar_at(time).unwrap().levels.get(&100).unwrap()
                    as *const crate::data::FootprintLevel,
                owner
            );
        }
        #[cfg(feature = "studies")]
        {
            let read = missing_history_input(&chart).footprint_snapshot(start);
            assert!(read.span_is_complete(start, at(3)));
            assert_eq!(
                read.bars(..).map(|bar| bar.volume()).collect::<Vec<_>>(),
                [3.0, 11.0, 7.0]
            );
        }
        assert!(footprint_requests(&feed).is_empty());
    }
}

#[test]
fn invalid_internal_history_preserves_complete_rows_and_the_unknown_gap() {
    for shared in [false, true] {
        for invalid in 0..5 {
            let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
            resume_missing_history(&feed, &mut chart, &mut frames);
            let start = bars(1)[0].time;
            let at = |minute| start + TimeDelta::minutes(minute);
            let mut answer = missing_history_page(shared, at(1), at(3), 11.0, false).0;
            match invalid {
                // An overlapping old owner is not a correction/replacement.
                0 => answer = missing_history_page(shared, start, at(3), 999.0, false).0,
                1 => answer.complete_before = at(3) + TimeDelta::seconds(1),
                2 => {
                    answer =
                        FootprintPage::new(Vec::new(), at(3), false).with_continue_before(at(3));
                }
                3 => {
                    let rows = answer.bars;
                    answer = FootprintPage::unavailable(at(1), at(3));
                    answer.bars = rows;
                }
                _ => {
                    answer = FootprintPage::new(Vec::new(), at(3), false)
                        .with_continue_before(at(1) + TimeDelta::seconds(1));
                }
            }
            feed.footprints.lock().unwrap().remove(0).send(Ok(answer));
            frames.run(&mut chart);
            assert!(
                matches!(chart.feed.last_error, Some(FeedError::InvalidData(_))),
                "shared={shared}, invalid={invalid}"
            );
            assert_missing_history_rows(&chart, owners, false);
            for _ in 0..3 {
                frames.run(&mut chart);
            }
            assert!(footprint_requests(&feed).is_empty());
            assert_missing_history_rows(&chart, owners, false);
        }
    }
}

#[test]
fn internal_history_replies_cannot_cross_context_changes_or_same_drain_replay_reset() {
    for shared in [false, true] {
        for change in 0..7 {
            let (feed, mut chart, mut frames, _) = missing_history_chart(shared);
            resume_missing_history(&feed, &mut chart, &mut frames);
            let start = bars(1)[0].time;
            let answer = missing_history_page(
                shared,
                start + TimeDelta::minutes(1),
                start + TimeDelta::minutes(3),
                999.0,
                false,
            )
            .0;
            let stale = feed.footprints.lock().unwrap().remove(0);
            match change {
                0 => chart.set_symbol("B"),
                1 => chart.set_timeframe(Timeframe::Min5),
                2 => chart.set_extended_hours(true),
                3 => chart.set_dividend_adjusted(true),
                4 => chart.set_conversion(Some("EUR".into())),
                // Both inbox orders matter: reply admission must see a reset in the drain.
                5 => {
                    stale.send(Ok(answer));
                    feed.sink().reset_replay(start);
                    frames.run(&mut chart);
                    assert!(
                        chart
                            .feed_link
                            .as_ref()
                            .unwrap()
                            .footprint
                            .as_ref()
                            .is_none_or(|flow| flow
                                .view()
                                .bars(..)
                                .all(|bar| bar.volume() != 999.0))
                    );
                    assert!(chart.feed.last_error.is_none());
                    continue;
                }
                _ => feed.sink().reset_replay(start),
            }
            if change < 5 {
                frames.run(&mut chart);
            }
            stale.send(Ok(answer));
            frames.run(&mut chart);
            assert!(
                chart
                    .feed_link
                    .as_ref()
                    .unwrap()
                    .footprint
                    .as_ref()
                    .is_none_or(|flow| flow.view().bars(..).all(|bar| bar.volume() != 999.0)),
                "shared={shared}, change={change}"
            );
            #[cfg(feature = "studies")]
            {
                assert!(
                    missing_history_input(&chart)
                        .footprint_snapshot(start)
                        .bars(..)
                        .all(|bar| bar.volume() != 999.0)
                );
            }
            assert!(chart.feed.last_error.is_none());
        }
    }
}

#[cfg(feature = "studies")]
#[test]
fn later_gap_page_keeps_the_studys_live_suffix_after_raw_eviction() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
        resume_missing_history(&feed, &mut chart, &mut frames);
        let start = bars(1)[0].time;
        let at = |minute| start + TimeDelta::minutes(minute);
        feed.trades
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .send(Ok(TradePage::new(Vec::new(), true)));
        frames.run(&mut chart);
        let input = missing_history_input(&chart);
        let reset = input.reset_revision();
        chart.feed_link.as_mut().unwrap().trades = Trades::new(3);
        for minute in 4..=10 {
            feed.sink()
                .push_tick(Tick::new(at(minute), 100.0, 5.0, None).with_id(minute as u64));
            frames.run(&mut chart);
        }
        assert!(
            chart.trades(SeriesId::MAIN).count() < 7,
            "the fixture must actually evict raw executions"
        );
        let volumes = |input: &crate::data::OrderFlowInput| {
            input
                .footprint_snapshot(start)
                .bars(..)
                .map(|bar| (bar.time, bar.volume()))
                .collect::<Vec<_>>()
        };
        let mut expected = vec![(start, 3.0), (at(3), 7.0)];
        expected.extend((4..=10).map(|minute| (at(minute), 5.0)));
        assert_eq!(volumes(&input), expected);
        assert!(
            !input
                .footprint_snapshot(start)
                .span_is_complete(start, at(10))
        );
        assert_eq!(input.reset_revision(), reset);
        let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
        let frozen_input = missing_history_input(&frozen);
        let (answer, row) = missing_history_page(shared, at(1), at(3), 11.0, false);
        feed.footprints.lock().unwrap().remove(0).send(Ok(answer));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(
            input.reset_revision(),
            reset,
            "a gap fill is not an input reset"
        );
        assert_eq!(volumes(&frozen_input), expected);
        expected.insert(1, (at(1), 11.0));
        assert_eq!(
            volumes(&input),
            expected,
            "a new prepared page must not rebuild a study from the truncated raw tape"
        );
        let read = input.footprint_snapshot(start);
        assert!(read.span_is_complete(start, at(10)));
        for (time, owner) in [start, at(1), at(3)]
            .into_iter()
            .zip([owners[0], row, owners[1]])
        {
            assert_eq!(
                read.bars(time..=time)
                    .next()
                    .unwrap()
                    .levels
                    .get(&100)
                    .unwrap() as *const crate::data::FootprintLevel,
                owner
            );
        }
        feed.calls();
        frames.run(&mut frozen);
        assert!(feed.calls().is_empty());
        assert!(
            !frozen_input
                .footprint_snapshot(start)
                .span_is_complete(start, at(10))
        );
    }
}

#[test]
fn frozen_export_shares_prepared_rows_and_keeps_its_live_boundary() {
    use crate::data::{FootprintBars, FootprintLevel};
    let (feed, mut chart, mut frames) = prepared_chart();
    chart.config_mut().show_countdown = false;
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::seconds(90);
    let saved = [
        Tick::new(start, 100.0, 3.0, None),
        Tick::new(start + TimeDelta::minutes(1), 100.0, 5.0, None),
    ];
    let FootprintBars::Owned(saved) = prepared_page(&saved, cutoff, true).bars else {
        unreachable!()
    };
    let owner = Arc::new(saved);
    let weak = Arc::downgrade(&owner);
    let row = owner[0].levels.get(&100).unwrap() as *const FootprintLevel;
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(FootprintPage::from_shared(owner, cutoff, true)));
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(
            vec![Tick::new(cutoff, 100.0, 7.0, None).with_id(1)],
            false,
        )));
    frames.run(&mut chart);
    let values = |chart: &Chart| {
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        (
            flow.bar_at(start).unwrap().levels.get(&100).unwrap() as *const FootprintLevel,
            flow.complete_bar_at(cutoff).unwrap().unknown,
        )
    };
    assert_eq!(values(&chart), (row, 12.0));
    let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
    assert_eq!(values(&frozen), (row, 12.0));
    feed.sink()
        .push_tick(Tick::new(cutoff + TimeDelta::seconds(1), 100.0, 11.0, None).with_id(2));
    frames.run(&mut chart);
    assert_eq!(values(&chart), (row, 23.0));
    feed.calls();
    frames.run(&mut frozen);
    assert!(
        feed.calls().is_empty(),
        "an export must not request live data"
    );
    assert_eq!(values(&frozen), (row, 12.0));
    drop(chart);
    assert!(weak.upgrade().is_some());
    frames.run(&mut frozen);
    assert_eq!(values(&frozen), (row, 12.0));
    drop(frozen);
    assert!(weak.upgrade().is_none());
}

/// The exclusive cutoff is independent of reply order and unordered execution ids.
#[test]
fn prepared_history_joins_live_once_and_keeps_unknown_sides() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::seconds(150);
    let saved = vec![
        Tick::new(start, 100.0, 2.0, Some(Buy)),
        Tick::new(start + TimeDelta::minutes(1), 101.0, 3.0, Some(Sell)),
        Tick::new(start + TimeDelta::minutes(2), 102.0, 5.0, None),
    ];
    for (page_first, delay, shared) in [(false, 0), (true, 0), (false, 2), (true, 2)]
        .into_iter()
        .flat_map(|(first, delay)| [false, true].map(move |shared| (first, delay, shared)))
    {
        let first = cutoff + TimeDelta::seconds(delay);
        let live = vec![
            Tick::new(first, 102.0, 7.0, Some(Buy)).with_id(99),
            Tick::new(first, 102.0, 11.0, None).with_id(7),
            Tick::new(first + TimeDelta::seconds(1), 102.0, 13.0, Some(Sell)).with_id(42),
        ];
        let (feed, mut chart, mut frames) = prepared_chart();
        let mut reply = Some(feed.footprints.lock().unwrap().remove(0));
        let mut deliver = || {
            let mut page = prepared_page(&saved, cutoff, true);
            if shared {
                let crate::data::FootprintBars::Owned(bars) = page.bars else {
                    unreachable!()
                };
                page.bars = crate::data::FootprintBars::Shared(Arc::new(bars));
            }
            reply.take().unwrap().send(Ok(page))
        };
        if page_first {
            deliver();
            frames.run(&mut chart);
        }
        let sink = feed.sink();
        sink.push_tick(Tick::new(
            cutoff - TimeDelta::seconds(1),
            102.0,
            999.0,
            None,
        ));
        for trade in &live {
            sink.push_tick(trade.clone());
        }
        frames.run(&mut chart);
        if !page_first {
            deliver();
            frames.run(&mut chart);
        }
        // A raw page still serves the tape, but its covered trades cannot add to saved rows.
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(
                saved.iter().chain(&live).cloned().collect(),
                true,
            )));
        frames.run(&mut chart);
        sink.push_tick(live[2].clone());
        frames.run(&mut chart);
        let expected = prepared_page(
            &saved.iter().chain(&live).cloned().collect::<Vec<_>>(),
            cutoff,
            true,
        );
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        assert_eq!(
            flow.bars(..).collect::<Vec<_>>(),
            expected.bars.iter().collect::<Vec<_>>(),
            "page first: {page_first}, shared: {shared}"
        );
        assert_eq!(flow.complete_from(), Some(start), "{page_first}, {delay}");
        assert_eq!(flow.bar_at(cutoff).unwrap().unknown, 16.0);
    }
}

#[test]
fn prepared_rows_survive_eviction_and_a_late_trade_page_without_cloning() {
    let (feed, mut chart, mut frames) = prepared_chart();
    #[cfg(feature = "studies")]
    let input = {
        let id = chart.add_indicator(
            crate::studies::IndicatorFactory::new()
                .create("Vol Profile FR")
                .unwrap(),
        );
        frames.run(&mut chart);
        chart
            .indicators()
            .find(|(seen, _)| *seen == id)
            .unwrap()
            .1
            .order_flow_inputs()[0]
            .clone()
    };
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::minutes(2);
    let saved = [Tick::new(start, 100.0, 5.0, None)];
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(&saved, cutoff, true)));
    frames.run(&mut chart);
    chart.feed_link.as_mut().unwrap().trades = Trades::new(2);
    let row_address = |chart: &Chart| {
        let row = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view()
            .bar_at(start)
            .unwrap()
            .levels
            .values()
            .next()
            .unwrap();
        std::ptr::from_ref(row) as usize
    };
    let original = row_address(&chart);
    for minute in 2..6 {
        feed.sink().push_tick(Tick::new(
            start + TimeDelta::minutes(minute),
            102.0,
            1.0,
            None,
        ));
        frames.run(&mut chart);
        assert_eq!(row_address(&chart), original);
    }
    // The profile kept the executions the store evicted, so the bars after the cutoff's are
    // complete before the late page.
    #[cfg(feature = "studies")]
    let reset = {
        let read = input.footprint_snapshot(start);
        assert!(
            read.span_is_complete(start + TimeDelta::minutes(3), start + TimeDelta::minutes(5))
        );
        input.reset_revision()
    };
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(saved.to_vec(), true)));
    frames.run(&mut chart);
    assert_eq!(row_address(&chart), original);
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .view();
    assert_eq!(flow.bar_at(start).unwrap().unknown, 5.0);
    #[cfg(feature = "studies")]
    {
        assert_eq!(
            input.reset_revision(),
            reset,
            "a late trade page must not reset the profile"
        );
        let read = input.footprint_snapshot(start);
        assert!(
            read.span_is_complete(start + TimeDelta::minutes(3), start + TimeDelta::minutes(5))
        );
        // The cutoff's bar keeps its unproven first instant: the store dropped that execution.
        assert_eq!(
            read.bars(..)
                .map(|bar| (bar.time, bar.volume()))
                .collect::<Vec<_>>(),
            [
                (start, 5.0),
                (start + TimeDelta::minutes(3), 1.0),
                (start + TimeDelta::minutes(4), 1.0),
                (start + TimeDelta::minutes(5), 1.0)
            ]
        );
        assert_eq!(
            read.bars(..)
                .next()
                .unwrap()
                .levels
                .values()
                .next()
                .unwrap() as *const _ as usize,
            original
        );
    }
    assert_eq!(
        flow.complete_bars(..)
            .map(|bar| bar.time)
            .collect::<Vec<_>>(),
        vec![
            start,
            start + TimeDelta::minutes(4),
            start + TimeDelta::minutes(5)
        ]
    );
}

#[test]
fn prepared_join_keeps_both_complete_sides_until_a_trade_page_fills_it() {
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::seconds(90);
    // An execution at the cutoff fills the join; one before it proves the join empty without
    // changing a bar, so the profile is not rebuilt.
    for (paged, cutoff_volume) in [
        (Tick::new(cutoff, 100.0, 2.0, None).with_id(99), 9.0),
        (
            Tick::new(start + TimeDelta::seconds(30), 100.0, 3.0, None),
            7.0,
        ),
    ] {
        let (feed, mut chart, mut frames) = prepared_chart();
        #[cfg(feature = "studies")]
        let input = {
            let id = chart.add_indicator(
                crate::studies::IndicatorFactory::new()
                    .create("Vol Profile FR")
                    .unwrap(),
            );
            frames.run(&mut chart);
            chart
                .indicators()
                .find(|(seen, _)| *seen == id)
                .unwrap()
                .1
                .order_flow_inputs()[0]
                .clone()
        };
        let saved = [
            Tick::new(start, 100.0, 5.0, None),
            Tick::new(start + TimeDelta::minutes(1), 100.0, 7.0, None),
        ];
        for (second, size, id) in [(130, 11.0, 7), (180, 13.0, 42)] {
            feed.sink().push_tick(
                Tick::new(start + TimeDelta::seconds(second), 100.0, size, None).with_id(id),
            );
        }
        frames.run(&mut chart);
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(prepared_page(&saved, cutoff, true)));
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        assert_eq!(flow.complete_from(), Some(start + TimeDelta::minutes(3)));
        assert_eq!(
            flow.complete_bars(..)
                .map(|bar| bar.time)
                .collect::<Vec<_>>(),
            vec![start, start + TimeDelta::minutes(3)]
        );
        assert!(flow.complete_bar_at(cutoff).is_none());
        #[cfg(feature = "studies")]
        {
            let read = input.footprint_snapshot(start);
            assert_eq!(
                read.bars(..)
                    .map(|bar| (bar.time, bar.volume()))
                    .collect::<Vec<_>>(),
                [(start, 5.0), (start + TimeDelta::minutes(3), 13.0)]
            );
            assert!(!read.span_is_complete(start, start + TimeDelta::minutes(3)));
        }
        #[cfg(feature = "studies")]
        let (reset, fills) = (input.reset_revision(), paged.ts >= cutoff);
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(vec![paged], false)));
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        assert!(!flow.span_has_gap(start, None));
        assert_eq!(flow.complete_from(), Some(start));
        assert_eq!(flow.bar_at(cutoff).unwrap().unknown, cutoff_volume);
        #[cfg(feature = "studies")]
        {
            let read = input.footprint_snapshot(start);
            assert_eq!(
                read.bars(..).map(|bar| bar.volume()).collect::<Vec<_>>(),
                [5.0, cutoff_volume, 11.0, 13.0]
            );
            assert!(read.span_is_complete(start, start + TimeDelta::minutes(3)));
            if !fills {
                assert_eq!(
                    input.reset_revision(),
                    reset,
                    "a page before the cutoff must not rebuild the profile"
                );
            }
        }
    }
}

#[test]
fn prepared_pages_reject_invalid_ranges_and_nonprogressing_continuations() {
    let start = bars(1)[0].time;
    for case in 0..7 {
        let (feed, mut chart, mut frames) = prepared_chart();
        let request = feed
            .calls()
            .into_iter()
            .find_map(|call| match call {
                Call::Footprints(request) => Some(request),
                _ => None,
            })
            .unwrap();
        let mut page = prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            start + TimeDelta::minutes(1),
            true,
        );
        let mut asked = request;
        match case {
            0 => {
                let crate::data::FootprintBars::Owned(bars) = &mut page.bars else {
                    unreachable!()
                };
                bars[0].unknown = -1.0;
            }
            1 => asked.before = Some(start),
            2 => asked.since = Some(start + TimeDelta::seconds(1)),
            3 => page.complete_before = start,
            4 => asked.count = 0,
            _ => {
                page.bars = crate::data::FootprintBars::Owned(Vec::new());
                asked.before = Some(page.complete_before);
                page.continue_before = asked.before;
                if case == 6 {
                    page.continue_before = Some(start);
                    page.complete_before -= TimeDelta::seconds(1);
                }
            }
        }
        // Keep the normal reply key and replace only its requested limits for this validation row.
        let link = chart.feed_link.as_mut().unwrap();
        for (_, request, _) in &mut link.pending {
            if let Request::Footprints(request) = request {
                *request = asked.clone();
            }
        }
        feed.footprints.lock().unwrap().remove(0).send(Ok(page));
        frames.run(&mut chart);
        assert!(
            matches!(chart.feed.last_error, Some(FeedError::InvalidData(_))),
            "case {case}"
        );
        assert!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .is_none_or(|flow| flow.live.prepared_before().is_none())
        );
        assert_eq!(chart.bars(SeriesId::MAIN).len(), 5);
    }
}

/// On a daily chart with an overnight session, a page cut in the evening session holds the bar
/// of the next trading date, which opens after the cutoff, and may hold no older bar.
#[cfg(feature = "timezones")]
#[test]
fn prepared_page_cut_in_the_evening_session_may_hold_only_the_next_date() {
    use crate::model::{Session, SessionWindow, TimezoneId};
    use chrono::{NaiveTime, Weekday::*};
    let chicago = TimezoneId::named("America/Chicago").unwrap();
    let hour = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    // A date trades from 17:00 the evening before to 16:00 in Chicago.
    let futures = Instrument::new("ES")
        .with_tick_size(1.0)
        .with_time_zone(chicago.clone())
        .with_session(Session::new([SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            hour(17),
            hour(16),
        )]));
    let feed = Arc::new(Script::default());
    feed.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = fed(&feed, Some("ES"));
    chart.chart_options.time_scale.timezone = crate::scales::TimezoneMode::Exchange(chicago);
    chart.set_timeframe(Timeframe::Day1);
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    // The session the feed reports dates the bars, so the chart loads again once it resolves.
    for _ in 0..2 {
        feed.resolve().send(Ok(futures.clone()));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
    }
    // 17:30 on Monday 8 January 2024 trades Tuesday, whose bar opens at midnight.
    let cutoff = "2024-01-08T23:30:00Z".parse::<DateTime<Utc>>().unwrap();
    let clock = chart.feed_link.as_ref().unwrap().clock.clone();
    let tuesday = clock.instant_open(cutoff);
    assert!(tuesday > cutoff);
    let mut flow = Footprint::new(clock, 1.0, 1).unwrap();
    let evening = Tick::new(cutoff - TimeDelta::minutes(20), 100.0, 2.0, None);
    flow.push(&evening).unwrap();
    let answer = FootprintPage::new(flow.bars().values().cloned().collect(), cutoff, false);
    // The newest request: the first load asked with the clock it had before the session.
    let reply = feed.footprints.lock().unwrap().pop().unwrap();
    reply.send(Ok(answer));
    frames.run(&mut chart);
    assert!(
        chart.feed.last_error.is_none(),
        "{:?}",
        chart.feed.last_error
    );
    let held = prepared_view(&chart)
        .bars(..)
        .map(|bar| (bar.time, bar.volume()));
    assert_eq!(held.collect::<Vec<_>>(), [(tuesday, 2.0)]);
}

#[test]
fn prepared_replies_follow_context_and_do_not_load_trade_intervals() {
    let start = bars(1)[0].time;
    for change in 0..5 {
        let (feed, mut chart, mut frames) = prepared_chart();
        let stale = feed.footprints.lock().unwrap().remove(0);
        match change {
            0 => chart.set_symbol("B"),
            1 => chart.set_timeframe(Timeframe::Min5),
            2 => chart.set_extended_hours(true),
            3 => chart.set_dividend_adjusted(true),
            _ => chart.set_conversion(Some("EUR".into())),
        }
        frames.run(&mut chart);
        stale.send(Ok(prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            start + TimeDelta::minutes(1),
            true,
        )));
        frames.run(&mut chart);
        assert!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .is_none_or(|flow| flow.live.prepared_before().is_none())
        );
    }
    for interval in ["2t", "5v", "2r"] {
        let (feed, mut chart, mut frames) = prepared_chart();
        chart.set_timeframe(interval.parse().unwrap());
        frames.run(&mut chart);
        feed.calls();
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        assert!(
            !feed
                .calls()
                .iter()
                .any(|call| matches!(call, Call::Footprints(_)))
        );
    }
}

#[test]
fn prepared_pages_continue_backwards_without_bridging_disjoint_coverage() {
    let start = bars(1)[0].time;
    for disjoint in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        #[cfg(feature = "studies")]
        let input = {
            let id = chart.add_indicator(
                crate::studies::IndicatorFactory::new()
                    .create("Vol Profile FR")
                    .unwrap(),
            );
            frames.run(&mut chart);
            chart
                .indicators()
                .find(|(seen, _)| *seen == id)
                .unwrap()
                .1
                .order_flow_inputs()[0]
                .clone()
        };
        let boundary = start + TimeDelta::minutes(2);
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(prepared_page(
                &[Tick::new(boundary, 100.0, 5.0, None)],
                boundary + TimeDelta::minutes(1),
                false,
            )));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let calls = feed.calls();
        assert!(
            calls
                .iter()
                .any(|c| matches!(c, Call::Footprints(r) if r.before == Some(boundary)))
        );
        // The empty reply certifies its own window; the next reply knows only its request.
        let cursor = start + TimeDelta::minutes(1);
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(
                FootprintPage::new(Vec::new(), boundary, false).with_continue_before(cursor)
            ));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert!(
            feed.calls()
                .iter()
                .any(|c| matches!(c, Call::Footprints(r) if r.before == Some(cursor)))
        );
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(prepared_page(
                &[Tick::new(start, 100.0, 7.0, None)],
                if disjoint {
                    cursor - TimeDelta::seconds(1)
                } else {
                    cursor
                },
                true,
            )));
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        assert_eq!(flow.bar_at(start).is_none(), disjoint);
        assert_eq!(flow.bar_at(boundary).unwrap().unknown, 5.0);
        #[cfg(feature = "studies")]
        {
            let read = input.footprint_snapshot(start);
            assert_eq!(
                read.bars(..)
                    .map(|bar| (bar.time, bar.volume()))
                    .collect::<Vec<_>>(),
                if disjoint {
                    vec![(boundary, 5.0)]
                } else {
                    vec![(start, 7.0), (boundary, 5.0)]
                }
            );
        }
        assert_eq!(
            matches!(chart.feed.last_error, Some(FeedError::InvalidData(_))),
            disjoint
        );
        frames.run(&mut chart);
        assert!(feed.footprints.lock().unwrap().is_empty());
    }
    let (feed, mut chart, mut frames) = prepared_chart();
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(FootprintPage::new(Vec::new(), start, false)));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert!(
        feed.footprints.lock().unwrap().is_empty(),
        "an empty page without a cursor ends paging"
    );
}

#[test]
fn prepared_join_uses_the_stream_start_after_a_context_change() {
    let start = bars(1)[0].time;
    for resume in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let tick = |minute, size| Tick::new(start + TimeDelta::minutes(minute), 100.0, size, None);
        feed.sink().push_tick(tick(20, 7.0));
        frames.run(&mut chart);
        feed.footprints.lock().unwrap().clear();
        if resume {
            chart.set_feed_paused(true);
            frames.run(&mut chart);
            chart.set_feed_paused(false);
        } else {
            chart.set_timeframe(Timeframe::Min5);
        }
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(24), true));
        frames.run(&mut chart);
        feed.sink().push_tick(tick(21, 11.0));
        feed.sink().push_tick(tick(23, 13.0));
        frames.run(&mut chart);
        let timeframe = if resume {
            Timeframe::Min1
        } else {
            Timeframe::Min5
        };
        let mut saved = Footprint::new(BarClock::new(timeframe), 1.0, 1).unwrap();
        for trade in [tick(20, 7.0), tick(21, 11.0)] {
            saved.push(&trade).unwrap();
        }
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(FootprintPage::new(
                saved.bars().values().cloned().collect(),
                start + TimeDelta::seconds(1290),
                true,
            )));
        for trade_page in [false, true] {
            if trade_page {
                feed.trades
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap()
                    .send(Ok(TradePage::new(
                        vec![tick(20, 7.0), tick(21, 11.0)],
                        false,
                    )));
            }
            frames.run(&mut chart);
            let flow = chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap()
                .owned()
                .unwrap();
            assert!(
                !flow.span_has_gap(start, None),
                "resume {resume}, trade page {trade_page}"
            );
            assert_eq!(flow.complete_from(), Some(start + TimeDelta::minutes(20)));
        }
    }
}

#[test]
fn prepared_repeated_retained_trade_before_cutoff_keeps_its_bar_complete() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    let retained = Tick::new(start + TimeDelta::minutes(3), 100.0, 7.0, None);
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            std::slice::from_ref(&retained),
            start + TimeDelta::minutes(5),
            true,
        )));
    feed.sink().push_tick(retained.clone());
    frames.run(&mut chart);
    chart.set_feed_paused(true);
    frames.run(&mut chart);
    chart.set_feed_paused(false);
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    frames.run(&mut chart);
    feed.trades
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(Ok(TradePage::new(vec![retained.clone()], false)));
    frames.run(&mut chart);
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .owned()
        .unwrap();
    assert_eq!(
        FootprintView::from(flow)
            .complete_bar_at(retained.ts)
            .map(|bar| bar.unknown),
        Some(7.0)
    );
}

#[test]
fn prepared_repeated_maximum_timestamp_does_not_panic() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            start + TimeDelta::minutes(1),
            true,
        )));
    frames.run(&mut chart);
    let trade = Tick::new(DateTime::<Utc>::MAX_UTC, 100.0, 1.0, None);
    let link = chart.feed_link.as_mut().unwrap();
    link.trades.prepend(vec![trade.clone()], &mut |_, _| ());
    link.trades.reanchor();
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(vec![trade], false)));
    frames.run(&mut chart);
    assert_eq!(
        chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap()
            .bar_at(start)
            .unwrap()
            .unknown,
        5.0
    );
}

#[test]
fn prepared_request_matches_ohlc_context() {
    let (feed, mut chart, mut frames) = prepared_chart();
    chart.set_extended_hours(true);
    chart.set_dividend_adjusted(true);
    chart.set_conversion(Some("EUR".into()));
    chart.set_timeframe(Timeframe::Min5);
    feed.calls();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(0.25)));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let calls = feed.calls();
    let history = calls
        .iter()
        .find_map(|c| match c {
            Call::History(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let rows = calls
        .iter()
        .find_map(|c| match c {
            Call::Footprints(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (
            &rows.symbol,
            rows.timeframe,
            &rows.clock,
            rows.before,
            rows.since,
            rows.count,
            rows.extended_hours,
            rows.adjusted,
            &rows.conversion
        ),
        (
            &history.symbol,
            history.timeframe,
            &history.clock,
            history.before,
            history.since,
            history.count,
            history.extended_hours,
            history.adjusted,
            &history.conversion
        )
    );
    assert_eq!((rows.tick_size, rows.ticks_per_row), (0.25, 1));
}

#[test]
fn prepared_rows_clear_on_context_reset_and_tick_size_changes() {
    let start = bars(1)[0].time;
    for change in 0..7 {
        let (feed, mut chart, mut frames) = prepared_chart();
        feed.footprints
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(prepared_page(
                &[Tick::new(start, 100.0, 5.0, None)],
                start + TimeDelta::minutes(1),
                true,
            )));
        frames.run(&mut chart);
        match change {
            0 => chart.set_symbol("B"),
            1 => chart.set_timeframe(Timeframe::Min5),
            2 => chart.set_extended_hours(true),
            3 => chart.set_dividend_adjusted(true),
            4 => chart.set_conversion(Some("EUR".into())),
            5 => feed.sink().reset_replay(start),
            _ => {
                chart.set_feed_paused(true);
                frames.run(&mut chart);
                chart.set_feed_paused(false);
                frames.run(&mut chart);
                feed.resolve()
                    .send(Ok(Instrument::new("A").with_tick_size(0.25)));
            }
        }
        frames.run(&mut chart);
        assert!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .is_none_or(|flow| flow.live.prepared_before().is_none()),
            "change {change}"
        );
    }
}

#[test]
fn unused_prepared_rows_do_not_request_more_raw_trades() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            start + TimeDelta::minutes(5),
            true,
        )));
    frames.run(&mut chart);
    chart.set_chart_type(ChartType::Candles);
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(
            vec![Tick::new(start + TimeDelta::minutes(2), 100.0, 1.0, None)],
            false,
        )));
    feed.calls();
    for _ in 0..2 {
        frames.run(&mut chart);
    }
    assert!(
        !feed
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Trades(_)))
    );
    assert!(
        chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap()
            .prepared_before()
            .is_some()
    );
    chart.set_chart_type(ChartType::VolumeFootprint);
    for _ in 0..2 {
        frames.run(&mut chart);
    }
    assert!(
        feed.calls()
            .iter()
            .any(|call| matches!(call, Call::Trades(_)))
    );
}

#[test]
fn prepared_first_live_instant_needs_trade_history_even_on_a_bar_open() {
    let start = bars(1)[0].time;
    for page_first in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let cutoff = start + TimeDelta::minutes(1);
        let first = Tick::new(cutoff, 100.0, 7.0, None).with_id(7);
        let reply = feed.footprints.lock().unwrap().remove(0);
        if !page_first {
            feed.sink().push_tick(first.clone());
            frames.run(&mut chart);
        }
        reply.send(Ok(prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            cutoff,
            true,
        )));
        frames.run(&mut chart);
        if page_first {
            feed.sink().push_tick(first.clone());
            frames.run(&mut chart);
        }
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap();
        assert!(FootprintView::from(flow).complete_bar_at(start).is_some());
        assert!(
            FootprintView::from(flow).complete_bar_at(cutoff).is_none(),
            "page first {page_first}"
        );
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(vec![first], false)));
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap();
        assert_eq!(
            FootprintView::from(flow)
                .complete_bar_at(cutoff)
                .map(|bar| bar.unknown),
            Some(7.0)
        );
    }
}

#[test]
fn host_footprint_overrides_prepared_rows_for_one_draw() {
    use crate::chart::overlays::{FootprintConfig, FootprintMode};
    let (feed, mut chart, mut frames) = prepared_chart();
    let time = bars(5)[4].time;
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            &[Tick::new(time, 104.0, 999.0, None)],
            time + TimeDelta::minutes(1),
            true,
        )));
    frames.run(&mut chart);
    let mut host = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    host.push(&Tick::new(time, 104.0, 777.0, None)).unwrap();
    chart.apply_synced_time_scale(160.0, 1.0);
    let config = FootprintConfig {
        modes: vec![FootprintMode::Volume],
        auto_row_height: None,
        ..Default::default()
    };
    let mut output = frames.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            chart.show_footprint(ui, &host, &config);
        },
    );
    output.textures_delta.clear();
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"777"));
    assert!(!texts.contains(&"999"));
    assert_eq!(
        chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap()
            .bar_at(time)
            .unwrap()
            .unknown,
        999.0
    );
}

#[test]
fn prepared_resume_retains_known_rows_before_the_new_subscription_gap() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    let tick = |second| {
        Tick::new(start + TimeDelta::seconds(second), 100.0, 1.0, None).with_id(second as u64)
    };
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            &[Tick::new(start, 100.0, 5.0, None)],
            start + TimeDelta::minutes(1),
            true,
        )));
    for second in [60, 120, 180] {
        feed.sink().push_tick(tick(second));
    }
    frames.run(&mut chart);
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(vec![tick(60)], false)));
    frames.run(&mut chart);
    for (seconds, expected) in [
        (vec![310, 360, 420, 480], vec![0, 1, 2, 6, 7, 8]),
        (vec![610, 660], vec![0, 1, 2, 6, 7, 11]),
    ] {
        chart.set_feed_paused(true);
        frames.run(&mut chart);
        chart.set_feed_paused(false);
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(Ok(HistoryPage::new(
            bars((seconds.last().unwrap() / 60 + 1) as usize),
            true,
        )));
        frames.run(&mut chart);
        for second in seconds {
            feed.sink().push_tick(tick(second));
        }
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap();
        assert!(
            flow.prepared_before().is_some(),
            "prepared footprint lost: {flow:?}"
        );
        assert_eq!(
            flow.complete_bars(..)
                .map(|bar| (bar.time - start).num_minutes())
                .collect::<Vec<_>>(),
            expected
        );
    }
    // Filling only the latest reconnect must not certify the earlier missing executions.
    feed.trades
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(Ok(TradePage::new(
            [420, 480, 550].map(tick).to_vec(),
            false,
        )));
    frames.run(&mut chart);
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .owned()
        .unwrap();
    assert_eq!(
        flow.complete_bars(..)
            .map(|bar| (bar.time - start).num_minutes())
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 6, 7, 8, 9, 10, 11]
    );
}

#[test]
fn prepared_history_is_independent_of_raw_row_bar_and_tape_budgets() {
    let count = FOOTPRINT_BARS + 1;
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::minutes(count as i64);
    let feed = Arc::new(Script::default());
    feed.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("A")
        .with_initial_bars(count)
        .with_chart_type(ChartType::VolumeFootprint)
        .build();
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    let mut source = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
    for price in 0..=FOOTPRINT_ROWS {
        source
            .push(&Tick::new(start, price as f64, 1.0, None))
            .unwrap();
    }
    let mut candles = vec![Bar::new(
        start,
        0.0,
        FOOTPRINT_ROWS as f64,
        0.0,
        FOOTPRINT_ROWS as f64,
        (FOOTPRINT_ROWS + 1) as f64,
    )];
    for minute in 1..count {
        let time = start + TimeDelta::minutes(minute as i64);
        source.push(&Tick::new(time, 100.0, 1.0, None)).unwrap();
        candles.push(Bar::new(time, 100.0, 100.0, 100.0, 100.0, 1.0));
    }
    feed.page().send(page(candles, true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(FootprintPage::new(
            source.bars().values().cloned().collect(),
            cutoff,
            true,
        )));
    frames.run(&mut chart);
    chart.feed_link.as_mut().unwrap().trades = Trades::new(2);
    for minute in 0..3 {
        feed.sink().push_tick(Tick::new(
            cutoff + TimeDelta::minutes(minute),
            100.0,
            1.0,
            None,
        ));
        frames.run(&mut chart);
    }
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .owned()
        .unwrap();
    assert_eq!(
        flow.bars().first_key_value().map(|(time, _)| *time),
        Some(start)
    );
    assert_eq!(flow.bars().len(), count + 2);
    assert_eq!(flow.bar_at(start).unwrap().levels.len(), FOOTPRINT_ROWS + 1);
    assert_eq!(chart.trades(SeriesId::MAIN).count(), 2);
}

#[test]
fn ambiguous_trade_page_boundary_keeps_prepared_cells_but_not_the_join_bar() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::seconds(90);
    let saved = Tick::new(start, 100.0, 5.0, None);
    let boundary = Tick::new(cutoff, 100.0, 7.0, None);
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(
            std::slice::from_ref(&saved),
            cutoff,
            true,
        )));
    feed.sink().push_tick(boundary.clone());
    feed.sink()
        .push_tick(Tick::new(start + TimeDelta::minutes(2), 100.0, 11.0, None));
    frames.run(&mut chart);
    // Without ids, the page's boundary print could repeat the live one or precede it.
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(vec![saved, boundary], true)));
    frames.run(&mut chart);
    let flow = chart
        .feed_link
        .as_ref()
        .unwrap()
        .footprint
        .as_ref()
        .unwrap()
        .owned()
        .unwrap();
    assert_eq!(flow.bar_at(cutoff).unwrap().unknown, 7.0);
    assert_eq!(
        flow.complete_bars(..)
            .map(|bar| (bar.time - start).num_minutes())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

/// Hand counts at price 100, on executions without ids and on the same executions with ids made of
/// their time and size, in seconds from 00:00. A prepared bar is complete only while no instant in
/// it may lack executions that a trade page holds, or that traded before the stream began; ids
/// join the page's executions to the held ones and complete it. A page fills the time between the
/// kept executions and the first streamed one at or after them, and an execution the stream stamps
/// before kept ones joins its bar.
/// 1. The feed resumes before the first trade page answers. The streams send 130 s (11), 180 s (13)
///    and 250 s (17), the page 90 s (2), 130 s (3), 130 s (11), 180 s (13), 245 s (1), 250 s (4)
///    and 250 s (17): the 2 min bar holds 11 of the instant's 14, the old stream may have stopped
///    inside 180 s, and the 4 min bar holds 1 and 17 of 22.
/// 2. The first page leaves out 1260 s (3), then the interval changes to five minutes: the 20 min
///    bar holds 7 and 11 of 21.
/// 3. The interval changes, and the page after it, answered before the prepared page, holds 1560 s
///    (5), which the old stream did not send: the 25 min bar holds 13, 3 and 17 of 38. The 20 min
///    bar, from the first page's 1200 s (7) and the stream's 1260 s (11), is whole: the later page
///    repeats the first page's executions at the oldest held instant.
/// 4. A page that starts after the first stream's first instant answers, then the interval
///    changes, the stream starting on the 5 min bar's open: the bar may lack executions of 300 s
///    until a page holds them, here 3.
/// 5. A feed without trade history answers each page empty: the 5 min bar is whole.
/// 6. After an interval change, once the prepared page answered, the stream starts with 1380 s
///    (5), inside the kept executions, then sends 1680 s (17): the 20 min bar holds 7, 11 and 5.
///    The page then holds 1560 s (13) and 1620 s (3): the 25 min bar holds 13, 3 and 17, and
///    without ids the old stream may have stopped inside 1560 s.
/// 7. After an interval change the stream starts with 1380 s (5), older than every kept execution,
///    and the page holds 1200 s (1) and 1440 s (4): the 20 min bar holds 1, 5 and 4. In both, the
///    stream's 1380 s without an id is taken by its time for a repeat and dropped.
/// 8. Without prepared history, the page answers an interval change before the stream starts with
///    1530 s (5), older than the kept 1560 s (13), then sends 1680 s (17): the 25 min bar holds 5,
///    13 and 17. Without an id 1530 s is taken by its time for a repeat and dropped.
#[test]
fn bars_across_resumes_and_interval_changes_complete_with_every_execution() {
    enum Step {
        /// Executions the stream sends: seconds and size.
        Stream(&'static [(i64, f64)]),
        /// The latest trade page.
        Trades(&'static [(i64, f64)]),
        /// The prepared page on the chart's interval: its executions and cutoff.
        Prepared(&'static [(i64, f64)], i64),
        Resume,
        /// A change to five minutes.
        Interval,
        /// The complete bar holding this time, by its volume without ids and with them.
        Bar(i64, Option<f64>, Option<f64>),
    }
    use Step::{Bar, Interval, Prepared, Resume, Stream, Trades};
    #[rustfmt::skip]
    let rows = [
        ("a resume before the first trade page", vec![
            Stream(&[(130, 11.0), (180, 13.0)]), Prepared(&[(0, 5.0), (60, 7.0)], 90), Resume,
            Stream(&[(250, 17.0)]),
            Trades(&[(90, 2.0), (130, 3.0), (130, 11.0), (180, 13.0), (245, 1.0), (250, 4.0),
                (250, 17.0)]),
            Bar(120, None, Some(14.0)), Bar(180, None, Some(13.0)), Bar(240, None, Some(22.0)),
        ]),
        ("an interval change after a page left out the stream's first instant", vec![
            Stream(&[(1260, 11.0)]), Trades(&[(1200, 7.0), (1260, 3.0), (1260, 11.0)]),
            Stream(&[(1620, 13.0)]), Interval, Stream(&[(1860, 17.0)]),
            Prepared(&[(1200, 7.0)], 1230), Bar(1200, None, Some(21.0)),
        ]),
        ("an interval change whose page leaves out the old stream's last instant", vec![
            Stream(&[(1260, 11.0)]), Trades(&[(1200, 7.0)]), Stream(&[(1560, 13.0)]), Interval,
            Stream(&[(1680, 17.0)]),
            Trades(&[(1200, 7.0), (1260, 11.0), (1560, 13.0), (1560, 5.0), (1620, 3.0)]),
            Prepared(&[(600, 1.0)], 1200), Bar(1200, Some(18.0), Some(18.0)),
            Bar(1500, None, Some(38.0)),
        ]),
        ("an interval change after a page that starts after the stream's first instant", vec![
            Stream(&[(300, 11.0), (600, 13.0)]), Trades(&[(600, 13.0)]), Interval,
            Stream(&[(660, 17.0)]), Prepared(&[(0, 5.0)], 300), Bar(300, None, None),
            Trades(&[(300, 3.0), (300, 11.0), (600, 13.0), (630, 1.0)]),
            Bar(300, None, Some(14.0)),
        ]),
        ("an interval change on a feed without trade history", vec![
            Stream(&[(300, 11.0)]), Trades(&[]), Interval, Stream(&[(660, 17.0)]),
            Prepared(&[(0, 5.0)], 300), Trades(&[]), Bar(300, Some(11.0), Some(11.0)),
        ]),
        ("an interval change whose stream starts inside the kept executions", vec![
            Stream(&[(1260, 11.0)]), Trades(&[(1200, 7.0)]), Stream(&[(1560, 13.0)]), Interval,
            Stream(&[]), Prepared(&[(600, 1.0)], 1200), Stream(&[(1380, 5.0), (1680, 17.0)]),
            Bar(1200, Some(18.0), Some(23.0)), Trades(&[(1560, 13.0), (1620, 3.0)]),
            Bar(1500, None, Some(33.0)),
        ]),
        ("an interval change whose stream starts before the kept executions", vec![
            Stream(&[(1560, 13.0)]), Trades(&[(1500, 7.0)]), Interval,
            Stream(&[(1380, 5.0), (1680, 17.0)]),
            Trades(&[(1200, 1.0), (1440, 4.0), (1500, 7.0), (1560, 13.0), (1620, 3.0)]),
            Prepared(&[(600, 1.0)], 1200), Bar(1200, Some(5.0), Some(10.0)),
        ]),
        ("without prepared history, a stream that starts before a kept execution", vec![
            Trades(&[(1200, 1.0)]), Stream(&[(1560, 13.0)]), Interval,
            Trades(&[(1200, 1.0), (1560, 13.0)]), Stream(&[(1530, 5.0), (1680, 17.0)]),
            Bar(1500, Some(30.0), Some(35.0)),
        ]),
    ];
    let start = bars(1)[0].time;
    let at = |second: i64| start + TimeDelta::seconds(second);
    for (label, steps) in rows {
        for ids in [false, true] {
            let tick = |&(second, size): &(i64, f64)| {
                let trade = Tick::new(at(second), 100.0, size, None);
                if ids {
                    trade.with_id(second as u64 * 100 + size as u64)
                } else {
                    trade
                }
            };
            let (feed, mut chart, mut frames) = prepared_chart();
            let mut timeframe = Timeframe::Min1;
            for step in &steps {
                match step {
                    Stream(trades) => {
                        for trade in *trades {
                            feed.sink().push_tick(tick(trade));
                        }
                    }
                    Trades(trades) => {
                        let reply = feed.trades.lock().unwrap().pop().unwrap();
                        reply.send(Ok(TradePage::new(trades.iter().map(tick).collect(), false)));
                    }
                    Prepared(trades, cutoff) => {
                        let mut saved = Footprint::new(BarClock::new(timeframe), 1.0, 1).unwrap();
                        for trade in *trades {
                            saved.push(&tick(trade)).unwrap();
                        }
                        let saved = saved.bars().values().cloned().collect();
                        let reply = feed.footprints.lock().unwrap().pop().unwrap();
                        reply.send(Ok(FootprintPage::new(saved, at(*cutoff), true)));
                    }
                    Resume | Interval => {
                        if matches!(step, Resume) {
                            chart.set_feed_paused(true);
                            frames.run(&mut chart);
                            chart.set_feed_paused(false);
                        } else {
                            timeframe = Timeframe::Min5;
                            chart.set_timeframe(timeframe);
                        }
                        frames.run(&mut chart);
                        feed.resolve()
                            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
                        feed.page().send(page(bars(40), true));
                    }
                    Bar(second, without, with) => {
                        let link = chart.feed_link.as_ref().unwrap();
                        let flow = link.footprint.as_ref().unwrap();
                        assert_eq!(
                            flow.view()
                                .complete_bar_at(at(*second))
                                .map(|bar| bar.unknown),
                            if ids { *with } else { *without },
                            "{label}, ids {ids}, {second} s"
                        );
                    }
                }
                frames.run(&mut chart);
            }
        }
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
            // Span = 50 / 0.82; the bottom takes 8% of it and the top takes 10%.
            assert!(
                (range.min - (1000.0 - 50.0 / 0.82 * 0.08)).abs() < 1e-10,
                "{range:?}"
            );
            assert!(
                (range.max - (1050.0 + 50.0 / 0.82 * 0.1)).abs() < 1e-10,
                "{range:?}"
            );
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
        assert!(chart.feed_link.as_ref().unwrap().footprint.is_some());
    }
}

/// A full limit of 16 bars evicts down to 16 - 16 / 8 = 14 when a new bar overflows it, so a
/// stream restarts the studies on one new bar in three instead of on every one. An eviction
/// leaves every study drawing its values on the bars that remain until its recompute, between
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
        assert!(
            chart.indicators().all(|(_, study)| if len >= previous {
                study.values().len() == len
            } else {
                !study.values().is_empty()
            }),
            "minute {minute}: a study blank or behind the bars"
        );
        held.push(len);
        previous = len;
        // Each study waits a quarter second from its last recompute, and they recompute one per
        // frame.
        for _ in 0..20 {
            frames.run(&mut chart);
        }
    }
    assert_eq!(held, [14, 15, 16, 14, 15, 16]);
    assert!(
        feed.pages.lock().unwrap().is_empty(),
        "an eviction must not resume paging"
    );
}

/// A scroll-back page asked for before the main series lost its oldest bars is dropped rather
/// than merged in front of the bars that remain, whether a lowered limit, live bars past the
/// limit or the host evicted them, and paging asks again before the oldest bar held once the
/// limit allows it. A continuation named before an eviction lapses with the bar it continued.
#[test]
fn an_eviction_drops_the_scroll_back_page_in_flight_and_its_continuation() {
    use std::num::NonZeroUsize;
    let asked = |feed: &Script, from: DateTime<Utc>| -> Vec<i64> {
        feed.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::History(request) => request.before,
                _ => None,
            })
            .map(|before| (before - from).num_minutes())
            .collect()
    };
    let all = bars(40);
    for (name, limit, loaded, asked_again) in [
        ("lowered limit", None, 10..40, None),
        ("live bars", NonZeroUsize::new(16), 10..23, None),
        ("removed", None, 10..40, Some(13)),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_bar_limit(SeriesId::MAIN, limit);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        feed.page().send(page(all[loaded].to_vec(), false));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(feed.pages.lock().unwrap().len(), 1, "{name}: in flight");
        feed.calls();
        match name {
            "lowered limit" => {
                chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(16));
            }
            "live bars" => {
                let sink = feed.sink();
                for minute in 23..27 {
                    let time = all[0].time + TimeDelta::minutes(minute);
                    sink.push_tick(Tick::new(time, 200.0, 1.0, None));
                }
                frames.run(&mut chart);
            }
            _ => drop(chart.remove_oldest_bars(SeriesId::MAIN, 3)),
        }
        let held = chart.bars(SeriesId::MAIN).to_vec();
        assert!(held[0].time > all[10].time, "{name}: evicted");
        feed.page().send(page(all[8..10].to_vec(), false));
        frames.run(&mut chart);
        assert_eq!(chart.bars(SeriesId::MAIN), &held[..], "{name}");
        let expected: Vec<_> = asked_again.into_iter().collect();
        assert_eq!(asked(&feed, all[0].time), expected, "{name}");
    }

    let all = bars(600);
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    feed.page().send(page(all[200..].to_vec(), false));
    frames.run(&mut chart);
    chart.set_start_idx(0);
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(feed.pages.lock().unwrap().len(), 1, "scroll-back asked");
    // The view leaves the oldest bar before the feed names where older bars continue.
    chart.set_start_idx(350);
    frames.run(&mut chart);
    let window = HistoryPage::new(Vec::new(), false).with_continue_before(all[50].time);
    feed.page().send(Ok(window));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert!(feed.pages.lock().unwrap().is_empty(), "the view is far");
    chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(16));
    let oldest = chart.bars(SeriesId::MAIN)[0].time;
    chart.set_bar_limit(SeriesId::MAIN, None);
    feed.calls();
    frames.run(&mut chart);
    assert_eq!(
        asked(&feed, all[0].time),
        [(oldest - all[0].time).num_minutes()],
        "the continuation lapsed with the evicted bar"
    );
}

/// Historical executions and a live burst have separate anchors. Paging must not shift the
/// live cells, equal exchange timestamps remain separate, and a new interval reuses the tape.
#[test]
fn footprint_trade_intervals_follow_pages_streams_and_interval_changes() {
    use crate::model::OrderSide::{Buy, Sell};
    for (interval, fills, expected) in [
        (
            "2t",
            vec![(100.0, 2.0, Buy), (101.0, 3.0, Sell), (102.0, 4.0, Buy)],
            vec![(5.0, -1.0), (4.0, 4.0)],
        ),
        (
            "5v",
            vec![(100.0, 2.0, Buy), (101.0, 6.0, Sell)],
            vec![(5.0, -1.0), (3.0, -3.0)],
        ),
        (
            "2r",
            vec![(100.0, 2.0, Buy), (102.0, 3.0, Sell), (101.0, 4.0, Buy)],
            vec![(5.0, -1.0), (4.0, 4.0)],
        ),
        (
            "5delta",
            vec![(100.0, 2.0, Buy), (101.0, 8.0, Sell)],
            vec![(9.0, -5.0), (1.0, -1.0)],
        ),
    ] {
        for chart_type in [ChartType::VolumeFootprint, ChartType::SessionVolumeProfile] {
            if chart_type == ChartType::SessionVolumeProfile && !cfg!(feature = "studies") {
                continue;
            }
            let feed = Arc::new(Script::default());
            let canonical = interval == "2t" && chart_type == ChartType::VolumeFootprint;
            let instrument =
                || Instrument::new(if canonical { "EX:A" } else { "A" }).with_tick_size(1.0);
            let mut chart = fed(&feed, Some("A"));
            chart.set_trade_history_on_demand(true);
            let timeframe = interval.parse().unwrap();
            chart.set_timeframe(timeframe);
            chart.set_chart_type(chart_type);
            let mut frames = Frames::new();
            frames.run(&mut chart);
            feed.resolve().send(Ok(instrument()));
            let start = bars(1)[0].time;
            let history: Vec<_> = fills
                .iter()
                .map(|&(price, size, side)| Tick::new(start, price, size, Some(side)))
                .collect();
            let candles =
                crate::data::aggregate_trades_with_tick_size(&history, timeframe, Some(1.0))
                    .unwrap();
            feed.page().send(page(candles.clone(), true));
            frames.run(&mut chart);
            let live_start = start + TimeDelta::seconds(1);
            let live: Vec<_> = fills
                .iter()
                .map(|&(price, size, side)| Tick::new(live_start, price + 10.0, size, Some(side)))
                .collect();
            let sink = feed.sink();
            for trade in &live {
                sink.push_tick(trade.clone());
                frames.run(&mut chart);
            }
            // The history page arrives after the live stream has already allocated its bars.
            feed.trades
                .lock()
                .unwrap()
                .remove(0)
                .send(Ok(TradePage::new(history.clone(), true)));
            frames.run(&mut chart);
            let mut all_candles = candles;
            all_candles.extend(
                crate::data::aggregate_trades_with_tick_size(&live, timeframe, Some(1.0)).unwrap(),
            );
            assert_eq!(chart.bars(SeriesId::MAIN), all_candles, "{interval}");
            let flow = chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap()
                .owned()
                .unwrap();
            let got: Vec<_> = flow
                .bars()
                .values()
                .map(|bar| (bar.volume(), bar.delta()))
                .collect();
            assert_eq!(
                got,
                expected
                    .iter()
                    .chain(&expected)
                    .copied()
                    .collect::<Vec<_>>(),
                "{interval}"
            );
            for bar in &all_candles {
                assert_eq!(
                    flow.bar_at(bar.time).unwrap().volume(),
                    bar.volume,
                    "{interval}"
                );
            }
            let image = chart.feed_link.as_ref().unwrap().frozen();
            assert_eq!(
                image.footprint.as_ref().unwrap().owned().unwrap().bars(),
                flow.bars()
            );
            // A replay cutoff keeps the executions before it, and only those, on their own anchors.
            let link = chart.feed_link.as_ref().unwrap();
            let replayed = |cutoff| {
                let flow = link
                    .footprint(Some(cutoff), chart.bars(SeriesId::MAIN))
                    .unwrap();
                flow.bars()
                    .values()
                    .map(|bar| (bar.volume(), bar.delta()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(replayed(live_start), expected, "{interval}: replay");
            assert_eq!(
                replayed(live_start + TimeDelta::nanoseconds(1)),
                got,
                "{interval}: replay"
            );
            // The last execution is still queued when the interval changes.
            let queued = Tick::new(start + TimeDelta::seconds(2), 120.0, 1.0, Some(Buy));
            sink.push_tick(queued.clone());
            // No new trade page has answered on the new interval.
            chart.set_timeframe("3t".parse().unwrap());
            frames.run(&mut chart);
            feed.resolve().send(Ok(instrument()));
            frames.run(&mut chart);
            frames.run(&mut chart);
            let mut regrouped =
                Footprint::new(BarClock::new("3t".parse().unwrap()), 1.0, 1).unwrap();
            for trade in history.iter().chain(&live).chain(std::iter::once(&queued)) {
                regrouped.push(trade).unwrap();
            }
            assert_eq!(
                chart
                    .feed_link
                    .as_ref()
                    .unwrap()
                    .footprint
                    .as_ref()
                    .unwrap()
                    .owned()
                    .unwrap()
                    .bars(),
                regrouped.bars(),
                "{interval}: retained tape"
            );
            if interval == "2t" && chart_type == ChartType::VolumeFootprint {
                chart.feed_link.as_mut().unwrap().trades = Trades::new(2);
                let at = start + TimeDelta::seconds(3);
                let sink = feed.sink();
                for i in 0..5 {
                    sink.push_tick(Tick::new(
                        at + TimeDelta::milliseconds(i),
                        120.0,
                        1.0,
                        Some(Buy),
                    ));
                }
                frames.run(&mut chart);
                let flow = chart
                    .feed_link
                    .as_ref()
                    .unwrap()
                    .footprint
                    .as_ref()
                    .unwrap()
                    .owned()
                    .unwrap();
                // On the new three-trade interval the first bar lost all three fills; the
                // next holds exactly the two retained fills, without shifting its open.
                let kept = at + TimeDelta::milliseconds(3);
                assert_eq!(flow.complete_from(), Some(kept));
                assert_eq!(
                    flow.bars()
                        .values()
                        .map(|bar| (bar.time, bar.volume(), bar.delta()))
                        .collect::<Vec<_>>(),
                    vec![(kept, 2.0, 2.0)]
                );
                chart.set_timeframe("2t".parse().unwrap());
                frames.run(&mut chart);
                feed.resolve().send(Ok(instrument()));
                frames.run(&mut chart);
                frames.run(&mut chart);
                assert_eq!(
                    chart.trades(SeriesId::MAIN).count(),
                    2,
                    "an interval change keeps the bounded tape"
                );
                let flow = chart
                    .feed_link
                    .as_ref()
                    .unwrap()
                    .footprint
                    .as_ref()
                    .unwrap()
                    .owned()
                    .unwrap();
                assert_eq!(
                    flow.bars()
                        .values()
                        .map(|bar| (bar.time, bar.volume(), bar.delta()))
                        .collect::<Vec<_>>(),
                    vec![(kept, 2.0, 2.0)]
                );
                assert_eq!(
                    flow.complete_from(),
                    None,
                    "dropped data stays incomplete after reanchoring"
                );
                chart.set_extended_hours(true);
                chart.set_timeframe("3t".parse().unwrap());
                frames.run(&mut chart);
                assert_eq!(
                    chart.trades(SeriesId::MAIN).count(),
                    0,
                    "a different hours choice reloads its executions"
                );
            }
        }
    }
}

/// Hand counts of the feed's bars, grouped from the first execution of each tape. 5v: buy 2 and 3
/// of the sell of 4 (5, -1); its last unit and the buy of 4 (5, 3); the sell of 3 (3, -3). 2t: buy
/// 1 and sell 2 (3, -1); buy 3 and sell 4 (7, -1); the buy of 5 sharing the sell's second and the
/// sell of 6 (11, -1). 2r: buy 1 and sell 2 (3, -1); buy 3, sell 4 and buy 5, reaching two ticks
/// (12, 4); sell 6 and buy 7 (13, 1). 5delta: buy 2 and 7 of the sell of 8 (9, -5); its last unit
/// and two buys of 3 (7, 5); the sell of 1 (1, -1). Each trade page leaves the tape's older
/// executions out, as a page of the newest trades does, so a bar shows its own executions or none.
/// The second 2t row starts at a bar's first execution; the next has history answer after trades;
/// the last turns the footprint on during a replay that hides the bars the page rebuilds, then
/// steps forward to them.
#[test]
fn paged_trade_bar_cells_match_the_history_bar_they_draw_on() {
    use crate::model::OrderSide::{Buy, Sell};
    type Cells = Option<(f64, f64)>;
    type Row<'a> = (
        &'a str,
        &'a [(i64, f64, f64, crate::model::OrderSide)],
        usize,
        bool,
        bool,
        [Cells; 3],
    );
    let tape_2t = [
        (0, 100.0, 1.0, Buy),
        (1, 101.0, 2.0, Sell),
        (2, 102.0, 3.0, Buy),
        (3, 103.0, 4.0, Sell),
        (3, 104.0, 5.0, Buy),
        (4, 105.0, 6.0, Sell),
    ];
    let rows: [Row; 7] = [
        (
            "5v",
            &[
                (0, 100.0, 2.0, Buy),
                (1, 101.0, 4.0, Sell),
                (2, 102.0, 4.0, Buy),
                (3, 103.0, 3.0, Sell),
            ],
            1,
            true,
            false,
            [None, None, Some((3.0, -3.0))],
        ),
        (
            "2t",
            &tape_2t,
            1,
            true,
            false,
            [None, Some((7.0, -1.0)), Some((11.0, -1.0))],
        ),
        (
            "2t",
            &tape_2t,
            2,
            true,
            false,
            [None, Some((7.0, -1.0)), Some((11.0, -1.0))],
        ),
        (
            "2r",
            &[
                (0, 100.0, 1.0, Buy),
                (1, 102.0, 2.0, Sell),
                (2, 103.0, 3.0, Buy),
                (3, 104.0, 4.0, Sell),
                (3, 105.0, 5.0, Buy),
                (3, 105.0, 6.0, Sell),
                (4, 107.0, 7.0, Buy),
            ],
            1,
            true,
            false,
            [None, Some((12.0, 4.0)), Some((13.0, 1.0))],
        ),
        (
            "5delta",
            &[
                (0, 100.0, 2.0, Buy),
                (1, 101.0, 8.0, Sell),
                (2, 102.0, 3.0, Buy),
                (3, 103.0, 3.0, Buy),
                (4, 104.0, 1.0, Sell),
            ],
            1,
            true,
            false,
            [None, None, Some((1.0, -1.0))],
        ),
        (
            "2t",
            &tape_2t,
            1,
            false,
            false,
            [None, Some((7.0, -1.0)), Some((11.0, -1.0))],
        ),
        (
            "2t",
            &tape_2t,
            1,
            true,
            true,
            [None, Some((7.0, -1.0)), Some((11.0, -1.0))],
        ),
    ];
    for (interval, fills, from, history_first, in_replay, want) in rows {
        let start = bars(1)[0].time;
        let trades: Vec<_> = fills
            .iter()
            .map(|&(second, price, size, side)| {
                Tick::new(start + TimeDelta::seconds(second), price, size, Some(side))
            })
            .collect();
        let timeframe: Timeframe = interval.parse().unwrap();
        let candles =
            crate::data::aggregate_trades_with_tick_size(&trades, timeframe, Some(1.0)).unwrap();
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(timeframe);
        if !in_replay {
            chart.set_chart_type(ChartType::VolumeFootprint);
        }
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        let mut history = Some(feed.page());
        if history_first && let Some(reply) = history.take() {
            reply.send(page(candles.clone(), true));
        }
        frames.run(&mut chart);
        frames.run(&mut chart);
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(trades[from..].to_vec(), false)));
        frames.run(&mut chart);
        if let Some(reply) = history {
            reply.send(page(candles.clone(), true));
            frames.run(&mut chart);
        }
        if in_replay {
            chart.set_replay_time(Some(start + TimeDelta::seconds(1)));
            chart.set_chart_type(ChartType::VolumeFootprint);
            frames.run(&mut chart);
            chart.set_replay_time(Some(start + TimeDelta::seconds(5)));
            frames.run(&mut chart);
        }
        assert_eq!(chart.bars(SeriesId::MAIN), candles, "{interval}");
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap();
        let complete = flow.complete_from();
        // As the renderer selects a bar's cells.
        let got: Vec<Cells> = candles
            .iter()
            .map(|bar| {
                flow.bar_at(bar.time)
                    .filter(|_| complete.is_some_and(|from| bar.time >= from))
                    .map(|cells| (cells.volume(), cells.delta()))
            })
            .collect();
        assert_eq!(got, want, "{interval} from execution {from}");
    }
}

/// Hand counts on two-trade bars: buy 1 and buy 2 give 3; sell 4 and buy 8, traded while the
/// studies are removed, give 4 and a total of 7; sell 16 and buy 32, after undo, give 16 and 23.
/// On one-minute bars all six fall in one bar of 23. The profile's footprint holds the same
/// executions: volumes 3, 12 and 48, or 63 on the one-minute bar. A spread of 2 quoted in the
/// first bar has no execution to rebuild it from, and stays.
#[cfg(feature = "studies")]
#[test]
fn restored_order_flow_studies_count_executions_traded_while_they_were_removed() {
    use crate::data::OrderFlowKind;
    use crate::model::OrderSide::{Buy, Sell};
    use crate::studies::{OrderFlowStudy, VolProfileVisible};
    for (interval, want_cvd, want_volumes) in [
        ("2t", vec![3.0, 7.0, 23.0], vec![3.0, 12.0, 48.0]),
        ("1min", vec![23.0], vec![63.0]),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(interval.parse().unwrap());
        let cvd = chart.add_indicator(Box::new(OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
            min: 0.0,
            max: f64::MAX,
        })));
        let profile = chart.add_indicator(Box::new(VolProfileVisible::new(24)));
        let spread = chart.add_indicator(Box::new(OrderFlowStudy::new(OrderFlowKind::Spread)));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        let start = bars(1)[0].time;
        let sink = feed.sink();
        let fills = [
            (1.0, Buy),
            (2.0, Buy),
            (4.0, Sell),
            (8.0, Buy),
            (16.0, Sell),
            (32.0, Buy),
        ];
        for (i, &(size, side)) in fills.iter().enumerate() {
            if i == 2 {
                assert!(chart.user_action(|chart| {
                    chart.remove_series(cvd)
                        && chart.remove_series(profile)
                        && chart.remove_series(spread)
                }));
                frames.run(&mut chart);
            }
            if i == 4 {
                assert!(chart.undo());
                frames.run(&mut chart);
            }
            let at = start + TimeDelta::seconds(i as i64);
            sink.push_tick(Tick::new(at, 100.0, size, Some(side)));
            if i == 0 {
                sink.push_quote(crate::model::Level1Quote {
                    ts: at,
                    symbol: "A".into(),
                    bid: 99.0,
                    ask: 101.0,
                    bid_size: 1.0,
                    ask_size: 1.0,
                });
            }
            frames.run(&mut chart);
        }
        for _ in 0..8 {
            frames.run(&mut chart);
        }
        let study = |id| chart.indicators().find(|(seen, _)| *seen == id).unwrap().1;
        assert_eq!(study(cvd).values().line(0).to_vec(), want_cvd, "{interval}");
        let volumes: Vec<_> = study(profile).order_flow_inputs()[0]
            .footprints_from(DateTime::<Utc>::MIN_UTC)
            .bars
            .values()
            .map(|bar| bar.volume())
            .collect();
        assert_eq!(volumes, want_volumes, "{interval}");
        assert_eq!(study(spread).values().line(0)[0], 2.0, "{interval}");
    }
}

/// Without bars to match, a page groups from its oldest execution, which may lie inside a bar: that
/// bar stays incomplete until a page reports the start of trade history.
#[test]
fn paged_trades_without_history_leave_their_first_bar_incomplete() {
    use crate::model::OrderSide::Buy;
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_timeframe("2t".parse().unwrap());
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(Vec::new(), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let start = bars(1)[0].time;
    let trades = (0..4)
        .map(|second| {
            let at = start + TimeDelta::seconds(second);
            Tick::new(at, 100.0, 1.0, Some(Buy))
        })
        .collect();
    feed.trades
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(TradePage::new(trades, false)));
    frames.run(&mut chart);
    let link = chart.feed_link.as_ref().unwrap();
    let flow = link.footprint.as_ref().unwrap().owned().unwrap();
    assert_eq!(flow.complete_from(), Some(start + TimeDelta::seconds(2)));
}

/// Hand counts on 2t at price 100, executions of 1 without ids. A trade page answered in the drain
/// that streams the first executions holds a sell at 0 s, traded before the subscription, and its
/// copy of the stream's first buy; the stream buys twice at 0 s, then at 1 s and 2 s. The sink's
/// bars are the two buys at 0 s and those at 1 s and 2 s, each bid 0 and ask 2: the sell is in
/// neither. A footprint turned on after that drain, and again after it is turned off, shows both
/// bars as the sink allocated them. The footprint a bar replay builds for profile drawings, with
/// its cutoff after the last execution, shows the 0 s bar that way or without cells, never with
/// the sell.
#[test]
fn footprint_turned_on_later_counts_the_stream_at_the_first_instant() {
    use crate::model::OrderSide::{Buy, Sell};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_timeframe("2t".parse().unwrap());
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(Vec::new(), true));
    frames.run(&mut chart);
    let start = bars(1)[0].time;
    let fill =
        |second: i64, side| Tick::new(start + TimeDelta::seconds(second), 100.0, 1.0, Some(side));
    // The chart applies the page before the executions it drains with.
    (feed.trades.lock().unwrap().remove(0))
        .send(Ok(TradePage::new(vec![fill(0, Sell), fill(0, Buy)], false)));
    let sink = feed.sink();
    for second in [0, 0, 1, 2] {
        sink.push_tick(fill(second, Buy));
    }
    frames.run(&mut chart);
    let opens = [start, start + TimeDelta::seconds(1)];
    // The bid and ask of each bar drawn with cells; `None` draws a candle.
    let cells = |flow: &Footprint| {
        let complete = flow.complete_from();
        (opens.iter())
            .map(|open| {
                flow.bar_at(*open)
                    .filter(|_| complete.is_some_and(|from| *open >= from))
                    .map(|bar| (bar.bid, bar.ask))
            })
            .collect::<Vec<_>>()
    };
    for turned in ["on", "on again"] {
        chart.set_chart_type(ChartType::VolumeFootprint);
        for _ in 0..4 {
            frames.run(&mut chart);
        }
        let drawn: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
            .map(|bar| bar.time)
            .collect();
        assert_eq!(drawn, opens, "turned {turned}");
        let flow = (chart.feed_link.as_ref().unwrap().footprint.as_ref())
            .unwrap()
            .owned()
            .unwrap();
        assert_eq!(cells(flow), [Some((0.0, 2.0)); 2], "turned {turned}");
        chart.set_chart_type(ChartType::Candles);
        frames.run(&mut chart);
    }
    let link = chart.feed_link.as_ref().unwrap();
    let cutoff = start + TimeDelta::seconds(3);
    let replay = link.footprint(Some(cutoff), chart.bars(SeriesId::MAIN));
    let replayed = cells(&replay.unwrap());
    assert_eq!(replayed[1], Some((0.0, 2.0)), "replay");
    assert!(
        replayed[0].is_none() || replayed[0] == Some((0.0, 2.0)),
        "replay {:?}",
        replayed[0]
    );
}

/// Hand counts on 2t, executions of 1 without ids, the page applied as in the test above. The page
/// holds copies of the stream's buys at 0 s and 1 s and a sell at 0.5 s that the stream sends after
/// the buy at 1 s, so the sink leaves it out as late; the stream goes on with a buy at 2 s, a sell
/// and a buy at 3 s, and buys at 4 s and 5 s, all at 100. The sink's bars open at 0 s (bid 0, ask
/// 2), 1 s (0, 2), 3 s (1, 1) and 4 s (0, 2). A footprint turned on afterwards never counts the
/// sell, whether it is at 99 or at 100: the 0 s bar shows its cells or none, and the later bars
/// show theirs.
#[test]
fn footprint_turned_on_later_leaves_out_a_paged_execution_the_sink_dropped() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    let fill =
        |ms, price, side| Tick::new(start + TimeDelta::milliseconds(ms), price, 1.0, Some(side));
    let buy = |ms| fill(ms, 100.0, Buy);
    let opens = [0, 1000, 3000, 4000].map(|ms| start + TimeDelta::milliseconds(ms));
    for price in [99.0, 100.0] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe("2t".parse().unwrap());
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        let late = fill(500, price, Sell);
        let paged = vec![buy(0), buy(0), late.clone(), buy(1000)];
        (feed.trades.lock().unwrap().remove(0)).send(Ok(TradePage::new(paged, false)));
        let sink = feed.sink();
        let sell = fill(3000, 100.0, Sell);
        for trade in [buy(0), buy(0), buy(1000), late, buy(2000), sell, buy(3000)] {
            sink.push_tick(trade);
        }
        for ms in [4000, 5000] {
            sink.push_tick(buy(ms));
        }
        frames.run(&mut chart);
        chart.set_chart_type(ChartType::VolumeFootprint);
        for _ in 0..4 {
            frames.run(&mut chart);
        }
        let drawn: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
            .map(|bar| bar.time)
            .collect();
        assert_eq!(drawn, opens, "sell at {price}");
        let flow = (chart.feed_link.as_ref().unwrap().footprint.as_ref())
            .unwrap()
            .owned()
            .unwrap();
        let complete = flow.complete_from();
        let cells: Vec<_> = (opens.iter())
            .map(|open| {
                flow.bar_at(*open)
                    .filter(|_| complete.is_some_and(|from| *open >= from))
                    .map(|bar| (bar.bid, bar.ask))
            })
            .collect();
        assert!(
            cells[0].is_none_or(|cell| cell == (0.0, 2.0)),
            "sell at {price}: {cells:?}"
        );
        let later = [Some((0.0, 2.0)), Some((1.0, 1.0)), Some((0.0, 2.0))];
        assert_eq!(cells[1..], later, "sell at {price}");
    }
}

/// Hand counts at price 100, executions of 1 without ids, the chart showing candles. In the first
/// three rows, on 2t, a trade page is answered in the drain that streams the first executions,
/// into a store too small for both. In the first row the page holds a sell at 23:59:59, a sell
/// at 00:00:00 that traded before the subscription and a copy of the stream's first buy; the
/// stream buys twice at 00:00:00 and once at 00:00:01, and the store of three drops the sell at
/// 23:59:59 for the last buy. In the second the page also holds a sell at 23:59:58, for which the
/// store of three has no room. The sink's bars are the two buys at 00:00:00 and the buy at
/// 00:00:01, without the sells. The loss is older than both, so the footprint shows them as the
/// stream sent them, bid 0 and ask 2 from the two buys the store kept apart, then bid 0 and ask 1,
/// never with a sell, after it is turned on and after it is turned off and on. In the third row
/// the page holds copies of the buys at 00:00:00 to 00:00:03, the stream buys once a second from
/// 00:00:00 to 00:00:05, and the store of four drops the page's buys at 00:00:00 and 00:00:01 for
/// the last two: the 00:00:00 bar lost them and draws a candle, while the copies at 00:00:02 and
/// 00:00:03 and the buys at 00:00:04 and 00:00:05 fill their bars, each bid 0 and ask 2. In the
/// last two rows, on 50000t, the stream buys once a second from 00:00:00, the page holds copies
/// of its buys at 0 s to 99,999 s and the store holds both, but the sink holds 100,000 executions
/// between frames. In the fourth row the page is answered in the drain that streams 100,001 buys,
/// and the sink drops the buy at 0 s. In the fifth the page's drain streams the buy at 0 s, which
/// the store keeps apart, and the next one streams 100,001 buys from 1 s, of which the sink drops
/// the buy at 1 s. Either way the stream sent every execution after the one dropped, so the
/// page's copies fill the 50,000 s bar, bid 0 and ask 50,000, and the forming 100,000 s bar
/// shows the buy at 100,000 s, ask 1, or those at 100,000 s and 100,001 s, ask 2.
#[test]
fn footprint_after_store_loss_does_not_fill_the_first_bar_from_the_page() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    let at = |second: i64| start + TimeDelta::seconds(second);
    let fill = |second, side| Tick::new(at(second), 100.0, 1.0, Some(side));
    // The interval, the store's capacity, the page, the stream's buys in each drain, the newest
    // execution dropped, the opens of the chart's bars, all in seconds from 00:00:00, and the bid
    // and ask of each bar drawn with cells, `None` for a candle.
    type Row = (
        &'static str,
        &'static str,
        usize,
        Vec<Tick>,
        Vec<Vec<i64>>,
        i64,
        &'static [i64],
        &'static [Option<(f64, f64)>],
    );
    let copies = || Vec::from_iter((0..100_000).map(|second| fill(second, Buy)));
    let rows: [Row; 5] = [
        (
            "a live execution evicts an older page execution",
            "2t",
            3,
            vec![fill(-1, Sell), fill(0, Sell), fill(0, Buy)],
            vec![vec![0, 0, 1]],
            -1,
            &[0, 1],
            &[Some((0.0, 2.0)), Some((0.0, 1.0))],
        ),
        (
            "the page is larger than the store",
            "2t",
            3,
            vec![fill(-2, Sell), fill(-1, Sell), fill(0, Sell), fill(0, Buy)],
            vec![vec![0, 0, 1]],
            -1,
            &[0, 1],
            &[Some((0.0, 2.0)), Some((0.0, 1.0))],
        ),
        (
            "the loss reaches the stream's first instant",
            "2t",
            4,
            (0..4).map(|second| fill(second, Buy)).collect(),
            vec![vec![0, 1, 2, 3, 4, 5]],
            1,
            &[0, 2, 4],
            &[None, Some((0.0, 2.0)), Some((0.0, 2.0))],
        ),
        (
            "the sink drops executions in the page's drain",
            "50000t",
            2 * TRADE_CAPACITY,
            copies(),
            vec![(0..=100_000).collect()],
            0,
            &[0, 50_000, 100_000],
            &[None, Some((0.0, 50_000.0)), Some((0.0, 1.0))],
        ),
        (
            "the sink drops executions after the store kept some apart",
            "50000t",
            2 * TRADE_CAPACITY,
            copies(),
            vec![vec![0], (1..=100_001).collect()],
            1,
            &[0, 50_000, 100_000],
            &[None, Some((0.0, 50_000.0)), Some((0.0, 2.0))],
        ),
    ];
    for (label, interval, capacity, paged, streamed, lost, opens, cells) in rows {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(interval.parse().unwrap());
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        chart.feed_link.as_mut().unwrap().trades = Trades::new(capacity);
        // The chart applies the page before the executions it drains with.
        (feed.trades.lock().unwrap().remove(0)).send(Ok(TradePage::new(paged, false)));
        let sink = feed.sink();
        for drain in streamed {
            for second in drain {
                sink.push_tick(fill(second, Buy));
            }
            frames.run(&mut chart);
        }
        let trades = &chart.feed_link.as_ref().unwrap().trades;
        assert_eq!(trades.lost_until(), Some(at(lost)), "{label}: loss");
        let opens: Vec<_> = opens.iter().map(|second| at(*second)).collect();
        for turned in ["on", "on again"] {
            chart.set_chart_type(ChartType::VolumeFootprint);
            for _ in 0..4 {
                frames.run(&mut chart);
            }
            let drawn: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
                .map(|bar| bar.time)
                .collect();
            assert_eq!(drawn, opens, "{label}, turned {turned}");
            let flow = (chart.feed_link.as_ref().unwrap().footprint.as_ref())
                .unwrap()
                .owned()
                .unwrap();
            let complete = flow.complete_from();
            let shown: Vec<_> = (opens.iter())
                .map(|open| {
                    flow.bar_at(*open)
                        .filter(|_| complete.is_some_and(|from| *open >= from))
                        .map(|bar| (bar.bid, bar.ask))
                })
                .collect();
            assert_eq!(shown, cells, "{label}, turned {turned}");
            chart.set_chart_type(ChartType::Candles);
            frames.run(&mut chart);
        }
    }
}

/// Hand counts on two-trade bars of buys at 100, one a second: 1 and 2 (volume 3), 3 and 4 (7),
/// 5 and 6 (11), and the live 7 and 8 (15). A store of seven leaves out the buy of 1 in both
/// orders, and a store of six the buys of 1 and 2, so that the oldest held execution opens the
/// second bar. Either way the first bar draws a candle and the others keep their cells, however
/// many pages rebuild the footprint afterwards. The last row's page also holds a copy of the live
/// 7, which the store keeps apart, and the live 8 then drops the buy of 1. A bar replay keeps the
/// page's cells while its cutoff is at or before the first live buy, at 6 s, held or kept apart,
/// and has no footprint past it, since after the loss the stream is not summed again.
#[test]
fn history_page_after_a_store_loss_keeps_the_matched_cells() {
    use crate::model::OrderSide::Buy;
    let timeframe: Timeframe = "2t".parse().unwrap();
    let start = bars(1)[0].time;
    let buy = |i: i64| {
        Tick::new(
            start + TimeDelta::seconds(i),
            100.0,
            (i + 1) as f64,
            Some(Buy),
        )
    };
    let tape: Vec<_> = (0..6).map(buy).collect();
    let candles =
        crate::data::aggregate_trades_with_tick_size(&tape, timeframe, Some(1.0)).unwrap();
    let volumes = |chart: &Chart| -> Vec<Option<f64>> {
        let flow = (chart.feed_link.as_ref().unwrap().footprint.as_ref())
            .unwrap()
            .owned()
            .unwrap();
        let complete = flow.complete_from();
        (chart.bars(SeriesId::MAIN).iter())
            .map(|bar| {
                flow.bar_at(bar.time)
                    .filter(|_| complete.is_some_and(|from| bar.time >= from))
                    .map(|cells| cells.volume())
            })
            .collect()
    };
    for (label, live_first, capacity, page_buys) in [
        ("history page after the loss", false, 7, 6),
        ("trade page that overflows the store", true, 7, 6),
        ("history page after the loss of a whole bar", false, 6, 6),
        ("trade page that leaves out a whole bar", true, 6, 6),
        ("trade page that holds the first live buy", false, 7, 7),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(timeframe);
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(candles.clone(), false));
        frames.run(&mut chart);
        frames.run(&mut chart);
        chart.feed_link.as_mut().unwrap().trades = Trades::new(capacity);
        let sink = feed.sink();
        let live = |chart: &mut Chart, frames: &mut Frames| {
            for i in 6..8 {
                sink.push_tick(buy(i));
                frames.run(chart);
            }
        };
        if live_first {
            live(&mut chart, &mut frames);
        }
        let trade_page = TradePage::new((0..page_buys).map(buy).collect(), false);
        (feed.trades.lock().unwrap().remove(0)).send(Ok(trade_page));
        frames.run(&mut chart);
        if !live_first {
            live(&mut chart, &mut frames);
        }
        let kept = vec![None, Some(7.0), Some(11.0), Some(15.0)];
        assert_eq!(volumes(&chart), kept, "{label}");
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        assert_eq!(volumes(&chart), kept, "{label}, after the empty page");
        let link = chart.feed_link.as_ref().unwrap();
        let replayed = |cutoff| {
            let flow = link.footprint(Some(cutoff), chart.bars(SeriesId::MAIN))?;
            let cells = flow.bars().iter().map(|(open, bar)| (*open, bar.volume()));
            Some((flow.complete_from(), cells.collect::<Vec<_>>()))
        };
        let at = |second| start + TimeDelta::seconds(second);
        let page_cells = (Some(at(2)), vec![(at(2), 7.0), (at(4), 11.0)]);
        assert_eq!(
            replayed(at(6)),
            Some(page_cells),
            "{label}: replay to the stream"
        );
        let past = at(6) + TimeDelta::nanoseconds(1);
        assert_eq!(replayed(past), None, "{label}: replay into the stream");
    }
}

#[cfg(feature = "studies")]
#[test]
fn history_only_tick_trade_rate_uses_proven_receipts() {
    let start = bars(1)[0].time;
    let later = start + TimeDelta::seconds(1);
    let tape = [
        (start, 1.0),
        (start, 2.0),
        (start, 4.0),
        (later, 8.0),
        (later, 16.0),
    ]
    .map(|(ts, size)| Tick::new(ts, 100.0, size, None));
    let bar = |at, volume| Bar::new(at, 100.0, 100.0, 100.0, 100.0, volume);
    check_history_only_trade_rate(
        "2t",
        false,
        &tape,
        &[
            bar(start, 3.0),
            bar(start + TimeDelta::nanoseconds(1), 12.0),
            bar(later, 16.0),
        ],
        &[[2.0, 1.0, 2.0], [2.0, 2.0, 1.0], [1.0, 2.5, 2.0]],
    );
}

#[cfg(feature = "studies")]
#[test]
fn history_only_range_trade_rate_uses_proven_receipts() {
    let start = bars(1)[0].time;
    let later = start + TimeDelta::seconds(1);
    let tape = [
        (start, 100.0, 1.0),
        (start, 100.125, 2.0),
        (start, 100.25, 4.0),
        (start, 101.0, 8.0),
        (later, 100.5, 16.0),
        (later, 100.375, 32.0),
    ]
    .map(|(ts, price, size)| Tick::new(ts, price, size, None));
    // Three executions reach the range, two overshoot it, and the last stays open.
    check_history_only_trade_rate(
        "2r",
        false,
        &tape,
        &[
            Bar::new(start, 100.0, 100.25, 100.0, 100.25, 7.0),
            Bar::new(
                start + TimeDelta::nanoseconds(1),
                101.0,
                101.0,
                100.5,
                100.5,
                24.0,
            ),
            Bar::new(later, 100.375, 100.375, 100.375, 100.375, 32.0),
        ],
        &[[3.0, 1.5, 3.0], [2.0, 2.5, 1.0], [1.0, 3.0, 2.0]],
    );
}

#[cfg(feature = "studies")]
#[test]
fn history_only_trade_rate_splits_tied_buys_of_6_7_8_lots() {
    let start = bars(1)[0].time;
    let tape = [6.0, 7.0, 8.0]
        .map(|size| Tick::new(start, 100.0, size, Some(crate::model::OrderSide::Buy)));
    let bar = |i, lots| {
        Bar::new(
            start + TimeDelta::nanoseconds(i),
            100.0,
            100.0,
            100.0,
            100.0,
            lots,
        )
    };
    // 6+4, 3+7, then the last lot of the third buy.
    let candles = [bar(0, 10.0), bar(1, 10.0), bar(2, 1.0)];
    for interval in ["10v", "10delta"] {
        check_history_only_trade_rate(
            interval,
            false,
            &tape,
            &candles,
            &[[2.0, 1.0, 2.0], [2.0, 1.5, 3.0], [1.0, 1.5, 3.0]],
        );
        check_history_only_trade_rate(
            interval,
            true,
            &tape,
            &candles,
            &[[10.0, 6.5, 13.0], [10.0, 10.5, 21.0], [1.0, 10.5, 21.0]],
        );
    }
}

#[cfg(feature = "studies")]
#[test]
fn history_only_split_trade_rate_counts_each_print_once_in_the_windows() {
    use crate::model::OrderSide::{Buy, Sell};

    let start = bars(1)[0].time;
    let later = start + TimeDelta::seconds(1);
    let end = start + TimeDelta::seconds(2);
    for (interval, buy) in [("10v", 7.0), ("10delta", 13.0)] {
        // The sell of 27 puts 4, 10, 10 and 3 lots into consecutive candles.
        // The buy then closes the fourth candle at 10 volume or +10 delta.
        let tape = [
            Tick::new(start, 100.0, 6.0, Some(Sell)),
            Tick::new(start, 99.0, 27.0, Some(Sell)),
            Tick::new(later, 101.0, buy, Some(Buy)),
            Tick::new(end, 102.0, 2.0, None),
        ];
        let open = |i| start + TimeDelta::nanoseconds(i);
        let candles = [
            Bar::new(start, 100.0, 100.0, 99.0, 99.0, 10.0),
            Bar::new(open(1), 99.0, 99.0, 99.0, 99.0, 10.0),
            Bar::new(open(2), 99.0, 99.0, 99.0, 99.0, 10.0),
            Bar::new(open(3), 99.0, 101.0, 99.0, 101.0, 3.0 + buy),
            Bar::new(end, 102.0, 102.0, 102.0, 102.0, 2.0),
        ];
        check_history_only_trade_rate(
            interval,
            false,
            &tape,
            &candles,
            &[
                [2.0, 1.0, 2.0],
                [1.0, 1.0, 2.0],
                [1.0, 1.0, 2.0],
                [2.0, 1.5, 1.0],
                [1.0, 1.0, 1.0],
            ],
        );
        check_history_only_trade_rate(
            interval,
            true,
            &tape,
            &candles,
            &[
                [10.0, 16.5, 33.0],
                [10.0, 16.5, 33.0],
                [10.0, 16.5, 33.0],
                [3.0 + buy, (33.0 + buy) / 2.0, buy],
                [2.0, (buy + 2.0) / 2.0, 2.0],
            ],
        );
    }
}

#[cfg(feature = "studies")]
fn check_history_only_trade_rate(
    interval: &str,
    volume: bool,
    tape: &[Tick],
    bars: &[Bar],
    want: &[[f64; 3]],
) {
    use crate::data::OrderFlowKind;
    use crate::studies::OrderFlowStudy;

    let tf: Timeframe = interval.parse().unwrap();
    for (trades_first, complete, mismatch, grid) in [
        (false, true, None, Some(0.125)),
        (true, true, None, Some(0.125)),
        (false, false, None, Some(0.125)),
        (true, true, Some(1), Some(0.125)),
        (true, true, Some(bars.len() - 1), Some(0.125)),
        (true, true, None, None),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(tf);
        let rate = chart.add_indicator(Box::new(OrderFlowStudy::new(OrderFlowKind::TradeRate {
            volume,
            seconds: 2,
            instant: 1,
        })));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        let mut instrument = Instrument::new("A");
        instrument.tick_size = grid;
        feed.resolve().send(Ok(instrument));
        frames.run(&mut chart);
        let candles = || {
            let mut bars = bars.to_vec();
            if let Some(i) = mismatch {
                bars[i].volume += 1.0;
            }
            feed.page().send(page(bars, true));
        };
        let trades = || {
            let held = if complete { tape } else { &tape[3..] };
            feed.trades
                .lock()
                .unwrap()
                .remove(0)
                .send(Ok(TradePage::new(held.to_vec(), complete)));
        };
        if trades_first {
            trades();
            frames.run(&mut chart);
            candles();
        } else {
            candles();
            frames.run(&mut chart);
            trades();
        }
        frames.run(&mut chart);
        // Volume and Trade Rate each get a frame for their full recalculation.
        frames.run(&mut chart);
        let study = chart.indicators().find(|(id, _)| *id == rate).unwrap().1;
        let values = |i| [0, 1, 2].map(|line| study.values().value(line, i));
        if complete && mismatch.is_none() && (!matches!(tf, Timeframe::Range(_)) || grid.is_some())
        {
            // Window rates use provider times through the last receipt of this bar,
            // not the allocated bar open or later receipts sharing that timestamp.
            for (i, row) in want.iter().enumerate() {
                assert_eq!(values(i), row.map(Some));
            }

            // The sink allocates this fill to the last bar key independently. The page
            // proves no join there: neither its count nor its windows carry over.
            let mut live = tape.last().unwrap().clone();
            live.ts = bars.last().unwrap().time;
            let x = if volume { live.size } else { 1.0 };
            feed.sink().push_tick(live);
            frames.run(&mut chart);
            frames.run(&mut chart);
            let study = chart.indicators().find(|(id, _)| *id == rate).unwrap().1;
            assert_eq!(
                [0, 1, 2].map(|line| study.values().value(line, bars.len() - 1)),
                [x, x / 2.0, x].map(Some)
            );
        } else {
            // A truncated tape, mismatched candle or missing range grid proves no run.
            for i in 0..bars.len() {
                assert_eq!(
                    values(i),
                    [None; 3],
                    "{interval} bar {i}, mismatch {mismatch:?}"
                );
            }
        }
    }
}

/// Hand counts at price 100, one a second, with a store of three. 2t: buys of 1 and 2 (delta 3),
/// 4 and 8 (12), 16 and 32 (48), then 64. 5v: buy 3 and 2 of the buy of 4 (5); its other 2 and 3
/// of the sell of 4 (-1); its last unit, the buy of 2 and 2 of the sell of 6 (-1); its other 4 and
/// 1 of the buy of 5 (-3); its other 4 (4). One study is away for the last two executions and
/// another joins at the undo. Holding the last executions before the newest, they lack the first
/// two bars and match the study that stayed on the others. In the other rows nothing is lost, but
/// a trade page is answered in the drain that streams the first executions. In two rows it stands
/// in for them without an id: the buy of 1, or the buys of 1, 2 and 4 at one instant, so that the
/// bar of 4 and 8 opens a nanosecond after the first. 2t bars of 1 and 2 (3), 4 and 8 (12), 16
/// and 32 (48), which the rebuilt studies and an added footprint source, buying 3, 12 and 48,
/// match on every bar. In the last three rows the page holds only a sell of 1 at the first
/// instant, which the stream never sends, and the stream buys 1 each time: once at that instant
/// and three times a second apart, for 2t bars of two buys each (2, 2); or three times at that
/// instant and then three more, for bars of two buys (2), the third buy with the fourth a
/// nanosecond later (2) and the last two (2); or as the first with an id on each buy. The sell
/// counts in no bar: the rebuilt studies show 2 on every bar and the footprint source buys 2.
#[cfg(feature = "studies")]
#[test]
fn restored_order_flow_study_keeps_the_chart_bars() {
    use crate::data::OrderFlowInput;
    use crate::model::OrderSide::{Buy, Sell};
    use crate::studies::OrderFlowStudy;
    let rows = [
        (
            "2t",
            vec![
                (1.0, Buy),
                (2.0, Buy),
                (4.0, Buy),
                (8.0, Buy),
                (16.0, Buy),
                (32.0, Buy),
                (64.0, Buy),
            ],
            vec![3.0, 12.0, 48.0, 64.0],
            0,
            vec![],
            false,
        ),
        (
            "5v",
            vec![
                (3.0, Buy),
                (4.0, Buy),
                (4.0, Sell),
                (2.0, Buy),
                (6.0, Sell),
                (5.0, Buy),
            ],
            vec![5.0, -1.0, -1.0, -3.0, 4.0],
            0,
            vec![],
            false,
        ),
        (
            "2t",
            vec![
                (1.0, Buy),
                (2.0, Buy),
                (4.0, Buy),
                (8.0, Buy),
                (16.0, Buy),
                (32.0, Buy),
            ],
            vec![3.0, 12.0, 48.0],
            1,
            vec![(1.0, Buy)],
            false,
        ),
        (
            "2t",
            vec![
                (1.0, Buy),
                (2.0, Buy),
                (4.0, Buy),
                (8.0, Buy),
                (16.0, Buy),
                (32.0, Buy),
            ],
            vec![3.0, 12.0, 48.0],
            3,
            vec![(1.0, Buy), (2.0, Buy), (4.0, Buy)],
            false,
        ),
        (
            "2t",
            vec![(1.0, Buy); 4],
            vec![2.0; 2],
            1,
            vec![(1.0, Sell)],
            false,
        ),
        (
            "2t",
            vec![(1.0, Buy); 6],
            vec![2.0; 3],
            3,
            vec![(1.0, Sell)],
            false,
        ),
        (
            "2t",
            vec![(1.0, Buy); 4],
            vec![2.0; 2],
            1,
            vec![(1.0, Sell)],
            true,
        ),
    ];
    let start = bars(1)[0].time;
    let values = |chart: &Chart, id| {
        let study = chart.indicators().find(|(seen, _)| *seen == id).unwrap().1;
        (study.values().line(0).iter())
            .map(|value| (!value.is_nan()).then_some(*value))
            .collect::<Vec<_>>()
    };
    // The interval, the stream's executions, the deltas of the study that stayed, how many
    // executions share the first instant, the page's executions at that instant, and whether the
    // stream's carry ids.
    for (row, (interval, fills, stayed_values, paged, paged_fills, ids)) in
        rows.into_iter().enumerate()
    {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(interval.parse().unwrap());
        let stayed = chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
        let restored = chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        if paged == 0 {
            chart.feed_link.as_mut().unwrap().trades = Trades::new(3);
        }
        let sink = feed.sink();
        let footprint = OrderFlowInput::footprint();
        let mut added = None;
        for (i, &(size, side)) in fills.iter().enumerate() {
            if i + 2 == fills.len() {
                assert!(chart.user_action(|chart| chart.remove_series(restored)));
                frames.run(&mut chart);
            }
            if i + 1 == fills.len() {
                assert!(chart.undo());
                added = Some(chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false))));
                if paged > 0 {
                    chart.add_indicator(Box::new(PocAndBid {
                        inputs: [footprint.clone(), OrderFlowInput::book(1)],
                        clock: BarClock::exact(),
                        values: crate::studies::IndicatorValues::new(),
                    }));
                }
                frames.run(&mut chart);
            }
            // The paged executions share the first instant and drain with the page.
            let at = start + TimeDelta::seconds(i.saturating_sub(paged.max(1) - 1) as i64);
            let tick = |&(size, side)| Tick::new(at, 100.0, size, Some(side));
            if i + 1 == paged {
                let trades = paged_fills.iter().map(tick).collect();
                (feed.trades.lock().unwrap().remove(0)).send(Ok(TradePage::new(trades, false)));
            }
            let sent = tick(&(size, side));
            sink.push_tick(if ids { sent.with_id(i as u64) } else { sent });
            if i + 1 >= paged {
                frames.run(&mut chart);
            }
        }
        for _ in 0..8 {
            frames.run(&mut chart);
        }
        let all: Vec<_> = stayed_values.iter().copied().map(Some).collect();
        assert_eq!(values(&chart, stayed), all, "row {row}, {interval}");
        let held: Vec<_> = (all.iter().enumerate())
            .map(|(bar, value)| value.filter(|_| paged > 0 || bar >= 2))
            .collect();
        assert_eq!(
            values(&chart, restored),
            held,
            "row {row}, {interval}, restored"
        );
        assert_eq!(
            values(&chart, added.unwrap()),
            held,
            "row {row}, {interval}, added"
        );
        if paged > 0 {
            let bought: Vec<_> = (footprint.footprints_from(DateTime::<Utc>::MIN_UTC).bars)
                .iter()
                .map(|(open, bar)| (*open, bar.ask))
                .collect();
            let opens = chart.bars(SeriesId::MAIN).iter().map(|bar| bar.time);
            let expected: Vec<_> = opens.zip(stayed_values.iter().copied()).collect();
            assert_eq!(bought, expected, "row {row}, {interval}, footprint");
        }
    }
}

/// Hand counts on 2t at price 100: live buys of 1, 2, 4, 8 and 16 one a second from 0 s, so the
/// chart's bars are 1 and 2 (delta 3) at 0 s, 4 and 8 (12) at 2 s and 16 at 4 s. A trade page
/// answered in the drain that streams them without an id stands in for the buy of 1, or for the
/// buys of 1 to 8, which leaves no live execution in the first two bars, or for the buys of 1 to
/// 4 on a chart limited to two bars, which keeps the 2 s and 4 s bars while the study's source
/// keeps the 0 s one. The replay cutoff at 3 s is exclusive and falls in the 2 s bar, before
/// which only the buy of 4 came. With the buys of 1, 2 and 4 at 0 s, 8 at 1 s and 16 at 2 s, the
/// bars are 1 and 2 at 0 s, 4 and 8 (12) at 0 s + 1 ns, opened a nanosecond after it, and 16 at
/// 2 s; a page of the three at 0 s on a chart limited to two bars drops the 0 s bar the source
/// keeps, and the cutoff at 0.5 s falls in the 0 s + 1 ns bar, before which only the buy of 4 came.
#[cfg(feature = "studies")]
#[test]
fn replay_after_a_trade_page_repeats_a_live_execution_stops_at_the_cutoff() {
    use crate::model::OrderSide::Buy;
    use crate::studies::OrderFlowStudy;
    use std::num::NonZeroUsize;
    let start = bars(1)[0].time;
    let apart = [0, 1000, 2000, 3000, 4000];
    // The buys' times and the replay cutoff in milliseconds, the executions the page holds, the
    // bar limit (none for 0), and the deltas before and during the replay.
    type Row = ([i64; 5], i64, usize, usize, &'static [f64], &'static [f64]);
    let rows: [Row; 4] = [
        (apart, 3000, 1, 0, &[3.0, 12.0, 16.0], &[3.0, 4.0]),
        (apart, 3000, 4, 0, &[3.0, 12.0, 16.0], &[3.0, 4.0]),
        (apart, 3000, 3, 2, &[12.0, 16.0], &[4.0]),
        ([0, 0, 0, 1000, 2000], 500, 3, 2, &[12.0, 16.0], &[4.0]),
    ];
    for (times, cutoff, paged, limit, live, replayed) in rows {
        let tape: Vec<_> = (times.iter().zip([1.0, 2.0, 4.0, 8.0, 16.0]))
            .map(|(at, size)| {
                let at = start + TimeDelta::milliseconds(*at);
                Tick::new(at, 100.0, size, Some(Buy))
            })
            .collect();
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe("2t".parse().unwrap());
        chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(limit));
        chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        let page = TradePage::new(tape[..paged].to_vec(), false);
        feed.trades.lock().unwrap().remove(0).send(Ok(page));
        let sink = feed.sink();
        tape.iter().for_each(|trade| sink.push_tick(trade.clone()));
        let mut deltas = |chart: &mut Chart| {
            for _ in 0..8 {
                frames.run(chart);
            }
            let study = chart.indicators().next().unwrap().1;
            study.values().line(0).to_vec()
        };
        assert_eq!(deltas(&mut chart), live, "{paged} paged, cutoff {cutoff}");
        chart.set_feed_paused(true);
        chart.set_replay_time(Some(start + TimeDelta::milliseconds(cutoff)));
        assert_eq!(
            deltas(&mut chart),
            replayed,
            "{paged} paged, cutoff {cutoff}, replay"
        );
        chart.set_replay_time(None);
        assert_eq!(
            deltas(&mut chart),
            live,
            "{paged} paged, cutoff {cutoff}, left"
        );
    }
}

/// Hand counts on 2t at price 100: live buys of 1, 2, 4, 8 and 16 one a second from 0 s, and the
/// chart's bars are 1 and 2 (delta 3) at 0 s, 4 and 8 (12) at 2 s and 16 at 4 s. A trade store
/// that keeps four evicts the buy of 1 at the drain; one that keeps five loses nothing. The replay
/// cutoff is exclusive. At 3 s it falls in the 2 s bar, which lost nothing and before which only
/// the buy of 4 came. At 0.5 s it falls in the 0 s bar, which lost the buy of 1, so that bar has
/// no value. Trade Rate summing volume has the same bar totals, but leaves the 2 s bar empty with
/// either store, as every rolling study does with the cutoff's bar: its 60 s window at 3 s reaches
/// the lost buy, or executions before the oldest held one.
#[cfg(feature = "studies")]
#[test]
fn replay_with_or_without_a_trade_store_loss_stops_at_the_cutoff() {
    use crate::data::OrderFlowKind;
    use crate::model::OrderSide::Buy;
    use crate::studies::OrderFlowStudy;
    let start = bars(1)[0].time;
    let tape: Vec<_> = (0..)
        .zip([1.0, 2.0, 4.0, 8.0, 16.0])
        .map(|(second, size)| {
            let at = start + TimeDelta::seconds(second);
            Tick::new(at, 100.0, size, Some(Buy))
        })
        .collect();
    let rate = OrderFlowKind::TradeRate {
        volume: true,
        seconds: 60,
        instant: 1,
    };
    let live = [Some(3.0), Some(12.0), Some(16.0)];
    // The study, the store's capacity, the replay cutoff in milliseconds, and the study's first
    // line during the replay.
    let rows: [(OrderFlowStudy, usize, i64, &[Option<f64>]); 4] = [
        (
            OrderFlowStudy::execution_delta(false),
            4,
            3000,
            &[Some(3.0), Some(4.0)],
        ),
        (OrderFlowStudy::execution_delta(false), 4, 500, &[None]),
        (OrderFlowStudy::new(rate), 4, 3000, &[Some(3.0), None]),
        (OrderFlowStudy::new(rate), 5, 3000, &[Some(3.0), None]),
    ];
    for (row, (study, capacity, cutoff, replayed)) in rows.into_iter().enumerate() {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe("2t".parse().unwrap());
        chart.add_indicator(Box::new(study));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        chart.feed_link.as_mut().unwrap().trades = Trades::new(capacity);
        let sink = feed.sink();
        tape.iter().for_each(|trade| sink.push_tick(trade.clone()));
        let mut line = |chart: &mut Chart| {
            for _ in 0..8 {
                frames.run(chart);
            }
            let values = chart.indicators().next().unwrap().1.values();
            (0..values.line(0).len())
                .map(|bar| values.value(0, bar))
                .collect::<Vec<_>>()
        };
        assert_eq!(line(&mut chart), live, "row {row}");
        let lost = chart.feed_link.as_ref().unwrap().trades.lost_until();
        assert_eq!(
            lost,
            (capacity < tape.len()).then_some(start),
            "row {row}, loss"
        );
        chart.set_feed_paused(true);
        chart.set_replay_time(Some(start + TimeDelta::milliseconds(cutoff)));
        assert_eq!(line(&mut chart), replayed, "row {row}, replay");
        chart.set_replay_time(None);
        assert_eq!(line(&mut chart), live, "row {row}, left");
    }
}

/// On 3t, replay groups all executions the sink allocated, including those left to a page.
/// The first two rows count buys in threes; the last merges an older live buy before a kept sell
/// whose page copy has a different timestamp. Each cutoff excludes executions at or after it.
/// A profile drawing on a Volume Footprint chart reads those same cells through the replay pump.
#[test]
fn replay_footprint_bars_open_at_chart_bars() {
    use crate::model::OrderSide::{self, Buy, Sell};
    const S: i64 = 1_000_000;
    type Fill = (i64, OrderSide, Option<u64>);
    // Page, stream, cutoff and replayed (open, bid, ask), with times in microseconds.
    type Row = (
        &'static [Fill],
        &'static [Fill],
        i64,
        &'static [(i64, f64, f64)],
    );
    let rows: [Row; 3] = [
        (
            &[(0, Buy, None), (0, Buy, None)],
            &[
                (0, Buy, None),
                (0, Buy, None),
                (S, Buy, None),
                (2 * S, Buy, None),
                (3 * S, Buy, None),
                (4 * S, Buy, None),
            ],
            5 * S,
            &[(0, 0.0, 3.0), (2 * S, 0.0, 3.0)],
        ),
        (
            &[(0, Buy, None), (S, Buy, None)],
            &[
                (0, Buy, None),
                (S, Buy, None),
                (2 * S, Buy, None),
                (3 * S, Buy, None),
                (4 * S, Buy, None),
                (5 * S, Buy, None),
            ],
            4 * S,
            &[(0, 0.0, 3.0), (3 * S, 0.0, 1.0)],
        ),
        (
            &[(400, Sell, Some(2))],
            &[
                (-S, Buy, Some(1)),
                (0, Sell, Some(2)),
                (S, Buy, Some(3)),
                (2 * S, Buy, Some(4)),
            ],
            3 * S,
            &[(-S, 1.0, 2.0), (2 * S, 0.0, 1.0)],
        ),
    ];
    let start = bars(1)[0].time;
    let at = |micros: i64| start + TimeDelta::microseconds(micros);
    let fill = |&(micros, side, id): &Fill| {
        let fill = Tick::new(at(micros), 100.0, 1.0, Some(side));
        match id {
            Some(id) => fill.with_id(id),
            None => fill,
        }
    };
    for (copies, stream, cutoff, cells) in rows {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe("3t".parse().unwrap());
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(Vec::new(), true));
        frames.run(&mut chart);
        // History answers are applied before the executions drained in the same frame.
        let copies = TradePage::new(copies.iter().map(fill).collect(), false);
        feed.trades.lock().unwrap().remove(0).send(Ok(copies));
        let sink = feed.sink();
        stream.iter().for_each(|trade| sink.push_tick(fill(trade)));
        frames.run(&mut chart);
        let cells: Vec<_> = (cells.iter())
            .map(|&(open, bid, ask)| (at(open), bid, ask))
            .collect();
        let opens: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
            .map(|bar| bar.time)
            .collect();
        let expected: Vec<_> = cells.iter().map(|cell| cell.0).collect();
        assert_eq!(opens, expected, "cutoff {cutoff}: chart");
        let link = chart.feed_link.as_ref().unwrap();
        let flow = link
            .footprint(Some(at(cutoff)), chart.bars(SeriesId::MAIN))
            .unwrap();
        let replayed: Vec<_> = (flow.bars().iter())
            .map(|(open, bar)| (*open, bar.bid, bar.ask))
            .collect();
        assert_eq!(replayed, cells, "cutoff {cutoff}: replay");
        #[cfg(feature = "drawings")]
        {
            use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
            chart.set_chart_type(ChartType::VolumeFootprint);
            let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(1.0, 100.0)];
            chart.drawings_mut().drawings.push(drawing);
            chart.set_replay_time(Some(at(cutoff)));
            frames.run(&mut chart);
            let link = chart.feed_link.as_ref().unwrap();
            let pumped = (link.replay_footprint.as_ref()).map(|(time, flow)| {
                let cells = flow.as_ref().map(|flow| {
                    (flow.bars().iter())
                        .map(|(open, bar)| (*open, bar.bid, bar.ask))
                        .collect::<Vec<_>>()
                });
                (*time, cells)
            });
            assert_eq!(
                pumped,
                Some((at(cutoff), Some(cells))),
                "cutoff {cutoff}: pump"
            );
        }
    }
}

type PagedCells = Option<Vec<(i64, f64, f64)>>;

/// What the stream sends before the trade page. The history page holds the bars of the tape
/// before it.
enum Stream {
    Nothing,
    /// One execution, which opens a bar of its own.
    Execution(Tick),
    /// The tape's complete bars in `pushed`, after the history page or, with `first`, before it.
    /// The history page holds the bars of the tape's first `history` executions.
    Bars {
        history: usize,
        pushed: std::ops::Range<usize>,
        first: bool,
    },
}

/// Runs a chart on `interval` whose history page holds the bars of `tape` before `stream`, which
/// the stream sends next, and whose trade page then holds `tape[page]`. Returns, for each
/// history bar, the cells the renderer would draw as (level, bid, ask), or `None` for a candle.
fn paged_cells(
    interval: &str,
    tape: &[Tick],
    page_range: std::ops::Range<usize>,
    stream: &Stream,
) -> Vec<PagedCells> {
    let timeframe: Timeframe = interval.parse().unwrap();
    let candles = crate::data::aggregate_trades_with_tick_size(tape, timeframe, Some(1.0)).unwrap();
    let history = match stream {
        Stream::Nothing => candles[..].to_vec(),
        Stream::Execution(trade) => {
            candles[..candles.partition_point(|bar| bar.time < trade.ts)].to_vec()
        }
        Stream::Bars { history, .. } => {
            crate::data::aggregate_trades_with_tick_size(&tape[..*history], timeframe, Some(1.0))
                .unwrap()
        }
    };
    let feed = Arc::new(Script::default());
    let push = |pushed: &std::ops::Range<usize>| {
        (candles[pushed.clone()].iter()).for_each(|bar| feed.sink().push_bar(bar.clone()));
    };
    let mut chart = fed(&feed, Some("A"));
    chart.set_timeframe(timeframe);
    chart.set_chart_type(ChartType::VolumeFootprint);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    if let Stream::Bars {
        pushed,
        first: true,
        ..
    } = stream
    {
        frames.run(&mut chart);
        push(pushed);
        frames.run(&mut chart);
    }
    feed.page().send(page(history.clone(), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    match stream {
        Stream::Execution(trade) => feed.sink().push_tick(trade.clone()),
        Stream::Bars {
            pushed,
            first: false,
            ..
        } => push(pushed),
        _ => {}
    }
    frames.run(&mut chart);
    (feed.trades.lock().unwrap().remove(0))
        .send(Ok(TradePage::new(tape[page_range].to_vec(), false)));
    frames.run(&mut chart);
    let sent = match stream {
        Stream::Bars { pushed, .. } => pushed.end,
        _ => history.len(),
    };
    assert_eq!(
        chart.bars(SeriesId::MAIN)[..sent],
        candles[..sent],
        "{interval}"
    );
    let link = chart.feed_link.as_ref().unwrap();
    let flow = link.footprint.as_ref().unwrap().owned().unwrap();
    let complete = flow.complete_from();
    candles[..history.len()]
        .iter()
        .map(|bar| {
            flow.bar_at(bar.time)
                .filter(|_| complete.is_some_and(|from| bar.time >= from))
                .map(|cells| {
                    (cells.levels.values())
                        .map(|level| (level.level, level.bid, level.ask))
                        .collect()
                })
        })
        .collect()
}

/// Hand counts, tick size 1. 10v: buy 8 at 100 and 2 of the buy of 4 at 101; its other 2, the
/// sell of 3 at 102 and 5 of the sell of 9 at 103 (delta -6); its other 4, the buy of 2 at 104
/// and 4 of the buy of 7 at 105 (2); its other 3 and the sell of 7 at 106 (-4). 5delta: buy 3
/// and 2 of the buy of 4 at 101; its other 2, the sell of 1 at 102 and 4 of the buy of 6 at 103;
/// its other 2, the sell of 1 at 104 and 4 of the buy of 6 at 105; its other 2 and the buy of 3
/// at 106. Every boundary splits a fill, and each page starts with the fill the first boundary
/// split, so a bar draws its own cells or a candle. The third tape's bars open with whole
/// executions at new prices, so its page, which ends inside a bar, proves its start and must
/// draw. The fourth tape's page stops before the sell that completes its fourth bar with 1, the
/// part of the buy at 101 in the bar before, so a grouping from that buy matches every bar to
/// the page's end. Its own cells: 1 of that buy, the sell of 5 at 102 and 4 of the sell of 6 at
/// 103; the other 2, the buy of 5 at 104 and 3 of the buy of 6 at 105; the other 3, the sell of 6
/// at 106 and 1 of the sell of 4 at 106. The fifth tape's 10 volume bars split nearly every
/// boundary, as fractional sizes do. The sixth row repeats the fourth tape with a history of its
/// four complete bars and, before the page, a live sell of 1 at 106 at the time of the sell of 4
/// that completes the last of them. Without ids the store leaves out the page's sell of 4, so the
/// page lacks 1 of that bar, and a grouping from the buy at 101 again matches every bar to its
/// end: the page's end proves nothing there, and the own cells are those of the fourth tape. The
/// pushed tape's bars hold the buy of 6 at 100 and the sell of 4 at 101; the buy of 5 at 101 and
/// the sell of 5 at 102; the buy of 3 at 102 and the sell of 7 at 103; the buy of 4 at 104 and
/// the sell of 6 at 105. Its history page holds the first three and its trade page starts at a
/// price the bar before closes at and ends with the third. The host pushes the third bar again
/// as it is, with or without the fourth, after the history page or before it: only the fourth
/// is news the history lacks, so the page's end proves its start in every order. The last two
/// rows repeat the fourth tape, whose history page holds its fourth bar while it forms, through
/// the sell of 6 at 106 with volume 9, and whose host pushes that bar complete, after the
/// history page or before it. The page, every execution but the first and the last, lacks the 1
/// that completes it, and a grouping from the buy at 101 matches every bar to the page's end
/// with the pushed bar: that bar is news, so the page's end proves nothing there.
#[test]
fn split_fill_boundaries_draw_only_a_bars_own_executions() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    let tape = |fills: &[(f64, f64, crate::model::OrderSide)]| -> Vec<Tick> {
        (fills.iter().enumerate())
            .map(|(second, &(price, size, side))| {
                let at = start + TimeDelta::seconds(second as i64);
                Tick::new(at, price, size, Some(side))
            })
            .collect()
    };
    // Sizes of 1/1024 to 3 in steps of 1/1024, so every sum is exact, a millisecond apart.
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut price = 1000.0;
    let fractional: Vec<Tick> = (0..3000)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            price += (seed % 3) as f64 - 1.0;
            let size = ((seed >> 8) % 3072 + 1) as f64 / 1024.0;
            let side = if (seed >> 20) & 1 == 0 { Buy } else { Sell };
            Tick::new(start + TimeDelta::milliseconds(i), price, size, Some(side))
        })
        .collect();
    // Each bar takes the next 10 of volume, splitting the fill that crosses it.
    let mut own = vec![std::collections::BTreeMap::new()];
    let mut volume = 0.0;
    for trade in &fractional {
        let level = trade.price as i64;
        let mut left = trade.size;
        while left > 0.0 {
            let part = left.min(10.0 - volume);
            let cell = (own.last_mut().unwrap())
                .entry(level)
                .or_insert((level, 0.0, 0.0));
            if trade.side == Some(Sell) {
                cell.1 += part;
            } else {
                cell.2 += part;
            }
            (volume, left) = (volume + part, left - part);
            if volume == 10.0 {
                own.push(std::collections::BTreeMap::new());
                volume = 0.0;
            }
        }
    }
    let fractional_cells: Vec<PagedCells> = (own.into_iter())
        .filter(|cells| !cells.is_empty())
        .map(|cells| Some(cells.into_values().collect()))
        .collect();
    let issue = tape(&[
        (100.0, 9.0, Buy),
        (101.0, 2.0, Buy),
        (102.0, 5.0, Sell),
        (103.0, 6.0, Sell),
        (104.0, 5.0, Buy),
        (105.0, 6.0, Buy),
        (106.0, 6.0, Sell),
        (106.0, 4.0, Sell),
    ]);
    let issue_own = vec![
        None,
        Some(vec![(101, 0.0, 1.0), (102, 5.0, 0.0), (103, 4.0, 0.0)]),
        Some(vec![(103, 2.0, 0.0), (104, 0.0, 5.0), (105, 0.0, 3.0)]),
        Some(vec![(105, 0.0, 3.0), (106, 7.0, 0.0)]),
        None,
    ];
    let pushed = tape(&[
        (100.0, 6.0, Buy),
        (101.0, 4.0, Sell),
        (101.0, 5.0, Buy),
        (102.0, 5.0, Sell),
        (102.0, 3.0, Buy),
        (103.0, 7.0, Sell),
        (104.0, 4.0, Buy),
        (105.0, 6.0, Sell),
    ]);
    let pushed_own = vec![
        None,
        Some(vec![(101, 0.0, 5.0), (102, 5.0, 0.0)]),
        Some(vec![(102, 0.0, 3.0), (103, 7.0, 0.0)]),
    ];
    let rows = [
        (
            "10v",
            "10v",
            tape(&[
                (100.0, 8.0, Buy),
                (101.0, 4.0, Buy),
                (102.0, 3.0, Sell),
                (103.0, 9.0, Sell),
                (104.0, 2.0, Buy),
                (105.0, 7.0, Buy),
                (106.0, 7.0, Sell),
            ]),
            1..7,
            vec![
                None,
                Some(vec![(101, 0.0, 2.0), (102, 3.0, 0.0), (103, 5.0, 0.0)]),
                Some(vec![(103, 4.0, 0.0), (104, 0.0, 2.0), (105, 0.0, 4.0)]),
                Some(vec![(105, 0.0, 3.0), (106, 7.0, 0.0)]),
            ],
            false,
            Stream::Nothing,
        ),
        (
            "5delta",
            "5delta",
            tape(&[
                (100.0, 3.0, Buy),
                (101.0, 4.0, Buy),
                (102.0, 1.0, Sell),
                (103.0, 6.0, Buy),
                (104.0, 1.0, Sell),
                (105.0, 6.0, Buy),
                (106.0, 3.0, Buy),
            ]),
            1..7,
            vec![
                None,
                Some(vec![(101, 0.0, 2.0), (102, 1.0, 0.0), (103, 0.0, 4.0)]),
                Some(vec![(103, 0.0, 2.0), (104, 1.0, 0.0), (105, 0.0, 4.0)]),
                Some(vec![(105, 0.0, 2.0), (106, 0.0, 3.0)]),
            ],
            false,
            Stream::Nothing,
        ),
        (
            "whole start at a new price, page ending inside a bar",
            "10v",
            tape(&[
                (100.0, 6.0, Buy),
                (101.0, 4.0, Sell),
                (102.0, 5.0, Buy),
                (103.0, 5.0, Sell),
                (104.0, 3.0, Buy),
                (105.0, 7.0, Sell),
                (106.0, 2.0, Buy),
                (107.0, 3.0, Sell),
            ]),
            2..7,
            vec![
                None,
                Some(vec![(102, 0.0, 5.0), (103, 5.0, 0.0)]),
                Some(vec![(104, 0.0, 3.0), (105, 7.0, 0.0)]),
                None,
            ],
            true,
            Stream::Nothing,
        ),
        (
            "page ending before the fill that completes its last bar",
            "10v",
            issue.clone(),
            1..7,
            issue_own.clone(),
            false,
            Stream::Nothing,
        ),
        (
            "fractional volume tape draws no cells of another bar",
            "10v",
            fractional,
            1000..3000,
            fractional_cells,
            false,
            Stream::Nothing,
        ),
        (
            "stream at the instant that completes the last bar, without ids",
            "10v",
            issue.clone(),
            1..8,
            issue_own[..4].to_vec(),
            false,
            Stream::Execution(Tick::new(
                start + TimeDelta::seconds(7),
                106.0,
                1.0,
                Some(Sell),
            )),
        ),
        (
            "page ending with the bar before the first new pushed bar",
            "10v",
            pushed.clone(),
            2..6,
            pushed_own.clone(),
            true,
            Stream::Bars {
                history: 6,
                pushed: 2..4,
                first: false,
            },
        ),
        (
            "newest history bar pushed again before the history page",
            "10v",
            pushed.clone(),
            2..6,
            pushed_own.clone(),
            true,
            Stream::Bars {
                history: 6,
                pushed: 2..3,
                first: true,
            },
        ),
        (
            "newest history bar and a new bar pushed before the history page",
            "10v",
            pushed,
            2..6,
            pushed_own,
            true,
            Stream::Bars {
                history: 6,
                pushed: 2..4,
                first: true,
            },
        ),
        (
            "forming newest history bar pushed complete after the history page",
            "10v",
            issue.clone(),
            1..7,
            issue_own[..4].to_vec(),
            false,
            Stream::Bars {
                history: 7,
                pushed: 3..4,
                first: false,
            },
        ),
        (
            "newest bar pushed complete before a history page that holds it forming",
            "10v",
            issue,
            1..7,
            issue_own[..4].to_vec(),
            false,
            Stream::Bars {
                history: 7,
                pushed: 3..4,
                first: true,
            },
        ),
    ];
    let mut failures = Vec::new();
    for (label, interval, fills, page_range, own, must_draw, stream) in rows {
        let got = paged_cells(interval, &fills, page_range, &stream);
        assert_eq!(got.len(), own.len(), "{label}");
        for (bar, (got, own)) in got.iter().zip(&own).enumerate() {
            if got != own && (must_draw || got.is_some()) {
                failures.push(format!("{label} bar {bar}: drawn {got:?}, own {own:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// The page answering an interval change holds what traded between the two subscriptions, and
/// repeating the retained executions does not end the scroll-back for older ones.
#[test]
fn an_interval_change_page_fills_the_retained_tape() {
    let start = bars(1)[0].time;
    let fill = |second, size| Tick::new(start + TimeDelta::seconds(second), 100.0, size, None);
    for (page, volume) in [
        (vec![fill(0, 1.0), fill(1, 2.0)], 3.0),
        (vec![fill(0, 1.0), fill(1, 2.0), fill(2, 4.0)], 7.0),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(Timeframe::Min1);
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        let answer = |fills| {
            let reply = feed.trades.lock().unwrap().pop().unwrap();
            reply.send(Ok(TradePage::new(fills, false)));
        };
        answer(vec![fill(0, 1.0)]);
        feed.sink().push_tick(fill(1, 2.0));
        frames.run(&mut chart);
        chart.set_timeframe(Timeframe::Min5);
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        answer(page);
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .owned()
            .unwrap();
        assert_eq!(
            flow.bars().values().map(|bar| bar.volume()).sum::<f64>(),
            volume
        );
        assert!(!chart.feed.trade_history_complete);
    }
}

/// The page answering an interval change joins the new stream at the instant of its first
/// execution. Tick size 1, executions a second apart from 00:00:00: buy 9 at 100, buy 2 at 101,
/// sell 5 at 102, sell 6 at 103, buy 5 at 104, buy 6 at 105, sell 6 at 106 and sell 4 at 106. The
/// 20 volume stream delivers the second through the seventh. On 10 volume the stream opens with a
/// sell of 1 at 106 at 00:00:07, the trade page holds every execution but the first and the
/// history page the four complete bars. By hand those hold the buy of 9 and 1 of the buy of 2;
/// its other 1, the sell of 5 and 4 of the sell of 6; its other 2, the buy of 5 and 3 of the buy
/// of 6; its other 3, the sell of 6 and 1 of the sell of 4. With ids the page's sell of 4 is not
/// held, so it joins the tape in front of the live sell. Without ids it may repeat the live sell
/// and is left out, so the page's end proves nothing about the bar that sell completes, also when
/// the history page answers first and the trade page then adds nothing. Either way the page lacks
/// the buy at 00:00:00, nothing proves where its grouping starts, and every bar draws a candle.
/// Counting the buy of 2 whole instead matches every bar's prices and volume and gives the bars
/// at 00:00:01, 00:00:03 and 00:00:05 part of their neighbours' fills, as (level, bid, ask):
/// (101, 0, 2), (102, 5, 0), (103, 3, 0); (103, 3, 0), (104, 0, 5), (105, 0, 2); and
/// (105, 0, 4), (106, 6, 0). That grouping ends with the retained executions at 00:00:06, which
/// lack the sell of 4, so their end proves nothing either while the trade page is outstanding or
/// after a feed without trade history fails it.
///
/// Nor does it on a tick interval, where a bar in the history page opens between the two streams.
/// Executions of 1 at 100: two buys at 00:00:00, a sell and then a buy at 00:00:01, and a buy each
/// second through 00:00:06. The 3 tick bars open at 00:00:00, at 00:00:01 with the buy, and at
/// 00:00:04. The 2 tick stream delivers the two executions at 00:00:01 and the buy at 00:00:02,
/// and the 3 tick stream opens at 00:00:09. Grouped from the sell, the three retained executions
/// match the prices and volume of the bar at 00:00:01 and end there, as (100, 1, 2). The bar's own
/// are (100, 0, 3), its buy at 00:00:03 is not held, and it draws a candle.
///
/// What a second bar proves stays drawn meanwhile, and a trade page that adds nothing proves the
/// rest. Buys of 1 at 100: nine a second apart from 00:00:01 and three at 00:00:10, all delivered
/// by the 2 tick stream. The 3 tick bars open at 00:00:01, 00:00:04, 00:00:07 and 00:00:10, and
/// the 3 tick stream opens at 00:00:12. While the trade page is outstanding the bars at 00:00:01,
/// 00:00:04 and 00:00:07 draw their own (100, 0, 3), each proven by the bar that opens after it,
/// also at the instant the retained executions end, and the bar at 00:00:10, which only their end
/// could prove, draws a candle. The trade page then repeats the retained executions: nothing
/// traded between the two streams, their end is the page's, and the bar at 00:00:10 draws its
/// (100, 0, 3) as well.
///
/// The first stream's trade page may also have left out its executions at the instant of that
/// stream's first one, as ids did not tell them apart, and they stay missing after the change.
/// The volume tape again, with a sell of 5 at 106 at 00:00:06 and two sells of 1 at 106 at
/// 00:00:07: the 20 volume stream delivers the second of those, and its trade page, every
/// execution but the first, adds those before 00:00:07 and leaves its two at that instant out.
/// On 10 volume the trade page adds nothing and the history page holds the four complete bars,
/// the last with 3 of the buy of 6, the sell of 5 and both sells of 1. Counted whole, the buy of 2
/// matches every bar and ends with the held sell of 1, giving the bars at 00:00:01, 00:00:03 and
/// 00:00:05 the cells listed above. Nothing else proves where the grouping starts, so every bar
/// draws a candle, whether or not the 10 volume stream opens with a buy of 1 at 107 at 00:00:08.
///
/// Nor does a grouping that includes that instant prove bars on a tick interval, while the
/// executions after it still do. Executions of 1 at 100: buys at 00:00:00, 00:00:01 and
/// 00:00:02, two at 00:00:03 and one at 00:00:04, two sells at 00:00:05, a buy at 00:00:06, a buy
/// and then a sell at 00:00:07, and buys at 00:00:08 to 00:00:11. The 2 tick stream delivers
/// them from the second buy at 00:00:03, so its trade page leaves out the two buys there; after
/// the change the page leaves out its buy at 00:00:11, the last retained instant. Grouped from the
/// held buy at 00:00:03, they match the 3 tick bars at 00:00:03, 00:00:05 and 00:00:07 and give
/// each (100, 1, 2), where the first two have (100, 0, 3) and (100, 2, 1). Grouped from the first
/// sell at 00:00:05 they prove that bar and the bar at 00:00:07, which draw their own
/// (100, 2, 1) and (100, 1, 2); the bar at 00:00:09, which holds 00:00:11, draws a candle.
///
/// When the executions after that instant prove no bar, those before it prove the bars they
/// rebuild, short of the bar before it. Executions of 1 at 100: buys a second apart from 00:00:00
/// through 00:00:10 but a sell at 00:00:05, a sell and a buy at 00:00:12, and buys at 00:00:13 and
/// 00:00:14. The 2 tick stream delivers them from the buy at 00:00:12, so its trade page leaves out
/// the sell and the buy there. The 3 tick bars open at 00:00:00, 00:00:03, 00:00:06, 00:00:09 and
/// 00:00:12, and the 3 tick stream opens at 00:00:16. The bars at 00:00:03 and 00:00:06 draw their
/// own (100, 1, 2) and (100, 0, 3). The bar at 00:00:09 closes with the sell at 00:00:12 that the
/// store lacks: with the held buy in its place it would show (100, 0, 3) instead of its own
/// (100, 1, 2), so it draws a candle, and so does the bar at 00:00:12, as no execution after that
/// instant opens a bar.
#[test]
fn an_interval_change_page_draws_only_a_bars_own_executions() {
    use crate::model::OrderSide::{self, Buy, Sell};
    let start = bars(1)[0].time;
    type Fill = (i64, f64, f64, OrderSide);
    // The intervals before and after the change, the executions by second, those the first
    // stream delivers, whether its trade page then answers with every execution but the first,
    // the first of the second stream if any, and the bars of the history page.
    type Tape<'a> = (
        [&'a str; 2],
        &'a [Fill],
        std::ops::Range<usize>,
        bool,
        Option<Fill>,
        usize,
    );
    let volume: Tape = (
        ["20v", "10v"],
        &[
            (0, 100.0, 9.0, Buy),
            (1, 101.0, 2.0, Buy),
            (2, 102.0, 5.0, Sell),
            (3, 103.0, 6.0, Sell),
            (4, 104.0, 5.0, Buy),
            (5, 105.0, 6.0, Buy),
            (6, 106.0, 6.0, Sell),
            (7, 106.0, 4.0, Sell),
        ],
        1..7,
        false,
        Some((7, 106.0, 1.0, Sell)),
        4,
    );
    let ticks: Tape = (
        ["2t", "3t"],
        &[
            (0, 100.0, 1.0, Buy),
            (0, 100.0, 1.0, Buy),
            (1, 100.0, 1.0, Sell),
            (1, 100.0, 1.0, Buy),
            (2, 100.0, 1.0, Buy),
            (3, 100.0, 1.0, Buy),
            (4, 100.0, 1.0, Buy),
            (5, 100.0, 1.0, Buy),
            (6, 100.0, 1.0, Buy),
        ],
        2..5,
        false,
        Some((9, 100.0, 1.0, Buy)),
        3,
    );
    let quiet: Tape = (
        ["2t", "3t"],
        &[
            (1, 100.0, 1.0, Buy),
            (2, 100.0, 1.0, Buy),
            (3, 100.0, 1.0, Buy),
            (4, 100.0, 1.0, Buy),
            (5, 100.0, 1.0, Buy),
            (6, 100.0, 1.0, Buy),
            (7, 100.0, 1.0, Buy),
            (8, 100.0, 1.0, Buy),
            (9, 100.0, 1.0, Buy),
            (10, 100.0, 1.0, Buy),
            (10, 100.0, 1.0, Buy),
            (10, 100.0, 1.0, Buy),
        ],
        0..12,
        false,
        Some((12, 100.0, 1.0, Buy)),
        4,
    );
    let left_out_fills: &[Fill] = &[
        (0, 100.0, 9.0, Buy),
        (1, 101.0, 2.0, Buy),
        (2, 102.0, 5.0, Sell),
        (3, 103.0, 6.0, Sell),
        (4, 104.0, 5.0, Buy),
        (5, 105.0, 6.0, Buy),
        (6, 106.0, 5.0, Sell),
        (7, 106.0, 1.0, Sell),
        (7, 106.0, 1.0, Sell),
    ];
    let left_out: Tape = (
        ["20v", "10v"],
        left_out_fills,
        8..9,
        true,
        Some((8, 107.0, 1.0, Buy)),
        4,
    );
    let left_out_quiet: Tape = (["20v", "10v"], left_out_fills, 8..9, true, None, 4);
    let after_left_out: Tape = (
        ["2t", "3t"],
        &[
            (0, 100.0, 1.0, Buy),
            (1, 100.0, 1.0, Buy),
            (2, 100.0, 1.0, Buy),
            (3, 100.0, 1.0, Buy),
            (3, 100.0, 1.0, Buy),
            (4, 100.0, 1.0, Buy),
            (5, 100.0, 1.0, Sell),
            (5, 100.0, 1.0, Sell),
            (6, 100.0, 1.0, Buy),
            (7, 100.0, 1.0, Buy),
            (7, 100.0, 1.0, Sell),
            (8, 100.0, 1.0, Buy),
            (9, 100.0, 1.0, Buy),
            (10, 100.0, 1.0, Buy),
            (11, 100.0, 1.0, Buy),
        ],
        4..15,
        true,
        Some((13, 100.0, 1.0, Buy)),
        5,
    );
    let before_left_out: Tape = (
        ["2t", "3t"],
        &[
            (0, 100.0, 1.0, Buy),
            (1, 100.0, 1.0, Buy),
            (2, 100.0, 1.0, Buy),
            (3, 100.0, 1.0, Buy),
            (4, 100.0, 1.0, Buy),
            (5, 100.0, 1.0, Sell),
            (6, 100.0, 1.0, Buy),
            (7, 100.0, 1.0, Buy),
            (8, 100.0, 1.0, Buy),
            (9, 100.0, 1.0, Buy),
            (10, 100.0, 1.0, Buy),
            (12, 100.0, 1.0, Sell),
            (12, 100.0, 1.0, Buy),
            (13, 100.0, 1.0, Buy),
            (14, 100.0, 1.0, Buy),
        ],
        12..15,
        true,
        Some((16, 100.0, 1.0, Buy)),
        5,
    );
    enum Answer {
        Bars,
        Executions,
        NoExecutions,
    }
    use Answer::{Bars, Executions, NoExecutions};
    // The cells of the bars drawn, as (level, bid, ask).
    type Drawn<'a> = &'a [&'a [(i64, f64, f64)]];
    let buys: &[(i64, f64, f64)] = &[(100, 0.0, 3.0)];
    // The answers in order; the held executions and the bars drawn are checked after each one
    // from the history page on.
    type Row<'a> = (
        &'a str,
        bool,
        &'a Tape<'a>,
        [Answer; 2],
        &'a [f64],
        [Drawn<'a>; 2],
    );
    let rows: [Row; 11] = [
        (
            "ids",
            true,
            &volume,
            [Executions, Bars],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 6.0, 4.0, 1.0],
            [&[], &[]],
        ),
        (
            "no ids",
            false,
            &volume,
            [Executions, Bars],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 6.0, 1.0],
            [&[], &[]],
        ),
        (
            "no ids, history page first",
            false,
            &volume,
            [Bars, Executions],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 6.0, 1.0],
            [&[], &[]],
        ),
        (
            "ids, no trade history",
            true,
            &volume,
            [Bars, NoExecutions],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 6.0, 1.0],
            [&[], &[]],
        ),
        (
            "no ids, no trade history",
            false,
            &volume,
            [Bars, NoExecutions],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 6.0, 1.0],
            [&[], &[]],
        ),
        (
            "ticks, a bar opens between the streams, no trade history",
            false,
            &ticks,
            [Bars, NoExecutions],
            &[1.0, 1.0, 1.0, 1.0],
            [&[], &[]],
        ),
        (
            "ticks, nothing traded between the streams, a trade page that adds nothing",
            true,
            &quiet,
            [Bars, Executions],
            &[1.0; 13],
            [&[buys, buys, buys], &[buys, buys, buys, buys]],
        ),
        (
            "no ids, the first stream's trade page left out executions",
            false,
            &left_out,
            [Executions, Bars],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 5.0, 1.0, 1.0],
            [&[], &[]],
        ),
        (
            "no ids, the first stream's trade page left out executions, nothing streams after",
            false,
            &left_out_quiet,
            [Executions, Bars],
            &[2.0, 5.0, 6.0, 5.0, 6.0, 5.0, 1.0],
            [&[], &[]],
        ),
        (
            "ticks, executions after those the first stream's trade page left out",
            false,
            &after_left_out,
            [Executions, Bars],
            &[1.0; 14],
            [&[], &[&[(100, 2.0, 1.0)], &[(100, 1.0, 2.0)]]],
        ),
        (
            "ticks, executions before those the first stream's trade page left out",
            false,
            &before_left_out,
            [Executions, Bars],
            &[1.0; 14],
            [&[], &[&[(100, 1.0, 2.0)], &[(100, 0.0, 3.0)]]],
        ),
    ];
    let mut failures = Vec::new();
    for (label, ids, (intervals, fills, streamed, first_page, live, paged), answers, held, cells) in
        rows
    {
        let execution = |&(second, price, size, side): &Fill, id| {
            let trade = Tick::new(start + TimeDelta::seconds(second), price, size, Some(side));
            if ids { trade.with_id(id) } else { trade }
        };
        let tape: Vec<Tick> = (fills.iter().zip(1..))
            .map(|(fill, id)| execution(fill, id))
            .collect();
        let live = live.as_ref().map(|live| execution(live, 100));
        let paged = *paged;
        let timeframe: Timeframe = intervals[1].parse().unwrap();
        let candles =
            crate::data::aggregate_trades_with_tick_size(&tape, timeframe, Some(1.0)).unwrap();
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_timeframe(intervals[0].parse().unwrap());
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        (tape[streamed.clone()].iter()).for_each(|trade| feed.sink().push_tick(trade.clone()));
        frames.run(&mut chart);
        if *first_page {
            let reply = feed.trades.lock().unwrap().remove(0);
            reply.send(Ok(TradePage::new(tape[1..].to_vec(), false)));
            frames.run(&mut chart);
        }
        chart.set_timeframe(timeframe);
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        if let Some(live) = live {
            feed.sink().push_tick(live);
        }
        frames.run(&mut chart);
        // Each subscription asked for its own pages; the newest are the new interval's.
        let mut trades = feed.trades.lock().unwrap().pop();
        let mut history = feed.pages.lock().unwrap().pop();
        for (at, answer) in answers.into_iter().enumerate() {
            match answer {
                Bars => (history.take().unwrap()).send(page(candles[..paged].to_vec(), true)),
                Executions => {
                    (trades.take().unwrap()).send(Ok(TradePage::new(tape[1..].to_vec(), false)))
                }
                NoExecutions => trades.take().unwrap().send(Err(FeedError::NotSupported)),
            }
            frames.run(&mut chart);
            if history.is_some() {
                continue;
            }
            let label = format!("{label}, after answer {at}");
            assert_eq!(
                chart.bars(SeriesId::MAIN)[..paged],
                candles[..paged],
                "{label}"
            );
            let sizes: Vec<_> = (chart.trades(SeriesId::MAIN).map(|trade| trade.size)).collect();
            if sizes != held {
                failures.push(format!("{label}: held {sizes:?}"));
            }
            let link = chart.feed_link.as_ref().unwrap();
            let flow = link.footprint.as_ref().unwrap().owned().unwrap();
            let complete = flow.complete_from();
            let drawn: Vec<Vec<_>> = (candles[..paged].iter())
                .filter(|bar| complete.is_some_and(|from| bar.time >= from))
                .filter_map(|bar| flow.bar_at(bar.time))
                .map(|cells| {
                    (cells.levels.values())
                        .map(|level| (level.level, level.bid, level.ask))
                        .collect()
                })
                .collect();
            if drawn != cells[at] {
                failures.push(format!("{label}: drawn {drawn:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// When the interval changes again before the trade page answers, the executions held from before
/// the first change and those streamed since follow a time no page has filled: until one does, no
/// grouping across it proves a bar. Executions of 1 at 100 a second apart from 00:00:00, with ids,
/// all buys but one sell, on a 2 tick chart. With two changes, the streams send those at 00:00:00
/// to 00:00:04, at 00:00:07 to 00:00:09 and at 00:00:11, and the sell at 00:00:05 is never
/// received. Grouped from 00:00:00 the executions match the bars at 00:00:00, 00:00:02 and
/// 00:00:04, the last with the buy at 00:00:07 in place of its own sell, so that bar draws a
/// candle and the two before it their own (100, 0, 2). With three changes, the streams send those
/// at 00:00:00 and 00:00:01, at 00:00:03 to 00:00:10, at 00:00:12 and 00:00:13, and at 00:00:15:
/// the page will fill the time of the first and the last change only, so the time of the second,
/// with the sell at 00:00:11, stays missing. The bars at 00:00:04 and 00:00:06 draw their own
/// (100, 0, 2), and the bar at 00:00:10 a candle, where the buy at 00:00:12 in place of its sell
/// would match it.
#[test]
fn interval_changes_before_the_trade_page_prove_no_bar_across_their_time() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    // The intervals in order with the seconds of the executions each stream sends, the second of
    // the sell, the bars of the history page and the cells of the bars drawn.
    type Row<'a> = (
        &'a str,
        &'a [(&'a str, std::ops::Range<i64>)],
        i64,
        usize,
        &'a [&'a [(i64, f64, f64)]],
    );
    let buys: &[(i64, f64, f64)] = &[(100, 0.0, 2.0)];
    let rows: [Row; 2] = [
        (
            "two changes",
            &[("3t", 0..5), ("4t", 7..10), ("2t", 11..12)],
            5,
            5,
            &[buys, buys],
        ),
        (
            "three changes",
            &[("3t", 0..2), ("4t", 3..11), ("5t", 12..14), ("2t", 15..16)],
            11,
            7,
            &[buys, buys],
        ),
    ];
    let mut failures = Vec::new();
    for (label, streams, sell, paged, cells) in rows {
        let seconds = 0..streams.last().unwrap().1.end;
        let tape: Vec<Tick> = (seconds.zip(1..))
            .map(|(second, id)| {
                let side = if second == sell { Sell } else { Buy };
                Tick::new(start + TimeDelta::seconds(second), 100.0, 1.0, Some(side)).with_id(id)
            })
            .collect();
        let timeframe: Timeframe = "2t".parse().unwrap();
        let candles =
            crate::data::aggregate_trades_with_tick_size(&tape, timeframe, Some(1.0)).unwrap();
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        for (interval, streamed) in streams {
            chart.set_timeframe(interval.parse().unwrap());
            frames.run(&mut chart);
            feed.resolve()
                .send(Ok(Instrument::new("A").with_tick_size(1.0)));
            frames.run(&mut chart);
            for trade in &tape[streamed.start as usize..streamed.end as usize] {
                feed.sink().push_tick(trade.clone());
            }
            frames.run(&mut chart);
        }
        (feed.pages.lock().unwrap().pop().unwrap()).send(page(candles[..paged].to_vec(), true));
        frames.run(&mut chart);
        let link = chart.feed_link.as_ref().unwrap();
        let flow = link.footprint.as_ref().unwrap().owned().unwrap();
        let complete = flow.complete_from();
        let drawn: Vec<Vec<_>> = (candles[..paged].iter())
            .filter(|bar| complete.is_some_and(|from| bar.time >= from))
            .filter_map(|bar| flow.bar_at(bar.time))
            .map(|cells| {
                (cells.levels.values())
                    .map(|level| (level.level, level.bid, level.ask))
                    .collect()
            })
            .collect();
        if drawn != cells {
            failures.push(format!("{label}: drawn {drawn:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
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
        .unwrap()
        .owned()
        .unwrap();
    assert_eq!(flow.bars(), summed.bars());
    assert_eq!(flow.complete_from(), Some(start + TimeDelta::minutes(1)));
}

/// The bar of the oldest held trade is complete only once a trade page reports that nothing
/// exists before it: without trade history, or with a page that stops short of its start,
/// that bar is drawn as a candle.
#[test]
fn oldest_held_trade_leaves_its_bar_incomplete_until_history_starts() {
    use crate::model::OrderSide::Buy;
    enum Step {
        Reply(Result<TradePage, FeedError>),
        Trade(Tick),
    }
    let (older, forming) = (bars(5)[3].time, bars(5)[4].time);
    let next = forming + TimeDelta::minutes(1);
    let buy = |ts, size| Tick::new(ts, 104.0, size, Some(Buy));
    let held = || {
        vec![
            buy(older + TimeDelta::seconds(40), 0.5),
            buy(forming + TimeDelta::seconds(5), 1.0),
        ]
    };
    for (row, steps) in [
        // No trade history: the forming bar holds only what arrived live.
        vec![
            (Step::Reply(Err(FeedError::NotSupported)), None),
            (
                Step::Trade(buy(forming + TimeDelta::seconds(30), 2.0)),
                None,
            ),
            (Step::Trade(buy(next, 1.0)), Some(next)),
        ],
        // A page that does not reach the start of trade history.
        vec![(
            Step::Reply(Ok(TradePage::new(held(), false))),
            Some(forming),
        )],
        // A page with nothing before it.
        vec![(Step::Reply(Ok(TradePage::new(held(), true))), Some(older))],
        // Live trades come first, then an empty page ends trade history.
        vec![
            (Step::Trade(buy(forming + TimeDelta::seconds(5), 1.0)), None),
            (
                Step::Reply(Ok(TradePage::new(Vec::new(), true))),
                Some(forming),
            ),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), false));
        frames.run(&mut chart);
        for (step, expected) in steps {
            match step {
                Step::Reply(reply) => feed.trades.lock().unwrap().pop().unwrap().send(reply),
                Step::Trade(trade) => feed.sink().push_tick(trade),
            }
            frames.run(&mut chart);
            let flow = chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap()
                .owned()
                .unwrap();
            assert_eq!(flow.complete_from(), expected, "row {row}");
        }
    }
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

#[test]
fn switching_from_candles_to_range_requests_deferred_executions() {
    use crate::model::RangeBarSource::{Executions, OhlcApproximation};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_trade_history_on_demand(true);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    let mut history = bars(1);
    let start = history[0].time;
    (history[0].high, history[0].low) = (104.0, 96.0);
    feed.page().send(page(history, true));
    frames.run(&mut chart);
    assert!(feed.trades.lock().unwrap().is_empty());

    chart.set_chart_type(crate::model::ChartType::Range);
    chart.set_chart_type_params(crate::model::ChartTypeParams {
        range_size: 4.0,
        ..Default::default()
    });
    frames.run(&mut chart);
    assert_eq!(chart.range_bar_source(), Some(OhlcApproximation));
    let tape = [100.0, 96.0, 104.0, 100.0]
        .into_iter()
        .enumerate()
        .map(|(i, price)| Tick::new(start + TimeDelta::seconds(i as i64), price, 1.0, None))
        .collect();
    let reply = feed
        .trades
        .lock()
        .unwrap()
        .pop()
        .expect("Range needs trades");
    reply.send(Ok(TradePage::new(tape, true)));
    frames.run(&mut chart);
    assert_eq!(chart.range_bar_source(), Some(Executions));
    let closes: Vec<_> = chart.state.shown().iter().map(|bar| bar.close).collect();
    assert_eq!(closes, [96.0, 100.0, 104.0, 100.0]);
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

/// Trades that arrive after OHLC history keep the range bars approximated before the bar that
/// holds the first trade, on either side of a replay cursor at that trade.
#[test]
fn range_bars_keep_the_approximated_history_before_the_first_trade() {
    use crate::model::RangeBarSource::{OhlcApproximation, OhlcThenExecutions};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_chart_type(crate::model::ChartType::Range);
    chart.set_chart_type_params(crate::model::ChartTypeParams {
        range_size: 1.0,
        ..Default::default()
    });
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve().send(Ok(Instrument::new("A")));
    // Flat minute bars at 100 to 105: range bars closing at 101 to 105, then one at 105 in
    // progress.
    feed.page().send(page(bars(6), true));
    frames.run(&mut chart);
    let closes = |chart: &Chart| {
        chart
            .state
            .shown()
            .iter()
            .map(|bar| bar.close)
            .collect::<Vec<_>>()
    };
    assert_eq!(closes(&chart), [101.0, 102.0, 103.0, 104.0, 105.0, 105.0]);
    let history = chart.state.shown()[..4].to_vec();
    // Trades at 105 and 106 in the last bar's minute replace that bar: the bars before it give
    // the range bars closing at 101 to 104, and the trades one from 105 to 106.
    let minute = bars(6)[5].time;
    let sink = feed.sink();
    for (seconds, price) in [(10, 105.0), (20, 106.0)] {
        sink.push_tick(Tick::new(
            minute + TimeDelta::seconds(seconds),
            price,
            1.0,
            None,
        ));
    }
    frames.run(&mut chart);
    assert_eq!(chart.range_bar_source(), Some(OhlcThenExecutions));
    assert_eq!(closes(&chart), [101.0, 102.0, 103.0, 104.0, 106.0]);
    assert_eq!(chart.state.shown()[..4], history);
    // Replay keeps the five completed bars: their approximation before the first trade, the
    // approximated range bars and that trade's range bar after it.
    for (seconds, source, last) in [
        (5, OhlcApproximation, 104.0),
        (15, OhlcThenExecutions, 105.0),
    ] {
        chart.set_replay_time(Some(minute + TimeDelta::seconds(seconds)));
        frames.run(&mut chart);
        assert_eq!(chart.range_bar_source(), Some(source), "{seconds} s");
        assert_eq!(chart.state.shown()[..4], history, "{seconds} s");
        assert_eq!(chart.state.shown()[4].close, last, "{seconds} s");
        assert_eq!(chart.state.shown().len(), 5, "{seconds} s");
    }
}

#[test]
fn replay_reset_removes_future_data_from_the_chart() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    #[cfg(feature = "studies")]
    let factory = crate::studies::IndicatorFactory::new();
    #[cfg(feature = "studies")]
    let mut studies: Vec<_> = ["Quoted Spread", "Trade Rate", "Book Size"]
        .map(|name| chart.add_indicator(factory.create(name).unwrap()))
        .into();
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
    // The host answers the trade request it held, then seeks: the chart drains both together.
    (feed.trades.lock().unwrap().remove(0)).send(Ok(TradePage::new(
        vec![Tick::new(time, 100.0, 2.0, None)],
        false,
    )));
    sink.reset_replay(time);
    sink.push_tick(Tick::new(time, 100.0, 2.0, None).with_id(7));
    sink.push_quote(quote(time, 99.0, 100.0));
    sink.advance_time(time + TimeDelta::seconds(1));
    // A study added before that frame connects while the reset waits for the chart.
    #[cfg(feature = "studies")]
    studies.push(chart.add_indicator(factory.create("Trade Rate").unwrap()));
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
        let expected = [Some(4.0), Some(1.0), None, Some(1.0)];
        for (n, (id, expected)) in studies.into_iter().zip(expected).enumerate() {
            let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
            let values = study.order_flow_inputs()[0].values();
            assert!(values.keys().all(|at| *at == time), "{} {n}", study.name());
            assert_eq!(study.values().value(0, 0), expected, "{} {n}", study.name());
            if study.name() == "Trade Rate" {
                assert_eq!(study.values().value(2, 0), Some(1.0 / 3.0), "{n}");
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

/// Appearance reaches the painter, survives visibility changes and image copies, and
/// repeated enable or configuration calls keep the uploaded texels.
#[test]
fn heatmap_appearance_survives_visibility_and_frozen_images() {
    use crate::chart::overlays::LiquidityHeatmapConfig;
    use crate::data::DepthColumn;

    let mut chart = fed(&Arc::new(Script::default()), None);
    chart.set_instrument(Some(Instrument::new("TEST").with_tick_size(1.0)));
    let config = LiquidityHeatmapConfig {
        brightness: 0.5,
        lower_cutoff: 10.0,
        full_intensity_size: Some(999.0),
    };
    chart.set_liquidity_heatmap_config(config);
    chart.set_liquidity_heatmap(true);
    let mut history = DepthHistory::new(Default::default()).unwrap();
    history.push(DepthColumn {
        time: "2026-01-05T10:00:00Z".parse().unwrap(),
        span: chrono::TimeDelta::seconds(1),
        bids: vec![(100, 99.0)],
        asks: vec![(101, 9.0)],
        ..Default::default()
    });
    chart.set_depth_history(Some(history));
    let ctx = egui::Context::default();
    for frame in 0..5 {
        match frame {
            1 | 4 => {
                chart.set_liquidity_heatmap(true);
                chart.set_liquidity_heatmap_config(config);
            }
            2 => {
                chart.set_liquidity_heatmap(false);
                chart.set_liquidity_heatmap(true);
            }
            3 => chart.feed_link = Some(chart.feed_link.as_ref().unwrap().frozen()),
            _ => {}
        }
        assert_eq!(chart.liquidity_heatmap_config(), config);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            chart.paint_liquidity(ui.painter(), &chart.get_chart_mapping());
        });
        let alphas: Vec<_> = output
            .textures_delta
            .set
            .values()
            .flatten()
            .filter(|delta| delta.pos.is_some() && delta.image.size() == [1, 256])
            .map(|delta| {
                let egui::ImageData::Color(image) = &delta.image;
                [image.pixels[100].a(), image.pixels[101].a()]
            })
            .collect();
        output.textures_delta.clear();
        // 99 reaches an opaque part of the scale at 2/3 intensity: round(255 * 0.5) = 128.
        assert_eq!(
            alphas,
            if frame == 1 || frame == 4 {
                vec![]
            } else {
                vec![[128, 0]]
            }
        );
    }
}

/// Releasing history frees its textures, restarts paging and leaves the open sample intact.
#[test]
fn clearing_depth_history_frees_textures_and_refills() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.set_liquidity_heatmap(true);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(1), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    feed.depth
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(DepthPage::new(vec![], false)));
    let time = bars(1)[0].time;
    let sink = feed.sink();
    sink.set_server_time(time);
    let level = |price| crate::model::OrderBookLevel {
        price,
        size: 5.0,
        order_cnt: None,
    };
    sink.push_depth_snapshot(
        1,
        crate::model::OrderBook {
            ts: time,
            symbol: "A".into(),
            bids: vec![level(99.0)],
            asks: vec![level(101.0)],
        },
    );
    sink.advance_time(time + TimeDelta::seconds(1));
    let ctx = frames.ctx.clone();
    let paint = |chart: &Chart| {
        ctx.run_ui(Default::default(), |ui| {
            chart.paint_liquidity(ui.painter(), &chart.get_chart_mapping());
        })
    };
    let populated = |output: &egui::FullOutput| {
        output.textures_delta.set.values().flatten().any(|delta| {
            let egui::ImageData::Color(image) = &delta.image;
            delta.pos.is_some() && image.size == [1, 256] && image.pixels[99].a() > 0
        })
    };
    let mut output = ctx.run_ui(Default::default(), |ui| {
        chart.show(ui);
    });
    assert!(populated(&output));
    output.textures_delta.clear();
    feed.calls();
    chart.clear_depth_history();
    assert_eq!(
        chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .count(),
        0
    );
    let mut output = paint(&chart);
    assert!(!output.textures_delta.free.is_empty());
    assert!(output.textures_delta.set.is_empty());
    output.textures_delta.clear();
    frames.run(&mut chart);
    assert!(
        feed.calls()
            .iter()
            .any(|call| matches!(call, Call::Depth(r) if r.before.is_none()))
    );
    sink.advance_time(time + TimeDelta::seconds(2));
    let mut output = ctx.run_ui(Default::default(), |ui| {
        chart.show(ui);
    });
    let held: Vec<_> = chart
        .depth_history(SeriesId::MAIN)
        .unwrap()
        .columns()
        .map(|c| c.time)
        .collect();
    assert_eq!(held, [time + TimeDelta::seconds(1)]);
    assert!(populated(&output));
    output.textures_delta.clear();
}

/// Releasing history forgets scroll-back pages and their retries so paging starts at the newest.
#[test]
fn clearing_depth_history_discards_old_pages_and_retries() {
    for retry in [false, true] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(0.25)));
        let bars = bars(1000);
        feed.page().send(page(bars.clone(), true));
        frames.run(&mut chart);
        let column = |index: usize| crate::data::DepthColumn {
            time: bars[index].time,
            span: TimeDelta::seconds(60),
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
            .send(Ok(DepthPage::new(vec![column(900)], false)));
        frames.run(&mut chart);
        feed.calls();
        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 850.0,
            right: 899.0,
        });
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert!(matches!(
            &feed.calls()[..],
            [Call::Depth(r)] if r.before == Some(bars[900].time)
        ));
        let old = feed.depth.lock().unwrap().remove(0);
        if retry {
            old.send(Err(FeedError::Network("offline".into())));
            frames.run(&mut chart);
            chart.clear_depth_history();
        } else {
            chart.clear_depth_history();
            old.send(Ok(DepthPage::new(vec![column(899)], false)));
        }
        frames.time += 2.0;
        frames.run(&mut chart);
        assert_eq!(
            chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .columns()
                .count(),
            0,
            "a released page must not refill history (retry: {retry})"
        );
        assert!(
            matches!(&feed.calls()[..], [Call::Depth(r)] if r.before.is_none()),
            "release must request the newest page (retry: {retry})"
        );
        feed.depth
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(DepthPage::new(vec![column(999)], true)));
        frames.run(&mut chart);
        let held: Vec<_> = chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .map(|c| c.time)
            .collect();
        assert_eq!(held, [bars[999].time]);
    }
}

/// A history filter that needs metadata absent from every held order samples no column, and the
/// pane says why whatever the book profile shows. Two different failures each get one notice.
#[test]
fn heatmap_order_filters_without_their_order_data_say_so() {
    use crate::chart::overlays::OrderBookView;
    use crate::data::{DepthHistoryConfig, OrderFilter, RestingOrder};
    use crate::model::OrderSide::Buy;
    let time = bars(1)[0].time;
    let fresh = OrderFilter {
        max_age: Some(TimeDelta::seconds(10)),
        ..Default::default()
    };
    let others = OrderFilter {
        exclude_participants: vec![7],
        ..Default::default()
    };
    let all = OrderFilter::default();
    let bare = RestingOrder::new(1, Buy, 99.0, 5.0);
    let (placed, named) = (
        bare.clone().with_placed(time),
        bare.clone().with_participant(3),
    );
    let times = "Order-level depth unavailable: no order has a placement time";
    let participants = "Order-level depth unavailable: no order names its participant";
    // History filter, held order, profile filter when shown, notices and sampled columns.
    for (filter, order, profile, notices, columns) in [
        (&fresh, &bare, None, vec![times], 0),
        (&fresh, &bare, Some(&all), vec![times], 0),
        (&fresh, &placed, None, vec![], 1),
        (&fresh, &placed, Some(&all), vec![], 1),
        (&others, &bare, None, vec![participants], 0),
        (&others, &bare, Some(&all), vec![participants], 0),
        (&others, &named, None, vec![], 1),
        (&others, &named, Some(&all), vec![], 1),
        (&fresh, &bare, Some(&fresh), vec![times], 0),
        (&fresh, &bare, Some(&others), vec![times, participants], 0),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let config = DepthHistoryConfig {
            orders: Some(filter.clone()),
            ..Default::default()
        };
        chart.set_depth_history(Some(DepthHistory::new(config).unwrap()));
        chart.set_liquidity_heatmap(true);
        chart.set_order_book_visible(profile.is_some());
        chart.set_order_book_view(OrderBookView {
            filter: profile.cloned().unwrap_or_default(),
            ..Default::default()
        });
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(0.25)));
        feed.page().send(page(bars(1), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let sink = feed.sink();
        sink.set_server_time(time);
        sink.push_order_snapshot(1, time, vec![order.clone()]);
        // The next period samples the book as it stood.
        sink.advance_time(time + TimeDelta::seconds(1));
        frames.run(&mut chart);
        let mut output = frames.ctx.run_ui(Default::default(), |ui| {
            chart.paint_liquidity(ui.painter(), &chart.get_chart_mapping());
        });
        output.textures_delta.clear();
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, notices, "{filter:?} {order:?} {profile:?}");
        assert_eq!(
            chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .columns()
                .count(),
            columns,
            "{filter:?} {order:?}"
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
    for (name, offset, span, end, continuation, retention, expected) in [
        (
            "overlapping page",
            Some(899),
            120,
            false,
            None,
            None,
            vec![900],
        ),
        (
            "final page",
            Some(899),
            60,
            true,
            None,
            None,
            vec![899, 900],
        ),
        ("empty page", None, 60, false, None, None, vec![900]),
        ("repeated page", Some(900), 60, false, None, None, vec![900]),
        ("empty window", None, 60, false, Some(880), None, vec![900]),
        (
            "retention reached",
            Some(899),
            60,
            false,
            None,
            Some(120),
            vec![899, 900],
        ),
    ] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_depth_history(Some(
            DepthHistory::new(
                crate::data::DepthHistoryConfig::default()
                    .with_retention(retention.map(TimeDelta::seconds)),
            )
            .unwrap(),
        ));
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
        let since = retention.map(|_| bars[899].time);
        assert!(
            matches!(&calls[0], Call::Depth(r) if r.before == Some(bars[900].time) && r.count == 600 && r.since == since),
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

/// Each hours choice reloads history, paging and the subscription together. Undo restores
/// both flags, and layouts saved before overnight hours restore without them.
#[test]
fn trading_hours_reload_history_paging_and_subscription_together() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    let mut previous = None;
    type Choice = fn(&mut Chart);
    let rows: [(Choice, (bool, bool)); 6] = [
        (|chart| chart.set_extended_hours(false), (false, false)),
        (|chart| chart.set_extended_hours(true), (true, false)),
        (|chart| chart.set_overnight_hours(true), (true, true)),
        (|chart| chart.set_extended_hours(false), (false, false)),
        (|chart| chart.set_overnight_hours(true), (true, true)),
        (|chart| chart.set_overnight_hours(false), (true, false)),
    ];
    for (choose, hours) in rows {
        choose(&mut chart);
        assert_eq!((chart.extended_hours(), chart.overnight_hours()), hours);
        frames.run(&mut chart);
        let calls = feed.calls();
        if let Some(previous) = previous {
            assert!(calls.contains(&Call::Unsubscribe(previous)));
        }
        assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_none() && (request.extended_hours, request.overnight_hours) == hours)));
        feed.page().send(page(bars(1000), false));
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let id = feed.sink().id;
        let mut calls = feed.calls();
        assert!(calls.contains(&Call::Subscribe(id, "A".into(), Timeframe::Min1, hours)));
        choose(&mut chart);
        frames.run(&mut chart);
        assert!(
            feed.calls().is_empty(),
            "an unchanged choice must keep the stream"
        );
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        calls.extend(feed.calls());
        assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_some() && (request.extended_hours, request.overnight_hours) == hours)), "{hours:?}: {calls:?}");
        feed.page().send(page(vec![], true));
        frames.run(&mut chart);
        previous = Some(id);
    }
    chart.user_action(|chart| chart.set_overnight_hours(true));
    assert!(chart.undo());
    assert_eq!(
        (chart.extended_hours(), chart.overnight_hours()),
        (true, false)
    );
    assert!(chart.redo());
    assert_eq!(
        (chart.extended_hours(), chart.overnight_hours()),
        (true, true)
    );
    #[cfg(feature = "ui")]
    {
        let saved = chart.snapshot().unwrap();
        let mut older = serde_json::to_value(&saved).unwrap();
        assert!(
            older
                .as_object_mut()
                .unwrap()
                .remove("overnight_hours")
                .is_some()
        );
        // These colours did not exist in older configs or palettes either.
        for field in ["config", "palette"] {
            assert!(
                older[field]
                    .as_object_mut()
                    .unwrap()
                    .remove("overnight_hours_color")
                    .is_some()
            );
        }
        let older = serde_json::from_value(older).unwrap();
        // A new chart shades overnight bars #D500F9 at 8% opacity, alpha 20, premultiplied; a
        // config saved without the colour follows the theme.
        let purple = egui::Color32::from_rgba_premultiplied(17, 0, 20, 20);
        for (snapshot, hours, color) in [
            (saved, (true, true), purple),
            (older, (true, false), egui::Color32::PLACEHOLDER),
        ] {
            let mut restored = fed(&Arc::new(Script::default()), Some("A"));
            restored.restore(&snapshot).unwrap();
            assert_eq!(
                (restored.extended_hours(), restored.overnight_hours()),
                hours
            );
            assert_eq!(restored.config().overnight_hours_color, color);
        }
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

/// A resolve answer belongs to the requested symbol even under another name. Its trading hours
/// reload once with delayed replies, and a pause keeps the bars and prepared study inputs.
#[test]
fn a_resolved_instrument_with_another_symbol_reloads_once_and_resumes() {
    use crate::model::{Session, SessionWindow};
    use chrono::{NaiveTime, Weekday};
    let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    // The Friday bars at 00:00 through 00:04 UTC fall inside these weekday hours.
    let weekdays = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ];
    let hours = Session::new([SessionWindow::new(weekdays, at(0), at(12))]);
    let instrument = Instrument::new("X:A")
        .with_session(hours)
        .with_tick_size(1.0);
    let feed = Arc::new(Script::default());
    feed.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    for frame in 0..10 {
        // A network feed answers in a later frame.
        for reply in std::mem::take(&mut *feed.resolves.lock().unwrap()) {
            reply.send(Ok(instrument.clone()));
        }
        frames.run(&mut chart);
        let symbol = chart.instrument().map(|i| i.symbol.as_str());
        assert_eq!(symbol, Some("X:A"), "frame {frame}");
    }
    let calls = feed.calls();
    let resolves = calls.iter().filter(|call| matches!(call, Call::Resolve(_)));
    let latest = calls
        .iter()
        .filter(|call| matches!(call, Call::History(request) if request.before.is_none()));
    assert!(resolves.count() <= 2 && latest.count() <= 2, "{calls:?}");

    let reply = feed.pages.lock().unwrap().pop().unwrap();
    reply.send(page(bars(5), true));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN), bars(5));
    #[cfg(feature = "studies")]
    let input = {
        let id = chart.add_indicator(
            crate::studies::IndicatorFactory::new()
                .create("Vol Profile FR")
                .unwrap(),
        );
        frames.run(&mut chart);
        let start = bars(1)[0].time;
        feed.footprints
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .send(Ok(prepared_page(
                &[Tick::new(start, 100.0, 5.0, None)],
                start + TimeDelta::minutes(1),
                true,
            )));
        frames.run(&mut chart);
        chart
            .indicators()
            .find(|(seen, _)| *seen == id)
            .unwrap()
            .1
            .order_flow_inputs()[0]
            .clone()
    };
    #[cfg(feature = "studies")]
    let reset = input.reset_revision();
    chart.set_feed_paused(true);
    chart.set_feed_paused(false);
    frames.run(&mut chart);
    // A resume keeps the bars but the last, which the next page rebuilds.
    assert_eq!(chart.bars(SeriesId::MAIN), &bars(5)[..4]);
    #[cfg(feature = "studies")]
    assert_eq!(
        input.reset_revision(),
        reset,
        "a resume keeps the study input"
    );
    #[cfg(feature = "studies")]
    assert_eq!(
        input
            .footprint_snapshot(bars(1)[0].time)
            .bars(..)
            .map(|bar| bar.volume())
            .collect::<Vec<_>>(),
        [5.0],
        "a resume keeps the prepared study input"
    );
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
        // An explicit instant replaces local navigation's following-bar rule.
        chart.go_to_local_time(all[0].time.date_naive(), None);
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
        // A range synchronised from another chart keeps the target, so a linked chart that
        // follows the active one still pages towards its own date.
        assert!(chart.apply_synced_time_range(chart.visible_time_range().unwrap()));
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

/// With the choice on, an interval change keeps the bar at the left edge in place at the same
/// spacing once the new bars replace the old ones: the bar whose period holds its open, also when
/// a feed pages older history to reach it. With it off the view stays where it was.
#[test]
fn interval_change_keeps_the_left_edge_when_chosen() {
    let all = bars(3000);
    let five: Vec<Bar> = all.iter().step_by(5).cloned().collect();
    let keep = |chart: &mut Chart| {
        let mut options = chart.chart_options().clone();
        options.time_scale.keep_left_edge_on_interval_change = true;
        chart.set_chart_options(options);
    };
    // The left pixel cuts the 16:43 bar a quarter of a slot before its centre.
    let shown = crate::model::LogicalRange::new(1003.25, 1102.25);
    let check = |chart: &Chart, left: f32, open: &str, row: &str| {
        let range = chart.visible_logical_range().unwrap();
        assert!(
            (range.left - left).abs() < 0.01 && (range.right - left - 99.0).abs() < 0.01,
            "{row}: {range:?}"
        );
        let time = chart.time_at_index(range.left.floor() as usize);
        assert_eq!(time, Some(open.parse().unwrap()), "{row}");
    };
    // Off, the bar at the right edge keeps its place. On, 16:40 holds 16:43; the nearest bar
    // would be 16:45. A host may supply the new bars a frame later.
    for (on, wait, left, open) in [
        (false, false, 121.25, "2024-03-15T10:05:00Z"),
        (true, false, 200.25, "2024-03-15T16:40:00Z"),
        (true, true, 200.25, "2024-03-15T16:40:00Z"),
    ] {
        let mut chart = Chart::new(BarData::from_bars(all.clone()));
        let mut frames = Frames::new();
        chart.set_timeframe(Timeframe::Min1);
        if on {
            keep(&mut chart);
        }
        frames.run(&mut chart);
        chart.set_visible_logical_range(shown);
        frames.run(&mut chart);
        let spacing = chart.state.time_scale().bar_spacing();
        chart.set_timeframe(Timeframe::Min5);
        if wait {
            frames.run(&mut chart);
        }
        chart.set_data(SeriesId::MAIN, five.clone());
        frames.run(&mut chart);
        frames.run(&mut chart);
        check(&chart, left, open, &format!("{on} {wait}"));
        assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
    }

    // The 16:40 bar opens before the kept open, so older pages are asked by count alone.
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    chart.set_timeframe(Timeframe::Min1);
    keep(&mut chart);
    frames.run(&mut chart);
    feed.page().send(page(all.clone(), false));
    frames.run(&mut chart);
    frames.run(&mut chart);
    chart.set_visible_logical_range(shown);
    frames.run(&mut chart);
    feed.calls();
    chart.set_timeframe(Timeframe::Min5);
    frames.run(&mut chart);
    feed.calls();
    feed.page().send(page(five[400..].to_vec(), false));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(
        feed.calls(),
        [history("A", Timeframe::Min5, Some(five[400].time))]
    );
    feed.page().send(page(five[..400].to_vec(), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    check(&chart, 200.25, "2024-03-15T16:40:00Z", "feed");
    assert!(feed.calls().is_empty());
}

/// Local navigation keeps the zoom and selects the following bar, including after paging.
/// New York is UTC-5 in November and UTC-4 after the March clock jump.
#[cfg(feature = "timezones")]
#[test]
fn go_to_local_time_centres_the_first_bar_at_or_after_the_local_time() {
    use crate::model::TimezoneId;
    use crate::scales::TimezoneMode;
    use chrono::{NaiveDate, NaiveTime};

    let ny = TimezoneId::named("America/New_York").unwrap();
    let series = |start: &str, count: i64, minutes: i64| {
        let start: DateTime<Utc> = start.parse().unwrap();
        (0..count)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(i * minutes),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    1.0,
                )
            })
            .collect::<Vec<_>>()
    };
    let sessions = [
        series("2026-11-16T14:30:00Z", 13, 30),
        series("2026-11-17T14:30:00Z", 13, 30),
    ]
    .concat();
    let skipped = series("2026-03-08T04:00:00Z", 12, 30);
    let repeated = series("2026-11-01T04:00:00Z", 12, 30);
    let daily = series("2026-11-16T00:00:00Z", 5, 1440);
    // Midnight EST is 05:00Z, nearer the previous session's 20:30Z than today's 14:30Z.
    // 02:30 in the spring gap resolves to 03:00 EDT = 07:00Z; the first autumn 01:30
    // is still EDT = 05:30Z. Supplied daily bars retain the UTC trading date on their axis.
    for (name, bars, interval, date, time, expected) in [
        (
            "day in the bars",
            &sessions,
            Timeframe::Min30,
            "2026-11-17",
            None,
            "2026-11-17T14:30:00Z",
        ),
        (
            "time on that day",
            &sessions,
            Timeframe::Min30,
            "2026-11-17",
            Some("14:30"),
            "2026-11-17T19:30:00Z",
        ),
        (
            "skipped hour",
            &skipped,
            Timeframe::Min30,
            "2026-03-08",
            Some("02:30"),
            "2026-03-08T07:00:00Z",
        ),
        (
            "repeated hour",
            &repeated,
            Timeframe::Min30,
            "2026-11-01",
            Some("01:30"),
            "2026-11-01T05:30:00Z",
        ),
        (
            "after the last bar",
            &sessions,
            Timeframe::Min30,
            "2026-11-20",
            None,
            "2026-11-17T20:30:00Z",
        ),
        (
            "before supplied bars",
            &sessions,
            Timeframe::Min30,
            "2026-11-15",
            None,
            "2026-11-16T14:30:00Z",
        ),
        (
            "supplied daily bars",
            &daily,
            Timeframe::Day1,
            "2026-11-17",
            None,
            "2026-11-17T00:00:00Z",
        ),
    ] {
        let expected: DateTime<Utc> = expected.parse().unwrap();
        let expected = bars.iter().position(|bar| bar.time == expected).unwrap();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_timeframe(interval);
        chart.chart_options.time_scale.timezone = TimezoneMode::Timezone(ny.clone());
        chart.set_visible_bars(4);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        let spacing = chart.state.time_scale().bar_spacing();
        chart.enable_tracking_mode();
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        let time = time.map(|text| NaiveTime::parse_from_str(text, "%H:%M").unwrap());
        chart.go_to_local_time(date, time);
        frames.run(&mut chart);
        let range = chart.state.time_scale().visible_logical_range();
        assert!(
            ((range.left + range.right) / 2.0 - expected as f32).abs() < 0.01,
            "{name}: {range:?}"
        );
        assert_eq!(chart.state.time_scale().bar_spacing(), spacing, "{name}");
        assert!(!chart.is_tracking_mode_active(), "{name}");
    }

    let all = bars(3000);
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.chart_options.time_scale.timezone = TimezoneMode::Timezone(ny.clone());
    chart.set_visible_bars(4);
    let mut frames = Frames::new();
    // Before even the first page arrives the target remains pending. 10:00 EDT is 14:00Z,
    // which is 14 * 60 = 840 minutes from the fixture's midnight on 15 March 2024.
    chart.go_to_local_time(
        NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(),
        NaiveTime::from_hms_opt(10, 0, 0),
    );
    frames.run(&mut chart);
    feed.calls();
    feed.page().send(page(all[2000..].to_vec(), false));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let ranged = |before| {
        let Call::History(mut request) = history("A", Timeframe::Min1, Some(before)) else {
            unreachable!()
        };
        request.clock = request.clock.with_timezone(ny.clone());
        request.since = Some(all[840].time);
        Call::History(request)
    };
    assert_eq!(feed.calls(), [ranged(all[2000].time)]);
    feed.page().send(page(all[1000..2000].to_vec(), false));
    frames.run(&mut chart);
    frames.run(&mut chart);
    assert_eq!(feed.calls(), [ranged(all[1000].time)]);
    feed.page().send(page(all[..1000].to_vec(), false));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let range = chart.state.time_scale().visible_logical_range();
    assert!(
        ((range.left + range.right) / 2.0 - 840.0).abs() < 0.01,
        "history: {range:?}"
    );
    assert!(feed.calls().is_empty());
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
    let subscribe = Call::Subscribe(id, "A".into(), Timeframe::Min1, (false, false));
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
    chart.set_volume_bubbles(Some(VolumeBubblesConfig {
        grouping: crate::chart::overlays::BubbleGrouping::Automatic,
        ..VolumeBubblesConfig::default()
    }));
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
    let row = footprint.and_then(|flow| flow.view().bar_at(forming.time)?.levels.get(&14));
    assert_eq!(
        row.map(|row| row.volume()),
        Some(1000.0),
        "c: and the footprint"
    );
    let frozen = chart.feed_link.as_ref().unwrap().frozen();
    let row = frozen
        .footprint
        .as_ref()
        .and_then(|flow| flow.view().bar_at(forming.time)?.levels.get(&14));
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
    {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        WARNINGS.set(Some(Vec::new()));
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), true));
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        assert!(
            WARNINGS.take().unwrap().is_empty(),
            "unsupported footprint history is normal"
        );
    }
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
        None,
        "a new rate leaves out bar 0, which lost an execution to the store's limit"
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
        .order_flow_inputs()[0]
        .clone();
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
/// keep their values and no order-flow study blanks for a frame, nor while it waits for its full
/// recalculation after a drop.
/// Hand values: one buy of 1 in each of 11,000 one-minute bars. Each input holds all 11,000,
/// so the CVD of bar 10,999 is 11,000 and a 10-bar flow sum is 10. Bar 11,000 is the 11,001st:
/// the input drops its oldest 1,001 and the CVD starts again at bar 1,001, 9,999 at bar 10,999.
/// On the first frame after the drop one study recalculates and the other keeps its 11,000
/// earlier values, 11,000 or 10 at bar 10,999, until the next frame. Bar 11,001 crosses no
/// boundary, so both studies update on its first frame and bar 10,999 keeps 9,999 and 10.
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
    // Each frame comes past the refresh waits so far, which follow the real time a recalculation
    // took, so the first frame after the drop finds both studies due on any machine.
    for _ in 0..30 {
        frames.run(&mut chart);
        frames.time = chart
            .next_refresh
            .iter()
            .copied()
            .fold(frames.time, f64::max);
    }
    let before = record(&chart);
    feed.sink().push_tick(buy(&all[11_000]));
    frames.run(&mut chart);
    let waiting = record(&chart);
    for _ in 0..9 {
        frames.run(&mut chart);
    }
    let dropped = record(&chart);
    // One study recalculates in full on the first frame after the drop; the other keeps its
    // values, without one for the new bar, until its turn.
    assert!(
        [
            [(11_001, Some(9_999.0)), (11_000, Some(10.0))],
            [(11_000, Some(11_000.0)), (11_001, Some(10.0))],
        ]
        .contains(&waiting),
        "{waiting:?}"
    );
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

/// The bar containing a replay cutoff sums only the executions before the cutoff, and only while
/// the trade store holds every one of that bar's; leaving replay restores the whole bar.
/// Hand values, all sizes in the first default range: bar 0 is +2, bar 1 is +3 - 5 + 7 = 5, and
/// before 00:01:30 bar 1 is +3 - 5 = -2. A store of two drops 00:00:10, then 00:01:10, which
/// leaves bar 1 short.
#[cfg(feature = "studies")]
#[test]
fn order_flow_studies_stop_at_the_replay_cutoff() {
    use crate::model::OrderSide::{Buy, Sell};
    let start = bars(1)[0].time;
    let at = |second| start + TimeDelta::seconds(second);
    for (capacity, before_cutoff) in [(TRADE_CAPACITY, Some(-2.0)), (2, None)] {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let factory = crate::studies::IndicatorFactory::new();
        let id = chart.add_indicator(factory.create("Multi-Range Delta").unwrap());
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(2), true));
        frames.run(&mut chart);
        chart.feed_link.as_mut().unwrap().trades = Trades::new(capacity);
        let sink = feed.sink();
        for (second, size, side) in [
            (10, 2.0, Buy),
            (70, 3.0, Buy),
            (80, 5.0, Sell),
            (100, 7.0, Buy),
        ] {
            sink.push_tick(Tick::new(at(second), 100.0, size, Some(side)));
        }
        for _ in 0..8 {
            frames.run(&mut chart);
        }
        let delta = |chart: &Chart, bar| {
            let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
            study.values().value(0, bar)
        };
        assert_eq!(
            (delta(&chart, 0), delta(&chart, 1)),
            (Some(2.0), Some(5.0)),
            "live, store of {capacity}"
        );
        // Kept paused through replay, so leaving it does not reload the trades.
        chart.set_feed_paused(true);
        chart.set_replay_time(Some(at(90)));
        assert!(chart.set_replay_source(
            vec![Bar::new(at(70), 100.0, 100.0, 100.0, 100.0, 3.0)],
            Timeframe::Sec1.into()
        ));
        for _ in 0..4 {
            frames.run(&mut chart);
        }
        assert_eq!(chart.state.data().bars.len(), 2);
        assert_eq!(
            (delta(&chart, 0), delta(&chart, 1)),
            (Some(2.0), before_cutoff),
            "replay, store of {capacity}"
        );
        chart.set_replay_time(None);
        for _ in 0..4 {
            frames.run(&mut chart);
        }
        assert_eq!(
            delta(&chart, 1),
            Some(5.0),
            "after replay, store of {capacity}"
        );
    }
}

/// A host study on a footprint and a book source: each bar's most traded price and best bid size.
#[derive(Clone)]
struct PocAndBid {
    inputs: [crate::data::OrderFlowInput; 2],
    clock: BarClock,
    values: crate::studies::IndicatorValues,
}

impl crate::studies::Indicator for PocAndBid {
    fn name(&self) -> &str {
        "POC and bid"
    }
    fn order_flow_inputs(&self) -> &[crate::data::OrderFlowInput] {
        &self.inputs
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        let flow = self.inputs[0].footprints_from(DateTime::<Utc>::MIN_UTC);
        let books = self.inputs[1].books_from(DateTime::<Utc>::MIN_UTC);
        self.values.clear();
        for bar in data {
            let open = self.clock.bar_open(bar.time);
            let poc = (flow.bars.get(&open).and_then(|bar| bar.poc()))
                .zip(flow.tick_size)
                .map_or(f64::NAN, |(level, tick)| level as f64 * tick);
            let bid = (books.bars.get(&open).and_then(|book| book.bids.first()))
                .map_or(f64::NAN, |level| level.size);
            self.values.push_lines(&[poc, bid]);
        }
    }
    fn values(&self) -> &crate::studies::IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<egui::Color32> {
        Vec::new()
    }
    fn set_colors(&mut self, _: Vec<egui::Color32>) {}
    fn set_visible(&mut self, _: bool) {}
    fn clone_box(&self) -> Box<dyn crate::studies::Indicator> {
        Box::new(self.clone())
    }
}

/// A host study reads a footprint and a book through the chart's feed, at the instrument's tick
/// of 0.25, live and as of each replay cutoff. Hand values: bar 0 trades 3 at 100.00 and 1 at
/// 100.25, POC 100.00, and its last book (00:40) bids 9; bar 1 trades 3 at 100.00 and 6 at
/// 100.25, POC 100.25, last book (01:25) bid 12. Before 01:20 bar 1 traded only 3 at 100.00 and
/// its last book came later; before 01:30 it traded 3 at 100.00 and 1 at 100.25 and the book of
/// 01:25 counts.
#[test]
fn host_studies_read_footprints_and_books_as_of_the_replay_cutoff() {
    use crate::data::OrderFlowInput;
    use crate::model::OrderBookLevel;
    use crate::model::OrderSide::{Buy, Sell};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let id = chart.add_indicator(Box::new(PocAndBid {
        inputs: [OrderFlowInput::footprint(), OrderFlowInput::book(1)],
        clock: BarClock::exact(),
        values: crate::studies::IndicatorValues::new(),
    }));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(0.25)));
    feed.page().send(page(bars(3), true));
    frames.run(&mut chart);
    let sink = feed.sink();
    let start = bars(1)[0].time;
    let at = |second| start + TimeDelta::seconds(second);
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
    for (sequence, second, bid, ask) in [
        (1, 5, (99.75, 4.0), (100.25, 6.0)),
        (2, 40, (100.0, 9.0), (100.5, 2.0)),
        (3, 75, (100.0, 8.0), (100.25, 3.0)),
        (4, 85, (100.0, 12.0), (100.25, 1.0)),
    ] {
        sink.push_depth_snapshot(
            sequence,
            book(second, vec![level(bid.0, bid.1)], vec![level(ask.0, ask.1)]),
        );
    }
    for (second, price, size, side) in [
        (10, 100.0, 2.0, Buy),
        (20, 100.25, 1.0, Sell),
        (50, 100.0, 1.0, Buy),
        (70, 100.0, 3.0, Buy),
        (80, 100.25, 1.0, Sell),
        (95, 100.25, 5.0, Buy),
    ] {
        sink.push_tick(Tick::new(at(second), price, size, Some(side)));
    }
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    let lines = |chart: &Chart| {
        let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
        (0..chart.state.data().bars.len())
            .map(|bar| [study.values().value(0, bar), study.values().value(1, bar)])
            .collect::<Vec<_>>()
    };
    let live = vec![
        [Some(100.0), Some(9.0)],
        [Some(100.25), Some(12.0)],
        [None, None],
    ];
    assert_eq!(lines(&chart), live, "live");
    chart.set_feed_paused(true);
    chart.set_replay_time(Some(at(80)));
    assert!(chart.set_replay_source(
        vec![Bar::new(at(70), 100.0, 100.0, 100.0, 100.0, 3.0)],
        Timeframe::Sec1.into()
    ));
    for _ in 0..4 {
        frames.run(&mut chart);
    }
    assert_eq!(
        lines(&chart),
        [[Some(100.0), Some(9.0)], [Some(100.0), None]],
        "at 00:01:20 bar 1 has traded 3 at 100.00 and its last book came later"
    );
    chart.set_replay_time(Some(at(90)));
    // A study without incremental updates recalculates from a later bar at most every 0.25 s.
    for _ in 0..20 {
        frames.run(&mut chart);
    }
    assert_eq!(
        lines(&chart),
        [[Some(100.0), Some(9.0)], [Some(100.0), Some(12.0)]],
        "at 00:01:30 the book of 00:01:25 counts"
    );
    chart.set_replay_time(None);
    for _ in 0..20 {
        frames.run(&mut chart);
    }
    assert_eq!(lines(&chart), live, "after replay");
}

/// A drawing's actual frame view reads shared prepared rows and keeps incomplete joins marked.
#[cfg(feature = "drawings")]
#[test]
fn profile_drawings_read_shared_prepared_rows() {
    use crate::data::{
        FootprintBar, FootprintBarView, FootprintLevel, FootprintRows, FootprintSource,
        ProfileSource, VolumeProfile,
    };
    use crate::drawings::rendering::ChartView;
    use crate::drawings::{
        ChartPoint, Drawing, DrawingRenderer, DrawingToolType, RegisteredDrawingTool,
    };
    use crate::model::OrderSide::{Buy, Sell};

    struct Rows(Vec<(FootprintBar, Vec<FootprintLevel>)>);
    impl FootprintSource for Rows {
        fn len(&self) -> usize {
            self.0.len()
        }
        fn bar(&self, index: usize) -> FootprintBarView<'_> {
            let (bar, rows) = &self.0[index];
            FootprintBarView {
                levels: FootprintRows::Slice(rows),
                ..bar.into()
            }
        }
    }
    struct Capture(Arc<Mutex<Option<VolumeProfile>>>);
    impl DrawingRenderer for Capture {
        fn render(&self, drawing: &Drawing, _: &egui::Painter, _: egui::Rect, view: &ChartView) {
            *self.0.lock().unwrap() = drawing.volume_profile_of(view);
        }
        fn hit_test(&self, _: &Drawing, _: egui::Pos2, _: f32) -> bool {
            false
        }
    }

    for shared in [false, true] {
        for gap in [false, true] {
            let (feed, mut chart, mut frames) = prepared_chart();
            let start = bars(1)[0].time;
            let at = |second| start + TimeDelta::seconds(second);
            let mut page = prepared_page(
                &[
                    Tick::new(at(0), 100.0, 2.0, Some(Buy)),
                    Tick::new(at(60), 101.0, 3.0, Some(Sell)),
                    Tick::new(at(120), 102.0, 5.0, None),
                ],
                at(150),
                true,
            );
            if shared {
                let crate::data::FootprintBars::Owned(bars) = page.bars else {
                    unreachable!()
                };
                let rows = bars
                    .into_iter()
                    .map(|mut bar| {
                        let rows = std::mem::take(&mut bar.levels).into_values().collect();
                        (bar, rows)
                    })
                    .collect();
                page.bars = crate::data::FootprintBars::Shared(Arc::new(Rows(rows)));
            }
            feed.footprints.lock().unwrap().remove(0).send(Ok(page));
            frames.run(&mut chart);
            let live = Tick::new(
                at(if gap { 180 } else { 150 }),
                if gap { 103.0 } else { 102.0 },
                7.0,
                Some(Buy),
            )
            .with_id(1);
            feed.sink().push_tick(live.clone());
            frames.run(&mut chart);
            // Confirm the first live timestamp; the partial page leaves the earlier join unknown.
            feed.trades
                .lock()
                .unwrap()
                .remove(0)
                .send(Ok(TradePage::new(vec![live], !gap)));
            frames.run(&mut chart);
            let captured = Arc::new(Mutex::new(None));
            chart
                .drawings_mut()
                .registry
                .register(
                    "capture",
                    RegisteredDrawingTool::new("Capture", 2, Arc::new(Capture(captured.clone()))),
                )
                .unwrap();
            let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
            drawing.custom_tool = Some("capture".into());
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(1.0, 100.0)];
            chart.drawings_mut().drawings.push(drawing);
            for kind in [ChartType::VolumeFootprint, ChartType::Candles] {
                chart.set_chart_type(kind);
                chart.drawings_mut().drawings[0].tool_type =
                    DrawingToolType::FixedRangeVolumeProfile;
                for (first, last, complete, volume, up, down) in [
                    (
                        0.0,
                        1.0,
                        true,
                        vec![2.0, 3.0],
                        vec![2.0, 0.0],
                        vec![0.0, 3.0],
                    ),
                    (
                        0.0,
                        3.0,
                        !gap,
                        if gap {
                            vec![2.0, 3.0, 0.0, 7.0]
                        } else {
                            vec![2.0, 3.0, 12.0]
                        },
                        if gap {
                            vec![2.0, 0.0, 0.0, 7.0]
                        } else {
                            vec![2.0, 0.0, 7.0]
                        },
                        if gap {
                            vec![0.0, 3.0, 0.0, 0.0]
                        } else {
                            vec![0.0, 3.0, 0.0]
                        },
                    ),
                    (1.0, 1.0, true, vec![3.0], vec![0.0], vec![3.0]),
                ] {
                    chart.drawings_mut().drawings[0].chart_points =
                        vec![ChartPoint::new(first, 100.0), ChartPoint::new(last, 100.0)];
                    *captured.lock().unwrap() = None;
                    frames.run(&mut chart);
                    let got = captured
                        .lock()
                        .unwrap()
                        .take()
                        .expect("drawing callback ran");
                    assert_eq!(
                        got.source(),
                        ProfileSource::Executions { complete },
                        "shared={shared} gap={gap} {kind:?} {first}..={last}"
                    );
                    assert_eq!(got.volume(), volume);
                    assert_eq!(got.up(), up);
                    assert_eq!(got.down(), down);
                }
                let drawing = &mut chart.drawings_mut().drawings[0];
                drawing.tool_type = DrawingToolType::AnchoredVolumeProfile;
                drawing.chart_points = vec![ChartPoint::new(1.0, 100.0)];
                frames.run(&mut chart);
                let got = captured.lock().unwrap().take().unwrap();
                assert_eq!(got.source(), ProfileSource::Executions { complete: !gap });
                assert_eq!(
                    got.volume(),
                    if gap {
                        vec![3.0, 0.0, 7.0]
                    } else {
                        vec![3.0, 12.0]
                    }
                );
            }
            let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
            feed.sink()
                .push_tick(Tick::new(at(241), 102.0, 8.0, None).with_id(2));
            frames.run(&mut chart);
            let live = captured.lock().unwrap().take().unwrap();
            assert_eq!(
                live.volume().iter().sum::<f64>(),
                if gap { 18.0 } else { 23.0 }
            );
            frames.run(&mut frozen);
            let saved = captured.lock().unwrap().take().unwrap();
            assert_eq!(
                saved.volume(),
                if gap {
                    vec![3.0, 0.0, 7.0]
                } else {
                    vec![3.0, 12.0]
                }
            );
        }
    }
}

/// Minute bars with held trades in the last historical and the forming bar.
#[cfg(feature = "drawings")]
fn traded_minute_bars() -> (Arc<Script>, Chart, Frames, DateTime<Utc>) {
    use crate::model::OrderSide::{Buy, Sell};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(5), false));
    frames.run(&mut chart);
    // Bar 4 (00:04) trades 100 (4 sold) and 101 (6 bought); bar 5 (00:05) trades 101 (3 bought).
    // An empty ending page confirms that the first held execution's bar is complete.
    feed.trades
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(Ok(TradePage::new(Vec::new(), true)));
    let start = bars(5)[4].time;
    let sink = feed.sink();
    for (seconds, price, size, side) in [
        (10, 100.0, 4.0, Sell),
        (20, 101.0, 6.0, Buy),
        (70, 101.0, 3.0, Buy),
    ] {
        sink.push_tick(Tick::new(
            start + TimeDelta::seconds(seconds),
            price,
            size,
            Some(side),
        ));
    }
    (feed, chart, frames, start)
}

/// Profile drawings use held executions on time-based charts and release them when hidden.
#[cfg(feature = "drawings")]
#[test]
fn profile_drawings_read_executions_on_every_time_based_chart_type() {
    use crate::data::ProfileSource;
    use crate::drawings::rendering::ChartView;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    let (_, mut chart, mut frames, start) = traded_minute_bars();
    let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
    drawing.chart_points = vec![ChartPoint::new(4.0, 100.0), ChartPoint::new(5.0, 100.0)];
    chart.drawings_mut().drawings.push(drawing.clone());
    let profile = |chart: &Chart, drawing: &Drawing| {
        let view = ChartView {
            instrument: None,
            price_formatter: None,
            thousands_sep: None,
            time_formatter: None,
            bar_seconds: 0,
            bars: chart.state.shown(),
            revision: chart.state.revision(),
            bar_to_x: &|x| x,
            bar_spacing: 1.0,
            price_to_y: &|price| price as f32,
            y_to_price: &f64::from,
            time_axis: true,
            executions: chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .map(|flow| flow.view()),
            timeframe: None,
            finer: &[],
        };
        drawing.volume_profile_of(&view)
    };
    let mut footprint = None;
    for (chart_type, executed) in [
        (ChartType::VolumeFootprint, true),
        (ChartType::Candles, true),
        (ChartType::Line, true),
        (ChartType::Heikin, true),
        (ChartType::Renko, false),
    ] {
        chart.set_chart_type(chart_type);
        frames.run(&mut chart);
        let held = chart.feed_link.as_ref().unwrap().footprint.is_some();
        assert_eq!(held, executed, "{chart_type:?}");
        if !executed {
            continue;
        }
        let profile = profile(&chart, &drawing).unwrap();
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: true },
            "{chart_type:?}"
        );
        match &footprint {
            None => {
                assert_eq!((profile.low(), profile.high()), (99.5, 101.5));
                assert_eq!(profile.volume(), [4.0, 9.0]);
                assert_eq!(
                    (profile.up(), profile.down()),
                    ([0.0, 9.0].as_slice(), [4.0, 0.0].as_slice())
                );
                footprint = Some(profile);
            }
            Some(reference) => assert_eq!(&profile, reference, "{chart_type:?}"),
        }
    }
    chart.set_chart_type(ChartType::Candles);
    frames.run(&mut chart);
    // Holding executions for a drawing must keep the selected candle chart's price series.
    // Wide bars make footprint cells readable, so accidentally enabling that renderer is visible.
    chart.apply_synced_time_scale(80.0, 0.0);
    let colors = [chart.config().bullish_color, chart.config().bearish_color];
    chart.drawings_mut().drawings[0].visible = false;
    frames.time += 1.0;
    let without = shapes(&frames.ctx, &mut chart, frames.time, Vec::new());
    let reference = colors.map(|color| candle_ys(&without, color));
    chart.drawings_mut().drawings[0].visible = true;
    frames.time += 1.0;
    let with = shapes(&frames.ctx, &mut chart, frames.time, Vec::new());
    assert_eq!(
        colors.map(|color| candle_ys(&with, color)),
        reference,
        "price candles with a profile drawing"
    );
    let mut anchored = drawing.clone();
    anchored.tool_type = DrawingToolType::AnchoredVolumeProfile;
    anchored.chart_points.truncate(1);
    assert_eq!(profile(&chart, &anchored), footprint);

    // Replay inside a bar excludes its execution at the cutoff, including after a rewind.
    chart.set_feed_paused(true);
    let source = [(10, 100.0, 4.0), (20, 101.0, 6.0), (70, 101.0, 3.0)]
        .into_iter()
        .map(|(second, price, volume)| {
            Bar::new(
                start + TimeDelta::seconds(second),
                price,
                price,
                price,
                price,
                volume,
            )
        })
        .collect::<Vec<_>>();
    for (second, expected_high) in [(20, 100.5), (71, 101.5), (20, 100.5)] {
        chart.set_replay_time(Some(start + TimeDelta::seconds(second)));
        assert!(chart.set_replay_source(
            source.clone(),
            BarClock::new(Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()))
        ));
        let color = chart.drawings().drawings[0].color;
        let tint = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 15);
        frames.time += 1.0 / 60.0;
        let painted = shapes(&frames.ctx, &mut chart, frames.time, Vec::new());
        let points: Vec<_> = painted
            .iter()
            .flat_map(|shape| match shape {
                egui::Shape::Rect(rect) if rect.fill == tint => vec![rect.rect.min, rect.rect.max],
                egui::Shape::Mesh(mesh) => mesh
                    .vertices
                    .iter()
                    .filter(|v| v.color == tint)
                    .map(|v| v.pos)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        assert!(!points.is_empty(), "profile tint");
        let range = egui::Rect::from_points(&points);
        let map = chart.get_chart_mapping();
        // Tessellation snaps the tint to pixels and feathers its edges.
        assert!(
            (range.top() - map.price_to_y(expected_high)).abs() < 1.1,
            "cutoff {second}: tint {range:?}, expected top {}, bottom {}",
            map.price_to_y(expected_high),
            map.price_to_y(99.5)
        );
        assert!(
            (range.bottom() - map.price_to_y(99.5)).abs() < 1.1,
            "cutoff {second}: tint {range:?}, expected top {}, bottom {}",
            map.price_to_y(expected_high),
            map.price_to_y(99.5)
        );
    }
    chart.set_replay_time(None);
    chart.drawings_mut().drawings[0].visible = false;
    frames.run(&mut chart);
    assert!(chart.feed_link.as_ref().unwrap().footprint.is_none());
    chart.drawings_mut().drawings[0].visible = true;
    frames.run(&mut chart);
    assert!(chart.feed_link.as_ref().unwrap().footprint.is_some());
    chart.drawings_mut().drawings.clear();
    frames.run(&mut chart);
    assert!(chart.feed_link.as_ref().unwrap().footprint.is_none());
}

/// The execution store follows the drawing's setting, and alerts follow forming executions.
#[cfg(feature = "drawings")]
#[test]
fn anchored_vwap_reads_executions_while_it_counts_them() {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType, VwapConfig};
    let (feed, mut chart, mut frames, start) = traded_minute_bars();
    let mut drawing = Drawing::new(2, DrawingToolType::AnchoredVWAP);
    drawing.completed = true;
    drawing.chart_points = vec![ChartPoint::new(4.0, 100.0)];
    let source = drawing.alert_source(0);
    chart.drawings_mut().drawings.push(drawing);
    frames.run(&mut chart);
    assert!(
        chart.feed_link.as_ref().unwrap().footprint.is_none(),
        "bar source"
    );
    let counted = |chart: &mut Chart, executions| {
        chart.drawings_mut().drawings[0].vwap = Some(VwapConfig {
            executions,
            ..VwapConfig::default()
        });
    };
    counted(&mut chart, true);
    frames.run(&mut chart);
    assert!(
        chart.feed_link.as_ref().unwrap().footprint.is_some(),
        "executions"
    );
    for (bar, expected) in [(4, 100.6), (5, 1309.0 / 13.0)] {
        let got = chart.alert_value(&source, bar).unwrap();
        assert!(
            (got - expected).abs() < 1e-9,
            "bar {bar}: {got}, want {expected}"
        );
    }
    feed.sink().push_tick(Tick::new(
        start + TimeDelta::seconds(80),
        104.0,
        7.0,
        Some(crate::model::OrderSide::Buy),
    ));
    frames.run(&mut chart);
    let got = chart.alert_value(&source, 5).unwrap();
    assert!((got - 101.85).abs() < 1e-9, "forming bar: {got}");
    counted(&mut chart, false);
    frames.run(&mut chart);
    assert!(
        chart.feed_link.as_ref().unwrap().footprint.is_none(),
        "released"
    );
}

/// A market replay's profile sums what the driver delivered, whether it played there or sought.
#[cfg(feature = "drawings")]
#[test]
fn market_replay_profile_drawings_follow_the_cursor() {
    use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    let start = DateTime::UNIX_EPOCH;
    let second = |seconds| start + TimeDelta::seconds(seconds);
    // The minute after the first has 100 at 00:01:10 and 101 at 00:01:20, the next 101 at 00:02:10.
    let events = [(0, 90.0), (70, 100.0), (80, 101.0), (130, 101.0)]
        .into_iter()
        .enumerate()
        .map(|(at, (seconds, price))| {
            let fill = Tick::new(second(seconds), price, 1.0, None).with_id(at as u64);
            ReplayEvent::new("A", second(seconds), at as u64, MarketEvent::Trade(fill))
        })
        .collect();
    let instruments = vec![Instrument::new("A").with_tick_size(1.0)];
    let driver = Arc::new(ReplayDriver::new(events, instruments).unwrap());
    driver.set_speed(0.0).unwrap();
    driver.seek(second(75));
    let mut chart = Chart::new(BarData::new());
    chart.set_symbol("A");
    chart.set_timeframe(Timeframe::Min1);
    chart.set_market_replay(Some(driver.clone()));
    let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
    drawing.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 100.0)];
    let color = drawing.color;
    chart.drawings_mut().drawings.push(drawing);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    frames.run(&mut chart);
    // The top of the profile's tint, one price row above the highest execution held.
    let mut top = |chart: &mut Chart| {
        let tint = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 15);
        frames.time += 1.0 / 60.0;
        let painted = shapes(&frames.ctx, chart, frames.time, Vec::new());
        let points = painted.iter().flat_map(|shape| match shape {
            egui::Shape::Rect(rect) if rect.fill == tint => vec![rect.rect.min, rect.rect.max],
            egui::Shape::Mesh(mesh) => (mesh.vertices.iter())
                .filter(|vertex| vertex.color == tint)
                .map(|vertex| vertex.pos)
                .collect(),
            _ => Vec::new(),
        });
        let top = points.map(|point| point.y).reduce(f32::min).expect("tint");
        chart.get_chart_mapping().y_to_price(top)
    };
    let row = |top: f64| (top * 2.0).round() / 2.0;
    assert_eq!(
        row(top(&mut chart)),
        100.5,
        "sought before the 101 executions"
    );
    driver.set_speed(1.0).unwrap();
    driver.advance(std::time::Duration::from_secs(60)).unwrap();
    driver.set_speed(0.0).unwrap();
    assert_eq!(row(top(&mut chart)), 101.5, "played past them");
    driver.seek(second(75));
    assert_eq!(row(top(&mut chart)), 100.5, "sought back");
    driver.seek(second(135));
    assert_eq!(row(top(&mut chart)), 101.5, "sought to where it played");
}

/// An order-flow study present when a market replay starts follows the driver's cursor through
/// playback and seeks. Bars open at 0, 60 and 120 s and the per-bar delta is each buy's size: 1 at
/// 10 s, 2 at 70 s and 4 at 130 s, so 75 s shows [1, 2] over two bars and 140 s [1, 2, 4] over three.
#[cfg(feature = "studies")]
#[test]
fn market_replay_order_flow_studies_follow_the_cursor() {
    use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
    use crate::model::OrderSide::Buy;
    use crate::studies::OrderFlowStudy;
    let start = DateTime::UNIX_EPOCH;
    let second = |seconds| start + TimeDelta::seconds(seconds);
    let events = [(10, 1.0), (70, 2.0), (130, 4.0)]
        .into_iter()
        .enumerate()
        .map(|(at, (seconds, size))| {
            let fill = Tick::new(second(seconds), 100.0, size, Some(Buy)).with_id(at as u64);
            ReplayEvent::new("A", second(seconds), at as u64, MarketEvent::Trade(fill))
        })
        .collect();
    let instruments = vec![Instrument::new("A").with_tick_size(1.0)];
    let driver = Arc::new(ReplayDriver::new(events, instruments).unwrap());
    driver.set_speed(0.0).unwrap();
    driver.seek(second(75));
    let mut chart = Chart::new(BarData::new());
    chart.set_symbol("A");
    chart.set_timeframe(Timeframe::Min1);
    chart.set_trade_history_on_demand(true);
    chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
    chart.set_market_replay(Some(driver.clone()));
    let mut frames = Frames::new();
    let mut deltas = |chart: &mut Chart| {
        for _ in 0..8 {
            frames.run(chart);
        }
        let bars = chart.bars(SeriesId::MAIN).len();
        let study = chart.indicators().next().unwrap().1;
        (bars, study.values().line(0).to_vec())
    };
    let sought = deltas(&mut chart);
    driver.set_speed(1.0).unwrap();
    driver.advance(std::time::Duration::from_secs(65)).unwrap();
    driver.set_speed(0.0).unwrap();
    let played = deltas(&mut chart);
    driver.seek(second(75));
    let sought_back = deltas(&mut chart);
    assert_eq!(
        (sought, played, sought_back),
        (
            (2, vec![1.0, 2.0]),
            (3, vec![1.0, 2.0, 4.0]),
            (2, vec![1.0, 2.0])
        )
    );
}

/// Leaving a market replay gives order-flow studies the executions the live feed delivered, also
/// while the feed stays paused or a candle replay starts before the next frame, and leaves a chart
/// without a feed none of the recording's. The per-bar delta is each buy's size. The feed bought 2
/// at 10 s and 3 at 70 s: [2, 3] on one-minute bars opening at 0 and 60 s, and on one-trade bars.
/// The recording bought 1 at 70 s: [1] over its one bar. A candle replay at 90 s shows the one
/// completed minute, [2]. Rows are (chart, feed paused by the host before the replay, candle
/// replay started right after leaving). On one-trade bars the replay's sink allocated what the
/// studies hold, and the live executions replace that too.
#[cfg(feature = "studies")]
#[test]
fn leaving_a_market_replay_restores_the_live_executions() {
    use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
    use crate::model::OrderSide::Buy;
    use crate::studies::OrderFlowStudy;
    let start = DateTime::UNIX_EPOCH;
    let second = |seconds| start + TimeDelta::seconds(seconds);
    // A paused driver at 75 s over buys of `size` at `seconds`.
    let driver = |buys: &[(i64, f64)]| {
        let events = buys
            .iter()
            .enumerate()
            .map(|(at, &(seconds, size))| {
                let fill = Tick::new(second(seconds), 100.0, size, Some(Buy)).with_id(at as u64);
                ReplayEvent::new("A", second(seconds), at as u64, MarketEvent::Trade(fill))
            })
            .collect();
        let instruments = vec![Instrument::new("A").with_tick_size(1.0)];
        let replay = ReplayDriver::new(events, instruments).unwrap();
        replay.set_speed(0.0).unwrap();
        replay.seek(second(75));
        Arc::new(replay)
    };
    let fed_live = |timeframe: Timeframe| {
        ChartBuilder::new()
            .with_feed(driver(&[(10, 2.0), (70, 3.0)]))
            .with_symbol("A")
            .with_timeframe(timeframe)
            .build()
    };
    let without_feed = || {
        let bar = |seconds| Bar::new(second(seconds), 100.0, 100.0, 100.0, 100.0, 1.0);
        let mut chart = Chart::new(BarData::from_bars(vec![bar(0), bar(60)]));
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        chart
    };
    let mut observed = Vec::new();
    for (mut chart, paused, candle) in [
        (fed_live(Timeframe::Min1), true, false),
        (fed_live("1t".parse().unwrap()), true, false),
        (fed_live(Timeframe::Min1), false, true),
        (without_feed(), false, false),
    ] {
        chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
        let mut frames = Frames::new();
        let mut deltas = |chart: &mut Chart| {
            for _ in 0..8 {
                frames.run(chart);
            }
            let bars = chart.bars(SeriesId::MAIN).len();
            let study = chart.indicators().next().unwrap().1;
            (0..bars)
                .map(|bar| study.values().value(0, bar))
                .collect::<Vec<_>>()
        };
        let before = deltas(&mut chart);
        chart.set_feed_paused(paused);
        chart.set_market_replay(Some(driver(&[(70, 1.0)])));
        let replayed = deltas(&mut chart);
        chart.set_market_replay(None);
        if candle {
            chart.set_replay_time(Some(second(90)));
        }
        observed.push((before, replayed, deltas(&mut chart)));
    }
    let live = vec![Some(2.0), Some(3.0)];
    let replayed = vec![Some(1.0)];
    assert_eq!(
        observed,
        [
            (live.clone(), replayed.clone(), live.clone()),
            (live.clone(), replayed.clone(), live.clone()),
            (live, replayed.clone(), vec![Some(2.0)]),
            (vec![None, None], replayed, vec![None, None]),
        ]
    );
}

/// Answers every request at once with bars closing at the symbol's base plus their index:
/// AAPL from 100, MSFT from 40. It records the history requests and subscriptions.
#[derive(Default)]
struct Instant {
    requests: Mutex<Vec<HistoryRequest>>,
    subscriptions: Mutex<Vec<Subscription>>,
    stopped: Mutex<Vec<SubscriptionId>>,
}

impl Datafeed for Instant {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol)));
    }
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let base = if request.symbol == "AAPL" {
            100.0
        } else {
            40.0
        };
        let bars = bars(6)
            .into_iter()
            .map(|bar| {
                let close = bar.close - 100.0 + base;
                Bar::new(bar.time, close, close, close, close, 1.0)
            })
            .collect();
        self.requests.lock().unwrap().push(request);
        reply.send(page(bars, true));
    }
    fn subscribe(&self, subscription: Subscription) {
        self.subscriptions.lock().unwrap().push(subscription);
    }
    fn unsubscribe(&self, id: SubscriptionId) {
        self.stopped.lock().unwrap().push(id);
    }
}

fn closes(chart: &Chart, id: SeriesId) -> Vec<f64> {
    chart.bars(id).iter().map(|bar| bar.close).collect()
}

/// A comparison or spread on a chart with a feed gets its bars from that feed, and a restored
/// snapshot gets them again; `add_comparison` and a chart without a feed leave the series to
/// the host.
#[test]
fn comparisons_are_filled_from_the_feed_and_survive_a_snapshot() {
    use crate::model::SeriesScale;
    use crate::widget::SeriesOptions;
    let feed = Arc::new(Instant::default());
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("AAPL")
        .build();
    let msft = chart.add_feed_series(SeriesOptions {
        name: "MSFT".into(),
        color: egui::Color32::RED,
        scale: SeriesScale::Separate(1),
        ..Default::default()
    });
    let spread = chart.add_feed_series(SeriesOptions {
        name: "AAPL-MSFT".into(),
        scale: SeriesScale::Left,
        ..Default::default()
    });
    chart
        .series_price_scale_mut(msft)
        .unwrap()
        .set_range(10.0, 80.0);
    let pane = chart.add_pane();
    chart.move_series_to_pane(spread, pane);
    chart.set_series_visible(spread, false);
    let mut frames = Frames::new();
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(closes(&chart, msft), [40.0, 41.0, 42.0, 43.0, 44.0, 45.0]);
    assert_eq!(closes(&chart, spread), [60.0; 6]);

    #[cfg(feature = "ui")]
    {
        use crate::widget::PaneId;
        chart.set_dividend_adjusted(true);
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        let saved = chart.snapshot().unwrap();
        let saved: crate::widget::ChartSnapshot =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let mut other = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("AAPL")
            .build();
        other.restore(&saved).unwrap();
        for _ in 0..3 {
            frames.run(&mut other);
        }
        let restored: Vec<_> = other.series().collect();
        assert_eq!(restored.len(), 2);
        assert!(other.dividend_adjusted());
        let (msft, spread) = (restored[0].0, restored[1].0);
        assert_eq!(restored[0].1.name, "MSFT");
        assert_eq!(restored[0].1.color, egui::Color32::RED);
        assert_eq!(restored[0].1.scale, SeriesScale::Separate(1));
        assert_eq!(restored[1].1.scale, SeriesScale::Left);
        let range = other.series_price_scale(msft).unwrap().range();
        assert_eq!((range.min, range.max), (10.0, 80.0));
        assert_eq!(closes(&other, msft), [40.0, 41.0, 42.0, 43.0, 44.0, 45.0]);
        assert_eq!(closes(&other, spread), [60.0; 6]);
        assert!(other.is_fed(msft) && other.is_fed(spread));
        assert!(other.is_series_visible(msft) && !other.is_series_visible(spread));
        assert_eq!(other.series_pane(msft), Some(PaneId::MAIN));
        assert_eq!(other.series_pane(spread), Some(other.panes()[1]));
    }

    let sink = feed
        .subscriptions
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|subscription| subscription.symbol == "MSFT" && subscription.sink.is_active())
        .unwrap()
        .sink
        .clone();
    let time = chart.bars(msft).last().unwrap().time;
    sink.push_bar(Bar::new(time, 50.0, 50.0, 50.0, 50.0, 1.0));
    frames.run(&mut chart);
    assert_eq!(chart.bars(msft).last().unwrap().close, 50.0);
    assert_eq!(chart.bars(spread).last().unwrap().close, 55.0);
    let duplicate = chart.add_feed_series(SeriesOptions {
        name: "MSFT".into(),
        ..Default::default()
    });
    frames.run(&mut chart);
    assert_eq!(closes(&chart, duplicate), closes(&chart, msft));
    let mut renamed = chart
        .series()
        .find(|(id, _)| *id == duplicate)
        .unwrap()
        .1
        .clone();
    renamed.name = "OTHER".into();
    chart.set_series_options(duplicate, renamed);
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.bars(duplicate).last().unwrap().close, 45.0);
    let mut options = chart
        .series()
        .find(|(id, _)| *id == msft)
        .unwrap()
        .1
        .clone();
    options.name = "OTHER".into();
    chart.begin_user_action();
    chart.set_series_options(msft, options);
    chart.end_user_action();
    frames.run(&mut chart);
    assert_eq!(chart.bars(msft).last().unwrap().close, 45.0);
    assert!(chart.undo());
    frames.run(&mut chart);
    assert_eq!(chart.bars(msft).last().unwrap().close, 50.0);
    chart.remove_series(msft);
    chart.remove_series(spread);
    frames.run(&mut chart);
    assert!(!sink.is_active());
    assert!(feed.stopped.lock().unwrap().contains(&sink.id));

    let host = chart.add_comparison("MSFT", egui::Color32::RED);
    assert!(!chart.is_fed(host));
    let mut manual = ChartBuilder::new().build();
    let id = manual.add_feed_series(SeriesOptions {
        name: "MSFT".into(),
        ..Default::default()
    });
    assert!(!manual.is_fed(id));
}

/// Undo of a removed feed-backed series shows the bars it held over the frames before its history
/// reply, merges a live bar into them and then leaves the series to the reply. A candle replay
/// entered before the reply windows the held bars with the live bar merged. Bars held under
/// another interval, hours, conversion or adjustment than the chart's are not shown, also when the
/// interval changed just before the removal, while the series still had its old bars, and neither
/// are the live feed's bars under a market replay, whether the series left before or during it.
#[test]
fn undo_of_a_removed_fed_series_keeps_its_bars() {
    // MSFT closes at 40 plus the index of the one-minute bar.
    let msft = |range: std::ops::Range<usize>| -> Vec<Bar> {
        bars(7)[range]
            .iter()
            .map(|bar| {
                let close = bar.close - 60.0;
                Bar::new(bar.time, close, close, close, close, 1.0)
            })
            .collect()
    };
    let held = [40.0, 41.0, 42.0, 43.0, 44.0, 45.0];
    let merged = [40.0, 41.0, 42.0, 43.0, 44.0, 50.0];
    // A market replay in the fifth minute of a recording that lacks MSFT, run for a frame.
    let market = |chart: &mut Chart| {
        use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
        let time = bars(5)[4].time + TimeDelta::seconds(30);
        let fill = Tick::new(time, 70.0, 1.0, None).with_id(0);
        let events = vec![ReplayEvent::new("A", time, 0, MarketEvent::Trade(fill))];
        let driver = ReplayDriver::new(events, vec![Instrument::new("A")]).unwrap();
        driver.set_speed(0.0).unwrap();
        driver.seek(time);
        chart.set_market_replay(Some(Arc::new(driver)));
        chart.pump(&egui::Context::default());
    };
    // How a row ends once a live bar has merged: the history reply arrives, a candle replay
    // starts before it, or a market replay drives the series and no reply comes.
    #[derive(PartialEq)]
    enum End {
        Reply,
        CandleReplay,
        MarketReplay,
    }
    // The change, whether it comes before the removal, the closes from undo to the reply,
    // without and with a live bar of 50 in the last slot, and how the row ends.
    type Row<'a> = (&'a str, bool, fn(&mut Chart), &'a [f64], &'a [f64], End);
    let rows: [Row; 8] = [
        ("none", false, |_| {}, &held, &merged, End::Reply),
        (
            "candle replay",
            false,
            |_| {},
            &held,
            &merged,
            End::CandleReplay,
        ),
        (
            "interval",
            true,
            |chart| chart.set_timeframe(Timeframe::Sec30),
            &[],
            &[50.0],
            End::Reply,
        ),
        (
            "hours",
            false,
            |chart| chart.set_extended_hours(true),
            &[],
            &[50.0],
            End::Reply,
        ),
        (
            "conversion",
            false,
            |chart| chart.set_conversion(Some("EUR".into())),
            &[],
            &[50.0],
            End::Reply,
        ),
        (
            "adjustment",
            false,
            |chart| chart.set_dividend_adjusted(true),
            &[],
            &[50.0],
            End::Reply,
        ),
        ("market replay", false, market, &[], &[], End::MarketReplay),
        (
            "removed in a market replay",
            true,
            market,
            &[],
            &[],
            End::MarketReplay,
        ),
    ];
    for (change, before_removal, apply, shown, live, end) in rows {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, None);
        let id = chart.add_feed_series(crate::widget::SeriesOptions {
            name: "MSFT".into(),
            ..Default::default()
        });
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("MSFT")));
        frames.run(&mut chart);
        feed.page().send(page(msft(0..6), true));
        frames.run(&mut chart);
        assert_eq!(closes(&chart, id), held);

        if before_removal {
            apply(&mut chart);
        }
        assert!(chart.user_action(|chart| chart.remove_series(id)));
        if !before_removal {
            apply(&mut chart);
        }
        frames.run(&mut chart);
        assert!(chart.undo());
        assert_eq!(closes(&chart, id), shown, "{change}: right after undo");
        frames.run(&mut chart);
        if end != End::MarketReplay {
            feed.resolve().send(Ok(Instrument::new("MSFT")));
        }
        for frame in 0..3 {
            frames.run(&mut chart);
            assert_eq!(
                closes(&chart, id),
                shown,
                "{change}: frame {frame} before the reply"
            );
        }
        let last = msft(5..6)[0].time;
        feed.sink()
            .push_bar(Bar::new(last, 50.0, 50.0, 50.0, 50.0, 1.0));
        frames.run(&mut chart);
        assert_eq!(
            closes(&chart, id),
            live,
            "{change}: a live bar before the reply"
        );
        match end {
            End::Reply => {}
            End::CandleReplay => {
                chart.set_replay_time(Some(msft(3..4)[0].time));
                frames.run(&mut chart);
                assert_eq!(closes(&chart, id), [40.0, 41.0, 42.0], "{change}: entered");
                chart.set_replay_time(Some(msft(6..7)[0].time));
                frames.run(&mut chart);
                assert_eq!(closes(&chart, id), merged, "{change}: sought past the bars");
                continue;
            }
            End::MarketReplay => continue,
        }
        feed.page().send(page(msft(1..7), true));
        frames.run(&mut chart);
        assert_eq!(
            closes(&chart, id),
            [41.0, 42.0, 43.0, 44.0, 50.0, 46.0],
            "{change}: the reply replaces the held bars"
        );
    }
}

/// Dividend adjustment is asked of the feed in the history and live subscriptions of the main
/// series and the comparisons, and changing it loads and subscribes them again.
#[test]
fn dividend_adjustment_is_requested_for_the_chart_and_its_comparisons() {
    let feed = Arc::new(Instant::default());
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("AAPL")
        .build();
    chart.add_feed_series(crate::widget::SeriesOptions {
        name: "MSFT".into(),
        ..Default::default()
    });
    let mut frames = Frames::new();
    for adjusted in [false, true] {
        chart.set_dividend_adjusted(adjusted);
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        #[cfg(feature = "alert-engine")]
        assert_eq!(chart.alert_data().unwrap().request.adjusted, adjusted);
        let mut asked: Vec<_> = std::mem::take(&mut *feed.requests.lock().unwrap())
            .into_iter()
            .map(|request| (request.symbol, request.adjusted))
            .collect();
        asked.sort();
        assert_eq!(
            asked,
            [("AAPL".into(), adjusted), ("MSFT".into(), adjusted)],
            "adjusted {adjusted}"
        );
        let mut subscribed: Vec<_> = std::mem::take(&mut *feed.subscriptions.lock().unwrap())
            .into_iter()
            .map(|subscription| (subscription.symbol, subscription.adjusted))
            .collect();
        subscribed.sort();
        subscribed.dedup();
        assert_eq!(
            subscribed,
            [("AAPL".into(), adjusted), ("MSFT".into(), adjusted)],
            "adjusted {adjusted}"
        );
    }
}

#[cfg(feature = "studies")]
#[test]
fn replay_exit_restores_prepared_history_to_a_study_added_while_paused() {
    use crate::data::{FootprintBars, MarketEvent, ReplayDriver, ReplayEvent};
    use crate::model::OrderSide::{Buy, Sell};

    for shared in [false, true] {
        let feed = Arc::new(Script::default());
        feed.prepared_footprints.store(true, Ordering::Relaxed);
        let mut chart = fed(&feed, Some("A"));
        chart.set_chart_type(ChartType::VolumeFootprint);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
        let start = bars(1)[0].time;
        let at = |second| start + TimeDelta::seconds(second);
        let mut prepared = prepared_page(
            &[
                Tick::new(at(0), 100.0, 2.0, Some(Buy)),
                Tick::new(at(60), 101.0, 3.0, Some(Sell)),
            ],
            at(120),
            true,
        );
        if shared {
            let FootprintBars::Owned(rows) = prepared.bars else {
                unreachable!()
            };
            prepared.bars = FootprintBars::Shared(Arc::new(rows));
        }
        feed.footprints.lock().unwrap().remove(0).send(Ok(prepared));
        frames.run(&mut chart);
        let flow = chart
            .feed_link
            .as_ref()
            .unwrap()
            .footprint
            .as_ref()
            .unwrap()
            .view();
        let owner = flow
            .complete_bars(..)
            .next()
            .unwrap()
            .levels
            .values()
            .next()
            .unwrap() as *const _;

        chart.set_feed_paused(true);
        chart.set_chart_type(ChartType::Candles);
        let study = crate::studies::IndicatorFactory::new()
            .create("Vol Profile FR")
            .unwrap();
        let input = study.order_flow_inputs()[0].clone();
        chart.add_indicator(study);
        // The paused live feed has not connected this study or promoted its owned prefix.
        let recorded = Tick::new(at(70), 500.0, 17.0, None).with_id(1);
        let driver = ReplayDriver::new(
            vec![ReplayEvent::new(
                "A",
                at(70),
                1,
                MarketEvent::Trade(recorded),
            )],
            vec![Instrument::new("A").with_tick_size(1.0)],
        )
        .unwrap();
        driver.set_speed(0.0).unwrap();
        driver.seek(at(75));
        chart.set_market_replay(Some(Arc::new(driver)));
        for _ in 0..8 {
            frames.run(&mut chart);
        }
        assert_eq!(
            input
                .footprint_snapshot(start)
                .bars(..)
                .map(|bar| bar.volume())
                .sum::<f64>(),
            17.0
        );
        chart.set_market_replay(None);
        let restored = input.footprint_snapshot(start);
        assert_eq!(
            restored
                .bars(..)
                .map(|bar| bar.volume())
                .collect::<Vec<_>>(),
            [2.0, 3.0]
        );
        assert_eq!(
            restored
                .bars(..)
                .next()
                .unwrap()
                .levels
                .values()
                .next()
                .unwrap() as *const _,
            owner
        );
    }
}

#[cfg(feature = "studies")]
#[test]
fn prepared_profiles_keep_received_trades_after_raw_eviction() {
    use crate::data::{FootprintBars, ProfileSource};

    for shared in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let factory = crate::studies::IndicatorFactory::new();
        let ids = [
            chart.add_indicator(factory.create("Vol Profile FR").unwrap()),
            chart.add_indicator(factory.create("Vol Profile FR").unwrap()),
        ];
        chart.set_chart_type(ChartType::Candles);
        frames.run(&mut chart);
        let start = bars(1)[0].time;
        let at = |second| start + TimeDelta::seconds(second);
        let mut page = prepared_page(&[Tick::new(start, 100.0, 3.0, None)], at(90), true);
        if shared && let FootprintBars::Owned(bars) = page.bars {
            page.bars = FootprintBars::Shared(Arc::new(bars));
        }
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(Vec::new(), true)));
        feed.footprints.lock().unwrap().remove(0).send(Ok(page));
        frames.run(&mut chart);
        frames.time += 1.0;
        frames.run(&mut chart);
        frames.run(&mut chart);
        let inputs: Vec<_> = ids
            .iter()
            .map(|id| {
                chart
                    .indicators()
                    .find(|(seen, _)| seen == id)
                    .unwrap()
                    .1
                    .order_flow_inputs()[0]
                    .clone()
            })
            .collect();
        let resets: Vec<_> = inputs.iter().map(|input| input.reset_revision()).collect();
        chart.feed_link.as_mut().unwrap().trades = Trades::new(3);
        let sink = feed.sink();
        for (index, second) in [90, 120, 180, 240, 300, 360].into_iter().enumerate() {
            sink.push_tick(Tick::new(at(second), 100.0, 5.0, None));
            frames.run(&mut chart);
            for (input, reset) in inputs.iter().zip(&resets) {
                assert_eq!(
                    input.reset_revision(),
                    *reset,
                    "raw eviction must not reset a profile"
                );
                let read = input.footprint_snapshot(start);
                assert!(read.span_is_complete(start, at(second)));
                assert_eq!(
                    read.bars(..).map(|bar| bar.volume()).sum::<f64>(),
                    3.0 + 5.0 * (index + 1) as f64
                );
            }
            for id in ids {
                let study = chart.indicators().find(|(seen, _)| *seen == id).unwrap().1;
                let plots = study.volume_profiles(0..chart.bars(SeriesId::MAIN).len());
                assert_eq!(
                    plots.len(),
                    1,
                    "both profiles remain visible on every update"
                );
                assert_eq!(
                    plots[0].profile.source(),
                    ProfileSource::Executions { complete: true }
                );
            }
        }
        // Reanchor as a subscription change does, after history confirmed its first instant.
        let tape = &mut chart.feed_link.as_mut().unwrap().trades;
        tape.prepend(Vec::new(), &mut |_, _| ());
        tape.reanchor();
        for second in [480, 540, 600, 660, 720] {
            sink.push_tick(Tick::new(at(second), 100.0, 1.0, None));
            frames.run(&mut chart);
        }
        for (input, reset) in inputs.iter().zip(&resets) {
            assert_eq!(input.reset_revision(), *reset);
            let read = input.footprint_snapshot(start);
            assert!(
                !read.span_is_complete(at(360), at(480)),
                "the subscription gap remains unknown"
            );
            assert!(
                read.span_is_complete(at(540), at(720)),
                "eviction cannot widen the subscription gap"
            );
        }
        let revisions: Vec<_> = inputs.iter().map(|input| input.revision()).collect();
        // Overflow the sink before a drain: neither profile ever receives the first execution.
        sink.push_tick(Tick::new(at(780), 100.0, 7.0, None));
        for _ in 0..TRADE_CAPACITY {
            sink.push_tick(Tick::new(at(781), 100.0, 1.0, None));
        }
        frames.run(&mut chart);
        for ((input, reset), revision) in inputs.iter().zip(&resets).zip(revisions) {
            assert_eq!(
                input.reset_revision(),
                *reset,
                "coverage changes invalidate only affected bars"
            );
            assert_eq!(input.changed_since(revision), Some(at(720)));
            let read = input.footprint_snapshot(start);
            assert!(
                read.span_is_complete(start, at(300)),
                "received history stays complete"
            );
            assert!(
                !read.span_is_complete(at(720), at(780)),
                "loss before the store stays unknown"
            );
        }
    }
}

#[cfg(feature = "studies")]
#[test]
fn prepared_profile_replay_keeps_the_boundary_seed() {
    use crate::data::FootprintBars;

    for (shared, capacity) in [(false, 3), (true, 3), (false, 2), (true, 2)] {
        let (feed, mut chart, mut frames) = prepared_chart();
        let id = chart.add_indicator(
            crate::studies::IndicatorFactory::new()
                .create("Vol Profile FR")
                .unwrap(),
        );
        frames.run(&mut chart);
        let start = bars(1)[0].time;
        let at = |second| start + TimeDelta::seconds(second);
        let mut page = prepared_page(&[Tick::new(at(10), 100.0, 3.0, None)], at(30), true);
        if shared && let FootprintBars::Owned(bars) = page.bars {
            page.bars = FootprintBars::Shared(Arc::new(bars));
        }
        feed.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Ok(TradePage::new(Vec::new(), true)));
        feed.footprints.lock().unwrap().remove(0).send(Ok(page));
        frames.run(&mut chart);
        chart.feed_link.as_mut().unwrap().trades = Trades::new(capacity);
        let sink = feed.sink();
        if capacity == 2 {
            sink.push_tick(Tick::new(at(10), 100.0, 3.0, None));
        }
        for (second, size) in [(40, 5.0), (50, 7.0)] {
            sink.push_tick(Tick::new(at(second), 100.0, size, None));
        }
        frames.run(&mut chart);
        let input = chart
            .indicators()
            .find(|(seen, _)| *seen == id)
            .unwrap()
            .1
            .order_flow_inputs()[0]
            .clone();
        let link = chart.feed_link.as_ref().unwrap();
        for (second, volume) in [(50, Some(8.0)), (30, Some(3.0)), (20, None)] {
            input.set_replay(&link.clock, Some(at(second)), Some(&link.trades), &[]);
            let read = input.footprint_snapshot(start);
            assert_eq!(
                read.span_is_complete(start, at(second - 1)),
                volume.is_some()
            );
            assert_eq!(
                read.bars(..).next().map(|bar| bar.volume()),
                volume,
                "shared={shared}, cutoff={second}"
            );
            assert_eq!(
                input
                    .footprints_from(start)
                    .bars
                    .values()
                    .next()
                    .map(|bar| bar.volume()),
                volume
            );
        }
        // Once the store loses a post-page execution in the boundary, its partial is unknown.
        sink.push_tick(Tick::new(at(70), 100.0, 1.0, None));
        sink.push_tick(Tick::new(at(80), 100.0, 1.0, None));
        frames.run(&mut chart);
        let link = chart.feed_link.as_ref().unwrap();
        input.set_replay(&link.clock, Some(at(50)), Some(&link.trades), &[]);
        assert!(
            !input
                .footprint_snapshot(start)
                .span_is_complete(start, at(49))
        );
        assert!(input.footprints_from(start).bars.is_empty());
    }
}

#[cfg(feature = "studies")]
#[test]
fn prepared_history_reaches_profile_studies_without_copying_rows() {
    use crate::data::{FootprintBars, FootprintLevel, ProfileSource};
    use crate::model::OrderSide::{Buy, Sell};

    for shared in [false, true] {
        for late_study in [false, true] {
            for delivery in 0..3 {
                let feed = Arc::new(Script::default());
                feed.prepared_footprints.store(true, Ordering::Relaxed);
                let mut chart = fed(&feed, Some("A"));
                let add = |chart: &mut Chart| {
                    chart.add_indicator(
                        crate::studies::IndicatorFactory::new()
                            .create("Vol Profile FR")
                            .unwrap(),
                    )
                };
                let mut id = if late_study {
                    chart.set_chart_type(ChartType::VolumeFootprint);
                    None
                } else {
                    Some(add(&mut chart))
                };
                let mut frames = Frames::new();
                frames.run(&mut chart);
                feed.resolve()
                    .send(Ok(Instrument::new("A").with_tick_size(1.0)));
                feed.page().send(page(bars(5), true));
                frames.run(&mut chart);
                frames.run(&mut chart);
                let mut reply = Some(
                    feed.footprints
                        .lock()
                        .unwrap()
                        .pop()
                        .expect("a profile on candles must request prepared history"),
                );
                let start = bars(1)[0].time;
                let at = |second| start + TimeDelta::seconds(second);
                let saved = [
                    Tick::new(at(0), 100.0, 2.0, Some(Buy)),
                    Tick::new(at(60), 101.0, 3.0, Some(Sell)),
                    Tick::new(at(120), 102.0, 5.0, None),
                ];
                let mut page = prepared_page(&saved, at(150), true);
                let FootprintBars::Owned(prepared) = page.bars else {
                    unreachable!()
                };
                let row = prepared[0].levels.values().next().unwrap() as *const FootprintLevel;
                page.bars = if shared {
                    FootprintBars::Shared(Arc::new(prepared))
                } else {
                    FootprintBars::Owned(prepared)
                };
                let mut page = Some(page);
                if delivery == 1 {
                    reply.take().unwrap().send(Ok(page.take().unwrap()));
                    frames.run(&mut chart);
                }
                let live = [
                    Tick::new(at(150), 102.0, 7.0, Some(Buy)).with_id(99),
                    Tick::new(at(150), 102.0, 11.0, None).with_id(7),
                    Tick::new(at(151), 102.0, 13.0, Some(Sell)).with_id(42),
                ];
                let sink = feed.sink();
                sink.push_tick(Tick::new(at(149), 102.0, 999.0, None));
                for trade in &live {
                    sink.push_tick(trade.clone());
                }
                if delivery == 2 {
                    reply.take().unwrap().send(Ok(page.take().unwrap()));
                }
                frames.run(&mut chart);
                if delivery == 0 {
                    reply.take().unwrap().send(Ok(page.take().unwrap()));
                    frames.run(&mut chart);
                }
                if late_study {
                    id = Some(add(&mut chart));
                    chart.set_chart_type(ChartType::Candles);
                    frames.run(&mut chart);
                }
                let input = chart
                    .indicators()
                    .find(|(seen, _)| Some(*seen) == id)
                    .unwrap()
                    .1
                    .order_flow_inputs()[0]
                    .clone();
                assert_eq!(
                    input
                        .footprint_snapshot(start)
                        .bars(..)
                        .map(|bar| bar.volume())
                        .collect::<Vec<_>>(),
                    [2.0, 3.0, 36.0],
                    "the page and live suffix must join once before a raw-history reply"
                );
                feed.trades
                    .lock()
                    .unwrap()
                    .remove(0)
                    .send(Ok(TradePage::new(
                        saved.iter().chain(&live).cloned().collect(),
                        true,
                    )));
                frames.run(&mut chart);
                sink.push_tick(live[2].clone());
                frames.run(&mut chart);
                frames.time += 1.0;
                frames.run(&mut chart);
                let (_, study) = chart
                    .indicators()
                    .find(|(seen, _)| Some(*seen) == id)
                    .unwrap();
                let plots = study.volume_profiles(0..5);
                let profile = &plots.first().expect("profile has executed rows").profile;
                assert_eq!(
                    profile.source(),
                    ProfileSource::Executions { complete: true },
                    "shared={shared}, late_study={late_study}, delivery={delivery}"
                );
                assert_eq!(profile.volume(), [2.0, 3.0, 36.0]);
                assert_eq!(profile.up(), [2.0, 0.0, 7.0]);
                assert_eq!(profile.down(), [0.0, 3.0, 13.0]);
                let read = study.order_flow_inputs()[0].footprint_snapshot(start);
                let first = read.bars(..).next().unwrap();
                assert_eq!(
                    first.levels.values().next().unwrap() as *const FootprintLevel,
                    row,
                    "completed prepared rows must keep their owner"
                );
                assert_eq!(
                    input
                        .footprints_from(start)
                        .bars
                        .values()
                        .map(|bar| bar.volume())
                        .collect::<Vec<_>>(),
                    [2.0, 3.0, 36.0]
                );
                assert!(
                    input
                        .footprint_snapshot_for_clock(start, &BarClock::new(Timeframe::Min5))
                        .is_none()
                );

                let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
                let frozen_bars = frozen.bars(SeriesId::MAIN).to_vec();
                frozen
                    .indicator_mut(id.unwrap())
                    .unwrap()
                    .calculate(&frozen_bars);
                sink.push_tick(Tick::new(at(152), 500.0, 17.0, None).with_id(100));
                frames.run(&mut chart);
                assert_eq!(
                    input
                        .footprint_snapshot(start)
                        .bars(..)
                        .map(|bar| bar.volume())
                        .sum::<f64>(),
                    58.0
                );
                let (_, saved) = frozen
                    .indicators()
                    .find(|(seen, _)| Some(*seen) == id)
                    .unwrap();
                assert_eq!(
                    saved.volume_profiles(0..5)[0].profile.volume(),
                    [2.0, 3.0, 36.0],
                    "a first query after a live update must still use the export's data"
                );
                feed.calls();
                frames.run(&mut frozen);
                assert!(
                    feed.calls().is_empty(),
                    "frozen profiles must not request data"
                );
            }
        }
    }
}

/// A volume profile study on a chart with a feed builds its rows from the executions the feed
/// streams, by the chart's bar clock.
#[cfg(feature = "studies")]
#[test]
fn volume_profile_studies_read_the_chart_executions() {
    use crate::data::ProfileSource;
    use crate::model::OrderSide::{Buy, Sell};
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    chart.chart_options.time_scale.timezone = crate::scales::TimezoneMode::Timezone(
        crate::model::TimezoneId::fixed_offset(3600).unwrap(),
    );
    let factory = crate::studies::IndicatorFactory::new();
    let id = chart.add_indicator(factory.create("Vol Profile FR").unwrap());
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(0.25)));
    feed.page().send(page(bars(2), true));
    frames.run(&mut chart);
    let start = bars(1)[0].time;
    let at = |second| start + TimeDelta::seconds(second);
    let sink = feed.sink();
    for (second, price, size, side) in [
        (10, 100.0, 2.0, Some(Buy)),
        (70, 100.25, 1.0, Some(Sell)),
        (80, 100.5, 3.0, None),
    ] {
        sink.push_tick(Tick::new(at(second), price, size, side));
    }
    for _ in 0..8 {
        frames.run(&mut chart);
    }
    // Past the wait before a study's next full recompute.
    frames.time += 1.0;
    frames.run(&mut chart);
    let (_, study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
    let profiles = study.volume_profiles(0..2);
    let profile = &profiles[0].profile;
    assert_eq!(
        profile.source(),
        ProfileSource::Executions { complete: true }
    );
    assert_eq!((profile.low(), profile.row_height()), (99.875, 0.25));
    assert_eq!(profile.volume(), [2.0, 1.0, 3.0]);
    assert_eq!(profile.up(), [2.0, 0.0, 0.0]);
    assert_eq!(profile.down(), [0.0, 1.0, 0.0]);

    let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
    sink.push_tick(Tick::new(at(81), 100.5, 11.0, None));
    frames.run(&mut chart);
    let (_, live_study) = chart.indicators().find(|(seen, _)| *seen == id).unwrap();
    assert_eq!(
        live_study.order_flow_inputs()[0]
            .footprints_from(start)
            .bars
            .values()
            .map(|bar| bar.volume())
            .sum::<f64>(),
        17.0
    );
    let (_, study) = frozen.indicators().find(|(seen, _)| *seen == id).unwrap();
    assert_eq!(
        study.volume_profiles(0..2)[0].profile.volume(),
        [2.0, 1.0, 3.0],
        "a deferred export query must not see later live executions"
    );
    feed.calls();
    frames.run(&mut frozen);
    assert!(
        feed.calls().is_empty(),
        "export must not subscribe or request history"
    );
}

/// Leaving replay keeps the bars and the book history the chart showed before it and rebuilds
/// only its last bar: from a latest page that reaches the kept bars, from executions the feed
/// replays on resubscribing without counting those of the kept bars twice or letting the bars it
/// pushes again replace them, or from a latest page alone when it starts after them.
#[test]
fn replay_exit_keeps_live_bars_and_depth() {
    let minute = |at: usize| bars(13)[at].time;
    let fill = |at, price| Tick::new(minute(at) + TimeDelta::seconds(10), price, 1.0, None);
    let mut caught_up = bars(10);
    caught_up[9] = Bar::new(minute(9), 109.0, 112.0, 108.0, 111.0, 3.0);
    let cases = [
        (
            "a page that reaches the kept bars",
            caught_up.clone(),
            vec![],
            vec![],
            caught_up,
        ),
        (
            "replayed executions",
            bars(5),
            vec![],
            vec![fill(8, 108.0), fill(9, 109.0)],
            bars(10),
        ),
        (
            "bars the feed pushes again",
            bars(5),
            vec![Bar::new(minute(7), 107.0, 107.0, 107.0, 107.0, 0.5)],
            vec![fill(9, 109.0)],
            bars(10),
        ),
        (
            "a page that starts after them",
            bars(13)[11..].to_vec(),
            vec![],
            vec![],
            bars(13)[11..].to_vec(),
        ),
    ];
    for (case, latest, pushed, replayed, expected) in cases {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(10), true));
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        let link = chart.feed_link.as_mut().unwrap();
        link.depth_history
            .as_mut()
            .unwrap()
            .push(crate::data::DepthColumn {
                time: minute(9),
                span: TimeDelta::seconds(1),
                bids: vec![(100, 5.0)],
                asks: vec![(101, 5.0)],
                ..Default::default()
            });
        let shown = |chart: &Chart| {
            let link = chart.feed_link.as_ref().unwrap();
            let book = link
                .depth_history
                .as_ref()
                .unwrap()
                .columns()
                .next()
                .map(|c| c.time);
            (chart.state.data().bars.clone(), book)
        };
        chart.set_replay_time(Some(minute(5)));
        frames.run(&mut chart);
        feed.calls();
        chart.set_replay_time(None);
        frames.run(&mut chart);
        assert_eq!(
            feed.calls(),
            [
                Call::Resolve("A".into()),
                history("A", Timeframe::Min1, None)
            ],
            "{case}: catching up"
        );
        assert_eq!(
            shown(&chart),
            (bars(10)[..9].to_vec(), Some(minute(9))),
            "{case}: kept"
        );
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        frames.run(&mut chart);
        for bar in pushed {
            feed.sink().push_bar(bar);
        }
        for tick in replayed {
            feed.sink().push_tick(tick);
        }
        feed.page().send(page(latest, true));
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        assert_eq!(
            shown(&chart),
            (expected, Some(minute(9))),
            "{case}: caught up"
        );
    }
}

/// Resuming keeps the bars the stream built before the pause: the latest page counts their
/// trades too, also in its newest bar, which must not add them a second time.
#[test]
fn resuming_keeps_the_bars_the_stream_built_before_the_pause() {
    let feed = Arc::new(Script::default());
    let mut chart = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    feed.page().send(page(bars(5), true));
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    for bar in &bars(8)[5..] {
        let fill = Tick::new(bar.time + TimeDelta::seconds(10), bar.close, 1.0, None);
        feed.sink().push_tick(fill);
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.state.data().bars, bars(8), "streamed");
    chart.set_feed_paused(true);
    chart.set_feed_paused(false);
    frames.run(&mut chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    // Completed bars only: minute 7 still forms.
    feed.page().send(page(bars(7), true));
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.state.data().bars, bars(7));
}
