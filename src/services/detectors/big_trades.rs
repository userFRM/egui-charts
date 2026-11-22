//! Large trades, as markers and as the price zones they leave.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};

use super::Threshold;
use crate::data::meets_threshold;
use crate::model::{
    BarClock, Direction, OrderSide, Placement, Signal, SignalId, SignalShape, Tick,
};
use crate::scales::VolumeFormatter;
use crate::services::SignalEvent;

/// Combined trade sizes the automatic threshold is calibrated on.
const WINDOW: usize = 10_000;
/// Combined trades seen before the automatic threshold applies.
const WARM_UP: u64 = 1_000;
/// Combined trades between two calibrations of the automatic threshold.
const CALIBRATE_EVERY: u64 = 100;

/// Resting orders whose fills are added up: the most recently filled are kept, and up to twice
/// as many are held before the others are dropped.
const RESTING: usize = 10_000;

/// What a big trade is made of.
#[derive(Clone, Copy)]
enum Kind {
    /// The fills of one explicit aggressor order id, or one fill without an id.
    Aggressive,
    /// Fills without aggressor ids at one exchange timestamp and side.
    SameTime,
    /// The fills against one resting order id.
    Passive,
}

impl Kind {
    /// The word after the size in the marker's label.
    fn word(self) -> &'static str {
        match self {
            Self::Aggressive => "aggressive",
            Self::SameTime => "same time",
            Self::Passive => "passive",
        }
    }
}

/// What makes a trade large, which kinds of large trades are reported, and which of their zones
/// are kept.
///
/// Fills form trades of three kinds, each reported while its switch is on:
/// - an aggressive order ([`aggressive_orders`](Self::aggressive_orders)): the fills sharing an
///   explicit aggressor order id in a row, or a fill without one;
/// - fills at one time ([`same_time_fills`](Self::same_time_fills)): two or more fills in a row
///   without aggressor ids that share an exchange timestamp and a known side, as the time and
///   sales panel combines them. The first such fill is an aggressive order until a second one
///   joins it; while the switch is off, each stands alone;
/// - a passive order ([`passive_orders`](Self::passive_orders)): the fills against one resting
///   order id, across aggressive orders and whether or not they are in a row. The fills of at
///   least the 10,000 most recently filled resting orders are added up.
///
/// A fill can count towards an aggressive order or fills at one time and towards a passive order
/// at once. A trade is large once its combined size reaches the threshold, within one part in a
/// billion, so that rounding in a sum of fractional sizes cannot hide it. With
/// [`Threshold::Auto`] that is the 99th percentile, by nearest rank, of the combined sizes of the
/// symbol's last 10,000 aggressive orders and fills at one time, recomputed every 100 of them
/// and inactive until 1,000 have been seen; trades from loaded history count. Passive orders are
/// compared with the same threshold but do not calibrate it.
///
/// A large trade shows as a marker at its first fill time and volume-weighted price, which for a
/// passive order is the resting order's price, coloured by the order's side: the aggressor's for
/// an aggressive order or fills at one time, the resting order's (the opposite of its fills'
/// aggressor side) for a passive order. Its label gives the size and the kind, as in
/// `600 aggressive`, `600 same time` or `600 passive`. The trade also shows as a zone from its
/// first fill onward over the prices it filled at, at least one tick high, centred on them. Both
/// follow the trade's later fills. A zone ends at the first trade at or after its start, other
/// than a fill of its own trade, strictly beyond its far side: below it for a buy, above it for
/// a sell, and on either side when the side is unknown.
///
/// The service uses one interval per symbol, chosen by its latest subscription, for every chart
/// of that symbol. Intraday bars use the instrument's exchange time zone (UTC when absent) and
/// regular session; daily and longer bars use UTC calendar boundaries. The chart's display time
/// zone and extended-hours setting are not inputs to the service.
///
/// When the bar of the subscribed time interval that contains the first fill closes, the trade
/// is in the body of that bar if its volume-weighted price lies between the bar's open and close,
/// both included, and in a wick otherwise. Zones of the kinds not kept are then removed. Zones
/// in a bar the service did not see open, the first one after tracking starts or the interval
/// changes, or under a trade-based interval, stay. A trade that first becomes large after its
/// first fill's bar has closed also keeps its zone: that bar is no longer held for classification.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct BigTradeRule {
    /// Combined size from which a trade is large.
    pub threshold: Threshold,
    /// Report aggressive orders: the fills of one explicit aggressor order id in a row, or a
    /// fill without one. On by default.
    pub aggressive_orders: bool,
    /// Report two or more fills in a row without aggressor ids that share an exchange timestamp
    /// and a known side, as one trade. Off by default, when such fills stand alone.
    pub same_time_fills: bool,
    /// Report passive orders: the fills against one resting order id, across aggressive
    /// orders. Off by default.
    pub passive_orders: bool,
    /// Keep the zones of large trades in the body of their bar.
    pub body_zones: bool,
    /// Keep the zones of large trades in a wick of their bar.
    pub wick_zones: bool,
}

impl Default for BigTradeRule {
    fn default() -> Self {
        Self {
            threshold: Threshold::Auto,
            aggressive_orders: true,
            same_time_fills: false,
            passive_orders: false,
            body_zones: true,
            wick_zones: true,
        }
    }
}

/// The big trade detector of one symbol.
#[derive(Default)]
pub(super) struct BigTrades {
    /// The symbol's tick size, positive and finite.
    tick: f64,
    /// Writes the size labels.
    sizes_label: VolumeFormatter,
    /// Running aggregate of the open trade's fills.
    fills: Option<Trade>,
    /// Open passive orders by resting order id.
    resting: HashMap<u64, Trade>,
    /// Fills seen, which orders the resting orders by their latest fill.
    filled: u64,
    /// Number of the next detection.
    next: u64,
    /// Combined sizes of the last trades, oldest first.
    sizes: VecDeque<f64>,
    /// Trades seen since tracking began.
    seen: u64,
    /// The automatic threshold, once calibrated.
    threshold: Option<f64>,
    /// Zones still to end or to classify.
    zones: Vec<Zone>,
    /// Bars of the subscribed interval, if it is time-based.
    clock: Option<BarClock>,
    /// The forming bar.
    bar: Option<Bar>,
    /// The bar of the last fill or zone start placed.
    placement: Placement,
}

struct Trade {
    aggressor_order_id: Option<u64>,
    first_time: DateTime<Utc>,
    /// The order's side: the aggressor's, or for a passive order the resting order's; unknown if
    /// its fills disagree.
    side: Option<OrderSide>,
    size: f64,
    price: f64,
    low: f64,
    high: f64,
    /// Fills added.
    fills: u64,
    /// Number of the trade's detection, once it is large.
    number: Option<u64>,
    /// The detector's fill count at the trade's latest fill.
    last: u64,
}

impl Trade {
    /// A trade of no fills yet, starting at `fill`, of side `side`.
    fn new(fill: &Tick, side: Option<OrderSide>) -> Self {
        Self {
            aggressor_order_id: fill.aggressor_order_id,
            first_time: fill.ts,
            side,
            size: 0.0,
            price: fill.price,
            low: fill.price,
            high: fill.price,
            fills: 0,
            number: None,
            last: 0,
        }
    }

    /// Adds `fill`, the detector's `filled`th, whose order side for this trade is `side`.
    fn add(&mut self, fill: &Tick, side: Option<OrderSide>, filled: u64) {
        let scale = self.size.max(fill.size);
        if scale > 0.0 {
            let weight = (fill.size / scale) / (self.size / scale + fill.size / scale);
            self.price += (fill.price - self.price) * weight;
        }
        self.size = (self.size + fill.size).min(f64::MAX);
        if self.side != side {
            self.side = None;
        }
        self.low = self.low.min(fill.price);
        self.high = self.high.max(fill.price);
        self.fills = self.fills.saturating_add(1);
        self.last = filled;
    }
}

struct Zone {
    number: u64,
    side: Option<OrderSide>,
    /// Volume-weighted price of the trade.
    price: f64,
    from: DateTime<Utc>,
    to: Option<DateTime<Utc>>,
    low: f64,
    high: f64,
    classified: bool,
}

struct Bar {
    open_time: DateTime<Utc>,
    open: f64,
    close: f64,
    /// Whether the service saw the bar's first trade.
    whole: bool,
}

impl BigTrades {
    /// The detector of a symbol of tick size `tick`, labelling sizes with `volume_decimals`
    /// decimals, or 2 without them.
    pub(super) fn new(tick: f64, volume_decimals: Option<u8>) -> Self {
        Self {
            tick,
            sizes_label: VolumeFormatter {
                precision: volume_decimals.map_or(2, usize::from),
            },
            ..Self::default()
        }
    }

    /// Classifies zones by the bars of `clock` from the next bar on.
    pub(super) fn set_clock(&mut self, clock: Option<BarClock>) {
        self.clock = clock;
        self.bar = None;
    }

    /// Stops following the zone `id`, which is no longer retained.
    pub(super) fn forget(&mut self, id: &SignalId) {
        if let Some(number) =
            id.0.strip_prefix("big-trade-zone-")
                .and_then(|number| number.parse::<u64>().ok())
        {
            self.zones.retain(|zone| zone.number != number);
        }
    }

    /// Detects on `fill`, a valid trade, pushing the changes to `out`.
    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        rule: &BigTradeRule,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        // shortcut: history has no observation clock, add a feed-time policy before rejecting jumps.
        let open = (self.clock.as_ref()).map(|clock| self.placement.open(clock, fill.ts));
        if let (Some(open), Some(bar)) = (open, &self.bar)
            && open > bar.open_time
        {
            self.classify(rule, out);
        }

        self.filled += 1;
        let extends = self.fills.as_ref().is_some_and(|trade| {
            match (trade.aggressor_order_id, fill.aggressor_order_id) {
                (Some(open), Some(id)) => open == id,
                // A timestamp alone cannot identify an unknown aggressor.
                (None, None) => {
                    rule.same_time_fills
                        && fill.side.is_some()
                        && trade.side == fill.side
                        && trade.first_time == fill.ts
                }
                _ => false,
            }
        });
        if !extends && let Some(trade) = self.fills.take() {
            self.calibrate(trade.size);
        }
        let own = [
            self.fills.as_ref().and_then(|trade| trade.number),
            fill.resting_order_id
                .and_then(|id| self.resting.get(&id))
                .and_then(|trade| trade.number),
        ];

        for zone in self.zones.iter_mut() {
            let beyond = match zone.side {
                Some(OrderSide::Buy) => fill.price < zone.low,
                Some(OrderSide::Sell) => fill.price > zone.high,
                None => fill.price < zone.low || fill.price > zone.high,
            };
            if zone.to.is_none()
                && fill.ts >= zone.from
                && !own.contains(&Some(zone.number))
                && beyond
            {
                zone.to = Some(fill.ts);
                out.push(SignalEvent::Upsert(zone.signal()));
            }
        }
        self.zones
            .retain(|zone| zone.to.is_none() || !zone.classified);

        let threshold = match rule.threshold {
            Threshold::Fixed(size) => Some(size),
            Threshold::Auto => self.threshold,
        };
        let mut trade = self
            .fills
            .take()
            .unwrap_or_else(|| Trade::new(fill, fill.side));
        trade.add(fill, fill.side, self.filled);
        let kind = if trade.aggressor_order_id.is_none() && trade.fills > 1 {
            Kind::SameTime
        } else {
            Kind::Aggressive
        };
        self.report(&mut trade, kind, rule, threshold, enabled, out);
        self.fills = Some(trade);

        if let Some(id) = fill.resting_order_id {
            let side = fill.side.map(|side| match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            });
            let mut trade = self
                .resting
                .remove(&id)
                .unwrap_or_else(|| Trade::new(fill, side));
            trade.add(fill, side, self.filled);
            self.report(&mut trade, Kind::Passive, rule, threshold, enabled, out);
            self.resting.insert(id, trade);
            if self.resting.len() > 2 * RESTING {
                let mut lasts: Vec<u64> = self.resting.values().map(|trade| trade.last).collect();
                let cut_at = lasts.len() - RESTING;
                let cut = *lasts.select_nth_unstable(cut_at).1;
                self.resting.retain(|_, trade| trade.last >= cut);
            }
        }

        match (open, &mut self.bar) {
            (Some(open), Some(bar)) if open == bar.open_time => bar.close = fill.price,
            (Some(open), bar) if bar.as_ref().is_none_or(|bar| open > bar.open_time) => {
                *bar = Some(Bar {
                    open_time: open,
                    open: fill.price,
                    close: fill.price,
                    whole: bar.is_some(),
                });
            }
            _ => {}
        }
    }

    /// Marks `trade`, of kind `kind`, and its zone once it is large and its kind is reported.
    fn report(
        &mut self,
        trade: &mut Trade,
        kind: Kind,
        rule: &BigTradeRule,
        threshold: Option<f64>,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        let reported = match kind {
            Kind::Aggressive => rule.aggressive_orders,
            Kind::SameTime => rule.same_time_fills,
            Kind::Passive => rule.passive_orders,
        };
        if !enabled
            || !reported
            || !threshold.is_some_and(|threshold| meets_threshold(trade.size, threshold))
        {
            return;
        }
        let (low, high) = (trade.low, trade.high);
        let tick = self.tick;
        let (low, high) = if high - low < tick {
            let middle = (low + high) / 2.0;
            (middle - tick / 2.0, middle + tick / 2.0)
        } else {
            (low, high)
        };
        let number = match trade.number {
            Some(number) => number,
            None => {
                self.next += 1;
                self.zones.push(Zone {
                    number: self.next,
                    side: trade.side,
                    price: trade.price,
                    from: trade.first_time,
                    to: None,
                    low,
                    high,
                    // Without bars there is nothing to classify by.
                    classified: self.clock.is_none(),
                });
                *trade.number.insert(self.next)
            }
        };
        out.push(SignalEvent::Upsert(Signal {
            id: SignalId(format!("big-trade-{number}")),
            shape: SignalShape::Marker {
                time: trade.first_time,
                price: Some(trade.price),
                direction: match trade.side {
                    Some(OrderSide::Buy) => Direction::Up,
                    Some(OrderSide::Sell) => Direction::Down,
                    None => Direction::Neutral,
                },
            },
            category: super::DetectionKind::BigTrade.category().into(),
            label: Some(format!(
                "{} {}",
                self.sizes_label
                    .format_with_min_unit_decimals(trade.size, 2),
                kind.word()
            )),
            tooltip: None,
        }));
        if let Some(zone) = self.zones.iter_mut().find(|zone| zone.number == number) {
            zone.side = trade.side;
            zone.price = trade.price;
            zone.low = low;
            zone.high = high;
            out.push(SignalEvent::Upsert(zone.signal()));
        }
    }

    /// Classifies the zones of the forming bar, which closed, removing those not kept.
    fn classify(&mut self, rule: &BigTradeRule, out: &mut Vec<SignalEvent>) {
        let (Some(bar), Some(clock)) = (&self.bar, &self.clock) else {
            return;
        };
        let (bottom, top) = (bar.open.min(bar.close), bar.open.max(bar.close));
        self.zones.retain_mut(|zone| {
            if zone.classified {
                return true;
            }
            zone.classified = true;
            // shortcut: late threshold crossings have no saved first bar,
            // retain bars if classification is needed.
            let body = (bottom..=top).contains(&zone.price);
            if !bar.whole
                || self.placement.open(clock, zone.from) < bar.open_time
                || if body {
                    rule.body_zones
                } else {
                    rule.wick_zones
                }
            {
                return zone.to.is_none();
            }
            out.push(SignalEvent::Remove(zone_id(zone.number)));
            false
        });
    }

    /// Adds a trade's combined size to the window of the automatic threshold.
    fn calibrate(&mut self, size: f64) {
        if self.sizes.len() == WINDOW {
            self.sizes.pop_front();
        }
        self.sizes.push_back(size);
        self.seen += 1;
        if self.seen >= WARM_UP && self.seen.is_multiple_of(CALIBRATE_EVERY) {
            let mut sizes: Vec<f64> = self.sizes.iter().copied().collect();
            // Nearest rank: the smallest size that 99% of the window does not exceed.
            let rank = (sizes.len() * 99).div_ceil(100);
            self.threshold = Some(*sizes.select_nth_unstable_by(rank - 1, f64::total_cmp).1);
        }
    }
}

impl Zone {
    fn signal(&self) -> Signal {
        Signal {
            id: zone_id(self.number),
            shape: SignalShape::Zone {
                from: self.from,
                to: self.to,
                low: self.low,
                high: self.high,
            },
            category: super::DetectionKind::BigTradeZone.category().into(),
            label: None,
            tooltip: None,
        }
    }
}

fn zone_id(number: u64) -> SignalId {
    SignalId(format!("big-trade-zone-{number}"))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Host, fill, marker, zone};
    use super::*;
    use crate::data::MarketEvent;
    use crate::model::{Instrument, Timeframe};

    #[test]
    fn big_trades_reach_the_threshold_by_kind() {
        type Row = (
            &'static str,
            u8,
            f64,
            fn(&mut BigTradeRule),
            Vec<Tick>,
            Vec<SignalEvent>,
        );
        let passive = || {
            (1..=10)
                .map(|i| {
                    fill(i, 100.25, 50.0, OrderSide::Buy, Some(i as u64)).with_resting_order_id(42)
                })
                .collect::<Vec<_>>()
        };
        let same_time = || vec![fill(1, 99.75, 100.0, OrderSide::Sell, None); 6];
        let rows: Vec<Row> = vec![
            (
                "fractional sizes reach the threshold",
                1,
                0.8,
                |_| {},
                vec![
                    fill(1, 100.0, 0.7, OrderSide::Buy, Some(7)),
                    fill(2, 100.0, 0.1, OrderSide::Buy, Some(7)),
                ],
                vec![
                    marker(1, 1, 100.0, Direction::Up, "0.8 aggressive"),
                    zone(1, 1, None, 99.875, 100.125),
                ],
            ),
            (
                "a resting order filled by ten aggressive orders",
                0,
                500.0,
                |rule| rule.passive_orders = true,
                passive(),
                vec![
                    marker(1, 1, 100.25, Direction::Down, "500 passive"),
                    zone(1, 1, None, 100.125, 100.375),
                ],
            ),
            ("passive orders off", 0, 500.0, |_| {}, passive(), vec![]),
            (
                "six fills at one time",
                0,
                500.0,
                |rule| rule.same_time_fills = true,
                same_time(),
                vec![
                    marker(1, 1, 99.75, Direction::Down, "500 same time"),
                    zone(1, 1, None, 99.625, 99.875),
                    marker(1, 1, 99.75, Direction::Down, "600 same time"),
                    zone(1, 1, None, 99.625, 99.875),
                ],
            ),
            (
                "fills at one time off",
                0,
                500.0,
                |_| {},
                same_time(),
                vec![],
            ),
            (
                "one fill changes kind on its second fill",
                0,
                100.0,
                |rule| rule.same_time_fills = true,
                vec![fill(1, 100.0, 100.0, OrderSide::Buy, None); 2],
                vec![
                    marker(1, 1, 100.0, Direction::Up, "100 aggressive"),
                    zone(1, 1, None, 99.875, 100.125),
                    marker(1, 1, 100.0, Direction::Up, "200 same time"),
                    zone(1, 1, None, 99.875, 100.125),
                ],
            ),
            (
                "same-time fills without aggressive reports",
                0,
                500.0,
                |rule| {
                    rule.aggressive_orders = false;
                    rule.same_time_fills = true;
                },
                same_time(),
                vec![
                    marker(1, 1, 99.75, Direction::Down, "500 same time"),
                    zone(1, 1, None, 99.625, 99.875),
                    marker(1, 1, 99.75, Direction::Down, "600 same time"),
                    zone(1, 1, None, 99.625, 99.875),
                ],
            ),
            (
                "all kinds off",
                0,
                500.0,
                |rule| rule.aggressive_orders = false,
                vec![fill(1, 100.0, 600.0, OrderSide::Buy, Some(7)).with_resting_order_id(42)],
                vec![],
            ),
            (
                "a fill reports both orders with distinct numbers",
                0,
                500.0,
                |rule| rule.passive_orders = true,
                vec![fill(1, 100.0, 500.0, OrderSide::Buy, Some(7)).with_resting_order_id(42)],
                vec![
                    marker(1, 1, 100.0, Direction::Up, "500 aggressive"),
                    zone(1, 1, None, 99.875, 100.125),
                    marker(2, 1, 100.0, Direction::Down, "500 passive"),
                    zone(2, 1, None, 99.875, 100.125),
                ],
            ),
            (
                "timestamp and side must both match",
                0,
                500.0,
                |rule| rule.same_time_fills = true,
                vec![
                    fill(1, 100.0, 250.0, OrderSide::Buy, None),
                    fill(2, 100.0, 250.0, OrderSide::Buy, None),
                    fill(2, 100.0, 250.0, OrderSide::Sell, None),
                ],
                vec![],
            ),
            (
                "unknown sides do not combine by time",
                0,
                500.0,
                |rule| rule.same_time_fills = true,
                vec![Tick::new(super::super::tests::time(1), 100.0, 250.0, None); 2],
                vec![],
            ),
        ];
        for (name, decimals, threshold, edit, fills, expected) in rows {
            let host = Host::tracking(
                &Instrument::new("ES")
                    .with_tick_size(0.25)
                    .with_volume_decimals(decimals),
                Timeframe::Min1,
                Some(threshold),
            );
            let mut settings = host.detectors.settings();
            edit(&mut settings.big_trades.rule);
            settings.big_trades.alert = true;
            host.detectors.set_settings(settings);
            let events: Vec<_> = fills
                .into_iter()
                .flat_map(|fill| host.push(MarketEvent::Trade(fill)))
                .collect();
            assert_eq!(events, expected, "{name}");
            let mut first = std::collections::HashMap::new();
            for event in &expected {
                if let SignalEvent::Upsert(signal) = event
                    && signal.category == "Big trades"
                {
                    first
                        .entry(&signal.id)
                        .or_insert(signal.label.as_deref().unwrap());
                }
            }
            let alerts = host.detectors.take_alerts();
            assert_eq!(
                alerts.len(),
                first.len(),
                "{name}: alerts count trades, not updates or zones"
            );
            for alert in alerts {
                assert!(
                    first
                        .values()
                        .any(|label| *label == alert.notification.body),
                    "{name}: alert names the kind"
                );
            }
            assert_eq!(
                host.detectors
                    .counts(
                        "ES",
                        super::super::DetectionKind::BigTrade,
                        super::super::tests::time(0)
                    )
                    .total,
                first.len() as u64,
                "{name}: every kind is counted"
            );
        }
    }
}
