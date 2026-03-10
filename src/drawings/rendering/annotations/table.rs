//! Table rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders a data table with configurable rows and columns.
    /// Features: header row, grid lines, alternating row colors.
    pub(crate) fn render_table(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Table configuration
        let cell_width = DESIGN_TOKENS.sizing.annotation.table_cell_width;
        let cell_height = DESIGN_TOKENS.sizing.annotation.table_cell_height;
        let header_height = DESIGN_TOKENS.sizing.annotation.table_header_height;
        let rows = 4; // Including header
        let cols = 3;

        let table_width = cell_width * cols as f32;
        let table_height = header_height + cell_height * (rows - 1) as f32;
        let table_rect = Rect::from_min_size(pos, egui::vec2(table_width, table_height));

        // Background
        painter.rect_filled(
            table_rect,
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.94),
        );

        // Header row background
        let header_rect = Rect::from_min_size(pos, egui::vec2(table_width, header_height));
        painter.rect_filled(header_rect, DESIGN_TOKENS.rounding.md, color);

        // Grid lines
        let grid_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        let grid_stroke = Stroke::new(stroke::HAIRLINE, grid_color);

        // Horizontal lines
        for i in 0..=rows {
            let y = if i == 0 {
                pos.y
            } else if i == 1 {
                pos.y + header_height
            } else {
                pos.y + header_height + (i - 1) as f32 * cell_height
            };
            painter.hline(
                egui::Rangef::new(pos.x, pos.x + table_width),
                y,
                grid_stroke,
            );
        }

        // Vertical lines
        for j in 0..=cols {
            let x = pos.x + j as f32 * cell_width;
            painter.vline(
                x,
                egui::Rangef::new(pos.y, pos.y + table_height),
                grid_stroke,
            );
        }

        // Header text
        let header_font = egui::FontId::proportional(typography::SM);
        let headers = ["Column A", "Column B", "Column C"];
        for (j, header) in headers.iter().enumerate() {
            painter.text(
                Pos2::new(
                    pos.x + j as f32 * cell_width + cell_width / 2.0,
                    pos.y + header_height / 2.0,
                ),
                egui::Align2::CENTER_CENTER,
                *header,
                header_font.clone(),
                Color32::WHITE,
            );
        }

        // Sample data cells
        let cell_font = egui::FontId::proportional(typography::XS);
        let cell_text_color = DESIGN_TOKENS.semantic.extended.chart_text;

        for i in 1..rows {
            // Alternating row background
            if i % 2 == 0 {
                let row_rect = Rect::from_min_size(
                    Pos2::new(pos.x, pos.y + header_height + (i - 1) as f32 * cell_height),
                    egui::vec2(table_width, cell_height),
                );
                painter.rect_filled(
                    row_rect,
                    0.0,
                    DESIGN_TOKENS
                        .semantic
                        .extended
                        .chart_axis_bg
                        .gamma_multiply(0.5),
                );
            }

            for j in 0..cols {
                let cell_x = pos.x + j as f32 * cell_width + cell_width / 2.0;
                let cell_y =
                    pos.y + header_height + (i - 1) as f32 * cell_height + cell_height / 2.0;
                painter.text(
                    Pos2::new(cell_x, cell_y),
                    egui::Align2::CENTER_CENTER,
                    format!("R{}C{}", i, j + 1),
                    cell_font.clone(),
                    cell_text_color,
                );
            }
        }

        // Border
        painter.rect_stroke(
            table_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, color),
            StrokeKind::Outside,
        );

        // Resize handle at bottom-right corner
        let handle_size = 8.0;
        let handle_pos = Pos2::new(
            table_rect.max.x - handle_size,
            table_rect.max.y - handle_size,
        );
        let handle_lines = [
            [
                Pos2::new(handle_pos.x + 2.0, handle_pos.y + handle_size),
                Pos2::new(handle_pos.x + handle_size, handle_pos.y + 2.0),
            ],
            [
                Pos2::new(handle_pos.x + 5.0, handle_pos.y + handle_size),
                Pos2::new(handle_pos.x + handle_size, handle_pos.y + 5.0),
            ],
        ];
        for line in &handle_lines {
            painter.line_segment(*line, Stroke::new(stroke::MEDIUM, grid_color));
        }
    }
}
