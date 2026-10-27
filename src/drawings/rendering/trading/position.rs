//! Long/short position tool rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::{Bar, percent_change};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders a long or short position: entry, target and stop zones, and the
    /// result of replaying the bars after the entry bar up to the right edge.
    pub(crate) fn render_pos_tool(&self, painter: &egui::Painter, view: &ChartView, is_long: bool) {
        let (&[entry, target, ..], &[entry_at, target_at, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            if let Some(&point) = self.points.first() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.md, self.color32());
            }
            return;
        };

        // Position colors
        let profit_color = DESIGN_TOKENS.semantic.extended.bullish;
        let loss_color = DESIGN_TOKENS.semantic.extended.bearish;
        let entry_color = DESIGN_TOKENS.semantic.extended.chart_text_secondary;

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
        let after_entry = if span >= 1.0 {
            bars_between(view.bars, entry_bar + 1.0, entry_bar + span).map_or(&[][..], |(_, b)| b)
        } else {
            &[]
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

        // Calculate width - extend across chart
        let width = (target.x - entry.x)
            .abs()
            .max(DESIGN_TOKENS.sizing.position_tool.min_width);
        let right = entry.x + width;

        // Draw filled zones
        self.draw_zone(painter, entry, target.y, right, profit_color);
        self.draw_zone(painter, entry, stop_y, right, loss_color);

        // Draw horizontal lines (thicker)
        self.draw_position_lines(
            painter,
            entry,
            target.y,
            stop_y,
            right,
            entry_color,
            profit_color,
            loss_color,
        );

        // Draw handle circles at endpoints
        self.draw_position_handles(
            painter,
            entry,
            target.y,
            stop_y,
            right,
            entry_color,
            profit_color,
            loss_color,
        );

        // Draw labels
        let (summary, summary_color) = match exit {
            Some((exit_price, closed)) => {
                let change = side * (exit_price - entry_price);
                let pnl = qty
                    .map(|qty| {
                        change
                            * qty
                            * self.position_sizing.point_value
                            * self.position_sizing.lot_size
                    })
                    .filter(|pnl| pnl.is_finite())
                    .map_or_else(|| "n/a".to_owned(), |pnl| format!("{pnl:.2}"));
                let risk_reward = (target_price - entry_price) / (entry_price - stop_price);
                let summary = format!(
                    "{} P&L: {pnl} ({:.2}%)  •  Risk/Reward: {risk_reward:.2}  •  Qty: {qty_text}",
                    if closed { "Closed" } else { "Open" },
                    pct(exit_price),
                );
                let color = if change >= 0.0 {
                    profit_color
                } else {
                    loss_color
                };
                (summary, color)
            }
            None => (
                format!("P&L: n/a  •  Risk/Reward: n/a  •  Qty: {qty_text}"),
                entry_color,
            ),
        };
        let capital = qty
            .and_then(|qty| self.position_sizing.required_capital(entry_price, qty))
            .map_or_else(|| "n/a".to_owned(), |capital| format!("{capital:.2}"));
        let summary = format!("{summary}  •  Capital: {capital}");
        let target_label = format!("Target: {target_price:.2} ({:.2}%)", pct(target_price));
        let stop_label = format!("Stop: {stop_price:.2} ({:.2}%)", pct(stop_price));
        self.draw_position_labels(
            painter,
            entry,
            target.y,
            stop_y,
            right,
            summary,
            summary_color,
            target_label,
            stop_label,
        );
    }

    /// Fills the band between the entry line and `level_y`.
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
        painter.rect_filled(zone, 0.0, color.gamma_multiply(30_f32 / 255.0));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_position_lines(
        &self,
        painter: &egui::Painter,
        entry: Pos2,
        target_y: f32,
        stop_y: f32,
        right: f32,
        entry_color: Color32,
        profit_color: Color32,
        loss_color: Color32,
    ) {
        // Visibility check: compute bounding rect for all lines
        let min_y = target_y.min(stop_y).min(entry.y);
        let max_y = target_y.max(stop_y).max(entry.y);
        let lines_rect = Rect::from_min_max(Pos2::new(entry.x, min_y), Pos2::new(right, max_y));
        if !painter.clip_rect().intersects(lines_rect) {
            return;
        }

        // Entry line (gray, thicker)
        painter.hline(
            entry.x..=right,
            entry.y,
            Stroke::new(stroke::THICK, entry_color),
        );

        // Target line (green, thicker)
        painter.hline(
            entry.x..=right,
            target_y,
            Stroke::new(stroke::THICK, profit_color),
        );

        // Stop line (red, thicker)
        painter.hline(
            entry.x..=right,
            stop_y,
            Stroke::new(stroke::THICK, loss_color),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_position_handles(
        &self,
        painter: &egui::Painter,
        entry: Pos2,
        target_y: f32,
        stop_y: f32,
        right: f32,
        entry_color: Color32,
        profit_color: Color32,
        loss_color: Color32,
    ) {
        let handle_radius = DESIGN_TOKENS.rounding.lg;

        // Visibility check: compute bounding rect for all handles (including radius)
        let min_y = target_y.min(stop_y).min(entry.y) - handle_radius;
        let max_y = target_y.max(stop_y).max(entry.y) + handle_radius;
        let handles_rect = Rect::from_min_max(
            Pos2::new(entry.x - handle_radius, min_y),
            Pos2::new(right + handle_radius, max_y),
        );
        if !painter.clip_rect().intersects(handles_rect) {
            return;
        }

        // Entry handles
        painter.circle_filled(Pos2::new(entry.x, entry.y), handle_radius, entry_color);
        painter.circle_filled(Pos2::new(right, entry.y), handle_radius, entry_color);

        // Target handles
        painter.circle_filled(Pos2::new(entry.x, target_y), handle_radius, profit_color);
        painter.circle_filled(Pos2::new(right, target_y), handle_radius, profit_color);

        // Stop handles
        painter.circle_filled(Pos2::new(entry.x, stop_y), handle_radius, loss_color);
        painter.circle_filled(Pos2::new(right, stop_y), handle_radius, loss_color);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_position_labels(
        &self,
        painter: &egui::Painter,
        entry: Pos2,
        target_y: f32,
        stop_y: f32,
        right: f32,
        summary: String,
        summary_color: Color32,
        target_label: String,
        stop_label: String,
    ) {
        if !painter.clip_rect().is_positive() {
            return;
        }
        let font_small = egui::FontId::proportional(typography::SM);
        let font_large = egui::FontId::proportional(typography::MD);

        let sizing = &DESIGN_TOKENS.sizing.position_tool;
        let width = sizing.label_info_width.min(painter.clip_rect().width());
        let galley = painter.layout(summary, font_large, summary_color, (width - 10.0).max(1.0));
        let height = (galley.size().y + 10.0).max(sizing.label_info_height);
        let info_rect = Rect::from_min_size(
            Pos2::new(
                entry.x + sizing.label_offset_x,
                target_y - sizing.label_offset_y - height,
            ),
            egui::vec2(width, height),
        );
        if painter.clip_rect().intersects(info_rect) {
            let clip = painter.clip_rect();
            let info_rect = Rect::from_min_size(
                Pos2::new(
                    info_rect.left().clamp(clip.left(), clip.right() - width),
                    info_rect
                        .top()
                        .clamp(clip.top(), (clip.bottom() - height).max(clip.top())),
                ),
                info_rect.size(),
            );
            painter.rect_filled(
                info_rect,
                DESIGN_TOKENS.rounding.md,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9),
            );
            painter.galley(info_rect.min + egui::vec2(5.0, 5.0), galley, summary_color);
        }

        // Target label (on right side of line)
        let target_bg = Rect::from_min_max(
            Pos2::new(
                right + DESIGN_TOKENS.sizing.position_tool.label_offset_x,
                target_y - DESIGN_TOKENS.sizing.position_tool.label_offset_x,
            ),
            Pos2::new(
                right
                    + DESIGN_TOKENS.sizing.position_tool.label_offset_x
                    + DESIGN_TOKENS.sizing.position_tool.label_target_width,
                target_y + DESIGN_TOKENS.sizing.position_tool.label_offset_x,
            ),
        );

        // Visibility check for target label
        if painter.clip_rect().intersects(target_bg) {
            painter.rect_filled(
                target_bg,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .bullish
                    .gamma_multiply(200_f32 / 255.0),
            );
            painter.text(
                Pos2::new(
                    right + DESIGN_TOKENS.sizing.position_tool.text_offset_x,
                    target_y,
                ),
                egui::Align2::LEFT_CENTER,
                target_label,
                font_small.clone(),
                Color32::WHITE,
            );
        }

        // Stop label (on right side of line)
        let stop_bg = Rect::from_min_max(
            Pos2::new(
                right + DESIGN_TOKENS.sizing.position_tool.label_offset_x,
                stop_y - DESIGN_TOKENS.sizing.position_tool.label_offset_x,
            ),
            Pos2::new(
                right
                    + DESIGN_TOKENS.sizing.position_tool.label_offset_x
                    + DESIGN_TOKENS.sizing.position_tool.label_target_width,
                stop_y + DESIGN_TOKENS.sizing.position_tool.label_offset_x,
            ),
        );

        // Visibility check for stop label
        if painter.clip_rect().intersects(stop_bg) {
            painter.rect_filled(
                stop_bg,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .bearish
                    .gamma_multiply(200_f32 / 255.0),
            );
            painter.text(
                Pos2::new(
                    right + DESIGN_TOKENS.sizing.position_tool.text_offset_x,
                    stop_y,
                ),
                egui::Align2::LEFT_CENTER,
                stop_label,
                font_small,
                Color32::WHITE,
            );
        }
    }
}

/// Price the position exits at and whether it has closed. The first bar that
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
) -> Option<(f64, bool)> {
    let ordered = if is_long {
        stop < entry && entry < target
    } else {
        target < entry && entry < stop
    };
    if !ordered {
        return None;
    }
    for bar in bars {
        let (stop_hit, target_hit) = if is_long {
            (bar.low <= stop, bar.high >= target)
        } else {
            (bar.high >= stop, bar.low <= target)
        };
        if stop_hit {
            return Some((stop, true));
        }
        if target_hit {
            return Some((target, true));
        }
    }
    Some((
        crate::model::last_with_value(bars).map_or(entry, |bar| bar.close),
        false,
    ))
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
            (after_bar_0, 11.0, 14.2, 9.5, true, Some((14.2, true))),
            // Long from 11: bar 1 (low 10) reaches the stop at 10.5 first.
            (after_bar_0, 11.0, 14.2, 10.5, true, Some((10.5, true))),
            // Bar 1 (low 10, high 13) reaches both 10.0 and 12.8: the stop is taken.
            (after_bar_0, 11.0, 12.8, 10.0, true, Some((10.0, true))),
            // Neither level is reached: open at the close of bar 5.
            (after_bar_0, 11.0, 20.0, 5.0, true, Some((12.5, false))),
            // Short from 13.5 at bar 3: bar 5 (low 12) reaches 12.2, no high reaches 15.5.
            (&bars[4..=5], 13.5, 12.2, 15.5, false, Some((12.2, true))),
            // No bar after the entry yet.
            (&bars[..0], 13.0, 14.0, 12.0, true, Some((13.0, false))),
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
