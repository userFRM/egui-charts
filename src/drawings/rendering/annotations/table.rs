//! Table rendering and cell editing
//!
//! A table keeps its cells in the drawing's text: one row per line and one cell per
//! tab-separated field, the first row being the header.

use crate::drawings::domain::Drawing;
use crate::localization as l10n;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Rows, header included, and columns of a table placed without text.
const NEW_TABLE: (usize, usize) = (4, 3);

/// Cells of table text, short rows padded to the widest one.
fn cells(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = text
        .split('\n')
        .map(|row| row.split('\t').map(str::to_owned).collect())
        .collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
    for row in &mut rows {
        row.resize(cols, String::new());
    }
    rows
}

fn join(cells: &[Vec<String>]) -> String {
    cells
        .iter()
        .map(|row| row.join("\t"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Drawing {
    /// The table's cell text: its own, or an empty table of the default size.
    pub(crate) fn table_text(&self) -> String {
        self.text
            .clone()
            .unwrap_or_else(|| join(&vec![vec![String::new(); NEW_TABLE.1]; NEW_TABLE.0]))
    }

    /// Edits table text as a grid of single-line cells, with buttons that add and remove rows
    /// and columns while keeping at least one of each, the cells scrolling beyond `max_height`.
    /// Returns the first cell's response.
    pub(crate) fn edit_table(
        ui: &mut egui::Ui,
        text: &mut String,
        id: egui::Id,
        max_height: f32,
    ) -> egui::Response {
        let mut grid = cells(text);
        let mut first = None;
        egui::ScrollArea::both()
            .max_height(max_height)
            .show(ui, |ui| {
                // Without a minimum, the first frame's narrow columns would stay narrow.
                egui::Grid::new(id.with("cells"))
                    .min_col_width(DESIGN_TOKENS.sizing.annotation.table_cell_width)
                    .show(ui, |ui| {
                        for (r, row) in grid.iter_mut().enumerate() {
                            for (c, cell) in row.iter_mut().enumerate() {
                                let field = if r == 0 && c == 0 {
                                    id
                                } else {
                                    id.with((r, c))
                                };
                                let response = ui.add(
                                    egui::TextEdit::singleline(cell).id(field).desired_width(
                                        DESIGN_TOKENS.sizing.annotation.table_cell_width,
                                    ),
                                );
                                // Tabs and line breaks separate the cells in the saved text.
                                cell.retain(|ch| !matches!(ch, '\t' | '\n' | '\r'));
                                first.get_or_insert(response);
                            }
                            ui.end_row();
                        }
                    });
            });
        let (rows, cols) = (grid.len(), grid[0].len());
        ui.horizontal_wrapped(|ui| {
            if ui.button(l10n::text(ui.ctx(), "Add row")).clicked() {
                grid.push(vec![String::new(); cols]);
            }
            if ui
                .add_enabled(
                    rows > 1,
                    egui::Button::new(l10n::text(ui.ctx(), "Remove row")),
                )
                .clicked()
            {
                grid.pop();
            }
            if ui.button(l10n::text(ui.ctx(), "Add column")).clicked() {
                grid.iter_mut().for_each(|row| row.push(String::new()));
            }
            if ui
                .add_enabled(
                    cols > 1,
                    egui::Button::new(l10n::text(ui.ctx(), "Remove column")),
                )
                .clicked()
            {
                grid.iter_mut().for_each(|row| {
                    row.pop();
                });
            }
        });
        *text = join(&grid);
        first.expect("a table has at least one cell")
    }

    /// Renders the table's cells, its first row as the header, each column as wide as its
    /// widest text.
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
        let font = self.annotation_font();
        let cell_text_color = DESIGN_TOKENS.semantic.extended.chart_text;
        let rows: Vec<Vec<_>> = cells(&self.table_text())
            .into_iter()
            .enumerate()
            .map(|(r, row)| {
                let text = if r == 0 {
                    Color32::WHITE
                } else {
                    cell_text_color
                };
                row.into_iter()
                    .map(|cell| painter.layout_no_wrap(cell, font.clone(), text))
                    .collect()
            })
            .collect();
        let line_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), color)
            .size()
            .y;
        let cell_height = DESIGN_TOKENS
            .sizing
            .annotation
            .table_cell_height
            .max(line_height + 2.0 * DESIGN_TOKENS.spacing.xs);
        let header_height = DESIGN_TOKENS
            .sizing
            .annotation
            .table_header_height
            .max(cell_height);
        let widths: Vec<f32> = (0..rows[0].len())
            .map(|c| {
                rows.iter()
                    .map(|row| row[c].size().x + 2.0 * DESIGN_TOKENS.spacing.sm)
                    .fold(DESIGN_TOKENS.sizing.annotation.table_cell_width, f32::max)
            })
            .collect();
        let lefts: Vec<f32> = std::iter::once(pos.x)
            .chain(widths.iter().scan(pos.x, |x, width| {
                *x += width;
                Some(*x)
            }))
            .collect();
        let row_top = |r: usize| {
            pos.y
                + if r == 0 {
                    0.0
                } else {
                    header_height + (r - 1) as f32 * cell_height
                }
        };

        let table_width = widths.iter().sum::<f32>();
        let table_height = row_top(rows.len()) - pos.y;
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

        // Alternating row background
        for r in (2..rows.len()).step_by(2) {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(pos.x, row_top(r)),
                    egui::vec2(table_width, cell_height),
                ),
                0.0,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.5),
            );
        }

        // Grid lines
        let grid_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        let grid_stroke = Stroke::new(stroke::HAIRLINE, grid_color);
        for r in 0..=rows.len() {
            painter.hline(
                egui::Rangef::new(pos.x, pos.x + table_width),
                row_top(r),
                grid_stroke,
            );
        }
        for &x in &lefts {
            painter.vline(
                x,
                egui::Rangef::new(pos.y, pos.y + table_height),
                grid_stroke,
            );
        }

        // Cell text, centred in each cell
        for (r, row) in rows.into_iter().enumerate() {
            let height = if r == 0 { header_height } else { cell_height };
            for (c, galley) in row.into_iter().enumerate() {
                let center = Pos2::new(lefts[c] + widths[c] / 2.0, row_top(r) + height / 2.0);
                painter.galley(center - galley.size() / 2.0, galley, color);
            }
        }

        // Border
        painter.rect_stroke(
            table_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, color),
            StrokeKind::Outside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use egui::{Context, LayerId, Painter, RawInput, Shape};

    /// Guards #984: a table paints its own cells, header row first and short rows padded, and
    /// an empty grid of the default size until it has text, its frame as tall as its rows: a
    /// 26 px header and 22 px rows.
    #[test]
    fn tables_paint_their_own_cells() {
        let mut table = Drawing::new(1, DrawingToolType::Table);
        table.points = vec![Pos2::new(10.0, 10.0)];
        for (text, rows, height, cells) in [
            (None, 4, 92.0, vec![]),
            (
                Some("Symbol\tLast\tChange\nAAPL\t189.50\nMSFT\t402.10\t+1.2%"),
                3,
                70.0,
                vec![
                    "Symbol", "Last", "Change", "AAPL", "189.50", "MSFT", "402.10", "+1.2%",
                ],
            ),
        ] {
            table.text = text.map(Into::into);
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |_| {
                table.render_table(&Painter::new(
                    ctx.clone(),
                    LayerId::background(),
                    Rect::EVERYTHING,
                ));
            });
            let (mut texts, mut row_rules, mut column_rules) = (Vec::new(), 0, Vec::new());
            for shape in &output.shapes {
                match &shape.shape {
                    Shape::Text(text) if !text.galley.text().is_empty() => {
                        texts.push(text.galley.text().to_owned());
                    }
                    Shape::LineSegment { points, .. } if points[0].x == points[1].x => {
                        column_rules.push(points[1].y - points[0].y);
                    }
                    Shape::LineSegment { .. } => row_rules += 1,
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(texts, cells);
            assert_eq!(row_rules, rows + 1);
            assert_eq!(column_rules, [height; 4], "column rules span the table");
        }
    }
}
