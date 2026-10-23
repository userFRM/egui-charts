//! Measurement tool rendering implementations
//!
//! Includes: measure, price range, date range, date and price range, info line, and bars pattern.

use super::{ChartView, bars_between};
use crate::chart::renderers::price_decimals;
use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::model::{Bar, percent_change};
use crate::scales::{PriceFormatter, VolumeFormatter};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    pub(crate) fn render_measure(&self, painter: &egui::Painter, view: &ChartView) {
        // Need at least 1 point (preview adds 2nd point via update_last_point)
        if self.points.is_empty() {
            return;
        }

        // Show semi-transparent rect if we have a second point (during preview or after completion)
        if self.points.len() >= 2 && self.chart_points.len() >= 2 {
            let start = self.points[0];
            let end = self.points[1];
            let cp1 = &self.chart_points[0];
            let cp2 = &self.chart_points[1];

            // Calculate price change to determine color (RED for down, BLUE for up)
            let price_diff = cp2.price - cp1.price;
            let is_measuring_down = price_diff < 0.0;

            // Color based on direction: Red for down, Blue for up
            let base_color = if is_measuring_down {
                DESIGN_TOKENS.semantic.extended.bearish // Red (down)
            } else {
                DESIGN_TOKENS.semantic.extended.accent // Blue (up)
            };

            // Draw SEMI-TRANSPARENT RECTANGLE filling the area from start to cursor
            // Color changes based on direction: red down, blue up
            let rect = egui::Rect::from_two_pos(start, end);
            let fill_color = Color32::from_rgba_unmultiplied(
                base_color.r(),
                base_color.g(),
                base_color.b(),
                30, // Very transparent
            );
            painter.rect_filled(rect, 0.0, fill_color);

            // Draw thin border lines around the rect
            let border_color = Color32::from_rgba_unmultiplied(
                base_color.r(),
                base_color.g(),
                base_color.b(),
                100,
            );
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(stroke::HAIRLINE, border_color),
                egui::epaint::StrokeKind::Outside,
            );

            // Draw cross lines inside rect (+ shape with arrows)
            let center_x = (rect.min.x + rect.max.x) / 2.0;
            let center_y = (rect.min.y + rect.max.y) / 2.0;
            let line_color = Color32::from_rgba_unmultiplied(
                base_color.r(),
                base_color.g(),
                base_color.b(),
                150,
            );
            let line_stroke = Stroke::new(stroke::MEDIUM, line_color);

            // Vertical line (from top to bottom)
            painter.line_segment(
                [
                    Pos2::new(center_x, rect.min.y),
                    Pos2::new(center_x, rect.max.y),
                ],
                line_stroke,
            );

            // Horizontal line (from left to right)
            painter.line_segment(
                [
                    Pos2::new(rect.min.x, center_y),
                    Pos2::new(rect.max.x, center_y),
                ],
                line_stroke,
            );

            // Draw arrow at right end of horizontal line (pointing right)
            let arrow_size = 8.0;
            let h_arrow_tip = Pos2::new(rect.max.x, center_y);
            let h_arrow_left = h_arrow_tip + egui::vec2(-arrow_size, -arrow_size * 0.4);
            let h_arrow_right = h_arrow_tip + egui::vec2(-arrow_size, arrow_size * 0.4);
            painter.add(egui::Shape::convex_polygon(
                vec![h_arrow_tip, h_arrow_left, h_arrow_right],
                line_color,
                Stroke::NONE,
            ));

            // Draw arrow at top/bottom of vertical line based on direction
            if price_diff < 0.0 {
                // Measuring down: arrow at bottom pointing down
                let v_arrow_tip = Pos2::new(center_x, rect.max.y);
                let v_arrow_left = v_arrow_tip + egui::vec2(-arrow_size * 0.4, -arrow_size);
                let v_arrow_right = v_arrow_tip + egui::vec2(arrow_size * 0.4, -arrow_size);
                painter.add(egui::Shape::convex_polygon(
                    vec![v_arrow_tip, v_arrow_left, v_arrow_right],
                    line_color,
                    Stroke::NONE,
                ));
            } else {
                // Measuring up: arrow at top pointing up
                let v_arrow_tip = Pos2::new(center_x, rect.min.y);
                let v_arrow_left = v_arrow_tip + egui::vec2(-arrow_size * 0.4, arrow_size);
                let v_arrow_right = v_arrow_tip + egui::vec2(arrow_size * 0.4, arrow_size);
                painter.add(egui::Shape::convex_polygon(
                    vec![v_arrow_tip, v_arrow_left, v_arrow_right],
                    line_color,
                    Stroke::NONE,
                ));
            }

            // Draw + symbol at center where lines cross (same color and thickness as cross lines)
            let plus_size = 6.0;
            painter.line_segment(
                [
                    Pos2::new(center_x - plus_size, center_y),
                    Pos2::new(center_x + plus_size, center_y),
                ],
                line_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center_x, center_y - plus_size),
                    Pos2::new(center_x, center_y + plus_size),
                ],
                line_stroke,
            );

            let info = self
                .measurement_label(view.bars, view.instrument)
                .unwrap_or_default();

            // Draw info box at the top-left corner of rect
            let font = egui::FontId::proportional(typography::SM);
            let galley = painter.layout_no_wrap(info.clone(), font.clone(), Color32::WHITE);
            let text_size = galley.size();

            // Pos box near the top-left corner of rect
            let box_x = rect.min.x.min(rect.max.x) + 5.0;
            let box_y = rect.min.y.min(rect.max.y) - text_size.y - 15.0;

            // Draw background (solid color matching direction: red for down, blue for up)
            let padding = egui::vec2(8.0, 6.0);
            let size = text_size + padding * 2.0;
            let clip = painter.clip_rect();
            let pos = Pos2::new(box_x, box_y).clamp(clip.min, clip.max - size.min(clip.size()));
            let (box_x, box_y) = (pos.x, pos.y);
            let box_rect = egui::Rect::from_min_size(pos, size);
            painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.md, base_color);

            // Draw text with padding
            painter.text(
                Pos2::new(box_x + padding.x, box_y + padding.y),
                egui::Align2::LEFT_TOP,
                info,
                font,
                Color32::WHITE,
            );
        }
    }

    pub(crate) fn render_price_range(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
        view: &ChartView,
    ) {
        if self.points.len() < 2 {
            if !self.points.is_empty() {
                painter.circle_filled(
                    self.points[0],
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Horizontal lines at both price levels
        painter.hline(
            chart_rect.x_range(),
            p1.y,
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.hline(
            chart_rect.x_range(),
            p2.y,
            Stroke::new(stroke::HAIRLINE, color),
        );

        // Vertical connector
        let mid_x = (p1.x + p2.x) / 2.0;
        painter.vline(
            mid_x,
            p1.y.min(p2.y)..=p1.y.max(p2.y),
            Stroke::new(stroke::HAIRLINE, color),
        );

        let info = self
            .measurement_label(&[], view.instrument)
            .unwrap_or_default();

        painter.text(
            Pos2::new(mid_x + 10.0, (p1.y + p2.y) / 2.0),
            egui::Align2::LEFT_CENTER,
            info,
            egui::FontId::proportional(typography::SM),
            color,
        );

        painter.circle_filled(p1, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
    }

    pub(crate) fn render_date_range(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
        view: &ChartView,
    ) {
        if self.points.len() < 2 {
            if !self.points.is_empty() {
                painter.circle_filled(
                    self.points[0],
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Vertical lines at both time points
        painter.vline(
            p1.x,
            chart_rect.y_range(),
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.vline(
            p2.x,
            chart_rect.y_range(),
            Stroke::new(stroke::HAIRLINE, color),
        );

        // Horizontal connector
        let mid_y = (p1.y + p2.y) / 2.0;
        painter.hline(
            p1.x.min(p2.x)..=p1.x.max(p2.x),
            mid_y,
            Stroke::new(stroke::HAIRLINE, color),
        );

        let info = self
            .measurement_label(view.bars, view.instrument)
            .unwrap_or_default();

        painter.text(
            Pos2::new((p1.x + p2.x) / 2.0, mid_y - 10.0),
            egui::Align2::CENTER_BOTTOM,
            info,
            egui::FontId::proportional(typography::SM),
            color,
        );

        // Fill the range area
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);
        let range_rect =
            Rect::from_x_y_ranges(p1.x.min(p2.x)..=p1.x.max(p2.x), chart_rect.y_range());
        painter.rect_filled(range_rect, 0.0, fill_color);

        painter.circle_filled(p1, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
    }

    // === NEW TOOL RENDER FUNCTIONS ===

    pub(crate) fn render_date_and_price_range(&self, painter: &egui::Painter, view: &ChartView) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw rect covering both ranges
        let rect = Rect::from_two_pos(p1, p2);
        let fill = Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 30);
        painter.rect_filled(rect, 0.0, fill);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );

        let info = self
            .measurement_label(view.bars, view.instrument)
            .unwrap_or_default();
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            info,
            egui::FontId::proportional(typography::XS),
            color,
        );
    }

    pub(crate) fn render_info_line(&self, painter: &egui::Painter, view: &ChartView) {
        if self.points.len() < 2 {
            return;
        }
        let start = self.points[0];
        let end = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw the main line with proper stroke
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));

        // Draw anchor points
        painter.circle_filled(start, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(end, DESIGN_TOKENS.rounding.md, color);

        let Some(info_text) = self.measurement_label(view.bars, view.instrument) else {
            return;
        };
        let is_positive = self.chart_points[1].price >= self.chart_points[0].price;

        // Calculate midpoint for info box positioning
        let mid = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);

        // Info box styling
        let font = egui::FontId::proportional(typography::XS);
        let galley = painter.layout_no_wrap(info_text.clone(), font.clone(), Color32::WHITE);
        let text_size = galley.size();

        // Position box above or below the midpoint based on line direction
        let box_offset = if end.y < start.y {
            -text_size.y - 15.0
        } else {
            15.0
        };
        let box_pos = Pos2::new(mid.x - text_size.x / 2.0 - 8.0, mid.y + box_offset);

        // Draw background box with color based on direction
        let bg_color = if is_positive {
            DESIGN_TOKENS.semantic.extended.bullish // Green/teal (up)
        } else {
            DESIGN_TOKENS.semantic.extended.bearish // Red (down)
        };

        let padding = egui::vec2(8.0, 6.0);
        let size = text_size + padding * 2.0;
        let clip = painter.clip_rect();
        let box_pos = box_pos.clamp(clip.min, clip.max - size.min(clip.size()));
        let box_rect = Rect::from_min_size(box_pos, size);
        painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.md, bg_color);

        // Draw text
        painter.text(
            box_pos + padding,
            egui::Align2::LEFT_TOP,
            info_text,
            font,
            Color32::WHITE,
        );

        // Draw connecting line from box to midpoint (thin dashed)
        let connector_start = Pos2::new(
            box_rect.center().x,
            if end.y < start.y {
                box_rect.max.y
            } else {
                box_rect.min.y
            },
        );
        let connector_stroke = Stroke::new(
            stroke::HAIRLINE,
            Color32::from_rgba_unmultiplied(bg_color.r(), bg_color.g(), bg_color.b(), 150),
        );
        painter.line_segment([connector_start, mid], connector_stroke);
    }

    // Note: render_bars_pattern is now in trading.rs with full projection features

    /// Label of the measuring tools: the price change from the first anchor to the second and
    /// its percentage, the bars between them and the time they span, the volume traded over them,
    /// and for the info line the angle of the line on screen. Each anchor counts at its nearest
    /// bar: anchors N bars apart span N bars of time and the volume of the N + 1 bars from one to
    /// the other, both included.
    pub(crate) fn measurement_label(
        &self,
        bars: &[Bar],
        instrument: Option<&crate::model::Instrument>,
    ) -> Option<String> {
        let [a, b, ..] = &self.chart_points[..] else {
            return None;
        };
        let change = b.price - a.price;
        let pct = percent_change(a.price, b.price);
        let decimals = price_decimals(a.price);
        let mut change_label = format!("{change:+.decimals$} ({pct:+.2}%)");
        if let Some(instrument) = instrument {
            for (size, unit) in [
                (instrument.tick_size, "ticks"),
                (instrument.pip_size, "pips"),
            ] {
                if let Some(size) = size.filter(|s| s.is_finite() && *s > 0.0) {
                    let count = change / size;
                    if count.is_finite() {
                        change_label.push_str(&format!("  {count:+.2} {unit}"));
                    }
                }
            }
        }
        let change = change_label;
        let (from, to) = (a.bar_idx.round(), b.bar_idx.round());
        let mut span = format!("{} bars", (to - from).abs() as i64);
        if let Some(ms) = elapsed_ms(bars, from, to) {
            span = format!("{span}, {}", format_duration(ms));
        }
        let volume = bars_between(bars, from, to)
            .map(|(_, bars)| {
                let volume = bars
                    .iter()
                    .map(|bar| bar.volume)
                    .filter(|v| v.is_finite())
                    .sum();
                format!("\nVol {}", VolumeFormatter::default().format(volume))
            })
            .unwrap_or_default();
        Some(match self.tool_type {
            DrawingToolType::Measure | DrawingToolType::DateAndPriceRange => {
                format!("{change}\n{span}{volume}")
            }
            DrawingToolType::PriceRange => change,
            DrawingToolType::DateRange => format!("{span}{volume}"),
            DrawingToolType::TrendAngle => format!("{change}, {span}"),
            DrawingToolType::InfoLine => {
                let [start, end, ..] = self.points[..] else {
                    return None;
                };
                // Screen y grows downward, so a rising line has a positive angle.
                let angle = (start.y - end.y).atan2(end.x - start.x).to_degrees();
                format!("{change}\n{span}\n{angle:.1}°")
            }
            _ => return None,
        })
    }

    /// The size labels of the flat top/bottom and the disjoint channel (the price distance
    /// between their two lines at the bar of the label) and of the rectangle (its width in bars,
    /// then its height in price), from the anchors' bars and prices. The lines are straight on
    /// screen, so their prices between the anchors come from the chart's `price_to_y`, which
    /// on a logarithmic scale puts the trend line's midpoint at the geometric mean.
    pub(crate) fn size_labels(&self, price_to_y: &dyn Fn(f64) -> f32) -> Vec<String> {
        let gap = |a: f64, b: f64| {
            let (low, gap) = (a.min(b), (a - b).abs());
            let pct = percent_change(low, low + gap);
            let decimals = price_decimals(low);
            format!("{gap:.decimals$} ({pct:.2}%)")
        };
        let label = |a: Option<f64>, b: Option<f64>| a.zip(b).map(|(a, b)| gap(a, b));
        match (self.tool_type, &self.chart_points[..]) {
            // Labelled halfway along the trend line.
            (DrawingToolType::FlatTopBottom, [a, b, flat, ..]) => {
                let mid = (price_to_y(a.price) + price_to_y(b.price)) / 2.0;
                label(Some(flat.price), price_at_y(price_to_y, mid, a.price))
                    .into_iter()
                    .collect()
            }
            // Labelled at the bar halfway between the second anchor and the third.
            (DrawingToolType::DisjointChannel, [a, b, c, ..]) => {
                let (ya, yb) = (price_to_y(a.price), price_to_y(b.price));
                let first_y = |bar: f32| {
                    let run = b.bar_idx - a.bar_idx;
                    let t = if run != 0.0 {
                        (bar - a.bar_idx) / run
                    } else {
                        0.0
                    };
                    ya + (yb - ya) * t
                };
                let y = first_y((b.bar_idx + c.bar_idx) / 2.0);
                let second_y = y + price_to_y(c.price) - first_y(c.bar_idx);
                label(
                    price_at_y(price_to_y, y, a.price),
                    price_at_y(price_to_y, second_y, c.price),
                )
                .into_iter()
                .collect()
            }
            (DrawingToolType::Rect, [a, b, ..]) => vec![
                format!("{} bars", (b.bar_idx - a.bar_idx).abs() as i64),
                gap(a.price, b.price),
            ],
            _ => Vec::new(),
        }
    }
}

/// The price that `price_to_y`, which is monotone, puts at screen `y`: a
/// bisection between `near` and a price on the other side of `y`, sought
/// outward from `near` in doubling steps, added and multiplied so that a
/// logarithmic scale is reached as well as a linear one.
fn price_at_y(price_to_y: &dyn Fn(f64) -> f32, y: f32, near: f64) -> Option<f64> {
    let off = |price: f64| {
        let at = price_to_y(price);
        (price.is_finite() && at.is_finite()).then_some(at - y)
    };
    let start = off(near)?;
    let beyond = |price: f64| off(price).is_some_and(|d| d * start <= 0.0);
    let far = (0..64).find_map(|k| {
        let k = 2f64.powi(k);
        let step = near.abs().max(1.0) * k;
        [near + step, near - step, near * (1.0 + k), near / (1.0 + k)]
            .into_iter()
            .find(|&price| beyond(price))
    })?;
    let (mut lo, mut hi) = (near, far);
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        if mid == lo || mid == hi {
            break;
        }
        if beyond(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// Paints `text` over a `bg` box sized to it, the box placed at `pos` by `align`.
pub(super) fn boxed_label(
    painter: &egui::Painter,
    align: egui::Align2,
    pos: Pos2,
    text: String,
    color: Color32,
    bg: Color32,
) {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(typography::XS), color);
    let rect = align.anchor_size(pos, galley.size() + egui::vec2(8.0, 2.0));
    painter.rect_filled(rect, DESIGN_TOKENS.rounding.sm, bg);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// Milliseconds from bar `a` to bar `b`, from the bars' times. An index past either end of the
/// series is placed at the most common spacing of the bars at that end; `None` with fewer than
/// two bars.
fn elapsed_ms(bars: &[Bar], a: f32, b: f32) -> Option<i64> {
    let time = |index: f32| {
        let last = bars.len().checked_sub(1)?;
        let ms = |i: usize| bars[i].time.timestamp_millis();
        let index = index.round() as i64;
        let (edge, step) = match usize::try_from(index) {
            Ok(i) if i <= last => return Some(ms(i)),
            _ if last == 0 => return None,
            Ok(_) => (last, spacing(bars.iter().rev())),
            Err(_) => (0, spacing(bars.iter())),
        };
        Some(ms(edge).saturating_add((index - edge as i64).saturating_mul(step)))
    };
    Some(time(b)?.saturating_sub(time(a)?))
}

/// The most common time between consecutive bars among the first eleven
/// `bars` yields, so that a weekend or a gap next to the end of the series does
/// not set the step of the bars placed past it.
fn spacing<'a>(bars: impl Iterator<Item = &'a Bar>) -> i64 {
    let times: Vec<i64> = bars
        .take(11)
        .map(|bar| bar.time.timestamp_millis())
        .collect();
    let mut steps: Vec<i64> = times.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    steps.sort_unstable();
    steps
        .chunk_by(|a, b| a == b)
        .max_by_key(|run| run.len())
        .map_or(0, |run| run[0])
}

/// A duration in its two largest units: days and hours, hours and minutes, or minutes, and
/// seconds below a minute.
fn format_duration(ms: i64) -> String {
    let secs = ms.unsigned_abs() / 1000;
    match (secs / 86_400, secs / 3600 % 24, secs / 60 % 60) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, 0) => format!("{h}h"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, 0, _) => format!("{d}d"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::ChartPoint;
    use crate::drawings::rendering::sample_bars;

    /// Hourly bars with volumes 100, 200, 150, 300, 250, 400 and 50, the last one 49 h after the
    /// one before it, at 54 h, as across a weekend. Prices change from the first anchor to the
    /// second with their sign, durations come from the bar times (past the last bar at their
    /// common spacing of 1 h, so bar 30 is at 54 + 24 = 78 h, 3d 6h), volume is the sum over the bars from one anchor
    /// to the other, and a line that rises on screen has a positive angle: 10 px up over 300 px
    /// is atan(10 / 300) = 1.9 degrees.
    #[test]
    fn measurement_labels_use_bar_times_volumes_and_signed_values() {
        let mut bars = sample_bars();
        for (i, bar) in bars.iter_mut().enumerate() {
            bar.time = chrono::DateTime::from_timestamp(i as i64 * 3600, 0).unwrap();
        }
        bars[6].time = chrono::DateTime::from_timestamp(54 * 3600, 0).unwrap();
        let cases = [
            (
                DrawingToolType::Measure,
                (1.0, 100.0),
                (4.0, 95.0),
                "-5.00 (-5.00%)\n3 bars, 3h\nVol 900.00",
            ),
            (
                DrawingToolType::InfoLine,
                (0.0, 100.0),
                (30.0, 110.0),
                "+10.00 (+10.00%)\n30 bars, 3d 6h\n1.9°",
            ),
            (
                DrawingToolType::DateRange,
                (2.0, 100.0),
                (5.0, 100.0),
                "3 bars, 3h\nVol 1.10K",
            ),
            // Fractional anchors count at their nearest bars, 1 and 4.
            (
                DrawingToolType::DateRange,
                (1.4, 100.0),
                (3.6, 100.0),
                "3 bars, 3h\nVol 900.00",
            ),
            (
                DrawingToolType::DateAndPriceRange,
                (5.0, 200.0),
                (2.0, 210.0),
                "+10.00 (+5.00%)\n3 bars, 3h\nVol 1.10K",
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, (b1, p1), (b2, p2), want) in cases {
            let mut drawing = Drawing::new(1, tool);
            drawing.chart_points = vec![ChartPoint::new(b1, p1), ChartPoint::new(b2, p2)];
            drawing.update_screen_coords(|bar| bar * 10.0, |price| 500.0 - price as f32);
            let got = drawing.measurement_label(&bars, None);
            if got.as_deref() != Some(want) {
                wrong.push(format!("{tool:?}: got {got:?}, want {want:?}"));
            }
        }
        let mut drawing = Drawing::new(0, DrawingToolType::Measure);
        drawing.chart_points = vec![ChartPoint::new(1.0, 1.1000), ChartPoint::new(4.0, 1.0995)];
        for (tick, pip, want) in [
            (Some(0.00001), Some(0.0001), "-50.00 ticks  -5.00 pips"),
            (Some(0.0), Some(f64::NAN), ""),
            (Some(-1.0), Some(f64::INFINITY), ""),
            (None, None, ""),
        ] {
            let mut instrument = crate::model::Instrument::new("PAIR");
            instrument.tick_size = tick;
            instrument.pip_size = pip;
            let label = drawing.measurement_label(&bars, Some(&instrument)).unwrap();
            if want.is_empty() {
                assert!(!label.contains("ticks") && !label.contains("pips"));
            } else {
                assert!(label.contains(want), "{label}");
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn measurements_receive_the_chart_instrument() {
        use crate::widget::test_frames::{chart, shapes};
        let mut chart = chart(&[90.0], &[110.0]);
        let ctx = egui::Context::default();
        let mut drawing = Drawing::new(1, DrawingToolType::Measure);
        drawing.chart_points = vec![ChartPoint::new(30.0, 100.0), ChartPoint::new(40.0, 95.0)];
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        chart.set_instrument(Some(
            crate::model::Instrument::new("TEST").with_tick_size(0.25),
        ));
        let output = shapes(&ctx, &mut chart, 0.0, vec![]);
        assert!(output.iter().any(
            |s| matches!(s,egui::Shape::Text(t) if t.galley.job.text.contains("-20.00 ticks"))
        ));
    }

    /// Flat top/bottom: trend line 100 at bar 0 to 104 at bar 10, flat line at 110; at bar 5 the
    /// trend line is at 102, 8 below, 8 / 102 = 7.84%. Disjoint channel: the same first line, the
    /// second starting at 108 at bar 0 and so at 110 at bar 5, also 8 above 102. Rectangle: 100 to
    /// 110 over bars 0 to 20 is 10 (10 / 100 = 10%) high and 20 bars wide. Each is labelled the
    /// same through two price maps with different ranges. On a logarithmic scale the lines are
    /// straight in log price: the trend line from 100 at bar 0 to 200 at bar 10 is at
    /// sqrt(100 * 200) = 141.42 at bar 5, 108.58 (76.78%) below a flat line at 250, and a
    /// disjoint second line starting at 150 stays 1.5 times the first, 70.71 (50.00%) above it.
    #[test]
    fn size_labels_are_in_price_and_bars() {
        let cases = [
            (
                DrawingToolType::FlatTopBottom,
                vec![(0.0, 100.0), (10.0, 104.0), (5.0, 110.0)],
                vec!["8.00 (7.84%)"],
                false,
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![(0.0, 100.0), (10.0, 104.0), (0.0, 108.0)],
                vec!["8.00 (7.84%)"],
                false,
            ),
            (
                DrawingToolType::Rect,
                vec![(0.0, 100.0), (20.0, 110.0)],
                vec!["20 bars", "10.00 (10.00%)"],
                false,
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![(0.0, 100.0), (10.0, 200.0), (5.0, 250.0)],
                vec!["108.58 (76.78%)"],
                true,
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![(0.0, 100.0), (10.0, 200.0), (0.0, 150.0)],
                vec!["70.71 (50.00%)"],
                true,
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, anchors, want, log) in cases {
            let mut drawing = Drawing::new(1, tool);
            drawing.chart_points = anchors
                .into_iter()
                .map(|(bar, price)| ChartPoint::new(bar, price))
                .collect();
            for (scale, top) in [(10.0, 120.0), (3.0, 300.0)] {
                let value = |price: f64| if log { price.ln() * 100.0 } else { price };
                let price_to_y = |price: f64| ((value(top) - value(price)) * scale) as f32;
                drawing.update_screen_coords(|bar| bar * 10.0, price_to_y);
                let got = drawing.size_labels(&price_to_y);
                if got != want {
                    wrong.push(format!(
                        "{tool:?} at {scale} px per unit, log {log}: got {got:?}, want {want:?}"
                    ));
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
