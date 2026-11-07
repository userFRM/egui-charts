//! Retained textures of resting liquidity, one texel per sampled column and price row.

use super::OrderBookView;
use crate::chart::coords::ChartMapping;
use crate::data::{DepthColumn, DepthHistory, OrderSizeTier};
use crate::theme::ThemeExt;
use chrono::{DateTime, Utc};
use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

type Key = (DateTime<Utc>, chrono::TimeDelta);

/// The share of held sizes, in percent, at or below the default upper cut-off, so that a few
/// outsized levels do not fade the rest of the heatmap.
const UPPER_CUTOFF_PERCENT: usize = 96;
/// The relative change of the upper cut-off beyond which the held columns are redrawn.
const REDRAW_CHANGE: f64 = 0.1;

/// The colour scale over a dark background, by position on the scale, each stop lighter than the
/// one before: blue for thin size, then yellow and orange, white at full intensity. Below the
/// first stop a cell fades out into the background.
const SCALE_ON_DARK: [(f32, Color32); 5] = [
    (0.35, Color32::from_rgb(12, 40, 110)),
    (0.65, Color32::from_rgb(24, 100, 190)),
    (0.8, Color32::from_rgb(150, 135, 30)),
    (0.92, Color32::from_rgb(240, 120, 20)),
    (1.0, Color32::WHITE),
];
/// The colour scale over a light background, each stop darker than the one before.
const SCALE_ON_LIGHT: [(f32, Color32); 4] = [
    (0.35, Color32::from_rgb(205, 225, 245)),
    (0.65, Color32::from_rgb(120, 175, 230)),
    (0.85, Color32::from_rgb(40, 105, 190)),
    (1.0, Color32::from_rgb(10, 35, 95)),
];

struct Column {
    source: Arc<DepthColumn>,
    slot: usize,
    rows: BTreeSet<i64>,
}

/// Brightness and resting-size range of the liquidity heatmap. Defaults shade logarithmically
/// against an upper cut-off shared by every held column. Does not change the sampled depth, best
/// bid/ask lines or live book profile. Serializable for host chart settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiquidityHeatmapConfig {
    /// Multiplies cell opacity, including order tier marks, saturating at full opacity.
    /// Defaults to 1. Negative values act as zero; non-finite values use the default.
    pub brightness: f32,
    /// Cells with total resting size at or below this value stay transparent, including marks.
    /// Defaults to 0. Negative or non-finite values act as zero.
    pub lower_cutoff: f64,
    /// Size that reaches full intensity: `ln(1 + size) / ln(1 + full_intensity_size)`, capped at 1.
    /// `None`, the default, uses the size that 96% of the totals held in the history do not
    /// exceed, the same for every column. Non-positive or non-finite sizes also use that shared
    /// cut-off. The cutoff does not shift the logarithmic curve.
    pub full_intensity_size: Option<f64>,
}

impl Default for LiquidityHeatmapConfig {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            lower_cutoff: 0.0,
            full_intensity_size: None,
        }
    }
}

impl LiquidityHeatmapConfig {
    fn normalized(self) -> Self {
        Self {
            brightness: if self.brightness.is_finite() {
                self.brightness.max(0.0)
            } else {
                1.0
            },
            lower_cutoff: if self.lower_cutoff.is_finite() {
                self.lower_cutoff.max(0.0)
            } else {
                0.0
            },
            full_intensity_size: self
                .full_intensity_size
                .filter(|s| s.is_finite() && *s > 0.0),
        }
    }
}

/// A liquidity heatmap backed by bounded texture tiles. Retain it between frames.
///
/// One colour scale encodes the resting size of a cell, the same for bids and asks: logarithmic
/// against an upper cut-off shared by every held column, which [`Self::config`] can fix. Its
/// contrast against the background grows with the size, and sizes near zero fade into the
/// background. A cell whose largest recorded order reaches a size tier of the [`OrderBookView`]
/// takes that tier's colour over its shading. Empty prices and missing time periods stay
/// transparent. Old columns release their atlas slots; new or changed columns upload only their
/// own texels. A change of the default cut-off by more than 10%, and changed backgrounds, tiers or
/// appearance, upload every held column once. No texture side exceeds the backend's limit.
#[derive(Default)]
pub struct LiquidityHeatmap {
    /// Cell brightness and size range, applied on the next render. Unchanged effective settings
    /// upload nothing; changed settings redraw every held column once.
    pub config: LiquidityHeatmapConfig,
    /// The colour behind the cells, which the colour scale contrasts with, or `None`, the default,
    /// for the plot background of the painter's active theme.
    pub background: Option<Color32>,
    drawn_config: Option<LiquidityHeatmapConfig>,
    /// The upper cut-off the held texels were drawn with.
    drawn_cutoff: Option<f64>,
    columns: BTreeMap<Key, Column>,
    tiles: BTreeMap<(usize, i64), TextureHandle>,
    size: usize,
    drawn_background: Option<Color32>,
    /// How many held rows have each total, by bit pattern, ordered by size as totals are positive.
    held: BTreeMap<u64, usize>,
    held_rows: usize,
    /// The size tiers, by bit pattern, and the tier colours the held texels were drawn with.
    marks: Option<([u64; 2], [Color32; 2])>,
}

impl LiquidityHeatmap {
    /// Draws the history using the pane's price mapping and the host's time-to-x conversion.
    /// `step` is the instrument tick size times the history's ticks per row. `view` gives the size
    /// tiers and their colours, as for the book profile, from the painter's active theme when the
    /// view sets none; its filter and profile do not apply, since
    /// the history sampled the orders of its own
    /// [`DepthHistoryConfig::orders`](crate::data::DepthHistoryConfig::orders). `colors` are the
    /// bullish and bearish colours of the 2 point best bid and best ask lines; call with the same
    /// colours as the price series.
    pub fn render(
        &mut self,
        painter: &Painter,
        mapping: &ChartMapping,
        history: &DepthHistory,
        step: f64,
        time_to_x: impl Fn(DateTime<Utc>) -> Option<f32>,
        view: &OrderBookView,
        colors: [Color32; 2],
    ) {
        if !step.is_finite() || step <= 0.0 {
            return;
        }
        let half_tick = step / f64::from(history.config().ticks_per_row) * 0.5;
        let size = painter.ctx().input(|i| i.max_texture_side).clamp(1, 256);
        let theme = painter.ctx().active_theme().chart;
        // Tiers by bit pattern, so tiers that are not numbers do not redraw every frame, and the
        // resolved colours, so a change of theme redraws the held columns.
        let marks = view.tiers.map(|t| {
            (
                [t.notable.to_bits(), t.large.to_bits()],
                [OrderSizeTier::Notable, OrderSizeTier::Large]
                    .map(|tier| view.tier_color(tier, &theme)),
            )
        });
        let config = self.config.normalized();
        let background = self.background.unwrap_or(theme.bg);
        if self.size != size
            || self.drawn_background != Some(background)
            || self.marks != marks
            || self.drawn_config != Some(config)
        {
            *self = Self {
                config: self.config,
                background: self.background,
                drawn_config: Some(config),
                size,
                drawn_background: Some(background),
                marks,
                ..Self::default()
            };
        }
        let keys: BTreeSet<_> = history.columns().map(|c| (c.time, c.span)).collect();
        let mut gone = Vec::new();
        self.columns.retain(|key, column| {
            let keep = keys.contains(key);
            if !keep {
                gone.push(Arc::clone(&column.source));
            }
            keep
        });
        let is_held = |columns: &BTreeMap<Key, Column>, source: &Arc<DepthColumn>| {
            columns
                .get(&(source.time, source.span))
                .is_some_and(|c| Arc::ptr_eq(&c.source, source))
        };
        let mut changed = !gone.is_empty();
        for source in gone {
            self.count_rows(&source, false);
        }
        for source in history.shared() {
            if !is_held(&self.columns, source) {
                changed = true;
                let replaced = self
                    .columns
                    .get(&(source.time, source.span))
                    .map(|c| Arc::clone(&c.source));
                if let Some(replaced) = replaced {
                    self.count_rows(&replaced, false);
                }
                self.count_rows(source, true);
            }
        }
        // Only a change of the held columns can move the cut-off, so idle frames do not walk them.
        let cutoff = changed
            .then(|| config.full_intensity_size.or_else(|| self.upper_cutoff()))
            .flatten();
        let redraw = cutoff.is_some_and(|cutoff| {
            self.drawn_cutoff
                .is_none_or(|drawn| (cutoff - drawn).abs() > REDRAW_CHANGE * drawn)
        });
        if redraw {
            self.drawn_cutoff = cutoff;
        }
        let max = self.drawn_cutoff.unwrap_or(0.0);
        let scale = scale_stops(background);
        let mut used: BTreeSet<_> = self.columns.values().map(|c| c.slot).collect();
        for source in history.shared() {
            let key = (source.time, source.span);
            if !redraw && is_held(&self.columns, source) {
                continue;
            }
            let slot = self.columns.get(&key).map_or_else(
                || {
                    (0..)
                        .find(|s| !used.contains(s))
                        .expect("a free atlas slot")
                },
                |c| c.slot,
            );
            used.insert(slot);
            let mut pixels: BTreeMap<i64, Vec<Color32>> = BTreeMap::new();
            for (row, (total, largest)) in rows(source) {
                let tile = row.div_euclid(size as i64);
                let pixels = pixels
                    .entry(tile)
                    .or_insert_with(|| vec![Color32::TRANSPARENT; size]);
                if total <= config.lower_cutoff {
                    continue;
                }
                let intensity = if max > 0.0 {
                    (total.ln_1p() / max.ln_1p()).min(1.0) as f32
                } else {
                    0.0
                };
                let shaded = scale_color(scale, intensity);
                // A row without a known order is never marked, whatever the tiers.
                let tier = view
                    .tiers
                    .filter(|_| largest > 0.0)
                    .and_then(|tiers| tiers.tier(largest));
                let cell = tier.map_or(shaded, |tier| shaded.blend(view.tier_color(tier, &theme)));
                pixels[row.rem_euclid(size as i64) as usize] =
                    cell.gamma_multiply(config.brightness.min(255.0 / f32::from(cell.a().max(1))));
            }
            let rows = pixels.keys().copied().collect();
            for (row, pixels) in pixels {
                let tile = self.tiles.entry((slot / size, row)).or_insert_with(|| {
                    painter.ctx().load_texture(
                        "liquidity",
                        ColorImage::filled([size, size], Color32::TRANSPARENT),
                        TextureOptions::NEAREST,
                    )
                });
                tile.set_partial(
                    [slot % size, 0],
                    ColorImage::new([1, size], pixels),
                    TextureOptions::NEAREST,
                );
            }
            self.columns.insert(
                key,
                Column {
                    source: Arc::clone(source),
                    slot,
                    rows,
                },
            );
        }
        let alive: BTreeSet<_> = self
            .columns
            .values()
            .flat_map(|c| c.rows.iter().map(|r| (c.slot / size, *r)))
            .collect();
        self.tiles.retain(|key, _| alive.contains(key));
        let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
        // The best bid and ask paths, broken where a time period has no column.
        let mut paths: [Vec<egui::Pos2>; 2] = Default::default();
        let mut path_end = None;
        let finish = |path: &mut Vec<egui::Pos2>, color| {
            if path.len() > 1 {
                painter.add(egui::Shape::line(
                    std::mem::take(path),
                    egui::Stroke::new(2.0, color),
                ));
            }
            path.clear();
        };
        for column in self.columns.values() {
            let Some(end) = column.source.time.checked_add_signed(column.source.span) else {
                continue;
            };
            let (Some(left), Some(right)) = (time_to_x(column.source.time), time_to_x(end)) else {
                continue;
            };
            if right < mapping.rect.left() || left > mapping.rect.right() {
                continue;
            }
            for &row in &column.rows {
                let nonlinear =
                    mapping.price.transform().mode == crate::scales::PriceScaleMode::Logarithmic;
                let mut mesh =
                    egui::Mesh::with_texture(self.tiles[&(column.slot / size, row)].id());
                // A logarithmic price scale needs one mapped edge per price row; a linear
                // scale can draw the whole tile with one quad.
                let stride = if nonlinear { 1 } else { size };
                for offset in (0..size).step_by(stride) {
                    let lo = (row as f64 * size as f64 + offset as f64) * step - half_tick;
                    let hi = lo + stride as f64 * step;
                    if nonlinear && lo <= 0.0 {
                        continue;
                    }
                    let y0 = mapping.price_to_y(lo);
                    let y1 = mapping.price_to_y(hi);
                    if !y0.is_finite() || !y1.is_finite() {
                        continue;
                    }
                    let rect = Rect::from_min_max(pos2(left, y0.min(y1)), pos2(right, y0.max(y1)));
                    if !rect.intersects(mapping.rect) {
                        continue;
                    }
                    let u0 = (column.slot % size) as f32 / size as f32;
                    let u1 = (column.slot % size + 1) as f32 / size as f32;
                    let low = offset as f32 / size as f32;
                    let high = (offset + stride) as f32 / size as f32;
                    let (v0, v1) = if y0 > y1 { (high, low) } else { (low, high) };
                    mesh.add_rect_with_uv(
                        rect,
                        Rect::from_min_max(pos2(u0, v0), pos2(u1, v1)),
                        Color32::WHITE,
                    );
                }
                if !mesh.is_empty() {
                    painter.add(egui::Shape::mesh(mesh));
                }
            }
            let adjacent = path_end == Some(column.source.time);
            path_end = Some(end);
            for (side, levels) in [&column.source.bids, &column.source.asks]
                .into_iter()
                .enumerate()
            {
                let y = levels
                    .first()
                    .map(|&(level, _)| mapping.price_to_y(level as f64 * step - half_tick))
                    .filter(|y| y.is_finite());
                if !adjacent || y.is_none() {
                    finish(&mut paths[side], colors[side]);
                }
                if let Some(y) = y {
                    for point in [pos2(left, y), pos2(right, y)] {
                        if paths[side].last() != Some(&point) {
                            paths[side].push(point);
                        }
                    }
                }
            }
        }
        for (side, path) in paths.iter_mut().enumerate() {
            finish(path, colors[side]);
        }
    }

    /// Adds the rows of a column to the held sizes or removes them. Rows without size do not count.
    fn count_rows(&mut self, source: &DepthColumn, add: bool) {
        for (total, _) in rows(source).into_values() {
            if !(total.is_finite() && total > 0.0) {
                continue;
            }
            let count = self.held.entry(total.to_bits()).or_default();
            if add {
                *count += 1;
                self.held_rows += 1;
            } else {
                *count -= 1;
                self.held_rows -= 1;
                if *count == 0 {
                    self.held.remove(&total.to_bits());
                }
            }
        }
    }

    /// The nearest-rank percentile of the held sizes at [`UPPER_CUTOFF_PERCENT`], walking down from
    /// the largest so that it costs little however many columns are held.
    fn upper_cutoff(&self) -> Option<f64> {
        let mut above = self.held_rows - (self.held_rows * UPPER_CUTOFF_PERCENT).div_ceil(100);
        for (&bits, &count) in self.held.iter().rev() {
            if count > above {
                return Some(f64::from_bits(bits));
            }
            above -= count;
        }
        None
    }
}

/// The total size and the largest known order of each row of a column, 0 when none is known.
fn rows(source: &DepthColumn) -> BTreeMap<i64, (f64, f64)> {
    let mut rows: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
    for (levels, largest) in [
        (&source.bids, &source.largest_bid_orders),
        (&source.asks, &source.largest_ask_orders),
    ] {
        for (at, &(row, volume)) in levels.iter().enumerate() {
            let cell = rows.entry(row).or_default();
            cell.0 += volume;
            cell.1 = cell.1.max(largest.get(at).copied().unwrap_or(0.0));
        }
    }
    rows
}

/// The stops of the colour scale that fits `background`, without those on the near side of it, so
/// that the contrast only grows along the scale whatever the background.
fn scale_stops(background: Color32) -> &'static [(f32, Color32)] {
    let contrast = |color| crate::theme::contrast(color, background);
    let stops: &[_] = if contrast(SCALE_ON_DARK[4].1) >= contrast(SCALE_ON_LIGHT[3].1) {
        &SCALE_ON_DARK
    } else {
        &SCALE_ON_LIGHT
    };
    let mut first = stops.len() - 1;
    while first > 0 && contrast(stops[first - 1].1) < contrast(stops[first].1) {
        first -= 1;
    }
    &stops[first..]
}

/// The colour at `position`, from 0 to 1, along `stops`, the last of which is at 1.
fn scale_color(stops: &[(f32, Color32)], position: f32) -> Color32 {
    let next = stops
        .partition_point(|stop| stop.0 < position)
        .min(stops.len() - 1);
    let (to, end) = stops[next];
    if next == 0 {
        return end.gamma_multiply((position / to).min(1.0));
    }
    let (from, start) = stops[next - 1];
    start.lerp_to_gamma(end, (position - from) / (to - from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::theme::Theme;
    use chrono::TimeDelta;

    fn mapping() -> ChartMapping {
        ChartMapping::new(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
            10.0,
            0,
            10,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(95.0, 105.0),
                0.0,
                100.0,
                false,
            ),
        )
    }

    /// Draws one frame and returns the column texels it uploaded, with what it painted.
    fn draw(
        ctx: &egui::Context,
        heatmap: &mut LiquidityHeatmap,
        history: &DepthHistory,
        step: f64,
        view: &OrderBookView,
    ) -> (Vec<egui::epaint::ImageDelta>, Vec<egui::Shape>) {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mapping = mapping();
        let mut output = ctx.run_ui(
            egui::RawInput {
                max_texture_side: Some(1024),
                ..Default::default()
            },
            |ui| {
                heatmap.render(
                    ui.painter(),
                    &mapping,
                    history,
                    step,
                    |t| Some((t - start).num_seconds() as f32 * 10.0),
                    view,
                    [Color32::GREEN, Color32::RED],
                );
            },
        );
        let uploads = output
            .textures_delta
            .set
            .iter()
            .filter(|(id, _)| heatmap.tiles.values().any(|t| t.id() == **id))
            .flat_map(|(_, deltas)| deltas)
            .filter(|delta| delta.pos.is_some())
            .cloned()
            .collect();
        output.textures_delta.clear();
        let shapes = output.shapes.into_iter().map(|s| s.shape).collect();
        (uploads, shapes)
    }

    /// The position on the scale of `size` against `cutoff`, worked out by hand from the
    /// logarithmic curve.
    fn position(size: f64, cutoff: f64) -> f32 {
        ((1.0 + size).ln() / (1.0 + cutoff).ln()).min(1.0) as f32
    }

    #[test]
    fn history_and_appearance_upload_only_changed_texels() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut heatmap = LiquidityHeatmap::default();
        let stops = scale_stops(Color32::BLACK);
        let scale = |size, cutoff| scale_color(stops, position(size, cutoff));
        let defaults = LiquidityHeatmapConfig::default();
        let shared = LiquidityHeatmapConfig {
            full_intensity_size: Some(999.0),
            ..defaults
        };
        let dimmed = LiquidityHeatmapConfig {
            brightness: 0.5,
            lower_cutoff: 10.0,
            ..shared
        };
        let invalid = LiquidityHeatmapConfig {
            brightness: f32::NAN,
            lower_cutoff: f64::INFINITY,
            full_intensity_size: Some(f64::NAN),
        };
        let half = |c: Color32| c.gamma_multiply(0.5);
        let clear = Color32::TRANSPARENT;
        let by_default = [
            [scale(9.0, 2000.0), scale(99.0, 2000.0)],
            [scale(99.0, 2000.0), scale(99.0, 2000.0)],
            [scale(99.0, 2000.0), scale(105.0, 2000.0)],
            [scale(999.0, 2000.0), scale(2000.0, 2000.0)],
        ];
        // ln(10)/ln(100) = 1/2; ln(100)/ln(1000) = 2/3; ln(10)/ln(1000) = 1/3.
        for (append, config, expected) in [
            // The cut-off of one column is its largest size: a thin level is half way up the scale.
            (
                Some([9.0, 99.0]),
                defaults,
                vec![[scale(9.0, 99.0), scale(99.0, 99.0)]],
            ),
            (None, defaults, vec![]),
            // Equal sizes draw equal colours on both sides.
            (
                Some([99.0, 99.0]),
                defaults,
                vec![[scale(99.0, 99.0), scale(99.0, 99.0)]],
            ),
            // 105 moves the cut-off by 6%: only the new column uploads, against the old cut-off.
            (
                Some([99.0, 105.0]),
                defaults,
                vec![[scale(99.0, 99.0), scale(105.0, 99.0)]],
            ),
            // 2000 moves it by more than 10%: every held column redraws once.
            (Some([999.0, 2000.0]), defaults, by_default.to_vec()),
            (None, defaults, vec![]),
            (
                None,
                shared,
                vec![
                    [scale(9.0, 999.0), scale(99.0, 999.0)],
                    [scale(99.0, 999.0), scale(99.0, 999.0)],
                    [scale(99.0, 999.0), scale(105.0, 999.0)],
                    [scale(999.0, 999.0), scale(2000.0, 999.0)],
                ],
            ),
            (None, shared, vec![]),
            (
                None,
                dimmed,
                vec![
                    [clear, half(scale(99.0, 999.0))],
                    [half(scale(99.0, 999.0)), half(scale(99.0, 999.0))],
                    [half(scale(99.0, 999.0)), half(scale(105.0, 999.0))],
                    [half(scale(999.0, 999.0)), half(scale(2000.0, 999.0))],
                ],
            ),
            (None, dimmed, vec![]),
            (
                None,
                LiquidityHeatmapConfig {
                    lower_cutoff: 99.0,
                    ..dimmed
                },
                vec![
                    [clear, clear],
                    [clear, clear],
                    [clear, half(scale(105.0, 999.0))],
                    [half(scale(999.0, 999.0)), half(scale(2000.0, 999.0))],
                ],
            ),
            (
                None,
                LiquidityHeatmapConfig {
                    brightness: 0.0,
                    ..shared
                },
                vec![[clear, clear]; 4],
            ),
            // Above full opacity the colours stay on the scale.
            (
                None,
                LiquidityHeatmapConfig {
                    brightness: 2.0,
                    lower_cutoff: 10.0,
                    ..shared
                },
                vec![
                    [clear, scale(99.0, 999.0)],
                    [scale(99.0, 999.0), scale(99.0, 999.0)],
                    [scale(99.0, 999.0), scale(105.0, 999.0)],
                    [scale(999.0, 999.0), scale(2000.0, 999.0)],
                ],
            ),
            (None, invalid, by_default.to_vec()),
            (None, invalid, vec![]),
            (None, defaults, vec![]),
            (
                None,
                LiquidityHeatmapConfig {
                    lower_cutoff: -1.0,
                    full_intensity_size: Some(0.0),
                    ..defaults
                },
                vec![],
            ),
            (
                None,
                LiquidityHeatmapConfig {
                    full_intensity_size: Some(f64::INFINITY),
                    ..defaults
                },
                vec![],
            ),
            (
                None,
                LiquidityHeatmapConfig {
                    brightness: -1.0,
                    ..defaults
                },
                vec![[clear, clear]; 4],
            ),
        ] {
            if let Some([bid, ask]) = append {
                let second = history.columns().count() as i64;
                history.push(DepthColumn {
                    time: start + TimeDelta::seconds(second),
                    span: TimeDelta::seconds(1),
                    bids: vec![(100, bid)],
                    asks: vec![(101, ask)],
                    ..Default::default()
                });
            }
            heatmap.config = config;
            let (uploads, _) = draw(&ctx, &mut heatmap, &history, 1.0, &OrderBookView::default());
            let mut uploads: Vec<_> = uploads
                .iter()
                .map(|delta| {
                    assert_eq!(delta.image.size(), [1, 256]);
                    let egui::ImageData::Color(image) = &delta.image;
                    (delta.pos, [image.pixels[100], image.pixels[101]])
                })
                .collect();
            uploads.sort_by_key(|(pos, _)| pos.map(|p| p[0]));
            if append == Some([99.0, 105.0]) {
                assert_eq!(uploads.len(), 1);
                assert_eq!(uploads[0].0, Some([2, 0]));
            }
            let texels: Vec<_> = uploads.iter().map(|(_, texels)| *texels).collect();
            assert_eq!(texels, expected, "{config:?}");
        }
    }

    /// Guards the shared cut-off: a level far above all the others does not fade them, and a thin
    /// column draws fainter than a deep one beside it.
    #[test]
    fn outsized_levels_do_not_fade_the_rest() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        for (second, size) in [(0, 99.0), (1, 99.0), (2, 9.0)] {
            history.push(DepthColumn {
                time: start + TimeDelta::seconds(second),
                span: TimeDelta::seconds(1),
                bids: (0..100).rev().map(|row| (row, size)).collect(),
                asks: (100..200)
                    .map(|row| (row, if row == 199 { 100_000.0 } else { size }))
                    .collect(),
                ..Default::default()
            });
        }
        let (uploads, _) = draw(
            &ctx,
            &mut LiquidityHeatmap::default(),
            &history,
            1.0,
            &OrderBookView::default(),
        );
        let stops = scale_stops(Color32::BLACK);
        let mut columns: Vec<_> = uploads
            .iter()
            .map(|delta| {
                let egui::ImageData::Color(image) = &delta.image;
                (delta.pos, image.pixels[50])
            })
            .collect();
        columns.sort_by_key(|(pos, _)| pos.map(|p| p[0]));
        assert_eq!(
            columns.iter().map(|c| c.1).collect::<Vec<_>>(),
            [
                scale_color(stops, 1.0),
                scale_color(stops, 1.0),
                scale_color(stops, position(9.0, 99.0))
            ]
        );
    }

    /// Guards the cut-off against sizes the history no longer holds: a replaced column and columns
    /// a merged one covers leave the count, so an outsized level stops setting it once it is gone.
    #[test]
    fn the_cutoff_follows_only_the_sizes_still_held() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let column = |second, span, size| DepthColumn {
            time: start + TimeDelta::seconds(second),
            span: TimeDelta::seconds(span),
            bids: vec![(100, size)],
            asks: vec![(101, size)],
            ..Default::default()
        };
        let mut heatmap = LiquidityHeatmap::default();
        let view = OrderBookView::default();
        history.push(column(0, 1, 1000.0));
        history.push(column(1, 1, 10.0));
        draw(&ctx, &mut heatmap, &history, 1.0, &view);
        assert_eq!((heatmap.drawn_cutoff, heatmap.held_rows), (Some(1000.0), 4));
        // The same period again, with new sizes: the old sizes leave the count.
        history.push(column(0, 1, 10.0));
        draw(&ctx, &mut heatmap, &history, 1.0, &view);
        assert_eq!((heatmap.drawn_cutoff, heatmap.held_rows), (Some(10.0), 4));
        // One column covering both periods: both leave the count.
        history.push(column(0, 2, 5.0));
        draw(&ctx, &mut heatmap, &history, 1.0, &view);
        assert_eq!((heatmap.drawn_cutoff, heatmap.held_rows), (Some(5.0), 2));
    }

    /// Guards the scale: over any background each size stands out more than a smaller one, sizes
    /// near zero fade away, and the colours differ from the candle and order tier colours of the
    /// themes.
    #[test]
    fn scale_contrast_grows_with_size_on_any_background() {
        let themes = [Theme::dark(), Theme::light(), Theme::high_contrast()];
        for background in themes.iter().map(|t| t.chart.bg).chain([
            Color32::from_gray(40),
            Color32::from_gray(100),
            Color32::from_gray(128),
            Color32::from_gray(190),
            Color32::from_rgb(30, 30, 120),
            Color32::from_rgb(250, 240, 200),
        ]) {
            let stops = scale_stops(background);
            let contrast = |at: f32| crate::theme::contrast(scale_color(stops, at), background);
            assert!(contrast(0.01) < 1.1, "{background:?}");
            assert!(contrast(1.0) > 3.0, "{background:?}");
            for step in 1..=200 {
                let (lower, upper) = ((step - 1) as f32 / 200.0, step as f32 / 200.0);
                // One 8-bit level of a faint premultiplied cell is about 0.02 of contrast.
                assert!(
                    contrast(upper) + 0.02 >= contrast(lower),
                    "{background:?} {upper}: {} < {}",
                    contrast(upper),
                    contrast(lower)
                );
            }
        }
        for theme in &themes {
            for &(_, color) in scale_stops(theme.chart.bg) {
                for other in [
                    theme.chart.candle_up,
                    theme.chart.candle_down,
                    theme.chart.order_notable,
                    theme.chart.order_large,
                ] {
                    let distance = (0..3)
                        .map(|c| color[c].abs_diff(other[c]) as u32)
                        .max()
                        .unwrap();
                    assert!(distance >= 60, "{color:?} is close to {other:?}");
                }
            }
        }
    }

    /// Guards the hues over the dark theme: past the blues the scale passes through a yellow,
    /// then an orange, and reaches white at full intensity, so the largest sizes stand apart from
    /// those one step below.
    #[test]
    fn dark_scale_runs_through_yellow_and_orange_to_white() {
        let stops = scale_stops(Theme::dark().chart.bg);
        let samples: Vec<_> = (66..=100)
            .map(|percent| scale_color(stops, percent as f32 / 100.0))
            .collect();
        let warm = |c: Color32| c.r() as f32 >= c.b() as f32 + 80.0;
        let yellow = samples
            .iter()
            .position(|&c| warm(c) && c.g() as f32 >= 0.8 * c.r() as f32);
        let orange = samples.iter().position(|&c| {
            warm(c) && (0.4 * c.r() as f32..=0.65 * c.r() as f32).contains(&(c.g() as f32))
        });
        assert!(yellow.is_some(), "{samples:?}");
        assert!(orange > yellow, "{samples:?}");
        assert_eq!(scale_color(stops, 1.0), Color32::WHITE);
    }

    /// Guards the best bid and ask: 2 point paths in the bullish and bearish colours, unbroken
    /// across adjacent columns, whichever row the best price is in, and broken over a missing
    /// period.
    #[test]
    fn best_bid_and_ask_are_two_point_paths() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        for (second, bid, ask) in [(0, 99, 101), (1, 98, 100), (2, 99, 101), (4, 99, 101)] {
            history.push(DepthColumn {
                time: start + TimeDelta::seconds(second),
                span: TimeDelta::seconds(1),
                bids: vec![(bid, 5.0)],
                asks: vec![(ask, 5.0)],
                ..Default::default()
            });
        }
        let (_, shapes) = draw(
            &ctx,
            &mut LiquidityHeatmap::default(),
            &history,
            1.0,
            &OrderBookView::default(),
        );
        let paths: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Path(path) => {
                    let egui::epaint::ColorMode::Solid(color) = path.stroke.color else {
                        panic!("a solid stroke");
                    };
                    Some((path.stroke.width, color, path.points.len()))
                }
                _ => None,
            })
            .collect();
        // Three columns, two risers: 6 points each, then the column after the gap on its own.
        assert_eq!(
            paths,
            [
                (2.0, Color32::GREEN, 6),
                (2.0, Color32::RED, 6),
                (2.0, Color32::GREEN, 2),
                (2.0, Color32::RED, 2),
            ]
        );
    }

    /// Guards the marks: a cell takes the colour of the tier its largest recorded order reaches,
    /// over its shading, while a row whose total alone reaches a tier and every row of aggregate
    /// depth keep their shading, whatever the tiers. The default view takes the colours of the
    /// painter's active theme. New tiers or a new theme redraw the held columns once, and
    /// unchanged ones, even when they are not numbers, upload nothing.
    #[test]
    fn cells_whose_largest_order_reaches_a_tier_take_its_colour() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let column = |second, largest_bid_orders, largest_ask_orders| DepthColumn {
            time: start + TimeDelta::seconds(second),
            span: TimeDelta::seconds(1),
            bids: vec![(198, 115.0), (197, 25.0)],
            asks: vec![(199, 30.0)],
            largest_bid_orders,
            largest_ask_orders,
        };
        // The first column knows its orders and the second is aggregate depth.
        history.push(column(0, vec![50.0, 5.0], vec![30.0]));
        history.push(column(1, vec![], vec![]));
        let tiers = |notable, large| OrderBookView {
            tiers: Some(crate::data::OrderSizeTiers { notable, large }),
            ..Default::default()
        };
        let (dark, light) = (Theme::dark(), Theme::light());
        let [dark_notable, dark_large] = [dark.chart.order_notable, dark.chart.order_large];
        let [light_notable, light_large] = [light.chart.order_notable, light.chart.order_large];
        // Rows 197, 198 and 199 against the shared cut-off, 115, the largest of six totals.
        let shaded = |theme: &Theme| {
            [25.0, 115.0, 30.0]
                .map(|size| scale_color(scale_stops(theme.chart.bg), position(size, 115.0)))
        };
        let dark_shaded = shaded(&dark);
        let mut heatmap = LiquidityHeatmap::default();
        let defaults = LiquidityHeatmapConfig::default();
        for (theme, view, config, expected) in [
            (
                &dark,
                OrderBookView::default(),
                defaults,
                vec![dark_shaded, dark_shaded],
            ),
            // Row 197 stays shaded: its total of 25 reaches a tier and its largest order of 5 none.
            (
                &dark,
                tiers(20.0, 40.0),
                defaults,
                vec![[dark_shaded[0], dark_large, dark_notable], dark_shaded],
            ),
            // The same tiers on another theme.
            (
                &light,
                tiers(20.0, 40.0),
                defaults,
                vec![
                    [shaded(&light)[0], light_large, light_notable],
                    shaded(&light),
                ],
            ),
            (
                &dark,
                tiers(0.0, 0.0),
                defaults,
                vec![[dark_large; 3], dark_shaded],
            ),
            (
                &dark,
                tiers(f64::NAN, f64::NAN),
                defaults,
                vec![dark_shaded, dark_shaded],
            ),
            (&dark, tiers(f64::NAN, f64::NAN), defaults, vec![]),
            (
                &dark,
                tiers(20.0, 40.0),
                LiquidityHeatmapConfig {
                    brightness: 0.5,
                    ..defaults
                },
                vec![
                    [dark_shaded[0], dark_large, dark_notable].map(|c| c.gamma_multiply(0.5)),
                    dark_shaded.map(|c| c.gamma_multiply(0.5)),
                ],
            ),
            (
                &dark,
                tiers(20.0, 40.0),
                LiquidityHeatmapConfig {
                    lower_cutoff: 115.0,
                    ..defaults
                },
                vec![[Color32::TRANSPARENT; 3]; 2],
            ),
        ] {
            crate::theme::apply_to_egui(&ctx, theme);
            heatmap.config = config;
            let (uploads, _) = draw(&ctx, &mut heatmap, &history, 0.5, &view);
            let mut uploads: Vec<_> = uploads
                .iter()
                .map(|delta| {
                    let egui::ImageData::Color(image) = &delta.image;
                    (
                        delta.pos.map(|p| p[0]),
                        [image.pixels[197], image.pixels[198], image.pixels[199]],
                    )
                })
                .collect();
            uploads.sort_by_key(|upload| upload.0);
            let texels: Vec<_> = uploads.iter().map(|upload| upload.1).collect();
            assert_eq!(texels, expected);
        }
    }
}
