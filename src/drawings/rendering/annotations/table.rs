//! Table rendering and cell editing
//!
//! A table keeps its cells in the drawing's text: one row per line and one cell per
//! tab-separated field.

use crate::drawings::domain::{
    Drawing, FontWeight, HandlePos, LineTextHorizontalAlign, LineTextVerticalAlign, TableCellStyle,
    TableConfig,
};
use crate::drawings::rendering::style::BOLD_OFFSET;
use crate::drawings::validation::{MAX_TABLE_CELL, MIN_TABLE_CELL};
use crate::localization as l10n;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, CornerRadius, Pos2, Rect, epaint::StrokeKind};
use std::sync::Arc;

/// Rows and columns of a table placed without text.
const NEW_TABLE: (usize, usize) = (4, 3);

/// A table laid out at an origin: each cell's text in its colour, row by row, and the edges of
/// its columns and rows from left to right and top to bottom, one more than the columns or the
/// rows.
pub(crate) struct TableLayout {
    pub(crate) cells: Vec<Vec<Arc<egui::Galley>>>,
    pub(crate) xs: Vec<f32>,
    pub(crate) ys: Vec<f32>,
}

/// The edges of cells of `sizes` laid end to end from `start`.
fn edges(start: f32, sizes: impl Iterator<Item = f32>) -> Vec<f32> {
    std::iter::once(start)
        .chain(sizes.scan(start, |edge, size| {
            *edge += size;
            Some(*edge)
        }))
        .collect()
}

fn rgba([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

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

    /// The number of rows and columns of the table's text.
    pub(crate) fn table_shape(&self) -> (usize, usize) {
        let grid = cells(&self.table_text());
        (grid.len(), grid[0].len())
    }

    /// The table's anchor, the top end of the right edge of each column and the left end of the
    /// bottom edge of each row, as painted: the points the table's handles sit at.
    pub(crate) fn table_handle_points(&self, painter: &egui::Painter) -> Option<Vec<Pos2>> {
        let &origin = self.points.first()?;
        let TableLayout { xs, ys, .. } = self.table_layout(painter, origin);
        let column_tops = xs[1..].iter().map(|&x| Pos2::new(x, origin.y));
        let row_lefts = ys[1..].iter().map(|&y| Pos2::new(origin.x, y));
        Some(
            std::iter::once(origin)
                .chain(column_tops)
                .chain(row_lefts)
                .collect(),
        )
    }

    /// Sizes the table so that the edge `handle` names follows `pos`: a column's right edge sets
    /// its width, a row's bottom edge its height, and the bottom-right corner scales every
    /// column and row. `edges` are the points [`Self::table_handle_points`] gave for the last
    /// paint, which a column's or row's own left or top edge keeps while it is sized. Sizes stay
    /// from 8 to 10,000 points, and the text keeps its size.
    pub(crate) fn resize_table(&mut self, handle: HandlePos, pos: Pos2, edges: &[Pos2]) {
        let (rows, cols) = self.table_shape();
        if edges.len() != 1 + cols + rows || !pos.is_finite() {
            return;
        }
        let xs: Vec<f32> = std::iter::once(edges[0].x)
            .chain(edges[1..=cols].iter().map(|edge| edge.x))
            .collect();
        let ys: Vec<f32> = std::iter::once(edges[0].y)
            .chain(edges[1 + cols..].iter().map(|edge| edge.y))
            .collect();
        let size = |size: f32| size.clamp(MIN_TABLE_CELL, MAX_TABLE_CELL);
        match handle {
            HandlePos::Column(c) if c < cols => self
                .table
                .get_or_insert_with(Default::default)
                .set_column_width(c, size(pos.x - xs[c])),
            HandlePos::Row(r) if r < rows => self
                .table
                .get_or_insert_with(Default::default)
                .set_row_height(r, size(pos.y - ys[r])),
            HandlePos::BottomRight => {
                let painted = |edges: &[f32]| -> Vec<f32> {
                    edges.windows(2).map(|pair| pair[1] - pair[0]).collect()
                };
                let scale = (pos - edges[0]) / egui::vec2(xs[cols] - xs[0], ys[rows] - ys[0]);
                self.table.get_or_insert_with(Default::default).scale(
                    &painted(&xs),
                    &painted(&ys),
                    scale,
                );
            }
            _ => {}
        }
    }

    /// Whether `draft` holds the same cells as the table's text, so that closing its editor
    /// changes nothing. Padding a short row or writing out an empty table is no change.
    pub(crate) fn same_table_cells(&self, draft: &str) -> bool {
        cells(&self.table_text()) == cells(draft)
    }

    /// Edits table text as a grid of single-line cells, and the settings of the focused cell:
    /// buttons insert a row above or below it or a column left or right of it and delete its row
    /// or column, keeping at least one of each, and a row of controls sets its text colour,
    /// background and alignment. `colors` are the table's text and background colours, shown for
    /// a cell without its own. Wide grids scroll horizontally. Returns the first cell's response.
    pub(crate) fn edit_table(
        ui: &mut egui::Ui,
        text: &mut String,
        table: &mut TableConfig,
        colors: [Color32; 2],
        id: egui::Id,
    ) -> egui::Response {
        use LineTextHorizontalAlign as H;
        use LineTextVerticalAlign as V;
        let mut grid = cells(text);
        // Remembered, so that the cell stays chosen while a button takes the click.
        let focus_key = id.with("focused_cell");
        let (mut row, mut column) = ui
            .data(|d| d.get_temp::<(usize, usize)>(focus_key))
            .unwrap_or_default();
        let mut first = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            // Without a minimum, the first frame's narrow columns would stay narrow.
            egui::Grid::new(id.with("cells"))
                .min_col_width(DESIGN_TOKENS.sizing.annotation.table_cell_width)
                .show(ui, |ui| {
                    for (r, fields) in grid.iter_mut().enumerate() {
                        for (c, cell) in fields.iter_mut().enumerate() {
                            let field = if r == 0 && c == 0 {
                                id
                            } else {
                                id.with((r, c))
                            };
                            let response =
                                ui.add(egui::TextEdit::singleline(cell).id(field).desired_width(
                                    DESIGN_TOKENS.sizing.annotation.table_cell_width,
                                ));
                            let label = l10n::format_text(
                                ui.ctx(),
                                "Row {row}, column {column}",
                                &[
                                    ("row", (r + 1).to_string()),
                                    ("column", (c + 1).to_string()),
                                ],
                            );
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_label(label);
                            });
                            // Tabs and line breaks separate the cells in the saved text.
                            cell.retain(|ch| !matches!(ch, '\t' | '\n' | '\r'));
                            if response.has_focus() {
                                (row, column) = (r, c);
                            }
                            first.get_or_insert(response);
                        }
                        ui.end_row();
                    }
                });
        });
        let (rows, cols) = (grid.len(), grid[0].len());
        (row, column) = (row.min(rows - 1), column.min(cols - 1));
        ui.horizontal_wrapped(|ui| {
            let mut button = |label: &str, enabled: bool| {
                ui.add_enabled(enabled, egui::Button::new(l10n::text(ui.ctx(), label)))
                    .clicked()
            };
            if button("Insert row above", true) {
                grid.insert(row, vec![String::new(); cols]);
                table.insert_row(row);
            }
            if button("Insert row below", true) {
                grid.insert(row + 1, vec![String::new(); cols]);
                table.insert_row(row + 1);
            }
            if button("Insert column left", true) {
                grid.iter_mut()
                    .for_each(|fields| fields.insert(column, String::new()));
                table.insert_column(column);
            }
            if button("Insert column right", true) {
                grid.iter_mut()
                    .for_each(|fields| fields.insert(column + 1, String::new()));
                table.insert_column(column + 1);
            }
            if button("Delete row", rows > 1) {
                grid.remove(row);
                table.remove_row(row);
            }
            if button("Delete column", cols > 1) {
                grid.iter_mut().for_each(|fields| {
                    fields.remove(column);
                });
                table.remove_column(column);
            }
        });
        let (rows, cols) = (grid.len(), grid[0].len());
        (row, column) = (row.min(rows - 1), column.min(cols - 1));
        ui.data_mut(|d| d.insert_temp(focus_key, (row, column)));

        let opened = table.cell(row, column);
        let mut style = opened;
        ui.horizontal_wrapped(|ui| {
            for (label, color, unset) in [
                ("Text color", &mut style.text_color, colors[0]),
                ("Background", &mut style.background, colors[1]),
            ] {
                let label = ui.label(l10n::text(ui.ctx(), label));
                let mut shown = color.map_or(unset, rgba);
                if egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut shown,
                    egui::color_picker::Alpha::OnlyBlend,
                )
                .labelled_by(label.id)
                .changed()
                {
                    *color = Some(shown.to_srgba_unmultiplied());
                }
            }
            // The table's own alignment is stored as no setting, as on a cell never aligned.
            let across = style.horizontal_align.unwrap_or(H::Left);
            for (align, name) in [
                (H::Left, "Left"),
                (H::Center, "Center"),
                (H::Right, "Right"),
            ] {
                if ui
                    .selectable_label(across == align, l10n::text(ui.ctx(), name))
                    .clicked()
                {
                    style.horizontal_align = (align != H::Left).then_some(align);
                }
            }
            let down = style.vertical_align.unwrap_or(V::Middle);
            for (align, name) in [
                (V::Top, "Top"),
                (V::Middle, "Middle"),
                (V::Bottom, "Bottom"),
            ] {
                if ui
                    .selectable_label(down == align, l10n::text(ui.ctx(), name))
                    .clicked()
                {
                    style.vertical_align = (align != V::Middle).then_some(align);
                }
            }
            if ui
                .add_enabled(
                    style != TableCellStyle::default(),
                    egui::Button::new(l10n::text(ui.ctx(), "Reset cell")),
                )
                .clicked()
            {
                style = TableCellStyle::default();
            }
        });
        if style != opened {
            table.set_cell(row, column, style);
        }
        *text = join(&grid);
        first.expect("a table has at least one cell")
    }

    /// Lays the table's cells out with its top left corner at `origin`: each cell's text in its
    /// own colour or the table's, each column as wide as set or as its widest text, and each row
    /// as high as set or as one line of text.
    pub(crate) fn table_layout(&self, painter: &egui::Painter, origin: Pos2) -> TableLayout {
        let text_color = self.text_box_colors(painter.ctx()).text;
        let config = self.table_config();
        let cells: Vec<Vec<_>> = cells(&self.table_text())
            .into_iter()
            .enumerate()
            .map(|(r, row)| {
                row.into_iter()
                    .enumerate()
                    .map(|(c, text)| {
                        let color = config.cell(r, c).text_color.map_or(text_color, rgba);
                        self.annotation_galley(painter, text, color)
                    })
                    .collect()
            })
            .collect();
        let sizing = &DESIGN_TOKENS.sizing.annotation;
        let line_height = painter
            .layout_no_wrap("Ag".into(), self.annotation_font(), text_color)
            .size()
            .y;
        let row_height = sizing
            .table_cell_height
            .max(line_height + 2.0 * DESIGN_TOKENS.spacing.xs);
        let xs = edges(
            origin.x,
            (0..cells[0].len()).map(|c| {
                config.column_width(c).unwrap_or_else(|| {
                    cells
                        .iter()
                        .map(|row| row[c].size().x + 2.0 * DESIGN_TOKENS.spacing.sm)
                        .fold(sizing.table_cell_width, f32::max)
                })
            }),
        );
        let ys = edges(
            origin.y,
            (0..cells.len()).map(|r| config.row_height(r).unwrap_or(row_height)),
        );
        TableLayout { cells, xs, ys }
    }

    /// Renders the grid in the table's border colour over its background and the cell
    /// backgrounds, each cell's text aligned and clipped in its cell.
    pub(crate) fn render_table(&self, painter: &egui::Painter) {
        let Some(&origin) = self.points.first() else {
            return;
        };
        let colors = self.text_box_colors(painter.ctx());
        let config = self.table_config();
        let TableLayout { cells, xs, ys } = self.table_layout(painter, origin);
        let (rows, cols) = (ys.len() - 1, xs.len() - 1);
        let cell_rect = |r: usize, c: usize| {
            Rect::from_min_max(Pos2::new(xs[c], ys[r]), Pos2::new(xs[c + 1], ys[r + 1]))
        };
        let table_rect = Rect::from_min_max(origin, Pos2::new(xs[cols], ys[rows]));
        let corner = DESIGN_TOKENS.rounding.md;

        // Background, then the cells' own
        if colors.fill != Color32::TRANSPARENT {
            painter.rect_filled(table_rect, corner, colors.fill);
        }
        let round = |at_corner: bool| if at_corner { corner as u8 } else { 0 };
        for r in 0..rows {
            for c in 0..cols {
                if let Some(background) = config.cell(r, c).background {
                    // A corner cell keeps inside the table's rounded border.
                    let (top, bottom) = (r == 0, r + 1 == rows);
                    let (left, right) = (c == 0, c + 1 == cols);
                    let radius = CornerRadius {
                        nw: round(top && left),
                        ne: round(top && right),
                        sw: round(bottom && left),
                        se: round(bottom && right),
                    };
                    painter.rect_filled(cell_rect(r, c), radius, rgba(background));
                }
            }
        }

        // Grid lines
        if !colors.border.is_empty() {
            for &y in &ys[1..rows] {
                painter.hline(egui::Rangef::new(xs[0], xs[cols]), y, colors.border);
            }
            for &x in &xs[1..cols] {
                painter.vline(x, egui::Rangef::new(ys[0], ys[rows]), colors.border);
            }
        }

        // Cell text, aligned inside the padding and clipped to its cell
        let (pad_x, pad_y) = (DESIGN_TOKENS.spacing.sm, DESIGN_TOKENS.spacing.xs);
        for (r, row) in cells.into_iter().enumerate() {
            for (c, galley) in row.into_iter().enumerate() {
                let (rect, style, size) = (cell_rect(r, c), config.cell(r, c), galley.size());
                let x = match style.horizontal_align {
                    None | Some(LineTextHorizontalAlign::Left) => rect.left() + pad_x,
                    Some(LineTextHorizontalAlign::Center) => {
                        (rect.left() + rect.right() - size.x) / 2.0
                    }
                    Some(LineTextHorizontalAlign::Right) => rect.right() - pad_x - size.x,
                };
                let y = match style.vertical_align {
                    None | Some(LineTextVerticalAlign::Middle) => {
                        rect.top() + (rect.height() - size.y) / 2.0
                    }
                    Some(LineTextVerticalAlign::Top) => rect.top() + pad_y,
                    Some(LineTextVerticalAlign::Bottom) => rect.bottom() - pad_y - size.y,
                };
                let pos = Pos2::new(x, y);
                let mut bounds = galley.mesh_bounds.translate(pos.to_vec2());
                if self.font_weight == FontWeight::Bold {
                    bounds.max.x += BOLD_OFFSET;
                }
                // Fitted cells share the pane's paint batch; overflow still clips to its cell.
                let clip = if rect.contains_rect(bounds) {
                    painter.clip_rect()
                } else {
                    rect
                };
                painter
                    .with_clip_rect(clip)
                    .galley(pos, galley, colors.text);
            }
        }

        // Border
        if !colors.border.is_empty() {
            painter.rect_stroke(table_rect, corner, colors.border, StrokeKind::Middle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, LayerId, Painter, RawInput, Shape, Stroke};

    /// The drawing of a table as an earlier release saved it, before tables had settings.
    const SAVED_TABLE: &str = r#"{"label":null,"pane":null,"selectable":true,"saveable":true,"listed":true,"group":null,"folder":[],"custom_tool":null,"custom_settings":null,"tool_type":"Table","screen_anchor":null,"color":[219,219,219,255],"stroke_width":3.0,"text":null,"post":null,"line_style":"Solid","fill_color":null,"locked":false,"visible":true,"font_size":14.0,"font_weight":"Normal","italic":false,"text_color":null,"background":true,"border":true,"border_color":null,"text_wrap":false,"text_wrap_width":200.0,"text_follows_zoom":false,"text_zoom_spacing":null,"note_text_style":null,"extend_left":false,"extend_right":false,"arrow_start_style":"None","arrow_end_style":"None","timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"price_axis_label":true,"time_axis_label":true,"line_measurements":{"persistent":false,"price_change":false,"percent_change":false,"pip_change":false,"bar_count":false,"elapsed_time":false,"length":false,"inclination":false,"midpoint":false,"price_labels":false,"position":"Right","text_horizontal_align":"Center","text_vertical_align":"Bottom","text_orientation":"Vertical","across_panes":false},"position_size":null,"position_sizing":{"point_value":1.0,"lot_size":1.0,"quantity_step":0.0,"leverage":1.0},"position_readout":{"persistent":false,"condensed":false,"account_currency":null,"conversion_rate":null,"quantity_decimals":null},"position_target_color":null,"position_stop_color":null,"position_style":{"line_color":null,"line_width":1.0,"text_color":null,"font_size":12.0,"label_background":true,"label_color":null,"fill":true,"border":false,"border_color":[102,123,139,255]},"marker":{"show_text":true,"glyph":"🙂","show_glyph":false,"glyph_color":null},"fib_config":null,"volume_profile":null,"regression_trend":null,"vwap":null,"gann":null,"ghost_feed":null,"channel_midline":false,"anchor_price_style":null,"middle_line":{"color":null,"width":1.0,"line_style":"Dashed"},"theme_shade":true,"image":null,"closed":false,"pitchfork":null,"elliott_degree":"Minor","bars_pattern":null,"z_order":0}"#;

    /// Guards #984 and #1399: cells and empty tables use a neutral grid with equal
    /// 22 px rows, thin borders and padded, left-aligned text in every theme. The default light
    /// grey keeps 4:1 on the theme: grey 127 is the first shade towards black that reaches it on
    /// white, 1.05 / (0.2122 + 0.05) = 4.004. A table saved before tables had settings reads
    /// back with its text and paints exactly as before.
    #[test]
    fn tables_paint_their_own_cells() {
        for (name, theme) in crate::theme::presets() {
            let grid = if name == "light" {
                Color32::from_gray(127)
            } else {
                Color32::from_gray(219)
            };
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
                let text_json = serde_json::to_string(&text).unwrap();
                let saved =
                    SAVED_TABLE.replace(r#""text":null"#, &format!(r#""text":{text_json}"#));
                let mut table: Drawing = serde_json::from_str(&saved).unwrap();
                assert_eq!((table.text.as_deref(), &table.table), (text, &None));
                table.points = vec![Pos2::new(10.0, 10.0)];
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
                                assert_eq!(rect.stroke, Stroke::new(1.0, grid));
                            }
                        }
                        Shape::LineSegment { stroke, .. } => {
                            assert_eq!(*stroke, Stroke::new(1.0, grid));
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

    /// A cell's text as painted.
    struct PaintedText {
        text: String,
        pos: Pos2,
        size: egui::Vec2,
        color: Color32,
        clip: Rect,
    }

    /// What a table paints through the drawing renderer, so that its style pass runs too.
    #[derive(Default)]
    struct Painted {
        texts: Vec<PaintedText>,
        /// Each filled rect with its fill and corners, in paint order.
        fills: Vec<(Rect, Color32, CornerRadius)>,
        lines: Vec<([Pos2; 2], Stroke)>,
        /// The stroke of each stroked rect.
        borders: Vec<Stroke>,
    }

    fn paint(table: &Drawing) -> Painted {
        let ctx = Context::default();
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let view = crate::drawings::rendering::ChartView {
                price_formatter: None,
                thousands_sep: None,
                time_formatter: None,
                bar_seconds: 0,
                instrument: None,
                bars: &[],
                revision: 0,
                bar_to_x: &|x| x,
                bar_spacing: 1.0,
                price_to_y: &|p| p as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: None,
                timeframe: None,
                finer: &[],
            };
            table.render_original(ui.painter(), pane, &view);
        });
        let mut painted = Painted::default();
        for clipped in std::mem::take(&mut output.shapes) {
            match clipped.shape {
                Shape::Text(text) if !text.galley.text().is_empty() => {
                    painted.texts.push(PaintedText {
                        text: text.galley.text().to_owned(),
                        pos: text.pos,
                        size: text.galley.size(),
                        color: text.galley.job.sections[0].format.color,
                        clip: clipped.clip_rect,
                    })
                }
                Shape::Rect(rect) => {
                    if rect.fill != Color32::TRANSPARENT {
                        painted
                            .fills
                            .push((rect.rect, rect.fill, rect.corner_radius));
                    }
                    if !rect.stroke.is_empty() {
                        painted.borders.push(rect.stroke);
                    }
                }
                Shape::LineSegment { points, stroke } => painted.lines.push((points, stroke)),
                _ => {}
            }
        }
        output.drop_without_applying_deltas();
        painted
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// Whether `rect` runs from `min` to `max`.
    fn spans(rect: Rect, min: (f32, f32), max: (f32, f32)) -> bool {
        near(rect.min.x, min.0)
            && near(rect.min.y, min.1)
            && near(rect.max.x, max.0)
            && near(rect.max.y, max.1)
    }

    /// Guards #3581: the table's text, background and border colours, each cell's text colour,
    /// background and alignment, set column widths and row heights and the switches for the
    /// background and the border reach the painted table, and text that does not fit its cell is
    /// clipped to it. Fitted text keeps the pane clip so it shares the pane's paint batch.
    /// Unset columns stay 70 wide and rows 22 high at the default 14 px font.
    #[test]
    fn table_settings_reach_the_painted_cells() {
        use crate::drawings::{
            DrawingToolType, LineTextHorizontalAlign as H, LineTextVerticalAlign as V,
        };
        type Case = (&'static str, &'static str, fn(&mut Drawing), fn(&Painted));
        let cases: [Case; 5] = [
            (
                "table colours",
                "A\tB\nC\tD",
                |table| {
                    table.text_color = Some([255, 0, 0, 255]);
                    table.fill_color = Some([0, 0, 255, 128]);
                    table.border_color = Some([0, 128, 0, 255]);
                },
                |painted| {
                    let green = Stroke::new(1.0, Color32::from_rgb(0, 128, 0));
                    assert_eq!(painted.texts.len(), 4);
                    assert!(painted.texts.iter().all(|text| text.color == Color32::RED));
                    let [(rect, fill, _)] = painted.fills[..] else {
                        panic!("one background: {:?}", painted.fills);
                    };
                    assert!(spans(rect, (10.0, 10.0), (150.0, 54.0)), "{rect:?}");
                    assert_eq!(fill, Color32::from_rgba_unmultiplied(0, 0, 255, 128));
                    assert_eq!(painted.lines.len(), 2);
                    assert!(painted.lines.iter().all(|(_, stroke)| *stroke == green));
                    assert_eq!(painted.borders, [green]);
                },
            ),
            (
                "cell style",
                "A\tB\nC\tD",
                |table| {
                    // The table's fill does not cover the cell's own background.
                    table.fill_color = Some([0, 0, 255, 128]);
                    let mut config = TableConfig::default();
                    let cell = TableCellStyle {
                        text_color: Some([255, 255, 0, 255]),
                        background: Some([0, 0, 0, 255]),
                        ..Default::default()
                    };
                    config.cells = vec![vec![TableCellStyle::default(), cell]];
                    table.table = Some(Box::new(config));
                },
                |painted| {
                    let theme_text = crate::theme::Theme::default().ui.text;
                    assert_eq!(painted.texts.len(), 4);
                    for text in &painted.texts {
                        let color = if text.text == "B" {
                            Color32::YELLOW
                        } else {
                            theme_text
                        };
                        assert_eq!(text.color, color, "{}", text.text);
                    }
                    let [_, (rect, fill, corners)] = painted.fills[..] else {
                        panic!("a background, then the cell's: {:?}", painted.fills);
                    };
                    assert!(spans(rect, (80.0, 10.0), (150.0, 32.0)), "{rect:?}");
                    assert_eq!(fill, Color32::BLACK);
                    let top_right = CornerRadius {
                        nw: 0,
                        ne: 6,
                        sw: 0,
                        se: 0,
                    };
                    assert_eq!(corners, top_right);
                },
            ),
            (
                "alignment",
                "A\tA\tA\nA\tA\tA\nA\tA\tA",
                |table| {
                    table.table = Some(Box::new(TableConfig {
                        column_widths: vec![Some(100.0); 3],
                        row_heights: vec![Some(40.0); 3],
                        cells: [V::Top, V::Middle, V::Bottom]
                            .map(|vertical| {
                                [H::Left, H::Center, H::Right]
                                    .map(|horizontal| TableCellStyle {
                                        horizontal_align: Some(horizontal),
                                        vertical_align: Some(vertical),
                                        ..Default::default()
                                    })
                                    .to_vec()
                            })
                            .to_vec(),
                        ..Default::default()
                    }));
                },
                |painted| {
                    let [(rect, ..)] = painted.fills[..] else {
                        panic!("one background: {:?}", painted.fills);
                    };
                    assert!(spans(rect, (10.0, 10.0), (310.0, 130.0)), "{rect:?}");
                    assert_eq!(painted.texts.len(), 9);
                    for (cell, text) in painted.texts.iter().enumerate() {
                        let (r, c) = (cell / 3, cell % 3);
                        let (pos, size) = (text.pos, text.size);
                        let x = [pos.x, pos.x + size.x / 2.0, pos.x + size.x][c];
                        let y = [pos.y, pos.y + size.y / 2.0, pos.y + size.y][r];
                        assert!(near(x, [14.0, 160.0, 306.0][c]), "{r} {c}: x {x}");
                        assert!(near(y, [12.0, 70.0, 128.0][r]), "{r} {c}: y {y}");
                    }
                },
            ),
            (
                "sizes and clipping",
                "Symbol\tLast\tA cell much wider than thirty points\nAAPL\t189.50",
                |table| {
                    table.table = Some(Box::new(TableConfig {
                        column_widths: vec![Some(120.0), None, Some(30.0)],
                        row_heights: vec![None, Some(50.0)],
                        ..Default::default()
                    }));
                },
                |painted| {
                    let [(rect, ..)] = painted.fills[..] else {
                        panic!("one background: {:?}", painted.fills);
                    };
                    assert!(spans(rect, (10.0, 10.0), (230.0, 82.0)), "{rect:?}");
                    let mut lines = painted.lines.iter().map(|(points, _)| *points);
                    let row_rule = lines.next().expect("one row rule");
                    assert!(near(row_rule[0].y, 32.0) && near(row_rule[1].y, 32.0));
                    assert!(near(row_rule[0].x, 10.0) && near(row_rule[1].x, 230.0));
                    for x in [130.0, 200.0] {
                        let column_rule = lines.next().expect("two column rules");
                        assert!(near(column_rule[0].x, x) && near(column_rule[1].x, x));
                        assert!(near(column_rule[0].y, 10.0) && near(column_rule[1].y, 82.0));
                    }
                    assert_eq!(lines.next(), None);
                    let text = |name: &str| {
                        (painted.texts.iter())
                            .find(|text| text.text == name)
                            .expect("painted text")
                    };
                    let long = text("A cell much wider than thirty points");
                    assert!(
                        spans(long.clip, (200.0, 10.0), (230.0, 32.0)),
                        "{:?}",
                        long.clip
                    );
                    assert_eq!(
                        text("Symbol").clip,
                        Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
                    );
                    let price = text("189.50");
                    assert!(near(price.pos.x, 134.0), "{:?}", price.pos);
                    assert!(
                        near(price.pos.y + price.size.y / 2.0, 57.0),
                        "{:?}",
                        price.pos
                    );
                },
            ),
            (
                "box off",
                "A\tB",
                |table| {
                    table.background = false;
                    table.border = false;
                },
                |painted| {
                    assert_eq!(painted.texts.len(), 2);
                    assert!(painted.fills.is_empty(), "{:?}", painted.fills);
                    assert!(painted.lines.is_empty(), "{:?}", painted.lines);
                    assert!(painted.borders.is_empty(), "{:?}", painted.borders);
                },
            ),
        ];
        for (name, text, setup, check) in cases {
            let mut table = Drawing::new(1, DrawingToolType::Table);
            table.points = vec![Pos2::new(10.0, 10.0)];
            table.text = Some(text.into());
            setup(&mut table);
            eprintln!("{name}");
            check(&paint(&table));
        }
    }

    /// Guards #3581: the editor inserts a row below the focused middle cell and deletes its
    /// column, moving each column's width, each row's height and each cell's style with its
    /// cells, and styles the cell that keeps the focus.
    #[test]
    fn editor_inserts_and_deletes_at_the_focused_cell() {
        use egui_kittest::{
            Harness,
            kittest::{By, Queryable},
        };
        let plain = TableCellStyle::default();
        let red = TableCellStyle {
            background: Some([255, 0, 0, 255]),
            ..plain
        };
        let table = TableConfig {
            column_widths: vec![None, Some(90.0), Some(120.0)],
            row_heights: vec![None, Some(30.0), Some(40.0)],
            cells: vec![vec![], vec![], vec![plain, plain, red]],
            ..Default::default()
        };
        let mut harness = Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(
                |ui, (text, table): &mut (String, TableConfig)| {
                    let colors = [Color32::WHITE, Color32::BLACK];
                    Drawing::edit_table(ui, text, table, colors, egui::Id::unique("table"));
                },
                ("a\tb\tc\nd\te\tf\ng\th\ti".to_owned(), table),
            );
        harness.run();
        harness
            .get(By::new().role(egui::Role::TextInput).value("e"))
            .click();
        harness.run();
        for button in ["Insert row below", "Delete column", "Right"] {
            harness.get_by_label(button).click();
            harness.run();
        }
        let (text, table) = harness.state();
        assert_eq!(text, "a\tc\nd\tf\n\t\ng\ti");
        assert_eq!(table.column_widths, [None, Some(120.0)]);
        assert_eq!(table.row_heights, [None, Some(30.0), None, Some(40.0)]);
        let right = TableCellStyle {
            horizontal_align: Some(LineTextHorizontalAlign::Right),
            ..plain
        };
        assert_eq!(
            table.cells,
            [vec![], vec![plain, right], vec![], vec![plain, red]]
        );
    }
}
