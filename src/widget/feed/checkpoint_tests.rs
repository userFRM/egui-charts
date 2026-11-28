use super::*;
use crate::widget::TradeReplayCheckpointError as Error;
use std::num::NonZeroU64;

#[derive(Default)]
struct ReplayFeed(Mutex<Option<Sink>>, std::sync::atomic::AtomicUsize);
impl Datafeed for ReplayFeed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol).with_tick_size(0.25)));
    }
    fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
        self.1.fetch_add(1, Ordering::Relaxed);
        reply.send(page(Vec::new(), true));
    }
    fn subscribe(&self, subscription: Subscription) {
        *self.0.lock().unwrap() = Some(subscription.sink);
    }
}

fn ready(lots: u64) -> (Chart, Sink, Frames) {
    ready_with_type(lots, ChartType::Candles)
}

fn ready_with_type(lots: u64, chart_type: ChartType) -> (Chart, Sink, Frames) {
    let feed = Arc::new(ReplayFeed::default());
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("A")
        .with_chart_type(chart_type)
        .with_timeframe(Timeframe::Volume(NonZeroU64::new(lots).unwrap()))
        .build();
    #[cfg(feature = "studies")]
    {
        chart.add_indicator(Box::new(crate::studies::EMA::new(3)));
        chart.add_indicator(Box::new(crate::studies::OrderFlowStudy::new(
            crate::data::OrderFlowKind::TradeRate {
                volume: true,
                seconds: 2,
                instant: 1,
            },
        )));
    }
    let mut frames = Frames::new();
    for _ in 0..6 {
        frames.run(&mut chart);
    }
    let sink = feed.0.lock().unwrap().clone().unwrap();
    (chart, sink, frames)
}
fn trade(id: u64, size: f64) -> Tick {
    Tick::new(
        DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        100.0 + id as f64,
        size,
        Some(crate::model::OrderSide::Buy),
    )
    .with_id(id)
}

#[cfg(feature = "studies")]
#[test]
fn chart_volume_without_an_average_keeps_checkpoints_at_any_length() {
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 10.0));
    frames.run(&mut chart);
    let id = chart
        .indicators()
        .find(|(_, study)| study.is_volume())
        .unwrap()
        .0;
    for length in [9, 20] {
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("period", crate::studies::InputValue::Length(length))
            .unwrap();
        assert!(
            chart.plan_trade_replay_checkpoint().is_ok(),
            "length {length}"
        );
    }
}

#[test]
fn resume_inside_shared_timestamp_volume_split() {
    let (mut reference, source, mut frames) = ready(2048);
    source.push_tick(trade(1, 2047.));
    frames.run(&mut reference);
    let plan = reference.plan_trade_replay_checkpoint().unwrap();
    let mark = reference.capture_trade_replay_checkpoint(plan).unwrap();
    source.push_tick(trade(2, 7.));
    // The UI may schedule one full study calculation per frame.
    for _ in 0..6 {
        frames.run(&mut reference);
    }
    assert_eq!(
        reference
            .bars(SeriesId::MAIN)
            .iter()
            .map(|b| b.volume)
            .collect::<Vec<_>>(),
        [2048., 6.]
    );
    let (mut dest, sink, mut dest_frames) = ready(2048);
    let mut old = trade(99, 13.);
    old.ts = DateTime::from_timestamp(1_699_999_000, 0).unwrap();
    sink.push_tick(old);
    dest_frames.run(&mut dest);
    for _ in 0..2 {
        dest.restore_trade_replay_checkpoint(&mark).unwrap();
        assert_eq!(dest.bars(SeriesId::MAIN).len(), 1);
        assert_eq!(dest.bars(SeriesId::MAIN)[0].volume, 2047.);
        sink.push_tick(trade(1, 2047.)); // The checkpoint must restore deduplication, not just the visible candle.
        sink.push_tick(trade(2, 7.));
        for _ in 0..6 {
            dest_frames.run(&mut dest);
        }
        assert_replay_outputs(&dest, &reference);
        assert_eq!(dest.feed.repeated_ticks, reference.feed.repeated_ticks + 1);
    }
}

fn assert_replay_outputs(chart: &Chart, reference: &Chart) {
    assert_eq!(chart.bars(SeriesId::MAIN), reference.bars(SeriesId::MAIN));
    assert!(
        chart
            .trades(SeriesId::MAIN)
            .eq(reference.trades(SeriesId::MAIN))
    );
    #[cfg(feature = "studies")]
    for ((_, left), (_, right)) in chart.indicators().zip(reference.indicators()) {
        assert_eq!(
            left.values().columns().len(),
            right.values().columns().len()
        );
        for bar in 0..reference.bars(SeriesId::MAIN).len() {
            for line in 0..right.values().columns().len() {
                assert_eq!(
                    left.values().value(line, bar).map(f64::to_bits),
                    right.values().value(line, bar).map(f64::to_bits)
                );
            }
        }
    }
}

#[test]
fn footprint_continues_same_time_split_after_retained_rows_are_evicted() {
    use crate::model::OrderSide::{Buy, Sell};

    let (mut chart, sink, mut frames) = ready_with_type(10, ChartType::VolumeFootprint);
    let (mut reference, control, mut control_frames) =
        ready_with_type(10, ChartType::VolumeFootprint);
    let at = trade(0, 1.0).ts;
    for (chart, sink, frames) in [
        (&mut chart, &sink, &mut frames),
        (&mut reference, &control, &mut control_frames),
    ] {
        chart.feed_link.as_mut().unwrap().trades = Trades::new(6);
        // Evict the first bars before capture, independently of the checkpoint implementation.
        for id in 1..=7 {
            let mut tick = trade(id, 10.0);
            tick.ts = at - TimeDelta::seconds(8 - id as i64);
            sink.push_tick(tick);
        }
        sink.push_tick(Tick::new(at, 100.0, 4.0, Some(Sell)).with_id(8));
        sink.push_tick(Tick::new(at, 100.0, 5.0, Some(Buy)).with_id(9));
        for _ in 0..6 {
            frames.run(chart);
        }
    }
    let before = prepared_view(&chart);
    assert!(before.bar_at(at - TimeDelta::seconds(7)).is_none());
    let forming = before.bar_at(at).unwrap();
    assert_eq!(
        (forming.volume(), forming.min_delta, forming.max_delta),
        (9.0, -4.0, 1.0)
    );
    let saved_rows: Vec<_> = before
        .bars(..)
        .map(crate::data::FootprintBar::copy_from)
        .collect();
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    let next = Tick::new(at, 101.0, 3.0, Some(Sell)).with_id(10);
    control.push_tick(next.clone());
    for _ in 0..6 {
        control_frames.run(&mut reference);
    }

    // All rows from the mark leave its source. A mark must not borrow that source's live rows.
    for id in 11..=18 {
        let mut tick = trade(id, 10.0);
        tick.ts = at + TimeDelta::seconds(id as i64);
        sink.push_tick(tick);
    }
    frames.run(&mut chart);
    assert!(prepared_view(&chart).bar_at(at).is_none());
    chart.restore_trade_replay_checkpoint(&mark).unwrap();
    assert!(
        prepared_view(&chart)
            .bars(..)
            .eq(saved_rows.iter().map(crate::data::FootprintBarView::from))
    );
    sink.push_tick(next);
    for _ in 0..6 {
        frames.run(&mut chart);
    }
    assert_replay_outputs(&chart, &reference);
    let view = prepared_view(&chart);
    let expected = prepared_view(&reference);
    assert!(view.bars(..).eq(expected.bars(..)));
    for bar in chart.bars(SeriesId::MAIN) {
        assert_eq!(view.complete_at(bar.time), expected.complete_at(bar.time));
    }
    let closed = view.bar_at(at).unwrap();
    let open = view.bars(..).next_back().unwrap();
    assert_ne!(open.time, closed.time);
    assert_eq!((closed.volume(), open.volume()), (10.0, 2.0));
    let level = view.level_of(101.0).unwrap();
    assert_eq!(closed.levels.get(&level).unwrap().bid, 1.0);
    assert_eq!(open.levels.get(&level).unwrap().bid, 2.0);
    assert_eq!((closed.min_delta, closed.max_delta), (-4.0, 1.0));
    assert_eq!((open.min_delta, open.max_delta), (-2.0, 0.0));
}

#[test]
fn incompatible_footprint_grouping_preserves_destination_allocation() {
    let (mut source, source_sink, mut source_frames) =
        ready_with_type(10, ChartType::VolumeFootprint);
    let (mut dest, sink, mut frames) = ready_with_type(10, ChartType::VolumeFootprint);
    for (chart, ticks) in [(&mut source, 1), (&mut dest, 4)] {
        chart.set_chart_type_params(crate::model::ChartTypeParams {
            footprint_row_ticks: ticks,
            footprint_row_atr: false,
            ..Default::default()
        });
    }
    source_sink.push_tick(trade(1, 9.0));
    source_frames.run(&mut source);
    let mark = source
        .capture_trade_replay_checkpoint(source.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    sink.push_tick(trade(2, 8.0));
    frames.run(&mut dest);
    assert!(matches!(
        dest.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    assert_footprint_continuation(&mut dest, &sink, &mut frames);
}

#[test]
fn shared_footprint_owner_without_a_byte_bound_refuses_capture() {
    use crate::data::{FootprintBar, FootprintBars, FootprintState};

    let (mut chart, sink, mut frames) = ready_with_type(10, ChartType::VolumeFootprint);
    sink.push_tick(trade(2, 8.0));
    frames.run(&mut chart);
    let plan = chart.plan_trade_replay_checkpoint().unwrap();

    // Prepared pages are normally requested only on time intervals. Attach just their
    // opaque owner so the volume clock, live rows, sink and trade position stay identical.
    let (feed, _, _) = prepared_chart();
    let mut request = footprint_requests(&feed).pop().unwrap();
    let view = prepared_view(&chart);
    request.tick_size = view.tick_size();
    let mut old = FootprintBar::copy_from(view.bars(..).next().unwrap());
    let at = request.clock.bar_open(old.time - TimeDelta::minutes(2));
    old.time = at;
    let page = FootprintBars::Shared(Arc::new(vec![old]));
    let mut shared = FootprintState::from_page(&request, page, true).unwrap();
    let display = chart
        .feed_link
        .as_mut()
        .unwrap()
        .footprint
        .as_mut()
        .unwrap();
    std::mem::swap(&mut display.live, &mut shared.live);
    *display = shared;

    assert!(matches!(
        chart.plan_trade_replay_checkpoint(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        chart.capture_trade_replay_checkpoint(plan),
        Err(Error::StalePlan)
    ));
    assert_eq!(prepared_view(&chart).bar_at(at).unwrap().volume(), 8.0);
    assert_footprint_continuation(&mut chart, &sink, &mut frames);
}

fn assert_footprint_continuation(chart: &mut Chart, sink: &Sink, frames: &mut Frames) {
    let at = trade(2, 8.0).ts;
    assert_eq!(prepared_view(chart).bar_at(at).unwrap().volume(), 8.0);
    assert_eq!(chart.bars(SeriesId::MAIN).last().unwrap().volume, 8.0);
    assert!(chart.trades(SeriesId::MAIN).eq([&trade(2, 8.0)]));
    sink.push_tick(trade(3, 3.0));
    frames.run(chart);
    let bars = chart.bars(SeriesId::MAIN);
    assert_eq!(bars.len(), 2);
    let view = prepared_view(chart);
    for (bar, volume) in bars.iter().zip([10.0, 1.0]) {
        assert_eq!(bar.volume, volume);
        assert_eq!(view.bar_at(bar.time).unwrap().volume(), volume);
    }
}

#[test]
fn restoring_replay_counters_preserves_current_subscription_health() {
    use crate::data::FeedStatus;

    let (mut chart, sink, mut frames) = ready(2048);
    sink.set_status(FeedStatus::Live);
    sink.push_tick(trade(1, 2047.));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();

    let error = FeedError::Network("connection closed".into());
    sink.set_status(FeedStatus::Disconnected);
    sink.symbols_changed();
    sink.report_error(error.clone());
    sink.push_tick(trade(1, 2047.));
    frames.run(&mut chart);
    assert_eq!(chart.feed_state(SeriesId::MAIN).unwrap().repeated_ticks, 1);

    chart.restore_trade_replay_checkpoint(&mark).unwrap();
    let state = chart.feed_state(SeriesId::MAIN).unwrap();
    assert_eq!(state.status, Some(FeedStatus::Disconnected));
    assert_eq!(state.symbol_catalog_revision, 1);
    assert_eq!(state.last_error.as_ref(), Some(&error));
    assert_eq!(state.repeated_ticks, 0);
}

#[test]
fn restoring_history_completeness_avoids_reloading_evicted_bars() {
    let feed = Arc::new(ReplayFeed::default());
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("A")
        .with_timeframe(Timeframe::Volume(NonZeroU64::new(10).unwrap()))
        .build();
    chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(30));
    let mut frames = Frames::new();
    for _ in 0..6 {
        frames.run(&mut chart);
    }
    let sink = feed.0.lock().unwrap().clone().unwrap();
    for id in 0..25 {
        let mut tick = trade(id, 10.);
        tick.ts += TimeDelta::seconds(id as i64);
        sink.push_tick(tick);
    }
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 25);
    assert!(chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
    let complete = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    for id in 25..35 {
        let mut tick = trade(id, 10.);
        tick.ts += TimeDelta::seconds(id as i64);
        sink.push_tick(tick);
    }
    frames.run(&mut chart);
    // Eviction temporarily keeps the volume display's old values until its throttled rebuild.
    for _ in 0..20 {
        frames.run(&mut chart);
    }
    // Retention evicts in chunks, keeping limit - limit / 8 bars.
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 27);
    assert!(!chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
    let evicted = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();

    let requests = feed.1.load(Ordering::Relaxed);
    chart.restore_trade_replay_checkpoint(&complete).unwrap();
    assert!(chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
    for _ in 0..3 {
        frames.run(&mut chart);
    }
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 25);
    assert_eq!(feed.1.load(Ordering::Relaxed), requests);

    // The reverse restore must not claim completeness for a tape that already evicted bars.
    chart.restore_trade_replay_checkpoint(&evicted).unwrap();
    assert!(!chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 27);
}

#[test]
fn refusals_leave_chart_and_next_sink_allocation_unchanged() {
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 2047.));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    let before_heartbeat = chart.plan_trade_replay_checkpoint().unwrap();
    sink.advance_time(trade(1, 2047.).ts + TimeDelta::seconds(1));
    frames.run(&mut chart);
    // It's a drain boundary again, but not the position authorized by the old plan.
    let stale = chart.plan_trade_replay_checkpoint().unwrap();
    assert!(matches!(
        chart.capture_trade_replay_checkpoint(before_heartbeat),
        Err(Error::StalePlan)
    ));
    sink.push_tick(trade(2, 7.));
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::NotDrained)
    ));
    frames.run(&mut chart);
    assert!(matches!(
        chart.capture_trade_replay_checkpoint(stale),
        Err(Error::StalePlan)
    ));
    sink.push_tick(trade(3, 3.));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN).last().unwrap().volume, 9.);
    let (mut other, other_sink, mut other_frames) = ready(10);
    other_sink.push_tick(trade(5, 8.));
    other_frames.run(&mut other);
    assert!(matches!(
        other.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    assert_eq!(other.bars(SeriesId::MAIN)[0].volume, 8.);
    other_sink.push_tick(trade(6, 3.));
    other_frames.run(&mut other);
    assert_eq!(
        other
            .bars(SeriesId::MAIN)
            .iter()
            .map(|b| b.volume)
            .collect::<Vec<_>>(),
        [10., 1.]
    );
    assert_eq!(
        other
            .trades(SeriesId::MAIN)
            .filter_map(|t| t.id)
            .collect::<Vec<_>>(),
        [5, 6]
    );
}

#[test]
fn another_compatible_chart_can_use_the_mark_but_not_the_plan() {
    let (mut source, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 2047.));
    frames.run(&mut source);
    let plan = source.plan_trade_replay_checkpoint().unwrap();
    let (dest, _, _) = ready(2048);
    assert!(matches!(
        dest.capture_trade_replay_checkpoint(plan),
        Err(Error::StalePlan)
    ));
}

#[test]
fn symbol_retention_and_execution_overlays_define_the_supported_boundary() {
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 10.0));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(8));
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    chart.set_bar_limit(SeriesId::MAIN, None);
    chart.set_volume_bubbles(Some(Default::default()));
    assert!(matches!(
        chart.plan_trade_replay_checkpoint(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Unsupported)
    ));
    chart.set_volume_bubbles(None);
    chart.set_symbol("B");
    for _ in 0..6 {
        frames.run(&mut chart);
    }
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    assert!(chart.bars(SeriesId::MAIN).is_empty());
}

#[cfg(feature = "studies")]
#[test]
fn study_keys_use_concrete_ordered_calculations_not_display_names() {
    use crate::studies::{CustomIndicator, EMA, InputValue, StudySource};
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 10.0));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    let rate = chart
        .indicators()
        .find(|(_, study)| study.name() == "Trade Rate")
        .unwrap()
        .0;
    chart
        .indicator_mut(rate)
        .unwrap()
        .set_input("seconds", InputValue::Length(5))
        .unwrap();
    frames.run(&mut chart);
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    chart
        .indicator_mut(rate)
        .unwrap()
        .set_input("seconds", InputValue::Length(2))
        .unwrap();
    frames.run(&mut chart);
    let ema = chart
        .indicators()
        .find(|(_, study)| study.name() == "EMA")
        .unwrap()
        .0;
    chart
        .indicator_mut(ema)
        .unwrap()
        .set_input("period", InputValue::Length(7))
        .unwrap();
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    chart
        .indicator_mut(ema)
        .unwrap()
        .set_input("period", InputValue::Length(3))
        .unwrap();
    chart
        .set_indicator_source(ema, StudySource::Price(crate::model::PriceSource::Open))
        .unwrap();
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    chart.remove_series(ema);
    let replacement = chart.add_indicator(Box::new(EMA::new(3)));
    // The parameter is restored, but the EMA now follows rather than precedes Trade Rate.
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    chart.remove_series(replacement);
    chart.add_indicator(Box::new(CustomIndicator::new("EMA", |bars| {
        vec![crate::studies::IndicatorValue::None; bars.len()]
    })));
    assert!(matches!(
        chart.plan_trade_replay_checkpoint(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Unsupported)
    ));
    assert_eq!(chart.bars(SeriesId::MAIN)[0].volume, 10.0);
}

#[test]
fn pending_resolution_and_changed_instrument_have_distinct_refusals() {
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 10.0));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    let feed = Arc::new(Script::default());
    chart.set_feed(feed.clone());
    frames.run(&mut chart);
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::NotDrained)
    ));
    feed.capabilities
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(Ok(FeedCapabilities::default()));
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(0.5)));
    feed.page().send(page(Vec::new(), true));
    frames.run(&mut chart);
    feed.trades
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(Err(FeedError::NotSupported));
    frames.run(&mut chart);
    assert!(matches!(
        chart.restore_trade_replay_checkpoint(&mark),
        Err(Error::Incompatible)
    ));
    assert!(chart.bars(SeriesId::MAIN).is_empty());
    feed.sink().push_tick(trade(1, 7.0));
    frames.run(&mut chart);
    assert_eq!(chart.bars(SeriesId::MAIN)[0].volume, 7.0);
}

#[test]
fn named_security_identifiers_count_against_the_hosts_capture_budget() {
    let (mut chart, _, _) = ready(2048);
    let without = chart.plan_trade_replay_checkpoint().unwrap().max_bytes();
    let label = "x".repeat(4096);
    let value = "y".repeat(8192);
    let payload_bytes = label.len() + value.len();
    let instrument = chart
        .feed_link
        .as_mut()
        .unwrap()
        .instrument
        .as_mut()
        .unwrap();
    instrument
        .identifiers
        .push((crate::model::IdentifierKind::Other(label), value));
    let with = chart.plan_trade_replay_checkpoint().unwrap().max_bytes();
    assert!(with >= without + payload_bytes);
}

#[cfg(feature = "studies")]
#[test]
fn planned_bytes_cover_retained_trades_volume_bars_and_footprint_rows() {
    for chart_type in [ChartType::Candles, ChartType::VolumeFootprint] {
        assert_planned_capture_bytes(chart_type);
    }
}

#[cfg(feature = "studies")]
fn assert_planned_capture_bytes(chart_type: ChartType) {
    use crate::widget::studies_data::tests::requested_bytes;

    // Make the tape, bars and (when enabled) retained footprint levels dwarf fixed costs.
    let (mut chart, sink, frames) = ready_with_type(8, chart_type);
    for batch in 0..32 {
        for id in batch * 128..(batch + 1) * 128 {
            let mut tick = trade(id, 1.0);
            tick.ts += TimeDelta::seconds(id as i64);
            sink.push_tick(tick);
        }
        chart.pump(&frames.ctx);
    }
    assert_eq!(chart.trades(SeriesId::MAIN).count(), 4096);
    let bars = chart.bars(SeriesId::MAIN);
    assert_eq!(bars.len(), 512);
    assert!(bars.iter().all(|bar| bar.volume == 8.0));
    if chart_type == ChartType::VolumeFootprint {
        assert_eq!(
            prepared_view(&chart)
                .bars(..)
                .map(|bar| bar.levels.len())
                .sum::<usize>(),
            4096
        );
    }

    let (requested, (bound, _mark)) = requested_bytes(|| {
        let plan = chart.plan_trade_replay_checkpoint().unwrap();
        let bound = plan.max_bytes();
        (bound, chart.capture_trade_replay_checkpoint(plan).unwrap())
    });
    assert!(
        requested <= bound,
        "requested {requested} bytes, planned {bound}"
    );
}
