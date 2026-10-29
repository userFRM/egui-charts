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
fn rating_color(rating: &str) -> egui::Color32 {
    match rating {
        "Strong Buy" => super::palette::BLUE,
        "Buy" => egui::Color32::from_rgb(91, 156, 246),
        "Strong Sell" => super::palette::BRIGHT_RED,
        "Sell" => egui::Color32::from_rgb(239, 154, 154),
        _ => super::palette::DISABLED,
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

    let rating_table = table.title == "Technical Ratings";
    let painter = ui.painter().clone();
    let theme = ui.active_theme();
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
    let size = vec2(
        width.max(title_width).min(available.width()),
        height.min(available.height()).min(rect.height() * 0.5),
    );
    let bounds = Rect::from_min_size(available.max - size, size);
    let painter = painter.with_clip_rect(bounds.intersect(ui.clip_rect()));
    painter.rect_filled(bounds, 3.0, theme.ui.panel_bg_floating);
    painter.rect_stroke(
        bounds,
        3.0,
        Stroke::new(
            if rating_table { 2.0 } else { 1.0 },
            if rating_table {
                egui::Color32::WHITE
            } else {
                theme.ui.border
            },
        ),
        StrokeKind::Inside,
    );
    painter.text(
        bounds.min + vec2(8.0, 11.0),
        Align2::LEFT_CENTER,
        &table.title,
        font.clone(),
        theme.ui.text,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(Rect::from_min_max(bounds.min + vec2(0.0, 22.0), bounds.max)),
    );
    child.set_clip_rect(painter.clip_rect());
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (content, _) =
                ui.allocate_exact_size(vec2(width, height - 22.0), egui::Sense::hover());
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
                for (cell, width) in row.iter().zip(&widths) {
                    let cell_rect = Rect::from_min_size(pos2(x, y), vec2(*width, 22.0));
                    let color = match cell {
                        _ if rating_table && row_index == 0 => {
                            painter.rect_filled(
                                cell_rect,
                                0.0,
                                egui::Color32::from_rgb(41, 91, 121),
                            );
                            egui::Color32::WHITE
                        }
                        _ if rating_table => {
                            let rating = match row.get(2) {
                                Some(StudyCell::Text(text)) => text.as_str(),
                                _ => "Neutral",
                            };
                            let fill = rating_color(rating);
                            painter.rect_filled(cell_rect, 0.0, fill);
                            painter.rect_stroke(
                                cell_rect,
                                0.0,
                                Stroke::new(1.0, egui::Color32::WHITE),
                                StrokeKind::Inside,
                            );
                            crate::theme::readable_color(
                                egui::Color32::WHITE,
                                egui::Color32::BLACK,
                                &[fill],
                                4.5,
                            )
                        }
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
        });
    bounds
}

#[cfg(all(test, feature = "widget"))]
mod tests {
    use super::*;

    /// A narrow or short pane keeps overflowing columns and years within a scrollable table.
    #[test]
    fn overflowing_tables_fit_the_pane() {
        use egui::{Event, MouseWheelUnit, RawInput, Rect, Shape, TouchPhase, vec2};

        let table = StudyTable {
            title: "Seasonality".into(),
            columns: (0..13).map(|i| format!("Column {i}")).collect(),
            rows: (0..40)
                .map(|i| {
                    let mut row = vec![StudyCell::Percent(Some(12.34)); 13];
                    row[12] = StudyCell::Text(format!("Last {i}"));
                    row
                })
                .collect(),
        };
        for (_, theme) in crate::theme::presets() {
            for width in [240.0, 800.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 300.0));
                for frame in 0..20 {
                    let mut events =
                        vec![Event::PointerMoved(rect.right_bottom() - vec2(100.0, 50.0))];
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
                            let bounds = paint(ui, rect, &table, ui.scope_id().with("table"));
                            assert!(rect.contains_rect(bounds), "{bounds:?} outside {rect:?}");
                        },
                    );
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
}
