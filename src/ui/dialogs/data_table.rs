//! Chart data table dialog.
//!
//! Lists the loaded bars with their volume and the visible study plots, aligned by chart time.
//! It follows the chart it is shown with, including its replay cutoff, and loads nothing: only
//! bars already loaded appear. The host opens it from
//! [`ContextMenuAction::OpenDataTable`](crate::ui::context_menu::ContextMenuAction::OpenDataTable)
//! and saves the text of [`DataTableAction::Export`]. Symbol search requests go to the host;
//! the table stays open while a watchlist or search changes the chart's symbol.

use crate::localization as l10n;
use crate::model::Timeframe;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};
use crate::widget::{Chart, ColumnId, DataTable, SeriesId};
use chrono::{DateTime, Utc};
use egui::{Align2, Context, FontId, Rect, ScrollArea, Sense, Ui, vec2};
use std::collections::{BTreeSet, HashMap};

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
    /// Open symbol search. An empty query requests the current symbol selected for replacement.
    /// Apply the chosen symbol to the chart passed to `show`, retaining its study ids.
    SearchSymbol(String),
}

/// Inputs to the table, without copying bars, study settings or the instrument, whose changes
/// move the chart's table revision, and the added series, which it reads by their own revisions.
#[derive(PartialEq)]
struct Source {
    revision: (u64, u64),
    series: Vec<crate::widget::SeriesFingerprint>,
    symbol: String,
    timeframe: Option<Timeframe>,
    len: usize,
    replay: Option<DateTime<Utc>>,
    timezone: crate::scales::TimezoneMode,
}

impl Source {
    fn of(chart: &Chart) -> Self {
        Self {
            revision: chart.data_table_revision(),
            series: chart.data_table_series(),
            symbol: chart.symbol().to_owned(),
            timeframe: chart.timeframe(),
            len: chart.bars(SeriesId::MAIN).len(),
            replay: chart.replay_time(),
            timezone: chart.chart_options().time_scale.timezone.clone(),
        }
    }
}

/// Chart data table dialog.
pub struct DataTableDialog {
    /// Whether the dialog is currently visible.
    pub is_open: bool,
    /// The columns turned off on each chart, identified across study removal and restores.
    hidden: HashMap<u64, BTreeSet<ColumnId>>,
    cached: Option<(Source, DataTable)>,
    column_ids: Vec<ColumnId>,
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
            hidden: HashMap::new(),
            cached: None,
            column_ids: Vec::new(),
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
            let (mut table, columns) = chart.data_table_with_columns();
            self.column_ids = columns;
            table.rows.reverse();
            self.cached = Some((source, table));
        }
        let table = &self.cached.as_ref().expect("table was just built").1;
        let hidden = self.hidden.entry(chart.data_table_identity()).or_default();
        let mut action = DataTableAction::None;
        let mut open = true;
        DialogFrame::new("Chart data", vec2(804.0, 472.0)).show(ctx, |ui| {
            open = !dialog_header(ui, &l10n::text(ctx, "Chart data"));
            ui.horizontal(|ui| {
                let symbol = if chart.symbol().is_empty() {
                    l10n::text(ctx, "Symbol")
                } else {
                    chart.symbol().to_owned()
                };
                if Button::new(symbol)
                    .show(ui)
                    .on_hover_text(l10n::text(ctx, "Change symbol"))
                    .clicked()
                {
                    action = DataTableAction::SearchSymbol(String::new());
                }
                if let Some(feed) = chart.feed_state(SeriesId::MAIN) {
                    if feed.loading_history {
                        ui.spinner();
                        ui.label(l10n::text(ctx, "Loading data"));
                    }
                    if let Some(error) = &feed.last_error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(error.to_string())
                                    .color(ui.visuals().error_fg_color),
                            )
                            .truncate(),
                        );
                    }
                }
            });
            let contents = Self::contents(ui, table, hidden, &self.column_ids);
            if contents != DataTableAction::None {
                action = contents;
            }
        });
        if !open {
            self.close();
            return DataTableAction::Close;
        }
        if !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(ctx)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && !ctx.input(|input| {
                input.modifiers.command || input.modifiers.ctrl || input.modifiers.alt
            })
        {
            let typed = ctx.input_mut(|input| {
                let at = input.events.iter().position(|event| {
                    matches!(event, egui::Event::Text(text)
                        if text.starts_with(|c: char| !c.is_control() && !c.is_whitespace()))
                })?;
                match input.events.remove(at) {
                    egui::Event::Text(text) => Some(text),
                    _ => None,
                }
            });
            if let Some(query) = typed {
                action = DataTableAction::SearchSymbol(query);
            }
        }
        action
    }

    fn contents(
        ui: &mut Ui,
        table: &DataTable,
        hidden: &mut BTreeSet<ColumnId>,
        columns: &[ColumnId],
    ) -> DataTableAction {
        let mut action = DataTableAction::None;
        let shown: Vec<usize> = (0..table.headers.len())
            .filter(|&column| !hidden.contains(&columns[column]))
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(l10n::text(ui.ctx(), "Columns"), |ui| {
                // The time column keys the rows, so it stays.
                for (header, column) in table.headers.iter().zip(columns).skip(1) {
                    let mut visible = !hidden.contains(column);
                    if ui.checkbox(&mut visible, header).changed() {
                        if visible {
                            hidden.remove(column);
                        } else {
                            hidden.insert(column.clone());
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
    use crate::model::{Bar, BarData, PriceSource};
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
        let show = |dialog: &mut DataTableDialog, chart: &Chart| {
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
        show(&mut dialog, &chart);
        let live = show(&mut dialog, &chart);
        assert!(
            live.iter().any(|text| text == "5000 loaded bars"),
            "{live:?}"
        );
        assert!(live.len() < 400, "{} text shapes for 5000 rows", live.len());
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
        show(&mut dialog, &chart);
        let mut corrected = chart.bars(crate::widget::SeriesId::MAIN).to_vec();
        corrected[2500].high = 22.0;
        corrected[2500].close = 21.0;
        chart.set_data(crate::widget::SeriesId::MAIN, corrected);
        show(&mut dialog, &chart);
        let cached_value = |dialog: &DataTableDialog, index: usize, study: &str| {
            let time = (start + chrono::Duration::days(index as i64)).to_rfc3339();
            let table = &dialog.cached.as_ref().unwrap().1;
            let column = table
                .headers
                .iter()
                .position(|header| header.starts_with(&format!("{study}: ")))
                .unwrap();
            table.rows.iter().find(|row| row[0] == time).unwrap()[column].clone()
        };
        // The corrected close and its neighbour average to (21 + 11) / 2.
        assert_eq!(cached_value(&dialog, 2501, "SMA"), "16.00");
        chart
            .indicator_mut(study)
            .unwrap()
            .set_input("period", crate::studies::InputValue::Length(3))
            .unwrap();
        show(&mut dialog, &chart);
        // The same three-bar window is (11 + 21 + 11) / 3.
        assert_eq!(cached_value(&dialog, 2501, "SMA"), "14.33");
        chart.update(
            SeriesId::MAIN,
            Bar::new(
                start + chrono::Duration::days(4999),
                11.0,
                32.0,
                9.0,
                31.0,
                100.0,
            ),
        );
        show(&mut dialog, &chart);
        assert_eq!(cached_value(&dialog, 4999, "SMA"), "17.67"); // (11 + 11 + 31) / 3
        chart.update(
            SeriesId::MAIN,
            Bar::new(
                start + chrono::Duration::days(5000),
                31.0,
                42.0,
                30.0,
                41.0,
                100.0,
            ),
        );
        show(&mut dialog, &chart);
        assert_eq!(cached_value(&dialog, 5000, "SMA"), "27.67"); // (11 + 31 + 41) / 3
        assert_eq!(
            dialog.cached.as_ref().unwrap().1.rows.first().unwrap()[0],
            (start + chrono::Duration::days(5000)).to_rfc3339()
        );
        chart.set_replay_time(Some(start + chrono::Duration::days(1000)));
        show(&mut dialog, &chart);
        let replay = show(&mut dialog, &chart);
        assert!(
            replay.iter().any(|text| text == "1000 loaded bars"),
            "{replay:?}"
        );

        chart.set_replay_time(None);
        let ratio = chart.add_indicator(Box::new(crate::studies::Ratio::default()));
        chart
            .indicator_mut(ratio)
            .unwrap()
            .set_input(
                "symbol_1",
                crate::studies::InputValue::Symbol("DENOM".into()),
            )
            .unwrap();
        let request = chart
            .study_data_requests()
            .into_iter()
            .find(|request| request.symbol == "DENOM")
            .unwrap();
        for (denominator, expected) in [(41.0, "1.00"), (82.0, "0.50")] {
            let history = chart
                .bars(SeriesId::MAIN)
                .iter()
                .map(|bar| Bar::from_value(bar.time, denominator))
                .collect();
            chart.set_study_data(request.clone(), history, None);
            show(&mut dialog, &chart);
            // A host-fed denominator changes with no edit to the chart bars or study settings.
            assert_eq!(cached_value(&dialog, 5000, "Ratio"), expected);
        }
        chart.set_instrument(Some(
            crate::model::Instrument::new("A").with_tick_size(0.001),
        ));
        show(&mut dialog, &chart);
        assert_eq!(dialog.cached.as_ref().unwrap().1.rows[0][1], "31.000");
        chart
            .set_indicator_source(study, crate::studies::StudySource::Price(PriceSource::High))
            .unwrap();
        show(&mut dialog, &chart);
        assert_eq!(cached_value(&dialog, 5000, "SMA"), "28.667"); // (12 + 32 + 42) / 3
        let headers = |dialog: &DataTableDialog| dialog.cached.as_ref().unwrap().1.headers.clone();
        chart.set_indicator_visible(ratio, false);
        show(&mut dialog, &chart);
        assert!(
            !headers(&dialog)
                .iter()
                .any(|name| name.starts_with("Ratio"))
        );
        chart.remove_series(study);
        show(&mut dialog, &chart);
        assert!(!headers(&dialog).iter().any(|name| name.starts_with("SMA")));
    }

    /// Watchlist picks and symbol search remain usable while the table keeps column choices.
    #[test]
    fn symbol_navigation_keeps_columns_and_exposes_feed_status() {
        use crate::data::FeedError;
        use egui::{Event, Modifiers, PointerButton, Pos2};

        let ctx = Context::default();
        let mut chart = Chart::new(BarData::from_bars(vec![Bar::from_value(
            DateTime::UNIX_EPOCH,
            10.0,
        )]));
        chart.set_symbol("AAA");
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
        let duplicate = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
        let header = chart.data_table().headers.last().unwrap().clone();
        assert_eq!(
            chart
                .data_table()
                .headers
                .iter()
                .filter(|name| **name == header)
                .count(),
            2
        );
        let mut dialog = DataTableDialog::new();
        dialog.open();
        let frame = |dialog: &mut DataTableDialog, chart: &mut Chart, events| {
            let mut action = DataTableAction::None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if ui.button("Watchlist BBB").clicked() {
                        chart.set_symbol("BBB");
                    }
                    action = dialog.show(ui.ctx(), chart);
                },
            );
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            let commands = output.platform_output.commands.clone();
            output.drop_without_applying_deltas();
            (action, texts, commands)
        };
        let click = |dialog: &mut DataTableDialog, chart: &mut Chart, at| {
            frame(dialog, chart, vec![Event::PointerMoved(at)]);
            frame(
                dialog,
                chart,
                [true, false]
                    .map(|pressed| Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    })
                    .to_vec(),
            )
        };
        let find = |texts: &[(String, Pos2)], label: &str| {
            texts.iter().find(|(text, _)| text == label).unwrap().1
        };
        frame(&mut dialog, &mut chart, vec![]);
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        click(&mut dialog, &mut chart, find(&texts, "Columns"));
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        // The last two matching labels are the two independent study checkboxes.
        let at = texts
            .iter()
            .filter(|(text, _)| *text == header)
            .rev()
            .nth(1)
            .unwrap()
            .1;
        click(&mut dialog, &mut chart, at);
        egui::Popup::close_all(&ctx);
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        let (_, _, commands) = click(&mut dialog, &mut chart, find(&texts, "Copy"));
        let copied = |commands: Vec<egui::OutputCommand>| {
            commands
                .into_iter()
                .find_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text),
                    _ => None,
                })
                .unwrap()
        };
        let before = copied(commands);
        assert_eq!(before.lines().next().unwrap().matches(&header).count(), 1);
        click(&mut dialog, &mut chart, find(&texts, "Watchlist BBB"));
        assert_eq!(chart.symbol(), "BBB");
        assert!(dialog.is_open);
        assert!(chart.indicators().any(|(id, _)| id == study));
        assert!(chart.indicators().any(|(id, _)| id == duplicate));
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        let (_, _, commands) = click(&mut dialog, &mut chart, find(&texts, "Copy"));
        assert_eq!(copied(commands), before);
        chart.set_indicator_visible(study, false);
        frame(&mut dialog, &mut chart, vec![]);
        chart.set_indicator_visible(study, true);
        chart
            .indicator_mut(study)
            .unwrap()
            .set_input("period", crate::studies::InputValue::Length(3))
            .unwrap();
        dialog.close();
        dialog.open();
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        let (_, _, commands) = click(&mut dialog, &mut chart, find(&texts, "Copy"));
        assert_eq!(copied(commands), before);
        let (action, _, _) = click(&mut dialog, &mut chart, find(&texts, "BBB"));
        assert_eq!(format!("{action:?}"), "SearchSymbol(\"\")");
        chart.feed.loading_history = true;
        chart.feed.last_error = Some(FeedError::SymbolNotFound("BBB".into()));
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        assert!(texts.iter().any(|(text, _)| text == "Loading data"));
        assert!(
            texts
                .iter()
                .any(|(text, _)| text == "symbol not found: BBB")
        );
        chart.set_symbol("");
        let (_, texts, _) = frame(&mut dialog, &mut chart, vec![]);
        let (action, _, _) = click(&mut dialog, &mut chart, find(&texts, "Symbol"));
        assert_eq!(format!("{action:?}"), "SearchSymbol(\"\")");
    }

    /// One frame of `dialog` over `chart`: the address of its rows, which move when the table is
    /// rebuilt, as the new rows are allocated while the old ones are alive.
    fn shown_rows(
        ctx: &Context,
        dialog: &mut DataTableDialog,
        chart: &Chart,
    ) -> *const Vec<String> {
        ctx.run_ui(Default::default(), |ui| {
            dialog.show(ui.ctx(), chart);
        })
        .drop_without_applying_deltas();
        dialog.cached.as_ref().unwrap().1.rows.as_ptr()
    }

    /// A column turned off on one chart stays off on that chart, also after it restores its saved
    /// state, and leaves the columns of another chart the dialog follows.
    #[test]
    fn column_choice_on_one_chart_hides_no_column_of_another() {
        use egui::{Event, Modifiers, PointerButton, Pos2};

        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..30)
            .map(|i| Bar::from_value(start + chrono::Duration::days(i), 10.0 + i as f64))
            .collect();
        let mut first = Chart::new(BarData::from_bars(bars.clone()));
        let mut second = Chart::new(BarData::from_bars(bars));
        first.add_indicator(Box::new(crate::studies::SMA::new(3)));
        second.add_indicator(Box::new(crate::studies::RSI::new(14)));
        let ctx = Context::default();
        let size = std::cell::Cell::new(vec2(1440.0, 900.0));
        let mut dialog = DataTableDialog::new();
        dialog.open();
        let frame = |dialog: &mut DataTableDialog, chart: &Chart, events| {
            let mut action = DataTableAction::None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size.get())),
                    events,
                    ..Default::default()
                },
                |ui| action = dialog.show(ui.ctx(), chart),
            );
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            let commands = output.platform_output.commands.clone();
            output.drop_without_applying_deltas();
            (action, texts, commands)
        };
        let click = |dialog: &mut DataTableDialog, chart: &Chart, at| {
            frame(dialog, chart, vec![Event::PointerMoved(at)]);
            frame(
                dialog,
                chart,
                [true, false]
                    .map(|pressed| Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    })
                    .to_vec(),
            )
        };
        let find = |texts: &[(String, Pos2)], label: &str| {
            texts.iter().rfind(|(text, _)| text == label).unwrap().1
        };
        let headers = |dialog: &mut DataTableDialog, chart: &Chart| {
            let names = chart.data_table().headers;
            let (_, texts, _) = frame(dialog, chart, vec![]);
            let painted: Vec<_> = texts
                .into_iter()
                .map(|(text, _)| text)
                .filter(|text| names.contains(text))
                .collect();
            painted
        };
        frame(&mut dialog, &first, vec![]);
        let (_, texts, _) = frame(&mut dialog, &first, vec![]);
        click(&mut dialog, &first, find(&texts, "Columns"));
        let (_, texts, _) = frame(&mut dialog, &first, vec![]);
        // The last label of that name is the box in the Columns menu.
        click(&mut dialog, &first, find(&texts, "SMA: SMA(3)"));
        egui::Popup::close_all(&ctx);
        let without_sma = ["time", "open", "high", "low", "close", "volume"];
        assert_eq!(headers(&mut dialog, &first), without_sma);

        assert_eq!(
            headers(&mut dialog, &second),
            second.data_table().headers,
            "the other chart keeps its first study's column"
        );
        let (_, texts, _) = frame(&mut dialog, &second, vec![]);
        let (_, _, commands) = click(&mut dialog, &second, find(&texts, "Copy"));
        assert!(commands.iter().any(|command| matches!(
            command,
            egui::OutputCommand::CopyText(text) if text.contains("RSI: RSI(14)")
        )));
        let (action, _, _) = click(&mut dialog, &second, find(&texts, "Export CSV"));
        assert!(matches!(action, DataTableAction::Export(csv) if csv.contains("RSI: RSI(14)")));

        assert_eq!(headers(&mut dialog, &first), without_sma);
        // Read back as storage would, which must keep the chart that took it.
        let saved =
            serde_json::from_str(&serde_json::to_string(&first.snapshot().unwrap()).unwrap())
                .unwrap();
        for _ in 0..2 {
            first.restore(&saved).unwrap();
            assert_eq!(first.data_table().headers.len(), 7);
            assert_eq!(headers(&mut dialog, &first), without_sma);
        }
        dialog.close();
        for _ in 0..2 {
            first.restore(&first.snapshot().unwrap()).unwrap();
        }
        dialog.open();
        assert_eq!(headers(&mut dialog, &first), without_sma);
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            first.set_theme(&ctx, &theme);
            for dimensions in [vec2(320.0, 450.0), vec2(1440.0, 900.0)] {
                size.set(dimensions);
                for _ in 0..3 {
                    frame(&mut dialog, &first, vec![]);
                }
                let rect = ctx.memory(|m| {
                    let layer = m.areas().top_layer_id(egui::Order::Middle).unwrap();
                    m.area_rect(layer.id).unwrap()
                });
                assert!(
                    rect.left() >= 23.0 && rect.right() <= dimensions.x - 23.0,
                    "{rect:?}"
                );
                assert!(
                    rect.top() >= 23.0 && rect.bottom() <= dimensions.y - 23.0,
                    "{rect:?}"
                );
            }
        }
    }

    /// Frames over an unchanged chart reuse its table while an edge bar is whitespace, whose NaN
    /// fields differ from themselves, until a bar of values replaces it.
    #[test]
    fn unchanged_chart_with_a_whitespace_edge_reuses_its_table() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        for edge in [49, 0] {
            let mut bars: Vec<_> = (0..50)
                .map(|i| {
                    let time = start + chrono::Duration::days(i);
                    Bar::new(time, 10.0, 12.0, 9.0, 11.0, 100.0)
                })
                .collect();
            let time = bars[edge].time;
            bars[edge] = Bar::whitespace(time);
            let mut chart = Chart::new(BarData::from_bars(bars));
            let ctx = Context::default();
            let mut dialog = DataTableDialog::new();
            dialog.open();
            let built = shown_rows(&ctx, &mut dialog, &chart);
            assert_eq!(shown_rows(&ctx, &mut dialog, &chart), built, "bar {edge}");

            chart.update(SeriesId::MAIN, Bar::new(time, 10.0, 99.0, 9.0, 99.0, 100.0));
            assert_ne!(shown_rows(&ctx, &mut dialog, &chart), built, "bar {edge}");
            let table = &dialog.cached.as_ref().unwrap().1;
            let row = table.rows.iter().find(|row| row[0] == time.to_rfc3339());
            assert_eq!(row.unwrap()[4], "99.00", "bar {edge}");
        }
    }

    /// Older bars in front change the table once, so it is built once with its final values, not
    /// once for each study the chart then recalculates on its own frames.
    #[test]
    fn history_load_builds_the_table_once() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..2000)
            .map(|i| {
                let close = 10.0 + (i % 7) as f64;
                let time = start + chrono::Duration::days(i);
                Bar::new(time, close, close + 1.0, close - 1.0, close, 100.0)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars[1000..].to_vec()));
        for period in [2, 3, 5, 8, 13] {
            chart.add_indicator(Box::new(crate::studies::SMA::new(period)));
        }
        let chart_ctx = Context::default();
        let ctx = Context::default();
        let mut dialog = DataTableDialog::new();
        dialog.open();
        let mut time = 0.0;
        // One host frame: the chart, then the dialog over it.
        let mut step = |chart: &mut Chart, dialog: &mut DataTableDialog| {
            time += 1.0;
            crate::widget::test_frames::frame(&chart_ctx, chart, time, vec![]);
            shown_rows(&ctx, dialog, chart)
        };
        let mut previous = std::ptr::null();
        for _ in 0..12 {
            previous = step(&mut chart, &mut dialog);
        }
        // A scroll-back page: the same bars with 1000 older ones in front.
        chart.set_data(SeriesId::MAIN, bars);
        let mut builds = 0;
        for _ in 0..12 {
            let rows = step(&mut chart, &mut dialog);
            builds += usize::from(rows != previous);
            previous = rows;
        }
        assert_eq!(builds, 1);
        let mut expected = chart.data_table().rows;
        expected.reverse();
        assert!(dialog.cached.as_ref().unwrap().1.rows == expected);
    }

    /// A study of the bars in view changes in the table as the chart pans; other studies leave the
    /// table as it is.
    #[test]
    fn view_dependent_study_column_follows_a_pan() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        // Closes of 10 for the first 200 days and of 30 for the last 200.
        let bars: Vec<_> = (0..400)
            .map(|i| {
                let close = if i < 200 { 10.0 } else { 30.0 };
                let time = start + chrono::Duration::days(i);
                Bar::new(time, close, close, close, close, 1.0)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.add_indicator(Box::new(crate::studies::VisibleAveragePrice::new()));
        let mut other = Chart::new(BarData::from_bars(bars));
        other.add_indicator(Box::new(crate::studies::SMA::new(3)));
        let chart_ctx = Context::default();
        let ctx = Context::default();
        let mut dialog = DataTableDialog::new();
        dialog.open();
        let newest = |dialog: &DataTableDialog| {
            let table = &dialog.cached.as_ref().unwrap().1;
            let column = table
                .headers
                .iter()
                .position(|header| header.starts_with("Visible Average Price"))
                .unwrap();
            table.rows[0][column].clone()
        };
        let frames = |chart: &mut Chart, frames: std::ops::Range<i32>| {
            for frame in frames {
                crate::widget::test_frames::frame(&chart_ctx, chart, f64::from(frame), vec![]);
            }
        };
        frames(&mut chart, 0..3);
        shown_rows(&ctx, &mut dialog, &chart);
        assert_eq!(newest(&dialog), "30.00");
        // Show bars 0 to 50, all closing at 10.
        assert!(chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, 50.0)));
        frames(&mut chart, 3..6);
        shown_rows(&ctx, &mut dialog, &chart);
        assert_eq!(newest(&dialog), "10.00");

        frames(&mut other, 0..3);
        let built = shown_rows(&ctx, &mut dialog, &other);
        assert!(other.set_visible_logical_range(crate::model::LogicalRange::new(0.0, 50.0)));
        frames(&mut other, 3..6);
        assert_eq!(shown_rows(&ctx, &mut dialog, &other), built);
    }

    /// A column turned off for one study stays with that study when a study of its title before
    /// it is removed, and moves to no study of another title that a restored layout adds under
    /// the id it was made for.
    #[test]
    fn column_choice_follows_its_study_after_an_earlier_one_is_removed() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..300)
            .map(|i| Bar::from_value(start + chrono::Duration::days(i), 10.0 + i as f64))
            .collect();
        // The layout of another chart whose second study, with the id of the SMA(50) below, is
        // an RSI.
        let mut other = Chart::new(BarData::from_bars(bars.clone()));
        other.add_indicator(Box::new(crate::studies::SMA::new(5)));
        let rsi = other.add_indicator(Box::new(crate::studies::RSI::new(14)));
        let layout = other.snapshot().unwrap();
        let cases = [
            (
                "the first SMA removed and another added",
                false,
                &["SMA: SMA(200)"][..],
            ),
            (
                "the layout of another chart restored",
                true,
                &["SMA: SMA(5)", "RSI: RSI(14)"][..],
            ),
        ];
        let ctx = Context::default();
        let shown = |dialog: &mut DataTableDialog, chart: &Chart| {
            shown_rows(&ctx, dialog, chart);
            let headers = &dialog.cached.as_ref().unwrap().1.headers;
            let hidden = &dialog.hidden[&chart.data_table_identity()];
            headers
                .iter()
                .zip(&dialog.column_ids)
                .skip(6)
                .filter(|(_, column)| !hidden.contains(*column))
                .map(|(header, _)| header.clone())
                .collect::<Vec<_>>()
        };
        for (case, restore, expected) in cases {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let first = chart.add_indicator(Box::new(crate::studies::SMA::new(10)));
            let second = chart.add_indicator(Box::new(crate::studies::SMA::new(50)));
            assert_eq!(
                second, rsi,
                "{case}: the layout's RSI has the id of the SMA(50)"
            );
            let mut dialog = DataTableDialog::new();
            dialog.open();
            assert_eq!(
                shown(&mut dialog, &chart),
                ["SMA: SMA(10)", "SMA: SMA(50)"],
                "{case}"
            );
            // Turn off the SMA(50) column, as its box in the Columns menu does.
            let headers = &dialog.cached.as_ref().unwrap().1.headers;
            let at = headers.iter().position(|h| h == "SMA: SMA(50)").unwrap();
            let column = dialog.column_ids[at].clone();
            let hidden = dialog.hidden.get_mut(&chart.data_table_identity()).unwrap();
            hidden.insert(column);
            assert_eq!(shown(&mut dialog, &chart), ["SMA: SMA(10)"], "{case}");

            if restore {
                chart.restore(&layout).unwrap();
            } else {
                chart.remove_series(first);
                chart.add_indicator(Box::new(crate::studies::SMA::new(200)));
            }
            assert_eq!(shown(&mut dialog, &chart), expected, "{case}");
        }
    }

    /// After a chart restores its own layout, a study of another chart's layout starts with its
    /// column shown, also when it was saved under the id of a study whose column is off here, and
    /// turning off the column of one study hides that study only, also when the studies share a
    /// title. Another chart's layout restored again starts with every column shown.
    #[test]
    fn restored_studies_of_one_title_keep_separate_column_choices() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..300)
            .map(|i| Bar::from_value(start + chrono::Duration::days(i), 10.0 + i as f64))
            .collect();
        let mut other = Chart::new(BarData::from_bars(bars.clone()));
        let five = other.add_indicator(Box::new(crate::studies::SMA::new(5)));
        other.add_indicator(Box::new(crate::studies::SMA::new(20)));
        let layout = other.snapshot().unwrap();
        // The same chart's layout once its SMA(5) is removed: the SMA(20) alone, saved with id 3.
        other.remove_series(five);
        let lone = other.snapshot().unwrap();
        // The layout of a third chart, with an EMA under the saved id of the SMA(5).
        let mut third = Chart::new(BarData::from_bars(bars.clone()));
        third.add_indicator(Box::new(crate::studies::EMA::new(9)));
        third.add_indicator(Box::new(crate::studies::SMA::new(20)));
        let mixed = third.snapshot().unwrap();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.add_indicator(Box::new(crate::studies::SMA::new(10)));
        chart.restore(&chart.snapshot().unwrap()).unwrap();
        let ctx = Context::default();
        let mut dialog = DataTableDialog::new();
        dialog.open();
        // The study columns shown after turning off the column of `off`, if any, as its box in
        // the Columns menu does.
        let mut shown = |chart: &Chart, off: Option<&str>| {
            shown_rows(&ctx, &mut dialog, chart);
            let headers = &dialog.cached.as_ref().unwrap().1.headers;
            let hidden = dialog.hidden.get_mut(&chart.data_table_identity()).unwrap();
            if let Some(at) = headers.iter().position(|h| Some(h.as_str()) == off) {
                hidden.insert(dialog.column_ids[at].clone());
            }
            headers
                .iter()
                .zip(&dialog.column_ids)
                .skip(6)
                .filter(|(_, column)| !hidden.contains(*column))
                .map(|(header, _)| header.clone())
                .collect::<Vec<_>>()
        };
        // `lone` saved its SMA(20) with id 3, the id of the SMA(10) turned off here.
        assert!(shown(&chart, Some("SMA: SMA(10)")).is_empty());
        chart.restore(&lone).unwrap();
        assert_eq!(shown(&chart, None), ["SMA: SMA(20)"]);
        chart.restore(&layout).unwrap();
        assert_eq!(shown(&chart, Some("SMA: SMA(5)")), ["SMA: SMA(20)"]);
        chart.restore(&mixed).unwrap();
        assert_eq!(shown(&chart, Some("EMA: EMA(9)")), ["SMA: SMA(20)"]);
        chart.restore(&mixed).unwrap();
        assert_eq!(shown(&chart, None), ["EMA: EMA(9)", "SMA: SMA(20)"]);
    }

    /// The open table shows a new last bar of an added series, which leaves the chart's revisions
    /// as they are. A column turned off for an added series stays off when the chart restores its
    /// own layout, which adds the series again under a new id.
    #[test]
    fn added_series_column_follows_its_last_bar_and_keeps_its_choice_through_a_restore() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = (0..5)
            .map(|i| Bar::from_value(start + chrono::Duration::days(i), 10.0 + i as f64))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let [_, other] = ["Peer", "Other"].map(|name| {
            chart.add_series(crate::widget::SeriesOptions {
                name: name.into(),
                ..Default::default()
            })
        });
        let layout = chart.snapshot().unwrap();
        let ctx = Context::default();
        let mut dialog = DataTableDialog::new();
        dialog.open();
        shown_rows(&ctx, &mut dialog, &chart);
        chart.update(other, Bar::from_value(start + chrono::Duration::days(4), 42.0));
        shown_rows(&ctx, &mut dialog, &chart);
        // Newest row first: time, the five main columns, Peer, Other.
        assert_eq!(dialog.cached.as_ref().unwrap().1.rows[0][7], "42.00");
        let headers = &dialog.cached.as_ref().unwrap().1.headers;
        let at = headers.iter().position(|h| h == "Peer").unwrap();
        let column = dialog.column_ids[at].clone();
        dialog
            .hidden
            .get_mut(&chart.data_table_identity())
            .unwrap()
            .insert(column);
        chart.restore(&layout).unwrap();
        shown_rows(&ctx, &mut dialog, &chart);
        let hidden = &dialog.hidden[&chart.data_table_identity()];
        let shown: Vec<_> = dialog.cached.as_ref().unwrap().1.headers[6..]
            .iter()
            .zip(&dialog.column_ids[6..])
            .filter(|(_, column)| !hidden.contains(*column))
            .map(|(header, _)| header.as_str())
            .collect();
        assert_eq!(shown, ["Other"]);
    }
}
