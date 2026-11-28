//! Footprint conditions on the bars of the latest subscribed time interval.

use chrono::{DateTime, Utc};

use super::DetectionKind;
use crate::data::{Footprint, FootprintBar};
use crate::model::{
    ChartTypeParams, Direction, FootprintAlertRule, FootprintCondition, Signal, SignalId,
    SignalShape,
};
use crate::scales::VolumeFormatter;
use crate::services::SignalEvent;

/// Footprint conditions that mark a bar of the latest subscribed time interval, each at most
/// once per bar, and alert as [`Detector::alert`](super::Detector::alert) says.
///
/// Imbalances are read when a bar closes, on rows of [`row_ticks`](Self::row_ticks) instrument
/// ticks, with this rule's ratio, comparison, minimum volume and stack, as
/// [`FootprintBar::imbalance`](crate::data::FootprintBar::imbalance) reads them and the footprint's
/// stacked imbalance zones join them; [`follow_chart`](Self::follow_chart) copies these from a
/// chart. The stacks of each closed bar become zones that stay active, through any number of
/// rejections, until a later bar closes beyond their far edge: below the lowest row of a buy
/// zone or above the highest row of a sell zone. A later closed bar that trades into an active
/// zone without such a close is a zone rejection; the bar that closes beyond it ends the zone
/// and fires nothing. Zones come from bars closed while footprint conditions are enabled.
/// A closed bar skipped because conditions are disabled or its retained footprint is incomplete
/// or missing ends all active zones. Disabling and enabling inside one bar keeps them if its
/// close is read.
/// Volume and delta levels are read on the forming bar after each sided execution: a level
/// fires on the execution that takes the value past it, by more than the rounding of a sum of
/// sizes, from the bar's previous value, which starts at zero. Unknown-side executions never
/// count. Trade-based intervals and the first retained bar have no conditions, and a bar that
/// opened before the bar of the first live event since tracking or
/// [`reset`](super::OrderFlowDetectors::reset), such as loaded history, is marked without an
/// alert.
///
/// ```
/// use egui_charts::model::ChartTypeParams;
/// use egui_charts::services::OrderFlowDetectors;
///
/// let detectors = OrderFlowDetectors::new();
/// let mut settings = detectors.settings();
/// settings.footprint.enabled = true;
/// settings.footprint.alert = true;
/// settings.footprint.rule.stacked_buy_imbalance = true;
/// settings.footprint.rule.sell_volume_above = Some(100.0);
/// settings.footprint.rule.follow_chart(&ChartTypeParams::default(), 1);
/// detectors.set_settings(settings);
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct FootprintRule {
    /// A closed bar with a row where buying dominates.
    pub buy_imbalance: bool,
    /// A closed bar with a row where selling dominates.
    pub sell_imbalance: bool,
    /// A closed bar with at least [`stack`](Self::stack) adjacent rows where buying dominates.
    pub stacked_buy_imbalance: bool,
    /// A closed bar with at least [`stack`](Self::stack) adjacent rows where selling dominates.
    pub stacked_sell_imbalance: bool,
    /// A closed bar that trades into an active stacked buy imbalance zone without closing below
    /// its lowest row.
    pub buy_zone_rejection: bool,
    /// A closed bar that trades into an active stacked sell imbalance zone without closing above
    /// its highest row.
    pub sell_zone_rejection: bool,
    /// The forming bar's buy volume rises above this finite, non-negative level.
    pub buy_volume_above: Option<f64>,
    /// The forming bar's sell volume rises above this finite, non-negative level.
    pub sell_volume_above: Option<f64>,
    /// The forming bar's delta rises above this finite level.
    pub delta_above: Option<f64>,
    /// The forming bar's delta falls below this finite level.
    pub delta_below: Option<f64>,
    /// Dominance multiple of an imbalance, 3 by default. Below 1 or not a number, none.
    pub imbalance_ratio: f64,
    /// Compare a side with the opposite side one row away rather than at the same price, by
    /// default.
    pub diagonal: bool,
    /// Inclusive minimum volume of the dominant side, 0 by default. Negative or not finite, none.
    pub imbalance_min_volume: f64,
    /// Adjacent imbalances that make a stack, at least one; 3 by default.
    pub stack: usize,
    /// Instrument ticks per row that imbalances, stacks and zones read, grouped downwards from
    /// price zero as the chart's footprint groups them; 1 by default, and 0 counts as 1. Settings
    /// saved without it read rows of one tick.
    pub row_ticks: u32,
}

impl Default for FootprintRule {
    fn default() -> Self {
        Self {
            buy_imbalance: false,
            sell_imbalance: false,
            stacked_buy_imbalance: false,
            stacked_sell_imbalance: false,
            buy_zone_rejection: false,
            sell_zone_rejection: false,
            buy_volume_above: None,
            sell_volume_above: None,
            delta_above: None,
            delta_below: None,
            imbalance_ratio: 3.0,
            diagonal: true,
            imbalance_min_volume: 0.0,
            stack: 3,
            row_ticks: 1,
        }
    }
}

impl FootprintRule {
    /// Reads only the saved alert rule, leaving marker and panel switches disabled.
    pub(super) fn reading(rule: FootprintAlertRule) -> Self {
        Self {
            imbalance_ratio: rule.imbalance_ratio,
            diagonal: rule.diagonal,
            imbalance_min_volume: rule.imbalance_min_volume,
            stack: rule.stack,
            row_ticks: rule.row_ticks,
            ..Self::default()
        }
    }

    /// Takes the imbalance ratio, comparison, minimum volume and stack of the footprint under
    /// `params`, such as a chart's `Chart::chart_type_params`, and `row_ticks`, the instrument
    /// ticks per row that footprint shows, such as `Chart::footprint_row_ticks`, so that alerts
    /// read the imbalances and zones that chart shows.
    pub fn follow_chart(&mut self, params: &ChartTypeParams, row_ticks: u32) {
        self.imbalance_ratio = params.footprint_imbalance_ratio;
        self.diagonal = params.footprint_diagonal;
        self.imbalance_min_volume = params.footprint_imbalance_min_volume;
        self.stack = params.footprint_stack;
        self.row_ticks = row_ticks;
    }
}

/// A stacked imbalance zone of an earlier closed bar, with the instrument ticks of its lowest and
/// highest row, so it keeps its prices when the rows change, active until a later bar closes
/// beyond its far edge or a closed bar is skipped.
#[derive(PartialEq, Eq)]
pub(super) struct Zone {
    low: i64,
    high: i64,
    /// Buy imbalances support from the low; sell imbalances resist from the high.
    buy: bool,
}

/// Active zones kept per interval. Zones far from the price can stay active all session; the
/// oldest go first.
const ZONES: usize = 1_000;

/// A footprint condition met by a live bar, before its symbol and interval are known.
pub(super) struct Met {
    pub(super) condition: FootprintCondition,
    pub(super) bar: DateTime<Utc>,
    pub(super) bar_end: Option<DateTime<Utc>>,
    pub(super) closed: bool,
    pub(super) time: DateTime<Utc>,
    pub(super) price: f64,
    pub(super) message: String,
}

/// Imbalanced row counts, longest adjacent runs (sell then buy), and stacked zones.
fn imbalances(bar: &FootprintBar, rule: &FootprintRule) -> ([usize; 2], [usize; 2], Vec<Zone>) {
    // The rows the chart shows: `row_ticks` one-tick rows summed into the row keyed by their
    // quotient, as `Footprint` groups them.
    let group = i64::from(rule.row_ticks.clamp(1, 1_000_000));
    let grouped;
    let bar = if group == 1 {
        bar
    } else {
        let mut rows = crate::data::FootprintBar::empty(bar.time);
        rows.absorb_grouped(bar, group);
        grouped = rows;
        &grouped
    };
    // The instrument ticks from the lowest of row `low` to the highest of row `high`.
    let ticks = |low: i64, high: i64| {
        (
            low.saturating_mul(group),
            high.saturating_mul(group).saturating_add(group - 1),
        )
    };
    let stack = rule.stack.max(1);
    // Per side, sell then buy: imbalanced rows, the run of adjacent ones, the run's first row and
    // the longest run.
    let (mut rows, mut run, mut first, mut longest) =
        ([0_usize; 2], [0_usize; 2], [0_i64; 2], [0_usize; 2]);
    let mut stacks = Vec::new();
    let mut previous: Option<i64> = None;
    for &level in bar.levels.keys() {
        let (sell, buy) = bar.imbalance(
            level,
            rule.imbalance_ratio,
            rule.diagonal,
            rule.imbalance_min_volume,
        );
        let adjacent = previous.and_then(|key| key.checked_add(1)) == Some(level);
        for (side, active) in [sell, buy].into_iter().enumerate() {
            if let Some(high) = previous.filter(|_| run[side] >= stack && !(active && adjacent)) {
                let (low, high) = ticks(first[side], high);
                stacks.push(Zone {
                    low,
                    high,
                    buy: side == 1,
                });
            }
            run[side] = match (active, adjacent) {
                (false, _) => 0,
                (true, true) if run[side] > 0 => run[side] + 1,
                (true, _) => {
                    first[side] = level;
                    1
                }
            };
            rows[side] += usize::from(active);
            longest[side] = longest[side].max(run[side]);
        }
        previous = Some(level);
    }
    for side in 0..2 {
        if let Some(high) = previous.filter(|_| run[side] >= stack) {
            let (low, high) = ticks(first[side], high);
            stacks.push(Zone {
                low,
                high,
                buy: side == 1,
            });
        }
    }
    (rows, longest, stacks)
}

/// Messages of the conditions `bar` meets, in `FootprintCondition::ALL` order, independent of
/// the detector's switches.
pub(super) fn met(bar: &FootprintBar, rule: &FootprintRule) -> [Option<String>; 4] {
    let (rows, longest, _) = imbalances(bar, rule);
    [
        (rows[1] > 0, "Buy imbalance", "on", rows[1]),
        (rows[0] > 0, "Sell imbalance", "on", rows[0]),
        (
            longest[1] >= rule.stack.max(1),
            "Stacked buy imbalance",
            "of",
            longest[1],
        ),
        (
            longest[0] >= rule.stack.max(1),
            "Stacked sell imbalance",
            "of",
            longest[0],
        ),
    ]
    .map(|(met, label, preposition, count)| {
        met.then(|| {
            let unit = if count == 1 { "row" } else { "rows" };
            format!("{label} {preposition} {count} {unit}")
        })
    })
}

/// Pushes the imbalance and zone rejection conditions of `rule` that the closed bar of `flow`
/// opened at `at` and closed at `close` meets, then ends the `zones` it broke and adds its own.
pub(super) fn closed(
    flow: &Footprint,
    at: DateTime<Utc>,
    close: f64,
    rule: &FootprintRule,
    zones: &mut Vec<Zone>,
    out: &mut Vec<SignalEvent>,
) {
    let Some(bar) = flow.bar_at(at) else {
        return;
    };
    let (_, _, mut stacks) = imbalances(bar, rule);
    for ((message, enabled), (key, label, up)) in met(bar, rule)
        .into_iter()
        .zip([
            rule.buy_imbalance,
            rule.sell_imbalance,
            rule.stacked_buy_imbalance,
            rule.stacked_sell_imbalance,
        ])
        .zip([
            ("buy-imbalance", "Buy imbalance", true),
            ("sell-imbalance", "Sell imbalance", false),
            ("stacked-buy", "Stacked buy imbalance", true),
            ("stacked-sell", "Stacked sell imbalance", false),
        ])
    {
        if enabled && let Some(tooltip) = message {
            out.push(signal(key, at, close, up, label, tooltip));
        }
    }
    // Earlier zones only: a close beyond the far edge ends one, trading into it rejects it.
    let row = flow.level_of(close);
    let traded = (bar.levels.keys().next())
        .zip(bar.levels.keys().next_back())
        .map(|(&low, &high)| {
            let group = i64::from(rule.row_ticks.clamp(1, 1_000_000));
            (
                low.div_euclid(group).saturating_mul(group),
                high.div_euclid(group)
                    .saturating_mul(group)
                    .saturating_add(group - 1),
            )
        });
    let decimals = crate::scales::decimals(flow.tick_size());
    let price = |key: i64| format!("{:.decimals$}", key as f64 * flow.tick_size());
    let mut rejected: [Vec<String>; 2] = Default::default();
    zones.retain(|zone| {
        let broken = row.is_some_and(|row| {
            if zone.buy {
                row < zone.low
            } else {
                row > zone.high
            }
        });
        if !broken && traded.is_some_and(|(low, high)| low <= zone.high && high >= zone.low) {
            rejected[usize::from(zone.buy)].push(format!(
                "{} to {}",
                price(zone.low),
                price(zone.high)
            ));
        }
        !broken
    });
    for (met, side, key, label, up) in [
        (
            rule.buy_zone_rejection,
            1,
            "buy-zone-rejection",
            "Buy zone rejection",
            true,
        ),
        (
            rule.sell_zone_rejection,
            0,
            "sell-zone-rejection",
            "Sell zone rejection",
            false,
        ),
    ] {
        if met && !rejected[side].is_empty() {
            let tooltip = format!("{label} at {}", rejected[side].join(", "));
            out.push(signal(key, at, close, up, label, tooltip));
        }
    }
    stacks.retain(|zone| !zones.contains(zone));
    zones.extend(stacks);
    if zones.len() > ZONES {
        zones.drain(..zones.len() - ZONES);
    }
}

/// Pushes the volume and delta levels of `rule` that one execution at `price` takes the forming
/// bar opened at `at` past, from `before` to `after`, each its buy volume, sell volume and delta,
/// writing values and levels with `sizes_label`.
pub(super) fn forming(
    before: [f64; 3],
    after: [f64; 3],
    at: DateTime<Utc>,
    price: f64,
    rule: &FootprintRule,
    sizes_label: &VolumeFormatter,
    out: &mut Vec<SignalEvent>,
) {
    for (level, index, above, key, label) in [
        (
            rule.buy_volume_above,
            0,
            true,
            "buy-volume",
            "Buy volume above",
        ),
        (
            rule.sell_volume_above,
            1,
            true,
            "sell-volume",
            "Sell volume above",
        ),
        (rule.delta_above, 2, true, "delta-above", "Delta above"),
        (rule.delta_below, 2, false, "delta-below", "Delta below"),
    ] {
        let Some(level) = level else { continue };
        let past = |value: f64| {
            if above {
                exceeds(value, level)
            } else {
                exceeds(level, value)
            }
        };
        if !past(before[index]) && past(after[index]) {
            let name = ["Buy volume", "Sell volume", "Delta"][index];
            let side = if above { "above" } else { "below" };
            let tooltip = format!(
                "{name} {} {side} {}",
                sizes_label.format_with_min_unit_decimals(after[index], 2),
                sizes_label.format_with_min_unit_decimals(level, 2)
            );
            out.push(signal(key, at, price, above && index != 1, label, tooltip));
        }
    }
}

/// Whether `value` exceeds `level` by more than the rounding of a sum of sizes.
fn exceeds(value: f64, level: f64) -> bool {
    value - level > 1e-9 * value.abs().max(level.abs())
}

fn signal(
    key: &str,
    at: DateTime<Utc>,
    price: f64,
    up: bool,
    label: &str,
    tooltip: String,
) -> SignalEvent {
    SignalEvent::Upsert(Signal {
        id: SignalId(format!("footprint-{key}-{}", at.timestamp_millis())),
        shape: SignalShape::Marker {
            time: at,
            price: Some(price),
            direction: if up { Direction::Up } else { Direction::Down },
        },
        category: DetectionKind::FootprintCondition.category().into(),
        label: Some(label.into()),
        tooltip: Some(tooltip),
    })
}
