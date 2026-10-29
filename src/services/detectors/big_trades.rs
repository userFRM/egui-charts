//! Large aggressive executions, as markers and as the price zones they leave.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use super::Threshold;
use crate::model::{BarClock, Direction, OrderSide, Signal, SignalId, SignalShape, Tick};
use crate::scales::{PriceFormatter, VolumeFormatter};
use crate::services::SignalEvent;

/// Combined trade sizes the automatic threshold is calibrated on.
const WINDOW: usize = 10_000;
/// Combined trades seen before the automatic threshold applies.
const WARM_UP: u64 = 1_000;
/// Combined trades between two calibrations of the automatic threshold.
const CALIBRATE_EVERY: u64 = 100;

/// What makes a trade large, and which of the zones of large trades are kept.
///
/// Fills sharing an explicit aggressor order id, in a row, form one trade; fills without one
/// stand alone. A trade is large once its combined size reaches the threshold. With
/// [`Threshold::Auto`] that is the 99th percentile, by nearest rank, of the combined sizes of
/// the symbol's last 10,000 trades, recomputed every 100 trades and inactive until 1,000 trades
/// have been seen; trades from loaded history count.
///
/// A large trade shows as a marker at its first fill time and volume-weighted price, coloured by
/// its aggressor side and labelled with its size, and as a zone from its first fill onward over
/// the prices it filled at, at least one tick high, centred on them. Both follow the later fills
/// of the same aggressor. A zone ends at the first trade at or after its start strictly beyond
/// its far side: below it after a buy, above it after a sell, and on either side when the side is
/// unknown.
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
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BigTradeRule {
    /// Combined size from which a trade is large.
    pub threshold: Threshold,
    /// Keep the zones of large trades in the body of their bar.
    pub body_zones: bool,
    /// Keep the zones of large trades in a wick of their bar.
    pub wick_zones: bool,
}

impl Default for BigTradeRule {
    fn default() -> Self {
        Self {
            threshold: Threshold::Auto,
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
    /// Number of the open trade's detection, once it is large.
    large: Option<u64>,
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
}

struct Trade {
    aggressor_order_id: Option<u64>,
    first_time: DateTime<Utc>,
    side: Option<OrderSide>,
    size: f64,
    price: f64,
    low: f64,
    high: f64,
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
    /// decimals, or 2 like the chart's volume readouts.
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
        self.zones.retain(|zone| zone_id(zone.number) != *id);
    }

    /// Detects on `fill`, a valid trade, pushing the changes to `out`.
    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        rule: &BigTradeRule,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        let open = self.clock.as_ref().map(|clock| clock.bar_open(fill.ts));
        if let (Some(open), Some(bar)) = (open, &self.bar)
            && open > bar.open_time
        {
            self.classify(rule, out);
        }

        let extends = fill.aggressor_order_id.is_some()
            && self.fills.as_ref().map(|trade| trade.aggressor_order_id)
                == Some(fill.aggressor_order_id);
        if !extends {
            if let Some(trade) = self.fills.take() {
                self.calibrate(trade.size);
            }
            self.large = None;
        }

        for zone in self.zones.iter_mut() {
            let beyond = match zone.side {
                Some(OrderSide::Buy) => fill.price < zone.low,
                Some(OrderSide::Sell) => fill.price > zone.high,
                None => fill.price < zone.low || fill.price > zone.high,
            };
            if zone.to.is_none()
                && fill.ts >= zone.from
                && Some(zone.number) != self.large
                && beyond
            {
                zone.to = Some(fill.ts);
                out.push(SignalEvent::Upsert(zone.signal()));
            }
        }
        self.zones
            .retain(|zone| zone.to.is_none() || !zone.classified);

        let trade = self.fills.get_or_insert_with(|| Trade {
            aggressor_order_id: fill.aggressor_order_id,
            first_time: fill.ts,
            side: fill.side,
            size: 0.0,
            price: fill.price,
            low: fill.price,
            high: fill.price,
        });
        let scale = trade.size.max(fill.size);
        if scale > 0.0 {
            let weight = (fill.size / scale) / (trade.size / scale + fill.size / scale);
            trade.price += (fill.price - trade.price) * weight;
        }
        trade.size = (trade.size + fill.size).min(f64::MAX);
        if trade.side != fill.side {
            trade.side = None;
        }
        trade.low = trade.low.min(fill.price);
        trade.high = trade.high.max(fill.price);
        let threshold = match rule.threshold {
            Threshold::Fixed(size) => Some(size),
            Threshold::Auto => self.threshold,
        };
        if enabled && threshold.is_some_and(|threshold| trade.size >= threshold) {
            let (low, high) = (trade.low, trade.high);
            let tick = self.tick;
            let (low, high) = if high - low < tick {
                let middle = (low + high) / 2.0;
                (middle - tick / 2.0, middle + tick / 2.0)
            } else {
                (low, high)
            };
            let number = match self.large {
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
                    *self.large.insert(self.next)
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
                label: Some(self.sizes_label.format(trade.size)),
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

    /// Classifies the zones of the forming bar, which closed, removing those not kept.
    fn classify(&mut self, rule: &BigTradeRule, out: &mut Vec<SignalEvent>) {
        let Some(bar) = &self.bar else {
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
                || zone.from < bar.open_time
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
    use super::*;

    #[test]
    fn a_repeating_aggressor_keeps_one_aggregate() {
        let mut detector = BigTrades::new(0.25, Some(0));
        let rule = BigTradeRule {
            threshold: Threshold::Fixed(10.0),
            ..BigTradeRule::default()
        };
        let fill = Tick::new(
            DateTime::from_timestamp(1, 0).unwrap(),
            100.0,
            1.0,
            Some(OrderSide::Buy),
        )
        .with_aggressor_order_id(0);
        let mut out = Vec::new();
        for _ in 0..4000 {
            out.clear();
            detector.trade(&fill, &rule, true, &mut out);
        }
        assert_eq!(
            detector.fills.iter().count(),
            1,
            "retained open trade aggregates"
        );
        let SignalEvent::Upsert(marker) = &out[0] else {
            panic!("missing marker")
        };
        assert_eq!(marker.label.as_deref(), Some("4K"));
        assert_eq!(
            marker.shape,
            SignalShape::Marker {
                time: fill.ts,
                price: Some(100.0),
                direction: Direction::Up,
            }
        );
        assert_eq!(detector.zones.len(), 1);
        assert_eq!(
            (detector.zones[0].low, detector.zones[0].high),
            (99.875, 100.125)
        );
    }
}
