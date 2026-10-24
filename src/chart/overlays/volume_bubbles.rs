//! # Volume Bubbles Overlay
//!
//! Draws the trades of the chart's series as bubbles, one per bar and price level: the diameter
//! grows with the volume traded there and the colour shows the aggressor side or the delta.
//! Turn them on with [`Chart::set_volume_bubbles`](crate::widget::Chart::set_volume_bubbles).

use crate::model::{Bar, BarClock, Tick, TradeSide};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::{Color32, Mesh, Painter, Pos2, Shape, Vec2};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Configuration for volume bubbles overlay
#[derive(Debug, Clone)]
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
    /// Height of a price level. `None` takes the instrument's tick size, or gives every traded
    /// price its own bubble when the feed reports none.
    pub price_step: Option<f64>,
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
            price_step: None,
        }
    }
}

/// How to scale bubble sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleScalingMode {
    /// Linear scaling
    Linear,
    /// Logarithmic scaling (better for large range)
    Logarithmic,
    /// Square root scaling
    SquareRoot,
}

/// What the colour of a bubble shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleColor {
    /// The aggressor side with more volume, or the neutral colour on a tie and when no trade
    /// has a side.
    Side,
    /// The delta as a share of the volume with a side: the sell colour when every trade sold,
    /// the buy colour when every trade bought, and the blend between them in between.
    Delta,
}

/// The trades of one bar at one price level.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Bubble {
    /// Open time of the bar.
    pub(crate) bar: DateTime<Utc>,
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
}

/// Counts a trade in (`added`) or out, `count` being the trades whose volume `sum` holds.
fn count(count: &mut usize, sum: &mut f64, size: f64, added: bool) {
    *count = match added {
        true => *count + 1,
        false => count.saturating_sub(1),
    };
    *sum = if *count == 0 { 0.0 } else { *sum + size };
}

/// The trades the chart holds, summed per bar and price level, and how to draw them.
pub(crate) struct VolumeBubbles {
    pub(crate) config: VolumeBubblesConfig,
    clock: BarClock,
    step: Option<f64>,
    /// By bar open time, then price level, so the bubbles of a run of bars are one range.
    bubbles: BTreeMap<(DateTime<Utc>, i64), Bubble>,
    /// The mesh of the last frame, refilled in place once the tessellator has let go of it.
    mesh: Mutex<Arc<Mesh>>,
}

impl VolumeBubbles {
    /// Sums `trades` per bar of `clock` and per price level, a level being the configured step
    /// high, else `tick_size`.
    pub(crate) fn new<'a>(
        config: VolumeBubblesConfig,
        clock: BarClock,
        tick_size: Option<f64>,
        trades: impl IntoIterator<Item = &'a Tick>,
    ) -> Self {
        let step = (config.price_step.or(tick_size)).filter(|step| step.is_finite() && *step > 0.0);
        let mut bubbles = Self {
            config,
            clock,
            step,
            bubbles: BTreeMap::new(),
            mesh: Mutex::default(),
        };
        for trade in trades {
            bubbles.fold(trade, true);
        }
        bubbles
    }

    /// Adds a trade the chart took (`added`), or takes out one it dropped.
    pub(crate) fn fold(&mut self, trade: &Tick, added: bool) {
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
        let bar = self.clock.bar_open(trade.ts);
        let key = (bar, level);
        let bubble = self.bubbles.entry(key).or_insert_with(|| Bubble {
            bar,
            price,
            ..Bubble::default()
        });
        let size = if added { trade.size } else { -trade.size };
        count(&mut bubble.trades, &mut bubble.volume, size, added);
        match trade.side {
            Some(TradeSide::Buy) => count(&mut bubble.buys, &mut bubble.buy, size, added),
            Some(TradeSide::Sell) => count(&mut bubble.sells, &mut bubble.sell, size, added),
            Some(TradeSide::Unknown) | None => {}
        }
        if bubble.trades == 0 {
            self.bubbles.remove(&key);
        }
    }

    /// The bubbles of the bars that open from `first` to `last`, by bar then price level.
    pub(crate) fn between(
        &self,
        first: DateTime<Utc>,
        last: DateTime<Utc>,
    ) -> impl Iterator<Item = &Bubble> {
        let range = (first, i64::MIN)..=(last.max(first), i64::MAX);
        self.bubbles.range(range).map(|(_, bubble)| bubble)
    }

    /// Draws the bubbles of `bars`, the visible bars, the `i`th bar's at `x(i)`. The levels of a
    /// bar that fall on one pixel row draw as one bubble, so the shapes are bounded by the
    /// visible bars times the pane height.
    pub(crate) fn render(
        &self,
        painter: &Painter,
        bars: &[Bar],
        x: impl Fn(usize) -> f32,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
            return;
        };
        let min_size = self.config.min_size_filter.unwrap_or(f64::NEG_INFINITY);
        let mut shown: Vec<(usize, f32, Bubble)> = Vec::new();
        for bubble in self.between(first.time, last.time) {
            let i = bars.partition_point(|bar| bar.time < bubble.bar);
            if bars.get(i).is_none_or(|bar| bar.time != bubble.bar) {
                continue;
            }
            // A bar's levels come in price order, so those on one row are adjacent.
            let y = price_to_y(bubble.price).round();
            match shown.last_mut() {
                Some((at, row, merged)) if (*at, *row) == (i, y) => merged.merge(bubble),
                _ => shown.push((i, y, bubble.clone())),
            }
        }
        shown.retain(|(_, _, bubble)| bubble.volume >= min_size);
        let scale = Scale::new(&self.config, shown.iter().map(|(_, _, b)| b.volume));
        let border = self.config.show_border.then_some(self.config.border_color);
        // One mesh for every bubble: the tessellator takes far longer over a circle shape per
        // bubble than over copying these vertices, and a fresh mesh of this size every frame
        // costs about as much again in page faults.
        let mut last = self.mesh.lock().unwrap_or_else(|e| e.into_inner());
        let mut mesh = Arc::try_unwrap(std::mem::take(&mut *last)).unwrap_or_default();
        mesh.clear();
        for (i, y, bubble) in &shown {
            let center = Pos2::new(x(*i), *y);
            let radius = scale.diameter(bubble.volume) / 2.0;
            let color = self.color(bubble);
            add_disc(
                &mut mesh,
                center,
                radius,
                color,
                border,
                painter.pixels_per_point(),
            );
        }
        *last = Arc::new(mesh);
        painter.add(Shape::Mesh(Arc::clone(&last)));
    }

    fn color(&self, bubble: &Bubble) -> Color32 {
        let config = &self.config;
        let sided = bubble.buy + bubble.sell;
        let base = match config.color_by {
            _ if bubble.buys + bubble.sells == 0 || sided <= 0.0 => config.neutral_color,
            BubbleColor::Side if bubble.buy > bubble.sell => config.buy_color,
            BubbleColor::Side if bubble.sell > bubble.buy => config.sell_color,
            BubbleColor::Side => config.neutral_color,
            BubbleColor::Delta => {
                let buy_share = (bubble.buy / sided) as f32;
                config.sell_color.lerp_to_gamma(config.buy_color, buy_share)
            }
        };
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), config.opacity)
    }
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
    use chrono::TimeDelta;

    /// A trade `second` seconds after 10:00.
    fn trade(second: i64, price: f64, size: f64, side: Option<TradeSide>) -> Tick {
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
        let (buy, sell) = (Some(TradeSide::Buy), Some(TradeSide::Sell));
        let ten = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let ten_one = ten + TimeDelta::minutes(1);
        #[rustfmt::skip]
        let cases = [
            ("a level and a bar make one bubble", None, Some(0.5), 10,
                vec![trade(1, 100.1, 2.0, buy), trade(50, 99.9, 3.0, sell), trade(59, 100.2, 1.0, None),
                    trade(61, 100.0, 4.0, buy)],
                vec![(ten, 100.0, 2.0, 3.0, 6.0, 3, 1, 1), (ten_one, 100.0, 4.0, 0.0, 4.0, 1, 1, 0)]),
            ("the configured step wins over the tick size", Some(1.0), Some(0.5), 10,
                vec![trade(1, 100.4, 1.0, buy), trade(2, 99.6, 1.0, buy), trade(3, 101.0, 1.0, sell)],
                vec![(ten, 100.0, 2.0, 0.0, 2.0, 2, 2, 0), (ten, 101.0, 0.0, 1.0, 1.0, 1, 0, 1)]),
            ("without a step every price has its own bubble", None, None, 10,
                vec![trade(1, 100.1, 1.0, buy), trade(2, 100.2, 1.0, buy), trade(3, 100.1, 2.0, buy)],
                vec![(ten, 100.1, 3.0, 0.0, 3.0, 2, 2, 0), (ten, 100.2, 1.0, 0.0, 1.0, 1, 1, 0)]),
            ("an evicted trade leaves its bubble", None, Some(1.0), 2,
                vec![trade(1, 100.0, 1.0, buy), trade(61, 100.0, 2.0, sell), trade(62, 101.0, 5.0, buy)],
                vec![(ten_one, 100.0, 0.0, 2.0, 2.0, 1, 0, 1), (ten_one, 101.0, 5.0, 0.0, 5.0, 1, 1, 0)]),
            ("a side whose trades were all evicted holds no volume", None, Some(1.0), 1,
                vec![trade(1, 100.0, 0.1, buy), trade(2, 100.0, 0.2, buy), trade(3, 100.0, 1.0, None)],
                vec![(ten, 100.0, 0.0, 0.0, 1.0, 1, 0, 0)]),
        ];
        for (name, price_step, tick_size, capacity, trades, expected) in cases {
            let config = VolumeBubblesConfig {
                price_step,
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
                    b.bar, b.price, b.buy, b.sell, b.volume, b.trades, b.buys, b.sells,
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
}
