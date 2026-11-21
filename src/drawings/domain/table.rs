//! Settings of a table drawing beyond its cell text.

use super::{LineTextHorizontalAlign, LineTextVerticalAlign};
use crate::drawings::validation::{MAX_TABLE_CELL, MIN_TABLE_CELL};
use serde::{Deserialize, Serialize};

/// Column widths, row heights, cell styles and placement of a
/// [`Table`](super::DrawingToolType::Table) drawing, whose cells are the rows and tab-separated
/// fields of [`Drawing::text`](super::Drawing::text). Rows and columns count from the top left,
/// from 0. Entries past the table's last row or column are kept and ignored, and a column, row
/// or cell without an entry is drawn as tables were before these settings.
///
/// ```
/// use egui_charts::drawings::{
///     Drawing, DrawingToolType, LineTextHorizontalAlign, TableCellStyle, TableConfig,
/// };
///
/// let mut table = Drawing::new(1, DrawingToolType::Table);
/// table.text = Some("Symbol\tLast\nAAPL\t189.50".into());
/// let mut header = TableCellStyle::default();
/// header.background = Some([41, 98, 255, 255]);
/// header.text_color = Some([255, 255, 255, 255]);
/// header.horizontal_align = Some(LineTextHorizontalAlign::Center);
/// let mut config = TableConfig::default();
/// config.cells = vec![vec![header; 2]];
/// config.column_widths = vec![None, Some(120.0)];
/// config.screen_anchored = true;
/// table.table = Some(Box::new(config));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TableConfig {
    /// Width of each column in points, from 1 to 10,000. `None`, or no entry, fits the column
    /// to its widest text, and at least 70 points.
    pub column_widths: Vec<Option<f32>>,
    /// Height of each row in points, from 1 to 10,000. `None`, or no entry, fits one line of
    /// text, and at least 22 points.
    pub row_heights: Vec<Option<f32>>,
    /// Style of each cell, row by row. A row may hold fewer cells than the table; a cell
    /// without an entry takes the table's style.
    pub cells: Vec<Vec<TableCellStyle>>,
    /// Whether the table keeps its place in its pane while the chart scrolls and zooms, as
    /// anchored text does. Off, it stays on the bar and price of its anchor.
    pub screen_anchored: bool,
}

/// The settings one cell of a table sets apart from the rest of the table. Each `None` takes
/// the table's own: its text colour and background, the left edge and the middle of the row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct TableCellStyle {
    /// Text colour as RGBA.
    pub text_color: Option<[u8; 4]>,
    /// Background colour as RGBA, painted over the table's.
    pub background: Option<[u8; 4]>,
    /// Where the text sits across the cell: at its left or right edge, inside the padding, or
    /// in its middle.
    pub horizontal_align: Option<LineTextHorizontalAlign>,
    /// Where the text sits down the cell: [`Top`](LineTextVerticalAlign::Top) puts its top
    /// edge at the cell's top, inside the padding, and
    /// [`Bottom`](LineTextVerticalAlign::Bottom) its bottom edge at the cell's bottom.
    pub vertical_align: Option<LineTextVerticalAlign>,
}

/// A stored size a table can be drawn with.
fn drawable(size: Option<&Option<f32>>) -> Option<f32> {
    size.copied()
        .flatten()
        .filter(|size| (1.0..=MAX_TABLE_CELL).contains(size))
}

/// Sets the size at `index`, with unset sizes before it.
fn set_size(sizes: &mut Vec<Option<f32>>, index: usize, size: f32) {
    if sizes.len() <= index {
        sizes.resize(index + 1, None);
    }
    sizes[index] = Some(size);
}

impl TableConfig {
    /// The width set for `column`, if it is one a table can be drawn with.
    pub(crate) fn column_width(&self, column: usize) -> Option<f32> {
        drawable(self.column_widths.get(column))
    }

    /// The height set for `row`, if it is one a table can be drawn with.
    pub(crate) fn row_height(&self, row: usize) -> Option<f32> {
        drawable(self.row_heights.get(row))
    }

    /// Sets the width of `column`.
    pub(crate) fn set_column_width(&mut self, column: usize, width: f32) {
        set_size(&mut self.column_widths, column, width);
    }

    /// Sets the height of `row`.
    pub(crate) fn set_row_height(&mut self, row: usize, height: f32) {
        set_size(&mut self.row_heights, row, height);
    }

    /// Sets each column to its painted width in `widths` times `scale.x`, and each row to its
    /// painted height in `heights` times `scale.y`, from 8 to 10,000 points. An axis scaled by
    /// exactly 1, or by no finite factor, keeps its sizes, so that a table made only taller
    /// keeps the columns that fit their text.
    pub(crate) fn scale(&mut self, widths: &[f32], heights: &[f32], scale: egui::Vec2) {
        let scaled = |size: f32, factor: f32| (size * factor).clamp(MIN_TABLE_CELL, MAX_TABLE_CELL);
        if scale.x.is_finite() && scale.x != 1.0 {
            for (column, &width) in widths.iter().enumerate() {
                self.set_column_width(column, scaled(width, scale.x));
            }
        }
        if scale.y.is_finite() && scale.y != 1.0 {
            for (row, &height) in heights.iter().enumerate() {
                self.set_row_height(row, scaled(height, scale.y));
            }
        }
    }

    /// The style of the cell at `row` and `column`, or the table's for a cell without one.
    pub(crate) fn cell(&self, row: usize, column: usize) -> TableCellStyle {
        self.cells
            .get(row)
            .and_then(|cells| cells.get(column))
            .copied()
            .unwrap_or_default()
    }

    /// Sets the style of the cell at `row` and `column`. Default styles at the end of a row, and
    /// rows without styles at the end of the table, are not kept.
    pub(crate) fn set_cell(&mut self, row: usize, column: usize, style: TableCellStyle) {
        if self.cell(row, column) == style {
            return;
        }
        if self.cells.len() <= row {
            self.cells.resize(row + 1, Vec::new());
        }
        let cells = &mut self.cells[row];
        if cells.len() <= column {
            cells.resize(column + 1, TableCellStyle::default());
        }
        cells[column] = style;
        while cells.last() == Some(&TableCellStyle::default()) {
            cells.pop();
        }
        while self.cells.last().is_some_and(Vec::is_empty) {
            self.cells.pop();
        }
    }

    /// Makes room for a row inserted at `at`: the heights and styles of the rows from `at` on
    /// move down with them.
    pub(crate) fn insert_row(&mut self, at: usize) {
        if at < self.row_heights.len() {
            self.row_heights.insert(at, None);
        }
        if at < self.cells.len() {
            self.cells.insert(at, Vec::new());
        }
    }

    /// Drops the height and styles of the row at `at`; those of the rows below move up.
    pub(crate) fn remove_row(&mut self, at: usize) {
        if at < self.row_heights.len() {
            self.row_heights.remove(at);
        }
        if at < self.cells.len() {
            self.cells.remove(at);
        }
    }

    /// Makes room for a column inserted at `at`: the widths and styles of the columns from `at`
    /// on move right with them.
    pub(crate) fn insert_column(&mut self, at: usize) {
        if at < self.column_widths.len() {
            self.column_widths.insert(at, None);
        }
        for cells in &mut self.cells {
            if at < cells.len() {
                cells.insert(at, TableCellStyle::default());
            }
        }
    }

    /// Drops the width and styles of the column at `at`; those of the columns to its right move
    /// left.
    pub(crate) fn remove_column(&mut self, at: usize) {
        if at < self.column_widths.len() {
            self.column_widths.remove(at);
        }
        for cells in &mut self.cells {
            if at < cells.len() {
                cells.remove(at);
            }
        }
    }

    /// Whether every set width and height is one a table can be drawn with.
    pub(crate) fn is_valid(&self) -> bool {
        (self.column_widths.iter().chain(&self.row_heights))
            .all(|size| size.is_none() || drawable(Some(size)).is_some())
    }
}
