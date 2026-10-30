//! Long/short position tool rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::{Bar, percent_change};
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders a long or short position: entry, target and stop zones, and the
    /// result of replaying the bars after the entry bar up to the right edge.
    pub(crate) fn render_pos_tool(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        is_long: bool,
        selected: bool,
    ) {
        let (&[entry, target, ..], &[entry_at, target_at, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            if let Some(&point) = self.points.first() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.md, self.color32());
            }
            return;
        };

        let theme = painter.ctx().active_theme();
        let [profit_color, loss_color] = self.position_colors();
        // Opaque, deeper label fills keep white text legible on a light chart too.
        let label_color = |color: Color32| {
            color.lerp_to_gamma(Color32::BLACK, if theme.dark { 0.08 } else { 0.18 })
        };
        let entry_color = theme.chart.axis_text_secondary;
        let zone_alpha = if theme.dark { 0.20 } else { 0.12 };
        let covered_alpha = if theme.dark { 0.14 } else { 0.10 };
        let line_width = 1.0 / painter.pixels_per_point();

        let (entry_price, target_price) = (entry_at.price, target_at.price);
        // The stop anchor is added when the tool is placed; until then (while
        // it is dragged out) the stop mirrors the target around the entry.
        let stop_price = self
            .chart_points
            .get(2)
            .map_or(2.0 * entry_price - target_price, |stop| stop.price);
        let stop_y = (view.price_to_y)(stop_price);

        // Returns are signed in the position's direction, so a level on the
        // losing side of the entry reads as negative.
        let side = if is_long { 1.0 } else { -1.0 };
        let pct = |price: f64| side * percent_change(entry_price, price);

        // The tool spans from the entry bar to the right edge.
        let entry_bar = entry_at.bar_idx.round();
        let span = (target_at.bar_idx - entry_at.bar_idx).abs().round();
        let (first_after, after_entry) = if span >= 1.0 {
            bars_between(view.bars, entry_bar + 1.0, entry_bar + span).unwrap_or((0, &[]))
        } else {
            (0, &[][..])
        };
        let exit = position_exit(after_entry, entry_price, target_price, stop_price, is_long);
        let qty = self.position_size.unwrap_or_default().quantity_with_sizing(
            entry_price,
            stop_price,
            &self.position_sizing,
        );
        let qty_text = qty.map_or_else(
            || "n/a".to_owned(),
            |qty| {
                if self.position_sizing.quantity_step > 0.0 {
                    let step = self.position_sizing.quantity_step.to_string();
                    let decimals = step
                        .split_once('.')
                        .map_or(0, |(_, fraction)| fraction.len());
                    return if decimals <= 19 && qty < 1e15 {
                        format!("{qty:.decimals$}")
                    } else {
                        format!("{qty:e}")
                    };
                }
                // Four decimals, or four significant digits below 0.1, where that
                // rounding is exact; an exponent beyond it.
                let scale = 10f64.powi(3 - qty.log10().floor() as i32).max(1e4);
                if scale <= 1e22 && qty < 1e15 {
                    ((qty * scale).round() / scale).to_string()
                } else {
                    format!("{qty:e}")
                }
            },
        );
        // Money for a price move of `points`, at the position's quantity.
        let money = |points: f64| {
            qty.map(|qty| {
                points * qty * self.position_sizing.point_value * self.position_sizing.lot_size
            })
            .filter(|amount| amount.is_finite())
        };

        let right = Self::position_right(entry, target);

        self.draw_zone(
            painter,
            entry,
            target.y,
            right,
            profit_color.gamma_multiply(zone_alpha),
        );
        self.draw_zone(
            painter,
            entry,
            stop_y,
            right,
            loss_color.gamma_multiply(zone_alpha),
        );

        // The trade's path from the entry to its exit, over the part of the zone it covered.
        if let Some((exit_price, _, exit_bar)) = exit {
            let exit_x = exit_bar.map_or(entry.x, |bar| {
                (view.bar_to_x)((first_after + bar) as f32).clamp(entry.x, right)
            });
            let exit_at = Pos2::new(exit_x, (view.price_to_y)(exit_price));
            let won = side * (exit_price - entry_price) >= 0.0;
            self.draw_zone(
                painter,
                entry,
                exit_at.y,
                exit_x,
                (if won { profit_color } else { loss_color }).gamma_multiply(covered_alpha),
            );
            draw_dashed_line(
                painter,
                entry,
                exit_at,
                Stroke::new(line_width, entry_color.gamma_multiply(0.45)),
                4.0,
                5.0,
            );
        }

        painter.hline(
            entry.x..=right,
            entry.y,
            Stroke::new(line_width, entry_color.gamma_multiply(0.65)),
        );

        let qty_part = format!("Qty: {qty_text}");
        let (summary, summary_fill, summary_text) = match exit {
            Some((exit_price, closed, _)) => {
                let change = side * (exit_price - entry_price);
                let pnl = money(change).map_or_else(|| "n/a".to_owned(), |pnl| format!("{pnl:.2}"));
                let risk_reward = (target_price - entry_price) / (entry_price - stop_price);
                let summary = format!(
                    "{} P&L: {pnl} ({:.2}%), {qty_part}\nRisk/Reward: {risk_reward:.2}",
                    if closed { "Closed" } else { "Open" },
                    pct(exit_price),
                );
                let fill = if change >= 0.0 {
                    profit_color
                } else {
                    loss_color
                };
                (summary, label_color(fill), Color32::WHITE)
            }
            None => (
                format!("P&L: n/a, {qty_part}\nRisk/Reward: n/a"),
                theme.chart.bg_axis,
                entry_color,
            ),
        };
        let capital = qty
            .and_then(|qty| self.position_sizing.required_capital(entry_price, qty))
            .map_or_else(|| "n/a".to_owned(), |capital| format!("{capital:.2}"));
        let summary = format!("{summary}, Capital: {capital}");
        let amount = |price: f64| {
            money((price - entry_price).abs())
                .map_or_else(String::new, |amount| format!(", Amount: {amount:.2}"))
        };
        let target_label = format!(
            "Target: {target_price:.2} ({:.2}%){}",
            pct(target_price),
            amount(target_price)
        );
        let stop_label = format!(
            "Stop: {stop_price:.2} ({:.2}%){}",
            pct(stop_price),
            amount(stop_price)
        );

        // Labels sit on the box's centre line: the stats beside the entry on the stop
        // side, the target and stop levels outside their zones, beyond the stats.
        let center_x = (entry.x + right) / 2.0;
        let stop_below = stop_y > entry.y;
        let stats = draw_pill(
            painter,
            summary,
            summary_fill,
            summary_text,
            center_x,
            entry.y,
            stop_below,
        );
        if selected && let Some(stats) = stats {
            painter.rect_stroke(
                stats,
                3.0,
                Stroke::new(stroke::HAIRLINE, Color32::WHITE.gamma_multiply(0.8)),
                egui::StrokeKind::Inside,
            );
        }
        let stop_edge = match stats {
            Some(stats) if stop_below => stop_y.max(stats.bottom()),
            Some(stats) => stop_y.min(stats.top()),
            None => stop_y,
        };
        draw_pill(
            painter,
            target_label,
            label_color(profit_color),
            Color32::WHITE,
            center_x,
            target.y,
            target.y > entry.y,
        );
        draw_pill(
            painter,
            stop_label,
            label_color(loss_color),
            Color32::WHITE,
            center_x,
            stop_edge,
            stop_below,
        );
    }

    /// Target and stop colours, preserving the presentation of saved drawings without overrides.
    pub(crate) fn position_colors(&self) -> [Color32; 2] {
        [
            (
                self.position_target_color,
                DESIGN_TOKENS.semantic.extended.bullish,
            ),
            (
                self.position_stop_color,
                DESIGN_TOKENS.semantic.extended.bearish,
            ),
        ]
        .map(|(saved, default)| {
            saved.map_or_else(
                || default.lerp_to_gamma(Color32::from_gray(128), 0.35),
                |[r, g, b, a]| Color32::from_rgba_unmultiplied(r, g, b, a),
            )
        })
    }

    /// Entry, target and stop for the price axis, including the mirrored stop of older drawings.
    pub(crate) fn position_price_tags(&self, entry_color: Color32) -> Vec<(f64, Color32)> {
        if !matches!(
            self.tool_type,
            crate::drawings::DrawingToolType::LongPos | crate::drawings::DrawingToolType::ShortPos
        ) {
            return Vec::new();
        }
        let [entry, target, rest @ ..] = self.chart_points.as_slice() else {
            return Vec::new();
        };
        let [profit, loss] = self.position_colors();
        [
            (entry.price, entry_color),
            (target.price, profit),
            (
                rest.first()
                    .map_or(2.0 * entry.price - target.price, |stop| stop.price),
                loss,
            ),
        ]
        .into_iter()
        .filter(|(price, _)| price.is_finite())
        .collect()
    }

    /// Screen x of the right edge of a position's box: as far right of the entry as the target
    /// anchor is, and at least the minimum width.
    pub(crate) fn position_right(entry: Pos2, target: Pos2) -> f32 {
        entry.x
            + (target.x - entry.x)
                .abs()
                .max(DESIGN_TOKENS.sizing.position_tool.min_width)
    }

    /// Fills the band between the entry line and `level_y`, from the entry to `right`.
    fn draw_zone(
        &self,
        painter: &egui::Painter,
        entry: Pos2,
        level_y: f32,
        right: f32,
        color: Color32,
    ) {
        let zone = Rect::from_two_pos(entry, Pos2::new(right, level_y));
        // Visibility check: skip rendering if rect is outside clip region
        if !painter.clip_rect().intersects(zone) {
            return;
        }
        painter.rect_filled(zone, 0.0, color);
    }
}

/// Paints `text`, centred on `center_x`, in a pill filled with `fill` whose near edge sits a
/// small gap beyond `edge_y`: below it when `below`, otherwise above. A pill partly outside the
/// plot is moved inside it; one wholly outside is not painted. Returns the painted pill.
fn draw_pill(
    painter: &egui::Painter,
    text: String,
    fill: Color32,
    text_color: Color32,
    center_x: f32,
    edge_y: f32,
    below: bool,
) -> Option<Rect> {
    let clip = painter.clip_rect();
    if !clip.is_positive() {
        return None;
    }
    let padding = egui::vec2(DESIGN_TOKENS.spacing.sm, DESIGN_TOKENS.spacing.xs);
    let gap = DESIGN_TOKENS.spacing.lg;
    let mut job = egui::text::LayoutJob::simple(
        text,
        egui::FontId::proportional(typography::SM),
        text_color,
        (clip.width() - 2.0 * padding.x).max(1.0),
    );
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    let size = galley.size() + 2.0 * padding;
    let top = if below {
        edge_y + gap
    } else {
        edge_y - gap - size.y
    };
    let pill = Rect::from_min_size(Pos2::new(center_x - size.x / 2.0, top), size);
    if !clip.intersects(pill) {
        return None;
    }
    let pill = pill.translate(egui::vec2(
        (clip.right() - pill.right()).min(0.0) + (clip.left() - pill.left()).max(0.0),
        (clip.bottom() - pill.bottom()).min(0.0) + (clip.top() - pill.top()).max(0.0),
    ));
    painter.rect_filled(pill, 3.0, fill);
    painter.galley(
        Pos2::new(pill.center().x, pill.top() + padding.y),
        galley,
        text_color,
    );
    Some(pill)
}

/// Price the position exits at, whether it has closed, and the index in `bars` of the bar it
/// exits on (`None` when no bar follows the entry). The first bar that
/// trades through the stop or the target closes it; when one bar reaches both,
/// the stop is taken, since the order within a bar is unknown. Otherwise the
/// position is open at the last close (at the entry when no bar follows it).
/// `None` unless the stop and the target lie on the losing and the winning
/// side of the entry: an order on the wrong side of the market fills at the
/// market, not at its level.
fn position_exit(
    bars: &[Bar],
    entry: f64,
    target: f64,
    stop: f64,
    is_long: bool,
) -> Option<(f64, bool, Option<usize>)> {
    let ordered = if is_long {
        stop < entry && entry < target
    } else {
        target < entry && entry < stop
    };
    if !ordered {
        return None;
    }
    for (i, bar) in bars.iter().enumerate() {
        let (stop_hit, target_hit) = if is_long {
            (bar.low <= stop, bar.high >= target)
        } else {
            (bar.high >= stop, bar.low <= target)
        };
        if stop_hit {
            return Some((stop, true, Some(i)));
        }
        if target_hit {
            return Some((target, true, Some(i)));
        }
    }
    let last = bars.iter().rposition(|bar| !bar.is_whitespace());
    Some((last.map_or(entry, |i| bars[i].close), false, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

    /// Risk 230 / (2.5 points * 50 per point * 10 units) = 0.184 lots, rounded
    /// down to 0.18. A five-point gain pays 450; capital is 100 * 50 * 10 * 0.18 / 5 = 1800.
    #[test]
    fn position_labels_use_lot_economics_and_leveraged_capital() {
        use crate::drawings::{ChartPoint, DrawingToolType, PositionSize};
        use crate::widget::test_frames::chart;
        for (tool, target, stop, low, high) in [
            (DrawingToolType::LongPos, 105.0, 97.5, 99.0, 106.0),
            (DrawingToolType::ShortPos, 95.0, 102.5, 94.0, 101.0),
        ] {
            for (size, step, labels) in [
                (
                    PositionSize::RiskAmount(230.0),
                    0.01,
                    ["Qty: 0.18", "Closed P&L: 450.00", "Capital: 1800.00"],
                ),
                (
                    PositionSize::RiskAmount(230.40125),
                    0.000001,
                    ["Qty: 0.184321", "Closed P&L: 460.80", "Capital: 1843.21"],
                ),
                (
                    PositionSize::Quantity(1e20),
                    0.01,
                    ["Qty: 1e20", "Closed P&L:", "Capital:"],
                ),
                (
                    PositionSize::Quantity(1e-25),
                    1e-27,
                    ["Qty: 1e-25", "Closed P&L:", "Capital:"],
                ),
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.position_target_color = Some([40, 100, 220, 255]);
                drawing.position_stop_color = Some([220, 140, 40, 255]);
                drawing.position_sizing = crate::drawings::PositionSizing {
                    point_value: 50.0,
                    lot_size: 10.0,
                    quantity_step: step,
                    leverage: 5.0,
                };
                drawing.position_size = Some(size);
                drawing.chart_points = vec![
                    ChartPoint::new(30.0, 100.0),
                    ChartPoint::new(50.0, target),
                    ChartPoint::new(50.0, stop),
                ];
                drawing.completed = true;
                for (dark, width) in [(false, 800.0), (true, 800.0), (false, 320.0), (true, 320.0)]
                {
                    let ctx = egui::Context::default();
                    let mut chart = chart(&[low], &[high]);
                    chart.set_instrument(Some(
                        crate::model::Instrument::new("TEST").with_price_decimals(5),
                    ));
                    chart.price_scale_mut().set_range(94.0, 106.0);
                    chart.set_theme(
                        &ctx,
                        &if dark {
                            crate::theme::Theme::dark()
                        } else {
                            crate::theme::Theme::light()
                        },
                    );
                    let render = |chart: &mut crate::Chart| {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    egui::vec2(width, 600.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                chart.show(ui);
                            },
                        )
                    };
                    render(&mut chart).drop_without_applying_deltas();
                    chart.set_visible_logical_range(crate::model::LogicalRange {
                        left: 10.0,
                        right: 70.0,
                    });
                    chart.drawings_mut().drawings.push(drawing.clone());
                    let output = render(&mut chart);
                    let plot = chart.get_rendered_price_rect();
                    for price in [100.0, target, stop] {
                        let label = format!("{price:.5}");
                        assert!(
                            output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Text(text) if text.pos.x >= plot.right()
                                && text.galley.text() == label)),
                            "missing axis tag {label}"
                        );
                    }
                    for fill in [
                        Color32::from_rgb(40, 100, 220),
                        Color32::from_rgb(220, 140, 40),
                    ] {
                        let alpha = if dark { 0.20 } else { 0.12 };
                        assert!(
                            output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Rect(rect) if rect.fill == fill.gamma_multiply(alpha))),
                            "configured zone colour {fill:?}"
                        );
                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Rect(rect) if rect.fill == fill && rect.rect.left() >= plot.right())),
                            "configured axis colour {fill:?}");
                    }
                    let summary = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.job.text.contains("Qty:") => Some(t),
                            _ => None,
                        })
                        .expect("position summary");
                    for label in labels {
                        assert!(
                            summary.galley.job.text.contains(label),
                            "{tool:?}: missing {label}: {}",
                            summary.galley.job.text
                        );
                    }
                    assert!(
                        chart
                            .get_rendered_price_rect()
                            .contains_rect(summary.visual_bounding_rect()),
                        "{tool:?}, width {width}: {:?}",
                        summary.visual_bounding_rect()
                    );
                    output.drop_without_applying_deltas();
                    // Hidden by the visibility flag, then by the timeframe filter alone.
                    for (visible, timeframes) in [
                        (false, crate::drawings::TimeframeVisibility::all()),
                        (
                            true,
                            crate::drawings::TimeframeVisibility::specific(&["1h"]),
                        ),
                    ] {
                        let drawing = &mut chart.drawings_mut().drawings[0];
                        drawing.visible = visible;
                        drawing.timeframe_visibility = timeframes;
                        let output = render(&mut chart);
                        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Text(text) if text.pos.x >= chart.get_rendered_price_rect().right()
                                && text.galley.text() == format!("{target:.5}"))),
                            "hidden position cannot contribute axis tags");
                        output.drop_without_applying_deltas();
                    }
                    chart.drawings_mut().drawings[0].timeframe_visibility =
                        crate::drawings::TimeframeVisibility::all();
                    // The tool ends at the plot's right edge: its target label stays inside.
                    chart.set_visible_logical_range(crate::model::LogicalRange {
                        left: 20.0,
                        right: 52.0,
                    });
                    let output = render(&mut chart);
                    let label = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.job.text.starts_with("Target:") => {
                                Some(t)
                            }
                            _ => None,
                        })
                        .expect("target label");
                    assert!(
                        chart
                            .get_rendered_price_rect()
                            .contains_rect(label.visual_bounding_rect()),
                        "{tool:?}, width {width}: {:?}",
                        label.visual_bounding_rect()
                    );
                    output.drop_without_applying_deltas();
                    chart.set_visible_logical_range(crate::model::LogicalRange {
                        left: 90.0,
                        right: 130.0,
                    });
                    let output = render(&mut chart);
                    assert!(
                        !output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.contains("Qty:"))),
                        "{tool:?}, width {width}: offscreen summary"
                    );
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    /// Expected exits read off the sample bars by hand.
    #[test]
    fn position_exit_replays_bars_after_entry() {
        let bars = sample_bars();
        let after_bar_0 = &bars[1..=5];
        let cases = [
            // Long from 11: bar 4 (high 15) reaches 14.2 before any low reaches 9.5.
            (
                after_bar_0,
                11.0,
                14.2,
                9.5,
                true,
                Some((14.2, true, Some(3))),
            ),
            // Long from 11: bar 1 (low 10) reaches the stop at 10.5 first.
            (
                after_bar_0,
                11.0,
                14.2,
                10.5,
                true,
                Some((10.5, true, Some(0))),
            ),
            // Bar 1 (low 10, high 13) reaches both 10.0 and 12.8: the stop is taken.
            (
                after_bar_0,
                11.0,
                12.8,
                10.0,
                true,
                Some((10.0, true, Some(0))),
            ),
            // Neither level is reached: open at the close of bar 5.
            (
                after_bar_0,
                11.0,
                20.0,
                5.0,
                true,
                Some((12.5, false, Some(4))),
            ),
            // Short from 13.5 at bar 3: bar 5 (low 12) reaches 12.2, no high reaches 15.5.
            (
                &bars[4..=5],
                13.5,
                12.2,
                15.5,
                false,
                Some((12.2, true, Some(1))),
            ),
            // No bar after the entry yet.
            (
                &bars[..0],
                13.0,
                14.0,
                12.0,
                true,
                Some((13.0, false, None)),
            ),
            // Long dragged down: target below the entry, stop mirrored above it.
            (&bars[2..=2], 12.0, 11.5, 12.5, true, None),
            // Short dragged up: target above the entry, stop mirrored below it.
            (&bars[2..=2], 12.0, 12.5, 11.5, false, None),
            // Long whose stop was moved above the entry.
            (after_bar_0, 11.0, 14.2, 11.5, true, None),
        ];
        for (after, entry, target, stop, is_long, want) in cases {
            assert_eq!(position_exit(after, entry, target, stop, is_long), want);
        }
    }
}
