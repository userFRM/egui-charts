//! A screener table of the rows the feed finds for the user's filters.

use std::sync::Arc;

use egui::{Sense, Ui, vec2};

use super::controls::{cell, dropdown, row};

use super::watchlist::ROW_HEIGHT;
use crate::data::inbox::Inbox;
use crate::data::{
    Datafeed, FeedError, ScreenerColumnSet, ScreenerFilter, ScreenerOp, ScreenerQuery, ScreenerRow,
};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// Rows asked for at most.
const LIMIT: usize = 200;

/// An action the host performs for the screener.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ScreenerAction {
    /// No host action.
    #[default]
    None,
    /// Show this symbol, such as on the chart.
    SelectSymbol(String),
    /// Save this CSV text of the shown rows to a file the host chooses.
    ExportCsv(String),
    /// Add these symbols, the shown rows in order, to a watchlist, such as with
    /// [`WatchlistPanel::import`](super::WatchlistPanel::import).
    SendToWatchlist(Vec<String>),
}

/// A screener fed by the host's [`Datafeed::screen`].
///
/// The host gives the column sets; the user picks one, adds filters on its columns and sorts by
/// a column by clicking its heading. Each change asks the feed again, which filters, sorts and
/// limits, and only the answer to the latest request is shown. Call [`poll`](Self::poll) each
/// frame, then [`show`](Self::show) while the panel is open.
pub struct ScreenerPanel {
    sets: Vec<ScreenerColumnSet>,
    set: usize,
    filters: Vec<ScreenerFilter>,
    sort_by: Option<String>,
    descending: bool,
    /// The filter being written: its column within the set, comparison and value text.
    draft: (usize, ScreenerOp, FilterWidget),
    feed: Option<Arc<dyn Datafeed>>,
    /// The latest answer and the set it was asked with.
    rows: Vec<ScreenerRow>,
    answered: usize,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
}

impl ScreenerPanel {
    /// A screener of `sets`, showing the first, without a feed.
    pub fn new(sets: Vec<ScreenerColumnSet>) -> Self {
        Self {
            sets,
            set: 0,
            filters: Vec::new(),
            sort_by: None,
            descending: true,
            draft: (
                0,
                ScreenerOp::Above,
                FilterWidget::new().placeholder("Value"),
            ),
            feed: None,
            rows: Vec::new(),
            answered: 0,
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
        }
    }

    /// Ask `feed` for rows from now on, starting at once.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        self.refresh();
    }

    fn query(&self) -> ScreenerQuery {
        ScreenerQuery {
            columns: self.sets.get(self.set).map_or_else(Vec::new, |set| {
                set.columns.iter().map(|column| column.id.clone()).collect()
            }),
            filters: self.filters.clone(),
            sort_by: self.sort_by.clone(),
            descending: self.descending,
            limit: LIMIT,
        }
    }

    /// Ask the feed again, such as when the host's data changed.
    pub fn refresh(&mut self) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        feed.screen(self.query(), self.inbox.reply(self.request));
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (key, answer) in self.inbox.drain().answers {
            if key != self.request {
                continue;
            }
            self.loading = false;
            match *answer
                .downcast::<Result<Vec<ScreenerRow>, FeedError>>()
                .expect("screener rows")
            {
                Ok(rows) => {
                    self.rows = rows;
                    self.answered = self.set;
                    self.error = None;
                }
                Err(error) => {
                    self.rows.clear();
                    self.error = Some(error);
                }
            }
        }
    }

    /// The shown rows as CSV, headed by Symbol and the column titles. Every field is quoted and
    /// embedded quotes doubled.
    fn csv(&self) -> String {
        let field = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
        let titles = self
            .sets
            .get(self.answered)
            .map_or(&[][..], |set| &set.columns);
        let mut csv = std::iter::once(field("Symbol"))
            .chain(titles.iter().map(|column| field(&column.title)))
            .collect::<Vec<_>>()
            .join(",");
        csv.push_str("\r\n");
        for row in &self.rows {
            let line = std::iter::once(&row.symbol)
                .chain(&row.values)
                .map(|text| field(text))
                .collect::<Vec<_>>()
                .join(",");
            csv.push_str(&line);
            csv.push_str("\r\n");
        }
        csv
    }

    /// Render the filters and the table after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> ScreenerAction {
        let mut action = ScreenerAction::None;
        let theme = ui.active_theme();
        let has_rows = !self.rows.is_empty();
        PanelHeader::new("Screener").show_plain(ui);
        let mut changed = false;
        let Some(set) = self.sets.get(self.set) else {
            EmptyState::new("No columns to screen")
                .icon(&icons::WIDGET_BAR_SCREENER)
                .fill_height()
                .show(ui);
            return action;
        };
        let (name, columns) = (set.name.clone(), set.columns.clone());
        let title = |id: &str| {
            columns
                .iter()
                .find(|column| column.id == id)
                .map_or(id.to_owned(), |column| column.title.clone())
        };
        ui.horizontal_wrapped(|ui| {
            let before = self.set;
            dropdown(ui, "screener_set", &name, |ui| {
                for (index, set) in self.sets.iter().enumerate() {
                    ui.selectable_value(&mut self.set, index, &set.name);
                }
            });
            if self.set != before {
                changed = true;
                self.draft.0 = 0;
            }
            if Button::new("Send to watchlist")
                .enabled(has_rows)
                .show(ui)
                .clicked()
            {
                action = ScreenerAction::SendToWatchlist(
                    self.rows.iter().map(|row| row.symbol.clone()).collect(),
                );
            }
            if Button::new("Export CSV…")
                .enabled(has_rows)
                .show(ui)
                .clicked()
            {
                action = ScreenerAction::ExportCsv(self.csv());
            }
        });
        if !columns.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let (column, op, value) = &mut self.draft;
                *column = (*column).min(columns.len() - 1);
                dropdown(
                    ui,
                    "screener_filter_column",
                    &columns[*column].title,
                    |ui| {
                        for (index, entry) in columns.iter().enumerate() {
                            ui.selectable_value(column, index, &entry.title);
                        }
                    },
                );
                dropdown(ui, "screener_filter_op", op_label(*op), |ui| {
                    for entry in [ScreenerOp::Above, ScreenerOp::Below] {
                        ui.selectable_value(op, entry, op_label(entry));
                    }
                });
                ui.allocate_ui(vec2(128.0, 28.0), |ui| {
                    value.show(ui);
                });
                let parsed = value
                    .query()
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite());
                if Button::new("Add filter")
                    .enabled(parsed.is_some())
                    .show(ui)
                    .clicked()
                    && let Some(number) = parsed
                {
                    self.filters.push(ScreenerFilter {
                        column: columns[*column].id.clone(),
                        op: *op,
                        value: number,
                    });
                    value.clear();
                    changed = true;
                }
            });
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (index, filter) in self.filters.iter().enumerate() {
                let text = format!(
                    "{} {} {}  ×",
                    title(&filter.column),
                    op_label(filter.op),
                    filter.value
                );
                if Button::new(text)
                    .small()
                    .show(ui)
                    .on_hover_text("Remove filter")
                    .clicked()
                {
                    remove = Some(index);
                }
            }
        });
        if let Some(index) = remove {
            self.filters.remove(index);
            changed = true;
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no screener.");
            }
            Some(error) => {
                status_message(ui, MessageKind::Error, &format!("Screen failed: {error}"));
            }
            None if self.loading && self.rows.is_empty() => {
                LoadingIndicator::new().message("Screening").show(ui);
            }
            None if self.rows.is_empty() => {
                EmptyState::new("No symbols pass these filters")
                    .icon(&icons::WIDGET_BAR_SCREENER)
                    .fill_height()
                    .show(ui);
            }
            None => {}
        }
        let shown = self
            .sets
            .get(self.answered)
            .map_or(&[][..], |set| &set.columns);
        if !self.rows.is_empty() {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let width = ui.available_width().max(80.0 + shown.len() as f32 * 60.0);
                    ui.set_min_width(width);
                    let columns = |rect: egui::Rect| {
                        let symbol = 80.0;
                        let number = (width - symbol) / shown.len().max(1) as f32;
                        std::iter::once(rect.with_max_x(rect.left() + symbol))
                            .chain((0..shown.len()).map(|i| {
                                rect.with_min_x(rect.left() + symbol + i as f32 * number)
                                    .with_max_x(rect.left() + symbol + (i + 1) as f32 * number)
                            }))
                            .map(|rect| rect.shrink2(vec2(6.0, 0.0)))
                            .collect::<Vec<_>>()
                    };
                    let (header, _) =
                        ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                    let cells = columns(header);
                    cell(ui, cells[0], "Symbol", false, theme.ui.text_secondary);
                    for (index, column) in shown.iter().enumerate() {
                        let sorted = self.sort_by.as_deref() == Some(column.id.as_str());
                        let mut rect = cells[index + 1];
                        if sorted {
                            let icon = egui::Rect::from_center_size(
                                egui::pos2(rect.right() - 6.0, rect.center().y),
                                vec2(12.0, 12.0),
                            );
                            let chevron = if self.descending {
                                &icons::CHEVRON_DOWN
                            } else {
                                &icons::CHEVRON_UP
                            };
                            chevron
                                .as_image_tinted(icon.size(), theme.ui.text_secondary)
                                .paint_at(ui, icon);
                            rect.set_right(icon.left() - 2.0);
                        }
                        cell(ui, rect, &column.title, true, theme.ui.text_secondary);
                        let response = ui.interact(
                            cells[index + 1],
                            ui.scope_id().with(("sort", &column.id)),
                            Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &column.title)
                        });
                        if response.on_hover_text("Sort by this column").clicked() {
                            if sorted {
                                self.descending = !self.descending;
                            } else {
                                self.sort_by = Some(column.id.clone());
                                self.descending = true;
                            }
                            changed = true;
                        }
                    }
                    for entry in &self.rows {
                        let (rect, response) = row(ui);
                        let cells = columns(rect);
                        cell(ui, cells[0], &entry.symbol, false, theme.ui.text);
                        for (rect, value) in cells.iter().skip(1).zip(&entry.values) {
                            cell(ui, *rect, value, true, theme.ui.text);
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &entry.symbol)
                        });
                        if response.clicked() {
                            action = ScreenerAction::SelectSymbol(entry.symbol.clone());
                        }
                    }
                });
        }
        if changed {
            self.refresh();
        }
        action
    }
}

fn op_label(op: ScreenerOp) -> &'static str {
    match op {
        ScreenerOp::Above => ">",
        ScreenerOp::Below => "<",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply, ScreenerColumn};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed(Mutex<Vec<(ScreenerQuery, Reply<Vec<ScreenerRow>>)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
            self.0.lock().unwrap().push((query, reply));
        }
    }

    /// Guards what the feed is asked and what leaves the panel: the query names the shown set's
    /// columns, the filters and the sort; the export heads the answered rows with that set's
    /// titles even after the user switched sets, and quotes fields so a comma or quote in a value
    /// cannot shift the columns.
    #[test]
    fn queries_carry_the_screen_and_exports_quote_the_answered_rows() {
        let feed = Arc::new(Feed::default());
        let mut panel = ScreenerPanel::new(vec![
            ScreenerColumnSet::new(
                "Overview",
                vec![
                    ScreenerColumn::new("price", "Price"),
                    ScreenerColumn::new("name", "Name"),
                ],
            ),
            ScreenerColumnSet::new("Volume", vec![ScreenerColumn::new("volume", "Vol")]),
        ]);
        panel.filters.push(ScreenerFilter {
            column: "price".into(),
            op: ScreenerOp::Above,
            value: 10.0,
        });
        panel.sort_by = Some("price".into());
        panel.set_feed(feed.clone());
        let (query, reply) = feed.0.lock().unwrap().pop().unwrap();
        assert_eq!(query.columns, ["price", "name"]);
        assert_eq!(query.filters, panel.filters);
        assert_eq!(
            (query.sort_by.as_deref(), query.descending),
            (Some("price"), true)
        );
        reply.send(Ok(vec![ScreenerRow::new(
            "A\"B",
            vec!["1,234.50".into(), "Acme \"Co\"".into()],
        )]));
        panel.poll(&egui::Context::default());
        panel.set = 1;
        assert_eq!(
            panel.csv(),
            "\"Symbol\",\"Price\",\"Name\"\r\n\"A\"\"B\",\"1,234.50\",\"Acme \"\"Co\"\"\"\r\n"
        );
    }
}
