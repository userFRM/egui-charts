//! Table rendering and cell editing
//!
//! A table keeps its cells in the drawing's text: one row per line and one cell per
//! tab-separated field.

use crate::drawings::domain::Drawing;
use crate::localization as l10n;
use crate::styles::stroke;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Stroke, epaint::StrokeKind};

/// Rows and columns of a table placed without text.
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
    /// and columns while keeping at least one of each. Wide grids scroll horizontally.
    /// Returns the first cell's response.
    pub(crate) fn edit_table(ui: &mut egui::Ui, text: &mut String, id: egui::Id) -> egui::Response {
        let mut grid = cells(text);
        let mut first = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
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
                            let response =
                                ui.add(egui::TextEdit::singleline(cell).id(field).desired_width(
                                    DESIGN_TOKENS.sizing.annotation.table_cell_width,
                                ));
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

    /// Renders a plain grid with left-aligned cells, each column as wide as its widest text.
    pub(crate) fn render_table(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let font = self.annotation_font();
        let theme = painter.ctx().active_theme();
        let cell_text_color = theme.ui.text;
        let rows: Vec<Vec<_>> = cells(&self.table_text())
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| painter.layout_no_wrap(cell, font.clone(), cell_text_color))
                    .collect()
            })
            .collect();
        let line_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), cell_text_color)
            .size()
            .y;
        let cell_height = DESIGN_TOKENS
            .sizing
            .annotation
            .table_cell_height
            .max(line_height + 2.0 * DESIGN_TOKENS.spacing.xs);
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
        let row_top = |r: usize| pos.y + r as f32 * cell_height;

        let table_width = widths.iter().sum::<f32>();
        let table_height = row_top(rows.len()) - pos.y;
        let table_rect = Rect::from_min_size(pos, egui::vec2(table_width, table_height));

        // Background
        painter.rect_filled(table_rect, DESIGN_TOKENS.rounding.md, theme.ui.panel_bg);

        // Grid lines
        let grid_stroke = Stroke::new(stroke::HAIRLINE, theme.ui.border);
        for r in 1..rows.len() {
            painter.hline(
                egui::Rangef::new(pos.x, pos.x + table_width),
                row_top(r),
                grid_stroke,
            );
        }
        for &x in &lefts[1..lefts.len() - 1] {
            painter.vline(
                x,
                egui::Rangef::new(pos.y, pos.y + table_height),
                grid_stroke,
            );
        }

        // Cell text, left-aligned and vertically centred
        for (r, row) in rows.into_iter().enumerate() {
            for (c, galley) in row.into_iter().enumerate() {
                let origin = Pos2::new(
                    lefts[c] + DESIGN_TOKENS.spacing.sm,
                    row_top(r) + (cell_height - galley.size().y) / 2.0,
                );
                painter.galley(origin, galley, cell_text_color);
            }
        }

        // Border
        painter.rect_stroke(
            table_rect,
            DESIGN_TOKENS.rounding.md,
            grid_stroke,
            StrokeKind::Middle,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use egui::{Color32, Context, LayerId, Painter, RawInput, Shape};

    /// Guards #984 and #1399: cells and empty tables use a neutral grid with equal
    /// 22 px rows, thin borders and padded, left-aligned text in every theme.
    #[test]
    fn tables_paint_their_own_cells() {
        let mut table = Drawing::new(1, DrawingToolType::Table);
        table.points = vec![Pos2::new(10.0, 10.0)];
        for (_, theme) in crate::theme::presets() {
            for (text, rows, height, cells) in [
                (None, 4, 88.0, vec![]),
                (
                    Some("Symbol\tLast\tChange\nAAPL\t189.50\nMSFT\t402.10\t+1.2%"),
                    3,
                    66.0,
                    vec![
                        "Symbol", "Last", "Change", "AAPL", "189.50", "MSFT", "402.10", "+1.2%",
                    ],
                ),
            ] {
                table.text = text.map(Into::into);
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut output = ctx.run_ui(RawInput::default(), |_| {
                    table.render_table(&Painter::new(
                        ctx.clone(),
                        LayerId::background(),
                        Rect::EVERYTHING,
                    ));
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                let (mut texts, mut row_rules, mut column_rules) = (Vec::new(), 0, Vec::new());
                for shape in &shapes {
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
                assert!(
                    shapes.iter().any(|shape| matches!(&shape.shape,
                        Shape::Rect(rect) if rect.rect.min == table.points[0]
                            && rect.rect.height() == height && rect.fill == theme.ui.panel_bg
                    )),
                    "the table surface follows the theme"
                );
                let mut fills = 0;
                let mut borders = 0;
                let mut cell = 0;
                for shape in &shapes {
                    match &shape.shape {
                        Shape::Rect(rect) => {
                            if rect.fill != Color32::TRANSPARENT {
                                fills += 1;
                                assert_eq!(rect.fill, theme.ui.panel_bg);
                            }
                            if !rect.stroke.is_empty() {
                                borders += 1;
                                assert_eq!(rect.stroke, Stroke::new(1.0, theme.ui.border));
                            }
                        }
                        Shape::LineSegment { stroke, .. } => {
                            assert_eq!(*stroke, Stroke::new(1.0, theme.ui.border));
                        }
                        Shape::Text(text) if !text.galley.text().is_empty() => {
                            assert!(
                                text.galley
                                    .job
                                    .sections
                                    .iter()
                                    .all(|section| section.format.color == theme.ui.text)
                            );
                            let (x, y) = [
                                (14.0, 21.0),
                                (84.0, 21.0),
                                (154.0, 21.0),
                                (14.0, 43.0),
                                (84.0, 43.0),
                                (14.0, 65.0),
                                (84.0, 65.0),
                                (154.0, 65.0),
                            ][cell];
                            assert_eq!(text.pos.x, x);
                            assert_eq!(text.pos.y + text.galley.size().y / 2.0, y);
                            cell += 1;
                        }
                        _ => {}
                    }
                }
                assert_eq!((fills, borders), (1, 1), "one plain surface and border");
                assert_eq!(texts, cells);
                assert_eq!(row_rules, rows - 1);
                assert_eq!(column_rules, [height; 2], "column rules span the table");
            }
        }
    }
}
