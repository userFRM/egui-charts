//! # Volume Bubbles Overlay
//!
//! Draws the trades of the chart's series as bubbles, grouped as [`BubbleGrouping`] says, by
//! default one per bar and price level: the diameter grows with the volume traded there and the
//! colour shows the aggressor side or the delta, as [`BubbleColor`] says.
//! Turn them on with [`Chart::set_volume_bubbles`](crate::widget::Chart::set_volume_bubbles).

use crate::model::{Bar, BarClock, OrderSide, Tick};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, TimeDelta, Utc};
use egui::{Color32, Mesh, Painter, Pos2, Shape, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Configuration for volume bubbles overlay. Serializable, so a host can keep it with its chart
/// settings; fields missing from older documents take their defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeBubblesConfig {
    /// Minimum bubble diameter
    pub min_diameter: f32,
    /// Maximum bubble diameter
    pub max_diameter: f32,
    /// Opacity for bubbles
    pub opacity: u8,
    /// Color for buy trades
    pub buy_color: Color32,
    /// Color for sell trades
    pub sell_color: Color32,
    /// Color for bubbles without a side
    pub neutral_color: Color32,
    /// What the colour of a bubble shows
    pub color_by: BubbleColor,
    /// Size scaling mode
    pub scaling_mode: BubbleScalingMode,
    /// Volumes more than this many standard deviations above the mean of the visible bubbles
    /// draw at the maximum diameter, so one large print does not shrink the rest. Infinity
    /// turns the clamp off.
    pub clamp_std_devs: f64,
    /// Show border around bubbles
    pub show_border: bool,
    /// Border color
    pub border_color: Color32,
    /// Bubbles with less volume are neither drawn nor scaled over
    pub min_size_filter: Option<f64>,
    /// Which trades share a bubble.
    pub grouping: BubbleGrouping,
}

impl Default for VolumeBubblesConfig {
    fn default() -> Self {
        Self {
            min_diameter: DESIGN_TOKENS.spacing.sm,
            max_diameter: DESIGN_TOKENS.sizing.charts_ext.volume_bubbles_max_diameter,
            opacity: 180,
            buy_color: DESIGN_TOKENS.semantic.chart.bullish,
            sell_color: DESIGN_TOKENS.semantic.chart.bearish,
            neutral_color: DESIGN_TOKENS.semantic.ui.text_muted_dark,
            color_by: BubbleColor::Side,
            scaling_mode: BubbleScalingMode::Logarithmic,
            clamp_std_devs: 3.0,
            show_border: true,
            border_color: Color32::from_rgba_unmultiplied(255, 255, 255, 100),
            min_size_filter: None,
            grouping: BubbleGrouping::Automatic,
        }
    }
}

/// Which trades share a bubble. Each group is one bubble, whatever the chart's bar interval,
/// except for [`Automatic`](Self::Automatic).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BubbleGrouping {
    /// One bubble per bar and price level, the level being the instrument's tick size, or every
    /// traded price when the feed reports none. Levels of a bar that fall on one pixel row draw
    /// as one bubble, so the shapes are bounded by the visible bars times the pane height.
    Automatic,
    /// One bubble per execution, at its own time and price, so fills of one price and instant
    /// stay separate bubbles even when they land on one pixel. Draws every visible execution,
    /// at most the chart's bounded trade store.
    Ungrouped,
    /// One bubble per price level and time window. Windows start on multiples of their length
    /// counted from the Unix epoch, and a bubble sits in the middle of its window. A length
    /// below a millisecond counts as one millisecond.
    TimeWindow(TimeDelta),
    /// One bubble per bar and price level of this height, which must be finite and positive,
    /// else every traded price has its own bubble. Levels on one pixel row merge as with
    /// [`Automatic`](Self::Automatic).
    PriceStep(f64),
    /// One bubble per this many consecutive executions, at the time of the last of them and the
    /// volume-weighted price of all of them. The newest group grows until it is full. Groups
    /// count from the oldest held execution, and a count of zero counts as one.
    Executions(usize),
}

/// How to scale bubble sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BubbleScalingMode {
    /// Linear scaling
    Linear,
    /// Logarithmic scaling (better for large range)
    Logarithmic,
    /// Square root scaling
    SquareRoot,
}

/// What the colour of a bubble shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BubbleColor {
    /// The aggressor side with more volume, or the neutral colour on a tie and when no trade
    /// has a side.
    Side,
    /// The delta as a share of the volume with a side: the sell colour when every trade sold,
    /// the buy colour when every trade bought, and the blend between them in between.
    Delta,
    /// Two circles on one centre, each sized by its own side's volume, so the smaller one shows
    /// inside the larger: the sell colour for the bid side, the buy colour for the ask side.
    /// Volume of unknown side is left out, and a bubble without any volume with a side is one
    /// neutral circle.
    Split,
    /// The sign of the delta picks the buy or sell colour, and its absolute size, relative to
    /// the largest absolute delta of the visible bubbles, the opacity, down to a fifth of it.
    /// A zero delta, and a bubble without volume with a side, take the neutral colour.
    AbsoluteDelta,
    /// The delta as a share of the whole volume, unknown side included: the neutral colour
    /// when it is zero, fading to the buy or sell colour as the share nears 100%, so volume of
    /// unknown side pulls the colour toward neutral.
    DeltaPercent,
    /// The neutral colour for every bubble: the diameter alone shows the volume.
    Volume,
}

/// The trades of one bubble.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Bubble {
    /// Open time of the bar, middle of the window, or time of the execution or of the last one.
    pub(crate) at: DateTime<Utc>,
    /// The price level, or the traded price without a step.
    pub(crate) price: f64,
    /// Volume of the trades that bought.
    pub(crate) buy: f64,
    /// Volume of the trades that sold.
    pub(crate) sell: f64,
    /// Volume of every trade, with those of unknown side.
    pub(crate) volume: f64,
    pub(crate) trades: usize,
    /// Trades that bought, so a side left without trades shows no rounding residue.
    pub(crate) buys: usize,
    /// Trades that sold.
    pub(crate) sells: usize,
}

impl Bubble {
    /// Adds the trades of `other`, a bubble drawn at the same place.
    fn merge(&mut self, other: &Bubble) {
        self.buy += other.buy;
        self.sell += other.sell;
        self.volume += other.volume;
        self.trades += other.trades;
        self.buys += other.buys;
        self.sells += other.sells;
    }

    /// The bubble of consecutive executions: at the time of the last, at their volume-weighted
    /// price, or the last price when none has volume.
    fn of_group<'a>(trades: impl Iterator<Item = &'a Tick>) -> Self {
        let mut weighted = 0.0;
        let mut group = Self::default();
        for trade in trades {
            weighted += trade.price * trade.size;
            group.merge(&Self::of(trade));
            group.at = trade.ts;
            group.price = trade.price;
        }
        if group.volume > 0.0 {
            group.price = weighted / group.volume;
        }
        group
    }

    /// The bubble of one execution.
    fn of(trade: &Tick) -> Self {
        let (buy, sell) = match trade.side {
            Some(OrderSide::Buy) => (trade.size, 0.0),
            Some(OrderSide::Sell) => (0.0, trade.size),
            None => (0.0, 0.0),
        };
        Self {
            at: trade.ts,
            price: trade.price,
            buy,
            sell,
            volume: trade.size,
            trades: 1,
            buys: usize::from(trade.side == Some(OrderSide::Buy)),
            sells: usize::from(trade.side == Some(OrderSide::Sell)),
        }
    }
}

/// Counts a trade in (`added`) or out, `count` being the trades whose volume `sum` holds.
fn count(count: &mut usize, sum: &mut f64, size: f64, added: bool) {
    *count = match added {
        true => *count + 1,
        false => count.saturating_sub(1),
    };
    *sum = if *count == 0 { 0.0 } else { *sum + size };
}

/// The trades the chart holds, summed as the grouping says, and how to draw them.
pub(crate) struct VolumeBubbles {
    pub(crate) config: VolumeBubblesConfig,
    clock: BarClock,
    step: Option<f64>,
    /// Milliseconds of a time window, for [`BubbleGrouping::TimeWindow`].
    window: i64,
    /// Executions of a group, for [`BubbleGrouping::Executions`].
    group: usize,
    /// By bar open time or window start, then price level, so the bubbles of a run of bars are
    /// one range. Empty for the groupings that keep the trades.
    bubbles: BTreeMap<(DateTime<Utc>, i64), Bubble>,
    /// The held trades by time, for the groupings that keep them: those of the chart's bounded
    /// trade store, which says when one is evicted.
    trades: VecDeque<Tick>,
    /// Trades evicted from the front of `trades`, so groups of executions keep their edges as
    /// the oldest trades go.
    evicted: usize,
    /// The mesh of the last frame, refilled in place once the tessellator has let go of it.
    mesh: Mutex<Arc<Mesh>>,
}

impl VolumeBubbles {
    /// Sums `trades` as the configured grouping says, a price level being the configured step
    /// high, else `tick_size`.
    pub(crate) fn new<'a>(
        config: VolumeBubblesConfig,
        clock: BarClock,
        tick_size: Option<f64>,
        trades: impl IntoIterator<Item = &'a Tick>,
    ) -> Self {
        let step = match config.grouping {
            BubbleGrouping::PriceStep(step) => Some(step),
            _ => tick_size,
        }
        .filter(|step| step.is_finite() && *step > 0.0);
        let window = match config.grouping {
            BubbleGrouping::TimeWindow(window) => window.num_milliseconds().max(1),
            _ => 1,
        };
        let group = match config.grouping {
            BubbleGrouping::Executions(count) => count.max(1),
            _ => 1,
        };
        let mut bubbles = Self {
            config,
            clock,
            step,
            window,
            group,
            bubbles: BTreeMap::new(),
            trades: VecDeque::new(),
            evicted: 0,
            mesh: Mutex::default(),
        };
        for trade in trades {
            bubbles.fold(trade, true);
        }
        bubbles
    }

    /// Adds a trade the chart took (`added`), or takes out one it dropped.
    pub(crate) fn fold(&mut self, trade: &Tick, added: bool) {
        if matches!(
            self.config.grouping,
            BubbleGrouping::Ungrouped | BubbleGrouping::Executions(_)
        ) {
            self.fold_trade(trade, added);
            return;
        }
        let (level, price) = match self.step {
            Some(step) => {
                let level = (trade.price / step).round();
                (level as i64, level * step)
            }
            // Trades are finite, so equal keys are equal prices; the flip orders negative prices
            // as `f64::total_cmp` does.
            None => {
                let bits = trade.price.to_bits() as i64;
                (bits ^ (((bits >> 63) as u64) >> 1) as i64, trade.price)
            }
        };
        let (open, at) = match self.config.grouping {
            BubbleGrouping::TimeWindow(_) => {
                let millis = trade.ts.timestamp_millis();
                let start = millis - millis.rem_euclid(self.window);
                let start = DateTime::from_timestamp_millis(start).unwrap_or(trade.ts);
                (start, start + TimeDelta::milliseconds(self.window / 2))
            }
            _ => {
                let bar = self.clock.bar_open(trade.ts);
                (bar, bar)
            }
        };
        let key = (open, level);
        let bubble = self.bubbles.entry(key).or_insert_with(|| Bubble {
            at,
            price,
            ..Bubble::default()
        });
        let size = if added { trade.size } else { -trade.size };
        count(&mut bubble.trades, &mut bubble.volume, size, added);
        match trade.side {
            Some(OrderSide::Buy) => count(&mut bubble.buys, &mut bubble.buy, size, added),
            Some(OrderSide::Sell) => count(&mut bubble.sells, &mut bubble.sell, size, added),
            None => {}
        }
        if bubble.trades == 0 {
            self.bubbles.remove(&key);
        }
    }

    /// Keeps `trade` in time order, after those at the same instant, or drops one equal to it.
    fn fold_trade(&mut self, trade: &Tick, added: bool) {
        if added {
            let at = self.trades.partition_point(|held| held.ts <= trade.ts);
            self.trades.insert(at, trade.clone());
            return;
        }
        let first = self.trades.partition_point(|held| held.ts < trade.ts);
        let end = self.trades.partition_point(|held| held.ts <= trade.ts);
        if let Some(at) = (first..end).find(|&at| self.trades[at] == *trade) {
            self.trades.remove(at);
            self.evicted += usize::from(at == 0);
        }
    }

    /// The bubbles from the bars that open from `first` to `last`, in time order, by price level
    /// within a bar. With a time window, those of the windows that start from a window before
    /// `first`; with executions kept, those of the executions at those times, and of the groups
    /// they belong to.
    pub(crate) fn between(
        &self,
        first: DateTime<Utc>,
        last: DateTime<Utc>,
    ) -> impl Iterator<Item = Bubble> {
        let last = last.max(first);
        let mut shown = Vec::new();
        match self.config.grouping {
            BubbleGrouping::Ungrouped => {
                let from = self.trades.partition_point(|t| t.ts < first);
                shown.extend(
                    self.trades
                        .iter()
                        .skip(from)
                        .take_while(|t| t.ts <= last)
                        .map(Bubble::of),
                );
            }
            BubbleGrouping::Executions(_) => {
                let from = self.trades.partition_point(|t| t.ts < first);
                let to = self.trades.partition_point(|t| t.ts <= last);
                // Whole groups: from the edge at or before the first trade to the one after the
                // last.
                let mut at = from - ((self.evicted + from) % self.group).min(from);
                let end = to + (self.group - (self.evicted + to) % self.group) % self.group;
                while from < to && at < end.min(self.trades.len()) {
                    let next =
                        (at + self.group - (self.evicted + at) % self.group).min(self.trades.len());
                    shown.push(Bubble::of_group(self.trades.range(at..next)));
                    at = next;
                }
            }
            _ => {
                let lower = match self.config.grouping {
                    BubbleGrouping::TimeWindow(_) => first - TimeDelta::milliseconds(self.window),
                    _ => first,
                };
                let range = (lower, i64::MIN)..=(last, i64::MAX);
                shown.extend(self.bubbles.range(range).map(|(_, bubble)| bubble.clone()));
            }
        }
        shown.into_iter()
    }

    /// Draws the bubbles of `bars`, the visible bars, the `i`th bar's at `x(i)` and `spacing`
    /// apart. Bubbles that do not sit on a bar's centre are placed in its slot by their time.
    pub(crate) fn render(
        &self,
        painter: &Painter,
        bars: &[Bar],
        spacing: f32,
        x: impl Fn(usize) -> f32,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        if bars.is_empty() {
            return;
        }
        let slot = self.clock.timeframe().map_or(0, |t| t.duration_ms());
        // The clock walks a calendar for each call, so each bar's open is asked once.
        let opens: Vec<_> = bars.iter().map(|bar| self.clock.bar_open(bar.time)).collect();
        let open_of = |i: usize| opens[i];
        let slot_of = |i: usize| match bars.get(i + 1) {
            Some(_) if slot > 0 => slot.min((open_of(i + 1) - open_of(i)).num_milliseconds()),
            Some(_) => (open_of(i + 1) - open_of(i)).num_milliseconds(),
            None => slot,
        };
        let end = opens[opens.len() - 1] + TimeDelta::milliseconds(slot_of(bars.len() - 1));
        let min_size = self.config.min_size_filter.unwrap_or(f64::NEG_INFINITY);
        let mut shown: Vec<(f32, f32, Bubble)> = Vec::new();
        for bubble in self.between(opens[0], end) {
            let i = opens.partition_point(|open| *open <= bubble.at);
            let Some(i) = i.checked_sub(1) else { continue };
            let elapsed = (bubble.at - open_of(i)).num_milliseconds();
            let (x, merges) = match self.config.grouping {
                BubbleGrouping::Automatic | BubbleGrouping::PriceStep(_) => {
                    if open_of(i) != bubble.at {
                        continue;
                    }
                    (x(i), true)
                }
                _ if elapsed < slot_of(i) => {
                    let share = elapsed as f32 / slot_of(i) as f32;
                    (x(i) + (share - 0.5) * spacing, false)
                }
                _ if slot_of(i) == 0 && elapsed == 0 => (x(i), false),
                _ => continue,
            };
            // A bar's levels come in price order, so those on one row are adjacent.
            let y = price_to_y(bubble.price).round();
            match shown.last_mut() {
                Some((at, row, merged)) if merges && (*at, *row) == (x, y) => merged.merge(&bubble),
                _ => shown.push((x, y, bubble)),
            }
        }
        shown.retain(|(_, _, bubble)| bubble.volume >= min_size);
        let scale = Scale::new(
            &self.config,
            shown.iter().flat_map(|(_, _, b)| {
                let (discs, n) = self.discs(b, 0.0);
                discs.into_iter().take(n).map(|(volume, _)| volume)
            }),
        );
        let max_delta = shown
            .iter()
            .map(|(_, _, b)| (b.buy - b.sell).abs())
            .fold(0.0, f64::max);
        let border = self.config.show_border.then_some(self.config.border_color);
        // One mesh for every bubble: the tessellator takes far longer over a circle shape per
        // bubble than over copying these vertices, and a fresh mesh of this size every frame
        // costs about as much again in page faults.
        let mut last = self.mesh.lock().unwrap_or_else(|e| e.into_inner());
        let mut mesh = Arc::try_unwrap(std::mem::take(&mut *last)).unwrap_or_default();
        mesh.clear();
        for (x, y, bubble) in &shown {
            let center = Pos2::new(*x, *y);
            let (discs, n) = self.discs(bubble, max_delta);
            for &(volume, color) in &discs[..n] {
                let radius = scale.diameter(volume) / 2.0;
                add_disc(
                    &mut mesh,
                    center,
                    radius,
                    color,
                    border,
                    painter.pixels_per_point(),
                );
            }
        }
        *last = Arc::new(mesh);
        painter.add(Shape::Mesh(Arc::clone(&last)));
    }

    /// The circles of a bubble, the largest first, as the volume that sizes each and its colour,
    /// and how many of the two there are.
    fn discs(&self, bubble: &Bubble, max_delta: f64) -> ([(f64, Color32); 2], usize) {
        let config = &self.config;
        let mut discs = [(bubble.volume, self.color(bubble, max_delta)); 2];
        if config.color_by != BubbleColor::Split || bubble.buy + bubble.sell <= 0.0 {
            return (discs, 1);
        }
        let mut sides = [
            (bubble.sell, bubble.sells, config.sell_color),
            (bubble.buy, bubble.buys, config.buy_color),
        ];
        sides.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut n = 0;
        for (volume, trades, color) in sides {
            if trades > 0 {
                discs[n] = (volume, with_opacity(color, config.opacity));
                n += 1;
            }
        }
        (discs, n)
    }

    fn color(&self, bubble: &Bubble, max_delta: f64) -> Color32 {
        let config = &self.config;
        let sided = bubble.buy + bubble.sell;
        let delta = bubble.buy - bubble.sell;
        let mut opacity = config.opacity;
        let base = match config.color_by {
            BubbleColor::Volume => config.neutral_color,
            BubbleColor::DeltaPercent if bubble.volume > 0.0 => {
                let share = (delta.abs() / bubble.volume).min(1.0) as f32;
                let side = if delta >= 0.0 {
                    config.buy_color
                } else {
                    config.sell_color
                };
                config.neutral_color.lerp_to_gamma(side, share)
            }
            _ if bubble.buys + bubble.sells == 0 || sided <= 0.0 => config.neutral_color,
            BubbleColor::Side if bubble.buy > bubble.sell => config.buy_color,
            BubbleColor::Side if bubble.sell > bubble.buy => config.sell_color,
            BubbleColor::Delta => {
                let buy_share = (bubble.buy / sided) as f32;
                config.sell_color.lerp_to_gamma(config.buy_color, buy_share)
            }
            BubbleColor::AbsoluteDelta if delta != 0.0 => {
                let share = (delta.abs() / max_delta).min(1.0) as f32;
                opacity = (f32::from(opacity) * (0.2 + 0.8 * share)).round() as u8;
                if delta > 0.0 {
                    config.buy_color
                } else {
                    config.sell_color
                }
            }
            _ => config.neutral_color,
        };
        with_opacity(base, opacity)
    }
}

fn with_opacity(color: Color32, opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), opacity)
}

/// Appends a disc of `fill` to `mesh`, ringed outside by a hairline of `border`, with edges
/// faded over one physical pixel as the tessellator fades them. The polygon has the fewest sides
/// that keep it within a quarter of a physical pixel of the circle at `pixels_per_point`.
fn add_disc(
    mesh: &mut Mesh,
    center: Pos2,
    radius: f32,
    fill: Color32,
    border: Option<Color32>,
    pixels_per_point: f32,
) {
    let half = 0.5 / pixels_per_point;
    let inner = (radius - half).max(0.0);
    let width = DESIGN_TOKENS.stroke.hairline;
    // Ring radii and colours from the centre out; a hairline no wider than the fade peaks once.
    let rings: &[(f32, Color32)] = match border {
        None => &[(inner, fill), (radius + half, Color32::TRANSPARENT)],
        Some(border) if width <= 2.0 * half => &[
            (inner, fill),
            (radius + half, border),
            (radius + width + half, Color32::TRANSPARENT),
        ],
        Some(border) => &[
            (inner, fill),
            (radius + half, border),
            (radius + width - half, border),
            (radius + width + half, Color32::TRANSPARENT),
        ],
    };
    let outer = rings[rings.len() - 1].0 * pixels_per_point;
    let max_step = (1.0 - 0.25 / outer).clamp(-1.0, 1.0).acos();
    let sides = ((std::f32::consts::PI / max_step).ceil() as u32).clamp(8, 256);
    let (sin, cos) = (std::f32::consts::TAU / sides as f32).sin_cos();
    let per_side = rings.len() as u32;
    let first = mesh.vertices.len() as u32;
    mesh.reserve_vertices(1 + (sides * per_side) as usize);
    mesh.reserve_triangles((sides * (2 * per_side - 1)) as usize);
    mesh.colored_vertex(center, fill);
    let mut direction = Vec2::X;
    for side in 0..sides {
        for &(r, color) in rings {
            mesh.colored_vertex(center + r * direction, color);
        }
        direction = Vec2::new(
            direction.x * cos - direction.y * sin,
            direction.x * sin + direction.y * cos,
        );
        let a = first + 1 + side * per_side;
        let b = first + 1 + (side + 1) % sides * per_side;
        mesh.add_triangle(first, a, b);
        for ring in 0..per_side - 1 {
            mesh.add_triangle(a + ring, b + ring, a + ring + 1);
            mesh.add_triangle(b + ring, b + ring + 1, a + ring + 1);
        }
    }
}

/// Diameters for the volumes of the bubbles drawn together.
struct Scale {
    low: f64,
    high: f64,
    mode: BubbleScalingMode,
    min_diameter: f32,
    max_diameter: f32,
}

impl Scale {
    /// Spans the diameters over `volumes`, the largest clamped to the configured number of
    /// standard deviations above their mean.
    fn new(config: &VolumeBubblesConfig, volumes: impl Iterator<Item = f64> + Clone) -> Self {
        let count = volumes.clone().count().max(1) as f64;
        let mean = volumes.clone().sum::<f64>() / count;
        let variance = volumes.clone().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
        let cap = mean + config.clamp_std_devs * variance.sqrt();
        let low = volumes.clone().fold(f64::INFINITY, f64::min);
        let high = volumes.fold(f64::NEG_INFINITY, f64::max).min(cap);
        Self {
            low,
            high,
            mode: config.scaling_mode,
            min_diameter: config.min_diameter,
            max_diameter: config.max_diameter,
        }
    }

    fn diameter(&self, volume: f64) -> f32 {
        let scaled = |volume: f64| match self.mode {
            BubbleScalingMode::Linear => volume,
            BubbleScalingMode::Logarithmic => volume.max(0.0).ln_1p(),
            BubbleScalingMode::SquareRoot => volume.max(0.0).sqrt(),
        };
        let (low, high) = (scaled(self.low), scaled(self.high));
        if high <= low {
            return (self.min_diameter + self.max_diameter) / 2.0;
        }
        let share = ((scaled(volume.min(self.high)) - low) / (high - low)).clamp(0.0, 1.0);
        self.min_diameter + share as f32 * (self.max_diameter - self.min_diameter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::trades::Trades;
    use crate::model::Timeframe;
    use BubbleGrouping::{Automatic, Executions, PriceStep, TimeWindow, Ungrouped};

    /// A trade `second` seconds after 10:00.
    fn trade(second: i64, price: f64, size: f64, side: Option<OrderSide>) -> Tick {
        let ts = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        Tick::new(ts + TimeDelta::seconds(second), price, size, side)
    }

    /// Guards the aggregation in data space and its bound: trades of one one-minute bar at one
    /// price level make one bubble wherever they sit on screen, a bubble is stamped with its
    /// bar, the level is the configured step else the tick size, and a trade the store evicts
    /// leaves its bubble, so the bubbles never hold more than the bounded store, nor a rounding
    /// residue on a side whose trades all left.
    #[test]
    fn bubbles_sum_the_held_trades_per_bar_and_price_level() {
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let ten = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let ten_one = ten + TimeDelta::minutes(1);
        #[rustfmt::skip]
        let cases = [
            ("a level and a bar make one bubble", Automatic, Some(0.5), 10,
                vec![trade(1, 100.1, 2.0, buy), trade(50, 99.9, 3.0, sell), trade(59, 100.2, 1.0, None),
                    trade(61, 100.0, 4.0, buy)],
                vec![(ten, 100.0, 2.0, 3.0, 6.0, 3, 1, 1), (ten_one, 100.0, 4.0, 0.0, 4.0, 1, 1, 0)]),
            ("the configured step wins over the tick size", PriceStep(1.0), Some(0.5), 10,
                vec![trade(1, 100.4, 1.0, buy), trade(2, 99.6, 1.0, buy), trade(3, 101.0, 1.0, sell)],
                vec![(ten, 100.0, 2.0, 0.0, 2.0, 2, 2, 0), (ten, 101.0, 0.0, 1.0, 1.0, 1, 0, 1)]),
            ("without a step every price has its own bubble", Automatic, None, 10,
                vec![trade(1, 100.1, 1.0, buy), trade(2, 100.2, 1.0, buy), trade(3, 100.1, 2.0, buy)],
                vec![(ten, 100.1, 3.0, 0.0, 3.0, 2, 2, 0), (ten, 100.2, 1.0, 0.0, 1.0, 1, 1, 0)]),
            ("an evicted trade leaves its bubble", Automatic, Some(1.0), 2,
                vec![trade(1, 100.0, 1.0, buy), trade(61, 100.0, 2.0, sell), trade(62, 101.0, 5.0, buy)],
                vec![(ten_one, 100.0, 0.0, 2.0, 2.0, 1, 0, 1), (ten_one, 101.0, 5.0, 0.0, 5.0, 1, 1, 0)]),
            ("a side whose trades were all evicted holds no volume", Automatic, Some(1.0), 1,
                vec![trade(1, 100.0, 0.1, buy), trade(2, 100.0, 0.2, buy), trade(3, 100.0, 1.0, None)],
                vec![(ten, 100.0, 0.0, 0.0, 1.0, 1, 0, 0)]),
        ];
        for (name, grouping, tick_size, capacity, trades, expected) in cases {
            let config = VolumeBubblesConfig {
                grouping,
                ..VolumeBubblesConfig::default()
            };
            let clock = BarClock::new(Timeframe::Min1);
            let mut bubbles = VolumeBubbles::new(config, clock, tick_size, []);
            let mut store = Trades::new(capacity);
            for trade in trades {
                let mut pending = store.empty();
                pending.push(trade);
                store.append(pending, &mut |trade, added| bubbles.fold(trade, added));
            }
            let got = Vec::from_iter((bubbles.between(ten, ten_one)).map(|b| {
                (
                    b.at, b.price, b.buy, b.sell, b.volume, b.trades, b.buys, b.sells,
                )
            }));
            assert_eq!(got, expected, "{name}");
        }
    }

    /// Guards the shape of a bubble at 1x and 2x: every side stays within a quarter of a physical
    /// pixel of the circle, the fill ends half a pixel inside the radius, and the edge fades to
    /// nothing half a pixel past the radius, or past the hairline outside it with a border. Too
    /// few sides, a rotation that drifts or a ring at the wrong radius fails it.
    #[test]
    fn bubbles_stay_round_and_fade_over_one_pixel() {
        let (fill, border) = (Color32::RED, Color32::WHITE);
        for (radius, ppp, with_border) in [
            (2.0, 1.0, false),
            (2.0, 2.0, true),
            (12.0, 1.0, true),
            (20.0, 1.0, false),
            (20.0, 2.0, true),
        ] {
            let half = 0.5 / ppp;
            let rings = match (with_border, ppp) {
                (false, _) => vec![(radius - half, fill), (radius + half, Color32::TRANSPARENT)],
                (true, 1.0) => vec![
                    (radius - half, fill),
                    (radius + half, border),
                    (radius + 1.0 + half, Color32::TRANSPARENT),
                ],
                (true, _) => vec![
                    (radius - half, fill),
                    (radius + half, border),
                    (radius + 1.0 - half, border),
                    (radius + 1.0 + half, Color32::TRANSPARENT),
                ],
            };
            let center = Pos2::new(100.0, 50.0);
            let mut mesh = Mesh::default();
            add_disc(
                &mut mesh,
                center,
                radius,
                fill,
                with_border.then_some(border),
                ppp,
            );
            let case = format!("radius {radius} at {ppp}x, border {with_border}");
            assert!(mesh.is_valid(), "{case}");
            let sides = (mesh.vertices.len() - 1) / rings.len();
            assert_eq!(
                mesh.indices.len(),
                3 * sides * (2 * rings.len() - 1),
                "{case}"
            );
            for (ring, (r, color)) in rings.iter().enumerate() {
                let at = |side: usize| mesh.vertices[1 + side % sides * rings.len() + ring];
                for side in 0..sides {
                    let (a, b) = (at(side), at(side + 1));
                    assert!(((a.pos - center).length() - r).abs() < 1e-3, "{case}");
                    assert_eq!(a.color, *color, "{case}");
                    let middle = (a.pos + b.pos.to_vec2()) / 2.0;
                    let error = (r - (middle - center).length()) * ppp;
                    assert!((0.0..=0.25).contains(&error), "{case}: {error} px");
                }
            }
        }
    }

    /// Guards the clamp of outliers: the sizes span the bubbles drawn together, and a volume
    /// beyond the mean plus `clamp_std_devs` standard deviations draws at the maximum diameter
    /// without shrinking the others. Expected values from the population formula computed
    /// independently.
    #[test]
    fn a_large_print_does_not_shrink_the_other_bubbles() {
        let volumes = [[10.0; 9].as_slice(), &[20.0, 1000.0]].concat();
        let config = |clamp_std_devs| VolumeBubblesConfig {
            min_diameter: 0.0,
            max_diameter: 100.0,
            scaling_mode: BubbleScalingMode::Linear,
            clamp_std_devs,
            ..VolumeBubblesConfig::default()
        };
        for (clamp, expected) in [
            (f64::INFINITY, [0.0, 1.010_101, 100.0]),
            (1.0, [0.0, 2.664_954, 100.0]),
        ] {
            let scale = Scale::new(&config(clamp), volumes.iter().copied());
            let got = [10.0, 20.0, 1000.0].map(|volume| scale.diameter(volume));
            for (got, expected) in got.into_iter().zip(expected) {
                assert!(
                    (got - expected).abs() < 1e-4,
                    "clamp {clamp}: {got} vs {expected}"
                );
            }
        }
    }

    /// Guards the groupings that do not follow the bar interval: fills of one instant and price
    /// stay separate when ungrouped, a time window and a price level make one bubble, and groups
    /// of executions keep their edges when the store evicts the oldest trade, so a bubble does
    /// not change shape as the tape moves. Expected values worked out by hand: `(seconds after
    /// 10:00, price, buy, sell, volume)`.
    #[test]
    fn groupings_other_than_the_bar_make_the_expected_bubbles() {
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let ten = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let tape = || {
            vec![
                trade(1, 100.0, 2.0, buy),
                trade(2, 102.0, 2.0, sell),
                trade(2, 102.0, 1.0, None),
                trade(12, 100.0, 3.0, sell),
            ]
        };
        #[rustfmt::skip]
        let cases = [
            ("ungrouped keeps fills of one instant and price apart", Ungrouped, 10,
                vec![(1, 100.0, 2.0, 0.0, 2.0), (2, 102.0, 0.0, 2.0, 2.0),
                    (2, 102.0, 0.0, 0.0, 1.0), (12, 100.0, 0.0, 3.0, 3.0)]),
            ("a time window and a level make one bubble, in the middle of the window", TimeWindow(TimeDelta::seconds(10)), 10,
                vec![(5, 100.0, 2.0, 0.0, 2.0), (5, 102.0, 0.0, 2.0, 3.0), (15, 100.0, 0.0, 3.0, 3.0)]),
            ("executions group at the volume weighted price and the last time", Executions(2), 10,
                vec![(2, 101.0, 2.0, 2.0, 4.0), (12, 100.5, 0.0, 3.0, 4.0)]),
            ("an eviction leaves the other groups as they were", Executions(2), 3,
                vec![(2, 102.0, 0.0, 2.0, 2.0), (12, 100.5, 0.0, 3.0, 4.0)]),
        ];
        for (name, grouping, capacity, expected) in cases {
            let config = VolumeBubblesConfig {
                grouping,
                ..VolumeBubblesConfig::default()
            };
            let clock = BarClock::new(Timeframe::Min1);
            let mut bubbles = VolumeBubbles::new(config, clock, Some(1.0), []);
            let mut store = Trades::new(capacity);
            for trade in tape() {
                let mut pending = store.empty();
                pending.push(trade);
                store.append(pending, &mut |trade, added| bubbles.fold(trade, added));
            }
            let got = Vec::from_iter(
                bubbles
                    .between(ten, ten + TimeDelta::minutes(1))
                    .map(|b| ((b.at - ten).num_seconds(), b.price, b.buy, b.sell, b.volume)),
            );
            assert_eq!(got, expected, "{name}");
        }
    }

    /// Guards the colour rules: the delta as a share of the volume with a side differs from the
    /// share of all volume, which unknown-side trades pull toward neutral; the absolute delta
    /// fades against the largest visible one; split shows both sides in circles of their own
    /// volume, the larger first; plain volume ignores the side.
    #[test]
    fn each_colour_mode_colours_the_bubble_as_documented() {
        let config = |color_by| VolumeBubblesConfig {
            color_by,
            opacity: 200,
            buy_color: Color32::from_rgb(0, 255, 0),
            sell_color: Color32::from_rgb(255, 0, 0),
            neutral_color: Color32::from_rgb(100, 100, 100),
            ..VolumeBubblesConfig::default()
        };
        let bubble = |buy: f64, sell: f64, unknown: f64| Bubble {
            buy,
            sell,
            volume: buy + sell + unknown,
            buys: usize::from(buy > 0.0),
            sells: usize::from(sell > 0.0),
            trades: 1,
            ..Bubble::default()
        };
        let with = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
        let (green, red, gray) = (
            Color32::from_rgb(0, 255, 0),
            Color32::from_rgb(255, 0, 0),
            Color32::from_rgb(100, 100, 100),
        );
        #[rustfmt::skip]
        let cases = [
            (BubbleColor::Delta, bubble(2.0, 0.0, 2.0), 4.0, vec![(4.0, with(green, 200))]),
            (BubbleColor::DeltaPercent, bubble(2.0, 0.0, 2.0), 4.0, vec![(4.0, with(Color32::from_rgb(50, 178, 50), 200))]),
            (BubbleColor::DeltaPercent, bubble(0.0, 3.0, 0.0), 4.0, vec![(3.0, with(red, 200))]),
            (BubbleColor::DeltaPercent, bubble(0.0, 0.0, 5.0), 4.0, vec![(5.0, with(gray, 200))]),
            (BubbleColor::AbsoluteDelta, bubble(4.0, 0.0, 1.0), 4.0, vec![(5.0, with(green, 200))]),
            (BubbleColor::AbsoluteDelta, bubble(0.0, 1.0, 0.0), 4.0, vec![(1.0, with(red, 80))]),
            (BubbleColor::AbsoluteDelta, bubble(2.0, 2.0, 0.0), 4.0, vec![(4.0, with(gray, 200))]),
            (BubbleColor::Split, bubble(3.0, 1.0, 9.0), 0.0, vec![(3.0, with(green, 200)), (1.0, with(red, 200))]),
            (BubbleColor::Split, bubble(1.0, 3.0, 0.0), 0.0, vec![(3.0, with(red, 200)), (1.0, with(green, 200))]),
            (BubbleColor::Split, bubble(0.0, 0.0, 2.0), 0.0, vec![(2.0, with(gray, 200))]),
            (BubbleColor::Volume, bubble(4.0, 0.0, 0.0), 0.0, vec![(4.0, with(gray, 200))]),
        ];
        for (mode, bubble, max_delta, expected) in cases {
            let bubbles =
                VolumeBubbles::new(config(mode), BarClock::new(Timeframe::Min1), None, []);
            let (discs, n) = bubbles.discs(&bubble, max_delta);
            assert_eq!(discs[..n], expected[..], "{mode:?} {bubble:?}");
        }
    }

    /// Guards the exact position of ungrouped bubbles: a fill sits in its bar's slot at the
    /// share of the bar interval that had elapsed, and two fills of one instant and price draw
    /// two circles on one spot instead of one, while a price step still merges the levels of a
    /// bar that fall on one pixel row. Stamps inside a minute or daily slot leave every
    /// grouping at the same position, including trades before the first visible stamp.
    #[test]
    fn bubbles_follow_their_bar_slots_and_execution_times() {
        let ten = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let mesh = |grouping, fills: &[(i64, f64)], stamp, timeframe: Timeframe| {
            let clock = BarClock::new(timeframe);
            let open = clock.bar_open(ten);
            let bars = [0, timeframe.duration_ms()].map(|ms| {
                Bar::new(
                    open + TimeDelta::milliseconds(ms) + TimeDelta::seconds(stamp),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    1.0,
                )
            });
            let config = VolumeBubblesConfig {
                grouping,
                ..VolumeBubblesConfig::default()
            };
            let trades = Vec::from_iter(
                fills
                    .iter()
                    .map(|&(s, p)| Tick::new(open + TimeDelta::seconds(s), p, 1.0, None)),
            );
            let bubbles = VolumeBubbles::new(config, clock, Some(1.0), &trades);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                bubbles.render(
                    ui.painter(),
                    &bars,
                    50.0,
                    |i| 100.0 + 50.0 * i as f32,
                    |price| 200.0 - price as f32,
                );
            });
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Mesh(mesh) => Some(Arc::clone(mesh)),
                    _ => None,
                })
                .unwrap_or_default()
        };
        let centres = |fills: &[(i64, f64)]| {
            let mesh = mesh(Ungrouped, fills, 0, Timeframe::Min1);
            let sides = mesh.vertices.len() / fills.len();
            Vec::from_iter(
                mesh.vertices
                    .chunks(sides)
                    .map(|disc| (disc[0].pos.x, disc[0].pos.y)),
            )
        };
        // A quarter into the first bar, half way, and three quarters into the second.
        assert_eq!(
            centres(&[(15, 100.0), (30, 99.0), (75, 99.0), (30, 99.0)]),
            [
                (87.5, 100.0),
                (100.0, 101.0),
                (100.0, 101.0),
                (137.5, 101.0)
            ]
        );
        // 99.0 and 99.25 both round to row 101: one disc, as large as a lone fill's.
        assert_eq!(
            mesh(
                PriceStep(0.25),
                &[(15, 99.0), (20, 99.25)],
                0,
                Timeframe::Min1
            )
            .vertices
            .len(),
            mesh(Ungrouped, &[(15, 99.0)], 0, Timeframe::Min1)
                .vertices
                .len()
        );
        for (timeframe, stamp, second) in [
            (Timeframe::Min1, 30, 60),
            (Timeframe::Day1, 4 * 3600, 86400),
        ] {
            for grouping in [Automatic, PriceStep(1.0)] {
                let fills = [(40, 100.0), (second + 40, 101.0)];
                let opens = mesh(grouping, &fills, 0, timeframe);
                let inside = mesh(grouping, &fills, stamp, timeframe);
                assert!(!opens.vertices.is_empty());
                assert_eq!(
                    inside.vertices, opens.vertices,
                    "{grouping:?} {timeframe:?}"
                );
            }
        }
        // 150 + (10/60 - 1/2) * 50 = 133 1/3; the first slot also retains
        // executions before its stamp: 100 + (15/60 - 1/2) * 50 = 87.5.
        // Ten-second windows sit at seconds 15 and 75, the latter at x = 137.5.
        for (grouping, second_x) in [
            (Ungrouped, 133.33333),
            (Executions(1), 133.33333),
            (TimeWindow(TimeDelta::seconds(10)), 137.5),
        ] {
            let fills = [(15, 100.0), (70, 101.0)];
            let inside = mesh(grouping, &fills, 30, Timeframe::Min1);
            let opens = mesh(grouping, &fills, 0, Timeframe::Min1);
            assert_eq!(inside.vertices, opens.vertices, "{grouping:?}");
            let second = inside.vertices.len() / 2;
            assert!((inside.vertices[0].pos.x - 87.5).abs() < 0.01);
            assert!((inside.vertices[second].pos.x - second_x).abs() < 0.01);
        }
    }
}
