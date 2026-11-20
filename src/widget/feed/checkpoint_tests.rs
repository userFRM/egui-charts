use super::*;
use crate::widget::TradeReplayCheckpointError as Error;
use std::num::NonZeroU64;

#[derive(Default)]
struct ReplayFeed(Mutex<Option<Sink>>);
impl Datafeed for ReplayFeed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol).with_tick_size(0.25)));
    }
    fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
        reply.send(page(Vec::new(), true));
    }
    fn subscribe(&self, subscription: Subscription) {
        *self.0.lock().unwrap() = Some(subscription.sink);
    }
}

fn ready(lots: u64) -> (Chart, Sink, Frames) {
    let feed = Arc::new(ReplayFeed::default());
    let mut chart = ChartBuilder::new()
        .with_feed(feed.clone())
        .with_symbol("A")
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
    sink.push_tick(trade(99, 13.));
    dest_frames.run(&mut dest);
    for _ in 0..2 {
        dest.restore_trade_replay_checkpoint(&mark).unwrap();
        assert_eq!(dest.feed, reference.feed);
        sink.push_tick(trade(1, 2047.)); // The checkpoint must restore deduplication, not just the visible candle.
        sink.push_tick(trade(2, 7.));
        for _ in 0..6 {
            dest_frames.run(&mut dest);
        }
        assert_eq!(dest.bars(SeriesId::MAIN), reference.bars(SeriesId::MAIN));
        assert!(
            dest.trades(SeriesId::MAIN)
                .eq(reference.trades(SeriesId::MAIN))
        );
        assert_eq!(dest.feed.repeated_ticks, reference.feed.repeated_ticks + 1);
        #[cfg(feature = "studies")]
        for ((_, left), (_, right)) in dest.indicators().zip(reference.indicators()) {
            assert_eq!(
                left.values().columns().len(),
                right.values().columns().len()
            );
            for bar in 0..2 {
                for line in 0..right.values().columns().len() {
                    assert_eq!(
                        left.values().value(line, bar).map(f64::to_bits),
                        right.values().value(line, bar).map(f64::to_bits)
                    );
                }
            }
        }
    }
}
#[test]
fn refusals_leave_chart_and_next_sink_allocation_unchanged() {
    let (mut chart, sink, mut frames) = ready(2048);
    sink.push_tick(trade(1, 2047.));
    frames.run(&mut chart);
    let mark = chart
        .capture_trade_replay_checkpoint(chart.plan_trade_replay_checkpoint().unwrap())
        .unwrap();
    let stale = chart.plan_trade_replay_checkpoint().unwrap();
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
