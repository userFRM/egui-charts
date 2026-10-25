//! Labelled study tables, independent of bar-indexed plots.

/// A cell in a study table.
#[derive(Clone, Debug, PartialEq)]
pub enum StudyCell {
    /// A row label or other text.
    Text(String),
    /// A percentage, coloured by sign; `None` displays an unavailable value.
    Percent(Option<f64>),
}

/// A labelled table drawn in the lower right of the study's pane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StudyTable {
    /// Caption above the column headings.
    pub title: String,
    /// One heading per column.
    pub columns: Vec<String>,
    /// Cells in each row, in column order.
    pub rows: Vec<Vec<StudyCell>>,
}

#[cfg(feature = "widget")]
pub(crate) fn paint(painter: &egui::Painter, rect: egui::Rect, table: &StudyTable) -> egui::Rect {
    use crate::theme::ThemeExt;
    use egui::{Align2, FontId, Rect, Stroke, StrokeKind, pos2, vec2};

    let theme = painter.ctx().active_theme();
    let font = FontId::proportional(11.0);
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
    let width = widths.iter().sum::<f32>();
    let height = 22.0 * (table.rows.len() + 2) as f32;
    let bounds = Rect::from_min_size(
        pos2(
            (rect.right() - width - 8.0).max(rect.left()),
            (rect.bottom() - height - 8.0).max(rect.top()),
        ),
        vec2(width, height),
    );
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(bounds, 3.0, theme.ui.panel_bg_floating);
    painter.rect_stroke(
        bounds,
        3.0,
        Stroke::new(1.0, theme.ui.border),
        StrokeKind::Inside,
    );
    painter.text(
        bounds.min + vec2(8.0, 11.0),
        Align2::LEFT_CENTER,
        &table.title,
        font.clone(),
        theme.ui.text,
    );
    let mut y = bounds.top() + 22.0;
    for row in std::iter::once(
        table
            .columns
            .iter()
            .cloned()
            .map(StudyCell::Text)
            .collect::<Vec<_>>(),
    )
    .chain(table.rows.iter().cloned())
    {
        let mut x = bounds.left();
        for (cell, width) in row.iter().zip(&widths) {
            let color = match cell {
                StudyCell::Percent(Some(value)) if value.is_finite() && *value != 0.0 => {
                    let color = if *value > 0.0 {
                        theme.chart.candle_up
                    } else {
                        theme.chart.candle_down
                    };
                    let tint =
                        color.gamma_multiply(0.15 + 0.5 * (value.abs() / 10.0).min(1.0) as f32);
                    painter.rect_filled(
                        Rect::from_min_size(pos2(x, y), vec2(*width, 22.0)),
                        0.0,
                        tint,
                    );
                    crate::theme::readable_color(
                        color,
                        theme.ui.text,
                        &[theme.ui.panel_bg_floating.blend(tint)],
                        4.5,
                    )
                }
                _ => theme.ui.text,
            };
            painter.text(
                pos2(x + 8.0, y + 11.0),
                Align2::LEFT_CENTER,
                text(cell),
                font.clone(),
                color,
            );
            x += width;
        }
        y += 22.0;
    }
    bounds
}
