//! Forecast tool rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_forecast(&self, painter: &egui::Painter, _chart_rect: Rect) {
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

        // Trend line
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));

        // Get forecast data for metrics calculation
        let start_price = self.forecast_start_price.unwrap_or(0.0);
        let target_price = self.forecast_target_price.unwrap_or(0.0);
        let start_time = self.forecast_start_time.unwrap_or(0);
        let target_time = self.forecast_target_time.unwrap_or(0);

        // Calculate price change
        let price_change = target_price - start_price;
        let price_change_pct = if start_price != 0.0 {
            (price_change / start_price) * 100.0
        } else {
            0.0
        };

        // Calculate time duration (in days)
        let duration_secs = target_time - start_time;
        let duration_days = duration_secs / 86400; // seconds to days

        // Determine success status
        let is_success = price_change > 0.0; // Positive price movement = success

        // Draw curved forecast extension and info box
        self.draw_forecast_curve(painter, start, end, color);
        self.draw_forecast_info_box(
            painter,
            start,
            end,
            is_success,
            price_change,
            price_change_pct,
            duration_days,
            start_time,
            target_time,
        );
    }

    fn draw_forecast_curve(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        _color: Color32,
    ) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;

        // Create control point for smooth curve
        let control_point = Pos2::new(end.x + dx * 0.5, end.y + dy * 0.5 - 50.0);
        let forecast_end = Pos2::new(end.x + dx, end.y + dy);

        // Draw dashed curved line
        let dashed =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
        let segments = 20;
        for i in 0..segments {
            if i % 2 == 0 {
                let t1 = i as f32 / segments as f32;
                let t2 = ((i + 1) as f32 / segments as f32).min(1.0);

                // Quadratic bezier interpolation
                let p1 = Self::quadratic_bezier(end, control_point, forecast_end, t1);
                let p2 = Self::quadratic_bezier(end, control_point, forecast_end, t2);

                painter.line_segment([p1, p2], Stroke::new(stroke::THICK, dashed));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_forecast_info_box(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        is_success: bool,
        price_change: f64,
        price_change_pct: f64,
        duration_days: i64,
        start_time: i64,
        target_time: i64,
    ) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let forecast_end = Pos2::new(end.x + dx, end.y + dy);

        // Success indicator box (green/red background)
        let success_color = if is_success {
            DESIGN_TOKENS.semantic.extended.bullish
        } else {
            DESIGN_TOKENS.semantic.extended.bearish
        };

        let box_width = 300.0;
        let box_height = 75.0;
        let box_rect = Rect::from_min_size(
            Pos2::new(forecast_end.x - box_width / 2.0, forecast_end.y - 50.0),
            egui::vec2(box_width, box_height),
        );

        // Green/Red header bar
        let header_rect = Rect::from_min_size(box_rect.min, egui::vec2(box_width, 25.0));
        painter.rect_filled(header_rect, DESIGN_TOKENS.rounding.md, success_color);

        // Success text in header
        painter.text(
            Pos2::new(box_rect.min.x + box_width / 2.0, box_rect.min.y + 12.0),
            egui::Align2::CENTER_CENTER,
            if is_success { "SUCCESS" } else { "FAILED" },
            egui::FontId::proportional(typography::MD),
            Color32::WHITE,
        );

        // White info section
        let info_rect = Rect::from_min_size(
            Pos2::new(box_rect.min.x, box_rect.min.y + 25.0),
            egui::vec2(box_width, 50.0),
        );
        painter.rect_filled(info_rect, DESIGN_TOKENS.rounding.md, Color32::WHITE);

        // Price and duration info (dark blue text on white)
        let info_text = format!(
            "{:.2} ({:.2}%) in {}d",
            price_change.abs(),
            price_change_pct.abs(),
            duration_days
        );
        painter.text(
            Pos2::new(box_rect.min.x + box_width / 2.0, box_rect.min.y + 42.0),
            egui::Align2::CENTER_CENTER,
            info_text,
            egui::FontId::proportional(typography::LG),
            DESIGN_TOKENS.semantic.chart.bg, // Dark chart background color for contrast
        );

        // Timestamps at bottom (if available)
        if start_time > 0 && target_time > 0 {
            let timestamp_text = format!(
                "{} -> {}",
                Self::format_timestamp(start_time),
                Self::format_timestamp(target_time)
            );
            painter.text(
                Pos2::new(box_rect.min.x + box_width / 2.0, box_rect.min.y + 62.0),
                egui::Align2::CENTER_CENTER,
                timestamp_text,
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.chart_text_secondary,
            );
        }
    }
}
