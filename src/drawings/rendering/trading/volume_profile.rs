//! Volume profile rendering (VWAP, Fixed Range, Anchored)

use super::draw_dashed_hline;
use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders Anchored VWAP with standard deviation bands.
    /// Features: VWAP line, ±1σ, ±2σ bands, price labels.
    pub(crate) fn render_anchored_vwap(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.is_empty() {
            return;
        }
        let anchor = self.points[0];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Get VWAP data if available, otherwise use anchor point
        let vwap_price = if !self.chart_points.is_empty() {
            self.chart_points[0].price
        } else {
            0.0
        };

        // Calculate standard deviation bands (approximate based on price range)
        let price_range = chart_rect.height() * 0.1;
        let std_dev = price_range * 0.15;

        let band_1_upper_y = anchor.y - std_dev;
        let band_1_lower_y = anchor.y + std_dev;
        let band_2_upper_y = anchor.y - std_dev * 2.0;
        let band_2_lower_y = anchor.y + std_dev * 2.0;

        let line_end = chart_rect.max.x;

        // Draw band fills
        self.draw_vwap_band_fills(
            painter,
            anchor,
            line_end,
            band_1_upper_y,
            band_1_lower_y,
            band_2_upper_y,
            band_2_lower_y,
        );

        // Draw band lines
        let band_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
        self.draw_vwap_band_lines(
            painter,
            anchor,
            line_end,
            band_1_upper_y,
            band_1_lower_y,
            band_2_upper_y,
            band_2_lower_y,
            band_color,
        );

        // Main VWAP line (thicker)
        painter.hline(
            anchor.x..=line_end,
            anchor.y,
            Stroke::new(self.stroke_width.max(2.0), color),
        );

        // Anchor point
        painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );

        // Labels on right edge
        self.draw_vwap_labels(
            painter,
            line_end,
            anchor.y,
            band_1_upper_y,
            band_1_lower_y,
            band_2_upper_y,
            band_2_lower_y,
            vwap_price,
            color,
            band_color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_vwap_band_fills(
        &self,
        painter: &egui::Painter,
        anchor: Pos2,
        line_end: f32,
        band_1_upper_y: f32,
        band_1_lower_y: f32,
        band_2_upper_y: f32,
        band_2_lower_y: f32,
    ) {
        let fill_1 =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 15);
        let fill_2 =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 8);

        // ±1σ fill
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(anchor.x, band_1_upper_y),
                Pos2::new(line_end, band_1_lower_y),
            ),
            0.0,
            fill_1,
        );

        // ±2σ fill (upper)
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(anchor.x, band_2_upper_y),
                Pos2::new(line_end, band_1_upper_y),
            ),
            0.0,
            fill_2,
        );

        // ±2σ fill (lower)
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(anchor.x, band_1_lower_y),
                Pos2::new(line_end, band_2_lower_y),
            ),
            0.0,
            fill_2,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_vwap_band_lines(
        &self,
        painter: &egui::Painter,
        anchor: Pos2,
        line_end: f32,
        band_1_upper_y: f32,
        band_1_lower_y: f32,
        band_2_upper_y: f32,
        band_2_lower_y: f32,
        band_color: Color32,
    ) {
        // ±2σ bands (dashed)
        draw_dashed_hline(
            painter,
            band_2_upper_y,
            anchor.x,
            line_end,
            Stroke::new(stroke::HAIRLINE, band_color),
            6.0,
            4.0,
        );
        draw_dashed_hline(
            painter,
            band_2_lower_y,
            anchor.x,
            line_end,
            Stroke::new(stroke::HAIRLINE, band_color),
            6.0,
            4.0,
        );

        // ±1σ bands (solid, thinner)
        painter.hline(
            anchor.x..=line_end,
            band_1_upper_y,
            Stroke::new(stroke::HAIRLINE, band_color),
        );
        painter.hline(
            anchor.x..=line_end,
            band_1_lower_y,
            Stroke::new(stroke::HAIRLINE, band_color),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_vwap_labels(
        &self,
        painter: &egui::Painter,
        line_end: f32,
        vwap_y: f32,
        band_1_upper_y: f32,
        band_1_lower_y: f32,
        band_2_upper_y: f32,
        band_2_lower_y: f32,
        vwap_price: f64,
        color: Color32,
        band_color: Color32,
    ) {
        let font = egui::FontId::proportional(typography::XS);
        let label_x = line_end - 60.0;

        // VWAP label
        let vwap_label = format!("VWAP {vwap_price:.2}");
        let vwap_bg = Rect::from_center_size(Pos2::new(label_x, vwap_y), egui::vec2(80.0, 16.0));
        painter.rect_filled(vwap_bg, DESIGN_TOKENS.rounding.sm, color);
        painter.text(
            vwap_bg.center(),
            egui::Align2::CENTER_CENTER,
            vwap_label,
            font.clone(),
            Color32::WHITE,
        );

        // +1σ label
        painter.text(
            Pos2::new(label_x + 40.0, band_1_upper_y),
            egui::Align2::CENTER_CENTER,
            "+1σ",
            font.clone(),
            band_color,
        );

        // -1σ label
        painter.text(
            Pos2::new(label_x + 40.0, band_1_lower_y),
            egui::Align2::CENTER_CENTER,
            "-1σ",
            font.clone(),
            band_color,
        );

        // +2σ label
        painter.text(
            Pos2::new(label_x + 40.0, band_2_upper_y),
            egui::Align2::CENTER_CENTER,
            "+2σ",
            font.clone(),
            band_color,
        );

        // -2σ label
        painter.text(
            Pos2::new(label_x + 40.0, band_2_lower_y),
            egui::Align2::CENTER_CENTER,
            "-2σ",
            font,
            band_color,
        );
    }

    /// Renders Fixed Range Volume Profile with POC, VAH, VAL.
    /// Features: horizontal histogram, Point of Control, Value Area.
    pub(crate) fn render_fixed_range_volume_profile(
        &self,
        painter: &egui::Painter,
        _chart_rect: Rect,
    ) {
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

        let rect = Rect::from_two_pos(p1, p2);

        // Generate volume distribution and calculate POC/Value Area
        let (volumes, poc_idx, vah_idx, val_idx) = self.calculate_volume_distribution(&rect);

        // Draw volume histogram
        self.draw_volume_histogram(painter, &rect, &volumes, poc_idx, vah_idx, val_idx);

        // Draw POC, VAH, VAL lines and labels
        let num_rows = volumes.len();
        let row_height = rect.height() / num_rows as f32;
        self.draw_volume_profile_lines(
            painter, &rect, row_height, poc_idx, vah_idx, val_idx, color,
        );

        // Border and anchor points
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );
        self.draw_volume_profile_anchors(painter, p1, p2, color);
    }

    fn calculate_volume_distribution(&self, rect: &Rect) -> (Vec<f32>, usize, usize, usize) {
        let num_rows = 24;
        let poc_row = num_rows / 2 + (num_rows as f32 * 0.1 * (rect.min.x * 0.01).sin()) as usize;
        let mut volumes: Vec<f32> = Vec::with_capacity(num_rows);

        for i in 0..num_rows {
            let dist_from_poc = (i as f32 - poc_row as f32).abs();
            let base_vol = (-dist_from_poc.powi(2) / (num_rows as f32 * 2.0)).exp();
            let noise = 0.7 + 0.3 * ((i as f32 * 17.3 + rect.min.x * 0.1).sin().abs());
            volumes.push(base_vol * noise);
        }

        // Find POC
        let (poc_idx, _) = volumes
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((poc_row, &1.0));

        // Calculate Value Area (70% of total volume centered on POC)
        let total_vol: f32 = volumes.iter().sum();
        let target_vol = total_vol * 0.7;
        let mut vah_idx = poc_idx;
        let mut val_idx = poc_idx;
        let mut accumulated = volumes[poc_idx];

        while accumulated < target_vol && (vah_idx > 0 || val_idx < num_rows - 1) {
            let upper_add = if vah_idx > 0 {
                volumes[vah_idx - 1]
            } else {
                0.0
            };
            let lower_add = if val_idx < num_rows - 1 {
                volumes[val_idx + 1]
            } else {
                0.0
            };

            if upper_add >= lower_add && vah_idx > 0 {
                vah_idx -= 1;
                accumulated += volumes[vah_idx];
            } else if val_idx < num_rows - 1 {
                val_idx += 1;
                accumulated += volumes[val_idx];
            } else if vah_idx > 0 {
                vah_idx -= 1;
                accumulated += volumes[vah_idx];
            } else {
                break;
            }
        }

        (volumes, poc_idx, vah_idx, val_idx)
    }

    fn draw_volume_histogram(
        &self,
        painter: &egui::Painter,
        rect: &Rect,
        volumes: &[f32],
        poc_idx: usize,
        vah_idx: usize,
        val_idx: usize,
    ) {
        let num_rows = volumes.len();
        let row_height = rect.height() / num_rows as f32;
        let max_width = rect.width() * 0.9;
        let max_vol = volumes.iter().cloned().fold(0.0f32, f32::max);

        for i in 0..num_rows {
            let y = rect.min.y + row_height * i as f32;
            let width = (volumes[i] / max_vol) * max_width;

            // Color coding: POC is bright, Value Area is medium, outside is dim
            let bar_color = if i == poc_idx {
                DESIGN_TOKENS.semantic.extended.favorite_gold // Gold for POC
            } else if i >= vah_idx && i <= val_idx {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180)
            } else {
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80)
            };

            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.min.x, y),
                    egui::vec2(width, row_height - 1.0),
                ),
                0.0,
                bar_color,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_volume_profile_lines(
        &self,
        painter: &egui::Painter,
        rect: &Rect,
        row_height: f32,
        poc_idx: usize,
        vah_idx: usize,
        val_idx: usize,
        _color: Color32,
    ) {
        let font = egui::FontId::proportional(typography::XS);

        // Draw POC line (dashed, extends across range)
        let poc_y = rect.min.y + row_height * poc_idx as f32 + row_height / 2.0;
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
        let vah_y = rect.min.y + row_height * vah_idx as f32;
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
        let val_y = rect.min.y + row_height * (val_idx + 1) as f32;
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
        p1: Pos2,
        p2: Pos2,
        color: Color32,
    ) {
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Renders Anchored Volume Profile similar to fixed range but from anchor.
    pub(crate) fn render_anchored_volume_profile(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.is_empty() {
            return;
        }

        // Create a rect from anchor to chart edge
        let anchor = self.points[0];
        let end = if self.points.len() >= 2 {
            self.points[1]
        } else {
            Pos2::new(chart_rect.max.x, anchor.y + 100.0)
        };

        // Store original points and use fixed range render
        let temp_p2 = Pos2::new(end.x, anchor.y + (end.y - anchor.y).abs().max(50.0) * 2.0);

        // Draw the profile
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let rect = Rect::from_two_pos(anchor, temp_p2);

        // Simplified profile for anchored version
        self.draw_simplified_profile(painter, &rect, chart_rect.max.x);

        // Anchor indicator
        painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );

        // Label
        let font = egui::FontId::proportional(typography::XS);
        let label_bg = Rect::from_center_size(
            Pos2::new(anchor.x + 50.0, anchor.y - 15.0),
            egui::vec2(100.0, 16.0),
        );
        painter.rect_filled(
            label_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            label_bg.center(),
            egui::Align2::CENTER_CENTER,
            "Anchored Profile",
            font,
            color,
        );
    }

    fn draw_simplified_profile(&self, painter: &egui::Painter, rect: &Rect, chart_max_x: f32) {
        let num_rows = 20;
        let row_height = rect.height() / num_rows as f32;
        let max_width = (chart_max_x - rect.min.x).min(200.0);

        for i in 0..num_rows {
            let y = rect.min.y + row_height * i as f32;
            let dist = (i as f32 - num_rows as f32 / 2.0).abs();
            let vol = (-dist.powi(2) / (num_rows as f32)).exp();
            let width = vol * max_width;

            let bar_color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                (100.0 + 100.0 * vol) as u8,
            );

            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.min.x, y),
                    egui::vec2(width, row_height - 1.0),
                ),
                0.0,
                bar_color,
            );
        }
    }
}
