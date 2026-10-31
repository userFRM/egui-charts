//! Retained textures of resting liquidity, one texel per sampled column and price row.

use super::OrderBookView;
use crate::chart::coords::ChartMapping;
use crate::data::{DepthColumn, DepthHistory};
use chrono::{DateTime, Utc};
use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

type Key = (DateTime<Utc>, chrono::TimeDelta);

struct Column {
    source: Arc<DepthColumn>,
    slot: usize,
    rows: BTreeSet<i64>,
}

/// A liquidity heatmap backed by bounded texture tiles. Retain it between frames.
///
/// Each column uses logarithmic contrast against its largest resting size. A cell whose largest
/// recorded order reaches a size tier of the [`OrderBookView`] takes that tier's colour over its
/// shading. Empty prices and missing time periods stay transparent. Old columns release their
/// atlas slots; new or changed columns upload only their own texels, and new colours or tiers
/// upload every held column once. No texture side exceeds the backend's limit.
#[derive(Default)]
pub struct LiquidityHeatmap {
    columns: BTreeMap<Key, Column>,
    tiles: BTreeMap<(usize, i64), TextureHandle>,
    size: usize,
    colors: Option<[Color32; 2]>,
    /// The size tiers, by bit pattern, and the tier colours the held texels were drawn with.
    marks: Option<([u64; 2], [Color32; 2])>,
}

impl LiquidityHeatmap {
    /// Draws the history using the pane's price mapping and the host's time-to-x conversion.
    /// `step` is the instrument tick size times the history's ticks per row. `view` gives the size
    /// tiers and their colours, as for the book profile; its filter and profile do not apply, since
    /// the history sampled the orders of its own
    /// [`DepthHistoryConfig::orders`](crate::data::DepthHistoryConfig::orders). Buy and sell
    /// colours follow the painter's host; call with the same colours as the price series.
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
        // Tiers by bit pattern, so tiers that are not numbers do not redraw every frame.
        let marks = view
            .tiers
            .map(|t| ([t.notable.to_bits(), t.large.to_bits()], view.tier_colors));
        if self.size != size || self.colors != Some(colors) || self.marks != marks {
            *self = Self {
                size,
                colors: Some(colors),
                marks,
                ..Self::default()
            };
        }
        let keys: BTreeSet<_> = history.columns().map(|c| (c.time, c.span)).collect();
        self.columns.retain(|key, _| keys.contains(key));
        let mut used: BTreeSet<_> = self.columns.values().map(|c| c.slot).collect();
        for source in history.shared() {
            let key = (source.time, source.span);
            if self
                .columns
                .get(&key)
                .is_some_and(|c| Arc::ptr_eq(&c.source, source))
            {
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
            // Bid size, ask size and the largest known order of each row, 0 when none is known.
            let mut volumes: BTreeMap<i64, [f64; 3]> = BTreeMap::new();
            for (side, (levels, largest)) in [
                (&source.bids, &source.largest_bid_orders),
                (&source.asks, &source.largest_ask_orders),
            ]
            .into_iter()
            .enumerate()
            {
                for (at, &(row, volume)) in levels.iter().enumerate() {
                    let cell = volumes.entry(row).or_default();
                    cell[side] += volume;
                    cell[2] = cell[2].max(largest.get(at).copied().unwrap_or(0.0));
                }
            }
            let max = volumes.values().map(|v| v[0] + v[1]).fold(0.0, f64::max);
            for (row, volume) in volumes {
                let total = volume[0] + volume[1];
                let tile = row.div_euclid(size as i64);
                let pixels = pixels
                    .entry(tile)
                    .or_insert_with(|| vec![Color32::TRANSPARENT; size]);
                let intensity = if max > 0.0 {
                    (total.ln_1p() / max.ln_1p()) as f32
                } else {
                    0.0
                };
                let color = colors[0].lerp_to_gamma(
                    colors[1],
                    if total > 0.0 {
                        (volume[1] / total) as f32
                    } else {
                        0.5
                    },
                );
                let shaded = color.gamma_multiply(intensity * 0.75);
                // A row without a known order is never marked, whatever the tiers.
                let tier = view
                    .tiers
                    .filter(|_| volume[2] > 0.0)
                    .and_then(|tiers| tiers.tier(volume[2]));
                pixels[row.rem_euclid(size as i64) as usize] =
                    tier.map_or(shaded, |tier| shaded.blend(view.tier_color(tier)));
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
            for (levels, color) in [
                (&column.source.bids, colors[0]),
                (&column.source.asks, colors[1]),
            ] {
                if let Some(&(level, _)) = levels.first() {
                    let y = mapping.price_to_y(level as f64 * step - half_tick);
                    if y.is_finite() {
                        painter.line_segment(
                            [pos2(left, y), pos2(right, y)],
                            egui::Stroke::new(1.0, color),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use chrono::TimeDelta;

    #[test]
    fn unchanged_history_uploads_nothing_and_new_columns_upload_only_their_texels() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        let mapping = ChartMapping::new(
            rect,
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
        );
        let mut heatmap = LiquidityHeatmap::default();
        for n in 0..3 {
            if n != 1 {
                history.push(DepthColumn {
                    time: start + TimeDelta::seconds(n),
                    span: TimeDelta::seconds(1),
                    bids: vec![(100, 15.0)],
                    asks: vec![(101, 3.0)],
                    ..Default::default()
                });
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    max_texture_side: Some(1024),
                    ..Default::default()
                },
                |ui| {
                    heatmap.render(
                        ui.painter(),
                        &mapping,
                        &history,
                        1.0,
                        |t| Some((t - start).num_seconds() as f32 * 10.0),
                        &OrderBookView::default(),
                        [Color32::GREEN, Color32::RED],
                    );
                },
            );
            let uploads: Vec<_> = output
                .textures_delta
                .set
                .iter()
                .filter(|(id, _)| heatmap.tiles.values().any(|t| t.id() == **id))
                .collect();
            if n == 1 {
                assert!(uploads.is_empty());
            }
            if n == 2 {
                assert_eq!(uploads.len(), 1);
                assert_eq!(uploads[0].1[0].pos, Some([1, 0]));
                assert_eq!(uploads[0].1[0].image.size(), [1, 256]);
                let egui::ImageData::Color(pixels) = &uploads[0].1[0].image;
                assert_eq!(pixels.pixels[101].a(), 96);
            }
            output.textures_delta.clear();
        }
    }

    /// Guards the marks: a cell takes the colour of the tier its largest recorded order reaches,
    /// over its shading, while a row whose total alone reaches a tier and every row of aggregate
    /// depth keep their shading, whatever the tiers. New tiers redraw the held columns once, and
    /// unchanged ones, even when they are not numbers, upload nothing.
    #[test]
    fn cells_whose_largest_order_reaches_a_tier_take_its_colour() {
        let ctx = egui::Context::default();
        let mut history = DepthHistory::new(Default::default()).unwrap();
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        let mapping = ChartMapping::new(
            rect,
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
        );
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
        let [caution, error] = OrderBookView::default().tier_colors;
        // Rows 197, 198 and 199 against the largest total, 115: colour * 0.75 * ln(1 + total) /
        // ln(1 + 115), so 255 * 0.75 * (3.2581 / 4.7536) = 131, 255 * 0.75 = 191 and
        // 255 * 0.75 * (3.4340 / 4.7536) = 138.
        let shaded = [(0, 131, 0, 131), (0, 191, 0, 191), (138, 0, 0, 138)]
            .map(|(r, g, b, a)| Color32::from_rgba_premultiplied(r, g, b, a));
        let mut heatmap = LiquidityHeatmap::default();
        for (view, expected) in [
            (OrderBookView::default(), vec![shaded, shaded]),
            // Row 197 stays shaded: its total of 25 reaches a tier and its largest order of 5 none.
            (tiers(20.0, 40.0), vec![[shaded[0], error, caution], shaded]),
            (tiers(0.0, 0.0), vec![[error; 3], shaded]),
            (tiers(f64::NAN, f64::NAN), vec![shaded, shaded]),
            (tiers(f64::NAN, f64::NAN), vec![]),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    max_texture_side: Some(1024),
                    ..Default::default()
                },
                |ui| {
                    heatmap.render(
                        ui.painter(),
                        &mapping,
                        &history,
                        0.5,
                        |t| Some((t - start).num_seconds() as f32 * 10.0),
                        &view,
                        [Color32::GREEN, Color32::RED],
                    );
                },
            );
            let uploads: Vec<_> = output
                .textures_delta
                .set
                .iter()
                .filter(|(id, _)| heatmap.tiles.values().any(|t| t.id() == **id))
                .flat_map(|(_, deltas)| deltas)
                .filter(|delta| delta.pos.is_some())
                .map(|delta| {
                    let egui::ImageData::Color(image) = &delta.image;
                    [image.pixels[197], image.pixels[198], image.pixels[199]]
                })
                .collect();
            assert_eq!(uploads, expected);
            output.textures_delta.clear();
        }
    }
}
