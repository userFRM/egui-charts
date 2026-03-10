//! Measurement tool rendering implementations
//!
//! Includes: measure, price range, date range, date and price range, info line, and bars pattern.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    pub(crate) fn render_measure(&self, painter: &egui::Painter) {
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

            // Calculate percentage (price_diff already calculated above)
            let price_pct = if cp1.price != 0.0 {
                (price_diff / cp1.price) * 100.0
            } else {
                0.0
            };

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

            // Calculate bars (time) difference
            let bars_diff = (cp2.bar_idx - cp1.bar_idx).abs() as i32;

            // Format time duration (assuming 1 bar = 1 minute for intraday charts)
            let total_minutes = bars_diff;
            let hours = total_minutes / 60;
            let minutes = total_minutes % 60;
            let time_str = if hours > 0 {
                format!("{hours}h {minutes}m")
            } else {
                format!("{minutes}m")
            };

            // Calculate pips/points (scaled price difference)
            let pips = (price_diff.abs() * 100.0).round() as i32;

            // Build info text (3 lines)
            // Line 1: "2.21 (0.86%) 221"
            // Line 2: "73 bars, 1h 13m"
            // Line 3: "Vol 895.43K"
            let info = format!(
                "{:.2} ({:.2}%) {}\n{} bars, {}\nVol --",
                price_diff.abs(),
                price_pct.abs(), // No + sign, just absolute percentage
                pips,
                bars_diff,
                time_str
            );

            // Draw info box at the top-left corner of rect
            let font = egui::FontId::proportional(typography::SM);
            let galley = painter.layout_no_wrap(info.clone(), font.clone(), Color32::WHITE);
            let text_size = galley.size();

            // Pos box near the top-left corner of rect
            let box_x = rect.min.x.min(rect.max.x) + 5.0;
            let box_y = rect.min.y.min(rect.max.y) - text_size.y - 15.0;

            // Draw background (solid color matching direction: red for down, blue for up)
            let padding = egui::vec2(8.0, 6.0);
            let box_rect =
                egui::Rect::from_min_size(Pos2::new(box_x, box_y), text_size + padding * 2.0);
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

    pub(crate) fn render_price_range(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        // Price difference info - use actual prices from chart_points
        let info = if self.chart_points.len() >= 2 {
            let cp1 = &self.chart_points[0];
            let cp2 = &self.chart_points[1];
            let price_diff = cp2.price - cp1.price;
            let price_pct = if cp1.price != 0.0 {
                (price_diff / cp1.price) * 100.0
            } else {
                0.0
            };
            format!("{price_diff:+.2} ({price_pct:+.2}%)")
        } else {
            let dy = (p2.y - p1.y).abs();
            format!("Δ: {dy:.1}")
        };

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

    pub(crate) fn render_date_range(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        // Time difference info - use actual bar counts from chart_points
        let info = if self.chart_points.len() >= 2 {
            let cp1 = &self.chart_points[0];
            let cp2 = &self.chart_points[1];
            let bars_diff = (cp2.bar_idx - cp1.bar_idx).abs() as i32;
            format!("{bars_diff} bars")
        } else {
            let dx = (p2.x - p1.x).abs();
            format!("Δ: {dx:.1}")
        };

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

    pub(crate) fn render_date_and_price_range(&self, painter: &egui::Painter, _chart_rect: Rect) {
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

        // Labels - use actual financial values from chart_points
        let info = if self.chart_points.len() >= 2 {
            let cp1 = &self.chart_points[0];
            let cp2 = &self.chart_points[1];
            let bars_diff = (cp2.bar_idx - cp1.bar_idx).abs() as i32;
            let price_diff = cp2.price - cp1.price;
            let price_pct = if cp1.price != 0.0 {
                (price_diff / cp1.price) * 100.0
            } else {
                0.0
            };
            format!("{bars_diff} bars | {price_diff:+.2} ({price_pct:+.2}%)")
        } else {
            let dx = (p2.x - p1.x).abs();
            let dy = (p2.y - p1.y).abs();
            format!("Δx:{dx:.0} Δy:{dy:.0}")
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            info,
            egui::FontId::proportional(typography::XS),
            color,
        );
    }

    pub(crate) fn render_info_line(&self, painter: &egui::Painter) {
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

        // Calculate info values
        let (info_text, is_positive) = if self.chart_points.len() >= 2 {
            let cp1 = &self.chart_points[0];
            let cp2 = &self.chart_points[1];
            let bars_diff = (cp2.bar_idx - cp1.bar_idx).abs() as i32;
            let price_diff = cp2.price - cp1.price;
            let price_pct = if cp1.price != 0.0 {
                (price_diff / cp1.price) * 100.0
            } else {
                0.0
            };

            // Calculate time duration (assuming 1 bar = 1 minute for intraday)
            let total_minutes = bars_diff;
            let hours = total_minutes / 60;
            let minutes = total_minutes % 60;
            let time_str = if hours > 0 {
                format!("{hours}h {minutes}m")
            } else {
                format!("{minutes}m")
            };

            // Calculate angle
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let angle_deg = dy.atan2(dx).to_degrees();

            // Info format: 3 lines
            // Line 1: Price change with percentage
            // Line 2: Bars and time
            // Line 3: Angle
            (
                format!(
                    "{price_diff:+.2} ({price_pct:+.2}%)\n{bars_diff} bars, {time_str}\n{angle_deg:.1}°"
                ),
                price_diff >= 0.0,
            )
        } else {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let angle_deg = dy.atan2(dx).to_degrees();
            (
                format!("D:{dist:.1}\nΔx:{dx:.0} Δy:{dy:.0}\n{angle_deg:.1}°"),
                dy <= 0.0,
            )
        };

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
        let box_rect = Rect::from_min_size(box_pos, text_size + padding * 2.0);
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
}
