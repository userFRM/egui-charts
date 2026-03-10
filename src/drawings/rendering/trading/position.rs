//! Long/short position tool rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_pos_tool(&self, painter: &egui::Painter, is_long: bool) {
        if self.points.len() < 2 {
            if !self.points.is_empty() {
                painter.circle_filled(
                    self.points[0],
                    DESIGN_TOKENS.rounding.md,
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

        let entry = self.points[0];
        let target = self.points[1];

        // Position colors
        let profit_color = DESIGN_TOKENS.semantic.extended.bullish;
        let loss_color = DESIGN_TOKENS.semantic.extended.bearish;
        let entry_color = DESIGN_TOKENS.semantic.extended.chart_text_secondary;

        // Get actual price values from chart_points
        let (entry_price, target_price) = if self.chart_points.len() >= 2 {
            (self.chart_points[0].price, self.chart_points[1].price)
        } else {
            (0.0, 0.0)
        };

        // Calculate target and stop prices based on position direction
        let (target_price_actual, stop_price, target_y, stop_y) = if is_long {
            // Long: target above entry, stop below entry
            let tp = entry_price.max(target_price);
            let sl = entry_price - (tp - entry_price); // Symmetric stop
            let ty = entry.y.min(target.y);
            let sy = entry.y + (entry.y - ty); // Symmetric stop y
            (tp, sl, ty, sy)
        } else {
            // Short: target below entry, stop above entry
            let tp = entry_price.min(target_price);
            let sl = entry_price + (entry_price - tp); // Symmetric stop
            let ty = entry.y.max(target.y);
            let sy = entry.y - (ty - entry.y); // Symmetric stop y
            (tp, sl, ty, sy)
        };

        // Calculate P&L percentages
        let profit_pct = if entry_price != 0.0 {
            ((target_price_actual - entry_price) / entry_price * 100.0).abs()
        } else {
            0.0
        };
        let loss_pct = if entry_price != 0.0 {
            ((stop_price - entry_price) / entry_price * 100.0).abs()
        } else {
            0.0
        };
        let risk_reward = if loss_pct != 0.0 {
            profit_pct / loss_pct
        } else {
            0.0
        };

        // Calculate real-time P&L
        let qty = self.quantity.unwrap_or(100.0);
        let curr_price = self.curr_price.unwrap_or(entry_price);

        let pnl_per_unit = if is_long {
            curr_price - entry_price
        } else {
            entry_price - curr_price
        };
        let total_pnl = pnl_per_unit * qty as f64;

        // Calculate width - extend across chart
        let width = (target.x - entry.x)
            .abs()
            .max(DESIGN_TOKENS.sizing.position_tool.min_width);
        let right = entry.x + width;

        // Draw filled zones
        self.draw_profit_zone(painter, is_long, entry, target_y, right);
        self.draw_loss_zone(painter, is_long, entry, stop_y, right);

        // Draw horizontal lines (thicker)
        self.draw_position_lines(
            painter,
            entry,
            target_y,
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
            target_y,
            stop_y,
            right,
            entry_color,
            profit_color,
            loss_color,
        );

        // Draw labels
        self.draw_position_labels(
            painter,
            entry,
            target_y,
            stop_y,
            right,
            entry_price,
            target_price_actual,
            stop_price,
            profit_pct,
            loss_pct,
            risk_reward,
            total_pnl,
            qty as f64,
            profit_color,
            loss_color,
        );
    }

    fn draw_profit_zone(
        &self,
        painter: &egui::Painter,
        is_long: bool,
        entry: Pos2,
        target_y: f32,
        right: f32,
    ) {
        let profit_rect = if is_long {
            Rect::from_min_max(Pos2::new(entry.x, target_y), Pos2::new(right, entry.y))
        } else {
            Rect::from_min_max(Pos2::new(entry.x, entry.y), Pos2::new(right, target_y))
        };
        // Visibility check: skip rendering if rect is outside clip region
        if !painter.clip_rect().intersects(profit_rect) {
            return;
        }
        painter.rect_filled(
            profit_rect,
            0.0,
            DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(30_f32 / 255.0),
        );
    }

    fn draw_loss_zone(
        &self,
        painter: &egui::Painter,
        is_long: bool,
        entry: Pos2,
        stop_y: f32,
        right: f32,
    ) {
        let loss_rect = if is_long {
            Rect::from_min_max(Pos2::new(entry.x, entry.y), Pos2::new(right, stop_y))
        } else {
            Rect::from_min_max(Pos2::new(entry.x, stop_y), Pos2::new(right, entry.y))
        };
        // Visibility check: skip rendering if rect is outside clip region
        if !painter.clip_rect().intersects(loss_rect) {
            return;
        }
        painter.rect_filled(
            loss_rect,
            0.0,
            DESIGN_TOKENS
                .semantic
                .extended
                .bearish
                .gamma_multiply(30_f32 / 255.0),
        );
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
        entry_price: f64,
        target_price_actual: f64,
        stop_price: f64,
        profit_pct: f64,
        loss_pct: f64,
        risk_reward: f64,
        total_pnl: f64,
        qty: f64,
        profit_color: Color32,
        loss_color: Color32,
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
            let pnl_text = format!(
                "P&L: {:.2} ({:.2}%)  •  Risk/Reward: {:.2}  •  Qty: {}",
                total_pnl,
                (total_pnl / (entry_price * qty) * 100.0),
                risk_reward,
                qty as i32
            );
            painter.text(
                Pos2::new(
                    entry.x + DESIGN_TOKENS.sizing.position_tool.text_offset_x,
                    info_y + DESIGN_TOKENS.spacing.xl,
                ),
                egui::Align2::LEFT_CENTER,
                pnl_text,
                font_large,
                if total_pnl >= 0.0 {
                    profit_color
                } else {
                    loss_color
                },
            );
        }

        // Target label (on right side of line)
        let target_label = format!(
            "Target: {:.2} ({:.2}%) {}",
            target_price_actual, profit_pct, qty as i32
        );
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
        let stop_label = format!("Stop: {:.2} ({:.2}%) {}", stop_price, loss_pct, qty as i32);
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
