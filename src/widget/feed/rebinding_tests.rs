use super::*;
use crate::data::{FootprintBars, FootprintLevel, PriceTargets};
use crate::widget::SeriesOptions;

// Distinct adapter allocations can share a backend without sharing a chart's binding.
struct Adapter(Arc<Script>);
impl Datafeed for Adapter {
    fn capabilities(&self, reply: Reply<FeedCapabilities>) {
        self.0.capabilities(reply);
    }
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        Datafeed::resolve(&*self.0, symbol, reply);
    }
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        self.0.history(request, reply);
    }
    fn subscribe(&self, subscription: Subscription) {
        self.0.subscribe(subscription);
    }
    fn unsubscribe(&self, id: SubscriptionId) {
        self.0.unsubscribe(id);
    }
}

fn load(feed: &Script, chart: &mut Chart, frames: &mut Frames, price: f64) {
    frames.run(chart);
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    let history = bars(5)
        .into_iter()
        .map(|bar| Bar::new(bar.time, price, price, price, price, 1.0))
        .collect();
    feed.page().send(page(history, true));
    frames.run(chart);
    frames.run(chart);
}

#[test]
fn set_feed_isolates_adapters_and_rejects_old_handshake_replies() {
    let backend = Arc::new(Script::default());
    let first: Arc<dyn Datafeed> = Arc::new(Adapter(backend.clone()));
    let second: Arc<dyn Datafeed> = Arc::new(Adapter(backend.clone()));
    let mut chart = ChartBuilder::new()
        .with_feed(first.clone())
        .with_symbol("A")
        .build();
    let mut sibling = ChartBuilder::new()
        .with_feed(second)
        .with_symbol("A")
        .build();
    let mut frames = Frames::new();
    load(&backend, &mut sibling, &mut frames, 40.0);
    let sibling_sink = backend.sink();
    frames.run(&mut chart);
    let old_resolve = backend.resolve();
    let old_page = backend.page();
    chart.user_action(|chart| chart.config_mut().show_grid = false);
    backend.calls();
    chart.set_feed(first);
    assert!(chart.can_undo());
    assert!(backend.calls().is_empty());

    let skipped = Arc::new(Script::default());
    let final_feed = Arc::new(Script::default());
    chart.set_feed(skipped.clone());
    chart.set_feed(final_feed.clone());
    assert!(skipped.calls().is_empty());
    assert!(final_feed.calls().is_empty());
    assert!(!chart.can_undo());
    frames.run(&mut chart);
    old_resolve.send(Ok(Instrument::new("A").with_tick_size(99.0)));
    old_page.send(page(bars(5), true));
    for reply in backend.capabilities.lock().unwrap().drain(..) {
        reply.send(Ok(FeedCapabilities::new().with_exchanges(["OLD".into()])));
    }
    frames.run(&mut chart);
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    assert!(chart.instrument().is_none());
    assert!(chart.feed_capabilities().is_none());
    final_feed
        .resolve()
        .send(Ok(Instrument::new("A").with_tick_size(0.5)));
    final_feed.page().send(page(
        vec![Bar::new(bars(1)[0].time, 7.0, 7.0, 7.0, 7.0, 1.0)],
        true,
    ));
    frames.run(&mut chart);
    assert_eq!(closes(&chart, SeriesId::MAIN), [7.0]);
    assert_eq!(chart.instrument().unwrap().tick_size, Some(0.5));
    assert!(sibling_sink.is_active());
    sibling_sink.push_bar(Bar::new(bars(5)[4].time, 42.0, 42.0, 42.0, 42.0, 1.0));
    frames.run(&mut sibling);
    assert_eq!(
        closes(&sibling, SeriesId::MAIN),
        [40.0, 40.0, 40.0, 40.0, 42.0]
    );
    assert!(
        backend
            .calls()
            .iter()
            .all(|call| !matches!(call, Call::Unsubscribe(_)))
    );
}

#[test]
fn set_feed_retires_stream_prepared_trade_depth_and_target_work() {
    let (old, mut chart, mut frames) = prepared_chart();
    chart.set_depth_history(Some(DepthHistory::new(Default::default()).unwrap()));
    chart.set_price_targets_visible(true);
    frames.run(&mut chart);
    let sink = old.sink();
    *old.teardown_push.lock().unwrap() = Some(sink.clone());
    let prepared = old.footprints.lock().unwrap().remove(0);
    let trades = old.trades.lock().unwrap().remove(0);
    let depth = old.depth.lock().unwrap().remove(0);
    let targets = old.targets.lock().unwrap().remove(0);
    let next = Arc::new(Script::default());
    old.calls();
    chart.set_feed(next.clone());
    assert!(!sink.is_active());
    assert!(
        old.calls()
            .iter()
            .all(|call| matches!(call, Call::Unsubscribe(_)))
    );
    assert!(next.calls().is_empty());
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    let start = bars(1)[0].time;
    let tick = Tick::new(start, 100.0, 999.0, None);
    prepared.send(Ok(prepared_page(
        std::slice::from_ref(&tick),
        start + TimeDelta::minutes(1),
        true,
    )));
    trades.send(Ok(TradePage::new(vec![tick.clone()], true)));
    depth.send(Ok(DepthPage::new(
        vec![crate::data::DepthColumn {
            time: start,
            span: TimeDelta::seconds(1),
            ..Default::default()
        }],
        true,
    )));
    targets.send(Ok(PriceTargets::new(
        120.0,
        110.0,
        100.0,
        3,
        start.date_naive(),
        (start + TimeDelta::days(365)).date_naive(),
    )));
    sink.push_tick(tick);
    load(&next, &mut chart, &mut frames, 7.0);
    assert_eq!(closes(&chart, SeriesId::MAIN), [7.0; 5]);
    assert_eq!(chart.trades(SeriesId::MAIN).count(), 0);
    assert!(chart.order_book(SeriesId::MAIN).is_none());
    assert!(
        chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .next()
            .is_none()
    );
    assert!(chart.price_targets().is_none());
    assert!(prepared_view(&chart).bar_at(start).is_none());
}

#[test]
fn set_feed_resets_immediately_and_preserves_options_while_paused_or_inactive() {
    for (paused, inactive) in [(false, false), (true, false), (false, true)] {
        let old = Arc::new(Script::default());
        let mut chart = ChartBuilder::new()
            .with_feed(old.clone())
            .with_symbol("A")
            .with_initial_bars(37)
            .build();
        chart.set_extended_hours(true);
        chart.set_conversion(Some("USD".into()));
        chart.set_dividend_adjusted(true);
        chart.set_legend_badge(Some("bytes://host-badge".into()));
        chart.set_legend_statuses(&[crate::widget::LegendStatus::new(
            "Host status",
            egui::Color32::WHITE,
            egui::Color32::DARK_RED,
        )]);
        let bubbles = VolumeBubblesConfig {
            opacity: 77,
            min_size_filter: Some(12.0),
            ..Default::default()
        };
        chart.set_volume_bubbles(Some(bubbles.clone()));
        chart.set_liquidity_heatmap(true);
        chart.set_order_book_visible(false);
        let heatmap = crate::chart::overlays::LiquidityHeatmapConfig {
            brightness: 0.5,
            lower_cutoff: 10.0,
            full_intensity_size: Some(999.0),
        };
        chart.set_liquidity_heatmap_config(heatmap);
        let book_view = crate::chart::overlays::OrderBookView {
            quantity_unit: Some("lots".into()),
            ..Default::default()
        };
        chart.set_order_book_view(book_view.clone());
        let depth_config = crate::data::DepthHistoryConfig {
            rows: 12,
            columns: 8,
            ticks_per_row: 2,
            ..Default::default()
        };
        chart.set_depth_history(Some(DepthHistory::new(depth_config.clone()).unwrap()));
        chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(123));
        let mut frames = Frames::new();
        load(&old, &mut chart, &mut frames, 100.0);
        old.trades
            .lock()
            .unwrap()
            .remove(0)
            .send(Err(FeedError::Network("old source".into())));
        let time = bars(1)[0].time;
        old.sink().push_quote(crate::model::Level1Quote {
            ts: time,
            symbol: "A".into(),
            bid: 99.0,
            ask: 101.0,
            bid_size: 2.0,
            ask_size: 3.0,
        });
        frames.run(&mut chart);
        assert!(chart.quote.is_some());
        assert!(chart.feed.last_error.is_some());
        let mut history = DepthHistory::new(depth_config.clone()).unwrap();
        history.push(crate::data::DepthColumn {
            time,
            span: TimeDelta::seconds(1),
            bids: vec![(50, 2.0)],
            ..Default::default()
        });
        chart.set_depth_history(Some(history));
        chart.set_feed_paused(paused);
        if inactive {
            chart.user_action(|chart| chart.remove_series(SeriesId::MAIN));
        }
        let next = Arc::new(Script::default());
        chart.set_feed(next.clone());
        assert!(chart.state.data().bars.is_empty());
        assert!(chart.instrument().is_none());
        assert!(chart.quote.is_none());
        assert!(chart.session_quote.is_none());
        assert_eq!(chart.feed, crate::data::FeedState::default());
        assert_eq!(chart.symbol(), "A");
        assert_eq!(chart.timeframe(), Some(Timeframe::Min1));
        assert!(chart.extended_hours() && chart.dividend_adjusted());
        assert_eq!(chart.conversion.as_deref(), Some("USD"));
        assert!(matches!(
            chart.legend_badge.as_ref(),
            Some(egui::ImageSource::Uri(uri)) if uri == "bytes://host-badge"
        ));
        assert_eq!(
            chart.legend_statuses,
            [(
                "Host status".into(),
                egui::Color32::WHITE,
                egui::Color32::DARK_RED
            )]
        );
        let link = chart.feed_link.as_ref().unwrap();
        assert_eq!(
            (link.host_paused, link.paused),
            (paused, paused || inactive)
        );
        assert_eq!(link.initial_bars, 37);
        assert!(link.bubbles.is_some() && link.heatmap.is_some());
        assert_eq!(link.book_visible, Some(false));
        assert!(link.depth_history.is_some());
        assert_eq!(link.bubbles_config, bubbles);
        assert_eq!(link.bubbles.as_ref().unwrap().config, bubbles);
        assert_eq!(link.heatmap_config, heatmap);
        assert_eq!(link.book_view, book_view);
        assert_eq!(link.depth_history.as_ref().unwrap().config(), &depth_config);
        assert!(
            link.depth_history
                .as_ref()
                .unwrap()
                .columns()
                .next()
                .is_none()
        );
        if !inactive {
            assert_eq!(chart.bar_limit(SeriesId::MAIN).unwrap().get(), 123);
        }
        frames.run(&mut chart);
        if paused || inactive {
            assert!(next.calls().is_empty());
        }
        if !inactive {
            chart.set_feed_paused(false);
            frames.run(&mut chart);
            let requests: Vec<_> = next
                .calls()
                .into_iter()
                .filter_map(|call| match call {
                    Call::History(request) => Some(request),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let request = &requests[0];
            assert_eq!(request.count, 37);
            assert!(request.extended_hours && request.adjusted);
            assert_eq!(request.conversion.as_deref(), Some("USD"));
            next.resolve()
                .send(Err(FeedError::SymbolNotFound("A".into())));
            next.page().send(page(Vec::new(), true));
            frames.run(&mut chart);
            assert!(chart.bars(SeriesId::MAIN).is_empty());
            assert!(chart.instrument().is_none());
        }
    }
    let feed = Arc::new(Script::default());
    let mut chart = Chart::new(BarData::from_bars(bars(5)));
    chart.set_symbol("A");
    chart.set_feed(feed.clone());
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    assert_eq!(chart.timeframe(), Some(Timeframe::default()));
    let mut frames = Frames::new();
    load(&feed, &mut chart, &mut frames, 9.0);
    assert_eq!(closes(&chart, SeriesId::MAIN), [9.0; 5]);
    assert!(
        feed.calls()
            .iter()
            .any(|call| matches!(call, Call::History(request) if request.count == 1000))
    );
}

#[test]
fn set_feed_clears_price_based_elements_before_a_paused_frame() {
    for kind in [ChartType::Candles, ChartType::Renko] {
        let old = Arc::new(Script::default());
        let mut chart = fed(&old, Some("A"));
        chart.set_chart_type(kind);
        let mut frames = Frames::new();
        frames.run(&mut chart);
        old.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        old.page().send(page(bars(50), true));
        for _ in 0..3 {
            frames.run(&mut chart);
        }
        assert!(!chart.state.shown().is_empty());
        #[cfg(feature = "drawings")]
        let drawing_id = {
            use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(2.0, 102.0)];
            let id = chart.drawings_mut().add_drawing(drawing.clone()).unwrap();
            assert!(chart.drawings_mut().undo());
            assert!(chart.drawings_mut().redo());
            chart.drawings_mut().add_drawing(drawing).unwrap();
            assert!(chart.drawings_mut().undo());
            id
        };
        chart.set_feed_paused(true);
        chart.set_feed(Arc::new(Script::default()));
        assert!(chart.state.shown().is_empty(), "{kind:?}");
        let frozen = chart.export_snapshot(&frames.ctx, &Default::default());
        assert!(frozen.state.shown().is_empty());
        #[cfg(feature = "drawings")]
        {
            assert_eq!(chart.drawings().drawings[0].id, drawing_id);
            assert_eq!(chart.drawings().drawings[0].chart_points.len(), 2);
            assert!(!chart.drawings_mut().undo());
            assert!(!chart.drawings_mut().redo());
            let drawing = chart.drawings().drawings[0].clone();
            for _ in 0..150 {
                chart.drawings_mut().add_drawing(drawing.clone()).unwrap();
            }
            for _ in 0..100 {
                assert!(chart.drawings_mut().undo());
            }
            assert!(!chart.drawings_mut().undo());
            assert_eq!(chart.drawings().drawings.len(), 51);
            assert_eq!(chart.drawings().drawings[0].id, drawing_id);
        }
    }
}

#[cfg(feature = "studies")]
#[test]
fn set_feed_clears_profile_and_trade_inputs_but_preserves_host_histories() {
    use crate::studies::{IndicatorFactory, OrderFlowStudy, StudyDataRequest};
    let old = Arc::new(Script::default());
    old.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = fed(&old, Some("A"));
    let profile = chart.add_indicator(IndicatorFactory::new().create("Vol Profile FR").unwrap());
    let delta = chart.add_indicator(Box::new(OrderFlowStudy::execution_delta(false)));
    let comparison = chart.add_feed_series(SeriesOptions {
        name: "B".into(),
        ..Default::default()
    });
    let host = chart.add_comparison("host", egui::Color32::RED);
    chart.set_data(host, bars(3));
    let override_id = chart.add_feed_series(SeriesOptions {
        name: "override".into(),
        ..Default::default()
    });
    chart.set_study_data(
        StudyDataRequest {
            symbol: "override".into(),
            timeframe: Some(Timeframe::Min1),
            metric: None,
        },
        bars(2),
        None,
    );
    let mut frames = Frames::new();
    frames.run(&mut chart);
    for reply in old.resolves.lock().unwrap().drain(..) {
        reply.send(Ok(Instrument::new("A").with_tick_size(1.0)));
    }
    for reply in old.pages.lock().unwrap().drain(..) {
        reply.send(page(bars(5), true));
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    for reply in old.pages.lock().unwrap().drain(..) {
        reply.send(page(bars(5), true));
    }
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::minutes(1);
    let tick = Tick::new(start, 100.0, 3.0, Some(crate::model::OrderSide::Buy));
    old.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(std::slice::from_ref(&tick), cutoff, true)));
    for reply in old.trades.lock().unwrap().drain(..) {
        reply.send(Ok(TradePage::new(vec![tick.clone()], true)));
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    let input = chart
        .indicators()
        .find(|(id, _)| *id == profile)
        .unwrap()
        .1
        .order_flow_inputs()[0]
        .clone();
    assert_eq!(
        input
            .footprint_snapshot(start)
            .bars(..)
            .map(|bar| bar.volume())
            .sum::<f64>(),
        3.0
    );
    assert!(!chart.bars(comparison).is_empty());
    chart.set_replay_time(Some(start + TimeDelta::seconds(30)));
    let next = Arc::new(Script::default());
    next.prepared_footprints.store(true, Ordering::Relaxed);
    chart.set_feed(next.clone());
    assert!(input.footprint_snapshot(start).bars(..).next().is_none());
    assert_eq!(input.footprint_snapshot(start).tick_size, None);
    assert_eq!(input.footprint_snapshot(start).cutoff, None);
    assert_eq!(input.footprint_snapshot(start).complete_from, None);
    assert!(chart.bars(comparison).is_empty());
    assert_eq!(chart.bars(host), bars(3));
    assert_eq!(chart.bars(override_id), bars(2));
    for _ in 0..2 {
        frames.run(&mut chart);
    }
    for reply in next.resolves.lock().unwrap().drain(..) {
        reply.send(Ok(Instrument::new("A").with_tick_size(0.5)));
    }
    for reply in next.pages.lock().unwrap().drain(..) {
        reply.send(page(bars(5), true));
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    for reply in next.pages.lock().unwrap().drain(..) {
        reply.send(page(bars(2), true));
    }
    let fresh = Tick::new(
        start + TimeDelta::seconds(40),
        100.5,
        7.0,
        Some(crate::model::OrderSide::Buy),
    );
    for reply in next.footprints.lock().unwrap().drain(..) {
        reply.send(Ok(FootprintPage::new(Vec::new(), start, true)));
    }
    for reply in next.trades.lock().unwrap().drain(..) {
        reply.send(Ok(TradePage::new(vec![fresh.clone()], true)));
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    let snapshot = input.footprint_snapshot(start);
    assert_eq!(snapshot.bars(..).map(|bar| bar.volume()).sum::<f64>(), 7.0);
    assert_eq!(snapshot.tick_size, Some(0.5));
    assert_eq!(snapshot.cutoff, None);
    assert_eq!(
        snapshot
            .bars(start..=start)
            .next()
            .unwrap()
            .levels
            .get(&201)
            .unwrap()
            .ask,
        7.0
    );
    assert_eq!(
        chart
            .indicators()
            .find(|(id, _)| *id == delta)
            .unwrap()
            .1
            .values()
            .value(0, 0),
        Some(7.0)
    );
    assert_eq!(chart.bars(comparison), bars(2));
    assert_eq!(chart.bars(override_id), bars(2));
}

fn driver() -> Arc<crate::data::ReplayDriver> {
    use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
    let start = bars(1)[0].time;
    let events = [10, 70, 130]
        .into_iter()
        .enumerate()
        .map(|(id, second)| {
            let at = start + TimeDelta::seconds(second);
            ReplayEvent::new(
                "A",
                at,
                id as u64,
                MarketEvent::Trade(Tick::new(at, 20.0, 1.0, None)),
            )
        })
        .collect();
    let driver = Arc::new(
        ReplayDriver::new(events, vec![Instrument::new("A").with_tick_size(1.0)]).unwrap(),
    );
    driver.set_speed(0.0).unwrap();
    driver.seek(start + TimeDelta::seconds(75));
    driver
}

#[test]
fn set_feed_exits_replay_locally_and_cannot_restore_old_undo_bars() {
    for market in [false, true] {
        for paused in [false, true] {
            let old = Arc::new(Script::default());
            let mut chart = ChartBuilder::new()
                .with_feed(old.clone())
                .with_symbol("A")
                .with_initial_bars(37)
                .build();
            let mut frames = Frames::new();
            load(&old, &mut chart, &mut frames, 100.0);
            chart.set_feed_paused(paused);
            let replay = driver();
            let mut sibling = fed(&Arc::new(Script::default()), Some("A"));
            sibling.set_market_replay(Some(replay.clone()));
            if market {
                chart.set_market_replay(Some(replay.clone()));
            } else {
                chart.set_replay_time(Some(bars(1)[0].time + TimeDelta::seconds(75)));
            }
            for _ in 0..3 {
                frames.run(&mut chart);
                frames.run(&mut sibling);
            }
            chart.user_action(|chart| chart.config_mut().show_grid = false);
            old.calls();
            let shown = chart.bars(SeriesId::MAIN).to_vec();
            chart.set_feed(old.clone());
            assert!(chart.can_undo());
            assert_eq!(chart.bars(SeriesId::MAIN), shown);
            assert!(chart.replay_time().is_some());
            assert!(old.calls().is_empty());
            let at = replay.time();
            let next = Arc::new(Script::default());
            chart.set_feed(next.clone());
            assert!(chart.replay_time().is_none());
            assert!(chart.bars(SeriesId::MAIN).is_empty());
            assert!(!chart.undo() && !chart.redo());
            assert!(!chart.config().show_grid);
            assert_eq!(chart.feed_link.as_ref().unwrap().initial_bars, 37);
            assert_eq!(chart.feed_link.as_ref().unwrap().host_paused, paused);
            assert_eq!(replay.time(), at);
            assert!(sibling.replay_driver().is_some());
            replay.seek(bars(1)[0].time + TimeDelta::seconds(135));
            chart.set_market_replay(None);
            chart.set_replay_time(None);
            frames.run(&mut chart);
            frames.run(&mut sibling);
            assert!(chart.bars(SeriesId::MAIN).is_empty());
            assert_eq!(closes(&sibling, SeriesId::MAIN), [20.0; 3]);
            if paused {
                assert!(next.calls().is_empty());
            }
        }
    }
    // A removed comparison and a removed main series retain bars in undo, not in the chart.
    for main in [false, true] {
        let mut chart = fed(&Arc::new(Script::default()), Some("A"));
        chart.set_data(SeriesId::MAIN, bars(5));
        let id = if main {
            SeriesId::MAIN
        } else {
            let id = chart.add_feed_series(SeriesOptions {
                name: "B".into(),
                ..Default::default()
            });
            chart.set_data(id, bars(3));
            id
        };
        chart.user_action(|chart| chart.remove_series(id));
        assert!(chart.can_undo());
        chart.set_feed(Arc::new(Script::default()));
        assert!(!chart.undo() && !chart.redo());
        assert!(chart.state.data().bars.is_empty());
        assert!(chart.bars(id).is_empty());
    }
}

#[test]
fn set_feed_keeps_frozen_shared_owners_without_exporting_old_source_again() {
    let (old, mut chart, mut frames) = prepared_chart();
    let start = bars(1)[0].time;
    let cutoff = start + TimeDelta::minutes(1);
    let saved = prepared_page(&[Tick::new(start, 100.0, 3.0, None)], cutoff, true);
    let FootprintBars::Owned(saved) = saved.bars else {
        unreachable!()
    };
    let owner = Arc::new(saved);
    let weak = Arc::downgrade(&owner);
    let row = owner[0].levels.get(&100).unwrap() as *const FootprintLevel;
    old.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(FootprintPage::from_shared(owner, cutoff, true)));
    frames.run(&mut chart);
    let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
    chart.set_feed(Arc::new(Script::default()));
    let mut after = chart.export_snapshot(&frames.ctx, &Default::default());
    assert!(after.bars(SeriesId::MAIN).is_empty());
    frames.run(&mut after);
    assert!(after.bars(SeriesId::MAIN).is_empty());
    frames.run(&mut frozen);
    let view = prepared_view(&frozen);
    assert_eq!(
        view.bar_at(start).unwrap().levels.get(&100).unwrap() as *const FootprintLevel,
        row
    );
    assert_eq!(view.bar_at(start).unwrap().volume(), 3.0);
    drop(chart);
    assert!(weak.upgrade().is_some());
    drop(frozen);
    assert!(weak.upgrade().is_none());
}

#[cfg(feature = "studies")]
#[test]
fn set_feed_clears_calculated_study_values_outside_replay() {
    let old = Arc::new(Script::default());
    let mut chart = fed(&old, Some("A"));
    chart.add_indicator(Box::new(crate::studies::OrderFlowStudy::execution_delta(
        false,
    )));
    let mut frames = Frames::new();
    load(&old, &mut chart, &mut frames, 100.0);
    let tick = Tick::new(
        bars(1)[0].time,
        100.0,
        3.0,
        Some(crate::model::OrderSide::Buy),
    );
    for reply in old.trades.lock().unwrap().drain(..) {
        reply.send(Ok(TradePage::new(vec![tick.clone()], true)));
    }
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.replay_time(), None);
    let values = chart.indicators().next().unwrap().1.values();
    assert!(!values.is_empty());
    assert_eq!(values.value(0, 0), Some(3.0));
    chart.set_feed(Arc::new(Script::default()));
    assert!(chart.indicators().next().unwrap().1.values().is_empty());
}

/// Two charts on one feed, same symbol and interval, still get distinct identities, and each
/// chart's history reply and live sink report the same one.
#[test]
fn history_replies_and_sinks_share_one_view_id_per_chart() {
    let feed = Arc::new(Script::default());
    let mut first = fed(&feed, Some("A"));
    let mut second = fed(&feed, Some("A"));
    let mut frames = Frames::new();
    frames.run(&mut first);
    frames.run(&mut second);
    for _ in 0..2 {
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    }
    frames.run(&mut first);
    frames.run(&mut second);
    let replies = [feed.page(), feed.page()];
    let history_ids = replies.each_ref().map(|reply| reply.view_id());
    for reply in replies {
        reply.send(page(bars(5), true));
    }
    for _ in 0..2 {
        frames.run(&mut first);
        frames.run(&mut second);
    }
    let sink_ids: Vec<_> = feed
        .sinks
        .lock()
        .unwrap()
        .iter()
        .map(|sink| sink.view_id())
        .collect();
    assert_eq!(
        sink_ids, history_ids,
        "each chart's sink follows its history"
    );
    assert_ne!(history_ids[0], history_ids[1], "two charts share no id");
}
