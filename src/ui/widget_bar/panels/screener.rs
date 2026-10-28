//! A screener table of the rows the feed finds for the user's filters, universe and description.

use crate::localization as l10n;
use std::sync::Arc;

use egui::{Sense, Ui, vec2};
use serde::{Deserialize, Serialize};

use super::controls::{cell, dropdown, row};
use super::watchlist::ROW_HEIGHT;
use crate::data::inbox::Inbox;
use crate::data::{
    Datafeed, FeedError, ScreenerColumnSet, ScreenerFilter, ScreenerOp, ScreenerQuery, ScreenerRow,
    ScreenerTextAnswer, ScreenerUniverse,
};
use crate::icons::icons;
use crate::services::{Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui::dialogs::templates::TemplateStore;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// Rows asked for at most.
const LIMIT: usize = 200;

/// An action the host performs for the screener.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
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

/// A screen the user named, as kept in [`StoreKind::Screens`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedScreen {
    universe: Option<String>,
    /// Name of the column set.
    set: String,
    filters: Vec<ScreenerFilter>,
    sort_by: Option<String>,
    descending: bool,
    text: String,
}

/// A screener fed by the host's [`Datafeed::screen`].
///
/// The host gives the column sets; the user picks one, adds filters on its columns and sorts by
/// a column by clicking its heading. With [`set_universes`](Self::set_universes) the user also
/// picks the market to screen, and with [`set_text_query`](Self::set_text_query) describes the
/// screen in words, which the feed interprets in [`Datafeed::screen_text`] and explains above the
/// rows. [`set_storage`](Self::set_storage) lets the user save, rename, load and delete named
/// screens. Each change asks the feed again, which filters, sorts and limits, and only the answer
/// to the latest request is shown. A full answer offers the next page of the same screen. The
/// chosen criteria stay through loading, failures, paging and export. Call [`poll`](Self::poll)
/// each frame, then [`show`](Self::show) while the panel is open.
pub struct ScreenerPanel {
    sets: Vec<ScreenerColumnSet>,
    set: usize,
    universes: Vec<ScreenerUniverse>,
    universe: Option<String>,
    text_query: bool,
    /// The description being written, and the one the rows answer.
    description: FilterWidget,
    text: String,
    explanation: String,
    saved: TemplateStore<SavedScreen>,
    stored: bool,
    /// The saved screen last saved or loaded, and the name being written.
    current: Option<String>,
    name: FilterWidget,
    /// The name a renamed screen had, deleted once the save under its new name succeeds.
    renaming: Option<String>,
    more: bool,
    appending: bool,
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
            universes: Vec::new(),
            universe: None,
            text_query: false,
            description: FilterWidget::new().placeholder("Describe a screen"),
            text: String::new(),
            explanation: String::new(),
            saved: TemplateStore::new(StoreKind::Screens),
            stored: false,
            current: None,
            name: FilterWidget::new().placeholder("Screen name"),
            renaming: None,
            more: false,
            appending: false,
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

    /// Offer `universes` as the markets to screen, the first at first. The chosen one stays chosen
    /// while it is still offered.
    pub fn set_universes(&mut self, universes: Vec<ScreenerUniverse>) {
        let before = self.universe.clone();
        if !universes
            .iter()
            .any(|u| Some(&u.id) == self.universe.as_ref())
        {
            self.universe = universes.first().map(|u| u.id.clone());
        }
        self.universes = universes;
        if self.universe != before {
            self.refresh();
        }
    }

    /// Offer a field to describe the screen in words, which the feed interprets in
    /// [`Datafeed::screen_text`].
    pub fn set_text_query(&mut self, enabled: bool) {
        self.text_query = enabled;
    }

    /// Keep named screens in `storage`, in [`StoreKind::Screens`]. Poll every frame, as the panel
    /// is.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.stored = true;
        self.saved.set_storage(storage);
    }

    fn query(&self, offset: usize) -> ScreenerQuery {
        ScreenerQuery {
            universe: self.universe.clone(),
            text: self.text.clone(),
            offset,
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
        self.ask(0);
    }

    /// Ask for the page after the rows shown, of the same screen.
    fn load_more(&mut self) {
        self.ask(self.rows.len());
    }

    fn ask(&mut self, offset: usize) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        self.appending = offset > 0;
        let query = self.query(offset);
        if self.text.is_empty() {
            feed.screen(query, self.inbox.reply(self.request));
        } else {
            feed.screen_text(query, self.inbox.reply(self.request));
        }
    }

    fn saved_screen(&self) -> SavedScreen {
        SavedScreen {
            universe: self.universe.clone(),
            set: self
                .sets
                .get(self.set)
                .map_or_else(String::new, |set| set.name.clone()),
            filters: self.filters.clone(),
            sort_by: self.sort_by.clone(),
            descending: self.descending,
            text: self.text.clone(),
        }
    }

    /// Make the criteria of `saved` the panel's and ask the feed.
    fn load_screen(&mut self, name: &str) {
        let Some(saved) = self.saved.entries().get(name).cloned() else {
            return;
        };
        let Some(set) = self.sets.iter().position(|set| set.name == saved.set) else {
            self.saved.error = Some(format!("The column set {} is not available", saved.set));
            return;
        };
        self.set = set;
        self.draft.0 = 0;
        self.universe = saved.universe;
        self.filters = saved.filters;
        self.sort_by = saved.sort_by;
        self.descending = saved.descending;
        *self.description.query_mut() = saved.text.clone();
        self.text = saved.text;
        *self.name.query_mut() = name.to_owned();
        self.current = Some(name.to_owned());
        self.saved.error = None;
        self.refresh();
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        self.saved.poll(ctx);
        if self.saved.ready()
            && let Some(old) = self.renaming.take()
            && self.saved.error.is_none()
        {
            let _ = self.saved.delete(&old);
        }
        for (key, answer) in self.inbox.drain().answers {
            if key != self.request {
                continue;
            }
            self.loading = false;
            let answer = match answer.downcast::<Result<Vec<ScreenerRow>, FeedError>>() {
                Ok(rows) => (*rows).map(|rows| ScreenerTextAnswer::new(rows, "")),
                Err(answer) => *answer
                    .downcast::<Result<ScreenerTextAnswer, FeedError>>()
                    .expect("screener rows"),
            };
            match answer {
                Ok(answer) => {
                    self.more = answer.rows.len() >= LIMIT;
                    if self.appending {
                        self.rows.extend(answer.rows);
                    } else {
                        self.rows = answer.rows;
                        self.explanation = answer.explanation;
                    }
                    self.answered = self.set;
                    self.error = None;
                }
                Err(error) => {
                    // A failed next page leaves the rows already shown.
                    if !self.appending {
                        self.rows.clear();
                        self.explanation.clear();
                        self.more = false;
                    }
                    self.error = Some(error);
                }
            }
            self.appending = false;
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
                    ui.selectable_value(&mut self.set, index, l10n::text(ui.ctx(), &set.name));
                }
            });
            if self.set != before {
                changed = true;
                self.draft.0 = 0;
            }
            if !self.universes.is_empty() {
                let before = self.universe.clone();
                let shown = self
                    .universes
                    .iter()
                    .find(|u| Some(&u.id) == self.universe.as_ref())
                    .map_or("Market", |u| u.name.as_str())
                    .to_owned();
                dropdown(ui, "screener_universe", &shown, |ui| {
                    for universe in &self.universes {
                        ui.selectable_value(
                            &mut self.universe,
                            Some(universe.id.clone()),
                            l10n::text(ui.ctx(), &universe.name),
                        );
                    }
                });
                changed |= self.universe != before;
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
        if self.text_query {
            ui.horizontal_wrapped(|ui| {
                self.description = std::mem::take(&mut self.description)
                    .placeholder("Describe a screen")
                    .show_icon(false);
                let response = ui
                    .allocate_ui(vec2(160.0, 28.0), |ui| self.description.show(ui).response)
                    .inner;
                let written = self.description.query().trim().to_owned();
                let entered =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (entered
                    || Button::new("Ask")
                        .enabled(written != self.text)
                        .show(ui)
                        .clicked())
                    && written != self.text
                {
                    self.text = written;
                    changed = true;
                }
            });
            if !self.text.is_empty() && !self.explanation.is_empty() {
                ui.label(l10n::text(ui.ctx(), &self.explanation));
            }
        }
        if self.stored {
            let mut load = None;
            let (mut save, mut rename, mut delete) = (false, false, false);
            ui.horizontal_wrapped(|ui| {
                if !self.saved.entries().is_empty() {
                    dropdown(
                        ui,
                        "screener_saved",
                        self.current.as_deref().unwrap_or("Saved screens"),
                        |ui| {
                            for name in self.saved.entries().keys() {
                                let on = self.current.as_deref() == Some(name.as_str());
                                if ui
                                    .selectable_label(on, l10n::text(ui.ctx(), name))
                                    .clicked()
                                {
                                    load = Some(name.clone());
                                }
                            }
                        },
                    );
                }
                self.name = std::mem::take(&mut self.name)
                    .placeholder("Screen name")
                    .show_icon(false);
                ui.allocate_ui(vec2(128.0, 28.0), |ui| {
                    self.name.show(ui);
                });
                let named = !self.name.query().trim().is_empty();
                let ready = self.saved.ready() && named;
                save = Button::new("Save").enabled(ready).show(ui).clicked();
                rename = Button::new("Rename")
                    .enabled(ready && self.current.is_some())
                    .show(ui)
                    .clicked();
                delete = Button::new("Delete")
                    .enabled(self.saved.ready() && self.current.is_some())
                    .show(ui)
                    .clicked();
            });
            if let Some(name) = load {
                self.load_screen(&name);
            }
            let name = self.name.query().trim().to_owned();
            if save && self.saved.save(&name, self.saved_screen()).is_ok() {
                self.current = Some(name.clone());
            }
            if rename
                && let Some(old) = self.current.clone()
                && old != name
                && self.saved.save(&name, self.saved_screen()).is_ok()
            {
                self.renaming = Some(old);
                self.current = Some(name);
            }
            if delete
                && let Some(old) = self.current.clone()
                && self.saved.delete(&old).is_ok()
            {
                self.current = None;
            }
        }
        if self.stored
            && let Some(error) = &self.saved.error
        {
            status_message(ui, MessageKind::Error, &format!("Saved screens: {error}"));
        }
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
                            ui.selectable_value(column, index, l10n::text(ui.ctx(), &entry.title));
                        }
                    },
                );
                dropdown(ui, "screener_filter_op", op_label(*op), |ui| {
                    for entry in [ScreenerOp::Above, ScreenerOp::Below] {
                        ui.selectable_value(op, entry, l10n::text(ui.ctx(), op_label(entry)));
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
                    .on_hover_text(l10n::text(ui.ctx(), "Remove filter"))
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
            Some(FeedError::NotSupported) if !self.text.is_empty() => {
                status_message(
                    ui,
                    MessageKind::Info,
                    "This feed takes no screen descriptions.",
                );
            }
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
        let mut more = false;
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
                        let translated_label = l10n::text(ui.ctx(), &column.title);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                        });
                        if response
                            .on_hover_text(l10n::text(ui.ctx(), "Sort by this column"))
                            .clicked()
                        {
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
                        let translated_label = l10n::text(ui.ctx(), &entry.symbol);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                        });
                        if response.clicked() {
                            action = ScreenerAction::SelectSymbol(entry.symbol.clone());
                        }
                    }
                    if self.more && !self.loading {
                        more = Button::new("Show more").show(ui).clicked();
                    }
                });
        }
        if more {
            self.load_more();
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
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed {
        rows: Mutex<Vec<(ScreenerQuery, Reply<Vec<ScreenerRow>>)>>,
        text: Mutex<Vec<(ScreenerQuery, Reply<ScreenerTextAnswer>)>>,
    }

    #[derive(Default)]
    struct Documents(Mutex<std::collections::BTreeMap<String, String>>);

    impl Storage for Documents {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self.0.lock().unwrap().keys().cloned().collect()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().insert(name.into(), document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, name: &str, reply: Reply<()>) {
            self.0.lock().unwrap().remove(name);
            reply.send(Ok(()));
        }
    }

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
            self.rows.lock().unwrap().push((query, reply));
        }
        fn screen_text(&self, query: ScreenerQuery, reply: Reply<ScreenerTextAnswer>) {
            self.text.lock().unwrap().push((query, reply));
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
        let (query, reply) = feed.rows.lock().unwrap().pop().unwrap();
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

    fn sets() -> Vec<ScreenerColumnSet> {
        vec![
            ScreenerColumnSet::new("Overview", vec![ScreenerColumn::new("price", "Price")]),
            ScreenerColumnSet::new("Volume", vec![ScreenerColumn::new("volume", "Vol")]),
        ]
    }

    /// Guards the criteria of a screen: the universe and description go out with every request,
    /// the next page repeats them with an offset, a failed page keeps the rows and the criteria,
    /// and a saved screen restored in a new panel reproduces the complete request, also after a
    /// rename.
    #[test]
    fn universe_description_and_saved_screens_reproduce_the_complete_request() {
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let documents = Arc::new(Documents::default());
        let mut panel = ScreenerPanel::new(sets());
        panel.set_universes(vec![
            ScreenerUniverse::new("stock", "Stocks"),
            ScreenerUniverse::new("coin", "Coins"),
        ]);
        assert_eq!(panel.universe.as_deref(), Some("stock"));
        panel.universe = Some("coin".into());
        panel.set = 1;
        panel.text = "cheap and liquid".into();
        panel.filters.push(ScreenerFilter {
            column: "volume".into(),
            op: ScreenerOp::Below,
            value: 5.0,
        });
        panel.sort_by = Some("volume".into());
        panel.descending = false;
        panel.set_feed(feed.clone());
        assert!(feed.rows.lock().unwrap().is_empty());
        let (first, reply) = feed.text.lock().unwrap().pop().unwrap();
        assert_eq!(
            (first.universe.as_deref(), first.text.as_str(), first.offset),
            (Some("coin"), "cheap and liquid", 0)
        );
        let page = |n: usize| {
            (0..n)
                .map(|i| ScreenerRow::new(format!("S{i}"), vec!["1".into()]))
                .collect::<Vec<_>>()
        };
        reply.send(Ok(ScreenerTextAnswer::new(page(LIMIT), "volume below 5")));
        panel.poll(&ctx);
        assert!(panel.more && panel.explanation == "volume below 5");

        panel.load_more();
        let (next, reply) = feed.text.lock().unwrap().pop().unwrap();
        assert_eq!(
            (next.offset, ScreenerQuery { offset: 0, ..next }),
            (LIMIT, first.clone())
        );
        reply.send(Err(FeedError::Network("down".into())));
        panel.poll(&ctx);
        assert_eq!(panel.rows.len(), LIMIT);
        assert!(panel.error.is_some() && panel.explanation == "volume below 5");
        panel.load_more();
        let (_, reply) = feed.text.lock().unwrap().pop().unwrap();
        reply.send(Ok(ScreenerTextAnswer::new(page(3), "volume below 5")));
        panel.poll(&ctx);
        assert_eq!((panel.rows.len(), panel.more), (LIMIT + 3, false));
        assert!(panel.error.is_none());

        panel.set_storage(documents.clone());
        for _ in 0..3 {
            panel.poll(&ctx);
        }
        panel.saved.save("Cheap", panel.saved_screen()).unwrap();
        panel.poll(&ctx);
        let mut restored = ScreenerPanel::new(sets());
        restored.set_storage(documents.clone());
        for _ in 0..4 {
            restored.poll(&ctx);
        }
        restored.load_screen("Cheap");
        assert_eq!(restored.query(0), first);
        // A feed without descriptions is told apart from one without a screener.
        restored.set_feed(feed.clone());
        let (_, reply) = feed.text.lock().unwrap().pop().unwrap();
        reply.send(Err(FeedError::NotSupported));
        restored.poll(&ctx);
        let mut h = Harness::builder()
            .with_context_setup(|ctx| egui_extras::install_image_loaders(ctx))
            .build_ui_state(|ui, panel: &mut ScreenerPanel| _ = panel.show(ui), restored);
        h.run();
        h.get_by_label("This feed takes no screen descriptions.");

        panel.saved.save("Cheaper", panel.saved_screen()).unwrap();
        panel.renaming = Some("Cheap".into());
        for _ in 0..3 {
            panel.poll(&ctx);
        }
        let names: Vec<_> = documents.0.lock().unwrap().keys().cloned().collect();
        assert_eq!(names, ["Cheaper"]);
    }
}
