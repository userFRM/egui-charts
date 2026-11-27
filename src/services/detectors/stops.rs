//! Same-side execution bursts and bars that sweep through diagonal imbalances.

use std::collections::{BTreeMap, HashMap, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};

use super::{DetectionKind, DetectorSettings, footprint};
use crate::data::{Footprint, meets_threshold};
use crate::model::{
    BarClock, Direction, FootprintCondition, OrderSide, Placement, Signal, SignalId, SignalShape,
    Tick,
};
use crate::scales::VolumeFormatter;
use crate::services::SignalEvent;

/// A burst of same-side fills, each at most [`max_gap_ms`](Self::max_gap_ms) after the one
/// before, travelling at least [`min_ticks`](Self::min_ticks) from its first fill's price without
/// reversing, with at least [`min_fills`](Self::min_fills) fills and a total size of at least
/// [`min_size`](Self::min_size). If any fill identifies its aggressor, at least
/// [`min_aggressors`](Self::min_aggressors) distinct aggressors are required. Unknown-side fills
/// and fills of size zero are excluded. The default is five fills, 20 ms, three ticks, three
/// aggressors and no size minimum, which settings saved before these fields existed restore.
///
/// ```
/// use egui_charts::services::{StopRunRule, StopSweepRule};
///
/// let mut stop_runs = StopRunRule::default();
/// stop_runs.min_size = 50.0;
/// println!("{:?}", (stop_runs, StopSweepRule::default()));
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct StopRunRule {
    /// Fewest fills in a burst; 0 counts as 1.
    pub min_fills: u64,
    /// Longest time between consecutive fills, in milliseconds; 0 joins only fills at one
    /// instant.
    pub max_gap_ms: u32,
    /// Fewest ticks between the first fill's price and the latest fill's.
    pub min_ticks: u64,
    /// Fewest distinct aggressor order ids when any fill of the burst names one; 0 requires none.
    pub min_aggressors: u8,
    /// Smallest total size of the burst's fills, finite and not negative, checked by
    /// [`DetectorSettings::validate`](super::DetectorSettings::validate); 0 for no minimum.
    pub min_size: f64,
}

impl Default for StopRunRule {
    fn default() -> Self {
        Self {
            min_fills: 5,
            max_gap_ms: 20,
            min_ticks: 3,
            min_aggressors: 3,
            min_size: 0.0,
        }
    }
}

/// A closed bar with directional delta of at least 25%, three diagonal imbalances at 3:1,
/// a body at least 60% of its range, and volume at least 1.5 times the previous ten traded bars.
/// Unknown-side fills never count. The latest subscribed time interval supplies the bars.
/// The first retained bar is left out because its opening executions may be missing.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct StopSweepRule;

#[derive(Default)]
pub(super) struct StopRuns {
    /// Writes the sizes in tooltips.
    sizes_label: VolumeFormatter,
    burst: Option<Burst>,
    next: u64,
}

struct Burst {
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    side: OrderSide,
    open: i64,
    price: i64,
    fills: u64,
    size: f64,
    // Only the rule's minimum of distinct ids is needed, even for a session-long burst.
    ids: Vec<u64>,
    number: Option<u64>,
}

impl StopRuns {
    /// The detector of a symbol whose sizes are written with `volume_decimals` decimals, or 2
    /// like the chart's volume readouts.
    pub(super) fn new(volume_decimals: Option<u8>) -> Self {
        Self {
            sizes_label: VolumeFormatter {
                precision: volume_decimals.map_or(2, usize::from),
            },
            ..Self::default()
        }
    }

    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        tick: f64,
        rule: &StopRunRule,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        let Some(side) = fill.side.filter(|_| fill.size > 0.0) else {
            return;
        };
        let price = (fill.price / tick).round() as i64;
        if self.burst.as_ref().is_none_or(|burst| {
            let gap = fill.ts - burst.last;
            side != burst.side
                || gap < TimeDelta::zero()
                || gap > TimeDelta::milliseconds(i64::from(rule.max_gap_ms))
                || match side {
                    OrderSide::Buy => price < burst.price,
                    OrderSide::Sell => price > burst.price,
                }
        }) {
            self.burst = Some(Burst {
                first: fill.ts,
                last: fill.ts,
                side,
                open: price,
                price,
                fills: 0,
                size: 0.0,
                ids: Vec::new(),
                number: None,
            });
        }
        let burst = self.burst.as_mut().unwrap();
        burst.last = fill.ts;
        burst.price = price;
        burst.fills = burst.fills.saturating_add(1);
        burst.size = (burst.size + fill.size).min(f64::MAX);
        if let Some(id) = fill.aggressor_order_id
            && burst.ids.len() < usize::from(rule.min_aggressors)
            && !burst.ids.contains(&id)
        {
            burst.ids.push(id);
        }
        let ticks = burst.price.abs_diff(burst.open);
        if !enabled
            || burst.fills < rule.min_fills
            || ticks < rule.min_ticks
            || (!burst.ids.is_empty() && burst.ids.len() < usize::from(rule.min_aggressors))
            || !meets_threshold(burst.size, rule.min_size)
        {
            return;
        }
        let number = *burst.number.get_or_insert_with(|| {
            self.next += 1;
            self.next
        });
        out.push(SignalEvent::Upsert(Signal {
            id: SignalId(format!("stop-run-{number}")),
            shape: SignalShape::Marker {
                time: fill.ts,
                price: Some(fill.price),
                direction: direction(side),
            },
            category: DetectionKind::StopRun.category().into(),
            label: Some("Stop run".into()),
            tooltip: Some(format!(
                "{} fills, {ticks} ticks, size {}, {} ms",
                burst.fills,
                self.sizes_label
                    .format_with_min_unit_decimals(burst.size, 2),
                (burst.last - burst.first).num_milliseconds()
            )),
        }));
    }
}

pub(super) fn direction(side: OrderSide) -> Direction {
    match side {
        OrderSide::Buy => Direction::Up,
        OrderSide::Sell => Direction::Down,
    }
}

pub(super) struct StopSweeps {
    tick: f64,
    sizes_label: VolumeFormatter,
    trades: VecDeque<Tick>,
    clock: Option<BarClock>,
    footprint: Option<Footprint>,
    bars: BTreeMap<DateTime<Utc>, Prices>,
    closed: Option<DateTime<Utc>>,
    first: Option<DateTime<Utc>>,
    volumes: VecDeque<f64>,
    last_time: Option<DateTime<Utc>>,
    /// The bar of the last fill or feed time placed.
    placement: Placement,
    /// The bar of the last trade evicted from `trades`, kept apart so that eviction and the live
    /// fills do not displace each other's date.
    evicted: Placement,
    /// The feed time of the first live event since tracking.
    live: Option<DateTime<Utc>>,
    /// The open of that event's bar on the current clock. Footprint conditions of bars that
    /// open earlier are history and raise no alert.
    pub(super) live_open: Option<DateTime<Utc>>,
    /// The latest bar of each footprint condition counted on the current clock, independent of
    /// chart marker retention.
    pub(super) conditions: HashMap<String, DateTime<Utc>>,
    /// Footprint stacked imbalance zones still active on the current clock.
    zones: Vec<footprint::Zone>,
    /// Last forming bar to meet each condition, in `FootprintCondition::ALL` order.
    formed: [Option<DateTime<Utc>>; 4],
    /// Conditions met by live bars since the service last took them.
    pub(super) met: Vec<footprint::Met>,
}

struct Prices {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl StopSweeps {
    pub(super) fn new(tick: f64, volume_decimals: Option<u8>) -> Self {
        Self {
            tick,
            sizes_label: VolumeFormatter {
                precision: volume_decimals.map_or(2, usize::from),
            },
            trades: VecDeque::new(),
            clock: None,
            footprint: None,
            bars: BTreeMap::new(),
            closed: None,
            first: None,
            volumes: VecDeque::new(),
            last_time: None,
            placement: Placement::default(),
            evicted: Placement::default(),
            live: None,
            live_open: None,
            conditions: HashMap::new(),
            zones: Vec::new(),
            formed: [None; 4],
            met: Vec::new(),
        }
    }

    /// Notes feed time `time` of a live event; only the first one since tracking counts.
    pub(super) fn go_live(&mut self, time: DateTime<Utc>) {
        if self.live.is_none() {
            self.live = Some(time);
            self.live_open = self.clock.as_ref().map(|clock| clock.instant_open(time));
        }
    }

    pub(super) fn set_clock(
        &mut self,
        clock: Option<BarClock>,
        settings: &DetectorSettings,
        out: &mut Vec<SignalEvent>,
    ) {
        self.clock = clock;
        self.live_open = self
            .live
            .zip(self.clock.as_ref())
            .map(|(time, clock)| clock.instant_open(time));
        self.conditions.clear();
        self.zones.clear();
        self.footprint = self.clock.clone().map(|clock| {
            let mut footprint = Footprint::new(clock, self.tick, 1).unwrap();
            footprint.set_budget(12, 100_000);
            footprint
        });
        self.bars.clear();
        self.closed = None;
        self.first = None;
        self.volumes.clear();
        let trades = std::mem::take(&mut self.trades);
        for trade in &trades {
            self.aggregate(trade, settings, out);
        }
        self.trades = trades;
        if let Some(time) = self.last_time {
            self.advance(time, settings, out);
        }
        self.met.clear();
        self.formed = [None; 4];
    }

    /// Starts another interval and rule from retained executions without reporting them.
    pub(super) fn watching(&self, clock: Option<BarClock>, settings: &DetectorSettings) -> Self {
        let mut watch = Self::new(self.tick, None);
        watch.sizes_label = self.sizes_label.clone();
        watch.trades = self.trades.clone();
        watch.live = self.live;
        watch.last_time = self.last_time;
        watch.set_clock(clock, settings, &mut Vec::new());
        watch
    }

    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        settings: &DetectorSettings,
        out: &mut Vec<SignalEvent>,
    ) {
        if fill.side.is_none()
            || fill.size <= 0.0
            || self.trades.back().is_some_and(|last| fill.ts < last.ts)
        {
            return;
        }
        self.trades.push_back(fill.clone());
        if self.trades.len() > 100_000
            && let Some(old) = self.trades.pop_front()
            && let Some(clock) = &self.clock
        {
            let cut = self.evicted.open(clock, old.ts);
            if self.first.is_none_or(|first| cut > first) {
                self.first = Some(cut);
                if cut == self.placement.open(clock, fill.ts) {
                    let mut footprint = Footprint::new(clock.clone(), self.tick, 1).unwrap();
                    footprint.set_budget(12, 100_000);
                    self.footprint = Some(footprint);
                    self.bars.clear();
                    self.volumes.clear();
                    self.zones.clear();
                }
            }
        }
        self.aggregate(fill, settings, out);
    }

    fn aggregate(&mut self, fill: &Tick, settings: &DetectorSettings, out: &mut Vec<SignalEvent>) {
        self.advance(fill.ts, settings, out);
        let (Some(clock), Some(footprint)) = (&self.clock, &mut self.footprint) else {
            return;
        };
        let open = self.placement.open(clock, fill.ts);
        let first = *self.first.get_or_insert(open);
        if open <= first {
            return;
        }
        let totals = |bar: &crate::data::FootprintBar| [bar.ask, bar.bid, bar.delta()];
        let before = footprint.bar_at(open).map_or([0.0; 3], totals);
        if !matches!(footprint.push(fill), Ok(true)) {
            return;
        }
        if self.live_open.is_some_and(|live| open >= live)
            && self.formed.iter().any(|formed| *formed != Some(open))
            && let Some(bar) = footprint.bar_at(open)
        {
            for (i, message) in footprint::met(bar, &settings.footprint.rule)
                .into_iter()
                .enumerate()
            {
                if self.formed[i] != Some(open)
                    && let Some(message) = message
                {
                    self.met.push(footprint::Met {
                        condition: FootprintCondition::ALL[i],
                        bar: open,
                        closed: false,
                        time: fill.ts,
                        price: fill.price,
                        message,
                    });
                    self.formed[i] = Some(open);
                }
            }
        }
        if settings.footprint.enabled
            && let Some(bar) = footprint.bar_at(open)
        {
            footprint::forming(
                before,
                totals(bar),
                open,
                fill.price,
                &settings.footprint.rule,
                &self.sizes_label,
                out,
            );
        }
        let bar = self.bars.entry(open).or_insert(Prices {
            open: fill.price,
            high: fill.price,
            low: fill.price,
            close: fill.price,
        });
        bar.high = bar.high.max(fill.price);
        bar.low = bar.low.min(fill.price);
        bar.close = fill.price;
        while self.bars.len() > 12 {
            self.bars.pop_first();
        }
    }

    pub(super) fn advance(
        &mut self,
        time: DateTime<Utc>,
        settings: &DetectorSettings,
        out: &mut Vec<SignalEvent>,
    ) {
        let enabled = settings.stop_sweeps.enabled;
        self.last_time = Some(self.last_time.map_or(time, |last| last.max(time)));
        let (Some(clock), Some(footprint)) = (&self.clock, &self.footprint) else {
            return;
        };
        let open = self.placement.open(clock, time);
        for (&at, prices) in self.bars.range(..open) {
            if self.closed.is_some_and(|closed| at <= closed) {
                continue;
            }
            self.closed = Some(at);
            let bar = footprint
                .bar_at(at)
                .filter(|_| self.first.is_none_or(|first| at > first));
            if settings.footprint.enabled && bar.is_some() {
                footprint::closed(
                    footprint,
                    at,
                    prices.close,
                    &settings.footprint.rule,
                    &mut self.zones,
                    out,
                );
            } else {
                self.zones.clear();
            }
            let Some(bar) = bar else {
                continue;
            };
            if self.live_open.is_some_and(|live| at >= live) {
                for (condition, message) in FootprintCondition::ALL
                    .into_iter()
                    .zip(footprint::met(bar, &settings.footprint.rule))
                {
                    if let Some(message) = message {
                        self.met.push(footprint::Met {
                            condition,
                            bar: at,
                            closed: true,
                            time,
                            price: prices.close,
                            message,
                        });
                    }
                }
            }
            let volume = bar.volume();
            let buy = prices.close > prices.open;
            let delta = if buy { bar.delta() } else { -bar.delta() };
            let count = bar
                .levels
                .keys()
                .filter(|&&level| {
                    let (bid, ask) = bar.imbalance(level, 3.0, true, 0.0);
                    if buy { ask } else { bid }
                })
                .count();
            if enabled
                && prices.high > prices.low
                && meets_threshold(delta, volume * 0.25)
                && meets_threshold(
                    (prices.close - prices.open).abs(),
                    (prices.high - prices.low) * 0.6,
                )
                && count >= 3
                && self.volumes.len() == 10
                && meets_threshold(
                    volume,
                    1.5 * self.volumes.iter().map(|volume| volume / 10.0).sum::<f64>(),
                )
            {
                out.push(SignalEvent::Upsert(Signal {
                    id: SignalId(format!("stop-sweep-{}", at.timestamp_millis())),
                    shape: SignalShape::Marker {
                        time: at,
                        price: Some(prices.close),
                        direction: if buy { Direction::Up } else { Direction::Down },
                    },
                    category: DetectionKind::StopSweep.category().into(),
                    label: Some("Stop sweep".into()),
                    tooltip: Some(format!(
                        "Volume {}, delta {}, {count} diagonal imbalances",
                        self.sizes_label.format_with_min_unit_decimals(volume, 2),
                        self.sizes_label
                            .format_with_min_unit_decimals(bar.delta(), 2)
                    )),
                }));
            }
            self.volumes.push_back(volume);
            if self.volumes.len() > 10 {
                self.volumes.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Host, time};
    use super::*;
    use crate::data::MarketEvent;
    use crate::model::{Instrument, Timeframe};

    /// A sweep's six fills sum to 3.3 with delta 2.7; the final 0.01 gives 3.31 and 2.71.
    /// Hover and alert text share the instrument's units, with two decimals without them.
    #[test]
    fn stop_sweep_hover_and_alert_write_volume_decimals() {
        for (decimals, scale, last, expected) in [
            (
                Some(1),
                0.1,
                0.01,
                "Volume 3.3, delta 2.7, 3 diagonal imbalances",
            ),
            (
                None,
                0.1,
                0.01,
                "Volume 3.31, delta 2.71, 3 diagonal imbalances",
            ),
            (
                Some(0),
                1.0,
                0.0,
                "Volume 33, delta 27, 3 diagonal imbalances",
            ),
            (
                Some(0),
                100.0,
                0.0,
                "Volume 3.30K, delta 2.70K, 3 diagonal imbalances",
            ),
        ] {
            let mut instrument = Instrument::new("ES").with_tick_size(0.25);
            instrument.volume_decimals = decimals;
            let host = Host::tracking(&instrument, Timeframe::Min1, None);
            let mut settings = host.detectors.settings();
            settings.stop_sweeps.enabled = true;
            settings.stop_sweeps.alert = true;
            host.detectors.set_settings(settings);
            for minute in 0..11 {
                host.push(MarketEvent::Trade(Tick::new(
                    time(minute * 60),
                    100.0,
                    20.0 * scale,
                    Some(OrderSide::Buy),
                )));
            }
            for (second, price, size, side) in [
                (660, 100.0, scale, OrderSide::Sell),
                (661, 100.25, scale, OrderSide::Sell),
                (662, 100.5, scale, OrderSide::Sell),
                (663, 100.25, 10.0 * scale, OrderSide::Buy),
                (664, 100.5, 10.0 * scale, OrderSide::Buy),
                (665, 100.75, 10.0 * scale, OrderSide::Buy),
                (666, 100.75, last, OrderSide::Buy),
            ] {
                host.push(MarketEvent::Trade(Tick::new(
                    time(second),
                    price,
                    size,
                    Some(side),
                )));
            }
            let events = host.push(MarketEvent::Trade(Tick::new(
                time(720),
                100.75,
                1.0,
                Some(OrderSide::Buy),
            )));
            let sweeps: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    SignalEvent::Upsert(signal) if signal.category == "Stop sweeps" => Some(signal),
                    _ => None,
                })
                .collect();
            assert_eq!(sweeps.len(), 1, "volume decimals {decimals:?}");
            assert_eq!(
                sweeps[0].tooltip.as_deref(),
                Some(expected),
                "volume decimals {decimals:?}"
            );
            let alerts = host.detectors.take_alerts();
            assert_eq!(alerts.len(), 1, "volume decimals {decimals:?}");
            assert_eq!(
                alerts[0].notification.body, expected,
                "volume decimals {decimals:?}"
            );
        }
    }

    #[test]
    fn stop_runs_follow_each_limit_of_their_rule() {
        type Row = (
            &'static str,
            fn(&mut StopRunRule),
            Vec<u64>,
            i64,
            Vec<i64>,
            Vec<f64>,
            bool,
            Option<&'static str>,
        );
        let prices = vec![400, 401, 401, 402, 403];
        let rows: Vec<Row> = vec![
            (
                "three aggressors",
                |_| {},
                vec![1, 1, 2, 2, 3],
                20,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 80 ms"),
            ),
            (
                "no ids",
                |_| {},
                vec![],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 40 ms"),
            ),
            (
                "one aggressor",
                |_| {},
                vec![1; 5],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "gap",
                |_| {},
                vec![],
                21,
                prices.clone(),
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "reversal",
                |_| {},
                vec![],
                10,
                vec![400, 401, 400, 402, 403],
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "unknown side",
                |_| {},
                vec![],
                10,
                prices.clone(),
                vec![2.0; 5],
                false,
                None,
            ),
            (
                "fractional sizes",
                |_| {},
                vec![],
                10,
                prices.clone(),
                vec![0.1, 0.2, 0.1, 0.2, 0.1],
                true,
                Some("5 fills, 3 ticks, size 0.7, 40 ms"),
            ),
            (
                "fills at limit",
                |r| r.min_fills = 7,
                vec![],
                10,
                (400..=406).collect(),
                vec![2.0; 7],
                true,
                Some("7 fills, 6 ticks, size 14.0, 60 ms"),
            ),
            (
                "fills beside limit",
                |r| r.min_fills = 8,
                vec![],
                10,
                (400..=406).collect(),
                vec![2.0; 7],
                true,
                None,
            ),
            (
                "gap at limit",
                |r| r.max_gap_ms = 50,
                vec![],
                50,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 200 ms"),
            ),
            (
                "gap beside limit",
                |r| r.max_gap_ms = 49,
                vec![],
                50,
                prices.clone(),
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "ticks at limit",
                |r| r.min_ticks = 5,
                vec![],
                10,
                vec![400, 401, 402, 403, 405],
                vec![2.0; 5],
                true,
                Some("5 fills, 5 ticks, size 10.0, 40 ms"),
            ),
            (
                "ticks beside limit",
                |r| r.min_ticks = 6,
                vec![],
                10,
                vec![400, 401, 402, 403, 405],
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "aggressors at limit",
                |r| r.min_aggressors = 5,
                vec![1, 2, 3, 4, 5],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 40 ms"),
            ),
            (
                "aggressors beside limit",
                |r| r.min_aggressors = 5,
                vec![1, 2, 3, 4, 4],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "no aggressor minimum",
                |r| r.min_aggressors = 0,
                vec![1; 5],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 40 ms"),
            ),
            (
                "size at limit",
                |r| r.min_size = 10.0,
                vec![],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                Some("5 fills, 3 ticks, size 10.0, 40 ms"),
            ),
            (
                "size beside limit",
                |r| r.min_size = 10.5,
                vec![],
                10,
                prices.clone(),
                vec![2.0; 5],
                true,
                None,
            ),
            (
                "fractional size minimum",
                |r| r.min_size = 1.8,
                vec![],
                10,
                prices.clone(),
                vec![0.1, 0.1, 0.2, 0.7, 0.7],
                true,
                Some("5 fills, 3 ticks, size 1.8, 40 ms"),
            ),
        ];
        for (name, edit, ids, gap, prices, sizes, known, expected) in rows {
            for side in [OrderSide::Buy, OrderSide::Sell] {
                let host = Host::tracking(
                    &Instrument::new("ES")
                        .with_tick_size(0.25)
                        .with_volume_decimals(1),
                    Timeframe::Min1,
                    Some(1_000.0),
                );
                let mut settings = host.detectors.settings();
                settings.stop_runs.enabled = true;
                settings.stop_runs.alert = true;
                edit(&mut settings.stop_runs.rule);
                host.detectors.set_settings(settings);
                let mut events = Vec::new();
                for (i, price) in prices.iter().enumerate() {
                    let price = if side == OrderSide::Buy {
                        *price
                    } else {
                        800 - price
                    };
                    let mut fill = Tick::new(
                        time(0) + TimeDelta::milliseconds(i as i64 * gap),
                        price as f64 * 0.25,
                        sizes[i],
                        known.then_some(side),
                    );
                    fill.aggressor_order_id = ids.get(i).copied();
                    events.extend(host.push(MarketEvent::Trade(fill)));
                }
                let signals: Vec<_> = events
                    .into_iter()
                    .filter_map(|event| match event {
                        SignalEvent::Upsert(signal) if signal.category == "Stop runs" => {
                            Some(signal)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    signals.len(),
                    usize::from(expected.is_some()),
                    "{name} {side:?}"
                );
                if let Some(signal) = signals.first() {
                    assert_eq!(signal.tooltip.as_deref(), expected, "{name} {side:?}");
                    let alert = &host.detectors.take_alerts()[0];
                    assert_eq!(
                        alert.notification.body,
                        expected.unwrap(),
                        "{name} {side:?}"
                    );
                    let last = prices[prices.len() - 1];
                    assert_eq!(
                        signal.shape,
                        SignalShape::Marker {
                            time: time(0)
                                + TimeDelta::milliseconds((prices.len() - 1) as i64 * gap),
                            price: Some(
                                if side == OrderSide::Buy {
                                    last
                                } else {
                                    800 - last
                                } as f64
                                    * 0.25
                            ),
                            direction: direction(side),
                        },
                        "{name} {side:?}"
                    );
                }
            }
        }
    }
}
