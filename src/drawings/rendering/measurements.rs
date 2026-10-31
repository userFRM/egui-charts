//! Measurement tool rendering implementations
//!
//! Includes: measure, price range, date range, date and price range, info line, and bars pattern.

use super::{ChartView, bars_between};
use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::model::{Bar, percent_change};
use crate::scales::price_decimals;
use crate::scales::{PriceFormatter, VolumeFormatter};
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Pos2, Rect, Stroke};

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
            let fill_color = self.fill_color32().unwrap_or_else(|| {
                Color32::from_rgba_unmultiplied(
                    base_color.r(),
                    base_color.g(),
                    base_color.b(),
                    30, // Very transparent
                )
            });
            painter.rect_filled(rect, 0.0, fill_color);

            let line_color = Color32::from_rgba_unmultiplied(
                base_color.r(),
                base_color.g(),
                base_color.b(),
                150,
            );
            let line_stroke = Stroke::new(stroke::MEDIUM, line_color);

            let center = rect.center();
            arrow(
                painter,
                Pos2::new(center.x, start.y),
                Pos2::new(center.x, end.y),
                line_stroke,
            );
            arrow(
                painter,
                Pos2::new(start.x, center.y),
                Pos2::new(end.x, center.y),
                line_stroke,
            );
            self.range_card(painter, rect, view);
        }
    }

    /// Renders the price range, date range and date and price range: a box between the anchors
    /// with edges at the measured prices, bars or both, arrows from the first anchor towards the
    /// second, and the measurement on a card. The price edges reach the pane's side when the
    /// drawing extends that way.
    pub(crate) fn render_range(
        &self,
        painter: &egui::Painter,
        pane: Rect,
        stroke: Stroke,
        view: &ChartView,
    ) {
        let &[p1, p2, ..] = self.points.as_slice() else {
            if let Some(&p) = self.points.first() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.sm, self.color32());
            }
            return;
        };
        let rect = Rect::from_two_pos(p1, p2);
        let center = rect.center();
        if let Some(fill) = self.fill_color32() {
            painter.rect_filled(rect, 0.0, fill);
        }
        if self.tool_type != DrawingToolType::DateRange
            && let Some(edges) = self.rectangle_bounds(pane)
        {
            for y in [p1.y, p2.y] {
                painter.hline(edges.x_range(), y, stroke);
            }
            arrow(
                painter,
                Pos2::new(center.x, p1.y),
                Pos2::new(center.x, p2.y),
                stroke,
            );
        }
        if self.tool_type != DrawingToolType::PriceRange {
            for x in [p1.x, p2.x] {
                painter.vline(x, rect.y_range(), stroke);
            }
            arrow(
                painter,
                Pos2::new(p1.x, center.y),
                Pos2::new(p2.x, center.y),
                stroke,
            );
        }
        self.range_card(painter, rect, view);
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

        let clip = painter.clip_rect();
        // A line out of view leaves no box at the pane's edge.
        let Some(info_text) = self
            .measurement_label(view.bars, view.instrument, view.time_axis)
            .filter(|_| crate::painting::dashed_line::clip_segment(start, end, clip).is_some())
        else {
            self.render_line_text(painter, start, end, end.y >= start.y);
            return;
        };
        let mid = start.lerp(end, 0.5);
        let card = stats_card(
            painter,
            Align2::LEFT_TOP,
            mid + egui::vec2(8.0, 8.0),
            info_text,
        );
        // Keep custom text on the side away from the card, even after the card is clamped.
        let angle = super::lines::upright_angle(start, end);
        let up = egui::vec2(angle.sin(), -angle.cos());
        let above = (card.center() - mid).dot(up) <= 0.0;
        let side = if above { up } else { -up };
        let reach = [card.min, card.max, card.left_bottom(), card.right_top()]
            .into_iter()
            .map(|corner| (corner - mid).dot(side))
            .fold(0.0_f32, f32::max);
        let offset = side * reach;
        self.render_line_text(painter, start + offset, end + offset, above);
    }

    // Note: render_bars_pattern is now in trading.rs with full projection features

    /// Label of the measuring tools: the price change from the first anchor to the second and
    /// its percentage, the bars between them and, on a `time_axis`, the time they span, the
    /// volume traded over them, and for the info line the angle of the line on screen. Each
    /// anchor counts at its nearest bar: anchors N bars apart span N bars of time and the volume
    /// of the N + 1 bars from one to the other, both included.
    pub(crate) fn measurement_label(
        &self,
        bars: &[Bar],
        instrument: Option<&crate::model::Instrument>,
        time_axis: bool,
    ) -> Option<String> {
        let [a, b, ..] = &self.chart_points[..] else {
            return None;
        };
        let change = b.price - a.price;
        let pct = percent_change(a.price, b.price);
        let decimals = change_decimals(instrument, a.price, change);
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
        if self.tool_type == DrawingToolType::PriceRange {
            return Some(change);
        }
        let (from, to) = (a.bar_idx.round(), b.bar_idx.round());
        let mut span = format!("{} bars", (to - from).abs() as i64);
        if let Some(ms) = elapsed_ms(bars, from, to).filter(|_| time_axis) {
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
            DrawingToolType::DateRange => format!("{span}{volume}"),
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

    /// The rectangle's width in bars and height in price, from its stored anchors.
    pub(crate) fn size_labels(&self) -> Vec<String> {
        let gap = |a: f64, b: f64| {
            let (low, gap) = (a.min(b), (a - b).abs());
            let pct = percent_change(low, low + gap);
            let decimals = price_decimals(low);
            format!("{gap:.decimals$} ({pct:.2}%)")
        };
        match (self.tool_type, &self.chart_points[..]) {
            (DrawingToolType::Rect, [a, b, ..]) => vec![
                format!("{} bars", (b.bar_idx - a.bar_idx).abs() as i64),
                gap(a.price, b.price),
            ],
            _ => Vec::new(),
        }
    }
}

/// Decimals of a price change from `price`, as the chart writes prices: the instrument's price
/// precision, or without one by the price's magnitude and enough to show a non-zero `change`.
pub(super) fn change_decimals(
    instrument: Option<&crate::model::Instrument>,
    price: f64,
    change: f64,
) -> usize {
    instrument
        .and_then(crate::scales::instrument_price_precision)
        .map_or_else(
            || {
                let first_digit = if change != 0.0 && change.is_finite() {
                    (-change.abs().log10()).ceil().max(0.0) as usize
                } else {
                    0
                };
                price_decimals(price).max(first_digit)
            },
            usize::from,
        )
}

impl Drawing {
    /// Paints the measurement on a card centred on `rect`: above it when the second anchor is
    /// level with or above the first on screen, below it otherwise.
    fn range_card(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        let (Some(text), [first, second, ..]) = (
            self.measurement_label(view.bars, view.instrument, view.time_axis),
            self.points.as_slice(),
        ) else {
            return;
        };
        let (align, pos) = if second.y <= first.y {
            (
                Align2::CENTER_BOTTOM,
                rect.center_top() - egui::vec2(0.0, 6.0),
            )
        } else {
            (
                Align2::CENTER_TOP,
                rect.center_bottom() + egui::vec2(0.0, 6.0),
            )
        };
        stats_card(painter, align, pos, text);
    }
}

/// Paints `text` on a card in the theme's tooltip colours, placed at `pos` by `align` and
/// moved inside the painter's clip rect.
fn stats_card(painter: &egui::Painter, align: Align2, pos: Pos2, text: String) -> Rect {
    let chart = painter.ctx().active_theme().chart;
    let galley = painter.layout_no_wrap(
        text,
        egui::FontId::proportional(typography::SM_MD),
        chart.axis_text,
    );
    let padding = egui::vec2(8.0, 6.0);
    let size = galley.size() + padding * 2.0;
    let clip = painter.clip_rect();
    let min = align
        .anchor_size(pos, size)
        .min
        .clamp(clip.min, clip.max - size.min(clip.size()));
    painter.rect_filled(
        Rect::from_min_size(min, size),
        DESIGN_TOKENS.rounding.md,
        chart.bg_tooltip,
    );
    painter.galley(min + padding, galley, chart.axis_text);
    Rect::from_min_size(min, size)
}

/// Paints a shaft from `from` to `to` and a filled head at `to`; nothing when they meet.
fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let direction = to - from;
    if direction.length() < 1e-3 {
        return;
    }
    painter.line_segment([from, to], stroke);
    let along = direction.normalized() * (3.0 * stroke.width).max(8.0);
    let across = egui::vec2(-along.y, along.x) * 0.4;
    let base = to - along;
    painter.add(egui::Shape::convex_polygon(
        vec![to, base + across, base - across],
        stroke.color,
        Stroke::NONE,
    ));
}

/// Paints `text` over a `bg` box sized to it, placed at `pos` by `align`, and returns its bounds.
pub(super) fn boxed_label(
    painter: &egui::Painter,
    align: egui::Align2,
    pos: Pos2,
    text: String,
    color: Color32,
    bg: Color32,
) -> Rect {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(typography::XS), color);
    let rect = align.anchor_size(pos, galley.size() + egui::vec2(8.0, 2.0));
    painter.rect_filled(rect, DESIGN_TOKENS.rounding.sm, bg);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    rect
}

/// Milliseconds from bar `a` to bar `b`, from the bars' times. An index past either end of the
/// series is placed at the most common spacing of the bars at that end; `None` with fewer than
/// two bars.
pub(super) fn elapsed_ms(bars: &[Bar], a: f32, b: f32) -> Option<i64> {
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
pub(super) fn format_duration(ms: i64) -> String {
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
            let got = drawing.measurement_label(&bars, None, true);
            if got.as_deref() != Some(want) {
                wrong.push(format!("{tool:?}: got {got:?}, want {want:?}"));
            }
        }
        let mut drawing = Drawing::new(0, DrawingToolType::Measure);
        drawing.chart_points = vec![ChartPoint::new(1.0, 1.1000), ChartPoint::new(4.0, 1.0995)];
        // On a horizontal scale other than time the bar times are keys, not times.
        assert_eq!(
            drawing.measurement_label(&bars, None, false).as_deref(),
            Some("-0.0005 (-0.05%)\n3 bars\nVol 900.00")
        );
        for (tick, pip, want) in [
            (Some(0.00001), Some(0.0001), "-50.00 ticks  -5.00 pips"),
            (Some(0.0), Some(f64::NAN), ""),
            (Some(-1.0), Some(f64::INFINITY), ""),
            (None, None, ""),
        ] {
            let mut instrument = crate::model::Instrument::new("PAIR");
            instrument.tick_size = tick;
            instrument.pip_size = pip;
            let label = drawing
                .measurement_label(&bars, Some(&instrument), true)
                .unwrap();
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

    /// 100 to 110 over bars 0 to 20 is 10 (10%) high and 20 bars wide.
    #[test]
    fn size_labels_are_in_price_and_bars() {
        let mut drawing = Drawing::new(1, DrawingToolType::Rect);
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(20.0, 110.0)];
        assert_eq!(drawing.size_labels(), ["20 bars", "10.00 (10.00%)"]);
    }

    /// In a 500 x 400 pane, info lines near its top and bottom edges and steep ones in either
    /// direction keep their text clear of the measurement box, which stays in the pane, taking
    /// the other side of the line where only that side has room. A line wholly above the pane
    /// paints nothing in it, near or far and with or without text.
    #[test]
    fn info_line_box_stays_clear_of_its_text_and_out_of_view_with_its_line() {
        use egui::{Shape, emath::Rot2};
        const LABEL: &str = "Weekly support";
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500., 400.));
        let bars = sample_bars();
        let paint = |a: (f32, f32), b: (f32, f32), text: Option<&str>| {
            let mut drawing = Drawing::new(1, DrawingToolType::InfoLine);
            drawing.text = text.map(Into::into);
            drawing.chart_points = [a, b]
                .map(|(x, y)| ChartPoint::new(x, f64::from(y)))
                .to_vec();
            drawing.update_screen_coords(|b| b, |p| p as f32);
            let output = egui::Context::default().run_ui(Default::default(), |ui| {
                let view = ChartView {
                    price_formatter: None,
                    instrument: None,
                    bars: &bars,
                    executions: None,
                    revision: 0,
                    bar_to_x: &|b| b,
                    price_to_y: &|p| p as f32,
                    y_to_price: &f64::from,
                    time_axis: true,
                };
                drawing.render(ui.painter(), pane, &view);
            });
            fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
                    _ => out.push(shape.clone()),
                }
            }
            let mut shapes = Vec::new();
            for s in &output.shapes {
                flatten(&s.shape, &mut shapes);
            }
            output.drop_without_applying_deltas();
            shapes
        };
        for (a, b) in [
            ((100., 40.), (300., 20.)),
            ((100., 360.), (300., 380.)),
            ((200., 300.), (200., 100.)),
            ((200., 100.), (200., 300.)),
            ((100., 300.), (300., 100.)),
            ((100., 100.), (300., 300.)),
        ] {
            let shapes = paint(a, b, Some(LABEL));
            let boxes: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Rect(r) if r.fill.a() > 0 => Some(r.rect),
                    _ => None,
                })
                .collect();
            let [measurement] = boxes[..] else {
                panic!("{a:?} {b:?}: {} boxes", boxes.len());
            };
            assert!(
                pane.contains_rect(measurement),
                "{a:?} {b:?}: {measurement:?}"
            );
            let Some(text) = shapes.iter().find_map(|s| match s {
                Shape::Text(t) if t.galley.text() == LABEL => Some(t),
                _ => None,
            }) else {
                panic!("{a:?} {b:?}: no text");
            };
            let glyphs = text.galley.rect;
            for i in 0..=20 {
                for j in 0..=4 {
                    let point =
                        glyphs.min + glyphs.size() * egui::vec2(i as f32 / 20., j as f32 / 4.);
                    let point = text.pos + Rot2::from_angle(text.angle) * point.to_vec2();
                    assert!(
                        !measurement.contains(point),
                        "{a:?} {b:?}: {point:?} under the box"
                    );
                }
            }
        }
        for (offset, text) in [(350., None), (650., Some(LABEL))] {
            for shape in paint((200., -50. - offset), (300., -offset), text) {
                assert!(
                    !shape.visual_bounding_rect().intersects(pane),
                    "{offset} above: {shape:?}"
                );
            }
        }
    }
}
