//! A screener table of the rows the feed finds for the user's filters, universe and description.

use crate::localization as l10n;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use egui::{Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::ROW_HEIGHT;
use super::controls::{self, cell, dropdown, row};
use crate::data::inbox::Inbox;
use crate::data::{
    Datafeed, FeedError, HistoryPage, HistoryRequest, ScreenerColumn, ScreenerColumnKind,
    ScreenerColumnSet, ScreenerCondition, ScreenerFilter, ScreenerQuery, ScreenerRow,
    ScreenerTextAnswer, ScreenerUniverse,
};
use crate::icons::icons;
use crate::model::{Bar, Timeframe};
use crate::services::{ConflictChoice, Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui::dialogs::templates::TemplateStore;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, status_message, translated_status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// Rows asked for at most.
const LIMIT: usize = 200;
/// Width kept free at the right of the table for the "+" that adds a column.
const ADD_WIDTH: f32 = 28.0;
/// Daily closes in each small chart, about three months of trading days.
const CHART_BARS: usize = 60;
/// Height of a row in chart view.
const CHART_ROW_HEIGHT: f32 = 48.0;

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
    /// [`WatchlistPanel::import`](crate::ui::widget_bar::WatchlistPanel::import).
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
    /// The ids of the columns shown, in order, or `None` in screens saved before columns could be
    /// shaped, which show their set's columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    columns: Option<Vec<String>>,
    /// How the rows show; a table in screens saved before the chart view.
    #[serde(default, skip_serializing_if = "View::is_table")]
    view: View,
}

/// A screener fed by the host's [`Datafeed::screen`].
///
/// The host gives the column sets; the user picks one, adds filters on its columns and sorts by
/// a column by clicking its heading. A heading's menu sorts by its column, moves it left or right
/// or removes it, and the "+" after the last heading adds any column of the host's sets; picking
/// a set again restores its columns. The user shows the rows as a table or as one small chart per
/// symbol of its last 60 daily closes, which the panel asks [`Datafeed::history`] for while the
/// rows are on screen, and can have the shown panel ask the feed again every 10 seconds or every
/// minute, keeping the rows and the pages already loaded until the answer. With
/// [`set_universes`](Self::set_universes) the user also picks the market to screen, and with
/// [`set_text_query`](Self::set_text_query) describes the screen in words, which the feed
/// interprets in [`Datafeed::screen_text`] and explains above the rows.
/// [`set_storage`](Self::set_storage) lets the user save, rename, load and delete named screens,
/// each with its columns and view. Each change asks the feed again, which filters, sorts and
/// limits, and only the answer to the latest request is shown. A full answer offers the next page
/// of the same screen. The chosen criteria stay through loading, failures, paging and export.
/// Call [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is open.
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
    /// The columns shown and asked for, in order: the chosen set's until the user shapes them.
    columns: Vec<ScreenerColumn>,
    /// How the rows show.
    view: View,
    /// How often the shown panel asks again by itself.
    auto_refresh: AutoRefresh,
    /// The input time of the latest answer, from which the timer counts.
    answered_at: Option<f64>,
    /// Rows asked for by the open request, which decides whether a full answer offers more.
    limit: usize,
    draft: Draft,
    feed: Option<Arc<dyn Datafeed>>,
    /// The latest answer and the columns it answers.
    rows: Vec<ScreenerRow>,
    answered: Vec<ScreenerColumn>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
    /// The small chart of each shown symbol, in chart view.
    sparks: HashMap<String, Spark>,
    /// Counts the first-page answers, so each one asks the shown charts again once.
    generation: u64,
    /// Answers to the charts' history requests, keyed by `Spark::request`.
    chart_inbox: Arc<Inbox>,
    chart_request: u64,
}

impl std::fmt::Debug for ScreenerPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenerPanel")
            .field("sets", &self.sets)
            .field("set", &self.set)
            .field("universes", &self.universes)
            .field("universe", &self.universe)
            .field("text_query", &self.text_query)
            .field("description", &self.description)
            .field("text", &self.text)
            .field("explanation", &self.explanation)
            .field("saved", &self.saved)
            .field("stored", &self.stored)
            .field("current", &self.current)
            .field("name", &self.name)
            .field("renaming", &self.renaming)
            .field("more", &self.more)
            .field("appending", &self.appending)
            .field("filters", &self.filters)
            .field("sort_by", &self.sort_by)
            .field("descending", &self.descending)
            .field("columns", &self.columns)
            .field("view", &self.view)
            .field("auto_refresh", &self.auto_refresh)
            .field("answered_at", &self.answered_at)
            .field("limit", &self.limit)
            .field("draft", &self.draft)
            .field("rows", &self.rows)
            .field("answered", &self.answered)
            .field("error", &self.error)
            .field("loading", &self.loading)
            .field("request", &self.request)
            .field("sparks", &self.sparks)
            .field("generation", &self.generation)
            .field("chart_request", &self.chart_request)
            .finish_non_exhaustive()
    }
}

impl ScreenerPanel {
    /// A screener of `sets`, showing the first, without a feed.
    pub fn new(sets: Vec<ScreenerColumnSet>) -> Self {
        let columns = sets
            .first()
            .map_or_else(Vec::new, |set| set.columns.clone());
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
            columns,
            view: View::Table,
            auto_refresh: AutoRefresh::Never,
            answered_at: None,
            limit: LIMIT,
            draft: Draft {
                column: 0,
                comparison: Comparison::Above,
                value: FilterWidget::new().placeholder("Value"),
                upper: FilterWidget::new().placeholder("To"),
                values: Vec::new(),
            },
            feed: None,
            rows: Vec::new(),
            answered: Vec::new(),
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
            sparks: HashMap::new(),
            generation: 0,
            chart_inbox: Arc::default(),
            chart_request: 0,
        }
    }

    /// Ask `feed` for rows from now on, starting at once.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        // Another feed's closes are not this one's.
        self.sparks.clear();
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

    /// The saved screen whose write the host refused because another client changed or created
    /// it; see [`TemplateStore::storage_conflict`].
    pub fn storage_conflict(&self) -> Option<&str> {
        self.saved.storage_conflict()
    }

    /// Settles the refused write of the saved screen `name`; see
    /// [`TemplateStore::resolve_storage_conflict`]. Only overwriting completes a rename refused
    /// so: otherwise the screen keeps its old name.
    pub fn resolve_storage_conflict(&mut self, name: &str, choice: ConflictChoice) {
        if choice != ConflictChoice::Retry && self.saved.storage_conflict() == Some(name) {
            self.renaming = None;
        }
        self.saved.resolve_storage_conflict(name, choice);
    }

    fn query(&self, offset: usize) -> ScreenerQuery {
        ScreenerQuery {
            universe: self.universe.clone(),
            text: self.text.clone(),
            offset,
            columns: self
                .columns
                .iter()
                .map(|column| column.id.clone())
                .collect(),
            filters: self.filters.clone(),
            sort_by: self.sort_by.clone(),
            descending: self.descending,
            limit: LIMIT,
        }
    }

    /// Every column of the host's sets, each id once, first occurrence first, in set order.
    fn offered(&self) -> Vec<ScreenerColumn> {
        let mut offered: Vec<ScreenerColumn> = Vec::new();
        for column in self.sets.iter().flat_map(|set| &set.columns) {
            if !offered.iter().any(|c| c.id == column.id) {
                offered.push(column.clone());
            }
        }
        offered
    }

    /// Show the set at `index` with its own columns.
    fn pick_set(&mut self, index: usize) {
        self.set = index;
        self.columns = self
            .sets
            .get(index)
            .map_or_else(Vec::new, |set| set.columns.clone());
        self.draft.column = 0;
    }

    /// Ask the feed again, such as when the host's data changed.
    pub fn refresh(&mut self) {
        self.ask(0, LIMIT);
    }

    /// Ask for the page after the rows shown, of the same screen.
    fn load_more(&mut self) {
        self.ask(self.rows.len(), LIMIT);
    }

    fn ask(&mut self, offset: usize, limit: usize) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        self.appending = offset > 0;
        self.limit = limit;
        let mut query = self.query(offset);
        query.limit = limit;
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
            columns: Some(
                self.columns
                    .iter()
                    .map(|column| column.id.clone())
                    .collect(),
            ),
            view: self.view,
        }
    }

    /// Make the criteria of `saved` the panel's and ask the feed.
    fn load_screen(&mut self, ctx: &egui::Context, name: &str) {
        let Some(saved) = self.saved.entries().get(name).cloned() else {
            return;
        };
        let Some(set) = self.sets.iter().position(|set| set.name == saved.set) else {
            self.saved.error = Some(l10n::format_text(
                ctx,
                "The column set {name} is not available",
                &[("name", saved.set.as_str())],
            ));
            return;
        };
        let set_columns = &self.sets[set].columns;
        let columns = match &saved.columns {
            None => set_columns.clone(),
            Some(ids) => {
                let offered = self.offered();
                let mut columns = Vec::new();
                for id in ids {
                    // The saved set's own column first, so a title it gives an id survives.
                    let Some(column) = set_columns.iter().chain(&offered).find(|c| &c.id == id)
                    else {
                        self.saved.error = Some(l10n::format_text(
                            ctx,
                            "The column {name} is not available",
                            &[("name", id.as_str())],
                        ));
                        return;
                    };
                    columns.push(column.clone());
                }
                columns
            }
        };
        self.pick_set(set);
        self.columns = columns;
        self.view = saved.view;
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
        let repaint = ctx.clone();
        self.chart_inbox
            .set_waker(move || repaint.request_repaint());
        self.saved.poll(ctx);
        let now = ctx.input(|i| i.time);
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
            self.answered_at = Some(now);
            let answer = match answer.downcast::<Result<Vec<ScreenerRow>, FeedError>>() {
                Ok(rows) => (*rows).map(|rows| ScreenerTextAnswer::new(rows, "")),
                Err(answer) => *answer
                    .downcast::<Result<ScreenerTextAnswer, FeedError>>()
                    .expect("screener rows"),
            };
            match answer {
                Ok(answer) => {
                    self.more = answer.rows.len() >= self.limit;
                    if self.appending {
                        self.rows.extend(answer.rows);
                    } else {
                        self.rows = answer.rows;
                        self.explanation = answer.explanation;
                        // A new screen asks the shown charts again once.
                        self.generation += 1;
                        let rows = &self.rows;
                        self.sparks
                            .retain(|symbol, _| rows.iter().any(|row| &row.symbol == symbol));
                    }
                    self.answered = self.columns.clone();
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
        for (key, answer) in self.chart_inbox.drain().answers {
            let Some(spark) = self.sparks.values_mut().find(|spark| spark.request == key) else {
                continue;
            };
            spark.closes = match *answer
                .downcast::<Result<HistoryPage, FeedError>>()
                .expect("daily closes")
            {
                Ok(page) => closes(page.bars),
                Err(_) => Vec::new(),
            };
        }
    }

    /// The shown rows as CSV, headed by Symbol and the column titles. Every field is quoted and
    /// embedded quotes doubled.
    fn csv(&self) -> String {
        let field = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
        let mut csv = std::iter::once(field("Symbol"))
            .chain(self.answered.iter().map(|column| field(&column.title)))
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
        let name = set.name.clone();
        let columns = self.columns.clone();
        let offered = self.offered();
        // A removed column's filter keeps its title.
        let title = |id: &str| {
            columns
                .iter()
                .chain(&offered)
                .find(|column| column.id == id)
                .map_or(id.to_owned(), |column| column.title.clone())
        };
        ui.horizontal_wrapped(|ui| {
            let mut picked = None;
            dropdown(ui, "screener_set", &name, |ui| {
                for (index, set) in self.sets.iter().enumerate() {
                    let text = l10n::text(ui.ctx(), &set.name);
                    if ui.selectable_label(index == self.set, text).clicked() {
                        picked = Some(index);
                    }
                }
            });
            // Picking the chosen set again restores its columns.
            if let Some(index) = picked {
                self.pick_set(index);
                changed = true;
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
            for (view, label) in [(View::Table, "Table"), (View::Chart, "Chart")] {
                if Button::new(label)
                    .small()
                    .selected(self.view == view)
                    .show(ui)
                    .clicked()
                {
                    self.view = view;
                }
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
            // Last and sized whole: a narrow panel wraps it under the actions instead of cutting
            // the choice short.
            controls::full_dropdown(
                ui,
                "screener_auto_refresh",
                self.auto_refresh.label(),
                |ui| {
                    for choice in AutoRefresh::ALL {
                        let text = l10n::text(ui.ctx(), choice.label());
                        ui.selectable_value(&mut self.auto_refresh, choice, text);
                    }
                },
            );
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
                self.load_screen(ui.ctx(), &name);
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
            translated_status_message(
                ui,
                MessageKind::Error,
                l10n::format_text(
                    ui.ctx(),
                    "Saved screens: {error}",
                    &[("error", error.as_str())],
                ),
            );
        }
        if !columns.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let draft = &mut self.draft;
                draft.column = draft.column.min(columns.len() - 1);
                let offered = match &columns[draft.column].kind {
                    ScreenerColumnKind::Text(values) => {
                        if !draft.values.iter().map(|(code, _)| code).eq(values) {
                            draft.values =
                                values.iter().map(|code| (code.clone(), false)).collect();
                        }
                        &Comparison::TEXT[..]
                    }
                    ScreenerColumnKind::Number => &Comparison::NUMBER[..],
                };
                if !offered.contains(&draft.comparison) {
                    draft.comparison = offered[0];
                }
                dropdown(
                    ui,
                    "screener_filter_column",
                    &columns[draft.column].title,
                    |ui| {
                        for (index, entry) in columns.iter().enumerate() {
                            ui.selectable_value(
                                &mut draft.column,
                                index,
                                l10n::text(ui.ctx(), &entry.title),
                            );
                        }
                    },
                );
                dropdown(ui, "screener_filter_op", draft.comparison.label(), |ui| {
                    for entry in offered {
                        ui.selectable_value(
                            &mut draft.comparison,
                            *entry,
                            l10n::text(ui.ctx(), entry.label()),
                        );
                    }
                });
                match draft.comparison {
                    Comparison::Above | Comparison::Below => {
                        draft.value = std::mem::take(&mut draft.value).placeholder("Value");
                        ui.allocate_ui(vec2(128.0, 28.0), |ui| draft.value.show(ui));
                    }
                    Comparison::Between => {
                        draft.value = std::mem::take(&mut draft.value).placeholder("From");
                        ui.allocate_ui(vec2(128.0, 28.0), |ui| draft.value.show(ui));
                        ui.allocate_ui(vec2(128.0, 28.0), |ui| draft.upper.show(ui));
                    }
                    Comparison::Is | Comparison::IsNot => controls::choices(
                        ui,
                        "screener_filter_values",
                        "Choose values",
                        "{count} values",
                        &mut draft.values,
                    ),
                }
                let number = |field: &FilterWidget| {
                    field
                        .query()
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                };
                let ticked = controls::chosen(&draft.values);
                let condition = match draft.comparison {
                    Comparison::Above => number(&draft.value).map(ScreenerCondition::Above),
                    Comparison::Below => number(&draft.value).map(ScreenerCondition::Below),
                    Comparison::Between => number(&draft.value)
                        .zip(number(&draft.upper))
                        .map(|(a, b)| ScreenerCondition::Between(a.min(b), a.max(b))),
                    Comparison::Is => (!ticked.is_empty()).then_some(ScreenerCondition::Is(ticked)),
                    Comparison::IsNot => {
                        (!ticked.is_empty()).then_some(ScreenerCondition::IsNot(ticked))
                    }
                };
                if Button::new("Add filter")
                    .enabled(condition.is_some())
                    .show(ui)
                    .clicked()
                    && let Some(condition) = condition
                {
                    self.filters.push(ScreenerFilter::new(
                        columns[draft.column].id.clone(),
                        condition,
                    ));
                    draft.value.clear();
                    draft.upper.clear();
                    for (_, on) in &mut draft.values {
                        *on = false;
                    }
                    changed = true;
                }
            });
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (index, filter) in self.filters.iter().enumerate() {
                let text = format!(
                    "{}  ×",
                    describe(ui.ctx(), &title(&filter.column), &filter.condition)
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
                translated_status_message(
                    ui,
                    MessageKind::Error,
                    l10n::format_text(
                        ui.ctx(),
                        "Screen failed: {error}",
                        &[("error", error.to_string().as_str())],
                    ),
                );
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
        let mut wanted: Vec<String> = Vec::new();
        let shown = &self.answered;
        if !self.rows.is_empty() && self.view == View::Chart {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for entry in &self.rows {
                        let (rect, response) = ui.allocate_exact_size(
                            vec2(ui.available_width(), CHART_ROW_HEIGHT),
                            Sense::click(),
                        );
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                        }
                        let symbol = rect.with_max_x(rect.left() + 80.0).shrink2(vec2(6.0, 0.0));
                        cell(ui, symbol, &entry.symbol, false, theme.ui.text);
                        let spark = self.sparks.get(&entry.symbol);
                        // Only rows on screen ask, once per screen answer.
                        if ui.is_rect_visible(rect)
                            && spark.is_none_or(|spark| spark.generation != self.generation)
                            && !wanted.contains(&entry.symbol)
                        {
                            wanted.push(entry.symbol.clone());
                        }
                        if let Some(spark) = spark
                            && let [first, .., last] = spark.closes.as_slice()
                        {
                            let color = if last > first {
                                theme.chart.candle_up
                            } else if last < first {
                                theme.chart.candle_down
                            } else {
                                theme.ui.text_secondary
                            };
                            let plot = rect.with_min_x(rect.left() + 80.0).shrink2(vec2(6.0, 8.0));
                            let line = sparkline(&spark.closes, plot);
                            ui.painter().add(Shape::line(line, Stroke::new(1.5, color)));
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
        } else if !self.rows.is_empty() {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let width = ui
                        .available_width()
                        .max(80.0 + shown.len() as f32 * 60.0 + ADD_WIDTH);
                    ui.set_min_width(width);
                    let columns = |rect: egui::Rect| {
                        let symbol = 80.0;
                        let number = (width - symbol - ADD_WIDTH) / shown.len().max(1) as f32;
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
                        let right = !matches!(column.kind, ScreenerColumnKind::Text(_));
                        cell(ui, rect, &column.title, right, theme.ui.text_secondary);
                        let response = ui.interact(
                            cells[index + 1],
                            ui.scope_id().with(("sort", &column.id)),
                            Sense::click(),
                        );
                        let translated_label = l10n::text(ui.ctx(), &column.title);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                        });
                        let response =
                            response.on_hover_text(l10n::text(ui.ctx(), "Sort by this column"));
                        if response.clicked() {
                            if sorted {
                                self.descending = !self.descending;
                            } else {
                                self.sort_by = Some(column.id.clone());
                                self.descending = true;
                            }
                            changed = true;
                        }
                        response.context_menu(|ui| {
                            // By id: the headings show the answered columns, which differ from
                            // the shown ones while an answer is pending.
                            let at = self.columns.iter().position(|c| c.id == column.id);
                            let last = self.columns.len().saturating_sub(1);
                            let entry = |ui: &mut Ui, label: &str, enabled: bool| {
                                let text = l10n::text(ui.ctx(), label);
                                ui.add_enabled(enabled, egui::Button::new(text)).clicked()
                            };
                            let down = entry(ui, "Sort descending", true);
                            let up = entry(ui, "Sort ascending", true);
                            let left = entry(ui, "Move left", at.is_some_and(|at| at > 0));
                            let right = entry(ui, "Move right", at.is_some_and(|at| at < last));
                            let remove = entry(ui, "Remove column", at.is_some());
                            if down || up {
                                self.sort_by = Some(column.id.clone());
                                self.descending = down;
                            }
                            if let Some(at) = at
                                && (left || right || remove)
                            {
                                if left {
                                    self.columns.swap(at, at - 1);
                                } else if right {
                                    self.columns.swap(at, at + 1);
                                } else {
                                    self.columns.remove(at);
                                    if self.sort_by.as_ref() == Some(&column.id) {
                                        self.sort_by = None;
                                    }
                                }
                                self.draft.column = 0;
                            }
                            if down || up || left || right || remove {
                                changed = true;
                                ui.close();
                            }
                        });
                    }
                    let missing: Vec<&ScreenerColumn> = offered
                        .iter()
                        .filter(|column| !self.columns.iter().any(|c| c.id == column.id))
                        .collect();
                    let add = egui::Rect::from_min_size(
                        egui::pos2(header.right() - ADD_WIDTH, header.top()),
                        vec2(ADD_WIDTH, ROW_HEIGHT),
                    );
                    let button = Button::new("+")
                        .small()
                        .tooltip("Add column")
                        .enabled(!missing.is_empty());
                    let button = ui.put(add, button);
                    egui::Popup::menu(&button).show(|ui| {
                        // A host may offer more columns than the window is tall.
                        egui::ScrollArea::vertical()
                            .max_height(ui.spacing().combo_height)
                            .show(ui, |ui| {
                                for column in missing {
                                    let text = l10n::text(ui.ctx(), &column.title);
                                    if ui.button(text).clicked() {
                                        self.columns.push(column.clone());
                                        changed = true;
                                        ui.close();
                                    }
                                }
                            });
                    });
                    for entry in &self.rows {
                        let (rect, response) = row(ui);
                        let cells = columns(rect);
                        cell(ui, cells[0], &entry.symbol, false, theme.ui.text);
                        for ((rect, value), column) in
                            cells.iter().skip(1).zip(&entry.values).zip(shown)
                        {
                            let right = !matches!(column.kind, ScreenerColumnKind::Text(_));
                            cell(ui, *rect, value, right, theme.ui.text);
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
        for symbol in wanted {
            let Some(feed) = &self.feed else { break };
            self.chart_request += 1;
            feed.history(
                HistoryRequest::new(symbol.clone(), Timeframe::Day1, CHART_BARS),
                self.chart_inbox.reply(self.chart_request),
            );
            let spark = self.sparks.entry(symbol).or_default();
            spark.generation = self.generation;
            spark.request = self.chart_request;
        }
        if more {
            self.load_more();
        }
        if changed {
            self.refresh();
        } else if let (Some(interval), Some(at)) = (self.auto_refresh.interval(), self.answered_at)
            && !self.loading
            && !matches!(self.error, Some(FeedError::NotSupported))
        {
            let now = ui.input(|i| i.time);
            // A clock behind the answer, or NaN, waits a full interval.
            let elapsed = Duration::try_from_secs_f64((now - at).max(0.0)).unwrap_or(Duration::MAX);
            let wait = interval.saturating_sub(elapsed);
            if wait.is_zero() {
                // Keep the pages on screen: ask from the first row for as many rows as they hold.
                self.ask(0, self.rows.len().div_ceil(LIMIT).max(1) * LIMIT);
            } else {
                ui.ctx().request_repaint_after(wait);
            }
        }
        action
    }
}

/// A comparison the filter row offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Above,
    Below,
    Between,
    Is,
    IsNot,
}

impl Comparison {
    const NUMBER: [Self; 3] = [Self::Above, Self::Below, Self::Between];
    const TEXT: [Self; 2] = [Self::Is, Self::IsNot];

    fn label(self) -> &'static str {
        match self {
            Self::Above => ">",
            Self::Below => "<",
            Self::Between => "between",
            Self::Is => "is",
            Self::IsNot => "is not",
        }
    }
}

/// How the screener shows its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum View {
    /// One row of values per symbol.
    #[default]
    Table,
    /// One small chart of recent daily closes per symbol.
    Chart,
}

impl View {
    fn is_table(&self) -> bool {
        *self == Self::Table
    }
}

/// How often the shown screener asks the feed again by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoRefresh {
    Never,
    TenSeconds,
    Minute,
}

impl AutoRefresh {
    const ALL: [Self; 3] = [Self::Never, Self::TenSeconds, Self::Minute];

    fn label(self) -> &'static str {
        match self {
            Self::Never => "No auto refresh",
            Self::TenSeconds => "Refresh every 10 seconds",
            Self::Minute => "Refresh every minute",
        }
    }

    fn interval(self) -> Option<Duration> {
        match self {
            Self::Never => None,
            Self::TenSeconds => Some(Duration::from_secs(10)),
            Self::Minute => Some(Duration::from_secs(60)),
        }
    }
}

/// The small chart of one symbol.
#[derive(Debug, Default)]
struct Spark {
    /// Daily closes, oldest first; empty until answered, or when the feed has none.
    closes: Vec<f64>,
    /// The `ScreenerPanel::generation` they were last asked for.
    generation: u64,
    /// The latest request for them; answers to older ones are dropped.
    request: u64,
}

/// The filter being written.
#[derive(Debug)]
struct Draft {
    /// The column, by its index within the shown columns.
    column: usize,
    comparison: Comparison,
    /// The number, or the lower end of a range.
    value: FilterWidget,
    /// The upper end of a range.
    upper: FilterWidget,
    /// The values of the drafted text column, ticked or not.
    values: Vec<(String, bool)>,
}

/// How a filter reads on its chip, such as "Price > 10" or "Exchange is NYSE, NASDAQ".
fn describe(ctx: &egui::Context, column: &str, condition: &ScreenerCondition) -> String {
    match condition {
        ScreenerCondition::Above(value) => format!("{column} > {value}"),
        ScreenerCondition::Below(value) => format!("{column} < {value}"),
        ScreenerCondition::Between(from, to) => l10n::format_text(
            ctx,
            "{column} between {from} and {to}",
            &[
                ("column", column.to_owned()),
                ("from", from.to_string()),
                ("to", to.to_string()),
            ],
        ),
        ScreenerCondition::Is(values) => l10n::format_text(
            ctx,
            "{column} is {values}",
            &[("column", column.to_owned()), ("values", values.join(", "))],
        ),
        ScreenerCondition::IsNot(values) => l10n::format_text(
            ctx,
            "{column} is not {values}",
            &[("column", column.to_owned()), ("values", values.join(", "))],
        ),
    }
}

/// The finite closes of `bars` in time order, the last of several bars at one time winning as on
/// the chart, keeping the newest `CHART_BARS`.
fn closes(mut bars: Vec<Bar>) -> Vec<f64> {
    bars.retain(|bar| bar.close.is_finite());
    // A stable sort, so the later of two bars at one time stays after the earlier.
    bars.sort_by_key(|bar| bar.time);
    bars.dedup_by(|later, earlier| {
        let same = later.time == earlier.time;
        if same {
            std::mem::swap(later, earlier);
        }
        same
    });
    let skip = bars.len().saturating_sub(CHART_BARS);
    bars[skip..].iter().map(|bar| bar.close).collect()
}

/// Where `closes` run across `rect`, evenly spaced, the lowest at the bottom and the highest at
/// the top; equal closes run through the middle. Fewer than two closes give no line.
fn sparkline(closes: &[f64], rect: Rect) -> Vec<Pos2> {
    if closes.len() < 2 {
        return Vec::new();
    }
    let (low, high) = closes
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), &close| {
            (low.min(close), high.max(close))
        });
    // Halves keep the range finite for closes near the ends of f64.
    let range = high / 2.0 - low / 2.0;
    let step = rect.width() / (closes.len() - 1) as f32;
    closes
        .iter()
        .enumerate()
        .map(|(index, &close)| {
            let share = if range > 0.0 {
                ((close / 2.0 - low / 2.0) / range) as f32
            } else {
                0.5
            };
            pos2(
                rect.left() + index as f32 * step,
                rect.bottom() - share * rect.height(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed {
        rows: Mutex<Vec<(ScreenerQuery, Reply<Vec<ScreenerRow>>)>>,
        text: Mutex<Vec<(ScreenerQuery, Reply<ScreenerTextAnswer>)>>,
        /// Each history request, with its reply until answered.
        histories: Mutex<Vec<(HistoryRequest, Option<Reply<HistoryPage>>)>>,
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
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.histories.lock().unwrap().push((request, Some(reply)));
        }
        fn screen(&self, query: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
            self.rows.lock().unwrap().push((query, reply));
        }
        fn screen_text(&self, query: ScreenerQuery, reply: Reply<ScreenerTextAnswer>) {
            self.text.lock().unwrap().push((query, reply));
        }
    }

    /// The panel, polled and shown each frame.
    fn harness(panel: ScreenerPanel) -> Harness<'static, ScreenerPanel> {
        Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, panel: &mut ScreenerPanel| {
                    panel.poll(ui.ctx());
                    _ = panel.show(ui);
                },
                panel,
            )
    }

    /// Clicks the last node labelled `label`. A control paints before the headings and a menu
    /// entry after them, so that is the heading or the entry.
    fn press(h: &mut Harness<'_, ScreenerPanel>, label: &str) {
        h.get_all_by_label(label).last().unwrap().click();
        h.run();
    }

    /// A daily bar `day` days after 2026-01-01, closing at `close`.
    fn bar(day: i64, close: f64) -> Bar {
        let time: chrono::DateTime<chrono::Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        Bar::new(
            time + chrono::TimeDelta::days(day),
            close,
            close,
            close,
            close,
            0.0,
        )
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
        panel
            .filters
            .push(ScreenerFilter::new("price", ScreenerCondition::Above(10.0)));
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
        panel.pick_set(1);
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
        panel.pick_set(1);
        panel.text = "cheap and liquid".into();
        panel.filters.extend([
            ScreenerFilter::new("volume", ScreenerCondition::Below(5.0)),
            ScreenerFilter::new("volume", ScreenerCondition::Between(1.0, 4.5)),
            ScreenerFilter::new("volume", ScreenerCondition::IsNot(vec!["OTC".into()])),
        ]);
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
        restored.load_screen(&egui::Context::default(), "Cheap");
        assert_eq!(restored.query(0), first);
        // A feed without descriptions is told apart from one without a screener.
        restored.set_feed(feed.clone());
        let (_, reply) = feed.text.lock().unwrap().pop().unwrap();
        reply.send(Err(FeedError::NotSupported));
        restored.poll(&ctx);
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
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

        // Screens saved before conditions carried ranges and text read and write unchanged.
        const BEFORE: &str = r#"{"universe":null,"set":"Overview","filters":[{"column":"price","op":"Above","value":10.0}],"sort_by":null,"descending":true,"text":""}"#;
        let saved: SavedScreen = serde_json::from_str(BEFORE).unwrap();
        assert_eq!(
            saved.filters,
            [ScreenerFilter::new("price", ScreenerCondition::Above(10.0))]
        );
        assert_eq!(serde_json::to_string(&saved).unwrap(), BEFORE);
        // They restore with their set's columns as a table, also in a panel showing another set
        // and view.
        documents.0.lock().unwrap().insert(
            "Old".into(),
            format!(r#"{{"version":1,"template":{BEFORE}}}"#),
        );
        let mut old = ScreenerPanel::new(sets());
        old.pick_set(1);
        old.view = View::Chart;
        old.set_storage(documents.clone());
        for _ in 0..4 {
            old.poll(&ctx);
        }
        old.load_screen(&ctx, "Old");
        let query = old.query(0);
        assert_eq!(
            (query.columns, query.filters, old.view),
            (vec!["price".to_owned()], saved.filters, View::Table)
        );
    }

    /// A text column's filter offers "is" and "is not" over the host's values and no number; a
    /// number column's offers a range. The query and the chips carry the ticked values in the
    /// order offered and the range from its smaller end, and a bound that is not a finite number
    /// adds nothing.
    #[test]
    fn text_conditions_and_ranges_reach_the_query_and_the_chips() {
        // The feed never answers, so the spinner keeps repainting: step instead of settling.
        fn click(h: &mut Harness<'_, ScreenerPanel>, label: &str) {
            h.get_by_label(label).click();
            h.run_steps(4);
        }
        fn type_into(h: &mut Harness<'_, ScreenerPanel>, label: &str, text: &str) {
            h.get_by_label(label).focus();
            h.run_steps(4);
            h.get_by_label(label).type_text(text);
            h.run_steps(4);
        }
        let feed = Arc::new(Feed::default());
        let mut panel = ScreenerPanel::new(vec![ScreenerColumnSet::new(
            "Overview",
            vec![
                ScreenerColumn::new("price", "Price"),
                ScreenerColumn::text(
                    "exchange",
                    "Exchange",
                    ["NYSE", "NASDAQ", "LSE"].map(String::from),
                ),
            ],
        )]);
        panel.set_feed(feed.clone());
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(|ui, panel: &mut ScreenerPanel| _ = panel.show(ui), panel);
        h.run_steps(4);
        click(&mut h, "Price");
        click(&mut h, "Exchange");
        assert!(h.query_by_label("Value").is_none());
        click(&mut h, "is");
        assert!(h.query_by_label(">").is_none() && h.query_by_label("between").is_none());
        click(&mut h, "is not");
        for value in ["NASDAQ", "NYSE"] {
            if h.query_by_role_and_label(egui::Role::CheckBox, value)
                .is_none()
            {
                let shown = if value == "NASDAQ" {
                    "Choose values"
                } else {
                    "NASDAQ"
                };
                click(&mut h, shown);
            }
            h.get_by_role_and_label(egui::Role::CheckBox, value).click();
            h.run_steps(4);
        }
        click(&mut h, "Add filter");
        let last = |feed: &Feed| feed.rows.lock().unwrap().last().unwrap().0.filters.clone();
        let is_not = ScreenerFilter::new(
            "exchange",
            ScreenerCondition::IsNot(vec!["NYSE".into(), "NASDAQ".into()]),
        );
        assert_eq!(last(&feed), std::slice::from_ref(&is_not));
        h.get_by_label("Exchange is not NYSE, NASDAQ  ×");

        click(&mut h, "Exchange");
        click(&mut h, "Price");
        click(&mut h, ">");
        click(&mut h, "between");
        type_into(&mut h, "From", "20");
        type_into(&mut h, "To", "10.5");
        click(&mut h, "Add filter");
        let between = ScreenerFilter::new("price", ScreenerCondition::Between(10.5, 20.0));
        assert_eq!(last(&feed), [is_not, between]);
        h.get_by_label("Price between 10.5 and 20  ×");

        type_into(&mut h, "From", "inf");
        type_into(&mut h, "To", "1");
        assert!(h.get_by_label("Add filter").accesskit_node().is_disabled());
    }

    /// Guards the shaped columns of a screen: a heading's menu moves and removes its column and
    /// the "+" adds any column of the host's sets, each change reaching the query at once;
    /// removing the sorted column drops the sort, a saved screen restores the shaped columns in a
    /// new panel, and picking the set again restores its own columns.
    #[test]
    fn header_menu_and_plus_shape_the_query_columns_and_saved_screens() {
        fn heading_menu(h: &mut Harness<'_, ScreenerPanel>, heading: &str) {
            h.get_all_by_role_and_label(egui::Role::Button, heading)
                .last()
                .unwrap()
                .click_secondary();
            h.run();
        }
        let sets = || {
            vec![
                ScreenerColumnSet::new(
                    "Overview",
                    vec![
                        ScreenerColumn::new("price", "Price"),
                        ScreenerColumn::new("change", "Chg %"),
                    ],
                ),
                ScreenerColumnSet::new("Volume", vec![ScreenerColumn::new("volume", "Vol")]),
            ]
        };
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let documents = Arc::new(Documents::default());
        let mut panel = ScreenerPanel::new(sets());
        panel.set_storage(documents.clone());
        panel.set_feed(feed.clone());
        // Answers the latest screen with one row of the asked ids, so the headings follow it.
        let answer = || {
            let (query, reply) = feed.rows.lock().unwrap().pop().unwrap();
            reply.send(Ok(vec![ScreenerRow::new("AAA", query.columns.clone())]));
            query
        };
        assert_eq!(answer().columns, ["price", "change"]);
        let mut h = harness(panel);
        h.run();

        heading_menu(&mut h, "Chg %");
        press(&mut h, "Move left");
        assert_eq!(answer().columns, ["change", "price"]);
        h.run();
        heading_menu(&mut h, "Price");
        assert!(h.get_by_label("Move right").accesskit_node().is_disabled());
        press(&mut h, "Remove column");
        assert_eq!(answer().columns, ["change"]);
        h.run();
        press(&mut h, "+");
        // Only the columns not shown are offered, a removed one among them.
        h.get_by_label("Price");
        press(&mut h, "Vol");
        assert_eq!(answer().columns, ["change", "volume"]);
        h.run();
        heading_menu(&mut h, "Vol");
        press(&mut h, "Sort ascending");
        let shaped = answer();
        assert_eq!(
            (shaped.sort_by.as_deref(), shaped.descending),
            (Some("volume"), false)
        );
        h.run();
        press(&mut h, "Chart");
        assert!(
            feed.rows.lock().unwrap().is_empty(),
            "a view switch asks no screen"
        );

        let screen = h.state().saved_screen();
        h.state_mut().saved.save("Shaped", screen).unwrap();
        h.run();
        let mut restored = ScreenerPanel::new(sets());
        restored.set_storage(documents.clone());
        for _ in 0..4 {
            restored.poll(&ctx);
        }
        restored.load_screen(&ctx, "Shaped");
        assert_eq!((restored.query(0), restored.view), (shaped, View::Chart));

        press(&mut h, "Table");
        heading_menu(&mut h, "Vol");
        press(&mut h, "Remove column");
        let query = answer();
        assert_eq!(
            (query.columns, query.sort_by),
            (vec!["change".to_owned()], None)
        );
        h.run();
        press(&mut h, "Overview");
        press(&mut h, "Overview");
        assert_eq!(answer().columns, ["price", "change"]);
    }

    /// Guards how the chart view loads its lines: each shown row asks for its symbol's last 60
    /// daily closes once per screen answer, not every frame and not again on a view switch; an
    /// answer reaches its own row in time order, a failure leaves the row without a line, and a
    /// new screen answer asks again.
    #[test]
    fn chart_view_asks_each_rows_closes_once_per_answer() {
        let feed = Arc::new(Feed::default());
        let mut panel = ScreenerPanel::new(sets());
        panel.set_feed(feed.clone());
        let answer = || {
            let (_, reply) = feed.rows.lock().unwrap().pop().unwrap();
            let row = |symbol: &str| ScreenerRow::new(symbol, vec!["1".into()]);
            reply.send(Ok(vec![row("AAA"), row("BBB")]));
        };
        let asked = || {
            let histories = feed.histories.lock().unwrap();
            histories
                .iter()
                .map(|(request, _)| request.clone())
                .collect::<Vec<_>>()
        };
        let reply = |index: usize, page: Result<HistoryPage, FeedError>| {
            let reply = feed.histories.lock().unwrap()[index].1.take().unwrap();
            reply.send(page);
        };
        answer();
        let mut h = harness(panel);
        h.run();
        assert!(asked().is_empty(), "a table asks no closes");
        press(&mut h, "Chart");
        let daily = |symbol: &str| HistoryRequest::new(symbol, Timeframe::Day1, 60);
        assert_eq!(asked(), [daily("AAA"), daily("BBB")]);
        reply(
            0,
            Ok(HistoryPage::new(vec![bar(2, 2.0), bar(1, 1.0)], false)),
        );
        reply(1, Err(FeedError::NotSupported));
        h.run();
        assert_eq!(h.state().sparks["AAA"].closes, [1.0, 2.0]);
        assert!(h.state().sparks["BBB"].closes.is_empty());
        press(&mut h, "Table");
        press(&mut h, "Chart");
        assert_eq!(asked().len(), 2, "asked again without a new answer");
        h.state_mut().refresh();
        answer();
        h.run();
        assert_eq!(asked().len(), 4);
    }

    /// Guards the small chart's line against what feeds send: pages in any order, two bars at one
    /// time (the later in the page wins, as on the chart), closes that are not finite, flat runs,
    /// the ends of f64, and more bars than a chart holds (the newest stay).
    #[test]
    fn small_charts_sort_drop_and_scale_closes() {
        type Row<'a> = (&'a [(i64, f64)], &'a [(f32, f32)]);
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 10.0));
        let (max, inf) = (f64::MAX, f64::INFINITY);
        let rows: [Row; 6] = [
            (
                &[(2, 0.5), (1, 1.0), (2, 5.0), (3, f64::NAN)],
                &[(0.0, 10.0), (100.0, 0.0)],
            ),
            (
                &[(1, 2.0), (2, 2.0), (3, 2.0)],
                &[(0.0, 5.0), (50.0, 5.0), (100.0, 5.0)],
            ),
            (&[(1, 7.0)], &[]),
            (&[], &[]),
            (
                &[(1, -max), (2, 0.0), (3, max)],
                &[(0.0, 10.0), (50.0, 5.0), (100.0, 0.0)],
            ),
            (&[(1, inf), (2, -inf)], &[]),
        ];
        for (page, points) in rows {
            let bars = page.iter().map(|&(day, close)| bar(day, close)).collect();
            let line: Vec<Pos2> = points.iter().map(|&(x, y)| pos2(x, y)).collect();
            assert_eq!(sparkline(&closes(bars), rect), line, "{page:?}");
        }
        let bars = (0..=60).map(|day| bar(day, day as f64)).collect();
        assert_eq!(closes(bars), (1..=60).map(f64::from).collect::<Vec<_>>());
    }

    /// Guards the timed refresh: with every 10 seconds chosen, the shown panel asks the current
    /// screen again once 10 seconds have passed since the last answer, never with a request open
    /// or while hidden, and for every page on screen so the loaded rows stay; shown again after
    /// the interval, it asks at once.
    #[test]
    fn auto_refresh_asks_again_while_shown_and_keeps_loaded_pages() {
        type Shown<'a> = Harness<'a, (ScreenerPanel, bool)>;
        // The first request is open, so the spinner repaints: step instead of settling.
        fn choose(h: &mut Shown<'_>, label: &str) {
            h.get_all_by_label(label).last().unwrap().click();
            h.run_steps(2);
        }
        fn at(h: &mut Shown<'_>, time: f64) {
            h.input_mut().time = Some(time);
            h.step();
        }
        let feed = Arc::new(Feed::default());
        let mut panel = ScreenerPanel::new(sets());
        panel.set_feed(feed.clone());
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, (panel, shown): &mut (ScreenerPanel, bool)| {
                    panel.poll(ui.ctx());
                    if *shown {
                        _ = panel.show(ui);
                    }
                },
                (panel, true),
            );
        let answers = std::cell::Cell::new(0);
        let answer = |rows: usize| {
            let (query, reply) = feed.rows.lock().unwrap().pop().unwrap();
            let row = |i| ScreenerRow::new(format!("S{i}"), vec!["1".into()]);
            reply.send(Ok((0..rows).map(row).collect()));
            answers.set(answers.get() + 1);
            query
        };
        let asked = || answers.get() + feed.rows.lock().unwrap().len();
        choose(&mut h, "No auto refresh");
        choose(&mut h, "Refresh every 10 seconds");
        let first = answer(LIMIT);
        at(&mut h, 10.0);
        assert_eq!(asked(), 1);
        h.state_mut().0.load_more();
        answer(3);
        at(&mut h, 12.0);
        assert_eq!((h.state().0.rows.len(), asked()), (LIMIT + 3, 2));
        at(&mut h, 21.9);
        assert_eq!(asked(), 2, "asked before the interval passed");
        at(&mut h, 22.0);
        assert_eq!(asked(), 3);
        let latest = feed.rows.lock().unwrap().last().unwrap().0.clone();
        assert_eq!((latest.offset, latest.limit), (0, 2 * LIMIT));
        assert_eq!(
            ScreenerQuery {
                limit: LIMIT,
                ..latest
            },
            first
        );
        assert_eq!(
            h.state().0.rows.len(),
            LIMIT + 3,
            "rows kept until the answer"
        );
        at(&mut h, 30.0);
        assert_eq!(asked(), 3, "asked with a request open");
        answer(LIMIT + 3);
        at(&mut h, 30.5);
        assert_eq!(
            (h.state().0.rows.len(), h.state().0.more),
            (LIMIT + 3, false)
        );
        h.state_mut().1 = false;
        at(&mut h, 40.6);
        at(&mut h, 100.0);
        assert_eq!(asked(), 3, "asked while hidden");
        h.state_mut().1 = true;
        at(&mut h, 100.1);
        assert_eq!(asked(), 4);
    }
}
