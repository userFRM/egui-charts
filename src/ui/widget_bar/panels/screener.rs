//! A screener table of the rows the feed finds for the user's filters.

use std::sync::Arc;

use egui::{RichText, Ui};

use crate::data::inbox::Inbox;
use crate::data::{
    Datafeed, FeedError, ScreenerColumnSet, ScreenerFilter, ScreenerOp, ScreenerQuery, ScreenerRow,
};
use crate::icons::icons;
use crate::theme::ThemeExt;
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
    draft: (usize, ScreenerOp, String),
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
            draft: (0, ScreenerOp::Above, String::new()),
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
            egui::ComboBox::from_id_salt("screener_set")
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for (index, set) in self.sets.iter().enumerate() {
                        ui.selectable_value(&mut self.set, index, &set.name);
                    }
                });
            if self.set != before {
                changed = true;
                self.draft.0 = 0;
            }
            if ui
                .add_enabled(has_rows, egui::Button::new("Send to watchlist"))
                .clicked()
            {
                action = ScreenerAction::SendToWatchlist(
                    self.rows.iter().map(|row| row.symbol.clone()).collect(),
                );
            }
            if ui
                .add_enabled(has_rows, egui::Button::new("Export CSV…"))
                .clicked()
            {
                action = ScreenerAction::ExportCsv(self.csv());
            }
        });
        if !columns.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let (column, op, value) = &mut self.draft;
                *column = (*column).min(columns.len() - 1);
                egui::ComboBox::from_id_salt("screener_filter_column")
                    .selected_text(&columns[*column].title)
                    .show_ui(ui, |ui| {
                        for (index, entry) in columns.iter().enumerate() {
                            ui.selectable_value(column, index, &entry.title);
                        }
                    });
                egui::ComboBox::from_id_salt("screener_filter_op")
                    .width(48.0)
                    .selected_text(op_label(*op))
                    .show_ui(ui, |ui| {
                        for entry in [ScreenerOp::Above, ScreenerOp::Below] {
                            ui.selectable_value(op, entry, op_label(entry));
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(56.0)
                        .hint_text("Value"),
                );
                let parsed = value.trim().parse::<f64>().ok().filter(|v| v.is_finite());
                if ui
                    .add_enabled(parsed.is_some(), egui::Button::new("Add filter"))
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
                if ui
                    .button(RichText::new(text).size(theme.text.small))
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
                    egui::Grid::new("screener_rows")
                        .striped(true)
                        .min_col_width(0.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Symbol").size(theme.text.small).strong());
                            for column in shown {
                                let sorted = self.sort_by.as_deref() == Some(column.id.as_str());
                                let clicked = ui
                                    .horizontal(|ui| {
                                        let clicked = ui
                                            .selectable_label(
                                                sorted,
                                                RichText::new(&column.title)
                                                    .size(theme.text.small)
                                                    .strong(),
                                            )
                                            .on_hover_text("Sort by this column")
                                            .clicked();
                                        if sorted {
                                            let chevron = if self.descending {
                                                &icons::CHEVRON_DOWN
                                            } else {
                                                &icons::CHEVRON_UP
                                            };
                                            ui.add(chevron.as_image_tinted(
                                                egui::Vec2::splat(theme.text.small),
                                                theme.ui.text_secondary,
                                            ));
                                        }
                                        clicked
                                    })
                                    .inner;
                                if clicked {
                                    if sorted {
                                        self.descending = !self.descending;
                                    } else {
                                        self.sort_by = Some(column.id.clone());
                                        self.descending = true;
                                    }
                                    changed = true;
                                }
                            }
                            ui.end_row();
                            for row in &self.rows {
                                if ui.link(&row.symbol).clicked() {
                                    action = ScreenerAction::SelectSymbol(row.symbol.clone());
                                }
                                for value in &row.values {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| ui.label(value),
                                    );
                                }
                                ui.end_row();
                            }
                        });
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
