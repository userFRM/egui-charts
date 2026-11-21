use super::*;
use crate::data::{HistoryPage, OrderEvent, RestingOrder};
use crate::model::{OrderBook, OrderBookLevel, Tick, Timeframe};
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::atomic::AtomicUsize;

/// Exact volume/delta splitting preserves total size, while range bars use only real fills.
#[test]
fn trade_bar_thresholds_survive_drains_and_keep_raw_fills() {
    use crate::model::OrderSide::{Buy, Sell};
    #[rustfmt::skip]
    let cases = [
        ("5v", vec![(100.0, 3.0, Buy), (102.0, 9.0, Sell)],
            vec![(100.0, 102.0, 100.0, 102.0, 5.0), (102.0, 102.0, 102.0, 102.0, 5.0), (102.0, 102.0, 102.0, 102.0, 2.0)]),
        ("5delta", vec![(100.0, 3.0, Buy), (101.0, 2.0, Sell), (102.0, 9.0, Buy)],
            vec![(100.0, 102.0, 100.0, 102.0, 9.0), (102.0, 102.0, 102.0, 102.0, 5.0)]),
        ("4r", vec![(100.0, 3.0, Buy), (100.5, 2.0, Sell), (101.0, 1.0, Buy), (99.0, 4.0, Sell)],
            vec![(100.0, 101.0, 100.0, 101.0, 6.0), (99.0, 99.0, 99.0, 99.0, 4.0)]),
    ];
    for (interval, trades, expected) in cases {
        let inbox = Arc::new(Inbox::default());
        let tf: Timeframe = interval.parse().unwrap();
        assert!(tf.is_trade_based());
        assert_eq!(tf.to_string(), interval);
        let sink = inbox.subscribe("A".into(), tf.into(), 100, Some(0.25));
        sink.set_server_time(at(0, 0));
        let mut bars = BTreeMap::new();
        let mut raw = Vec::new();
        for (i, (price, size, side)) in trades.iter().enumerate() {
            let trade = Tick::new(at(0, 0), *price, *size, Some(*side)).with_id(i as u64);
            sink.push_tick(trade.clone());
            sink.push_tick(trade);
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            bars.extend(updates.bars);
            raw.extend(updates.trades.iter().cloned());
        }
        let actual: Vec<_> = bars
            .values()
            .map(|(b, _)| (b.open, b.high, b.low, b.close, b.volume))
            .collect();
        assert_eq!(actual, expected, "{interval}");
        assert_eq!(raw.len(), trades.len());
        assert_eq!(
            bars.values().map(|(b, _)| b.volume).sum::<f64>(),
            trades.iter().map(|t| t.1).sum::<f64>()
        );
        assert!(crate::data::resample(&[], Timeframe::Min1, tf).is_none());
    }
}

/// Guards that one filter gives one answer: the book size study, the heatmap's depth columns and
/// the ladder's book (`OrderQueue::filtered`) sum the same orders, leaving out a small order, a
/// young one, one of an excluded participant and one whose age is unknown, that a maximum age keeps
/// the orders younger than it, so a minimum and a maximum of the same age split the orders that
/// carry a placement time, and that a feed with aggregate depth only gives no value or column
/// instead of an unfiltered one.
#[test]
fn one_order_filter_gives_book_size_heatmap_and_ladder_the_same_sizes() {
    use crate::data::{OrderFilter, OrderFilterUnavailable, OrderFlowInput, OrderFlowKind};
    use crate::model::OrderSide::{Buy, Sell};
    let time = at(1, 0);
    let ago = |seconds| time - TimeDelta::seconds(seconds);
    let orders = vec![
        RestingOrder::new(1, Buy, 99.0, 5.0)
            .with_participant(1)
            .with_placed(ago(30)),
        RestingOrder::new(2, Buy, 99.0, 1.0)
            .with_participant(2)
            .with_placed(ago(2)),
        RestingOrder::new(3, Buy, 98.75, 8.0)
            .with_participant(1)
            .with_placed(ago(60)),
        RestingOrder::new(4, Sell, 99.25, 4.0)
            .with_participant(2)
            .with_placed(ago(40)),
        RestingOrder::new(5, Sell, 99.5, 2.0),
    ];
    let filter = OrderFilter {
        min_size: 2.0,
        min_age: Some(TimeDelta::seconds(10)),
        max_age: None,
        exclude_participants: vec![2],
    };
    let config = DepthHistoryConfig {
        cadence: TimeDelta::seconds(1),
        rows: 10,
        columns: 10,
        orders: Some(filter.clone()),
        ..Default::default()
    };
    let size = |filter: &OrderFilter| {
        OrderFlowInput::new(OrderFlowKind::BookSize {
            levels: 10,
            distance: 0.0,
        })
        .with_order_filter(filter.clone())
    };
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe("A".into(), Timeframe::Min1.into(), 10, Some(0.25));
    let (filtered, everything) = (size(&filter), size(&OrderFilter::default()));
    sink.update(|live| {
        live.study_inputs
            .extend([filtered.clone(), everything.clone()])
    });
    inbox.sample_depth(sink.id, Some(config.clone()));
    sink.set_server_time(time);
    sink.push_order_snapshot(1, time, orders.clone());
    let next = time + TimeDelta::seconds(1);
    sink.push_order_event(
        2,
        next,
        OrderEvent::Add(RestingOrder::new(6, Sell, 100.0, 1.0)),
    );
    let (_, updates) = inbox.drain().updates.pop().unwrap();

    // The ladder: the accepted orders are the two bids of 5 and 8, and no ask.
    let ladder = updates.orders.unwrap().filtered("A", &filter).unwrap();
    let sizes = |levels: &[OrderBookLevel]| Vec::from_iter(levels.iter().map(|l| l.size));
    assert_eq!(
        (sizes(&ladder.bids), sizes(&ladder.asks)),
        (vec![5.0, 8.0], vec![])
    );
    // Book size and the heatmap column sum the same orders.
    let bar = at(1, 0);
    assert_eq!(filtered.values()[&bar], [13.0, 0.0, 13.0]);
    assert_eq!(everything.values()[&bar], [14.0, 7.0, 7.0]);
    let column = &updates.depth_columns[0];
    assert_eq!(
        (column.time, column.bids.clone(), column.asks.clone()),
        (time, vec![(396, 5.0), (395, 8.0)], vec![])
    );

    // Aggregate depth knows no orders: no value and no column, never the unfiltered sizes.
    let sink = inbox.subscribe("B".into(), Timeframe::Min1.into(), 10, Some(0.25));
    let aggregate = size(&filter);
    sink.update(|live| live.study_inputs.push(aggregate.clone()));
    inbox.sample_depth(sink.id, Some(config));
    sink.set_server_time(time);
    let book = |ts, price| OrderBook {
        ts,
        symbol: "B".into(),
        bids: vec![OrderBookLevel {
            price,
            size: 9.0,
            order_cnt: None,
        }],
        asks: vec![],
    };
    sink.push_depth_snapshot(1, book(time, 99.0));
    sink.push_depth_delta(2, &book(next, 98.75));
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert!(updates.depth_columns.is_empty() && updates.orders.is_none());
    assert!(aggregate.values()[&bar].iter().all(|v| v.is_nan()));

    // A filter on metadata no order has is unavailable, not silently ignored.
    let mut queue = crate::data::OrderQueue::new(Some(0.25));
    queue
        .snapshot(1, time, vec![RestingOrder::new(1, Buy, 99.0, 5.0)])
        .unwrap();
    for (filter, expected) in [
        (
            OrderFilter {
                exclude_participants: vec![1],
                ..Default::default()
            },
            Err(OrderFilterUnavailable::Participants),
        ),
        (
            OrderFilter {
                min_age: Some(TimeDelta::seconds(1)),
                ..Default::default()
            },
            Err(OrderFilterUnavailable::PlacementTimes),
        ),
        (
            OrderFilter {
                max_age: Some(TimeDelta::seconds(1)),
                ..Default::default()
            },
            Err(OrderFilterUnavailable::PlacementTimes),
        ),
        (
            OrderFilter {
                min_size: 1.0,
                ..Default::default()
            },
            Ok(()),
        ),
    ] {
        assert_eq!(queue.filtered("A", &filter).map(|_| ()), expected);
    }

    // A minimum and a maximum of the same age split the orders that carry a placement time: 2 s
    // is fresh, 10 s and 30 s persistent, and the order without one is in neither.
    let mut queue = crate::data::OrderQueue::new(Some(0.25));
    let orders = vec![
        RestingOrder::new(1, Buy, 99.0, 1.0).with_placed(ago(2)),
        RestingOrder::new(2, Buy, 99.0, 2.0).with_placed(ago(10)),
        RestingOrder::new(3, Buy, 99.0, 4.0).with_placed(ago(30)),
        RestingOrder::new(4, Buy, 99.0, 8.0),
    ];
    queue.snapshot(1, time, orders).unwrap();
    let ten = Some(TimeDelta::seconds(10));
    for (filter, expected) in [
        (
            OrderFilter {
                max_age: ten,
                ..Default::default()
            },
            vec![1.0],
        ),
        (
            OrderFilter {
                min_age: ten,
                ..Default::default()
            },
            vec![6.0],
        ),
    ] {
        assert_eq!(sizes(&queue.filtered("A", &filter).unwrap().bids), expected);
    }
}

#[test]
fn order_events_feed_aggregated_depth_and_link_execution_trades() {
    use crate::model::OrderSide::{Buy, Sell};
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe("A".into(), Timeframe::Min1.into(), 10, Some(0.25));
    let input = crate::data::OrderFlowInput::new(crate::data::OrderFlowKind::BookSize {
        levels: 10,
        distance: 0.0,
    });
    sink.update(|live| live.study_inputs.push(input.clone()));
    let time = at(0, 0);
    sink.set_server_time(time);
    sink.push_order_snapshot(
        1,
        time,
        vec![
            RestingOrder::new(7, Buy, 99.0, 3.0),
            RestingOrder::new(8, Sell, 100.0, 5.0),
        ],
    );
    sink.push_order_event(
        2,
        time,
        OrderEvent::Execute {
            id: 8,
            size: 2.0,
            trade: Some(
                Tick::new(time, 100.0, 2.0, Some(Buy))
                    .with_id(42)
                    .with_aggressor_order_id(9),
            ),
        },
    );
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    let book = updates.depth.unwrap();
    assert_eq!(
        (book.bids[0].size, book.asks[0].size, book.asks[0].order_cnt),
        (3.0, 3.0, Some(1))
    );
    assert_eq!(input.values()[&time], [3.0, 3.0, 0.0]);
    let trades: Vec<_> = updates.trades.iter().collect();
    assert_eq!(
        (
            trades.len(),
            trades[0].resting_order_id,
            trades[0].aggressor_order_id
        ),
        (1, Some(8), Some(9))
    );
    sink.push_order_event(4, at(0, 10), OrderEvent::Cancel { id: 7 });
    sink.push_order_event(4, at(0, 10), OrderEvent::Cancel { id: 7 });
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert!(updates.resnapshot);
    assert!(updates.depth.unwrap().bids.is_empty());
    assert!(input.values()[&time].iter().all(|value| value.is_nan()));
    sink.push_order_snapshot(3, at(0, 20), vec![RestingOrder::new(7, Buy, 99.0, 3.0)]);
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert!(!updates.resnapshot);
    assert!(updates.orders.unwrap().orders().next().is_none());
    assert_eq!(input.values()[&time], [0.0; 3]);
}

/// `minute` and `second` after 10:00.
fn at(minute: i64, second: i64) -> DateTime<Utc> {
    "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap()
        + TimeDelta::minutes(minute)
        + TimeDelta::seconds(second)
}

fn tick(ts: DateTime<Utc>, price: f64, size: f64) -> Tick {
    Tick::new(ts, price, size, None)
}

/// The one-minute bar opening `minute` minutes after 10:00.
fn bar(minute: i64, [open, high, low, close, volume]: [f64; 5]) -> Bar {
    Bar::new(at(minute, 0), open, high, low, close, volume)
}

fn subscribe(inbox: &Arc<Inbox>) -> Sink {
    inbox.subscribe("ES".into(), BarClock::new(Timeframe::Min1), 100, None)
}

/// Count bars must keep their progress across drains, count only accepted trades, and
/// separate bars even when the exchange stamps several executions at the same instant.
#[test]
fn count_bars_follow_accepted_trades_across_frames() {
    for count in [1, 10, 100, 1000] {
        let timeframe = format!("{count}t").parse().unwrap();
        let inbox = Arc::new(Inbox::default());
        let clock = BarClock::new(timeframe);
        let sink = inbox.subscribe("ES".into(), clock.clone(), 10_000, None);
        let mut bars = BTreeMap::new();
        let mut merged = Vec::new();
        let mut from_ticks = std::collections::BTreeMap::new();
        for i in 0..(count * 2 + 1) {
            let trade =
                tick(at(0, 5), 100.0 + (i % 3) as f64, (i % 4 + 1) as f64).with_id(i as u64);
            sink.push_tick(trade.clone());
            sink.push_tick(trade); // Replays must not consume a count.
            sink.push_tick(tick(at(0, 5), f64::NAN, 1.0));
            sink.advance_time(at(2, 0)); // A clock heartbeat never consumes a count.
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            super::super::merge::merge(
                &mut merged,
                &mut from_ticks,
                &clock,
                updates.bars.values().cloned().collect(),
            );
            bars.extend(updates.bars.into_iter().map(|(t, (bar, _))| (t, bar)));
            assert_eq!(updates.repeated_ticks, 1);
            assert_eq!(updates.rejected, 1);
        }
        assert_eq!(merged, bars.values().cloned().collect::<Vec<_>>());
        assert_eq!(bars.len(), 3, "{count}");
        for (group, bar) in bars.values().enumerate() {
            let start = group * count;
            let end = (start + count).min(count * 2 + 1);
            let prices: Vec<_> = (start..end).map(|i| 100.0 + (i % 3) as f64).collect();
            assert_eq!(
                [bar.open, bar.high, bar.low, bar.close, bar.volume],
                [
                    prices[0],
                    prices.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    prices.iter().copied().fold(f64::INFINITY, f64::min),
                    *prices.last().unwrap(),
                    (start..end).map(|i| (i % 4 + 1) as f64).sum()
                ],
                "{count}, group {group}",
            );
        }
    }
}

/// Hidden streams bound bars independently of raw fills and ask for a reload on eviction.
#[test]
fn undrained_bars_are_bounded_and_overflow_asks_for_a_reload() {
    for pushed_bars in [false, true] {
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe(
            "ES".into(),
            "1t".parse::<Timeframe>().unwrap().into(),
            2,
            None,
        );
        sink.set_server_time(at(0, 0));
        for id in 0..100_002 {
            if pushed_bars {
                let time = at(0, 0) + TimeDelta::nanoseconds(id);
                sink.push_bar(Bar::new(time, 100.0, 100.0, 100.0, 100.0, 1.0));
            } else {
                sink.push_tick(tick(at(0, 0), 100.0, 1.0).with_id(id as u64));
            }
        }
        sink.report_error(FeedError::Network("another failure".into()));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(updates.bars.len(), 100_000);
        assert_eq!(
            updates.bars.first_key_value().unwrap().0,
            &(at(0, 0) + TimeDelta::nanoseconds(2))
        );
        assert!(updates.reload);
        assert_eq!(
            updates.error,
            Some(FeedError::Network("another failure".into()))
        );
        assert_eq!(updates.rejected, 0);
        sink.push_tick(tick(at(0, 0) + TimeDelta::nanoseconds(100_003), 101.0, 1.0));
        let (_, next) = inbox.drain().updates.pop().unwrap();
        assert!(!next.reload);
        assert_eq!(next.bars.len(), 1);
        assert_eq!(next.error, None);
        assert_eq!(next.rejected, 0);
        sink.reset_replay(at(0, 0));
        sink.push_tick(tick(at(0, 0), 99.0, 1.0));
        let (_, reset) = inbox.drain().updates.pop().unwrap();
        assert!(reset.reset);
        assert_eq!(reset.bars.len(), 1);
        assert_eq!(reset.error, None);
    }
    // An explicit historical batch is not a hidden live queue.
    let trades = (0..100_002)
        .map(|id| tick(at(0, 0), 10.0, 1.0).with_id(id))
        .collect::<Vec<_>>();
    assert_eq!(
        aggregate_trades(&trades, "1t".parse::<Timeframe>().unwrap())
            .unwrap()
            .len(),
        trades.len()
    );
}

/// A delayed execution cannot become the close, within a drain or after it.
#[test]
fn backwards_executions_are_late_even_inside_the_forming_bucket() {
    for timeframe in [Timeframe::Min1, Timeframe::Hour1, "1t".parse().unwrap()] {
        for drain in [false, true] {
            let inbox = Arc::new(Inbox::default());
            let sink = inbox.subscribe("ES".into(), timeframe.into(), 100, None);
            let latest = tick(at(0, 50), 110.0, 1.0);
            let delayed = tick(at(0, 10), 90.0, 1.0);
            sink.push_tick(latest.clone());
            if drain {
                inbox.drain();
            }
            sink.push_tick(delayed.clone());
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            assert_eq!(updates.late_ticks, 1, "{timeframe}, drain={drain}");
            assert!(updates.bars.values().all(|(bar, _)| bar.close == 110.0));
            assert_eq!(updates.trades.iter().count(), usize::from(!drain));
            assert!(matches!(
                aggregate_trades(&[latest, delayed], timeframe),
                Err(FeedError::InvalidData(_))
            ));
            // Equal execution times are valid, and a replay seek resets the watermark.
            sink.push_tick(tick(at(0, 50), 111.0, 1.0));
            assert_eq!(inbox.drain().updates[0].1.late_ticks, 0);
            sink.reset_replay(at(0, 0));
            sink.push_tick(tick(at(0, 10), 90.0, 1.0));
            assert_eq!(inbox.drain().updates[0].1.late_ticks, 0);
        }
    }
}

/// The first trade on a boundary belongs to the next bar, and a quiet interval opens
/// without trades. Its first execution replaces the carried close, even across drains.
#[test]
fn clock_boundaries_and_empty_intervals() {
    for timeframe in [Timeframe::Sec1, Timeframe::Min1, "45s".parse().unwrap()] {
        let clock = BarClock::new(timeframe);
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("ES".into(), clock.clone(), 100, None);
        let start = at(0, 0);
        let boundary = start + timeframe.duration();
        sink.push_tick(tick(start, 100.0, 2.0));
        sink.push_tick(tick(boundary - TimeDelta::milliseconds(1), 103.0, 3.0));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let first = &updates.bars[&start].0;
        assert_eq!(
            [first.open, first.high, first.low, first.close, first.volume],
            [100.0, 103.0, 100.0, 103.0, 5.0]
        );
        sink.advance_time(boundary);
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            updates.bars[&boundary].0,
            Bar::new(boundary, 103.0, 103.0, 103.0, 103.0, 0.0)
        );
        let mut bars = Vec::new();
        let mut from_ticks = std::collections::BTreeMap::new();
        super::super::merge::merge(
            &mut bars,
            &mut from_ticks,
            &clock,
            updates.bars.into_values().collect(),
        );
        sink.push_tick(tick(boundary, 99.0, 4.0));
        sink.push_tick(tick(boundary + TimeDelta::milliseconds(1), 101.0, 5.0));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            updates.bars[&boundary],
            (
                Bar::new(boundary, 99.0, 101.0, 99.0, 101.0, 9.0),
                Origin::TicksAfterHeartbeat(2)
            )
        );
        super::super::merge::merge(
            &mut bars,
            &mut from_ticks,
            &clock,
            updates.bars.into_values().collect(),
        );
        assert_eq!(bars, [Bar::new(boundary, 99.0, 101.0, 99.0, 101.0, 9.0)]);
    }
}

/// Guards coalescing: folding must keep the highs and lows of every trade, a pushed bar must
/// replace the forming bar, late and invalid trades must be dropped from the bars and the trade
/// store alike, a lost book must reach the chart as one snapshot request unless the feed healed
/// it first, and the latest status and error must win.
#[test]
fn pushes_coalesce_into_bars() {
    let inbox = Arc::new(Inbox::default());
    let sink = subscribe(&inbox);
    sink.push_tick(tick(at(0, 5), 100.0, 1.0));
    sink.push_tick(tick(at(0, 20), 103.0, 2.0));
    sink.push_tick(tick(at(0, 40), 99.0, 1.0));
    sink.push_tick(tick(at(0, 50), f64::NAN, 1.0)); // invalid
    sink.push_tick(tick(at(1, 10), 101.0, 3.0)); // opens the 10:01 bar
    sink.push_tick(tick(at(0, 59), 104.0, 1.0)); // late
    sink.push_bar(bar(1, [101.0, 102.0, 100.0, 101.5, 50.0])); // replaces the forming bar
    sink.push_tick(tick(at(1, 30), 105.0, 1.0));
    sink.push_bar(bar(-2, [98.0, 99.0, 97.0, 98.5, 40.0])); // older than everything pending
    let book = |price| OrderBook {
        ts: at(1, 30),
        symbol: "ES".into(),
        bids: vec![OrderBookLevel {
            price,
            size: 1.0,
            order_cnt: None,
        }],
        asks: Vec::new(),
    };
    sink.push_depth_snapshot(5, book(99.0));
    sink.push_depth_delta(6, &book(f64::NAN)); // invalid: the book is lost
    let quote = |second, bid| Level1Quote {
        ts: at(1, second),
        symbol: "ES".into(),
        bid,
        ask: 101.0,
        bid_size: 2.0,
        ask_size: 3.0,
    };
    sink.push_quote(quote(0, 99.0));
    sink.push_quote(quote(1, 100.0));
    sink.push_quote(quote(0, 98.0)); // older must not replace the latest
    sink.push_quote(quote(1, 102.0)); // crossed, as in an auction: kept
    sink.push_quote(quote(2, f64::NAN)); // invalid must not replace it either
    let mut future = quote(2, 100.0);
    future.ts = DateTime::from_timestamp(1_710_496_805_000, 0).unwrap();
    sink.push_quote(future); // a bad time must not make every later quote stale
    sink.set_status(FeedStatus::Connecting);
    sink.set_status(FeedStatus::Live);
    sink.report_error(FeedError::Network("reset".into()));
    sink.report_error(FeedError::Network("timeout".into()));

    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(
        Vec::from_iter(updates.bars.into_values()),
        [
            (bar(-2, [98.0, 99.0, 97.0, 98.5, 40.0]), Origin::Live),
            (bar(0, [100.0, 103.0, 99.0, 99.0, 4.0]), Origin::Ticks(3)),
            (bar(1, [101.0, 105.0, 100.0, 105.0, 51.0]), Origin::Live),
        ]
    );
    let prices = Vec::from_iter(updates.trades.iter().map(|trade| trade.price));
    assert_eq!(prices, [100.0, 103.0, 99.0, 101.0, 105.0]);
    let depth = updates.depth.map(|book| book.bids.len());
    assert_eq!(
        (depth, updates.resnapshot, updates.rejected_depth),
        (Some(0), true, 1)
    );
    assert_eq!((updates.late_ticks, updates.rejected), (1, 1));
    assert_eq!(
        updates.quote.as_ref().map(|q| (q.ts, q.bid)),
        Some((at(1, 1), 102.0))
    );
    assert_eq!(updates.status, Some(FeedStatus::Live));
    assert_eq!(updates.error, Some(FeedError::Network("timeout".into())));

    sink.push_depth_snapshot(7, book(98.0));
    sink.push_depth_delta(9, &book(97.0)); // a gap
    sink.push_depth_snapshot(10, book(96.0)); // healed before the chart saw the gap
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    let depth = updates.depth.map(|book| book.bids[0].price);
    assert_eq!((depth, updates.resnapshot), (Some(96.0), false));
}

/// Guards the memory bound and the repaint rate: a queue of raw updates or an unbounded trade
/// store grows with the update rate, a wake per push floods the UI, and a waker that a drain
/// does not re-arm stalls the chart.
#[test]
fn a_burst_holds_one_bar_and_wakes_once() {
    let inbox = Arc::new(Inbox::default());
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    inbox.set_waker(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    let sink = subscribe(&inbox);
    inbox.drain(); // the burst lands in the store a drain left behind
    let producer = sink.clone();
    std::thread::spawn(move || {
        for i in 0..10_000 {
            let price = 100.0 + (i % 7) as f64;
            producer.push_tick(tick(at(0, 0) + TimeDelta::milliseconds(5 * i), price, 1.0));
        }
    })
    .join()
    .unwrap();
    assert_eq!(wakes.load(Ordering::Relaxed), 1);

    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(updates.trades.iter().count(), 100);
    let bar = bar(0, [100.0, 106.0, 100.0, 103.0, 10_000.0]);
    assert_eq!(
        Vec::from_iter(updates.bars.into_values()),
        [(bar, Origin::Ticks(10_000))]
    );
    sink.push_tick(tick(at(0, 59), 100.0, 1.0));
    assert_eq!(wakes.load(Ordering::Relaxed), 2);
}

/// Guards the trade id contract: a feed replaying its last trades after a reconnect must not
/// count them twice in a bar, within one drain or across drains.
#[test]
fn a_trade_id_is_folded_once() {
    let inbox = Arc::new(Inbox::default());
    let sink = subscribe(&inbox);
    let trade = tick(at(0, 5), 100.0, 1.0).with_id(7);
    sink.push_tick(trade.clone());
    sink.push_tick(trade.clone());
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(
        Vec::from_iter(updates.bars.into_values()),
        [(bar(0, [100.0, 100.0, 100.0, 100.0, 1.0]), Origin::Ticks(1))]
    );
    assert_eq!(updates.trades.iter().count(), 1);
    assert_eq!(updates.repeated_ticks, 1);

    sink.push_tick(trade);
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(updates.bars, BTreeMap::new());
    assert_eq!(updates.trades.iter().count(), 0);
    assert_eq!(updates.repeated_ticks, 1);

    // A store of two trades remembers the last two ids: 1 is forgotten once 3 is folded.
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe("ES".into(), BarClock::new(Timeframe::Min1), 2, None);
    for (second, id) in [(1, 1), (2, 2), (3, 3), (4, 1), (5, 3)] {
        sink.push_tick(tick(at(0, second), 100.0, 1.0).with_id(id));
    }
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(
        Vec::from_iter(updates.bars.into_values()),
        [(bar(0, [100.0, 100.0, 100.0, 100.0, 4.0]), Origin::Ticks(4))]
    );
    assert_eq!(updates.repeated_ticks, 1);
}

/// Guards the forming bar against a push stamped far ahead, such as a millisecond time read as
/// seconds: it must be rejected without making the later trades late. A push within the bound
/// must be kept: the trades of a feed clock running ahead of the system clock, a trade on short
/// bars from a feed whose clock is seconds ahead, a daily bar stamped on a trading date days
/// ahead.
#[test]
fn a_push_far_ahead_of_the_feed_clock_is_rejected() {
    let inbox = Arc::new(Inbox::default());
    let sink = subscribe(&inbox);
    let far = DateTime::from_timestamp(1_710_496_805_000, 0).unwrap();
    sink.push_tick(tick(at(0, 5), 100.0, 1.0));
    sink.push_tick(tick(far, 101.0, 1.0));
    sink.push_bar(Bar::new(far, 101.0, 101.0, 101.0, 101.0, 1.0));
    sink.push_tick(tick(at(0, 6), 102.0, 1.0));
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(
        Vec::from_iter(updates.bars.into_values()),
        [(bar(0, [100.0, 102.0, 100.0, 102.0, 2.0]), Origin::Ticks(2))]
    );
    let prices = Vec::from_iter(updates.trades.iter().map(|trade| trade.price));
    assert_eq!(prices, [100.0, 102.0]);
    assert_eq!((updates.rejected, updates.late_ticks), (2, 0));

    let time = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
    // (timeframe, feed clock, pushes bars rather than trades, pushed times, kept slots, rejected)
    #[rustfmt::skip]
    let cases = [
        // 00:02 is two slots and 90 s ahead.
        (Timeframe::Min1, "2100-01-01T00:00:30Z", false,
            &["2100-01-01T00:00:40Z", "2100-01-01T00:01:59Z", "2100-01-01T00:02:00Z"][..],
            &["2100-01-01T00:00:00Z", "2100-01-01T00:01:00Z"][..], 1),
        // Five slots but seconds ahead is kept; 61 s ahead is rejected.
        (Timeframe::Sec1, "2100-01-01T00:00:00Z", false,
            &["2100-01-01T00:00:05Z", "2100-01-01T00:01:01Z"],
            &["2100-01-01T00:00:05Z"], 1),
        // A trading date three days ahead is kept; fifteen days ahead is rejected.
        (Timeframe::Day1, "2100-01-01T12:00:00Z", true,
            &["2100-01-04T00:00:00Z", "2100-01-16T00:00:00Z"],
            &["2100-01-04T00:00:00Z"], 1),
    ];
    for (timeframe, now, bars, pushes, kept, rejected) in cases {
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("ES".into(), BarClock::new(timeframe), 100, None);
        sink.set_server_time(time(now));
        for &push in pushes {
            let ts = time(push);
            if bars {
                sink.push_bar(Bar::new(ts, 1.0, 1.0, 1.0, 1.0, 1.0));
            } else {
                sink.push_tick(tick(ts, 1.0, 1.0));
            }
        }
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let kept = Vec::from_iter(kept.iter().map(|&slot| time(slot)));
        assert_eq!(
            (Vec::from_iter(updates.bars.into_keys()), updates.rejected),
            (kept, rejected),
            "{timeframe:?}"
        );
    }
}

/// Guards against updates of a replaced subscription (an older symbol or timeframe) leaking
/// into the series that replaced it.
#[test]
fn an_ended_subscription_is_ignored() {
    let inbox = Arc::new(Inbox::default());
    let old = subscribe(&inbox);
    old.push_tick(tick(at(0, 5), 100.0, 1.0));
    let new = subscribe(&inbox);
    inbox.unsubscribe(old.id);
    old.push_tick(tick(at(0, 10), 101.0, 1.0));
    new.push_tick(tick(at(0, 10), 200.0, 1.0));
    assert!(!old.is_active());
    assert!(new.is_active());

    let drained = inbox.drain().updates;
    let ids: Vec<_> = drained.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [new.id]);
    let bar = bar(0, [200.0, 200.0, 200.0, 200.0, 1.0]);
    assert_eq!(
        Vec::from_iter(drained[0].1.bars.values().cloned()),
        [(bar, Origin::Ticks(1))]
    );
}

/// Guards the one-shot contract: a lost request must not leave the chart loading forever,
/// and an answered one must not also report `Unanswered` when dropped.
#[test]
fn a_reply_answers_once_and_a_dropped_one_reports_unanswered() {
    let inbox = Arc::new(Inbox::default());
    let page = HistoryPage::default();
    inbox.reply(1).send(Ok(page.clone()));
    let lost = inbox.reply::<HistoryPage>(2);
    std::thread::spawn(move || drop(lost)).join().unwrap();

    let answers: Vec<_> = inbox
        .drain()
        .answers
        .into_iter()
        .map(|(request, answer)| {
            let answer = answer.downcast::<Result<HistoryPage, FeedError>>();
            (request, *answer.unwrap())
        })
        .collect();
    assert_eq!(answers, [(1, Ok(page)), (2, Err(FeedError::Unanswered))]);
}

/// Guards the depth history samples: the book keys prices by integer ticks (a level removed
/// at a price computed with a rounding error must leave, or the history shows liquidity that
/// left), rows group ticks downwards, sum their sizes and keep the nearest `rows`, and a
/// period that ends gives one column of the book as it stood before the update that opened
/// the next period, spanning every period it did not change in. An update stamped before the
/// open period, or ahead of the feed's time, samples nothing.
#[test]
fn depth_columns_sample_the_book_at_the_end_of_each_period() {
    let inbox = Arc::new(Inbox::default());
    let clock = BarClock::new(Timeframe::Min1);
    let sink = inbox.subscribe("ES".into(), clock, 100, Some(0.1));
    let config = DepthHistoryConfig {
        cadence: TimeDelta::seconds(1),
        rows: 2,
        ticks_per_row: 2,
        columns: 10,
        retention: None,
        orders: None,
    };
    inbox.sample_depth(sink.id, Some(config));
    let level = |price, size| OrderBookLevel {
        price,
        size,
        order_cnt: None,
    };
    let book = |second, bids, asks| OrderBook {
        ts: at(0, second),
        symbol: "ES".into(),
        bids,
        asks,
    };
    let bids = vec![
        level(0.3, 1.0),
        level(0.2, 2.0),
        level(-0.1, 4.0),
        level(-0.3, 8.0),
    ];
    sink.push_depth_snapshot(1, book(0, bids, vec![level(0.5, 3.0)]));
    sink.push_depth_delta(2, &book(0, vec![level(0.1 + 0.2, 0.0)], vec![]));
    let bids = vec![level(0.2, 5.0), level(0.3, 1.0)];
    sink.push_depth_delta(3, &book(3, bids, vec![]));
    sink.push_depth_delta(4, &book(2, vec![], vec![level(0.5, 0.0)]));
    let mut future = book(4, vec![], vec![]);
    future.ts = Utc::now() + TimeDelta::days(1);
    sink.push_depth_delta(5, &future);
    sink.push_depth_delta(6, &book(4, vec![], vec![]));

    let (_, updates) = inbox.drain().updates.pop().unwrap();
    let column = |second, span, bids: &[(i64, f64)], asks: &[(i64, f64)]| DepthColumn {
        time: at(0, second),
        span: TimeDelta::seconds(span),
        bids: bids.to_vec(),
        asks: asks.to_vec(),
        ..Default::default()
    };
    assert_eq!(
        Vec::from(updates.depth_columns),
        [
            column(0, 3, &[(1, 2.0), (-1, 4.0)], &[(2, 3.0)]),
            column(3, 1, &[(1, 6.0), (-1, 4.0)], &[]),
        ]
    );
    sink.push_depth_delta(7, &book(5, vec![], vec![]));
    inbox.sample_depth(sink.id, Some(DepthHistoryConfig::default()));
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert!(
        updates.depth_columns.is_empty(),
        "new settings discard pending old rows"
    );
}

/// A short retention saves tier memory without shrinking the queue of an undrawn chart.
#[test]
fn waiting_depth_columns_keep_600_under_a_retention() {
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe("ES".into(), BarClock::new(Timeframe::Min1), 100, Some(0.1));
    let config = DepthHistoryConfig::default().with_retention(Some(TimeDelta::minutes(15)));
    assert_eq!(config.columns, 5);
    inbox.sample_depth(sink.id, Some(config));
    let level = |price| OrderBookLevel {
        price,
        size: 1.0,
        order_cnt: None,
    };
    sink.push_depth_snapshot(
        1,
        OrderBook {
            ts: at(0, 0),
            symbol: "ES".into(),
            bids: vec![level(0.1)],
            asks: vec![level(0.2)],
        },
    );
    for second in 1..=700 {
        sink.advance_time(at(0, 0) + TimeDelta::seconds(second));
    }
    let (_, updates) = inbox.drain().updates.pop().unwrap();
    assert_eq!(updates.depth_columns.len(), 600);
    assert_eq!(
        updates.depth_columns.front().unwrap().time,
        at(0, 0) + TimeDelta::seconds(100)
    );
}

/// Guards the depth history across a lost book: a period that ends while the aggregate book or
/// the order queue waits for a snapshot has no column (an empty one shows no liquidity where the
/// book was unknown, and merging averages it into the columns beside it), and sampling resumes
/// with the period the snapshot arrives in, also when the snapshot is the first update to sample
/// the lost order queue.
#[test]
fn depth_columns_leave_a_gap_while_the_book_waits_for_a_snapshot() {
    use crate::data::OrderFilter;
    use crate::model::OrderSide::{Buy, Sell};
    let time = |millis| at(0, 0) + TimeDelta::milliseconds(millis);
    let level = |price, size| OrderBookLevel {
        price,
        size,
        order_cnt: None,
    };
    let book = |millis, bids, asks| OrderBook {
        ts: time(millis),
        symbol: "A".into(),
        bids,
        asks,
    };
    let orders = |bid| {
        vec![
            RestingOrder::new(1, Buy, 100.0, bid),
            RestingOrder::new(2, Sell, 101.0, 7.0),
        ]
    };
    // Changes neither the price nor the size of order 2.
    let unchanged = || OrderEvent::Modify {
        id: 2,
        price: None,
        size: None,
        keep_priority: true,
    };
    for (name, events, filter) in [
        ("aggregate depth", false, None),
        ("order events, aggregate history", true, None),
        (
            "order events, history of orders",
            true,
            Some(OrderFilter::default()),
        ),
    ] {
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("A".into(), Timeframe::Min1.into(), 10, Some(1.0));
        let largest = filter.is_some();
        let config = DepthHistoryConfig {
            columns: 10,
            orders: filter,
            ..Default::default()
        };
        inbox.sample_depth(sink.id, Some(config));
        if events {
            // 3 does not follow 1, and 4 is held in the same second, so the snapshot's update is
            // the first to sample the lost queue.
            sink.push_order_snapshot(1, time(500), orders(5.0));
            sink.push_order_event(3, time(1_200), unchanged());
            sink.push_order_event(4, time(1_800), unchanged());
            sink.push_order_snapshot(4, time(3_400), orders(6.0));
            sink.push_order_event(5, time(4_500), unchanged());
            sink.push_order_event(6, time(5_500), unchanged());
        } else {
            // 3 does not follow 1.
            sink.push_depth_snapshot(
                1,
                book(500, vec![level(100.0, 5.0)], vec![level(101.0, 7.0)]),
            );
            sink.push_depth_delta(3, &book(1_200, vec![], vec![]));
            sink.push_depth_delta(4, &book(2_300, vec![], vec![]));
            sink.push_depth_snapshot(
                4,
                book(3_400, vec![level(100.0, 6.0)], vec![level(101.0, 7.0)]),
            );
            sink.push_depth_delta(5, &book(4_500, vec![], vec![]));
            sink.push_depth_delta(6, &book(5_500, vec![], vec![]));
        }
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        // Rows 100 and 101 at tick 1: bid 5 until the gap at 1.2 s, nothing for seconds 1 and 2,
        // which end while the book waits, then bid 6 from the snapshot at 3.4 s.
        let column = |second, bid| DepthColumn {
            time: at(0, second),
            span: TimeDelta::seconds(1),
            bids: vec![(100, bid)],
            asks: vec![(101, 7.0)],
            largest_bid_orders: if largest { vec![bid] } else { vec![] },
            largest_ask_orders: if largest { vec![7.0] } else { vec![] },
        };
        assert_eq!(
            Vec::from(updates.depth_columns),
            [column(0, 5.0), column(3, 6.0), column(4, 6.0)],
            "{name}"
        );
    }
}

/// Guards the largest order of a row: a column sampled from individual orders records, next to
/// each row's total, the largest order the filter accepts among the row's ticks, and the same
/// book sampled as aggregate depth records none, so no heatmap cell of it can be marked.
#[test]
fn order_columns_record_the_largest_accepted_order_of_each_row() {
    use crate::data::OrderFilter;
    use crate::model::OrderSide::{Buy, Sell};
    let time = at(1, 0);
    let mut orders = vec![
        RestingOrder::new(1, Buy, 99.0, 50.0),
        RestingOrder::new(2, Buy, 99.25, 15.0),
        RestingOrder::new(3, Buy, 99.25, 60.0).with_participant(2),
        RestingOrder::new(4, Sell, 99.5, 30.0),
    ];
    orders.extend((5..10).map(|id| RestingOrder::new(id, Buy, 99.0, 10.0)));
    let filter = OrderFilter {
        exclude_participants: vec![2],
        ..Default::default()
    };
    let mut columns = Vec::new();
    for sampled in [Some(filter), None] {
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("A".into(), Timeframe::Min1.into(), 10, Some(0.25));
        let config = DepthHistoryConfig {
            ticks_per_row: 2,
            orders: sampled,
            ..Default::default()
        };
        inbox.sample_depth(sink.id, Some(config));
        sink.push_order_snapshot(1, time, orders.clone());
        let next = RestingOrder::new(10, Sell, 100.0, 1.0);
        sink.push_order_event(2, time + TimeDelta::seconds(1), OrderEvent::Add(next));
        let (_, mut updates) = inbox.drain().updates.pop().unwrap();
        columns.push(updates.depth_columns.pop_front().unwrap());
    }
    // Ticks 396 (99.0) and 397 (99.25) make bid row 198, tick 398 (99.5) ask row 199.
    let column = |bid, largest_bid_orders, largest_ask_orders| DepthColumn {
        time,
        span: TimeDelta::seconds(1),
        bids: vec![(198, bid)],
        asks: vec![(199, 30.0)],
        largest_bid_orders,
        largest_ask_orders,
    };
    // Accepted: 50 + 5 * 10 + 15 = 115, the largest 50; aggregate depth also counts the 60.
    assert_eq!(
        columns,
        [
            column(115.0, vec![50.0], vec![30.0]),
            column(175.0, vec![], vec![])
        ]
    );
}

/// A whitespace bar keeps the last real close, so heartbeats after it append empty candles at that
/// close, and without a real close they append none.
#[test]
fn heartbeats_after_whitespace_keep_the_last_real_close() {
    for seeded in [true, false] {
        let clock = BarClock::new(Timeframe::Min1);
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("ES".into(), clock.clone(), 10, None);
        let (mut bars, mut from_ticks) = (Vec::new(), std::collections::BTreeMap::new());
        if seeded {
            sink.push_bar(Bar::new(at(0, 0), 109.0, 109.0, 109.0, 109.0, 1.0));
        }
        sink.push_bar(Bar::whitespace(at(1, 0)));
        for minute in 2..=4 {
            sink.advance_time(at(minute, 1));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let incoming = updates.bars.into_values().collect();
            let merged = super::super::merge::merge(&mut bars, &mut from_ticks, &clock, incoming);
            assert_eq!((updates.rejected, merged.rejected), (0, 0));
        }
        let empty = |minute| Bar::new(at(minute, 0), 109.0, 109.0, 109.0, 109.0, 0.0);
        if seeded {
            assert_eq!(bars[2..], [empty(2), empty(3), empty(4)]);
        } else {
            assert!(matches!(&bars[..], [bar] if bar.is_whitespace()));
        }
    }
}

/// A heartbeat opens a candle only inside the windows the subscription selected, whatever the
/// interval, and the first one of the next session opens that session's slot at the last close.
/// The candles are worked out by hand from a Mon-Fri 09:30-16:00 UTC session and a trade at 100.
#[test]
fn heartbeat_outside_the_session_opens_no_candle() {
    use crate::model::{Session, SessionWindow, TimezoneId};
    use chrono::Weekday::{Fri, Mon, Thu, Tue, Wed};
    use chrono::{NaiveDate, NaiveTime};
    let time = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
    let hm = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
    let days = [Mon, Tue, Wed, Thu, Fri];
    let regular = Session::new([SessionWindow::new(days, hm(9, 30), hm(16, 0))])
        .with_holidays(["2024-03-14".parse::<NaiveDate>().unwrap()]);
    let with_break = Session::new([
        SessionWindow::new(days, hm(9, 30), hm(12, 0)),
        SessionWindow::new(days, hm(13, 0), hm(16, 0)),
    ]);
    let with_premarket = Session::new([SessionWindow::new(days, hm(9, 30), hm(16, 0))])
        .with_extended_windows([SessionWindow::new(days, hm(4, 0), hm(9, 30))]);
    let with_overnight = with_premarket
        .clone()
        .with_overnight_windows([SessionWindow::new(days, hm(20, 0), hm(4, 0))]);
    #[rustfmt::skip]
    let cases = [
        ("after the close, overnight, on a holiday and a weekend", Timeframe::Min1,
            regular.clone(), (false, false), "2024-03-12T15:59:30Z",
            vec![("2024-03-12T16:00:00Z", None),
                ("2024-03-12T16:05:00Z", None),
                ("2024-03-13T03:00:00Z", None),
                ("2024-03-13T09:30:05Z", Some("2024-03-13T09:30:00Z")),
                ("2024-03-14T10:00:00Z", None),
                ("2024-03-16T12:00:00Z", None),
                ("2024-03-18T09:31:00Z", Some("2024-03-18T09:31:00Z"))]),
        ("in the break between two windows", Timeframe::Min1, with_break, (false, false),
            "2024-03-12T11:59:30Z",
            vec![("2024-03-12T12:30:00Z", None),
                ("2024-03-12T13:00:05Z", Some("2024-03-12T13:00:00Z"))]),
        ("a daily bar on a weekend", Timeframe::Day1, regular, (false, false), "2024-03-15T15:59:30Z",
            vec![("2024-03-16T12:00:00Z", None),
                ("2024-03-18T03:00:00Z", None),
                ("2024-03-18T09:30:05Z", Some("2024-03-18T00:00:00Z"))]),
        ("an extended window left out", Timeframe::Min1, with_premarket.clone(), (false, false),
            "2024-03-12T15:59:30Z",
            vec![("2024-03-13T08:00:00Z", None),
                ("2024-03-13T09:30:05Z", Some("2024-03-13T09:30:00Z"))]),
        ("hours without regular windows, extended hours left out", Timeframe::Min1,
            Session::new([]).with_extended_windows([SessionWindow::new(days, hm(4, 0), hm(9, 30))]),
            (false, false), "2024-03-12T08:00:00Z",
            vec![("2024-03-12T08:30:00Z", None), ("2024-03-12T12:00:00Z", None)]),
        ("an extended window selected", Timeframe::Min1, with_premarket, (true, false),
            "2024-03-12T15:59:30Z",
            vec![("2024-03-13T03:00:00Z", None),
                ("2024-03-13T08:00:00Z", Some("2024-03-13T08:00:00Z"))]),
        ("an overnight window left out", Timeframe::Min1, with_overnight.clone(), (true, false),
            "2024-03-12T15:59:30Z",
            vec![("2024-03-12T22:00:00Z", None),
                ("2024-03-13T04:00:05Z", Some("2024-03-13T04:00:00Z"))]),
        ("overnight requested without extended hours stays regular", Timeframe::Min1,
            with_overnight.clone(), (false, true), "2024-03-12T15:59:30Z",
            vec![("2024-03-12T22:00:00Z", None), ("2024-03-13T05:00:00Z", None)]),
        ("an overnight window selected", Timeframe::Min1, with_overnight, (true, true),
            "2024-03-12T15:59:30Z",
            vec![("2024-03-12T22:00:05Z", Some("2024-03-12T22:00:00Z"))]),
    ];
    for (name, timeframe, session, hours, trade, heartbeats) in cases {
        let clock = BarClock::new(timeframe).with_session_hours(
            TimezoneId::utc(),
            session,
            hours.0,
            hours.1,
        );
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("ES".into(), clock, 10, None);
        sink.push_tick(tick(time(trade), 100.0, 1.0));
        inbox.drain();
        for (now, expected) in heartbeats {
            sink.advance_time(time(now));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let opened: Vec<_> = updates.bars.into_values().map(|(bar, _)| bar).collect();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|open| Bar::new(time(open), 100.0, 100.0, 100.0, 100.0, 0.0))
                .collect();
            assert_eq!(opened, expected, "{name}: heartbeat at {now}");
            // The chart's clock keeps running while the market is closed.
            assert_eq!(updates.time, Some(time(now)), "{name}: heartbeat at {now}");
        }
    }
}

#[cfg(feature = "widget")]
mod checkpoint {
    use super::*;

    fn feed(capacity: usize) -> (Arc<Inbox>, Sink) {
        let inbox = Arc::new(Inbox::default());
        let tf: Timeframe = "10v".parse().unwrap();
        let sink = inbox.subscribe("A".into(), tf.into(), capacity, None);
        (inbox, sink)
    }

    #[test]
    fn pending_replies_and_buffered_depth_do_not_cross_the_checkpoint() {
        let (inbox, sink) = feed(2);
        let revision = || inbox.checkpoint_guard(sink.id).unwrap().revision();
        let before = revision();
        inbox.reply::<()>(99).send(Ok(()));
        assert!(matches!(
            inbox.checkpoint_guard(sink.id),
            Err(SinkCheckpointError::NotReady)
        ));
        inbox.drain();
        assert_ne!(before, revision());
        let before = revision();
        sink.advance_time(at(0, 0)); // No candle and no wake for a volume stream.
        assert_ne!(before, revision());
        let before = revision();
        sink.push_depth_delta(
            2,
            &OrderBook {
                ts: at(0, 0),
                symbol: "A".into(),
                bids: vec![],
                asks: vec![],
            },
        );
        inbox.drain();
        // A buffered delta is book state even without a snapshot or displayed levels.
        assert!(matches!(
            inbox.checkpoint_guard(sink.id),
            Err(SinkCheckpointError::Unsupported)
        ));
        sink.reset_replay(at(0, 0));
        inbox.drain();
        assert_ne!(before, revision());
    }

    #[test]
    fn restore_resumes_dedup_eviction_order_and_feed_clock() {
        let (inbox, sink) = feed(2);
        sink.set_server_time(at(0, 0));
        for id in 1..=3 {
            sink.push_tick(tick(at(0, 0), 100.0, 1.0).with_id(id));
        }
        inbox.drain();
        let source = inbox.checkpoint_guard(sink.id).unwrap();
        assert_eq!(source.heap_bytes(), 2 * size_of::<u64>());
        let saved = source.capture();
        drop(source);

        let (inbox, sink) = feed(2);
        sink.set_server_time(at(240, 0));
        let mut dest = inbox.checkpoint_guard(sink.id).unwrap();
        assert!(dest.compatible(&saved));
        let before = dest.revision();
        dest.restore(&saved);
        assert_ne!(dest.revision(), before);
        drop(dest);
        for id in [3, 4, 2] {
            sink.push_tick(tick(at(0, 0), 100.0, 1.0).with_id(id));
        }
        sink.push_tick(tick(at(120, 0), 100.0, 1.0).with_id(5));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(updates.repeated_ticks, 1);
        assert_eq!(updates.rejected, 1);
        assert!(updates.trades.iter().filter_map(|t| t.id).eq([4, 2]));
    }
}
