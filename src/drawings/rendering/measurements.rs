//! Measurement tool rendering implementations
//!
//! Includes: measure, price range, date range, date and price range, info line, and bars pattern.

use super::{ChartView, bars_between};
use crate::drawings::LineTextVerticalAlign;
use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::model::{Bar, percent_change};
use crate::scales::price_decimals;
use crate::scales::{DefaultPriceFormatter, PriceFormatter, VolumeFormatter};
use crate::styles::typography;
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
            let rect = egui::Rect::from_two_pos(start, end);
            if let Some(fill) = self.fill_color32() {
                painter.rect_filled(rect, 0.0, fill);
            }

            let line_stroke = self.stroke();

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

    /// Readout card beside a trend line, ray, extended line, trend angle or info line, which writes
    /// it in its text settings; the midpoint and price labels are independent of selection and
    /// readout visibility. Its screen midpoint remains on the line on logarithmic scales.
    pub(super) fn render_line_measurements(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        selected: bool,
    ) {
        let &[a, b, ..] = self.points.as_slice() else {
            return;
        };
        if !(a.is_finite() && b.is_finite()) {
            return;
        }
        let settings = self.line_measurements;
        if settings.midpoint && !selected {
            painter.circle_filled(a.lerp(b, 0.5), 3.0, self.color32());
        }
        if settings.price_labels {
            self.render_anchor_prices(painter, a, b, view);
        }
        if (settings.persistent || selected || !self.completed)
            && let Some((galley, align, pos, size)) = self.line_measurement_card(painter, view)
        {
            paint_stats_card(painter, align, pos, galley, size);
        }
    }

    /// The rectangle the readout card covers when `painter` clips to the drawing's pane, or
    /// `None` without a card. Computed on demand for what must stay clear of the card.
    #[cfg(feature = "ui")]
    pub(crate) fn line_measurement_card_rect(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) -> Option<Rect> {
        let (_, align, pos, size) = self.line_measurement_card(painter, view)?;
        Some(card_rect(painter.clip_rect(), align, pos, size))
    }

    /// The readout card's text, the point and alignment that place it, and its size.
    fn line_measurement_card(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) -> Option<(std::sync::Arc<egui::Galley>, Align2, Pos2, egui::Vec2)> {
        use crate::drawings::LineMeasurementPosition as P;
        let &[a, b, ..] = self.points.as_slice() else {
            return None;
        };
        if !(a.is_finite() && b.is_finite()) {
            return None;
        }
        let settings = self.line_measurements;
        let text = self.line_measurement_label(view);
        if text.is_empty() {
            return None;
        }
        let clip = painter.clip_rect();
        if self
            .trend_line_ends(clip)
            .is_none_or(|(a, b)| crate::painting::dashed_line::clip_segment(a, b, clip).is_none())
        {
            return None;
        }
        let galley = if self.tool_type == DrawingToolType::InfoLine {
            let color = self.text_box_colors(painter.ctx()).text;
            self.annotation_galley(painter, text, color)
        } else {
            stats_galley(painter, text)
        };
        let size = galley.size() + CARD_PADDING * 2.0;
        let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
        let gap = DESIGN_TOKENS.spacing.sm + self.stroke_width / 2.0;
        // Side readouts sit away from the line's continuation, including horizontal lines.
        let rising = right.y < left.y;
        let place = |position| match position {
            P::Left => (
                left + egui::vec2(-gap, if rising { -gap } else { gap }),
                if rising {
                    Align2::RIGHT_BOTTOM
                } else {
                    Align2::RIGHT_TOP
                },
            ),
            P::Center | P::Auto => {
                let direction = right - left;
                let normal = if direction.length_sq() == 0.0 {
                    egui::Vec2::UP
                } else if direction.x == 0.0 {
                    egui::Vec2::LEFT
                } else {
                    egui::vec2(direction.y, -direction.x).normalized()
                };
                // The custom text takes this much of each side, up the normal and down it.
                let height = self.text_galley(painter).map_or(0.0, |text| text.size().y);
                let (above, below) = match self.line_measurements.text_vertical_align {
                    _ if height == 0.0 => (0.0, 0.0),
                    LineTextVerticalAlign::Bottom => (gap + height, 0.0),
                    LineTextVerticalAlign::Middle => (height / 2.0, height / 2.0),
                    LineTextVerticalAlign::Top => (0.0, gap + height),
                };
                let at = |(normal, taken): (egui::Vec2, f32)| {
                    a.lerp(b, 0.5) + normal * (gap + taken + normal.abs().dot(size) / 2.0)
                };
                let (free, other) = if below < above {
                    ((-normal, below), (normal, above))
                } else {
                    ((normal, above), (-normal, below))
                };
                // The side with more room for the card, unless only the other one fits it.
                let pos = [free, other]
                    .map(at)
                    .into_iter()
                    .find(|&pos| clip.contains_rect(Rect::from_center_size(pos, size)))
                    .unwrap_or_else(|| at(free));
                (pos, Align2::CENTER_CENTER)
            }
            P::Right => (
                right + egui::vec2(gap, if rising { gap } else { -gap }),
                if rising {
                    Align2::LEFT_TOP
                } else {
                    Align2::LEFT_BOTTOM
                },
            ),
        };
        let position = if settings.position == P::Auto {
            [P::Right, P::Left]
                .into_iter()
                .find(|&side| {
                    let (pos, align) = place(side);
                    clip.contains_rect(align.anchor_size(pos, size))
                })
                .unwrap_or(P::Center)
        } else {
            settings.position
        };
        let (pos, align) = place(position);
        Some((galley, align, pos, size))
    }

    /// Paints each anchor's price on a small label on the side away from the other anchor.
    fn render_anchor_prices(&self, painter: &egui::Painter, a: Pos2, b: Pos2, view: &ChartView) {
        let [first, second, ..] = self.chart_points.as_slice() else {
            return;
        };
        let chart = painter.ctx().active_theme().chart;
        let gap = DESIGN_TOKENS.spacing.sm + self.stroke_width / 2.0;
        for (point, other, price) in [(a, b, first.price), (b, a, second.price)] {
            let Some(text) = super::annotations::price_text(price, view) else {
                continue;
            };
            let (align, away) = if point.x < other.x {
                (Align2::RIGHT_CENTER, egui::vec2(-gap, 0.0))
            } else if point.x > other.x {
                (Align2::LEFT_CENTER, egui::vec2(gap, 0.0))
            } else if point.y <= other.y {
                (Align2::CENTER_BOTTOM, egui::vec2(0.0, -gap))
            } else {
                (Align2::CENTER_TOP, egui::vec2(0.0, gap))
            };
            boxed_label(
                painter,
                align,
                point + away,
                text,
                chart.axis_text,
                chart.bg_tooltip,
            );
        }
    }

    /// Readouts use raw prices and timestamps. Only inclination and length depend on the
    /// displayed geometry; length is measured in horizontal bar spacings, not price units.
    /// Up to three lines, each present only with a readout: the price change and its percentage
    /// and pips, then the bars, elapsed time and length, then the inclination.
    fn line_measurement_label(&self, view: &ChartView) -> String {
        let ([a, b, ..], [start, end, ..]) = (self.chart_points.as_slice(), self.points.as_slice())
        else {
            return String::new();
        };
        let settings = self.line_measurements;
        let change = b.price - a.price;
        let price = (settings.price_change && change.is_finite())
            .then(|| change_text(a.price, change, view.instrument, view.price_formatter));
        let percent = percent_change(a.price, b.price);
        let percent = (settings.percent_change && a.price != 0.0 && percent.is_finite())
            .then(|| format!("{percent:+.2}%"));
        let mut prices = Vec::new();
        prices.extend(match (price, percent) {
            (Some(price), Some(percent)) => Some(format!("{price} ({percent})")),
            (price, percent) => price.or(percent),
        });
        if settings.pip_change {
            prices.extend(unit_change_text(change, view.instrument, false));
        }
        let mut spans = Vec::new();
        if settings.bar_count {
            spans.push(format!(
                "{} bars",
                (b.bar_idx.round() - a.bar_idx.round()).abs() as i64
            ));
        }
        if settings.elapsed_time && view.time_axis {
            let exact_time = |point: &crate::drawings::ChartPoint| {
                point
                    .time_anchor
                    .filter(|(_, index)| *index == point.bar_idx)
                    .map(|(time, _)| time.timestamp_millis())
            };
            let elapsed = exact_time(a)
                .zip(exact_time(b))
                .map(|(a, b)| b.saturating_sub(a))
                .or_else(|| elapsed_ms(view.bars, a.bar_idx, b.bar_idx));
            if let Some(elapsed) = elapsed {
                spans.push(format_duration(elapsed));
            }
        }
        let delta = *end - *start;
        if settings.length {
            let spacing = ((view.bar_to_x)(a.bar_idx + 1.0) - (view.bar_to_x)(a.bar_idx)).abs();
            let length = delta.length() / spacing;
            if spacing > 0.0 && length.is_finite() {
                spans.push(format!("{length:.2} periods"));
            }
        }
        let mut lines = vec![prices.join("  "), spans.join(", ")];
        if settings.inclination && delta.is_finite() {
            // Use the trend angle's own reading, including a level line's zero height.
            let angle = (start.y - end.y).atan2(end.x - start.x).to_degrees();
            lines.push(format!("{:.1}°", angle + 0.0));
        }
        lines.retain(|line| !line.is_empty());
        lines.join("\n")
    }

    // Note: render_bars_pattern is now in trading.rs with full projection features

    /// Label of the measuring tools: the price change from the first anchor to the second in
    /// `price_formatter`'s text, else the instrument's price decimals (by magnitude without them),
    /// its percentage and its whole number of pips, or else ticks, the bars between them and,
    /// on a `time_axis`, the time they span, and the volume traded over them.
    /// Volume drops trailing zeros. Each anchor counts at its nearest bar: anchors N bars apart
    /// span N bars of time and the volume of the N + 1 bars from one to the other, both included.
    pub(crate) fn measurement_label(
        &self,
        bars: &[Bar],
        instrument: Option<&crate::model::Instrument>,
        time_axis: bool,
        price_formatter: Option<&dyn PriceFormatter>,
    ) -> Option<String> {
        let [a, b, ..] = &self.chart_points[..] else {
            return None;
        };
        let change = b.price - a.price;
        let pct = percent_change(a.price, b.price);
        let mut change = format!(
            "{} ({pct:+.2}%)",
            change_text(a.price, change, instrument, price_formatter)
        );
        if let Some(text) = unit_change_text(b.price - a.price, instrument, true) {
            change.push_str(&format!("  {text}"));
        }
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
                let text = VolumeFormatter::default().format(volume);
                let digits = text.trim_end_matches(char::is_alphabetic);
                let unit = &text[digits.len()..];
                // Precision 2 writes a point, so trimming zeros never eats an integer digit.
                let digits = digits.trim_end_matches('0').trim_end_matches('.');
                format!("\nVol {digits}{unit}")
            })
            .unwrap_or_default();
        Some(match self.tool_type {
            DrawingToolType::Measure | DrawingToolType::DateAndPriceRange => {
                format!("{change}\n{span}{volume}")
            }
            DrawingToolType::DateRange => format!("{span}{volume}"),
            _ => return None,
        })
    }

    /// The rectangle's width in bars and height in price, from its stored anchors.
    pub(crate) fn size_labels(&self, price_formatter: Option<&dyn PriceFormatter>) -> Vec<String> {
        let gap = |a: f64, b: f64| {
            let (low, gap) = (a.min(b), (a - b).abs());
            let pct = percent_change(low, low + gap);
            match price_formatter {
                Some(formatter) => format!("{} ({pct:.2}%)", formatter.format(gap)),
                None => {
                    let decimals = price_decimals(low);
                    format!("{gap:.decimals$} ({pct:.2}%)")
                }
            }
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

/// Signed price change, retaining the host formatter's own sign and precision.
fn change_text(
    price: f64,
    change: f64,
    instrument: Option<&crate::model::Instrument>,
    formatter: Option<&dyn PriceFormatter>,
) -> String {
    let built_in =
        DefaultPriceFormatter::with_precision(change_decimals(instrument, price, change));
    let text = formatter.unwrap_or(&built_in).format(change);
    let sign = if text.starts_with(['-', '+']) || (formatter.is_some() && change < 0.0) {
        ""
    } else {
        "+"
    };
    format!("{sign}{text}")
}

/// Pips when supplied by the instrument, otherwise ticks; omit unavailable or inexact counts.
fn unit_change_text(
    change: f64,
    instrument: Option<&crate::model::Instrument>,
    whole: bool,
) -> Option<String> {
    let instrument = instrument?;
    let (size, unit) = [(instrument.pip_size, "pip"), (instrument.tick_size, "tick")]
        .into_iter()
        .find_map(|(size, unit)| Some((size.filter(|s| s.is_finite() && *s > 0.0)?, unit)))?;
    let precision = if whole || unit == "tick" {
        0
    } else {
        instrument
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .map_or(6, |tick| crate::scales::decimals(tick / size).min(16))
    };
    let count = change / size;
    if count.abs() > 2f64.powi(53) || !count.is_finite() {
        return None;
    }
    let rounded = if precision == 0 {
        count.round() + 0.0
    } else {
        count + 0.0
    };
    let text = format!("{rounded:+.precision$}");
    let text = if precision > 0 {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    let text = if text == "-0" { "+0" } else { text };
    let plural = if matches!(text, "+1" | "-1") { "" } else { "s" };
    Some(format!("{text} {unit}{plural}"))
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
            self.measurement_label(
                view.bars,
                view.instrument,
                view.time_axis,
                view.price_formatter,
            ),
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

/// Padding between a stats card's border and its text.
const CARD_PADDING: egui::Vec2 = egui::vec2(8.0, 6.0);

/// Paints `text` on a card in the theme's tooltip colours, placed at `pos` by `align` and
/// moved inside the painter's clip rect.
fn stats_card(painter: &egui::Painter, align: Align2, pos: Pos2, text: String) -> Rect {
    let galley = stats_galley(painter, text);
    let size = galley.size() + CARD_PADDING * 2.0;
    paint_stats_card(painter, align, pos, galley, size)
}

fn stats_galley(painter: &egui::Painter, text: String) -> std::sync::Arc<egui::Galley> {
    painter.layout_no_wrap(
        text,
        egui::FontId::proportional(typography::SM_MD),
        painter.ctx().active_theme().chart.axis_text,
    )
}

fn paint_stats_card(
    painter: &egui::Painter,
    align: Align2,
    pos: Pos2,
    galley: std::sync::Arc<egui::Galley>,
    size: egui::Vec2,
) -> Rect {
    let theme = painter.ctx().active_theme();
    let chart = theme.chart;
    let card = card_rect(painter.clip_rect(), align, pos, size);
    // The border keeps the white light-theme card visible on the white chart.
    painter.rect(
        card,
        DESIGN_TOKENS.rounding.md,
        chart.bg_tooltip,
        Stroke::new(1.0, theme.ui.border),
        egui::StrokeKind::Inside,
    );
    painter.galley(card.min + CARD_PADDING, galley, chart.axis_text);
    card
}

/// A card of `size` placed at `pos` by `align` and moved inside `clip`.
fn card_rect(clip: Rect, align: Align2, pos: Pos2, size: egui::Vec2) -> Rect {
    let min = align
        .anchor_size(pos, size)
        .min
        .clamp(clip.min, clip.max - size.min(clip.size()));
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
                "-5.00 (-5.00%)\n3 bars, 3h\nVol 900",
            ),
            (
                DrawingToolType::DateRange,
                (2.0, 100.0),
                (5.0, 100.0),
                "3 bars, 3h\nVol 1.1K",
            ),
            (
                DrawingToolType::DateRange,
                (0.0, 100.0),
                (0.0, 100.0),
                "0 bars, 0s\nVol 100",
            ),
            // Fractional anchors count at their nearest bars, 1 and 4.
            (
                DrawingToolType::DateRange,
                (1.4, 100.0),
                (3.6, 100.0),
                "3 bars, 3h\nVol 900",
            ),
            (
                DrawingToolType::DateAndPriceRange,
                (5.0, 200.0),
                (2.0, 210.0),
                "+10.00 (+5.00%)\n3 bars, 3h\nVol 1.1K",
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, (b1, p1), (b2, p2), want) in cases {
            let mut drawing = Drawing::new(1, tool);
            drawing.chart_points = vec![ChartPoint::new(b1, p1), ChartPoint::new(b2, p2)];
            drawing.update_screen_coords(|bar| bar * 10.0, |price| 500.0 - price as f32);
            let got = drawing.measurement_label(&bars, None, true, None);
            if got.as_deref() != Some(want) {
                wrong.push(format!("{tool:?}: got {got:?}, want {want:?}"));
            }
        }
        let mut drawing = Drawing::new(0, DrawingToolType::Measure);
        drawing.chart_points = vec![ChartPoint::new(1.0, 1.1000), ChartPoint::new(4.0, 1.0995)];
        // On a horizontal scale other than time the bar times are keys, not times.
        assert_eq!(
            drawing
                .measurement_label(&bars, None, false, None)
                .as_deref(),
            Some("-0.0005 (-0.05%)\n3 bars\nVol 900")
        );
        for (tick, pip, precision, want) in [
            (
                Some(0.00001),
                Some(0.0001),
                None,
                "-0.00050 (-0.05%)  -5 pips",
            ),
            (Some(0.00001), None, None, "-0.00050 (-0.05%)  -50 ticks"),
            (
                Some(0.00001),
                Some(f64::NAN),
                None,
                "-0.00050 (-0.05%)  -50 ticks",
            ),
            (Some(0.0005), None, None, "-0.0005 (-0.05%)  -1 tick"),
            (Some(25.0), None, None, "+0 (-0.05%)  +0 ticks"),
            (
                Some(1e-10),
                None,
                None,
                "-0.0005000000 (-0.05%)  -5000000 ticks",
            ),
            (Some(0.0), Some(f64::NAN), None, "-0.0005 (-0.05%)"),
            (Some(-1.0), Some(f64::INFINITY), None, "-0.0005 (-0.05%)"),
            (None, None, None, "-0.0005 (-0.05%)"),
            (
                Some(0.00001),
                None,
                Some(6),
                "-0.000500 (-0.05%)  -50 ticks",
            ),
        ] {
            let mut instrument = crate::model::Instrument::new("PAIR");
            instrument.tick_size = tick;
            instrument.pip_size = pip;
            instrument.price_decimals = precision;
            let label = drawing
                .measurement_label(&bars, Some(&instrument), true, None)
                .unwrap();
            assert_eq!(
                label.lines().next(),
                Some(want),
                "{tick:?}, {pip:?}, {precision:?}"
            );
        }
        // Counts beyond the exactly representable integers, including overflow, are omitted.
        let instrument = crate::model::Instrument::new("PAIR").with_tick_size(1e-300);
        let label = drawing
            .measurement_label(&bars, Some(&instrument), true, None)
            .unwrap();
        assert!(
            !label.contains("ticks") && !label.contains("pips"),
            "{label}"
        );
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// A 30-by-40 screen segment is 50 points, or five 10-point periods, at atan(4/3).
    /// Raw 100 to 200 is +100 (+100%), 400 quarter-point ticks or 200 half-point pips.
    /// A logarithmic midpoint is sqrt(100*200), not the arithmetic price midpoint.
    #[test]
    fn line_tool_readouts_follow_units_selection_and_scale() {
        use crate::drawings::{LineMeasurementPosition as P, LineMeasurements};
        let mut bars = sample_bars();
        for (i, bar) in bars.iter_mut().enumerate() {
            bar.time = chrono::DateTime::from_timestamp(i as i64 * 3600, 0).unwrap();
        }
        let mut instrument = crate::model::Instrument::new("TEST").with_tick_size(0.25);
        let x = |bar| 300.0 + bar * 10.0;
        let y = |price: f64| 300.0 - 40.0 * (price / 100.0).log2() as f32;
        let inverse = |y: f32| 100.0 * 2_f64.powf(f64::from(300.0 - y) / 40.0);
        let mut drawing = Drawing::new(1, DrawingToolType::ExtendedLine);
        drawing.completed = true;
        let options = LineMeasurements {
            price_change: true,
            percent_change: true,
            pip_change: true,
            bar_count: true,
            elapsed_time: true,
            length: true,
            inclination: true,
            midpoint: true,
            ..Default::default()
        };
        let cases = [
            (
                false,
                None,
                true,
                "+100.00 (+100.00%)  +400 ticks\n3 bars, 3h, 5.00 periods\n53.1°",
            ),
            (
                true,
                Some(0.5),
                true,
                "-100.00 (-50.00%)  -200 pips\n3 bars, 3h, 5.00 periods\n-126.9°",
            ),
            (
                false,
                Some(0.5),
                false,
                "+100.00 (+100.00%)  +200 pips\n3 bars, 5.00 periods\n53.1°",
            ),
        ];
        let tools = [
            DrawingToolType::TrendLine,
            DrawingToolType::ExtendedLine,
            DrawingToolType::Ray,
            DrawingToolType::InfoLine,
            DrawingToolType::TrendAngle,
        ];
        for ((reverse, pip, time_axis, want), tool) in cases
            .into_iter()
            .flat_map(|case| tools.map(|tool| (case, tool)))
        {
            drawing.tool_type = tool;
            instrument.pip_size = pip;
            let view = ChartView {
                instrument: Some(&instrument),
                price_formatter: None,
                time_formatter: None,
                bar_seconds: 3600,
                bars: &bars,
                revision: 0,
                bar_to_x: &x,
                price_to_y: &y,
                y_to_price: &inverse,
                time_axis,
                executions: None,
                timeframe: None,
                finer: &[],
            };
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(3.0, 200.0)];
            if reverse {
                drawing.chart_points.reverse();
            }
            drawing.update_screen_coords(x, y);
            assert!(drawing.line_measurement_label(&view).is_empty());
            drawing.line_measurements = options;
            assert_eq!(drawing.line_measurement_label(&view), want);
            // Each switch independently removes exactly its own readout.
            for (index, label) in want
                .split(['\n', ','])
                .flat_map(|line| line.split("  "))
                .flat_map(|part| part.split(" ("))
                .map(|readout| readout.trim().trim_end_matches(')'))
                .enumerate()
            {
                let mut only = LineMeasurements::default();
                match label {
                    text if text.ends_with('%') => only.percent_change = true,
                    text if text.ends_with("ticks") || text.ends_with("pips") => {
                        only.pip_change = true
                    }
                    text if text.ends_with("bars") => only.bar_count = true,
                    text if text.ends_with('h') => only.elapsed_time = true,
                    text if text.ends_with("periods") => only.length = true,
                    text if text.ends_with('°') => only.inclination = true,
                    _ => only.price_change = true,
                }
                drawing.line_measurements = only;
                assert_eq!(
                    drawing.line_measurement_label(&view),
                    label,
                    "readout {index}"
                );
            }
            let midpoint = drawing.points[0].lerp(drawing.points[1], 0.5);
            assert!((inverse(midpoint.y) - 141.4213562373095).abs() < 1e-9);
            for (selected, persistent, visible) in [
                (false, false, false),
                (true, false, true),
                (false, true, true),
            ] {
                let mut previous_position = None;
                for position in [P::Left, P::Center, P::Right] {
                    drawing.line_measurements = LineMeasurements {
                        persistent,
                        position,
                        ..options
                    };
                    let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
                        drawing.render_with_selection(
                            ui.painter(),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 400.0)),
                            &view,
                            selected,
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let text = shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == want => Some(text),
                        _ => None,
                    });
                    assert_eq!(text.is_some(), visible, "{selected} {persistent}");
                    if let Some(text) = text {
                        assert_ne!(Some(text.pos), previous_position);
                        previous_position = Some(text.pos);
                        assert!(
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 400.0))
                                .contains_rect(Rect::from_min_size(text.pos, text.galley.size()))
                        );
                        let bounds = Rect::from_min_size(text.pos, text.galley.size());
                        let side = if position == P::Right { 1.0 } else { -1.0 };
                        for corner in [
                            bounds.min,
                            bounds.max,
                            bounds.left_bottom(),
                            bounds.right_top(),
                        ] {
                            assert!((corner - midpoint).dot(egui::vec2(0.8, 0.6)) * side >= 4.9);
                        }
                    }
                    if !selected {
                        assert!(shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Circle(c) if c.center == midpoint)));
                    }
                }
            }
            drawing.text = Some("note".into());
            drawing.line_measurements = LineMeasurements {
                persistent: true,
                position: P::Center,
                ..options
            };
            let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
                drawing.render_with_selection(
                    ui.painter(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 400.0)),
                    &view,
                    false,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let text = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == want => Some(text),
                    _ => None,
                })
                .unwrap();
            let bounds = Rect::from_min_size(text.pos, text.galley.size());
            // The entire readout clears the lower side; the custom label occupies the upper side.
            for corner in [
                bounds.min,
                bounds.max,
                bounds.left_bottom(),
                bounds.right_top(),
            ] {
                assert!((corner - midpoint).dot(egui::vec2(0.8, 0.6)) >= 4.9);
            }
            drawing.text = None;
            drawing.line_measurements = LineMeasurements {
                pip_change: true,
                ..Default::default()
            };
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(3.0, 101.25)];
            assert_eq!(
                drawing.line_measurement_label(&view),
                if pip.is_some() {
                    "+2.5 pips"
                } else {
                    "+5 ticks"
                }
            );
            if time_axis {
                drawing.line_measurements = LineMeasurements {
                    elapsed_time: true,
                    ..Default::default()
                };
                drawing.chart_points = [0.5, 3.75]
                    .map(|index| {
                        let mut point = ChartPoint::new(index, 100.0);
                        point.time_anchor = Some((
                            chrono::DateTime::from_timestamp((index * 3600.0) as i64, 0).unwrap(),
                            index,
                        ));
                        point
                    })
                    .to_vec();
                if reverse {
                    drawing.chart_points.reverse();
                }
                assert_eq!(drawing.line_measurement_label(&view), "3h 15m");
            }
            drawing.line_measurements = Default::default();
        }
    }

    /// Automatic placement takes the first of right, left and centre where the card fits the
    /// 500 x 400 pane; a trend angle also paints its price labels, unselected, away from the line.
    #[test]
    fn automatic_placement_and_trend_angle_price_labels() {
        use crate::drawings::LineMeasurementPosition as P;
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let x = |bar: f32| bar;
        let y = |price: f64| 400.0 - price as f32;
        let inverse = |y: f32| f64::from(400.0 - y);
        let view = ChartView {
            instrument: None,
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            bars: &[],
            revision: 0,
            bar_to_x: &x,
            price_to_y: &y,
            y_to_price: &inverse,
            time_axis: false,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        let texts = |drawing: &Drawing, selected: bool| {
            let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
                drawing.render_with_selection(ui.painter(), pane, &view, selected);
            });
            let texts: Vec<_> = (output.shapes.iter())
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.job.text.clone(),
                        Rect::from_min_size(text.pos, text.galley.size()),
                    )),
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            texts
        };
        let line = |tool, from: (f32, f64), to: (f32, f64)| {
            let mut drawing = Drawing::new(1, tool);
            drawing.completed = true;
            drawing.chart_points =
                vec![ChartPoint::new(from.0, from.1), ChartPoint::new(to.0, to.1)];
            drawing.update_screen_coords(x, y);
            drawing
        };
        for tool in [DrawingToolType::TrendAngle, DrawingToolType::TrendLine] {
            for (from, to, want) in [
                ((100.0, 100.0), (200.0, 150.0), P::Right),
                ((400.0, 100.0), (497.0, 150.0), P::Left),
                ((3.0, 100.0), (497.0, 150.0), P::Center),
            ] {
                let mut drawing = line(tool, from, to);
                drawing.line_measurements.bar_count = true;
                let mut card = |position| {
                    drawing.line_measurements.position = position;
                    (texts(&drawing, true).into_iter())
                        .find(|(text, _)| text.ends_with("bars"))
                        .unwrap()
                        .1
                };
                assert_eq!(card(P::Auto), card(want), "{tool:?} {from:?}");
            }
        }
        let mut angle = line(DrawingToolType::TrendAngle, (100.0, 100.0), (200.0, 150.0));
        angle.line_measurements.price_labels = true;
        let painted = texts(&angle, false);
        let bounds = |price: &str| painted.iter().find(|(text, _)| text == price).unwrap().1;
        assert!(bounds("100.00").right() < 100.0, "{painted:?}");
        assert!(bounds("150.00").left() > 200.0, "{painted:?}");
    }

    #[test]
    fn measurements_receive_the_chart_instrument() {
        use crate::widget::test_frames::{chart, shapes};
        let mut chart = chart(&[90.0], &[110.0]);
        let ctx = egui::Context::default();
        let mut drawing = Drawing::new(1, DrawingToolType::Measure);
        drawing.color = [12, 34, 56, 255];
        drawing.stroke_width = 3.0;
        let expected_stroke = drawing.stroke();
        drawing.chart_points = vec![ChartPoint::new(30.0, 100.0), ChartPoint::new(40.0, 95.0)];
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        chart.set_instrument(Some(
            crate::model::Instrument::new("TEST").with_tick_size(0.25),
        ));
        let output = shapes(&ctx, &mut chart, 0.0, vec![]);
        assert!(output.iter().any(
            |s| matches!(s,egui::Shape::Text(t) if t.galley.job.text.contains("-5.00 (-5.00%)  -20 ticks"))
        ));
        // The chart can cache meshes; inspect styling through the shared renderer fixture.
        let mut drawing = chart.drawings().drawings[0].clone();
        drawing.points = vec![Pos2::new(100.0, 100.0), Pos2::new(300.0, 250.0)];
        let area = Rect::from_two_pos(drawing.points[0], drawing.points[1]);
        for fill in [Some([12, 34, 56, 80]), None] {
            drawing.fill_color = fill;
            let output = super::super::tests::paint(&drawing);
            assert!(output.iter().any(
                |s| matches!(s, egui::Shape::LineSegment { stroke, .. } if *stroke == expected_stroke)
            ));
            assert_eq!(
                output.iter().any(|s| matches!(s,
                    egui::Shape::Rect(r) if r.rect == area && r.fill.a() > 0
                )),
                fill.is_some()
            );
        }
    }

    /// 100 to 110 over bars 0 to 20 is 10 (10%) high and 20 bars wide.
    #[test]
    fn size_labels_are_in_price_and_bars() {
        let mut drawing = Drawing::new(1, DrawingToolType::Rect);
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(20.0, 110.0)];
        assert_eq!(drawing.size_labels(None), ["20 bars", "10.00 (10.00%)"]);
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
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &bars,
                    executions: None,
                    timeframe: None,
                    finer: &[],
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

    /// A new info line shows every readout without being selected, in its own blue 14 px text;
    /// colour, opacity, size and slant reach the card. +10 on 100 is 10%, 40 quarter-point
    /// ticks; bars 0 to 30 are 30 bars and 30 periods of 10 px plus a 10 px rise, atan(10 / 300)
    /// = 1.9 degrees.
    #[test]
    fn info_line_writes_its_readouts_in_its_text_settings() {
        let bars = sample_bars();
        let instrument = crate::model::Instrument::new("TEST").with_tick_size(0.25);
        let card_text = |drawing: &Drawing| {
            let output = egui::Context::default().run_ui(Default::default(), |ui| {
                let view = ChartView {
                    instrument: Some(&instrument),
                    price_formatter: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    bars: &bars,
                    revision: 0,
                    bar_to_x: &|bar| 10.0 * bar,
                    price_to_y: &|price| 300.0 - price as f32,
                    y_to_price: &|y| f64::from(300.0 - y),
                    time_axis: false,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                };
                drawing.render_with_selection(
                    ui.painter(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0)),
                    &view,
                    false,
                );
            });
            let text = output.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.clone()),
                _ => None,
            });
            output.drop_without_applying_deltas();
            text
        };
        let mut drawing = Drawing::new(1, DrawingToolType::InfoLine);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(30.0, 110.0)];
        drawing.update_screen_coords(|bar| 10.0 * bar, |price| 300.0 - price as f32);
        let text = card_text(&drawing).expect("readouts show without selection");
        assert_eq!(
            text.galley.job.text,
            "+10.00 (+10.00%)  +40 ticks\n30 bars, 30.02 periods\n1.9°"
        );
        let format = &text.galley.job.sections[0].format;
        // sqrt(300^2 + 10^2) / 10 = 30.02 periods.
        assert_eq!(format.color, Color32::from_rgb(41, 98, 255));
        assert_eq!(format.font_id.size, 14.0);
        assert!(!format.italics);

        // The line's colour leaves the readouts' colour alone.
        drawing.color = [255, 0, 0, 255];
        drawing.text_color = Some([1, 2, 3, 128]);
        drawing.font_size = 20.0;
        drawing.italic = true;
        let plain = card_text(&drawing).unwrap();
        let format = &plain.galley.job.sections[0].format;
        assert_eq!(
            format.color,
            Color32::from_rgba_unmultiplied(1, 2, 3, 128),
            "opacity"
        );
        assert_eq!(format.font_id.size, 20.0);
        assert!(format.italics);
        drawing.line_measurements.persistent = false;
        assert!(card_text(&drawing).is_none(), "selection only");
    }

    /// On a horizontal line from x = 100 to 300 at y = 200, in either anchor order, the alignment
    /// settings put the text's left, middle or right at x = 100, 200 or 300, and its top below the
    /// line, its middle on it or its bottom above it. Price labels sit outside each anchor.
    #[test]
    fn info_line_text_alignment_and_price_labels_follow_the_settings() {
        use crate::drawings::{LineTextHorizontalAlign as H, LineTextVerticalAlign as V};
        use egui::Shape;
        for points in [[(100., 200.), (300., 200.)], [(300., 200.), (100., 200.)]] {
            let mut drawing = Drawing::new(1, DrawingToolType::InfoLine);
            drawing.completed = true;
            drawing.text = Some("note".into());
            drawing.line_measurements = Default::default();
            drawing.points = points.map(|(x, y)| Pos2::new(x, y)).to_vec();
            drawing.chart_points = points
                .map(|(x, y)| ChartPoint::new(x / 10.0, 500.0 - f64::from(y)))
                .to_vec();
            for (horizontal, want) in [(H::Left, 100.0), (H::Center, 200.0), (H::Right, 300.0)] {
                for vertical in [V::Top, V::Middle, V::Bottom] {
                    drawing.line_measurements.text_horizontal_align = horizontal;
                    drawing.line_measurements.text_vertical_align = vertical;
                    let shapes = super::super::tests::paint(&drawing);
                    let bounds = shapes
                        .iter()
                        .find_map(|s| match s {
                            Shape::Text(t) if t.galley.job.text == "note" => {
                                Some(Rect::from_min_size(t.pos, t.galley.size()))
                            }
                            _ => None,
                        })
                        .expect("text");
                    let got = match horizontal {
                        H::Left => bounds.left(),
                        H::Center => bounds.center().x,
                        H::Right => bounds.right(),
                    };
                    assert!((got - want).abs() < 0.01, "{horizontal:?}: {bounds:?}");
                    match vertical {
                        V::Top => assert!(bounds.top() > 200.0),
                        V::Middle => assert!((bounds.center().y - 200.0).abs() < 0.01),
                        V::Bottom => assert!(bounds.bottom() < 200.0),
                    }
                }
            }
            drawing.text = None;
            drawing.line_measurements.price_labels = true;
            // The line now rises to y = 100 at x = 300, a price of 500 - 100.
            let rising = points.map(|(x, y)| if x == 300. { (x, 100.) } else { (x, y) });
            drawing.points = rising.map(|(x, y)| Pos2::new(x, y)).to_vec();
            drawing.chart_points = rising
                .map(|(x, y)| ChartPoint::new(x / 10.0, 500.0 - f64::from(y)))
                .to_vec();
            let shapes = super::super::tests::paint(&drawing);
            let label = |price: &str| {
                shapes
                    .iter()
                    .find_map(|s| match s {
                        Shape::Text(t) if t.galley.job.text == price => {
                            Some(Rect::from_min_size(t.pos, t.galley.size()))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{price}: {shapes:?}"))
            };
            assert!(label("300.00").right() < 100.0);
            assert!(label("400.00").left() > 300.0);
        }
    }

    /// A trend angle view: one point per bar and per price unit, price 0 at the
    /// bottom of a 500 x 400 pane.
    fn paint_trend_angle(
        first: (f32, f64),
        second: (f32, f64),
        measurements: crate::drawings::LineMeasurements,
    ) -> (Drawing, Vec<egui::Shape>) {
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let x = |bar: f32| bar;
        let y = |price: f64| 400.0 - price as f32;
        let inverse = |y: f32| f64::from(400.0 - y);
        let view = ChartView {
            instrument: None,
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            bars: &[],
            revision: 0,
            bar_to_x: &x,
            price_to_y: &y,
            y_to_price: &inverse,
            time_axis: false,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        let mut angle = Drawing::new(1, DrawingToolType::TrendAngle);
        angle.completed = true;
        angle.chart_points = vec![
            ChartPoint::new(first.0, first.1),
            ChartPoint::new(second.0, second.1),
        ];
        angle.update_screen_coords(x, y);
        angle.line_measurements = measurements;
        let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
            angle.render_with_selection(ui.painter(), pane, &view, false);
        });
        fn flat(shapes: impl IntoIterator<Item = egui::Shape>, out: &mut Vec<egui::Shape>) {
            for shape in shapes {
                match shape {
                    egui::Shape::Vec(inner) => flat(inner, out),
                    shape => out.push(shape),
                }
            }
        }
        let mut shapes = Vec::new();
        flat(
            std::mem::take(&mut output.shapes)
                .into_iter()
                .map(|s| s.shape),
            &mut shapes,
        );
        output.drop_without_applying_deltas();
        (angle, shapes)
    }

    /// A level line from x = 300 to x = 100 points straight left, 180 degrees, as the info line
    /// read before its readouts moved to the line tools' and as a trend angle's own label reads.
    #[test]
    fn level_line_drawn_right_to_left_reads_180_degrees() {
        use crate::drawings::LineMeasurements;
        let bars = sample_bars();
        let view = ChartView {
            instrument: None,
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            bars: &bars,
            revision: 0,
            bar_to_x: &|bar| 10.0 * bar,
            price_to_y: &|price| 300.0 - price as f32,
            y_to_price: &|y| f64::from(300.0 - y),
            time_axis: false,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        for tool in [
            DrawingToolType::TrendLine,
            DrawingToolType::Ray,
            DrawingToolType::ExtendedLine,
            DrawingToolType::InfoLine,
            DrawingToolType::TrendAngle,
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.line_measurements = LineMeasurements {
                inclination: true,
                ..Default::default()
            };
            for (from, to, want) in [(30.0, 10.0, "180.0°"), (10.0, 30.0, "0.0°")] {
                drawing.chart_points =
                    vec![ChartPoint::new(from, 100.0), ChartPoint::new(to, 100.0)];
                drawing.update_screen_coords(|bar| 10.0 * bar, |price| 300.0 - price as f32);
                assert_eq!(drawing.line_measurement_label(&view), want, "{tool:?}");
            }
        }
        let (_, shapes) = paint_trend_angle(
            (300.0, 200.0),
            (100.0, 200.0),
            LineMeasurements {
                persistent: true,
                inclination: true,
                ..Default::default()
            },
        );
        let texts: Vec<_> = shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Text(t) => Some(t.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["180.0°", "180.0°"]);
    }
}
