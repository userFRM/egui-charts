//! Chart data table dialog.
//!
//! Lists the loaded bars with their volume and the visible study plots, aligned by chart time.
//! It follows the chart it is shown with, including its replay cutoff, and loads nothing: only
//! bars already loaded appear. The host opens it from
//! [`ContextMenuAction::OpenDataTable`](crate::ui::context_menu::ContextMenuAction::OpenDataTable)
//! and saves the text of [`DataTableAction::Export`].

use crate::localization as l10n;
use crate::model::{Bar, Timeframe};
use crate::ui_kit::buttons::Button;
use crate::ui_kit::modal::Modal;
use crate::widget::{Chart, DataTable, SeriesId};
use chrono::{DateTime, Utc};
use egui::{Align2, Context, FontId, Rect, ScrollArea, Sense, Ui, vec2};
use std::collections::BTreeSet;

/// Width of the time column in logical points.
const TIME_WIDTH: f32 = 210.0;
/// Width of every other column in logical points.
const COLUMN_WIDTH: f32 = 96.0;
const ROW_HEIGHT: f32 = 22.0;

/// What the dialog asks of the host.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataTableAction {
    /// Nothing happened.
    None,
    /// The dialog was closed.
    Close,
    /// CSV text of the displayed columns, for the host to save.
    Export(String),
}

/// What the cached table was built from. A correction to a bar in the middle of the history
/// that changes none of these shows after the next change of one of them.
#[derive(PartialEq)]
struct Source {
    symbol: String,
    timeframe: Option<Timeframe>,
    len: usize,
    first: Option<Bar>,
    last: Option<Bar>,
    replay: Option<DateTime<Utc>>,
    studies: Vec<(SeriesId, bool)>,
}

impl Source {
    fn of(chart: &Chart) -> Self {
        let bars = chart.bars(SeriesId::MAIN);
        Self {
            symbol: chart.symbol().to_owned(),
            timeframe: chart.timeframe(),
            len: bars.len(),
            first: bars.first().cloned(),
            last: bars.last().cloned(),
            replay: chart.replay_time(),
            studies: chart
                .indicators()
                .map(|(id, study)| (id, study.is_visible()))
                .collect(),
        }
    }
}

/// Chart data table dialog.
pub struct DataTableDialog {
    /// Whether the dialog is currently visible.
    pub is_open: bool,
    /// Titles of the columns the user turned off, so they stay off when studies come and go.
    hidden: BTreeSet<String>,
    cached: Option<(Source, DataTable)>,
}

impl Default for DataTableDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTableDialog {
    /// Create a closed [`DataTableDialog`] showing every column.
    pub fn new() -> Self {
        Self {
            is_open: false,
            hidden: BTreeSet::new(),
            cached: None,
        }
    }

    /// Open the dialog.
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Close the dialog and release its table.
    pub fn close(&mut self) {
        self.is_open = false;
        self.cached = None;
    }

    /// Show the dialog over `chart`, the chart the user is looking at, and return the host's
    /// request. Copying puts the displayed columns on the clipboard as tab-separated text.
    pub fn show(&mut self, ctx: &Context, chart: &Chart) -> DataTableAction {
        if !self.is_open {
            return DataTableAction::None;
        }
        let source = Source::of(chart);
        if self
            .cached
            .as_ref()
            .is_none_or(|(cached, _)| *cached != source)
        {
            self.cached = Some((source, chart.data_table()));
        }
        let table = &self.cached.as_ref().expect("table was just built").1;
        let hidden = &mut self.hidden;
        let mut action = DataTableAction::None;
        let mut open = true;
        Modal::new("chart_data_table", l10n::text(ctx, "Chart data"))
            .width(860.0)
            .height(520.0)
            .show(ctx, &mut open, |ui| {
                action = Self::contents(ui, table, hidden);
            });
        if !open {
            self.close();
            return DataTableAction::Close;
        }
        action
    }

    fn contents(ui: &mut Ui, table: &DataTable, hidden: &mut BTreeSet<String>) -> DataTableAction {
        let mut action = DataTableAction::None;
        let shown: Vec<usize> = (0..table.headers.len())
            .filter(|&column| !hidden.contains(&table.headers[column]))
            .collect();
        ui.horizontal(|ui| {
            ui.menu_button(l10n::text(ui.ctx(), "Columns"), |ui| {
                // The time column keys the rows, so it stays.
                for header in table.headers.iter().skip(1) {
                    let mut visible = !hidden.contains(header);
                    if ui.checkbox(&mut visible, header).changed() {
                        if visible {
                            hidden.remove(header);
                        } else {
                            hidden.insert(header.clone());
                        }
                    }
                }
            });
            if Button::new("Copy").show(ui).clicked() {
                ui.ctx().copy_text(table.tsv(&shown));
            }
            if Button::new("Export CSV").show(ui).clicked()
                && let Ok(csv) = table.csv(&shown)
            {
                action = DataTableAction::Export(csv);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    table.rows.len(),
                    l10n::text(ui.ctx(), "loaded bars")
                ))
                .color(ui.visuals().weak_text_color()),
            );
        });
        ui.add_space(6.0);
        let widths: Vec<f32> = shown
            .iter()
            .map(|&column| {
                if column == 0 {
                    TIME_WIDTH
                } else {
                    COLUMN_WIDTH
                }
            })
            .collect();
        let total = widths.iter().sum::<f32>();
        ScrollArea::horizontal()
            .id_salt("chart_data_table_columns")
            .show(ui, |ui| {
                Self::paint_row(
                    ui,
                    &widths,
                    total,
                    true,
                    shown.iter().map(|&c| table.headers[c].as_str()),
                );
                ScrollArea::vertical()
                    .id_salt("chart_data_table_rows")
                    .auto_shrink([false; 2])
                    .show_rows(ui, ROW_HEIGHT, table.rows.len(), |ui, range| {
                        for index in range {
                            let row = &table.rows[index];
                            Self::paint_row(
                                ui,
                                &widths,
                                total,
                                false,
                                shown.iter().map(|&c| row[c].as_str()),
                            );
                        }
                    });
            });
        action
    }

    /// One row of cells, an en dash for each missing value.
    fn paint_row<'a>(
        ui: &mut Ui,
        widths: &[f32],
        total: f32,
        header: bool,
        cells: impl Iterator<Item = &'a str>,
    ) {
        let (rect, _) = ui.allocate_exact_size(vec2(total, ROW_HEIGHT), Sense::hover());
        let font = FontId::monospace(12.0);
        let weak = ui.visuals().weak_text_color();
        let color = ui.visuals().text_color();
        let mut x = rect.left();
        for (index, (cell, width)) in cells.zip(widths).enumerate() {
            let (text, color) = match (cell.is_empty(), header) {
                (true, _) => ("\u{2013}", weak),
                (false, true) => (cell, weak),
                (false, false) => (cell, color),
            };
            // Times read left to right; numbers align on the right.
            let (pos, align) = if index == 0 {
                (egui::pos2(x + 4.0, rect.center().y), Align2::LEFT_CENTER)
            } else {
                (
                    egui::pos2(x + width - 8.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                )
            };
            ui.painter()
                .with_clip_rect(Rect::from_min_size(
                    egui::pos2(x, rect.top()),
                    vec2(*width, ROW_HEIGHT),
                ))
                .text(pos, align, text, font.clone(), color);
            x += width;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData};
    use chrono::DateTime;

    #[test]
    fn long_histories_paint_only_visible_rows_and_follow_the_replay_cutoff() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..5000)
            .map(|i| {
                let time = start + chrono::Duration::days(i);
                Bar::new(time, 10.0, 12.0, 9.0, 11.0, 100.0)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let ctx = Context::default();
        let mut dialog = DataTableDialog::new();
        dialog.open();
        let mut show = |chart: &Chart| {
            let output = ctx.run_ui(Default::default(), |ui| {
                dialog.show(ui.ctx(), chart);
            });
            let texts: Vec<String> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        };
        show(&chart);
        let live = show(&chart);
        assert!(
            live.iter().any(|text| text == "5000 loaded bars"),
            "{live:?}"
        );
        assert!(live.len() < 400, "{} text shapes for 5000 rows", live.len());
        chart.set_replay_time(Some(start + chrono::Duration::days(1000)));
        show(&chart);
        let replay = show(&chart);
        assert!(
            replay.iter().any(|text| text == "1000 loaded bars"),
            "{replay:?}"
        );
    }
}
