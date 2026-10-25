//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use super::draw_dashed_hline;
use crate::drawings::domain::{Drawing, ProfileDisplay};
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::{Bar, SpanCache};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke, epaint::StrokeKind};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Bars between the saved running sums of an anchored VWAP.
const VWAP_CHECKPOINT: usize = 1024;

thread_local! {
    static VWAP_CHECKPOINTS: RefCell<SpanCache<Vec<VwapSums>>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles by their number of rows.
    static PROFILES: RefCell<SpanCache<Option<VolumeProfile>, (usize, Option<f64>)>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    /// Renders the anchored VWAP from the anchor bar to the last bar, with
    /// bands one and two volume-weighted standard deviations away.
    pub(crate) fn render_anchored_vwap(&self, painter: &egui::Painter, view: &ChartView) {
        let (Some(&anchor), Some(anchor_at)) = (self.points.first(), self.chart_points.first())
        else {
            return;
        };
        let color = self.color32();

        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            self.draw_vwap(painter, view, first, bars, color);
        }

        painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );
    }

    fn draw_vwap(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
        color: Color32,
    ) {
        // VWAP, then the bands, in standard deviations from it.
        const OFFSETS: [f64; 5] = [0.0, 1.0, -1.0, 2.0, -2.0];
        const NAMES: [&str; 4] = ["+1σ", "-1σ", "+2σ", "-2σ"];

        // Only bars inside the clip rect (plus one on each side) become
        // points; bar_to_x is linear, so they are found without a scan.
        let x0 = (view.bar_to_x)(first as f32);
        let step = (view.bar_to_x)(first as f32 + 1.0) - x0;
        if step.is_nan() || step <= 0.0 {
            return;
        }
        let clip = painter.clip_rect();
        let from = (((clip.left() - x0) / step).floor() - 1.0).max(0.0) as usize;
        let to = ((((clip.right() - x0) / step).ceil() + 2.0).max(0.0) as usize).min(bars.len());
        if from >= to {
            return;
        }

        // The checkpoints only cover whole blocks before the last bar, so the
        // forming bar's updates reuse them.
        let covered = (bars.len() - 1) / VWAP_CHECKPOINT * VWAP_CHECKPOINT;
        let checkpoints = VWAP_CHECKPOINTS.with_borrow_mut(|cache| {
            cache.get((), first, &bars[..covered], view.revision, vwap_checkpoints)
        });
        let mut lines: [Vec<Pos2>; 5] = Default::default();
        let mut last_vwap = None;
        for (i, value) in (from..to).zip(anchored_vwap(bars, &checkpoints, from..to)) {
            let Some((vwap, sigma)) = value else { continue };
            let x = (view.bar_to_x)((first + i) as f32);
            for (line, k) in lines.iter_mut().zip(OFFSETS) {
                line.push(Pos2::new(x, (view.price_to_y)(vwap + k * sigma)));
            }
            last_vwap = Some(vwap);
        }
        let (Some(vwap), Some(&end)) = (last_vwap, lines[0].last()) else {
            return;
        };

        // Bands (the 2σ ones fainter), then the VWAP line, each labelled at
        // the last drawn bar
        let font = egui::FontId::proportional(typography::XS);
        let band_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
        let [vwap_line, bands @ ..] = lines;
        for (i, (band, name)) in bands.into_iter().zip(NAMES).enumerate() {
            let label_at = band.last().map(|p| Pos2::new(p.x - 20.0, p.y));
            let alpha = if i < 2 { 1.0 } else { 0.5 };
            painter.add(Shape::line(
                band,
                Stroke::new(stroke::HAIRLINE, band_color.gamma_multiply(alpha)),
            ));
            if let Some(pos) = label_at {
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    name,
                    font.clone(),
                    band_color,
                );
            }
        }
        painter.add(Shape::line(
            vwap_line,
            Stroke::new(self.stroke_width.max(2.0), color),
        ));

        let vwap_bg =
            Rect::from_center_size(Pos2::new(end.x - 60.0, end.y), egui::vec2(80.0, 16.0));
        painter.rect_filled(vwap_bg, DESIGN_TOKENS.rounding.sm, color);
        painter.text(
            vwap_bg.center(),
            egui::Align2::CENTER_CENTER,
            format!("VWAP {vwap:.2}"),
            font,
            Color32::WHITE,
        );
    }

    /// Renders the volume profile of the bars between the two anchors, with
    /// the point of control and the value area.
    pub(crate) fn render_fixed_range_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) {
        let (&[p1, p2, ..], &[a, b, ..]) = (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };
        let color = self.color32();
        if let Some((first, bars)) = bars_between(view.bars, a.bar_idx, b.bar_idx) {
            let (left, right) = (p1.x.min(p2.x), p1.x.max(p2.x));
            self.draw_volume_profile(painter, view, first, bars, left, right);
        }
        self.draw_volume_profile_anchors(painter, &[p1, p2], color);
    }

    /// Renders the volume profile from the anchor bar to the last bar.
    pub(crate) fn render_anchored_volume_profile(&self, painter: &egui::Painter, view: &ChartView) {
        let (Some(&anchor), Some(anchor_at)) = (self.points.first(), self.chart_points.first())
        else {
            return;
        };
        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            let right = (view.bar_to_x)((first + bars.len() - 1) as f32);
            self.draw_volume_profile(painter, view, first, bars, anchor.x, right);
        }
        self.draw_volume_profile_anchors(painter, &[anchor], self.color32());
    }

    /// Draws the profile of `bars`, the span that starts at bar `first`, as
    /// rows growing right from `left`, boxed from `left` to `right` across the
    /// bars' price range.
    fn draw_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
        left: f32,
        right: f32,
    ) {
        let config = self.volume_profile.unwrap_or_default();
        let profile = span_profile(view, config.rows(), config.row_size(), first, bars);
        let Some(profile) = profile.as_ref() else {
            return;
        };
        let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(config.value_area()) else {
            return;
        };
        let y_at_row =
            |row: usize| (view.price_to_y)(profile.low + row as f64 * profile.row_height);
        let max_width = (right - left) * 0.9;
        let max_volume = profile.volume[poc];

        for (row, &volume) in profile.volume.iter().enumerate() {
            // Ordered on screen: the upper row edge is lower down on an inverted scale.
            let (upper, lower) = (y_at_row(row + 1), y_at_row(row));
            let (top, bottom) = (upper.min(lower), upper.max(lower));
            let width = (volume / max_volume) as f32 * max_width;

            // Color coding: POC is bright, Value Area is medium, outside is dim
            let bar_color = if row == poc {
                DESIGN_TOKENS.semantic.extended.favorite_gold
            } else if (va_low..=va_high).contains(&row) {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180)
            } else {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80)
            };

            let paint = |start, width, color| {
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(start, top),
                        Pos2::new(start + width, (bottom - 1.0).max(top)),
                    ),
                    0.0,
                    color,
                );
            };
            let rgba = |c: [u8; 4]| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
            match config.display {
                ProfileDisplay::Total => paint(left, width, bar_color),
                ProfileDisplay::UpDown => {
                    let up_width = (profile.up[row] / max_volume) as f32 * max_width;
                    paint(left, up_width, rgba(config.up_color));
                    paint(
                        left + up_width,
                        (width - up_width).max(0.0),
                        rgba(config.down_color),
                    );
                }
                ProfileDisplay::Delta => {
                    let delta = profile.up[row] - profile.down[row];
                    let color = if delta >= 0.0 {
                        config.up_color
                    } else {
                        config.down_color
                    };
                    paint(
                        left,
                        (delta.abs() / max_volume) as f32 * max_width,
                        rgba(color),
                    );
                }
            }
        }

        let rect = Rect::from_two_pos(
            Pos2::new(left, y_at_row(profile.volume.len())),
            Pos2::new(right, y_at_row(0)),
        );
        let poc_y = (y_at_row(poc) + y_at_row(poc + 1)) / 2.0;
        self.draw_volume_profile_lines(
            painter,
            &rect,
            poc_y,
            y_at_row(va_high + 1),
            y_at_row(va_low),
        );
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, self.color32()),
            StrokeKind::Outside,
        );
    }

    fn draw_volume_profile_lines(
        &self,
        painter: &egui::Painter,
        rect: &Rect,
        poc_y: f32,
        vah_y: f32,
        val_y: f32,
    ) {
        let font = egui::FontId::proportional(typography::XS);

        // Draw POC line (dashed, extends across range)
        let poc_color = DESIGN_TOKENS.semantic.extended.favorite_gold;
        draw_dashed_hline(
            painter,
            poc_y,
            rect.min.x,
            rect.max.x,
            Stroke::new(stroke::HAIRLINE, poc_color),
            8.0,
            4.0,
        );

        // POC label
        let poc_bg =
            Rect::from_center_size(Pos2::new(rect.max.x - 25.0, poc_y), egui::vec2(40.0, 14.0));
        painter.rect_filled(poc_bg, DESIGN_TOKENS.rounding.sm, poc_color);
        painter.text(
            poc_bg.center(),
            egui::Align2::CENTER_CENTER,
            "POC",
            font.clone(),
            Color32::BLACK,
        );

        // Draw VAH line
        let va_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);
        painter.hline(
            rect.min.x..=rect.max.x,
            vah_y,
            Stroke::new(stroke::HAIRLINE, va_color),
        );

        // Each label sits outside the value area, away from the other line;
        // the VAH line is the lower one on an inverted scale.
        let away = if vah_y <= val_y { 8.0 } else { -8.0 };

        // VAH label
        painter.text(
            Pos2::new(rect.max.x - 25.0, vah_y - away),
            egui::Align2::CENTER_CENTER,
            "VAH",
            font.clone(),
            va_color,
        );

        // Draw VAL line
        painter.hline(
            rect.min.x..=rect.max.x,
            val_y,
            Stroke::new(stroke::HAIRLINE, va_color),
        );

        // VAL label
        painter.text(
            Pos2::new(rect.max.x - 25.0, val_y + away),
            egui::Align2::CENTER_CENTER,
            "VAL",
            font,
            va_color,
        );
    }

    fn draw_volume_profile_anchors(
        &self,
        painter: &egui::Painter,
        anchors: &[Pos2],
        color: Color32,
    ) {
        for &anchor in anchors {
            painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                anchor,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );
        }
    }
}

/// Running sums of volume, typical price times volume, and squared typical
/// price times volume.
#[derive(Clone, Copy, Default)]
struct VwapSums {
    volume: f64,
    price_volume: f64,
    square_volume: f64,
}

impl VwapSums {
    fn add(&mut self, bar: &Bar) {
        let price = bar.typical_price();
        self.volume += bar.volume;
        self.price_volume += price * bar.volume;
        self.square_volume += price * price * bar.volume;
    }

    /// VWAP and the volume-weighted standard deviation around it; `None`
    /// until volume trades.
    fn value(&self) -> Option<(f64, f64)> {
        (self.volume > 0.0).then(|| {
            let vwap = self.price_volume / self.volume;
            let variance = self.square_volume / self.volume - vwap * vwap;
            (vwap, variance.max(0.0).sqrt())
        })
    }
}

/// The running sums at the start of the span and after every block of
/// `VWAP_CHECKPOINT` bars of `prefix`, the span's leading whole blocks, so the
/// VWAP at any bar takes fewer than `VWAP_CHECKPOINT` further additions.
fn vwap_checkpoints(prefix: &[Bar]) -> Vec<VwapSums> {
    let mut sums = VwapSums::default();
    let mut checkpoints = vec![sums];
    for block in prefix.chunks(VWAP_CHECKPOINT) {
        block.iter().for_each(|bar| sums.add(bar));
        checkpoints.push(sums);
    }
    checkpoints
}

/// VWAP from the first bar of the span, with its standard deviation, at each
/// bar of the non-empty `range`, resuming from the checkpoint before it.
fn anchored_vwap<'a>(
    bars: &'a [Bar],
    checkpoints: &[VwapSums],
    range: Range<usize>,
) -> impl Iterator<Item = Option<(f64, f64)>> + 'a {
    let checkpoint = range.start / VWAP_CHECKPOINT;
    let mut sums = checkpoints[checkpoint];
    bars[checkpoint * VWAP_CHECKPOINT..range.start]
        .iter()
        .for_each(|bar| sums.add(bar));
    bars[range].iter().map(move |bar| {
        sums.add(bar);
        sums.value()
    })
}

/// Volume traded at each price row of a span of bars.
#[derive(Clone, Debug, PartialEq)]
struct VolumeProfile {
    /// Price at the bottom of row 0.
    low: f64,
    /// Highest price of the bars.
    high: f64,
    row_height: f64,
    fixed_height: bool,
    /// Volume per row, lowest prices first.
    volume: Vec<f64>,
    up: Vec<f64>,
    down: Vec<f64>,
}

/// Profile of `bars`, the span that starts at bar `first`, with `rows` rows.
/// While the last bar, which may be forming, trades inside the range of the
/// bars before it, their profile is reused, so a tick or a new bar costs one
/// bar instead of the span.
fn span_profile(
    view: &ChartView,
    rows: usize,
    row_size: Option<f64>,
    first: usize,
    bars: &[Bar],
) -> Rc<Option<VolumeProfile>> {
    PROFILES.with_borrow_mut(|cache| {
        view.cached(
            cache,
            (rows, row_size),
            first,
            bars,
            |bars| VolumeProfile::of(bars, rows, row_size),
            |profile, bar| profile.as_ref()?.with(bar).map(Some),
        )
    })
}

impl VolumeProfile {
    /// Spreads each bar's volume evenly over its high-low range across
    /// `rows` equal price rows (a bar without range puts it in the row of its
    /// price). `None` when the bars span no price range.
    fn of(bars: &[Bar], rows: usize, row_size: Option<f64>) -> Option<Self> {
        let low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
        let high = bars
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let (low, high, row_height, rows) = if let Some(height) = row_size {
            let (low, high, rows) = fixed_grid(low, high, height)?;
            (low, high, height, rows)
        } else {
            (low, high, (high - low) / rows as f64, rows)
        };
        if !row_height.is_finite() || row_height <= 0.0 {
            return None;
        }
        let mut profile = Self {
            low,
            high,
            row_height,
            fixed_height: row_size.is_some(),
            volume: vec![0.0; rows],
            up: vec![0.0; rows],
            down: vec![0.0; rows],
        };
        bars.iter().for_each(|bar| profile.add(bar));
        Some(profile)
    }

    /// This profile with `bar` added; `None` when the bar trades outside the
    /// profile's price range, which would move the rows.
    fn with(&self, bar: &Bar) -> Option<Self> {
        let mut profile = self.clone();
        if self.fixed_height {
            let (low, high, _) = fixed_grid(bar.low, bar.high, self.row_height)?;
            let low = low.min(self.low);
            let high = high.max(self.high);
            let count = ((high - low) / self.row_height).round();
            if count > 10_000.0 {
                return None;
            }
            let prepend = ((self.low - low) / self.row_height).round() as usize;
            for rows in [&mut profile.volume, &mut profile.up, &mut profile.down] {
                let old_len = rows.len();
                rows.resize(count as usize, 0.0);
                if prepend > 0 {
                    rows.copy_within(..old_len, prepend);
                    rows[..prepend].fill(0.0);
                }
            }
            profile.low = low;
            profile.high = high;
        } else if bar.low < self.low || bar.high > self.high {
            return None;
        }
        profile.add(bar);
        Some(profile)
    }

    /// Adds the volume of `bar` to the rows that its high-low range covers,
    /// in proportion to the overlap.
    fn add(&mut self, bar: &Bar) {
        let (low, row_height) = (self.low, self.row_height);
        let direction = if bar.close >= bar.open {
            &mut self.up
        } else {
            &mut self.down
        };
        let top_row = self.volume.len() - 1;
        let row_of = |price: f64| (((price - low) / row_height) as usize).min(top_row);
        if bar.high > bar.low {
            let (first, last) = (row_of(bar.low), row_of(bar.high));
            for (row, row_volume) in self
                .volume
                .iter_mut()
                .enumerate()
                .take(last + 1)
                .skip(first)
            {
                let row_low = low + row as f64 * row_height;
                let overlap = bar.high.min(row_low + row_height) - bar.low.max(row_low);
                let amount = bar.volume * overlap.max(0.0) / (bar.high - bar.low);
                *row_volume += amount;
                direction[row] += amount;
            }
        } else {
            self.volume[row_of(bar.low)] += bar.volume;
            direction[row_of(bar.low)] += bar.volume;
        }
    }

    /// The row with the most volume (point of control), and the lowest and
    /// highest row of the value area, which holds `share` of the volume;
    /// `None` when no volume traded.
    fn poc_and_value_area(&self, share: f64) -> Option<(usize, (usize, usize))> {
        let volume = &self.volume;
        let total: f64 = volume.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let poc = (0..volume.len()).max_by(|&a, &b| volume[a].total_cmp(&volume[b]))?;
        Some((poc, value_area(volume, poc, total * share)))
    }
}

/// A zero-aligned grid, including the row of a zero-range bar on an edge.
fn fixed_grid(low: f64, high: f64, height: f64) -> Option<(f64, f64, usize)> {
    if !height.is_finite() || height <= 0.0 {
        return None;
    }
    let first = (low / height).floor();
    let last = (high / height).floor();
    let count = last - first + 1.0;
    if !first.is_finite()
        || !last.is_finite()
        || first.abs() >= 2_f64.powi(52)
        || last.abs() >= 2_f64.powi(52)
        || !(1.0..=10_000.0).contains(&count)
    {
        return None;
    }
    let bounds = (first * height, (last + 1.0) * height);
    (bounds.0.is_finite() && bounds.1.is_finite() && bounds.1 > bounds.0).then_some((
        bounds.0,
        bounds.1,
        count as usize,
    ))
}

/// Rows around the POC that hold `target` volume. Market Profile rule: grow
/// toward whichever of the next two rows above or the next two rows below
/// holds more volume, one row at a time.
fn value_area(volume: &[f64], poc: usize, target: f64) -> (usize, usize) {
    let (mut low, mut high, mut sum) = (poc, poc, volume[poc]);
    while sum < target && (low > 0 || high + 1 < volume.len()) {
        let above: f64 = volume[high + 1..].iter().take(2).sum();
        let below: f64 = volume[..low].iter().rev().take(2).sum();
        let up = high + 1 < volume.len() && (low == 0 || above >= below);
        for _ in 0..2 {
            if up && high + 1 < volume.len() {
                high += 1;
                sum += volume[high];
            } else if !up && low > 0 {
                low -= 1;
                sum += volume[low];
            }
            if sum >= target {
                break;
            }
        }
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;
    use crate::drawings::rendering::sample_bars;

    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    /// Reference: VWAP of (h + l + c) / 3 from bar 1, and the standard
    /// deviation from its definition, sqrt(sum v (tp - vwap)^2 / sum v).
    #[test]
    fn anchored_vwap_matches_reference() {
        let want = [
            (11.666666666666664, 0.0),
            (11.595238095238095, 0.08247860988423196),
            (12.166666666666666, 0.6201736729460428),
            (12.675925925925924, 0.9757431833967023),
            (12.775641025641026, 0.8255305309085764),
            (12.78395061728395, 0.8112060000493586),
        ];
        let bars = &sample_bars()[1..];
        let got: Vec<_> = anchored_vwap(bars, &vwap_checkpoints(&[]), 0..bars.len()).collect();
        assert_eq!(got.len(), want.len());
        for (got, (vwap, sigma)) in got.into_iter().zip(want) {
            let (got_vwap, got_sigma) = got.unwrap();
            assert_close(got_vwap, vwap);
            // One-pass variance cancels to about 1e-14, so its root is near 1e-7.
            assert!(
                (got_sigma - sigma).abs() < 1e-6,
                "sigma {got_sigma}, want {sigma}"
            );
        }

        // Windows that start inside a block or on a block boundary resume
        // from a checkpoint: compare with the definition over a series three
        // blocks long, the forming bar in the third block.
        let long: Vec<Bar> = (0..3 * VWAP_CHECKPOINT)
            .map(|i| {
                let close = 100.0 + (i * 37 % 101) as f64 / 10.0;
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                let volume = (1 + i % 7) as f64;
                Bar::new(time, close, close + 1.0, close - 1.5, close, volume)
            })
            .collect();
        let checkpoints = vwap_checkpoints(&long[..2 * VWAP_CHECKPOINT]);
        for window in [
            VWAP_CHECKPOINT + 5..VWAP_CHECKPOINT + 9,
            2 * VWAP_CHECKPOINT..2 * VWAP_CHECKPOINT + 4,
        ] {
            let got = anchored_vwap(&long, &checkpoints, window.clone());
            for (i, got) in window.zip(got) {
                let prefix = &long[..=i];
                let volume: f64 = prefix.iter().map(|b| b.volume).sum();
                let vwap = prefix
                    .iter()
                    .map(|b| b.typical_price() * b.volume)
                    .sum::<f64>()
                    / volume;
                let variance = prefix
                    .iter()
                    .map(|b| b.volume * (b.typical_price() - vwap).powi(2))
                    .sum::<f64>()
                    / volume;
                let (got_vwap, got_sigma) = got.unwrap();
                assert!(
                    (got_vwap - vwap).abs() < 1e-9,
                    "bar {i}: vwap {got_vwap}, want {vwap}"
                );
                assert!(
                    (got_sigma - variance.sqrt()).abs() < 1e-6,
                    "bar {i}: sigma {got_sigma}, want {}",
                    variance.sqrt()
                );
            }
        }
    }

    /// Reference: rows of 1.5 from 9.0, each bar's volume split by how much of
    /// its range falls in the row, the zero-range bar whole in the row of 13.0.
    /// For 70% the value area takes the two rows below the POC (433.3) over
    /// the one row above it (397.5); comparing single rows would take the row
    /// above.
    #[test]
    fn volume_profile_matches_reference() {
        let profile = VolumeProfile::of(&sample_bars(), 4, None).unwrap();
        assert_close(profile.low, 9.0);
        assert_close(profile.row_height, 1.5);
        for (got, want) in
            profile
                .volume
                .iter()
                .zip([83.33333333333334, 350.0, 619.1666666666667, 397.5])
        {
            assert_close(*got, want);
        }
        // The down bars are 150 over 11..12.5 and 400 over 12..14.5.
        for (row, (up, down)) in [
            (250.0 / 3.0, 0.0),
            (250.0, 100.0),
            (3950.0 / 12.0, 290.0),
            (237.5, 160.0),
        ]
        .into_iter()
        .enumerate()
        {
            assert_close(profile.up[row], up);
            assert_close(profile.down[row], down);
            assert_close(profile.up[row] + profile.down[row], profile.volume[row]);
        }
        for (share, value_area) in [(0.5, (1, 2)), (0.7, (0, 2)), (1.0, (0, 3))] {
            assert_eq!(
                profile.poc_and_value_area(share),
                Some((2, value_area)),
                "share {share}"
            );
        }
    }

    /// Existing rows keep their prices and volume when a fixed grid grows at either edge.
    #[test]
    fn fixed_profile_grows_without_rebinning() {
        let time = chrono::DateTime::UNIX_EPOCH;
        let first = Bar::new(time, -0.5, 1.0, -1.0, 0.5, 20.0);
        let mut bars = vec![first];
        let mut profile = VolumeProfile::of(&bars, 24, Some(1.0)).unwrap();
        assert_eq!(profile.volume, [10.0, 10.0, 0.0]);
        for (low, high, volume, expected) in [
            (-2.0, -1.0, 6.0, vec![6.0, 10.0, 10.0, 0.0]),
            (1.0, 2.0, 8.0, vec![6.0, 10.0, 10.0, 8.0, 0.0]),
            (2.0, 2.0, 3.0, vec![6.0, 10.0, 10.0, 8.0, 3.0]),
        ] {
            let bar = Bar::new(time, low, high, low, high, volume);
            // Extension must succeed: a fallback rebuild would conceal the regression.
            profile = profile.with(&bar).expect("extend fixed grid");
            bars.push(bar);
            assert_eq!(profile.volume, expected);
            assert_eq!(
                Some(profile.clone()),
                VolumeProfile::of(&bars, 24, Some(1.0))
            );
        }
        assert!(VolumeProfile::of(&bars, 24, Some(1e-12)).is_none());
    }

    /// The 24 default rows of the sample bars (9 to 15) are 0.25 apart, 12.5 px
    /// at 50 px per unit, so each row is drawn 11.5 px tall with its 1 px gap,
    /// inside a box 300 px tall, whichever way up the scale is. The VAH and VAL
    /// labels sit 8 px outside the value area: beyond the VAH line on the side
    /// away from the VAL line, and the other way round.
    #[test]
    fn volume_profile_rows_are_drawn_on_either_scale_orientation() {
        let bars = sample_bars();
        let drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        let upright = |price: f64| (16.0 - price) as f32 * 50.0;
        let inverted = |price: f64| (price - 8.0) as f32 * 50.0;
        for (name, price_to_y) in [
            ("upright", &upright as &dyn Fn(f64) -> f32),
            ("inverted", &inverted),
        ] {
            let view = ChartView {
                instrument: None,
                bars: &bars,
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y,
            };
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let painter = egui::Painter::new(
                    ui.ctx().clone(),
                    egui::LayerId::background(),
                    Rect::EVERYTHING,
                );
                drawing.draw_volume_profile(&painter, &view, 0, &bars, 100.0, 500.0);
            });
            let rects: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Rect(r) if r.rect.min.x == 100.0 => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect();
            // The two full-width lines are the value area edges.
            let mut edges: Vec<f32> = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::LineSegment { points, .. }
                        if points[0].x == 100.0 && points[1].x == 500.0 =>
                    {
                        Some(points[0].y)
                    }
                    _ => None,
                })
                .collect();
            let label_y = |label: &str| {
                output.shapes.iter().find_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos.y + t.galley.size().y / 2.0)
                    }
                    _ => None,
                })
            };
            let (vah_label, val_label) = (label_y("VAH").unwrap(), label_y("VAL").unwrap());
            output.drop_without_applying_deltas();
            edges.sort_by(f32::total_cmp);
            let [top, bottom] = edges[..] else {
                panic!("{name}: value area edges {edges:?}")
            };
            // The higher price, VAH, is the top edge upright and the bottom one inverted.
            let (want_vah, want_val) = if name == "upright" {
                (top - 8.0, bottom + 8.0)
            } else {
                (bottom + 8.0, top - 8.0)
            };
            assert!(
                (vah_label - want_vah).abs() < 1.0,
                "{name}: VAH {vah_label}, want {want_vah}"
            );
            assert!(
                (val_label - want_val).abs() < 1.0,
                "{name}: VAL {val_label}, want {want_val}"
            );
            let (boxes, rows): (Vec<_>, Vec<_>) = rects
                .into_iter()
                .partition(|&(_, fill)| fill == Color32::TRANSPARENT);
            assert_eq!(rows.len(), 24, "{name}");
            for (row, _) in rows {
                assert!((row.height() - 11.5).abs() < 1e-3, "{name}: {row:?}");
            }
            assert_eq!(boxes.len(), 1, "{name}");
            let outline = boxes[0].0;
            assert!(
                (outline.height() - 300.0).abs() < 1e-3,
                "{name}: {outline:?}"
            );
        }
    }

    /// The profile of a span that ends on the forming bar follows its ticks
    /// and the next bar, extending the cached profile of the bars before it
    /// while they trade inside its range and rebuilding the rows otherwise.
    /// Either way it is the profile built from all the bars.
    #[test]
    fn span_profile_follows_the_forming_bar() {
        let check = |bars: &[Bar], case: &str| {
            let view = ChartView {
                instrument: None,
                bars,
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y: &|price| price as f32,
            };
            let want = VolumeProfile::of(&bars[1..], 24, None);
            assert_eq!(
                *span_profile(&view, 24, None, 1, &bars[1..]),
                want,
                "{case}"
            );
        };
        let mut bars = sample_bars();
        check(&bars, "first frame");
        for (high, low, close, volume) in [
            (16.0, 12.0, 14.0, 150.0),
            (14.0, 12.5, 13.5, 90.0),
            (14.5, 12.0, 12.5, 130.0),
        ] {
            let forming = bars.last_mut().unwrap();
            (forming.high, forming.low, forming.close, forming.volume) = (high, low, close, volume);
            check(&bars, &format!("tick {low}..{high}"));
        }
        // The bar closes on a trade that was not drawn, and the next one opens.
        bars.last_mut().unwrap().volume += 10.0;
        let time = bars.last().unwrap().time + chrono::Duration::minutes(1);
        bars.push(Bar::new(time, 13.0, 13.5, 12.5, 13.2, 20.0));
        check(&bars, "new bar");
    }
}
