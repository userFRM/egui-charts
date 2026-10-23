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
        let qty = self
            .position_size
            .unwrap_or_default()
            .quantity(entry_price, stop_price);
        let qty_text = qty.map_or_else(
            || "n/a".to_owned(),
            |qty| {
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
                let pnl =
                    qty.map_or_else(|| "n/a".to_owned(), |qty| format!("{:.2}", change * qty));
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
        let font_small = egui::FontId::proportional(typography::SM);
        let font_large = egui::FontId::proportional(typography::MD);

        // Top info box (above profit zone) - background with rounded corners
        let info_y =
            target_y - DESIGN_TOKENS.sizing.position_tool.label_offset_y - DESIGN_TOKENS.spacing.xl;
        let info_rect = Rect::from_min_max(
            Pos2::new(
                entry.x + DESIGN_TOKENS.sizing.position_tool.label_offset_x,
                info_y,
            ),
            Pos2::new(
                entry.x + DESIGN_TOKENS.sizing.position_tool.label_info_width,
                info_y + DESIGN_TOKENS.sizing.position_tool.label_info_height,
            ),
        );

        // Visibility check for info box
        if painter.clip_rect().intersects(info_rect) {
            painter.rect_filled(
                info_rect,
                DESIGN_TOKENS.rounding.md,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9),
            );

            // P&L and Risk/Reward in info box
            painter.text(
                Pos2::new(
                    entry.x + DESIGN_TOKENS.sizing.position_tool.text_offset_x,
                    info_y + DESIGN_TOKENS.spacing.xl,
                ),
                egui::Align2::LEFT_CENTER,
                summary,
                font_large,
                summary_color,
            );
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
    Some((bars.last().map_or(entry, |bar| bar.close), false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

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
