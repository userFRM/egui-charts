//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use super::draw_dashed_hline;
use crate::drawings::domain::Drawing;
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::Bar;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke, epaint::StrokeKind};

/// Price rows in a volume profile.
const PROFILE_ROWS: usize = 24;
/// Share of the profile's volume inside the value area.
const VALUE_AREA_SHARE: f64 = 0.7;

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

        // Only bars inside the clip rect (plus one on each side) become points.
        let clip = painter.clip_rect();
        let margin = ((view.bar_to_x)(1.0) - (view.bar_to_x)(0.0)).abs() + 1.0;
        let mut lines: [Vec<Pos2>; 5] = Default::default();
        let mut last_vwap = None;
        for (i, value) in anchored_vwap(bars).into_iter().enumerate() {
            let x = (view.bar_to_x)((first + i) as f32);
            if x > clip.right() + margin {
                break;
            }
            let Some((vwap, sigma)) = value else { continue };
            if x < clip.left() - margin {
                continue;
            }
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
        if let Some((_, bars)) = bars_between(view.bars, a.bar_idx, b.bar_idx) {
            self.draw_volume_profile(painter, view, bars, p1.x.min(p2.x), p1.x.max(p2.x));
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
            self.draw_volume_profile(painter, view, bars, anchor.x, right);
        }
        self.draw_volume_profile_anchors(painter, &[anchor], self.color32());
    }

    /// Draws the profile of `bars` as rows growing right from `left`, boxed
    /// from `left` to `right` across the bars' price range.
    fn draw_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        bars: &[Bar],
        left: f32,
        right: f32,
    ) {
        let Some(profile) = volume_profile(bars, PROFILE_ROWS) else {
            return;
        };
        let y_at_row =
            |row: usize| (view.price_to_y)(profile.low + row as f64 * profile.row_height);
        let max_width = (right - left) * 0.9;
        let max_volume = profile.volume[profile.poc];
        let (va_low, va_high) = profile.value_area;

        for (row, &volume) in profile.volume.iter().enumerate() {
            let (top, bottom) = (y_at_row(row + 1), y_at_row(row));
            let width = (volume / max_volume) as f32 * max_width;

            // Color coding: POC is bright, Value Area is medium, outside is dim
            let bar_color = if row == profile.poc {
                DESIGN_TOKENS.semantic.extended.favorite_gold
            } else if (va_low..=va_high).contains(&row) {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180)
            } else {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80)
            };

            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(left, top),
                    Pos2::new(left + width, (bottom - 1.0).max(top)),
                ),
                0.0,
                bar_color,
            );
        }

        let rect = Rect::from_min_max(
            Pos2::new(left, y_at_row(PROFILE_ROWS)),
            Pos2::new(right, y_at_row(0)),
        );
        let poc_y = (y_at_row(profile.poc) + y_at_row(profile.poc + 1)) / 2.0;
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

        // VAH label
        painter.text(
            Pos2::new(rect.max.x - 25.0, vah_y - 8.0),
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
            Pos2::new(rect.max.x - 25.0, val_y + 8.0),
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

/// VWAP of the typical price from the first bar, with the volume-weighted
/// standard deviation around it, at each bar; `None` until volume trades.
fn anchored_vwap(bars: &[Bar]) -> Vec<Option<(f64, f64)>> {
    let (mut volume, mut price_volume, mut square_volume) = (0.0, 0.0, 0.0);
    bars.iter()
        .map(|bar| {
            let price = bar.typical_price();
            volume += bar.volume;
            price_volume += price * bar.volume;
            square_volume += price * price * bar.volume;
            (volume > 0.0).then(|| {
                let vwap = price_volume / volume;
                (vwap, (square_volume / volume - vwap * vwap).max(0.0).sqrt())
            })
        })
        .collect()
}

/// Volume traded at each price row of a span of bars.
#[derive(Debug)]
struct VolumeProfile {
    /// Price at the bottom of row 0.
    low: f64,
    row_height: f64,
    /// Volume per row, lowest prices first.
    volume: Vec<f64>,
    /// Row with the most volume (point of control).
    poc: usize,
    /// Lowest and highest row of the value area.
    value_area: (usize, usize),
}

/// Spreads each bar's volume evenly over its high-low range across `rows`
/// equal price rows (a bar without range puts it in the row of its price).
/// `None` when the bars span no price range or trade no volume.
fn volume_profile(bars: &[Bar], rows: usize) -> Option<VolumeProfile> {
    let low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
    let high = bars
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let row_height = (high - low) / rows as f64;
    if !row_height.is_finite() || row_height <= 0.0 {
        return None;
    }
    let row_of = |price: f64| (((price - low) / row_height) as usize).min(rows - 1);

    let mut volume = vec![0.0; rows];
    for bar in bars {
        if bar.high > bar.low {
            let (first, last) = (row_of(bar.low), row_of(bar.high));
            for (row, row_volume) in volume.iter_mut().enumerate().take(last + 1).skip(first) {
                let row_low = low + row as f64 * row_height;
                let overlap = bar.high.min(row_low + row_height) - bar.low.max(row_low);
                *row_volume += bar.volume * overlap.max(0.0) / (bar.high - bar.low);
            }
        } else {
            volume[row_of(bar.low)] += bar.volume;
        }
    }

    let total: f64 = volume.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let poc = (0..rows).max_by(|&a, &b| volume[a].total_cmp(&volume[b]))?;
    let value_area = value_area(&volume, poc, total * VALUE_AREA_SHARE);
    Some(VolumeProfile {
        low,
        row_height,
        volume,
        poc,
        value_area,
    })
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
        let got = anchored_vwap(&sample_bars()[1..]);
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
    }

    /// Reference: rows of 1.5 from 9.0, each bar's volume split by how much of
    /// its range falls in the row, the zero-range bar whole in the row of 13.0.
    /// The value area takes the two rows below the POC (433.3) over the one row
    /// above it (397.5); comparing single rows would take the row above.
    #[test]
    fn volume_profile_matches_reference() {
        let profile = volume_profile(&sample_bars(), 4).unwrap();
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
        assert_eq!(profile.poc, 2);
        assert_eq!(profile.value_area, (0, 2));
    }
}
