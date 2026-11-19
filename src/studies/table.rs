//! Labelled study tables, independent of bar-indexed plots.

/// A cell in a study table.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StudyCell {
    /// A row label or other text.
    Text(String),
    /// A percentage, coloured by sign; `None` displays an unavailable value.
    Percent(Option<f64>),
}

/// A labelled table drawn in a corner, an edge or the middle of the study's pane.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StudyTable {
    /// Caption above the column headings.
    pub title: String,
    /// One heading per column.
    pub columns: Vec<String>,
    /// Cells in each row, in column order.
    pub rows: Vec<Vec<StudyCell>>,
    /// Background of each cell, row by row and in column order within a row. Cells past the end
    /// of their row's fills, and rows past the end of the list, keep the default cell background.
    pub cell_colors: Vec<Vec<egui::Color32>>,
    /// Background of the column headings, which also frames the table in white, or `None` for
    /// the default.
    pub header_color: Option<egui::Color32>,
    /// Text of the column headings and of cells with a background, or `None` for the default.
    /// Text over a background it does not contrast with turns towards black.
    pub text_color: Option<egui::Color32>,
    /// Where the table sits in the pane.
    pub position: TablePosition,
    /// Size of the table's text.
    pub text_size: TableTextSize,
    /// Width of the table as a share of its pane's width, or `None` to fit its columns. Columns
    /// stretch to fill a wider table and scroll in a narrower one. A share of 0 or less, or NaN,
    /// fits; the table never leaves the pane's margins.
    pub width: Option<f32>,
    /// Height of the table as a share of its pane's height, less any tables stacked below it, or
    /// `None` to fit its rows in at most half that height. Rows stretch to fill a taller table and
    /// scroll in a shorter one. A share of 0 or less, or NaN, fits; the table never leaves the
    /// pane's margins.
    pub height: Option<f32>,
}

/// Where a study table sits in its pane. The `Debug` names are the choices study inputs list. The
/// set is closed: the nine places of a three by three grid.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TablePosition {
    /// Upper left corner.
    TopLeft,
    /// Middle of the upper edge.
    TopCenter,
    /// Upper right corner.
    TopRight,
    /// Middle of the left edge.
    MiddleLeft,
    /// Centre of the pane.
    MiddleCenter,
    /// Middle of the right edge.
    MiddleRight,
    /// Lower left corner.
    BottomLeft,
    /// Middle of the lower edge.
    BottomCenter,
    /// Lower right corner.
    #[default]
    BottomRight,
}

impl TablePosition {
    /// Every position, top row first, each row from left to right.
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::MiddleLeft,
        Self::MiddleCenter,
        Self::MiddleRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];
}

impl std::fmt::Debug for TablePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TopLeft => "Top left",
            Self::TopCenter => "Top center",
            Self::TopRight => "Top right",
            Self::MiddleLeft => "Middle left",
            Self::MiddleCenter => "Middle center",
            Self::MiddleRight => "Middle right",
            Self::BottomLeft => "Bottom left",
            Self::BottomCenter => "Bottom center",
            Self::BottomRight => "Bottom right",
        })
    }
}

/// The text size of a study table. The `Debug` names are the choices study inputs list.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableTextSize {
    /// 8 points.
    Tiny,
    /// 10 points.
    Small,
    /// 11 points.
    #[default]
    Normal,
    /// 14 points.
    Large,
    /// 18 points.
    Huge,
}

impl TableTextSize {
    /// Every size, smallest first.
    pub const ALL: [Self; 5] = [
        Self::Tiny,
        Self::Small,
        Self::Normal,
        Self::Large,
        Self::Huge,
    ];

    /// The font size in points.
    pub fn points(self) -> f32 {
        match self {
            Self::Tiny => 8.0,
            Self::Small => 10.0,
            Self::Normal => 11.0,
            Self::Large => 14.0,
            Self::Huge => 18.0,
        }
    }
}

impl std::fmt::Debug for TableTextSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Tiny => "Tiny",
            Self::Small => "Small",
            Self::Normal => "Normal",
            Self::Large => "Large",
            Self::Huge => "Huge",
        })
    }
}

#[cfg(feature = "widget")]
pub(crate) fn paint(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    table: &StudyTable,
    id: egui::Id,
) -> egui::Rect {
    use crate::theme::ThemeExt;
    use egui::{Align2, FontId, Rect, Stroke, StrokeKind, pos2, vec2};

    let styled = table.header_color.is_some();
    let painter = ui.painter().clone();
    let theme = ui.active_theme();
    let font = FontId::proportional(table.text_size.points());
    // Rows are twice the font size high: 22 points at the normal size.
    let mut row_height = 2.0 * table.text_size.points();
    let text = |cell: &StudyCell| match cell {
        StudyCell::Text(text) => text.clone(),
        StudyCell::Percent(Some(value)) if value.is_finite() => format!("{value:+.2}%"),
        StudyCell::Percent(_) => "N/A".into(),
    };
    let mut widths: Vec<_> = table
        .columns
        .iter()
        .map(|column| {
            painter
                .layout_no_wrap(column.clone(), font.clone(), theme.ui.text)
                .size()
                .x
                + 16.0
        })
        .collect();
    for row in &table.rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = width.max(
                painter
                    .layout_no_wrap(text(cell), font.clone(), theme.ui.text)
                    .size()
                    .x
                    + 16.0,
            );
        }
    }
    let mut width = widths.iter().sum::<f32>();
    let mut height = row_height * (table.rows.len() + 2) as f32;
    let mut available = rect.shrink(8.0);
    available.min.y += 24.0;
    if !available.is_positive() || rect.height() < 44.0 {
        return Rect::from_min_size(rect.min, egui::Vec2::ZERO);
    }
    let title_width = painter
        .layout_no_wrap(table.title.clone(), font.clone(), theme.ui.text)
        .size()
        .x
        + 16.0;
    // A share of 0 or less, or NaN, fits the cells.
    let shares = (
        table.width.filter(|share| *share > 0.0),
        table.height.filter(|share| *share > 0.0),
    );
    let size = vec2(
        shares
            .0
            .map_or(width.max(title_width), |share| rect.width() * share)
            .min(available.width()),
        shares
            .1
            .map_or(height.min(rect.height() * 0.5), |share| {
                rect.height() * share
            })
            .min(available.height()),
    );
    // Cells stretch to fill a table sized larger than they are.
    if shares.0.is_some() && width < size.x {
        let scale = size.x / width;
        widths.iter_mut().for_each(|column| *column *= scale);
        width = size.x;
    }
    if shares.1.is_some() && height < size.y {
        row_height *= size.y / height;
        height = size.y;
    }
    let (across, down) = match table.position {
        TablePosition::TopLeft => (0.0, 0.0),
        TablePosition::TopCenter => (0.5, 0.0),
        TablePosition::TopRight => (1.0, 0.0),
        TablePosition::MiddleLeft => (0.0, 0.5),
        TablePosition::MiddleCenter => (0.5, 0.5),
        TablePosition::MiddleRight => (1.0, 0.5),
        TablePosition::BottomLeft => (0.0, 1.0),
        TablePosition::BottomCenter => (0.5, 1.0),
        TablePosition::BottomRight => (1.0, 1.0),
    };
    let bounds = Rect::from_min_size(
        available.min + (available.size() - size) * vec2(across, down),
        size,
    );
    let painter = painter.with_clip_rect(bounds.intersect(ui.clip_rect()));
    painter.rect_filled(bounds, 3.0, theme.ui.panel_bg_floating);
    painter.rect_stroke(
        bounds,
        3.0,
        Stroke::new(
            if styled { 2.0 } else { 1.0 },
            if styled {
                egui::Color32::WHITE
            } else {
                theme.ui.border
            },
        ),
        StrokeKind::Inside,
    );
    painter.text(
        bounds.min + vec2(8.0, row_height / 2.0),
        Align2::LEFT_CENTER,
        &table.title,
        font.clone(),
        theme.ui.text,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(Rect::from_min_max(
                bounds.min + vec2(0.0, row_height),
                bounds.max,
            )),
    );
    child.set_clip_rect(painter.clip_rect());
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (content, _) =
                ui.allocate_exact_size(vec2(width, height - row_height), egui::Sense::hover());
            let painter = ui.painter();
            let mut y = content.top();
            for (row_index, row) in std::iter::once(
                table
                    .columns
                    .iter()
                    .cloned()
                    .map(StudyCell::Text)
                    .collect::<Vec<_>>(),
            )
            .chain(table.rows.iter().cloned())
            .enumerate()
            {
                let mut x = content.left();
                for (column, (cell, width)) in row.iter().zip(&widths).enumerate() {
                    let cell_rect = Rect::from_min_size(pos2(x, y), vec2(*width, row_height));
                    let fill = match row_index {
                        0 => table.header_color,
                        row => table
                            .cell_colors
                            .get(row - 1)
                            .and_then(|fills| fills.get(column))
                            .copied(),
                    };
                    let color = if let Some(fill) = fill {
                        painter.rect_filled(cell_rect, 0.0, fill);
                        if styled && row_index > 0 {
                            painter.rect_stroke(
                                cell_rect,
                                0.0,
                                Stroke::new(1.0, egui::Color32::WHITE),
                                StrokeKind::Inside,
                            );
                        }
                        let text = table.text_color.unwrap_or(egui::Color32::WHITE);
                        crate::theme::readable_color(
                            text,
                            egui::Color32::BLACK,
                            &[theme.ui.panel_bg_floating.blend(fill)],
                            4.5,
                        )
                    } else {
                        match cell {
                            StudyCell::Percent(Some(value)) if value.is_finite() => {
                                let color = if *value >= 0.0 {
                                    egui::Color32::from_rgb(8, 153, 129)
                                } else {
                                    egui::Color32::from_rgb(242, 55, 69)
                                };
                                let tint = color.gamma_multiply(
                                    0.1 + if table.title == "Seasonality" {
                                        0.4
                                    } else {
                                        0.2
                                    } * (value.abs() / 10.0).min(1.0) as f32,
                                );
                                painter.rect_filled(cell_rect, 0.0, tint);
                                crate::theme::readable_color(
                                    color,
                                    theme.ui.text,
                                    &[theme.ui.panel_bg_floating.blend(tint)],
                                    4.5,
                                )
                            }
                            _ if matches!(table.title.as_str(), "Performance" | "Seasonality") => {
                                let alpha = if matches!(cell, StudyCell::Percent(None)) {
                                    if table.title == "Seasonality" {
                                        128
                                    } else {
                                        26
                                    }
                                } else {
                                    51
                                };
                                painter.rect_filled(
                                    cell_rect,
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(120, 123, 134, alpha),
                                );
                                theme.ui.text
                            }
                            _ => theme.ui.text,
                        }
                    };
                    painter.text(
                        pos2(x + 8.0, y + row_height / 2.0),
                        Align2::LEFT_CENTER,
                        text(cell),
                        font.clone(),
                        color,
                    );
                    x += width;
                }
                y += row_height;
            }
        });
    if matches!(
        table.position,
        TablePosition::BottomLeft | TablePosition::BottomCenter | TablePosition::BottomRight
    ) {
        bounds
    } else {
        // shortcut: only tables at the bottom stack upwards, so two tables at the top or in the
        // middle of one pane overlap; give them their own stacks when a pane needs several.
        Rect::from_min_size(pos2(rect.min.x, rect.max.y + 8.0), egui::Vec2::ZERO)
    }
}

#[cfg(all(test, feature = "widget"))]
mod tests {
    use super::*;

    /// A narrow or short pane keeps overflowing columns and years within a scrollable table, at
    /// the table's position and text size. Only a table at the bottom takes the space below the
    /// next table.
    #[test]
    fn overflowing_tables_fit_the_pane() {
        use egui::{Event, MouseWheelUnit, RawInput, Rect, Shape, TouchPhase, vec2};

        let lower_right = StudyTable {
            title: "Seasonality".into(),
            columns: (0..13).map(|i| format!("Column {i}")).collect(),
            rows: (0..40)
                .map(|i| {
                    let mut row = vec![StudyCell::Percent(Some(12.34)); 13];
                    row[12] = StudyCell::Text(format!("Last {i}"));
                    row
                })
                .collect(),
            ..Default::default()
        };
        let upper_left = StudyTable {
            position: TablePosition::TopLeft,
            text_size: TableTextSize::Large,
            cell_colors: vec![vec![
                egui::Color32::from_rgb(1, 2, 3),
                egui::Color32::from_rgb(4, 5, 6),
            ]],
            ..lower_right.clone()
        };
        for ((_, theme), table) in crate::theme::presets()
            .into_iter()
            .flat_map(|theme| [(theme.clone(), &lower_right), (theme, &upper_left)])
        {
            let top = table.position == TablePosition::TopLeft;
            for width in [240.0, 800.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 300.0));
                for frame in 0..20 {
                    // The wheel scrolls the table under the pointer.
                    let pointer = if top {
                        rect.left_top() + vec2(100.0, 100.0)
                    } else {
                        rect.right_bottom() - vec2(100.0, 50.0)
                    };
                    let mut events = vec![Event::PointerMoved(pointer)];
                    if frame == 1 {
                        events.push(Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta: vec2(-2000.0, -2000.0),
                            phase: TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: Default::default(),
                        });
                    }
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(rect),
                            time: Some(frame as f64 / 60.0),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let bounds = paint(ui, rect, table, ui.scope_id().with("table"));
                            if top {
                                assert_eq!(bounds.top() - 8.0, rect.max.y);
                            } else {
                                assert!(rect.contains_rect(bounds), "{bounds:?} outside {rect:?}");
                            }
                        },
                    );
                    if frame == 0 && top {
                        for fill in &table.cell_colors[0] {
                            let cells = output
                                .shapes
                                .iter()
                                .filter(|clipped| {
                                    matches!(
                                        &clipped.shape, Shape::Rect(rect) if rect.fill == *fill
                                    )
                                })
                                .count();
                            assert_eq!(
                                cells, 1,
                                "each column must paint its own fill at width {width}"
                            );
                        }
                    }
                    let title = output
                        .shapes
                        .iter()
                        .find_map(|clipped| match &clipped.shape {
                            Shape::Text(text) if text.galley.text() == "Seasonality" => Some(text),
                            _ => None,
                        });
                    let title = title.expect("title painted");
                    assert_eq!(
                        title.galley.job.sections[0].format.font_id.size,
                        table.text_size.points()
                    );
                    // Below the pane's 8-point margin and its 24-point legend row at the top.
                    assert_eq!(title.galley.rect.top() + title.pos.y < 50.0, top);
                    if frame == 19 {
                        assert!(output.shapes.iter().any(|clipped| matches!(&clipped.shape,
                            Shape::Text(text) if text.galley.text() == "Last 39"
                                && clipped.clip_rect.contains_rect(Rect::from_min_size(text.pos, text.galley.size()))
                        )), "the last column of the last year must be reachable at width {width}");
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    /// A width or height share sizes the table to that share of its pane, within the pane's
    /// margins, its cells stretched to fill it, while a NaN share fits the cells. Text over a
    /// translucent cell background reads against the colour shown, and a table without a header
    /// colour draws no white grid round its cells.
    #[test]
    fn sized_tables_take_their_share_of_the_pane() {
        use egui::{Color32, Pos2, RawInput, Rect, Shape, Stroke, pos2, vec2};

        let faint = crate::studies::palette::UP.gamma_multiply(0.1);
        let sized = StudyTable {
            title: "Sized".into(),
            columns: vec!["A".into(), "B".into()],
            rows: vec![vec![
                StudyCell::Text("faint".into()),
                StudyCell::Text("last".into()),
            ]],
            cell_colors: vec![vec![faint, Color32::GOLD]],
            ..Default::default()
        };
        // An 800 by 300 pane leaves (8, 32) to (792, 292) inside its 8-point margin and below its
        // 24-point legend row; three 22-point rows (title, headings, one row) fit in 66 points.
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 300.0));
        for (_, theme) in crate::theme::presets() {
            for (position, width, height, [left, top, right, bottom]) in [
                (
                    TablePosition::BottomRight,
                    Some(0.5),
                    Some(0.5),
                    [392.0, 142.0, 792.0, 292.0],
                ),
                (
                    TablePosition::BottomLeft,
                    Some(0.25),
                    Some(0.25),
                    [8.0, 217.0, 208.0, 292.0],
                ),
                // Twice the pane's width stays within its margins; NaN fits the rows.
                (
                    TablePosition::BottomCenter,
                    Some(2.0),
                    Some(f32::NAN),
                    [8.0, 226.0, 792.0, 292.0],
                ),
            ] {
                let table = StudyTable {
                    position,
                    width,
                    height,
                    ..sized.clone()
                };
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut bounds = Rect::NOTHING;
                let mut shapes = Vec::new();
                for time in [0.0, 1.0 / 60.0] {
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(rect),
                            time: Some(time),
                            ..Default::default()
                        },
                        |ui| bounds = paint(ui, rect, &table, ui.scope_id().with("table")),
                    );
                    shapes = output
                        .shapes
                        .iter()
                        .map(|clipped| clipped.shape.clone())
                        .collect();
                    output.drop_without_applying_deltas();
                }
                assert_eq!(
                    bounds,
                    Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                    "{position:?}"
                );
                let last = shapes.iter().find_map(|shape| match shape {
                    Shape::Rect(cell) if cell.fill == Color32::GOLD => Some(cell.rect),
                    _ => None,
                });
                let last = last.expect("last cell painted");
                assert!(
                    (last.max - bounds.max).length() < 0.5,
                    "{position:?}: the last cell {last:?} ends inside {bounds:?}"
                );
                let text = shapes.iter().find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text() == "faint" => {
                        Some(text.galley.job.sections[0].format.color)
                    }
                    _ => None,
                });
                let text = text.expect("text over the faint fill");
                let shown = theme.ui.panel_bg_floating.blend(faint);
                assert!(
                    crate::theme::contrast(text, shown) >= 4.5,
                    "{position:?}: {text:?} over {shown:?}"
                );
                assert!(
                    !shapes.iter().any(|shape| matches!(shape,
                        Shape::Rect(cell) if cell.stroke == Stroke::new(1.0, Color32::WHITE))),
                    "a white grid without a header colour"
                );
            }
        }
    }
}
