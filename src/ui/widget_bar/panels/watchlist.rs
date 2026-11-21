//! Host-fed watchlists: named lists of symbols with sections, colour flags, sorting and text
//! import and export. The host's quote service supplies the quotes, and the host persists the
//! lists.

use crate::localization as l10n;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{
    Align, Align2, Color32, EventFilter, Key, Label, Layout, Modifiers, RichText, Sense, Shape,
    Stroke, TextStyle, Ui, pos2, vec2,
};

use super::market_details::WatchlistDetails;
use super::symbol_details::detail_section;
use super::{HEADER_HEIGHT, grouped, price_decimals, signed, weighted};
use crate::data::inbox::Inbox;
use crate::data::{CalendarEvent, NewsStory};
use crate::icons::icons;
use crate::model::{Instrument, Quote, SymbolType};
use crate::painting::symbol_mark::paint_symbol_mark;
use crate::scales::StudyFormat;
use crate::services::{Listener, Quotes, ShareKind, Sharing};
use crate::theme::{Theme, ThemeExt};
use crate::ui::share_controls::ShareControls;
use crate::ui_kit::alert::Alert;
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::{EmptyState, IconButton};

/// Height of a watchlist row, which fits its symbol mark.
const LIST_ROW_HEIGHT: f32 = 30.0;
/// Height of a summary row below the wide table.
const SUMMARY_ROW_HEIGHT: f32 = 22.0;
/// Diameter of a symbol's mark.
const MARK_SIZE: f32 = 18.0;
/// The narrowest a dragged quote column gets, and the room the quote columns leave the symbol
/// column.
const MIN_COLUMN_WIDTH: f32 = 32.0;

use crate::model::{SkippedEntries, Watchlist, WatchlistColumn, WatchlistFlag, WatchlistItem};

/// The header label of `column`.
fn column_label(column: WatchlistColumn) -> &'static str {
    match column {
        WatchlistColumn::Symbol => "Symbol",
        WatchlistColumn::Last => "Last",
        WatchlistColumn::Change => "Chg",
        WatchlistColumn::ChangePercent => "Chg%",
        WatchlistColumn::Volume => "Vol",
        WatchlistColumn::ExtendedLast => "Ext",
        WatchlistColumn::ExtendedChange => "Ext chg",
        WatchlistColumn::ExtendedChangePercent => "Ext chg%",
    }
}

/// The figure of `quote` that `column` shows and sorts by, or `None` when the quote lacks it.
/// Extended-hours changes are measured from the regular session's last price.
fn column_value(column: WatchlistColumn, quote: &Quote) -> Option<f64> {
    let extended = |change: fn(f64, f64) -> f64| {
        quote
            .extended
            .map(|extended| change(extended.last, quote.last))
    };
    match column {
        WatchlistColumn::Symbol => None,
        WatchlistColumn::Last => Some(quote.last),
        WatchlistColumn::Change => Some(quote.change()),
        WatchlistColumn::ChangePercent => Some(quote.change_percent()),
        WatchlistColumn::Volume => quote.volume,
        WatchlistColumn::ExtendedLast => quote.extended.map(|extended| extended.last),
        WatchlistColumn::ExtendedChange => extended(|last, regular| last - regular),
        WatchlistColumn::ExtendedChangePercent => {
            extended(|last, regular| (last - regular) / regular.abs() * 100.0)
        }
    }
    .filter(|value| value.is_finite())
}

/// A section of the wide watchlist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WatchlistSection {
    /// The table of quotes.
    #[default]
    Overview,
    /// Earnings of the shown symbols.
    Earnings,
    /// Dividends of the shown symbols.
    Dividends,
    /// News about the shown symbols.
    News,
}

/// An action the host handles.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum WatchlistAction {
    /// No host action.
    #[default]
    None,
    /// Show this symbol on the chart. The panel already marks it selected.
    SelectSymbol(String),
    /// Let the user pick a symbol, such as with symbol search, and pass it to
    /// [`WatchlistPanel::add_symbol`].
    AddSymbol,
    /// Show the active list in the host's full-size view, drawn with
    /// [`WatchlistPanel::show_wide`].
    AdvancedView,
    /// Return from the wide view to the compact panel, drawn with [`WatchlistPanel::show`].
    CompactView,
    /// Read a text file the user chooses and pass it to [`WatchlistPanel::import`].
    Import,
    /// Save `text`, the active list in the [`Watchlist::from_text`] format, to a file the user
    /// chooses.
    Export {
        /// Display name.
        name: String,
        /// Exported watchlist text for the host to save or copy.
        text: String,
    },
}

/// How the wide watchlist groups its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WatchlistGrouping {
    /// Under the list's own sections, as the user arranged them.
    #[default]
    Sections,
    /// A flat table without section headings.
    None,
    /// By exchange.
    Exchange,
    /// By instrument type.
    InstrumentType,
    /// By sector.
    Sector,
    /// By price currency.
    Currency,
}

const GROUPINGS: [(WatchlistGrouping, &str); 6] = [
    (WatchlistGrouping::None, "No group"),
    (WatchlistGrouping::Sections, "Sections"),
    (WatchlistGrouping::Exchange, "Exchange"),
    (WatchlistGrouping::InstrumentType, "Type"),
    (WatchlistGrouping::Sector, "Sector"),
    (WatchlistGrouping::Currency, "Currency"),
];

/// The details of a symbol the wide view groups by, from its instrument.
#[derive(Clone, Debug, Default)]
struct InstrumentGroups {
    exchange: Option<String>,
    kind: Option<SymbolType>,
    sector: Option<String>,
    currency: Option<String>,
}

/// A numeric column the host added to the wide view.
#[derive(Clone, Debug)]
struct HostColumn {
    name: String,
    decimals: u8,
    values: HashMap<String, f64>,
}

/// A column of the table: a quote figure or a host column by name.
#[derive(Clone, Debug, PartialEq)]
enum TableColumn {
    Quote(WatchlistColumn),
    Host(String),
}

impl From<WatchlistColumn> for TableColumn {
    fn from(column: WatchlistColumn) -> Self {
        Self::Quote(column)
    }
}

impl TableColumn {
    /// Whether this view shows the column and can apply its sort.
    fn is_shown(&self, wide: bool, compact_columns: &[WatchlistColumn]) -> bool {
        wide || matches!(self, Self::Quote(column) if *column == WatchlistColumn::Symbol || compact_columns.contains(column))
    }

    fn label(&self) -> &str {
        match self {
            Self::Quote(column) => column_label(*column),
            Self::Host(name) => name,
        }
    }
}

/// A list item or a generated group heading, with missing details under Other.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Item(usize),
    Group(Option<String>),
}

/// Reads the instrument detail used as a group heading, absent without that detail.
type GroupKey<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The source list and row index, so separate panels cannot exchange row indices.
struct RowDrag(egui::Id, usize);

/// An edit of the list at `list` that undo reverts while the list still holds `after`, so undo
/// never discards a later change. Section moves carry current collapse states back, while
/// clearing restores the removed sections' states.
struct ListEdit {
    list: usize,
    before: Vec<WatchlistItem>,
    after: Vec<WatchlistItem>,
    sections: SectionUndo,
}

/// Only clearing removes section states; moves undo their swap of the current states.
enum SectionUndo {
    Clear(Vec<bool>),
    Move { at: usize, down: bool },
}

/// Watchlists fed by host quotes.
///
/// With a [`Quotes`] service attached through [`set_service`](Self::set_service), the panel
/// subscribes to the [`symbols`](Self::symbols) of the active list and the selected symbol, and
/// subscribes again whenever they change; call [`poll`](Self::poll) each frame, even while the
/// panel is hidden. Without one, set quotes with [`set_quote`](Self::set_quote). Either way
/// [`quote`](Self::quote) reads them back, so the selected symbol's quote can feed the symbol
/// details panel, the symbol header and the order ticket. Keep the chart symbol selected with
/// [`set_selected`](Self::set_selected).
/// The list name in the header opens the list menu; beside it, the add button returns
/// [`WatchlistAction::AddSymbol`], the advanced view button [`WatchlistAction::AdvancedView`],
/// and the more button opens the actions on the active list. Clicking a symbol, or moving
/// through the list with the up and down arrow keys after a click, returns
/// [`WatchlistAction::SelectSymbol`]. From the chart, answer
/// [`KeyboardAction::NextWatchlistSymbol`](crate::config::KeyboardAction::NextWatchlistSymbol) and
/// `PreviousWatchlistSymbol`, bound to Down or Space and Up or Shift+Space, with
/// [`select_next_symbol`](Self::select_next_symbol) and
/// [`select_previous_symbol`](Self::select_previous_symbol), which wrap at the ends. Column headers
/// cycle through ascending, descending and list order; sorting keeps symbols inside their section.
/// Dragging the left edge of a quote column header resizes the column and a double click restores
/// its width. Clicking a section collapses or expands its symbols independently; its state
/// follows renames, moves and sorting. Unsorted rows are dragged to
/// reorder, and rows are right-clicked to flag, rename or remove, and sections to move them with
/// their symbols. The list menu stars lists, which it then shows first, clears the active list and
/// undoes a clear or a section move; the more menu shows the descriptions given with
/// [`set_description`](Self::set_description) and hides the symbol marks. The host persists
/// [`lists`](Self::lists), with the stars, descriptions and logos settings and column widths of
/// each, and handles file intents.
/// The wide view groups rows by section, exchange, instrument type, sector or currency from
/// [`set_instrument`](Self::set_instrument), shows optional summary rows, and shows the host's
/// numeric columns added with [`add_column`](Self::add_column).
/// Each symbol has the same flag in every list; removing it from a flag view clears its flag.
pub struct WatchlistPanel {
    lists: Vec<Watchlist>,
    active: usize,
    quotes: HashMap<String, Quote>,
    /// Decimals of the price columns per symbol, where the host knows them.
    decimals: HashMap<String, u8>,
    /// Marks of the symbols, where the host supplies them.
    marks: HashMap<String, egui::ImageSource<'static>>,
    /// Descriptions of the symbols, where the host supplies them.
    descriptions: HashMap<String, String>,
    /// The columns after the symbol.
    columns: Vec<WatchlistColumn>,
    sort: Option<(TableColumn, bool)>,
    section: WatchlistSection,
    grouping: WatchlistGrouping,
    summary: bool,
    groups: HashMap<String, InstrumentGroups>,
    host_columns: Vec<HostColumn>,
    /// Whether the last draw was the wide view.
    wide: bool,
    /// Shows the symbols flagged this way in any list instead of the active list.
    flag_filter: Option<WatchlistFlag>,
    selected: Option<String>,
    service: Option<Arc<dyn Quotes>>,
    inbox: Arc<Inbox>,
    share: ShareControls,
    listener: Option<Listener>,
    /// Key of the current stream's pushes in the inbox.
    generation: u64,
    /// The symbols of the current stream.
    subscribed: Vec<String>,
    /// Clears and section moves, newest last.
    undo: Vec<ListEdit>,
    /// What the last import left out and the list it made, shown above that list until
    /// dismissed.
    import_note: Option<(usize, SkippedEntries)>,
    /// Scrolls the selected row into view on the next draw.
    reveal: bool,
    /// The list as it was when Save a copy was chosen, which the host's answer adds.
    copy_source: Option<Watchlist>,
    /// Per list, whether each section, by its position among the sections, is collapsed.
    /// A missing entry is expanded; section moves and removals carry the states along.
    collapsed_sections: Vec<Vec<bool>>,
}

impl std::fmt::Debug for WatchlistPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchlistPanel")
            .field("lists", &self.lists)
            .field("active", &self.active)
            .field("quotes", &self.quotes)
            .field("decimals", &self.decimals)
            .field("marks", &self.marks)
            .field("descriptions", &self.descriptions)
            .field("columns", &self.columns)
            .field("sort", &self.sort)
            .field("section", &self.section)
            .field("grouping", &self.grouping)
            .field("summary", &self.summary)
            .field(
                "host_columns",
                &self
                    .host_columns
                    .iter()
                    .map(|c| &c.name)
                    .collect::<Vec<_>>(),
            )
            .field("flag_filter", &self.flag_filter)
            .field("selected", &self.selected)
            .field("generation", &self.generation)
            .field("subscribed", &self.subscribed)
            .finish_non_exhaustive()
    }
}

impl Default for WatchlistPanel {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl WatchlistPanel {
    /// A panel showing the first of `lists`, or one empty list when there are none.
    /// Conflicting saved flags take the first flagged entry in list order, then row order.
    pub fn new(mut lists: Vec<Watchlist>) -> Self {
        if lists.is_empty() {
            lists.push(Watchlist::new("Watchlist"));
        }
        share_flags(&mut lists);
        Self {
            lists,
            active: 0,
            quotes: HashMap::new(),
            decimals: HashMap::new(),
            marks: HashMap::new(),
            descriptions: HashMap::new(),
            columns: WatchlistColumn::DEFAULT.to_vec(),
            sort: None,
            section: WatchlistSection::default(),
            grouping: WatchlistGrouping::default(),
            summary: false,
            groups: HashMap::new(),
            host_columns: Vec::new(),
            wide: false,
            flag_filter: None,
            selected: None,
            service: None,
            inbox: Arc::default(),
            share: ShareControls::default(),
            listener: None,
            generation: 0,
            subscribed: Vec::new(),
            undo: Vec::new(),
            import_note: None,
            reveal: false,
            copy_source: None,
            collapsed_sections: Vec::new(),
        }
    }

    /// Shows these columns after the symbol, in order; the default is
    /// [`WatchlistColumn::DEFAULT`]. A quote without a column's figure shows a dash, apart from a
    /// symbol without a quote, which shows nothing.
    pub fn set_columns(&mut self, columns: impl IntoIterator<Item = WatchlistColumn>) {
        self.columns = columns
            .into_iter()
            .filter(|column| *column != WatchlistColumn::Symbol)
            .collect();
    }

    /// The flag whose symbols the panel shows across every list, if any.
    pub fn flag_filter(&self) -> Option<WatchlistFlag> {
        self.flag_filter
    }

    /// Shows the symbols flagged `flag` in any list, each once, instead of the active list; `None`
    /// returns to the active list. The view follows flag changes in the lists.
    pub fn set_flag_filter(&mut self, flag: Option<WatchlistFlag>) {
        self.flag_filter = flag;
    }

    /// Every list, for the host to persist.
    pub fn lists(&self) -> &[Watchlist] {
        &self.lists
    }

    /// The symbols the panel shows, which need quotes: those of the active list, or with a
    /// [flag filter](Self::set_flag_filter) the flagged ones of every list, each once.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        let symbols: Vec<&str> = match self.flag_filter {
            Some(flag) => flagged_symbols(&self.lists, flag)
                .into_iter()
                .filter_map(|(list, item)| match &self.lists[list].items[item] {
                    WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
                    WatchlistItem::Section(_) => None,
                })
                .collect(),
            None => self.lists[self.active].symbols().collect(),
        };
        symbols.into_iter()
    }

    /// Sets the latest quote of `symbol`, ignoring older timestamps and non-finite prices or
    /// supplied numeric fields. Equal timestamps can correct the current quote.
    pub fn set_quote(&mut self, symbol: &str, quote: Quote) {
        if !quote.last.is_finite()
            || !quote.prev_close.is_finite()
            || [quote.volume, quote.bid, quote.ask]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
            || quote
                .extended
                .is_some_and(|extended| !extended.last.is_finite())
            || self
                .quotes
                .get(symbol)
                .is_some_and(|previous| quote.time < previous.time)
        {
            return;
        }
        self.quotes.insert(symbol.to_owned(), quote);
    }

    /// The latest quote of `symbol`, if the panel holds one.
    pub fn quote(&self, symbol: &str) -> Option<&Quote> {
        self.quotes.get(symbol)
    }

    /// Shows the prices of `symbol` with `decimals`, as `Instrument::price_decimals`. Without
    /// them, the decimals follow the size of the last price.
    pub fn set_price_decimals(&mut self, symbol: &str, decimals: u8) {
        self.decimals.insert(symbol.to_owned(), decimals);
    }

    /// Shows `mark`, such as the issuer's logo, in a circle before `symbol` in the lists that show
    /// logos, as new lists do. Without one, or while it loads or cannot be read, the row shows the
    /// symbol's first letter or digit on a neutral disc.
    pub fn set_mark(&mut self, symbol: &str, mark: egui::ImageSource<'static>) {
        self.marks.insert(symbol.to_owned(), mark);
    }

    /// Shows `description`, such as `Instrument::description`, under `symbol` in the lists that
    /// show descriptions.
    pub fn set_description(&mut self, symbol: &str, description: impl Into<String>) {
        self.descriptions
            .insert(symbol.to_owned(), description.into());
    }

    /// Groups `instrument.symbol` in the wide view by its exchange, type, sector or currency.
    /// A symbol without the detail grouped by, or without an instrument, is listed under Other.
    pub fn set_instrument(&mut self, instrument: &Instrument) {
        self.groups.insert(
            instrument.symbol.clone(),
            InstrumentGroups {
                exchange: instrument.exchange.clone(),
                kind: instrument.symbol_type,
                sector: instrument.sector.clone(),
                currency: instrument.currency.clone(),
            },
        );
    }

    /// How the wide view groups the rows.
    pub fn grouping(&self) -> WatchlistGrouping {
        self.grouping
    }

    /// Groups the wide view as its Group by menu does. The compact view keeps the list's
    /// sections. Sorting stays inside each group; dragging to reorder is off outside Sections.
    pub fn set_grouping(&mut self, grouping: WatchlistGrouping) {
        self.grouping = grouping;
    }

    /// Whether the wide view shows the summary rows.
    pub fn summary(&self) -> bool {
        self.summary
    }

    /// Shows average, median, minimum and maximum rows below the wide table, over every shown
    /// symbol, including those under collapsed headings. Missing values are left out.
    pub fn set_summary(&mut self, summary: bool) {
        self.summary = summary;
    }

    /// Adds a numeric column to the wide view after the quote columns. It sorts and resizes
    /// like a quote column; each list keeps its width in [`Watchlist::host_column_widths`].
    /// Values use `decimals` decimal places and thousands separators, or a dash when missing.
    /// Returns false, changing nothing, for a blank name or one already added.
    pub fn add_column(&mut self, name: impl Into<String>, decimals: u8) -> bool {
        let name = name.into();
        if name.trim().is_empty() || self.host_columns.iter().any(|c| c.name == name) {
            return false;
        }
        self.host_columns.push(HostColumn {
            name,
            decimals,
            values: HashMap::new(),
        });
        true
    }

    /// Removes the named column, its values and a sort by it. Lists keep its width for when it
    /// is added again. Returns false for a column not added.
    pub fn remove_column(&mut self, name: &str) -> bool {
        let Some(at) = self.host_columns.iter().position(|c| c.name == name) else {
            return false;
        };
        self.host_columns.remove(at);
        if self
            .sort
            .as_ref()
            .is_some_and(|(c, _)| *c == TableColumn::Host(name.into()))
        {
            self.sort = None;
        }
        true
    }

    /// Sets `symbol`'s value in the named column. `None` or a non-finite value clears it.
    /// Returns false for a column not added.
    pub fn set_column_value(&mut self, name: &str, symbol: &str, value: Option<f64>) -> bool {
        let Some(column) = self.host_columns.iter_mut().find(|c| c.name == name) else {
            return false;
        };
        match value.filter(|v| v.is_finite()) {
            Some(value) => {
                column.values.insert(symbol.into(), value);
            }
            None => {
                column.values.remove(symbol);
            }
        }
        true
    }

    /// Takes quotes from `service` from the next [`poll`](Self::poll) on.
    pub fn set_service(&mut self, service: Arc<dyn Quotes>) {
        self.service = Some(service);
        self.listener = None;
    }

    /// Offers copy-link and save-a-copy on the active list, as `sharing` reports them for
    /// [`ShareKind::Watchlist`]. A saved copy holds the list as it was when Save a copy was
    /// chosen, whatever its name or place when the host answers, and is added under the name
    /// the host answers; like a duplicate, it is not starred. [`poll`](Self::poll) receives the
    /// answers.
    pub fn set_sharing(&mut self, sharing: Arc<dyn Sharing>) {
        self.share.set_service(sharing);
        self.copy_source = None;
    }

    /// Subscribes again when the symbols to quote changed or the stream lagged, and applies the
    /// quotes the service pushed. The quotes of symbols no longer subscribed are dropped.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(name) = self.share.poll(ctx)
            && let Some(mut copy) = self.copy_source.take()
        {
            (copy.name, copy.favorite) = (name, false);
            adopt_flags(&mut copy.items, &self.lists);
            self.lists.push(copy);
        }
        let Some(service) = &self.service else {
            return;
        };
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let mut symbols: Vec<String> = self.symbols().map(str::to_owned).collect();
        symbols.extend(self.selected.clone());
        symbols.sort();
        symbols.dedup();
        let lagged = self.listener.as_ref().is_none_or(|l| !l.is_active());
        if lagged || symbols != self.subscribed {
            self.generation += 1;
            let (listener, events) = Listener::new(&self.inbox, self.generation);
            // Replacing the listener stops the previous stream.
            self.listener = Some(listener);
            self.quotes.retain(|symbol, _| symbols.contains(symbol));
            self.subscribed = symbols;
            service.subscribe(&self.subscribed, events);
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation
                && let Ok(event) = answer.downcast::<(String, Quote)>()
            {
                let (symbol, quote) = *event;
                self.set_quote(&symbol, quote);
            }
        }
    }

    /// Marks `symbol` selected, such as the symbol the chart shows.
    pub fn set_selected(&mut self, symbol: &str) {
        self.selected = Some(symbol.to_owned());
    }

    /// Selects the symbol after the selected one in the order the panel shows, wrapping from the
    /// last to the first, and returns [`WatchlistAction::SelectSymbol`] for the host to chart, or
    /// [`WatchlistAction::None`] when there is no other symbol to select. Without a shown
    /// selection it selects the first symbol. Answer
    /// [`KeyboardAction::NextWatchlistSymbol`](crate::config::KeyboardAction::NextWatchlistSymbol)
    /// with it.
    ///
    /// The order is the shown one: the active list, or the flagged symbols under a
    /// [flag filter](Self::set_flag_filter), sorted as the table is and grouped as the last view
    /// drawn. Sections are skipped; symbols under collapsed headings are included.
    ///
    /// ```
    /// use egui_charts::model::Watchlist;
    /// use egui_charts::ui::widget_bar::{WatchlistAction, WatchlistPanel};
    ///
    /// let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "AAPL,MSFT")]);
    /// panel.set_selected("MSFT");
    /// assert_eq!(panel.select_next_symbol(), WatchlistAction::SelectSymbol("AAPL".into()));
    /// ```
    pub fn select_next_symbol(&mut self) -> WatchlistAction {
        self.step(true)
    }

    /// Selects the symbol before the selected one, wrapping from the first to the last, as
    /// [`select_next_symbol`](Self::select_next_symbol) does forward; without a shown selection
    /// it selects the last symbol. Answer
    /// [`KeyboardAction::PreviousWatchlistSymbol`](crate::config::KeyboardAction::PreviousWatchlistSymbol)
    /// with it.
    pub fn select_previous_symbol(&mut self) -> WatchlistAction {
        self.step(false)
    }

    /// Selects the next or previous shown symbol, wrapping at the ends.
    fn step(&mut self, forward: bool) -> WatchlistAction {
        let items = match self.flag_filter {
            Some(flag) => flag_view(&self.lists, flag),
            None => self.lists[self.active].items.clone(),
        };
        let group = |symbol: &str| group_key(&self.groups, self.grouping, symbol);
        let symbols: Vec<&str> = display_rows(
            &items,
            self.sort
                .as_ref()
                .filter(|(column, _)| column.is_shown(self.wide, &self.columns))
                .map(|(c, d)| (c, *d)),
            |c, s| table_value(c, s, &self.quotes, &self.host_columns),
            (self.wide && self.grouping != WatchlistGrouping::Sections)
                .then_some(&group as GroupKey<'_>),
            |_| false,
        )
        .into_iter()
        .filter_map(|row| match row {
            Row::Item(i) => match &items[i] {
                WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
                WatchlistItem::Section(_) => None,
            },
            Row::Group(_) => None,
        })
        .collect();
        let count = symbols.len();
        let at = symbols
            .iter()
            .position(|symbol| Some(*symbol) == self.selected.as_deref());
        let next = match (at, forward) {
            _ if count == 0 => return WatchlistAction::None,
            (Some(at), true) => (at + 1) % count,
            (Some(at), false) => (at + count - 1) % count,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        if at == Some(next) {
            return WatchlistAction::None;
        }
        let symbol = symbols[next].to_owned();
        (self.selected, self.reveal) = (Some(symbol.clone()), true);
        WatchlistAction::SelectSymbol(symbol)
    }

    /// Appends `symbol` to the active list as [`Watchlist::add_symbol`] does, such as the symbol
    /// picked after [`WatchlistAction::AddSymbol`]. Returns false when the list is unchanged.
    pub fn add_symbol(&mut self, symbol: &str) -> bool {
        self.add_symbol_to(self.active, symbol)
    }

    /// Appends `symbol` to the list at `list` in [`lists`](Self::lists) as [`Watchlist::add_symbol`]
    /// does, such as for [`ContextMenuAction::AddToWatchlist`](crate::ui::context_menu::ContextMenuAction::AddToWatchlist).
    /// Returns false when there is no such list or it is unchanged.
    pub fn add_symbol_to(&mut self, list: usize, symbol: &str) -> bool {
        let added = self
            .lists
            .get_mut(list)
            .is_some_and(|list| list.add_symbol(symbol));
        if added {
            share_flags(&mut self.lists);
        }
        added
    }

    /// Adds a list read with [`Watchlist::read_text`] and shows it, turning any flag filter off.
    /// When the text held entries the list leaves out, the panel says how many above the list
    /// until the user dismisses it or imports again.
    pub fn import(&mut self, name: impl Into<String>, text: &str) {
        let (list, skipped) = Watchlist::read_text(name, text);
        self.lists.push(list);
        share_flags(&mut self.lists);
        (self.active, self.flag_filter) = (self.lists.len() - 1, None);
        self.import_note = (skipped != SkippedEntries::default()).then_some((self.active, skipped));
    }

    /// Render the active list and return the host action.
    pub fn show(&mut self, ui: &mut Ui) -> WatchlistAction {
        self.show_with(ui, None)
    }

    /// Render the active list in the wide view: the table with every [`WatchlistColumn::WIDE`]
    /// column, and tabs for the earnings, dividends and news of `details`, which the host
    /// fills for the [`symbols`](Self::symbols) shown. The sections list the rows of the
    /// selected symbol, or of every shown symbol while none of them is selected. Selection,
    /// sorting, the active list and the flag filter are those of [`show`](Self::show), so
    /// switching views keeps them; while sorted by a column the compact view hides, that view
    /// shows the list in its own order. Returns [`WatchlistAction::CompactView`] from its view button.
    pub fn show_wide(&mut self, ui: &mut Ui, details: &WatchlistDetails) -> WatchlistAction {
        self.show_with(ui, Some(details))
    }

    fn show_with(&mut self, ui: &mut Ui, wide: Option<&WatchlistDetails>) -> WatchlistAction {
        self.wide = wide.is_some();
        let mut action = WatchlistAction::None;
        let shown_symbols: Vec<String> = self.symbols().map(str::to_owned).collect();
        let Self {
            lists,
            share,
            active,
            quotes,
            decimals,
            marks,
            descriptions,
            columns: compact_columns,
            sort,
            section,
            grouping,
            summary,
            groups,
            host_columns,
            flag_filter,
            selected,
            undo,
            import_note,
            reveal: reveal_next,
            copy_source,
            collapsed_sections,
            ..
        } = self;
        let theme = ui.active_theme();
        let (mut menu, mut more) = (None, None);
        let header = vec2(ui.available_width(), HEADER_HEIGHT);
        ui.allocate_ui_with_layout(header, Layout::right_to_left(Align::Center), |ui| {
            ui.set_min_height(header.y);
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            more = Some(
                IconButton::new(&icons::UI_MORE_HORIZONTAL)
                    .tooltip("More")
                    .show(ui),
            );
            if IconButton::new(&icons::TABLE)
                .tooltip(if wide.is_some() {
                    "Compact view"
                } else {
                    "Advanced view"
                })
                .show(ui)
                .clicked()
            {
                action = if wide.is_some() {
                    WatchlistAction::CompactView
                } else {
                    WatchlistAction::AdvancedView
                };
            }
            if IconButton::new(&icons::PLUS)
                .tooltip("Add symbol")
                .show(ui)
                .clicked()
            {
                action = WatchlistAction::AddSymbol;
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let chevron = icons::CHEVRON_DOWN.as_image_square(16.0);
                let name = match flag_filter {
                    Some(flag) => l10n::format_text(
                        ui.ctx(),
                        "{0} flagged",
                        &[("0", l10n::text(ui.ctx(), format!("{flag:?}")))],
                    ),
                    None => l10n::text(ui.ctx(), lists[*active].name.clone()),
                };
                let title = LayoutJob::single_section(
                    name,
                    weighted(theme.text.title, 700.0, theme.ui.text),
                );
                menu = Some(
                    ui.add(
                        egui::Button::new((title, chevron))
                            .frame_when_inactive(false)
                            .image_tint_follows_text_color(true)
                            .truncate(),
                    )
                    .on_hover_text(l10n::text(ui.ctx(), "Lists")),
                );
            });
        });
        let (Some(menu), Some(more)) = (menu, more) else {
            return action;
        };
        egui::Popup::menu(&menu).show(|ui| {
            let mut menu_order: Vec<usize> = (0..lists.len()).collect();
            menu_order.sort_by_key(|&index| !lists[index].favorite);
            for index in menu_order {
                ui.horizontal(|ui| {
                    let list = &mut lists[index];
                    if IconButton::new(if list.favorite {
                        &icons::STAR_FILLED
                    } else {
                        &icons::STAR_EMPTY
                    })
                    .size(ButtonSize::SM)
                    .tooltip(if list.favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    })
                    .show(ui)
                    .clicked()
                    {
                        list.favorite = !list.favorite;
                    }
                    if ui
                        .selectable_label(
                            flag_filter.is_none() && index == *active,
                            l10n::text(ui.ctx(), &list.name),
                        )
                        .clicked()
                    {
                        (*active, *flag_filter) = (index, None);
                    }
                });
            }
            ui.separator();
            ui.label(l10n::text(ui.ctx(), "Flagged symbols"));
            ui.horizontal(|ui| {
                for choice in WatchlistFlag::ALL {
                    let (swatch, response) =
                        ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                    let translated_label = l10n::text(ui.ctx(), format!("{choice:?}"));
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::RadioButton,
                            response.enabled(),
                            *flag_filter == Some(choice),
                            &translated_label,
                        )
                    });
                    ui.painter().circle_filled(
                        swatch.center(),
                        if *flag_filter == Some(choice) {
                            8.0
                        } else {
                            6.0
                        },
                        choice.color(),
                    );
                    if response.on_hover_text(&translated_label).clicked() {
                        *flag_filter = (*flag_filter != Some(choice)).then_some(choice);
                        ui.close();
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(l10n::text(ui.ctx(), "Name"));
                ui.text_edit_singleline(&mut lists[*active].name)
                    .accessible_name(l10n::text(ui.ctx(), "Name"));
            });
            if ui.button(l10n::text(ui.ctx(), "New list")).clicked() {
                let name = (2..)
                    .map(|n| format!("Watchlist {n}"))
                    .find(|name| lists.iter().all(|list| list.name != *name))
                    .expect("an unused name exists");
                lists.push(Watchlist::new(name));
                (*active, *flag_filter) = (lists.len() - 1, None);
            }
            if ui.button(l10n::text(ui.ctx(), "Duplicate list")).clicked() {
                let copy = lists[*active].duplicate(lists.iter().map(|list| list.name.as_str()));
                lists.push(copy);
                (*active, *flag_filter) = (lists.len() - 1, None);
            }
            if ui
                .add_enabled(
                    flag_filter.is_none() && !lists[*active].items.is_empty(),
                    egui::Button::new(l10n::text(ui.ctx(), "Clear list")),
                )
                .clicked()
            {
                record(
                    undo,
                    lists,
                    SectionUndo::Clear(
                        collapsed_sections.get(*active).cloned().unwrap_or_default(),
                    ),
                    *active,
                    |list| {
                        list.items.clear();
                        true
                    },
                );
            }
            let undoable = undo.last().is_some_and(|edit| {
                lists
                    .get(edit.list)
                    .is_some_and(|list| list.items == edit.after)
            });
            if ui
                .add_enabled(undoable, egui::Button::new(l10n::text(ui.ctx(), "Undo")))
                .clicked()
                && let Some(mut edit) = undo.pop()
            {
                adopt_flags(&mut edit.before, lists);
                lists[edit.list].items = edit.before;
                let states = section_states(collapsed_sections, edit.list);
                match edit.sections {
                    SectionUndo::Clear(collapsed) => *states = collapsed,
                    SectionUndo::Move { at, down } => {
                        states.swap(at, if down { at + 1 } else { at - 1 });
                    }
                }
                (*active, *flag_filter) = (edit.list, None);
            }
            if ui
                .add_enabled(
                    lists.len() > 1,
                    egui::Button::new(l10n::text(ui.ctx(), "Delete list")),
                )
                .clicked()
            {
                if *active < collapsed_sections.len() {
                    collapsed_sections.remove(*active);
                }
                lists.remove(*active);
                *active = active.saturating_sub(1);
                // The recorded edits and the import note name lists by position.
                undo.clear();
                *import_note = None;
            }
        });
        egui::Popup::menu(&more).show(|ui| {
            if ui.button(l10n::text(ui.ctx(), "Add section")).clicked() {
                lists[*active]
                    .items
                    .push(WatchlistItem::Section("New section".to_owned()));
            }
            ui.checkbox(
                &mut lists[*active].show_descriptions,
                l10n::text(ui.ctx(), "Symbol descriptions"),
            );
            ui.checkbox(
                &mut lists[*active].show_logos,
                l10n::text(ui.ctx(), "Symbol logos"),
            );
            if ui.button(l10n::text(ui.ctx(), "Import list…")).clicked() {
                action = WatchlistAction::Import;
            }
            if ui.button(l10n::text(ui.ctx(), "Export list…")).clicked() {
                action = WatchlistAction::Export {
                    name: lists[*active].name.clone(),
                    text: lists[*active].to_text(),
                };
            }
            if share.show(ui, ShareKind::Watchlist, &lists[*active].name) {
                *copy_source = Some(lists[*active].clone());
            }
        });
        if let Some((list, skipped)) = *import_note
            && list == *active
            && flag_filter.is_none()
        {
            let mut lines = Vec::new();
            if skipped.past_limit > 0 {
                lines.push(l10n::format_text(
                    ui.ctx(),
                    "Entries past the limit of {0} not imported: {1}",
                    &[
                        ("0", grouped(Watchlist::MAX_IMPORTED as f64, 0)),
                        ("1", grouped(skipped.past_limit as f64, 0)),
                    ],
                ));
            }
            if skipped.invalid > 0 {
                lines.push(l10n::format_text(
                    ui.ctx(),
                    "Entries with control characters not imported: {0}",
                    &[("0", grouped(skipped.invalid as f64, 0))],
                ));
            }
            if Alert::warning(lines.join("\n"))
                .dismissible(true)
                .show(ui)
                .dismissed
            {
                *import_note = None;
            }
        }
        if let Some(details) = wide {
            ui.horizontal(|ui| {
                for (choice, label) in [
                    (WatchlistSection::Overview, "Overview"),
                    (WatchlistSection::Earnings, "Earnings"),
                    (WatchlistSection::Dividends, "Dividends"),
                    (WatchlistSection::News, "News"),
                ] {
                    ui.selectable_value(section, choice, l10n::text(ui.ctx(), label));
                }
                if *section == WatchlistSection::Overview {
                    ui.separator();
                    ui.label(l10n::text(ui.ctx(), "Group by"));
                    egui::ComboBox::from_id_salt("watchlist_grouping")
                        .selected_text(l10n::text(
                            ui.ctx(),
                            GROUPINGS.iter().find(|(g, _)| g == grouping).unwrap().1,
                        ))
                        .show_ui(ui, |ui| {
                            for (choice, label) in GROUPINGS {
                                ui.selectable_value(grouping, choice, l10n::text(ui.ctx(), label));
                            }
                        })
                        .response
                        .accessible_name(l10n::text(ui.ctx(), "Group by"));
                    ui.checkbox(summary, l10n::text(ui.ctx(), "Summary"));
                }
            });
            if *section != WatchlistSection::Overview {
                // The selected symbol's rows, or those of every shown symbol without one.
                let focus: Vec<&str> = match selected.as_deref() {
                    Some(symbol) if shown_symbols.iter().any(|shown| shown == symbol) => {
                        vec![symbol]
                    }
                    _ => shown_symbols.iter().map(String::as_str).collect(),
                };
                if let Some(symbol) = detail_rows(ui, *section, details, &focus) {
                    *selected = Some(symbol.clone());
                    action = WatchlistAction::SelectSymbol(symbol);
                }
                return action;
            }
        }
        let list_id = match flag_filter {
            Some(flag) => ui.scope_id().with(("watchlist_flagged", *flag)),
            None => ui.scope_id().with(("watchlist_rows", *active)),
        };
        let font = TextStyle::Body.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        // The number columns share 70.5% of the row, 23.5% each by default, which leaves the
        // symbol column room for a six-letter name after its mark; the last column's own gap
        // completes the right margin. A column the user resized keeps its width.
        let mut shown_columns: Vec<TableColumn> = if wide.is_some() {
            WatchlistColumn::WIDE
                .iter()
                .copied()
                .map(Into::into)
                .collect()
        } else {
            compact_columns.iter().copied().map(Into::into).collect()
        };
        if wide.is_some() {
            shown_columns.extend(
                host_columns
                    .iter()
                    .map(|c| TableColumn::Host(c.name.clone())),
            );
        }
        // Keep hidden columns' sorts for the wide view; compact rows keep list order.
        let shown_sort = sort
            .as_ref()
            .filter(|(column, _)| column.is_shown(wide.is_some(), compact_columns))
            .cloned();
        let numbers = shown_columns.len();
        let columns =
            |rect: egui::Rect, widths: &(BTreeMap<WatchlistColumn, f32>, BTreeMap<String, f32>)| {
                let (left, right) = (rect.left() + 10.0, rect.right() - 4.0);
                let number =
                    ((rect.width() - 18.0).max(0.0) * 0.705 / numbers.max(1) as f32).min(96.0);
                // Left edges of the number columns, laid out from the right. Widths kept from a wider
                // row leave the symbol column the room a drag leaves it.
                let mut edges: Vec<f32> = shown_columns
                    .iter()
                    .rev()
                    .scan(right, |edge, column| {
                        let width = column_width(widths, column);
                        *edge -= width
                            .filter(|w| w.is_finite() && *w > 0.0)
                            .unwrap_or(number);
                        Some(edge.max(left + MIN_COLUMN_WIDTH))
                    })
                    .collect();
                edges.push(left);
                edges.reverse();
                edges.push(right);
                edges
                    .windows(2)
                    .map(|edge| {
                        egui::Rect::from_min_max(
                            pos2(edge[0], rect.top()),
                            pos2((edge[1] - 4.0).max(edge[0]), rect.bottom()),
                        )
                    })
                    .collect::<Vec<_>>()
            };
        let all_columns: Vec<TableColumn> = std::iter::once(WatchlistColumn::Symbol.into())
            .chain(shown_columns.iter().cloned())
            .collect();

        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let widths = (
            lists[*active].column_widths.clone(),
            lists[*active].host_column_widths.clone(),
        );
        let cells = columns(header, &widths);
        for (index, column) in all_columns.iter().enumerate() {
            let label = column.label();
            let align = if index == 0 {
                Align2::LEFT_CENTER
            } else {
                Align2::RIGHT_CENTER
            };
            let text = ui.painter().text(
                egui::pos2(
                    if index == 0 {
                        cells[index].left()
                    } else {
                        cells[index].right()
                    },
                    header.center().y,
                ),
                align,
                l10n::text(ui.ctx(), label),
                font.clone(),
                theme.ui.text_muted,
            );
            if let Some((sorted, descending)) = shown_sort.as_ref()
                && sorted == column
            {
                // A painted triangle beside the label: the fonts lack arrow glyphs.
                let x = if index == 0 {
                    text.right() + 7.0
                } else {
                    text.left() - 7.0
                };
                let (tip, base) = if *descending {
                    (3.0, -2.0)
                } else {
                    (-3.0, 2.0)
                };
                let y = text.center().y;
                ui.painter().add(Shape::convex_polygon(
                    vec![
                        pos2(x - 3.5, y + base),
                        pos2(x + 3.5, y + base),
                        pos2(x, y + tip),
                    ],
                    theme.ui.text_muted,
                    Stroke::NONE,
                ));
            }
            let response = ui
                .interact(
                    text.expand(3.0),
                    list_id.with(("column", index)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let translated_label = l10n::text(ui.ctx(), label);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::ColumnHeader,
                    response.enabled(),
                    &translated_label,
                )
            });
            if let Some((sorted, descending)) = shown_sort.as_ref()
                && sorted == column
            {
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_sort_direction(if *descending {
                        egui::accesskit::SortDirection::Descending
                    } else {
                        egui::accesskit::SortDirection::Ascending
                    });
                });
            }
            if response.clicked() {
                *sort = match sort.as_ref() {
                    Some((c, false)) if c == column => Some((column.clone(), true)),
                    Some((c, true)) if c == column => None,
                    _ => Some((column.clone(), false)),
                };
            }
        }
        ui.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0, theme.ui.border),
        );
        // After the labels, so a handle wins over the label beside it.
        for (at, column) in all_columns.iter().enumerate().skip(1) {
            let edge = cells[at].left();
            let response = ui
                .interact(
                    egui::Rect::from_x_y_ranges(edge - 4.0..=edge + 2.0, header.y_range()),
                    list_id.with(("resize", at)),
                    Sense::click_and_drag(),
                )
                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeHorizontal);
            let name = l10n::format_text(
                ui.ctx(),
                "Resize {0}",
                &[("0", l10n::text(ui.ctx(), column.label()))],
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Splitter, response.enabled(), &name)
            });
            if response.hovered() || response.dragged() {
                ui.painter().vline(
                    edge - 1.0,
                    header.y_range(),
                    Stroke::new(1.0, theme.ui.border),
                );
            }
            if response.double_clicked() {
                set_column_width(&mut lists[*active], column, None);
            } else if response.dragged() && response.drag_delta().x != 0.0 {
                // Dragging the left edge to the left widens the column, down to the room the
                // symbol column keeps.
                let width = cells[at].width() + 4.0;
                let widest = width + cells[1].left() - cells[0].left() - MIN_COLUMN_WIDTH;
                set_column_width(
                    &mut lists[*active],
                    column,
                    Some(
                        (width - response.drag_delta().x)
                            .min(widest)
                            .max(MIN_COLUMN_WIDTH),
                    ),
                );
            }
        }

        // A flag filter draws copies of the flagged symbols, whose flag changes and removals are
        // applied to their lists after drawing.
        let mut flagged = flag_filter.map(|flag| {
            let mut view = Watchlist::new("");
            view.items = flag_view(lists, flag);
            view
        });
        // The flag view shows these settings of the active list.
        let widths = (
            lists[*active].column_widths.clone(),
            lists[*active].host_column_widths.clone(),
        );
        let show_descriptions = lists[*active].show_descriptions;
        let show_logos = lists[*active].show_logos;
        let filtered = flagged.is_some();
        let list = match &mut flagged {
            Some(view) => view,
            None => &mut lists[*active],
        };
        let grouped = wide.is_some() && *grouping != WatchlistGrouping::Sections;
        let reorderable = shown_sort.is_none() && !filtered && !grouped;

        let sections: Vec<usize> = (0..list.items.len())
            .filter(|&i| matches!(list.items[i], WatchlistItem::Section(_)))
            .collect();
        // Positions among sections keep equal titles apart and survive renames and sorts.
        // The flag view has no sections and leaves the active list's states alone.
        let states = section_states(collapsed_sections, *active);
        if !filtered {
            states.resize(sections.len(), false);
        }
        let collapsed: HashSet<usize> = sections
            .iter()
            .zip(states.iter())
            .filter_map(|(&index, &hidden)| hidden.then_some(index))
            .collect();
        let group_id = |label: &Option<String>| list_id.with(("collapsed_group", *grouping, label));
        let group = |symbol: &str| group_key(groups, *grouping, symbol);
        let value =
            |column: &TableColumn, symbol: &str| table_value(column, symbol, quotes, host_columns);
        let order = display_rows(
            &list.items,
            shown_sort.as_ref().map(|(c, d)| (c, *d)),
            value,
            grouped.then_some(&group as GroupKey<'_>),
            |row| match row {
                Row::Item(i) => collapsed.contains(i),
                Row::Group(label) => {
                    ui.data(|data| data.get_temp(group_id(label)).unwrap_or(false))
                }
            },
        );
        // Arrow keys move the selection instead of the keyboard focus.
        let arrows = EventFilter {
            vertical_arrows: true,
            ..Default::default()
        };
        let mut reveal = std::mem::take(reveal_next);
        let focused = ui
            .memory(|m| m.focused())
            .filter(|id| (0..list.items.len()).any(|i| list_id.with(i) == *id));
        if let Some(focused) = focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(focused, arrows));
            let step = ui.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::ArrowDown) as isize
                    - i.consume_key(Modifiers::NONE, Key::ArrowUp) as isize
            });
            let symbols: Vec<&str> = order
                .iter()
                .filter_map(|row| match row {
                    Row::Item(i) => match &list.items[*i] {
                        WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
                        WatchlistItem::Section(_) => None,
                    },
                    Row::Group(_) => None,
                })
                .collect();
            if step != 0 && !symbols.is_empty() {
                let next = match symbols.iter().position(|s| Some(*s) == selected.as_deref()) {
                    Some(at) => at.saturating_add_signed(step).min(symbols.len() - 1),
                    None => 0,
                };
                if selected.as_deref() != Some(symbols[next]) {
                    *selected = Some(symbols[next].to_owned());
                    action = WatchlistAction::SelectSymbol(symbols[next].to_owned());
                    reveal = true;
                }
            }
        }

        if list.items.is_empty() {
            let empty = if filtered {
                EmptyState::new("No flagged symbols")
                    .description("Flag symbols from their right-click menu.")
            } else {
                EmptyState::new("This list is empty")
                    .description("Add symbols with the add button above.")
            };
            empty.icon(&icons::WATCHLIST).fill_height().show(ui);
            return action;
        }

        let mut moved = None;
        let mut removed = None;
        let mut flag_edit = None;
        let mut section_move = None;
        let show_summary = wide.is_some() && *summary;
        let summaries: Vec<_> = shown_columns
            .iter()
            .filter(|_| show_summary)
            .map(|column| {
                let mut seen = HashSet::new();
                let symbols: Vec<_> = list
                    .symbols()
                    .filter(|s| seen.insert(*s) && value(column, s).is_some())
                    .collect();
                let figures = summarise(symbols.iter().filter_map(|s| value(column, s)).collect());
                let precision = match column {
                    TableColumn::Host(name) => host_columns
                        .iter()
                        .find(|c| c.name == *name)
                        .map(|c| c.decimals)
                        .unwrap_or(2),
                    TableColumn::Quote(_) => symbols
                        .iter()
                        .filter_map(|s| {
                            decimals
                                .get(*s)
                                .copied()
                                .or_else(|| quotes.get(*s).map(|q| price_decimals(q.last)))
                        })
                        .max()
                        .unwrap_or(2),
                };
                (figures, precision)
            })
            .collect();
        egui::ScrollArea::vertical()
            .max_height(if show_summary {
                (ui.available_height() - 4.0 * SUMMARY_ROW_HEIGHT - 1.0).max(0.0)
            } else {
                ui.available_height()
            })
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for row in &order {
                    if let Row::Group(label) = row {
                        // No heading is needed for the flat table.
                        if *grouping == WatchlistGrouping::None {
                            continue;
                        }
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), LIST_ROW_HEIGHT),
                            Sense::hover(),
                        );
                        let id = group_id(label);
                        let collapsed = ui.data(|d| d.get_temp(id).unwrap_or(false));
                        let response = ui.interact(rect, id, Sense::click());
                        let title = label
                            .clone()
                            .unwrap_or_else(|| l10n::text(ui.ctx(), "Other"));
                        heading(ui, rect, &response, &title, collapsed, &theme);
                        if response.clicked() {
                            ui.data_mut(|d| d.insert_temp(id, !collapsed));
                        }
                        continue;
                    }
                    let Row::Item(index) = *row else {
                        continue;
                    };
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), LIST_ROW_HEIGHT),
                        Sense::hover(),
                    );
                    let sense = if reorderable {
                        Sense::click_and_drag()
                    } else {
                        Sense::click()
                    };
                    let response = ui.interact(rect, list_id.with(index), sense);
                    if response.dragged() {
                        response.dnd_set_drag_payload(RowDrag(list_id, index));
                    }
                    if let Some(from) = response.dnd_hover_payload::<RowDrag>()
                        && from.0 == list_id
                        && reorderable
                    {
                        let y = if from.1 < index {
                            rect.bottom()
                        } else {
                            rect.top()
                        };
                        ui.painter()
                            .hline(rect.x_range(), y, Stroke::new(2.0, theme.ui.accent));
                    }
                    if let Some(from) = response.dnd_release_payload::<RowDrag>()
                        && from.0 == list_id
                        && reorderable
                        && from.1 < list.items.len()
                    {
                        moved = Some((from.1, index));
                    }
                    let cells = columns(rect, &widths);
                    let painter = ui.painter().clone();
                    match &mut list.items[index] {
                        WatchlistItem::Section(title) => {
                            let is_collapsed = collapsed.contains(&index);
                            heading(ui, rect, &response, title, is_collapsed, &theme);
                            if response.clicked()
                                && let Some(at) =
                                    sections.iter().position(|&section| section == index)
                            {
                                states[at] = !states[at];
                            }
                            response.context_menu(|ui| {
                                ui.text_edit_singleline(title)
                                    .accessible_name(l10n::text(ui.ctx(), "Section name"));
                                for (down, label, last) in [
                                    (false, "Move section up", sections.first()),
                                    (true, "Move section down", sections.last()),
                                ] {
                                    if ui
                                        .add_enabled(
                                            last != Some(&index),
                                            egui::Button::new(l10n::text(ui.ctx(), label)),
                                        )
                                        .clicked()
                                    {
                                        section_move = Some((index, down));
                                        ui.close();
                                    }
                                }
                                if ui.button(l10n::text(ui.ctx(), "Remove section")).clicked() {
                                    removed = Some(index);
                                    ui.close();
                                }
                            });
                        }
                        WatchlistItem::Symbol { symbol, flag } => {
                            let is_selected = selected.as_deref() == Some(symbol.as_str());
                            let translated_label = symbol.as_str();
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::ListBoxOption,
                                    response.enabled(),
                                    translated_label,
                                )
                            });
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_selected(is_selected)
                            });
                            if is_selected && reveal {
                                response.scroll_to_me(None);
                            }
                            if is_selected {
                                painter.rect_filled(rect, 0.0, theme.ui.btn_bg_active);
                            } else if response.hovered() {
                                painter.rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                            }
                            if let Some(flag) = flag {
                                let bar =
                                    egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
                                painter.rect_filled(bar, 0.0, flag.color());
                            }
                            let text = if show_logos {
                                let mark = egui::Rect::from_min_size(
                                    pos2(cells[0].left(), rect.center().y - MARK_SIZE / 2.0),
                                    vec2(MARK_SIZE, MARK_SIZE),
                                );
                                paint_symbol_mark(
                                    &painter,
                                    mark,
                                    symbol,
                                    marks.get(symbol.as_str()).cloned(),
                                );
                                cells[0].with_min_x(mark.right() + 6.0)
                            } else {
                                cells[0]
                            };
                            let description = show_descriptions
                                .then(|| descriptions.get(symbol.as_str()))
                                .flatten();
                            // With a description the symbol sits above the centre line.
                            let (name, align) = match description {
                                Some(_) => (text.with_max_y(text.center().y + 2.0), Align::Max),
                                None => (text, Align::Center),
                            };
                            ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(name)
                                    .layout(Layout::left_to_right(align)),
                            )
                            .add(
                                Label::new(
                                    RichText::new(symbol.as_str())
                                        .font(font.clone())
                                        .color(theme.ui.text),
                                )
                                .truncate()
                                .selectable(false),
                            );
                            if let Some(description) = description {
                                ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(text.with_min_y(text.center().y + 2.0))
                                        .layout(Layout::left_to_right(Align::Min)),
                                )
                                .add(
                                    Label::new(
                                        RichText::new(l10n::text(ui.ctx(), description))
                                            .font(small.clone())
                                            .color(theme.ui.text_muted),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            }
                            for (at, column) in all_columns.iter().enumerate().skip(1) {
                                let precision = match column {
                                    TableColumn::Quote(_) => {
                                        let Some(quote) = quotes.get(symbol.as_str()) else {
                                            continue;
                                        };
                                        decimals
                                            .get(symbol.as_str())
                                            .copied()
                                            .unwrap_or_else(|| price_decimals(quote.last))
                                    }
                                    TableColumn::Host(name) => host_columns
                                        .iter()
                                        .find(|c| c.name == *name)
                                        .map(|c| c.decimals)
                                        .unwrap_or(2),
                                };
                                let (text, color) =
                                    cell_text(column, value(column, symbol), precision, &theme);
                                ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(cells[at])
                                        .layout(Layout::right_to_left(Align::Center)),
                                )
                                .add(
                                    Label::new(
                                        RichText::new(l10n::text(ui.ctx(), text))
                                            .font(font.clone())
                                            .color(color),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            }
                            if response.clicked() {
                                *selected = Some(symbol.clone());
                                action = WatchlistAction::SelectSymbol(symbol.clone());
                                ui.memory_mut(|m| {
                                    m.request_focus(response.id);
                                    m.set_focus_lock_filter(response.id, arrows);
                                });
                            }
                            response.context_menu(|ui| {
                                ui.horizontal(|ui| {
                                    for choice in WatchlistFlag::ALL {
                                        let (swatch, response) = ui
                                            .allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                                        let translated_label =
                                            l10n::text(ui.ctx(), format!("{choice:?}"));
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::Role::RadioButton,
                                                response.enabled(),
                                                *flag == Some(choice),
                                                &translated_label,
                                            )
                                        });
                                        ui.painter().circle_filled(
                                            swatch.center(),
                                            if *flag == Some(choice) { 8.0 } else { 6.0 },
                                            choice.color(),
                                        );
                                        if response.on_hover_text(&translated_label).clicked() {
                                            flag_edit = Some((symbol.clone(), Some(choice)));
                                            ui.close();
                                        }
                                    }
                                });
                                if ui
                                    .add_enabled(
                                        flag.is_some(),
                                        egui::Button::new(l10n::text(ui.ctx(), "Remove flag")),
                                    )
                                    .clicked()
                                {
                                    flag_edit = Some((symbol.clone(), None));
                                    ui.close();
                                }
                                if ui
                                    .button(l10n::text(ui.ctx(), "Remove from list"))
                                    .clicked()
                                {
                                    if filtered {
                                        flag_edit = Some((symbol.clone(), None));
                                    } else {
                                        removed = Some(index);
                                    }
                                    ui.close();
                                }
                            });
                        }
                    }
                }
            });
        if show_summary {
            let y = ui.cursor().top();
            ui.painter().hline(
                ui.max_rect().x_range(),
                y,
                Stroke::new(1.0, theme.ui.border),
            );
            ui.spacing_mut().item_spacing.y = 0.0;
            for (row, label) in ["Average", "Median", "Minimum", "Maximum"]
                .iter()
                .enumerate()
            {
                let (rect, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), SUMMARY_ROW_HEIGHT),
                    Sense::hover(),
                );
                let cells = columns(rect, &widths);
                ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(cells[0])
                        .layout(Layout::left_to_right(Align::Center)),
                )
                .add(
                    Label::new(
                        RichText::new(l10n::text(ui.ctx(), *label))
                            .font(font.clone())
                            .color(theme.ui.text_muted),
                    )
                    .truncate()
                    .selectable(false),
                );
                for (at, (column, (figures, precision))) in
                    shown_columns.iter().zip(&summaries).enumerate()
                {
                    let (text, color) =
                        cell_text(column, figures.map(|v| v[row]), *precision, &theme);
                    ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(cells[at + 1])
                            .layout(Layout::right_to_left(Align::Center)),
                    )
                    .add(
                        Label::new(
                            RichText::new(l10n::text(ui.ctx(), text))
                                .font(font.clone())
                                .color(color),
                        )
                        .truncate()
                        .selectable(false),
                    );
                }
            }
        }
        if let Some((from, to)) = moved {
            let item = list.items.remove(from);
            list.items.insert(to, item);
            // A dragged heading takes its state to its new place among the sections.
            if let Some(at) = sections.iter().position(|&section| section == from) {
                let state = states.remove(at);
                let to_at = list.items[..to]
                    .iter()
                    .filter(|item| matches!(item, WatchlistItem::Section(_)))
                    .count();
                states.insert(to_at, state);
            }
        }
        if let Some(index) = removed
            && !filtered
        {
            if let Some(at) = sections.iter().position(|&section| section == index) {
                states.remove(at);
            }
            list.items.remove(index);
        }
        if let Some((symbol, flag)) = flag_edit {
            set_flag(lists, &symbol, flag);
        }
        if let Some((index, down)) = section_move
            && let Some(at) = sections.iter().position(|&section| section == index)
            && record(
                undo,
                lists,
                SectionUndo::Move { at, down },
                *active,
                |list| list.move_section(index, down),
            )
        {
            states.swap(at, if down { at + 1 } else { at - 1 });
        }
        action
    }
}

/// The section states of the list at `list` in `all`, growing `all` to reach it.
fn section_states(all: &mut Vec<Vec<bool>>, list: usize) -> &mut Vec<bool> {
    if all.len() <= list {
        all.resize_with(list + 1, Vec::new);
    }
    &mut all[list]
}

/// Applies `edit` to the list at `list` as one step that undo reverts, if `edit` reports a
/// change. The oldest of 100 steps is dropped. Returns whether it did.
fn record(
    undo: &mut Vec<ListEdit>,
    lists: &mut [Watchlist],
    sections: SectionUndo,
    list: usize,
    edit: impl FnOnce(&mut Watchlist) -> bool,
) -> bool {
    let before = lists[list].items.clone();
    if edit(&mut lists[list]) {
        if undo.len() == 100 {
            undo.remove(0);
        }
        undo.push(ListEdit {
            list,
            before,
            after: lists[list].items.clone(),
            sections,
        });
        true
    } else {
        false
    }
}

/// Rows of the wide view's section, for the symbols in `focus`; returns the symbol the user
/// clicked.
fn detail_rows(
    ui: &mut Ui,
    section: WatchlistSection,
    details: &WatchlistDetails,
    focus: &[&str],
) -> Option<String> {
    let in_focus = |symbol: &str| focus.contains(&symbol);
    let mut picked = None;
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let events = |ui: &mut Ui, id: &str, events: &[CalendarEvent]| -> Option<String> {
                let mut clicked = None;
                let mut rows: Vec<&CalendarEvent> = events
                    .iter()
                    .filter(|event| event.symbol.as_deref().is_some_and(in_focus))
                    .collect();
                rows.sort_by_key(|event| event.time);
                egui::Grid::new(id).striped(true).show(ui, |ui| {
                    for heading in ["Date", "Symbol", "Name", "Actual", "Forecast", "Prior"] {
                        ui.label(RichText::new(l10n::text(ui.ctx(), heading)).strong());
                    }
                    ui.end_row();
                    for event in rows {
                        ui.label(event.time.format("%Y-%m-%d").to_string());
                        let symbol = event.symbol.clone().unwrap_or_default();
                        if ui.link(&symbol).clicked() {
                            clicked = Some(symbol);
                        }
                        ui.label(l10n::text(ui.ctx(), &event.title));
                        for figure in [&event.actual, &event.forecast, &event.prior] {
                            ui.label(figure.as_deref().unwrap_or("-"));
                        }
                        ui.end_row();
                    }
                });
                clicked
            };
            let has_event = |events: &[CalendarEvent]| {
                events
                    .iter()
                    .any(|event| event.symbol.as_deref().is_some_and(in_focus))
            };
            match section {
                WatchlistSection::Overview => {}
                WatchlistSection::Earnings => detail_section(
                    ui,
                    "Earnings",
                    &details.earnings,
                    |list| !has_event(list),
                    |ui, list| picked = events(ui, "watchlist_earnings", list),
                ),
                WatchlistSection::Dividends => detail_section(
                    ui,
                    "Dividends",
                    &details.dividends,
                    |list| !has_event(list),
                    |ui, list| picked = events(ui, "watchlist_dividends", list),
                ),
                WatchlistSection::News => {
                    let about =
                        |story: &NewsStory| story.symbols.iter().any(|symbol| in_focus(symbol));
                    detail_section(
                        ui,
                        "News",
                        &details.news,
                        |stories| !stories.iter().any(about),
                        |ui, stories| {
                            let mut rows: Vec<&NewsStory> =
                                stories.iter().filter(|story| about(story)).collect();
                            rows.sort_by_key(|story| std::cmp::Reverse(story.time));
                            egui::Grid::new("watchlist_news")
                                .striped(true)
                                .show(ui, |ui| {
                                    for heading in ["Time", "Source", "Headline", "Symbols"] {
                                        ui.label(
                                            RichText::new(l10n::text(ui.ctx(), heading)).strong(),
                                        );
                                    }
                                    ui.end_row();
                                    for story in rows {
                                        ui.label(story.time.format("%Y-%m-%d %H:%M").to_string());
                                        ui.label(l10n::text(ui.ctx(), &story.source));
                                        ui.label(l10n::text(ui.ctx(), &story.headline));
                                        ui.horizontal(|ui| {
                                            for symbol in
                                                story.symbols.iter().filter(|s| in_focus(s))
                                            {
                                                if ui.link(symbol).clicked() {
                                                    picked = Some(symbol.clone());
                                                }
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                        },
                    );
                }
            }
        });
    picked
}

/// Reconcile loaded lists: the first flagged entry of a symbol supplies every entry's flag.
fn share_flags(lists: &mut [Watchlist]) {
    let mut flags = HashMap::new();
    for item in lists.iter().flat_map(|list| &list.items) {
        if let WatchlistItem::Symbol {
            symbol,
            flag: Some(flag),
        } = item
        {
            flags.entry(symbol.clone()).or_insert(*flag);
        }
    }
    for item in lists.iter_mut().flat_map(|list| &mut list.items) {
        if let WatchlistItem::Symbol { symbol, flag } = item {
            *flag = flags.get(symbol.as_str()).copied();
        }
    }
}

/// Change a symbol's flag everywhere it occurs.
fn set_flag(lists: &mut [Watchlist], symbol: &str, flag: Option<WatchlistFlag>) {
    for item in lists.iter_mut().flat_map(|list| &mut list.items) {
        if let WatchlistItem::Symbol {
            symbol: at,
            flag: entry,
        } = item
            && at == symbol
        {
            *entry = flag;
        }
    }
}

/// Restored snapshots take current flags, including a removed flag, for symbols still listed.
fn adopt_flags(items: &mut [WatchlistItem], lists: &[Watchlist]) {
    let current: HashMap<&str, Option<WatchlistFlag>> = lists
        .iter()
        .flat_map(|list| &list.items)
        .filter_map(|item| match item {
            WatchlistItem::Symbol { symbol, flag } => Some((symbol.as_str(), *flag)),
            WatchlistItem::Section(_) => None,
        })
        .collect();
    for item in items {
        if let WatchlistItem::Symbol { symbol, flag } = item
            && let Some(shared) = current.get(symbol.as_str())
        {
            *flag = *shared;
        }
    }
}

/// Copies of the items the `flag` view shows: each symbol flagged `flag`, once.
fn flag_view(lists: &[Watchlist], flag: WatchlistFlag) -> Vec<WatchlistItem> {
    flagged_symbols(lists, flag)
        .into_iter()
        .map(|(list, item)| lists[list].items[item].clone())
        .collect()
}

/// Where each symbol flagged `flag` first appears: its list and item index, in list order.
fn flagged_symbols(lists: &[Watchlist], flag: WatchlistFlag) -> Vec<(usize, usize)> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for (list, watchlist) in lists.iter().enumerate() {
        for (item, entry) in watchlist.items.iter().enumerate() {
            if let WatchlistItem::Symbol {
                symbol,
                flag: Some(flagged),
            } = entry
                && *flagged == flag
                && seen.insert(symbol.as_str())
            {
                found.push((list, item));
            }
        }
    }
    found
}

/// The value of a quote or host column, absent for a missing field.
fn table_value(
    column: &TableColumn,
    symbol: &str,
    quotes: &HashMap<String, Quote>,
    host: &[HostColumn],
) -> Option<f64> {
    match column {
        TableColumn::Quote(column) => column_value(*column, quotes.get(symbol)?),
        TableColumn::Host(name) => host
            .iter()
            .find(|c| c.name == *name)?
            .values
            .get(symbol)
            .copied(),
    }
}

/// The instrument detail chosen for grouping. Currency case distinguishes units such as GBP and GBp.
fn group_key(
    groups: &HashMap<String, InstrumentGroups>,
    grouping: WatchlistGrouping,
    symbol: &str,
) -> Option<String> {
    let details = groups.get(symbol)?;
    match grouping {
        WatchlistGrouping::Sections | WatchlistGrouping::None => None,
        WatchlistGrouping::Exchange => details.exchange.clone(),
        WatchlistGrouping::InstrumentType => details.kind.map(|k| k.to_string()),
        WatchlistGrouping::Sector => details.sector.clone(),
        WatchlistGrouping::Currency => details.currency.clone(),
    }
}

fn column_width(
    widths: &(BTreeMap<WatchlistColumn, f32>, BTreeMap<String, f32>),
    column: &TableColumn,
) -> Option<f32> {
    match column {
        TableColumn::Quote(column) => widths.0.get(column).copied(),
        TableColumn::Host(name) => widths.1.get(name).copied(),
    }
}

fn set_column_width(list: &mut Watchlist, column: &TableColumn, width: Option<f32>) {
    match (column, width) {
        (TableColumn::Quote(column), Some(width)) => {
            list.column_widths.insert(*column, width);
        }
        (TableColumn::Host(name), Some(width)) => {
            list.host_column_widths.insert(name.clone(), width);
        }
        (TableColumn::Quote(column), None) => {
            list.column_widths.remove(column);
        }
        (TableColumn::Host(name), None) => {
            list.host_column_widths.remove(name);
        }
    }
}

/// The text and colour shared by numeric cells and their summaries.
fn cell_text(
    column: &TableColumn,
    value: Option<f64>,
    decimals: u8,
    theme: &Theme,
) -> (String, Color32) {
    let Some(value) = value.filter(|v| v.is_finite()) else {
        return ("-".into(), theme.ui.text);
    };
    let text = match column {
        TableColumn::Quote(WatchlistColumn::Volume) => StudyFormat::Volume.format(value, None),
        TableColumn::Quote(
            WatchlistColumn::ChangePercent | WatchlistColumn::ExtendedChangePercent,
        ) => format!("{}%", signed(value, 2)),
        TableColumn::Quote(WatchlistColumn::Change | WatchlistColumn::ExtendedChange) => {
            signed(value, usize::from(decimals))
        }
        _ => grouped(value, usize::from(decimals)),
    };
    let color = match column {
        TableColumn::Quote(
            WatchlistColumn::Change
            | WatchlistColumn::ChangePercent
            | WatchlistColumn::ExtendedChange
            | WatchlistColumn::ExtendedChangePercent,
        ) => {
            if value > 0.0 {
                theme.chart.candle_up
            } else if value < 0.0 {
                theme.chart.candle_down
            } else {
                theme.ui.text
            }
        }
        _ => theme.ui.text,
    };
    (text, color)
}

/// Paints a collapsible section or generated group heading with the same accessible state.
fn heading(
    ui: &mut Ui,
    rect: egui::Rect,
    response: &egui::Response,
    title: &str,
    collapsed: bool,
    theme: &Theme,
) {
    let label = l10n::text(ui.ctx(), title);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), &label));
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_expanded(!collapsed));
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
    }
    let chevron = if collapsed {
        &icons::CHEVRON_RIGHT
    } else {
        &icons::CHEVRON_DOWN
    };
    let icon =
        egui::Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(14.0, 14.0));
    chevron
        .as_image_tinted(icon.size(), theme.ui.text_secondary)
        .paint_at(ui, icon);
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(
                rect.with_min_x(icon.right() + 6.0)
                    .with_max_x(rect.right() - 10.0),
            )
            .layout(Layout::left_to_right(Align::Center)),
    )
    .add(
        Label::new(
            RichText::new(l10n::text(ui.ctx(), title.to_uppercase()))
                .font(TextStyle::Small.resolve(ui.style()))
                .color(theme.ui.text_secondary),
        )
        .truncate()
        .selectable(false),
    );
}

/// Average, median, minimum and maximum of finite values, absent without values.
fn summarise(mut values: Vec<f64>) -> Option<[f64; 4]> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    // Carry rounding errors so opposite large values do not erase smaller figures. Divide
    // after adding to retain tiny values, scaling the terms first only when the sum overflows.
    let sum = |divisor: f64| {
        let (mut total, mut error) = (0.0_f64, 0.0);
        for value in &values {
            let value = value / divisor;
            let next = total + value;
            if !next.is_finite() {
                return next;
            }
            error += if total.abs() >= value.abs() {
                (total - next) + value
            } else {
                (value - next) + total
            };
            total = next;
        }
        total + error
    };
    let total = sum(1.0);
    let mean = if total.is_finite() {
        total / n as f64
    } else {
        sum(n as f64)
    }
    .clamp(values[0], values[n - 1]);
    let median = if n % 2 == 1 {
        values[n / 2]
    } else {
        values[n / 2 - 1].midpoint(values[n / 2])
    };
    Some([mean, median, values[0], values[n - 1]].map(|v| v + 0.0))
}

/// Rows in display order: sections in place, or named groups ignoring case with Other last.
/// Each run keeps list order, or sorts stably with missing values last in either direction.
/// A collapsed heading stays while its symbols are omitted.
fn display_rows(
    items: &[WatchlistItem],
    sort: Option<(&TableColumn, bool)>,
    value: impl Fn(&TableColumn, &str) -> Option<f64>,
    group: Option<GroupKey<'_>>,
    collapsed: impl Fn(&Row) -> bool,
) -> Vec<Row> {
    let symbol = |i: usize| match &items[i] {
        WatchlistItem::Symbol { symbol, .. } => symbol.as_str(),
        WatchlistItem::Section(_) => "",
    };
    let flush = |run: &mut Vec<usize>, rows: &mut Vec<Row>| {
        if let Some((column, descending)) = sort {
            run.sort_by(|&a, &b| {
                let ordering = if *column == TableColumn::Quote(WatchlistColumn::Symbol) {
                    symbol(a).cmp(symbol(b))
                } else {
                    match (value(column, symbol(a)), value(column, symbol(b))) {
                        (Some(a), Some(b)) => a.total_cmp(&b),
                        (Some(_), None) => return Ordering::Less,
                        (None, Some(_)) => return Ordering::Greater,
                        (None, None) => Ordering::Equal,
                    }
                };
                if descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
        }
        rows.extend(run.drain(..).map(Row::Item));
    };
    let mut rows = Vec::new();
    if let Some(group) = group {
        let mut groups: BTreeMap<(bool, String, String), Vec<usize>> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            if !matches!(item, WatchlistItem::Symbol { .. }) {
                continue;
            }
            let key = group(symbol(i))
                .map(|key| key.trim().to_owned())
                .filter(|key| !key.is_empty());
            let label = key.as_deref().unwrap_or("");
            groups
                .entry((key.is_none(), label.to_lowercase(), label.to_owned()))
                .or_default()
                .push(i);
        }
        for ((other, _, label), mut run) in groups {
            let heading = Row::Group((!other).then_some(label));
            let hidden = collapsed(&heading);
            rows.push(heading);
            if !hidden {
                flush(&mut run, &mut rows);
            }
        }
    } else {
        let mut run = Vec::new();
        let mut hidden = false;
        for (i, item) in items.iter().enumerate() {
            if matches!(item, WatchlistItem::Section(_)) {
                flush(&mut run, &mut rows);
                let heading = Row::Item(i);
                hidden = collapsed(&heading);
                rows.push(heading);
            } else if !hidden {
                run.push(i);
            }
        }
        flush(&mut run, &mut rows);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(symbol: &str) -> WatchlistItem {
        WatchlistItem::Symbol {
            symbol: symbol.into(),
            flag: None,
        }
    }

    #[test]
    fn text_keeps_sections_and_skips_blank_and_repeated_entries() {
        let list =
            Watchlist::from_text("Mine", " AAPL ,\n###Tech, MSFT\r\n,AAPL,,###Crypto,BTCUSD");
        assert_eq!(
            list.items,
            [
                symbol("AAPL"),
                WatchlistItem::Section("Tech".into()),
                symbol("MSFT"),
                WatchlistItem::Section("Crypto".into()),
                symbol("BTCUSD"),
            ]
        );
        assert_eq!(list.to_text(), "AAPL,###Tech,MSFT,###Crypto,BTCUSD");
        let mut list = list;
        for invalid in [
            "",
            " ",
            "AAPL",
            "A,B",
            "A\nB",
            "A\rB",
            "A;B",
            "A\tB",
            "A\u{7f}B",
            "###Section",
        ] {
            assert!(!list.add_symbol(invalid), "{invalid:?}");
        }
        assert!(list.add_symbol(" EURUSD "));
        assert_eq!(Watchlist::from_text("Mine", &list.to_text()), list);
        let mut titled = Watchlist::new("Titles");
        titled
            .items
            .push(WatchlistItem::Section("Rates, FX\nand more".into()));
        assert_eq!(
            Watchlist::from_text("Again", &titled.to_text()).items,
            [WatchlistItem::Section("Rates  FX and more".into())]
        );
        titled.items = vec![WatchlistItem::Section("A;B\tC\u{1}D".into())];
        assert_eq!(
            Watchlist::from_text("Again", &titled.to_text()).items,
            [WatchlistItem::Section("A B C D".into())]
        );
        // A byte order mark, as an editor writes, semicolons, tabs, and control characters, as
        // a binary file chosen by mistake gives.
        assert_eq!(
            Watchlist::from_text("Mine", "\u{feff}AAPL;MSFT\t\u{0}JUNK,\u{1b}[0m\n AAPL ").items,
            [symbol("AAPL"), symbol("MSFT")]
        );
    }

    /// An import keeps the first items up to the limit and leaves out entries with control
    /// characters, and the panel says above the imported list what it left out until dismissed.
    #[test]
    fn imports_stop_at_the_limit_and_say_what_they_left_out() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut text: String = (0..1_003).map(|i| format!("S{i},")).collect();
        text.push_str("S0,###Late,\u{7}X");
        let (list, skipped) = Watchlist::read_text("Big", &text);
        assert_eq!(list.items.len(), Watchlist::MAX_IMPORTED);
        assert_eq!(
            (list.items.first(), list.items.last()),
            (Some(&symbol("S0")), Some(&symbol("S999")))
        );
        // S1000 to S1002 and the late section; the repeated S0 is in the list already.
        assert_eq!((skipped.invalid, skipped.past_limit), (1, 4));
        assert_eq!(Watchlist::from_text("Big", &text), list);
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                WatchlistPanel::default(),
            );
        h.state_mut().import("Big", &text);
        h.run();
        for line in [
            "past the limit of 1,000 not imported: 4",
            "with control characters not imported: 1",
        ] {
            assert!(h.query_by_label_contains(line).is_some(), "{line}");
        }
        h.get_by_label("Dismiss").click();
        h.run();
        assert!(h.query_by_label_contains("not imported").is_none());
        h.state_mut().import("Clean", "A");
        h.run();
        assert!(h.query_by_label_contains("not imported").is_none());
    }

    /// A section moves with its symbols past the neighbouring section, and the symbols before the
    /// first section stay first.
    #[test]
    fn sections_move_with_their_symbols() {
        let text = "A,###One,B,C,###Two,D,###Three,E";
        for (index, down, expected) in [
            (1, true, Some("A,###Two,D,###One,B,C,###Three,E")),
            (4, false, Some("A,###Two,D,###One,B,C,###Three,E")),
            (6, false, Some("A,###One,B,C,###Three,E,###Two,D")),
            (4, true, Some("A,###One,B,C,###Three,E,###Two,D")),
            (1, false, None),
            (6, true, None),
            (2, true, None),
            (8, false, None),
        ] {
            let mut list = Watchlist::from_text("List", text);
            assert_eq!(
                list.move_section(index, down),
                expected.is_some(),
                "{index} {down}"
            );
            assert_eq!(list.to_text(), expected.unwrap_or(text), "{index} {down}");
        }
    }

    /// Stars, the descriptions and logos settings and column widths survive the settings document,
    /// a list saved before they existed reads with stars and descriptions off and logos on, and
    /// the width of a column a newer release added is left out while the rest reads.
    #[test]
    fn list_settings_survive_the_settings_document() {
        use crate::services::UserSettings;
        let mut list = Watchlist::from_text("Main", "A,###S");
        (list.favorite, list.show_descriptions, list.show_logos) = (true, true, false);
        list.column_widths.insert(WatchlistColumn::Last, 70.0);
        list.host_column_widths.insert("Rating".into(), 60.0);
        let settings = UserSettings {
            watchlists: vec![list],
            ..Default::default()
        };
        assert_eq!(
            UserSettings::from_json(&settings.to_json()).unwrap(),
            settings
        );
        let older = r#"{"version":1,"watchlists":[{"name":"Main","items":[{"Symbol":{"symbol":"A","flag":null}},{"Section":"S"}]}]}"#;
        assert_eq!(
            UserSettings::from_json(older).unwrap().watchlists,
            [Watchlist::from_text("Main", "A,###S")]
        );
        let prior = r#"{"version":1,"watchlists":[{"name":"Main","items":[{"Symbol":{"symbol":"A","flag":null}},{"Section":"S"}],"favorite":true,"show_descriptions":true,"show_logos":false,"column_widths":{"Last":70.0}}]}"#;
        let mut expected_prior = settings.watchlists[0].clone();
        expected_prior.host_column_widths.clear();
        assert_eq!(
            UserSettings::from_json(prior).unwrap().watchlists,
            [expected_prior]
        );
        let newer = r#"{"version":1,"watchlists":[{"name":"Main","items":[],"column_widths":{"Bid":50.0,"Last":70.0}}]}"#;
        let mut expected = Watchlist::new("Main");
        expected.column_widths.insert(WatchlistColumn::Last, 70.0);
        assert_eq!(
            UserSettings::from_json(newer).unwrap().watchlists,
            [expected]
        );
    }

    /// The More menu's Symbol logos box starts checked for a new list. Unchecked, the rows of both
    /// views paint neither the host's logo nor the initial disc, and each symbol starts under the
    /// Symbol heading instead of after its mark.
    #[test]
    fn symbol_logos_switch_hides_the_marks_in_both_views() {
        use egui::accesskit::Toggled;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let logo = egui::TextureId::User(7);
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "MSFT,NVDA")]);
        panel.set_mark(
            "MSFT",
            egui::load::SizedTexture::new(logo, vec2(MARK_SIZE, MARK_SIZE)).into(),
        );
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                panel,
            );
        h.get_by_label("More").click();
        h.run();
        let toggled = h
            .get_by_label("Symbol logos")
            .accesskit_node()
            .data()
            .toggled();
        assert_eq!(toggled, Some(Toggled::True));
        h.get_by_label("Symbol logos").click();
        h.run();
        assert!(!h.state().lists()[0].show_logos);
        // Logo rects painted, whether NVDA's initial is painted, and how far right of the Symbol
        // heading each symbol starts.
        let ctx = egui::Context::default();
        let details = WatchlistDetails::default();
        let rows = |panel: &mut WatchlistPanel, wide: bool| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(300.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    if wide {
                        panel.show_wide(ui, &details);
                    } else {
                        panel.show(ui);
                    }
                },
            );
            let (mut logos, mut texts) = (0, Vec::new());
            for shape in &output.shapes {
                match &shape.shape {
                    Shape::Rect(rect) if rect.fill_texture_id() == logo => logos += 1,
                    Shape::Text(text) => texts.push((text.galley.text().to_owned(), text.pos.x)),
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
            let x = |label: &str| {
                texts
                    .iter()
                    .find(|(text, _)| text == label)
                    .map(|&(_, x)| x)
            };
            let heading = x("Symbol").unwrap();
            (
                logos,
                x("N").is_some(),
                x("MSFT").unwrap() - heading,
                x("NVDA").unwrap() - heading,
            )
        };
        for wide in [false, true] {
            assert_eq!(
                rows(h.state_mut(), wide),
                (0, false, 0.0, 0.0),
                "off, wide {wide}"
            );
        }
        h.state_mut().lists[0].show_logos = true;
        for wide in [false, true] {
            assert_eq!(
                rows(h.state_mut(), wide),
                (1, true, 24.0, 24.0),
                "on, wide {wide}"
            );
        }
    }

    /// Clearing empties only the active list, and undo reverts clears and section moves newest
    /// first, keeps later changes outside the items and never reverts over a later change of them.
    #[test]
    fn clears_and_section_moves_are_undone_until_the_list_changes() {
        use egui_kittest::{Harness, kittest::Queryable};
        let lists = vec![
            Watchlist::from_text("Main", "A,###One,B,###Two,C"),
            Watchlist::from_text("Other", "X"),
        ];
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                WatchlistPanel::new(lists),
            );
        let texts = |h: &Harness<'_, WatchlistPanel>| {
            h.state()
                .lists()
                .iter()
                .map(Watchlist::to_text)
                .collect::<Vec<_>>()
        };
        // The list name opens the list menu; a section opens its own on a right click.
        let press = |h: &mut Harness<'_, WatchlistPanel>, menu: &str, item: &str| {
            if menu == "Main" {
                h.get_by_label(menu).click();
            } else {
                h.get_by_label(menu).click_secondary();
            }
            h.run();
            h.get_by_label(item).click();
            h.run();
        };
        press(&mut h, "One", "Move section down");
        assert_eq!(texts(&h), ["A,###Two,C,###One,B", "X"]);
        press(&mut h, "Main", "Clear list");
        assert_eq!(texts(&h), ["", "X"]);
        h.state_mut().lists[0].favorite = true;
        press(&mut h, "Main", "Undo");
        assert_eq!(texts(&h), ["A,###Two,C,###One,B", "X"]);
        assert!(h.state().lists()[0].favorite, "the later star stays");
        press(&mut h, "Main", "Undo");
        assert_eq!(texts(&h), ["A,###One,B,###Two,C", "X"]);
        press(&mut h, "Two", "Move section up");
        h.state_mut().add_symbol("D");
        press(&mut h, "Main", "Undo");
        assert_eq!(texts(&h), ["A,###Two,C,###One,B,D", "X"]);
    }

    /// New list avoids names still in use after deletion, and Save a copy holds the chosen
    /// list's contents at the time of the choice even across equal names and later edits.
    #[test]
    fn equally_named_lists_stay_apart() {
        use crate::data::Reply;
        use crate::services::{ShareLink, ShareOffer};
        use egui_kittest::{Harness, kittest::Queryable};
        use std::sync::Mutex;

        #[derive(Default)]
        struct Host(Mutex<Vec<Reply<String>>>);
        impl Sharing for Host {
            fn offers(&self, _: ShareKind) -> ShareOffer {
                ShareOffer::new(false, true)
            }
            fn link(&self, _: ShareKind, _: &str, _: Reply<ShareLink>) {
                unreachable!();
            }
            fn copy(&self, _: ShareKind, _: &str, reply: Reply<String>) {
                self.0.lock().unwrap().push(reply);
            }
        }
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
                WatchlistPanel::new(vec![
                    Watchlist::new("Watchlist"),
                    Watchlist::new("Watchlist 2"),
                ]),
            );
        let press = |h: &mut Harness<'_, WatchlistPanel>, menu: &str, item: &str| {
            h.get_by_label(menu).click();
            h.run();
            h.get_by_label(item).click();
            h.run();
        };
        press(&mut h, "Watchlist", "Delete list");
        press(&mut h, "Watchlist 2", "New list");
        assert_eq!(
            h.state()
                .lists()
                .iter()
                .map(|list| list.name.as_str())
                .collect::<Vec<_>>(),
            ["Watchlist 2", "Watchlist 3"]
        );

        let host = Arc::new(Host::default());
        let mut panel = WatchlistPanel::new(vec![
            Watchlist::from_text("Dup", "A"),
            Watchlist::from_text("Dup", "B"),
        ]);
        panel.set_sharing(host.clone());
        panel.active = 1;
        panel.lists[1].favorite = true;
        *h.state_mut() = panel;
        h.run();
        press(&mut h, "More", "Save a copy");
        set_flag(&mut h.state_mut().lists, "B", Some(WatchlistFlag::Red));
        h.state_mut().lists[1].name = "Renamed".into();
        h.state_mut().lists[1].add_symbol("C");
        host.0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .send(Ok("Dup copy".into()));
        h.run();
        assert_eq!(
            h.state()
                .lists()
                .iter()
                .map(|list| (list.name.as_str(), list.to_text()))
                .collect::<Vec<_>>(),
            [
                ("Dup", "A".into()),
                ("Renamed", "B,C".into()),
                ("Dup copy", "B".into())
            ]
        );
        assert!(!h.state().lists()[2].favorite);
        assert!(matches!(
            &h.state().lists()[2].items[0],
            WatchlistItem::Symbol {
                flag: Some(WatchlistFlag::Red),
                ..
            }
        ));
    }

    /// Equal section titles collapse separately, and rename, move, undo, heading drag and
    /// removal keep the collapse state with the section rather than its title or old position.
    #[test]
    fn sections_collapse_on_their_own_and_keep_their_state() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                WatchlistPanel::new(vec![Watchlist::from_text(
                    "Main",
                    "A,###New section,B,###New section,C",
                )]),
            );
        let shown = |h: &Harness<'_, WatchlistPanel>| {
            ["A", "B", "C"]
                .into_iter()
                .filter(|label| {
                    h.query_by_role_and_label(egui::Role::ListBoxOption, label)
                        .is_some()
                })
                .collect::<Vec<_>>()
        };
        let press = |h: &mut Harness<'_, WatchlistPanel>, menu: &str, item: &str| {
            if menu == "Main" {
                h.get_by_label(menu).click();
            } else {
                h.get_by_label(menu).click_secondary();
            }
            h.run();
            h.get_by_label(item).click();
            h.run();
        };
        h.get_all_by_label("New section").next().unwrap().click();
        h.run();
        assert_eq!(shown(&h), ["A", "C"]);
        h.state_mut().lists[0].items[1] = WatchlistItem::Section("Renamed".into());
        h.run();
        assert_eq!(shown(&h), ["A", "C"]);
        press(&mut h, "Renamed", "Move section down");
        assert_eq!(
            h.state().lists()[0].to_text(),
            "A,###New section,C,###Renamed,B"
        );
        assert_eq!(shown(&h), ["A", "C"]);
        h.get_by_label("New section").click();
        h.run();
        assert_eq!(shown(&h), ["A"]);
        h.get_by_label("Renamed").click();
        h.run();
        assert_eq!(shown(&h), ["A", "B"]);
        press(&mut h, "Main", "Undo");
        assert_eq!(
            h.state().lists()[0].to_text(),
            "A,###Renamed,B,###New section,C"
        );
        assert_eq!(shown(&h), ["A", "B"], "the later toggles survive undo");
        h.get_by_label("Renamed").click();
        h.run();
        h.get_by_label("New section").click();
        h.run();
        assert_eq!(shown(&h), ["A", "C"]);
        let from = h.get_by_label("New section").rect().center();
        h.drag_at(from);
        h.run();
        h.hover_at(from + vec2(0.0, 12.0));
        h.run();
        let to = h
            .get_by_role_and_label(egui::Role::ListBoxOption, "A")
            .rect()
            .center();
        h.hover_at(to);
        h.run();
        h.drop_at(to);
        h.run();
        assert_eq!(
            h.state().lists()[0].to_text(),
            "###New section,A,###Renamed,B,C"
        );
        assert_eq!(shown(&h), ["A"]);
        press(&mut h, "New section", "Remove section");
        assert_eq!(h.state().lists()[0].to_text(), "A,###Renamed,B,C");
        assert_eq!(shown(&h), ["A"]);
    }

    /// A sort by a column hidden in the compact view leaves its rows in list order and
    /// draggable, with chart navigation in that order, while the wide view keeps its sort.
    #[test]
    fn compact_view_keeps_list_order_under_a_wide_sort() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (label, column) in [
            ("Vol", WatchlistColumn::Volume.into()),
            ("Rating", TableColumn::Host("Rating".into())),
        ] {
            let details = WatchlistDetails::default();
            let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "AAA,BBB")]);
            panel.add_column("Rating", 1);
            panel.set_column_value("Rating", "AAA", Some(5.0));
            panel.set_column_value("Rating", "BBB", Some(9.0));
            let time = Default::default();
            panel.set_quote("AAA", Quote::new(10.0, 9.0, time).with_volume(5.0));
            panel.set_quote("BBB", Quote::new(20.0, 19.0, time).with_volume(9.0));
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_size(vec2(1000.0, 600.0))
                .build_ui_state(
                    |ui, (panel, wide): &mut (WatchlistPanel, bool)| {
                        if *wide {
                            panel.show_wide(ui, &details);
                        } else {
                            panel.show(ui);
                        }
                    },
                    (panel, true),
                );
            for _ in 0..2 {
                h.get_by_label(label).click();
                h.run();
            }
            assert!(
                h.get_by_role_and_label(egui::Role::ListBoxOption, "BBB")
                    .rect()
                    .min
                    .y
                    < h.get_by_role_and_label(egui::Role::ListBoxOption, "AAA")
                        .rect()
                        .min
                        .y
            );
            assert_eq!(
                h.state_mut().0.select_next_symbol(),
                WatchlistAction::SelectSymbol("BBB".into())
            );
            h.state_mut().1 = false;
            h.run();
            assert!(
                h.get_by_role_and_label(egui::Role::ListBoxOption, "AAA")
                    .rect()
                    .min
                    .y
                    < h.get_by_role_and_label(egui::Role::ListBoxOption, "BBB")
                        .rect()
                        .min
                        .y
            );
            h.state_mut().0.set_selected("ZZZ");
            assert_eq!(
                h.state_mut().0.select_next_symbol(),
                WatchlistAction::SelectSymbol("AAA".into()),
                "chart navigation follows compact list order under a hidden {label} sort"
            );
            let from = h
                .get_by_role_and_label(egui::Role::ListBoxOption, "BBB")
                .rect()
                .center();
            h.drag_at(from);
            h.run();
            h.hover_at(from + vec2(0.0, 12.0));
            h.run();
            let to = h
                .get_by_role_and_label(egui::Role::ListBoxOption, "AAA")
                .rect()
                .center();
            h.hover_at(to);
            h.run();
            h.drop_at(to);
            h.run();
            assert_eq!(h.state().0.lists()[0].to_text(), "BBB,AAA");
            h.state_mut().1 = true;
            h.run();
            assert_eq!(h.state().0.sort, Some((column, true)));
            assert!(
                h.get_by_role_and_label(egui::Role::ListBoxOption, "BBB")
                    .rect()
                    .min
                    .y
                    < h.get_by_role_and_label(egui::Role::ListBoxOption, "AAA")
                        .rect()
                        .min
                        .y
            );
        }
    }

    /// Sorting stays inside each section, puts symbols without a value last in both directions
    /// and keeps list order on ties. A collapsed section keeps its title and hides its symbols.
    #[test]
    fn sorting_keeps_sections_in_place_and_missing_values_last() {
        let items = vec![
            symbol("C"),
            symbol("A"),
            WatchlistItem::Section("Second".into()),
            symbol("NONE"),
            symbol("UP"),
            symbol("FLAT"),
            symbol("DOWN"),
            symbol("TIE"),
        ];
        let quote = |last, prev_close| Quote::new(last, prev_close, Default::default());
        let quotes = HashMap::from([
            ("A".to_owned(), quote(10.0, 10.0)),
            ("C".to_owned(), quote(30.0, 20.0)),
            ("UP".to_owned(), quote(110.0, 100.0)),
            ("FLAT".to_owned(), quote(50.0, 50.0)),
            ("DOWN".to_owned(), quote(90.0, 100.0)),
            ("TIE".to_owned(), quote(10.0, 10.0)),
        ]);
        let names = |sort: Option<(WatchlistColumn, bool)>, collapsed: &[usize]| {
            let sort = sort.map(|(c, d)| (TableColumn::from(c), d));
            display_rows(
                &items,
                sort.as_ref().map(|(c, d)| (c, *d)),
                |c, s| table_value(c, s, &quotes, &[]),
                None,
                |row| matches!(row, Row::Item(i) if collapsed.contains(i)),
            )
            .into_iter()
            .map(|row| {
                match &items[match row {
                    Row::Item(i) => i,
                    Row::Group(_) => unreachable!(),
                }] {
                    WatchlistItem::Symbol { symbol, .. } => symbol.as_str(),
                    WatchlistItem::Section(_) => "|",
                }
            })
            .collect::<Vec<_>>()
        };
        use WatchlistColumn::*;
        assert_eq!(names(Some((Symbol, true)), &[2]), ["C", "A", "|"]);
        for (sort, expected) in [
            (None, ["C", "A", "|", "NONE", "UP", "FLAT", "DOWN", "TIE"]),
            (
                Some((Symbol, false)),
                ["A", "C", "|", "DOWN", "FLAT", "NONE", "TIE", "UP"],
            ),
            (
                Some((ChangePercent, false)),
                ["A", "C", "|", "DOWN", "FLAT", "TIE", "UP", "NONE"],
            ),
            (
                Some((ChangePercent, true)),
                ["C", "A", "|", "UP", "FLAT", "TIE", "DOWN", "NONE"],
            ),
            (
                Some((Last, true)),
                ["C", "A", "|", "UP", "DOWN", "FLAT", "TIE", "NONE"],
            ),
        ] {
            assert_eq!(names(sort, &[]), expected, "{sort:?}");
        }
        // Group details trim blanks, retain currency case and put absent metadata last.
        let mut panel =
            WatchlistPanel::new(vec![Watchlist::from_text("List", "C,A,###Hidden,B,D")]);
        for (symbol, exchange, kind, sector, currency) in [
            ("A", " alpha ", SymbolType::Stock, "Tech", "GBP"),
            ("B", "Alpha", SymbolType::Crypto, "Energy", "GBp"),
            ("C", "zeta", SymbolType::Stock, "Tech", " "),
        ] {
            let mut instrument = Instrument::new(symbol);
            instrument.exchange = Some(exchange.into());
            instrument.symbol_type = Some(kind);
            instrument.sector = Some(sector.into());
            instrument.currency = Some(currency.into());
            panel.set_instrument(&instrument);
        }
        let items = &panel.lists[0].items;
        for (grouping, expected) in [
            (
                WatchlistGrouping::Exchange,
                vec!["Alpha", "B", "alpha", "A", "zeta", "C", "Other", "D"],
            ),
            (
                WatchlistGrouping::InstrumentType,
                vec!["crypto", "B", "stock", "C", "A", "Other", "D"],
            ),
            (
                WatchlistGrouping::Sector,
                vec!["Energy", "B", "Tech", "C", "A", "Other", "D"],
            ),
            (
                WatchlistGrouping::Currency,
                vec!["GBP", "A", "GBp", "B", "Other", "C", "D"],
            ),
        ] {
            let group = |symbol: &str| group_key(&panel.groups, grouping, symbol);
            let actual: Vec<_> = display_rows(items, None, |_, _| None, Some(&group), |_| false)
                .into_iter()
                .map(|row| match row {
                    Row::Item(i) => match &items[i] {
                        WatchlistItem::Symbol { symbol, .. } => symbol.clone(),
                        _ => unreachable!(),
                    },
                    Row::Group(label) => label.unwrap_or("Other".into()),
                })
                .collect();
            assert_eq!(actual, expected, "{grouping:?}");
        }
        assert_eq!(quote(-37.63, -10.0).change_percent(), -276.3);
    }

    /// Flags belong to symbols across lists, including imports, delayed copies and undo.
    #[test]
    fn a_symbol_keeps_one_flag_in_every_list() {
        use WatchlistFlag::{Blue, Green, Red};
        use egui_kittest::{Harness, kittest::Queryable};
        let entry = |symbol: &str, flag| WatchlistItem::Symbol {
            symbol: symbol.into(),
            flag,
        };
        let mut tech = Watchlist::from_text("Tech", "MSFT,NVDA,AAPL");
        tech.items = vec![
            entry("MSFT", Some(Red)),
            entry("NVDA", None),
            entry("AAPL", Some(Red)),
        ];
        let mut other = Watchlist::new("Other");
        other.items = vec![
            entry("AAPL", Some(Blue)),
            entry("NVDA", Some(Green)),
            entry("XOM", Some(Red)),
            entry("BP", None),
        ];
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                WatchlistPanel::new(vec![tech, other]),
            );
        let flags = |h: &Harness<'_, WatchlistPanel>, symbol: &str| {
            h.state()
                .lists()
                .iter()
                .flat_map(|l| &l.items)
                .filter_map(|i| match i {
                    WatchlistItem::Symbol { symbol: at, flag } if at == symbol => Some(*flag),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(flags(&h, "AAPL"), [Some(Red), Some(Red)]);
        assert_eq!(flags(&h, "NVDA"), [Some(Green), Some(Green)]);
        h.get_by_label("AAPL").click_secondary();
        h.run();
        h.get_by_label("Green").click();
        h.run();
        assert_eq!(flags(&h, "AAPL"), [Some(Green), Some(Green)]);
        assert!(h.state_mut().add_symbol_to(0, "XOM"));
        assert_eq!(flags(&h, "XOM"), [Some(Red), Some(Red)]);
        h.state_mut().set_flag_filter(Some(Red));
        h.run();
        assert_eq!(h.state().symbols().collect::<Vec<_>>(), ["MSFT", "XOM"]);
        h.get_by_label("XOM").click_secondary();
        h.run();
        h.get_by_label("Remove from list").click();
        h.run();
        assert_eq!(
            h.state()
                .lists()
                .iter()
                .map(Watchlist::to_text)
                .collect::<Vec<_>>(),
            ["MSFT,NVDA,AAPL,XOM", "AAPL,NVDA,XOM,BP"]
        );
        assert_eq!(flags(&h, "XOM"), [None, None]);
        assert_eq!(h.state().symbols().collect::<Vec<_>>(), ["MSFT"]);
        h.state_mut().import("Imported", "AAPL,TSLA");
        h.run();
        assert_eq!(flags(&h, "AAPL"), [Some(Green); 3]);
        assert_eq!(flags(&h, "TSLA"), [None]);
        let choose = |h: &mut Harness<'_, WatchlistPanel>, from: &str, to: &str| {
            h.get_by_label(from).click();
            h.run();
            h.get_by_label(to).click();
            h.run();
        };
        choose(&mut h, "Imported", "Tech");
        choose(&mut h, "Tech", "Clear list");
        choose(&mut h, "Tech", "Other");
        h.get_by_label("AAPL").click_secondary();
        h.run();
        h.get_by_label("Remove flag").click();
        h.run();
        choose(&mut h, "Other", "Undo");
        assert_eq!(h.state().lists()[0].to_text(), "MSFT,NVDA,AAPL,XOM");
        assert_eq!(flags(&h, "AAPL"), [None; 3]);
        assert_eq!(flags(&h, "MSFT"), [Some(Red)]);
        assert_eq!(flags(&h, "NVDA"), [Some(Green); 2]);
    }

    /// A list copy keeps sections, symbol order and flags under a fresh name, the flag view lists a
    /// symbol once however many lists flag it and its edits reach each of them, and the new columns
    /// read the quote's volume and extended-hours price.
    #[test]
    fn duplicates_flag_views_and_quote_columns_follow_the_lists_and_quotes() {
        let flagged = |name: &str, flag| WatchlistItem::Symbol {
            symbol: name.into(),
            flag,
        };
        let mut tech = Watchlist::new("Tech");
        tech.items = vec![
            flagged("MSFT", Some(WatchlistFlag::Red)),
            WatchlistItem::Section("Chips".into()),
            flagged("NVDA", Some(WatchlistFlag::Blue)),
            flagged("AAPL", Some(WatchlistFlag::Red)),
        ];
        let copy = tech.duplicate(["Tech", "Tech copy"]);
        assert_eq!(copy.name, "Tech copy 2");
        assert_eq!(copy.items, tech.items);

        let mut other = Watchlist::new("Other");
        other.items = vec![
            flagged("AAPL", Some(WatchlistFlag::Red)),
            flagged("XOM", Some(WatchlistFlag::Red)),
            flagged("BP", None),
        ];
        let mut panel = WatchlistPanel::new(vec![tech, other]);
        panel.set_flag_filter(Some(WatchlistFlag::Red));
        assert_eq!(
            panel.symbols().collect::<Vec<_>>(),
            ["MSFT", "AAPL", "XOM"],
            "the first list holding a symbol decides its place"
        );
        panel.set_flag_filter(None);
        assert_eq!(panel.symbols().count(), 3);
        assert!(panel.add_symbol_to(1, "SHEL") && !panel.add_symbol_to(1, "SHEL"));
        assert!(!panel.add_symbol_to(9, "SHEL"));
        panel.set_flag_filter(Some(WatchlistFlag::Red));
        panel.import("Imported", "TSLA");
        assert_eq!(
            panel.flag_filter(),
            None,
            "an import shows the imported list"
        );
        assert_eq!(panel.symbols().collect::<Vec<_>>(), ["TSLA"]);

        let time = chrono::Utc::now();
        let regular = Quote::new(100.0, 90.0, time).with_volume(2_000.0);
        let extended = |last| crate::model::ExtendedHoursQuote {
            session: crate::model::ExtendedSession::PostMarket,
            last,
            time,
        };
        let quote = regular.with_extended(extended(105.0));
        // Hand calculations: 105 - 100 = 5, and 5 / 100 * 100 = 5%.
        for (column, expected) in [
            (WatchlistColumn::Volume, Some(2_000.0)),
            (WatchlistColumn::ExtendedLast, Some(105.0)),
            (WatchlistColumn::ExtendedChange, Some(5.0)),
            (WatchlistColumn::ExtendedChangePercent, Some(5.0)),
        ] {
            assert_eq!(column_value(column, &quote), expected, "{column:?}");
            assert_eq!(
                column_value(column, &Quote::new(100.0, 90.0, time)),
                None,
                "{column:?}"
            );
        }
        assert_eq!(
            column_value(
                WatchlistColumn::ExtendedChangePercent,
                &Quote::new(0.0, 0.0, time).with_extended(extended(1.0))
            ),
            None,
            "a change from a zero price is undefined"
        );
        panel.sort = Some((WatchlistColumn::Volume.into(), false));
        panel.set_columns([WatchlistColumn::Last]);
        assert_eq!(
            panel.sort,
            Some((WatchlistColumn::Volume.into(), false)),
            "the advanced view keeps its sort when compact columns change"
        );
    }

    #[test]
    fn row_drops_only_reorder_their_own_unsorted_list() {
        for (foreign, sorted, expected) in [
            (false, false, "A,C,B"),
            (true, false, "A,B,C"),
            (false, true, "A,B,C"),
        ] {
            let ctx = egui::Context::default();
            let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "A,B,C")]);
            panel.sort = sorted.then_some((WatchlistColumn::Symbol.into(), false));
            let frame = |panel: &mut WatchlistPanel, events| {
                let mut id = egui::Id::NULL;
                ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        id = ui.scope_id().with(("watchlist_rows", 0_usize));
                        panel.show(ui);
                    },
                )
                .drop_without_applying_deltas();
                id
            };
            let id = frame(&mut panel, vec![]);
            let target = ctx.read_response(id.with(1_usize)).unwrap().rect.center();
            frame(
                &mut panel,
                vec![
                    egui::Event::PointerMoved(target),
                    egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            egui::DragAndDrop::set_payload(
                &ctx,
                RowDrag(if foreign { id.with("other") } else { id }, 2),
            );
            frame(
                &mut panel,
                vec![egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert_eq!(panel.lists()[0].to_text(), expected);
        }
    }

    /// Dragging the left edge of a quote column header to the left widens that column by the
    /// distance dragged and keeps the width in the list, never past the room the symbol column
    /// keeps, even for a width kept from a wider row; descriptions show only when the list asks
    /// for them.
    #[test]
    fn header_drags_resize_columns_and_lists_choose_descriptions() {
        let ctx = egui::Context::default();
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "A,B")]);
        panel.set_quote("A", Quote::new(10.0, 9.0, Default::default()));
        panel.set_description("A", "Alpha Inc");
        let frame = |panel: &mut WatchlistPanel, events| {
            let mut id = egui::Id::NULL;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(300.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    id = ui.scope_id().with(("watchlist_rows", 0_usize));
                    panel.show(ui);
                },
            );
            let texts: Vec<String> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            (id, texts)
        };
        let (id, texts) = frame(&mut panel, vec![]);
        assert!(!texts.contains(&"Alpha Inc".into()));
        let edge = |at: usize| {
            ctx.read_response(id.with(("resize", at)))
                .unwrap()
                .rect
                .center()
        };
        let (last, change) = (edge(1), edge(2));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // A response read after a frame has the previous frame's rect, so a frame without input
        // follows the release.
        let drag = |panel: &mut WatchlistPanel, from: egui::Pos2, to: egui::Pos2| {
            frame(panel, vec![egui::Event::PointerMoved(from)]);
            frame(panel, vec![button(from, true)]);
            frame(panel, vec![egui::Event::PointerMoved(to)]);
            frame(panel, vec![button(to, false)]);
            frame(panel, vec![]);
        };
        drag(&mut panel, last, last - vec2(20.0, 0.0));
        let width = panel.lists()[0].column_widths[&WatchlistColumn::Last];
        assert!(
            (width - (change.x - last.x + 20.0)).abs() < 0.01,
            "{width} from {last:?} {change:?}"
        );
        assert!((edge(2).x - change.x).abs() < 0.01, "the next column stays");
        // Dragged past the room the symbol column keeps, the edge stops; a width kept from a
        // wider row stops at the same place instead of covering the symbols.
        drag(&mut panel, edge(1), pos2(1.0, last.y));
        let limit = edge(1).x;
        panel.lists[0]
            .column_widths
            .insert(WatchlistColumn::Last, 1000.0);
        frame(&mut panel, vec![]);
        frame(&mut panel, vec![]);
        assert!((edge(1).x - limit).abs() < 0.01, "{} {limit}", edge(1).x);
        panel.lists[0].show_descriptions = true;
        assert!(frame(&mut panel, vec![]).1.contains(&"Alpha Inc".into()));
    }

    /// After a click, the arrow keys move through the displayed order and select each symbol.
    #[test]
    fn arrow_keys_after_a_click_select_the_next_displayed_symbol() {
        let ctx = egui::Context::default();
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "B,###Section,A,C")]);
        panel.sort = Some((WatchlistColumn::Symbol.into(), true));
        let frame = |panel: &mut WatchlistPanel, events: Vec<egui::Event>| {
            let mut action = WatchlistAction::None;
            let mut list_id = egui::Id::NULL;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(300.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    list_id = ui.scope_id().with(("watchlist_rows", 0_usize));
                    action = panel.show(ui);
                },
            )
            .drop_without_applying_deltas();
            (action, list_id)
        };
        let (_, list_id) = frame(&mut panel, vec![]);
        // Displayed: B, Section, C, A.
        let row = ctx.read_response(list_id.with(3)).unwrap().rect.center();
        let click = |pressed| egui::Event::PointerButton {
            pos: row,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&mut panel, vec![egui::Event::PointerMoved(row)]);
        let (action, _) = frame(&mut panel, vec![click(true), click(false)]);
        assert_eq!(action, WatchlistAction::SelectSymbol("C".into()));
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        for (pressed, expected) in [
            (Key::ArrowDown, "A"),
            (Key::ArrowUp, "C"),
            (Key::ArrowUp, "B"),
        ] {
            let (action, _) = frame(&mut panel, vec![key(pressed)]);
            assert_eq!(action, WatchlistAction::SelectSymbol(expected.into()));
        }
    }

    /// The next and previous symbol steps follow the shown order: sorted, without sections and
    /// under a flag filter. They wrap at the ends and mark the symbol selected.
    #[test]
    fn next_and_previous_symbol_follow_the_shown_order_and_wrap() {
        use egui::accesskit::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        type Step = fn(&mut WatchlistPanel) -> WatchlistAction;
        let mut panel = WatchlistPanel::new(vec![
            Watchlist::from_text("List", "B,###Section,A,C"),
            Watchlist::from_text("Other", "Z,Y"),
        ]);
        panel.sort = Some((WatchlistColumn::Symbol.into(), true));
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 600.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show(ui);
                },
                panel,
            );
        h.run();
        let (next, previous): (Step, Step) = (
            WatchlistPanel::select_next_symbol,
            WatchlistPanel::select_previous_symbol,
        );
        // Shown: B, Section, C, A. Without a selection the first symbol comes next.
        for (step, expected) in [
            (next, "B"),
            (next, "C"),
            (next, "A"),
            (next, "B"),
            (previous, "A"),
            (previous, "C"),
        ] {
            assert_eq!(
                step(h.state_mut()),
                WatchlistAction::SelectSymbol(expected.into())
            );
            h.run();
        }
        assert_eq!(
            h.get_by_role_and_label(Role::ListBoxOption, "C")
                .accesskit_node()
                .is_selected(),
            Some(true)
        );
        // The red view shows B and A of the first list and Z of the other, sorted Z, B, A. The
        // selected C is not shown, so the first symbol comes next.
        let red = Some(WatchlistFlag::Red);
        for (list, item) in [(0, 0), (0, 2), (1, 0)] {
            if let WatchlistItem::Symbol { flag, .. } = &mut h.state_mut().lists[list].items[item] {
                *flag = red;
            }
        }
        h.state_mut().set_flag_filter(red);
        for (step, expected) in [(next, "Z"), (previous, "A")] {
            assert_eq!(
                step(h.state_mut()),
                WatchlistAction::SelectSymbol(expected.into())
            );
        }
        h.state_mut().set_flag_filter(Some(WatchlistFlag::Blue));
        assert_eq!(next(h.state_mut()), WatchlistAction::None);
    }

    /// Groups and host values determine both rendered order and stepping; summaries include
    /// collapsed symbols and ignore missing values. Host widths persist separately from quotes.
    #[test]
    fn wide_view_groups_summarises_and_sorts_host_columns() {
        use egui::accesskit::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text(
            "List",
            "AAA,BBB,###Second,CCC,DDD,EEE",
        )]);
        for (symbol, sector, last, prior) in [
            ("AAA", "Tech", 10.0, 8.0),
            ("BBB", "Energy", 20.0, 25.0),
            ("CCC", "Tech", 30.0, 30.0),
            ("DDD", "Tech", 50.0, 40.0),
        ] {
            let mut instrument = Instrument::new(symbol);
            instrument.sector = Some(sector.into());
            panel.set_instrument(&instrument);
            panel.set_quote(symbol, Quote::new(last, prior, Default::default()));
        }
        assert!(!panel.add_column(" ", 1));
        assert!(panel.add_column("Rating", 1));
        assert!(!panel.add_column("Rating", 2));
        for (symbol, rating) in [("AAA", 3.0), ("BBB", 4.5), ("CCC", 1.0), ("DDD", f64::NAN)] {
            assert!(panel.set_column_value("Rating", symbol, Some(rating)));
        }
        panel.set_grouping(WatchlistGrouping::Sector);
        panel.set_summary(true);
        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(1000.0, 700.0))
            .build_ui_state(
                |ui, panel: &mut WatchlistPanel| {
                    panel.show_wide(ui, &WatchlistDetails::default());
                },
                panel,
            );
        let texts = |h: &Harness<'_, WatchlistPanel>| {
            h.output()
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let order = |h: &Harness<'_, WatchlistPanel>| {
            texts(h)
                .into_iter()
                .filter(|s| {
                    [
                        "ENERGY", "TECH", "OTHER", "SECOND", "AAA", "BBB", "CCC", "DDD", "EEE",
                    ]
                    .contains(&s.as_str())
                })
                .collect::<Vec<_>>()
        };
        h.run();
        assert_eq!(
            order(&h),
            ["ENERGY", "BBB", "TECH", "AAA", "CCC", "DDD", "OTHER", "EEE"]
        );
        h.get_by_label("Rating").click();
        h.run();
        assert_eq!(
            order(&h),
            ["ENERGY", "BBB", "TECH", "CCC", "AAA", "DDD", "OTHER", "EEE"]
        );
        h.get_by_label("Rating").click();
        h.run();
        assert_eq!(
            order(&h),
            ["ENERGY", "BBB", "TECH", "AAA", "CCC", "DDD", "OTHER", "EEE"]
        );
        assert_eq!(
            h.state_mut().select_next_symbol(),
            WatchlistAction::SelectSymbol("BBB".into())
        );
        h.run();
        // Last: sum 110 / 4, middle (20 + 30) / 2. Changes: sum 7 / 4, middle (0 + 2) / 2.
        // Percent changes: sum 30 / 4, middle (0 + 25) / 2. Ratings: 8.5 / 3, middle 3.
        let expected = [
            "Average", "27.50", "+1.75", "+7.50%", "-", "-", "-", "-", "2.8", "Median", "25.00",
            "+1.00", "+12.50%", "-", "-", "-", "-", "3.0", "Minimum", "10.00", "-5.00", "-20.00%",
            "-", "-", "-", "-", "1.0", "Maximum", "50.00", "+10.00", "+25.00%", "-", "-", "-", "-",
            "4.5",
        ];
        let summary = |h: &Harness<'_, WatchlistPanel>| {
            let text = texts(h);
            text[text.len() - expected.len()..].to_vec()
        };
        assert_eq!(summary(&h), expected);
        h.get_by_role_and_label(Role::Button, "Tech").click();
        h.run();
        assert_eq!(order(&h), ["ENERGY", "BBB", "TECH", "OTHER", "EEE"]);
        assert_eq!(
            summary(&h),
            expected,
            "collapse keeps the summary population"
        );
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Tech")
                .accesskit_node()
                .data()
                .is_expanded(),
            Some(false)
        );
        let from = h.get_by_label("Resize Rating").rect().center();
        h.hover_at(from);
        h.run();
        h.drag_at(from);
        h.run();
        h.hover_at(from - vec2(20.0, 0.0));
        h.run();
        h.drop_at(from - vec2(20.0, 0.0));
        h.run();
        assert!(
            h.state().lists()[0]
                .host_column_widths
                .contains_key("Rating")
        );
        assert!(h.state().lists()[0].column_widths.is_empty());
        assert!(h.state_mut().remove_column("Rating"));
        assert!(!h.state_mut().remove_column("Rating"));
        assert_eq!(h.state().sort, None);
        assert!(
            h.state().lists()[0]
                .host_column_widths
                .contains_key("Rating")
        );
        h.state_mut().set_grouping(WatchlistGrouping::None);
        h.run();
        assert_eq!(order(&h), ["AAA", "BBB", "CCC", "DDD", "EEE"]);
        // Instrument grouping never changes the list itself or the compact view.
        h.state_mut().set_grouping(WatchlistGrouping::Sector);
        h.state_mut().wide = false;
        h.state_mut().set_selected("BBB");
        assert_eq!(
            h.state_mut().select_next_symbol(),
            WatchlistAction::SelectSymbol("CCC".into())
        );
        assert_eq!(
            h.state().lists()[0].to_text(),
            "AAA,BBB,###Second,CCC,DDD,EEE"
        );
    }

    /// Summary arithmetic stays finite at the numeric limits and handles empty and single values.
    #[test]
    fn summaries_handle_missing_single_and_extreme_values() {
        for (values, expected) in [
            (vec![], None),
            (vec![7.0], Some([7.0; 4])),
            (vec![-0.0], Some([0.0; 4])),
            (vec![f64::MAX; 3], Some([f64::MAX; 4])),
            (
                vec![f64::MAX, -f64::MAX],
                Some([0.0, 0.0, -f64::MAX, f64::MAX]),
            ),
            (vec![f64::from_bits(1); 2], Some([f64::from_bits(1); 4])),
            (vec![-1e16, 1.0, 1e16], Some([1.0 / 3.0, 1.0, -1e16, 1e16])),
            (
                vec![f64::from_bits(1), f64::from_bits(2)],
                Some([
                    f64::from_bits(2),
                    f64::from_bits(2),
                    f64::from_bits(1),
                    f64::from_bits(2),
                ]),
            ),
        ] {
            assert_eq!(summarise(values), expected);
        }
    }

    /// Guards the quote subscription: the panel subscribes to its list and the selected symbol,
    /// applies the pushes of the current stream, and subscribes again with a new stream when its
    /// symbols change, stopping the old one (whose late pushes must not land) and dropping the
    /// quotes of symbols it no longer quotes. A lagged stream is replaced; unchanged symbols keep
    /// the stream.
    #[test]
    fn quotes_follow_the_list_and_the_selected_symbol() {
        type Subscriptions = Vec<(Vec<String>, crate::services::Events<(String, Quote)>)>;
        #[derive(Default)]
        struct Recorder(std::sync::Mutex<Subscriptions>);
        impl Quotes for Recorder {
            fn subscribe(
                &self,
                symbols: &[String],
                events: crate::services::Events<(String, Quote)>,
            ) {
                self.0.lock().unwrap().push((symbols.to_vec(), events));
            }
        }
        let service = Arc::new(Recorder::default());
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "B,A")]);
        panel.set_selected("C");
        panel.set_service(service.clone());
        let ctx = egui::Context::default();
        let quote = |last| Quote::new(last, 1.0, Default::default());
        let stream = |i: usize| service.0.lock().unwrap()[i].clone();
        panel.poll(&ctx);
        let (symbols, first) = stream(0);
        assert_eq!(symbols, ["A", "B", "C"]);
        first.push(("A".into(), quote(2.0)));
        first.push(("C".into(), quote(3.0)));
        panel.poll(&ctx);
        assert_eq!(panel.quote("C").map(|q| q.last), Some(3.0));

        // Both service pushes and direct updates retain the latest finite quote.
        let latest = Quote::new(
            102.0,
            100.0,
            chrono::DateTime::from_timestamp(2, 0).unwrap(),
        );
        first.push(("A".into(), latest.clone()));
        panel.poll(&ctx);
        for invalid in [
            Quote::new(
                101.0,
                100.0,
                chrono::DateTime::from_timestamp(1, 0).unwrap(),
            ),
            Quote::new(f64::NAN, 100.0, latest.time),
            Quote::new(103.0, f64::INFINITY, latest.time),
            latest.clone().with_volume(f64::NEG_INFINITY),
            latest.clone().with_bid_ask(f64::NAN, 103.0),
            latest.clone().with_bid_ask(101.0, f64::INFINITY),
            latest
                .clone()
                .with_extended(crate::model::ExtendedHoursQuote {
                    session: crate::model::ExtendedSession::PostMarket,
                    last: f64::NAN,
                    time: latest.time,
                }),
        ] {
            first.push(("A".into(), invalid.clone()));
            panel.poll(&ctx);
            assert_eq!(panel.quote("A"), Some(&latest), "{invalid:?}");
            panel.set_quote("A", invalid);
            assert_eq!(panel.quote("A"), Some(&latest));
        }
        let corrected = latest.clone().with_bid_ask(-1.0, 0.0).with_volume(0.0);
        panel.set_quote("A", corrected.clone());
        assert_eq!(panel.quote("A"), Some(&corrected));

        panel.set_selected("D");
        panel.poll(&ctx);
        let (symbols, second) = stream(1);
        assert_eq!(symbols, ["A", "B", "D"]);
        assert!(!first.is_active() && second.is_active());
        first.push(("A".into(), quote(9.0)));
        panel.poll(&ctx);
        assert_eq!(panel.quote("A").map(|q| q.last), Some(102.0));
        assert_eq!(panel.quote("C"), None);
        assert_eq!(service.0.lock().unwrap().len(), 2);

        (0..=10_000).for_each(|_| second.push(("A".into(), quote(1.0))));
        panel.poll(&ctx);
        assert_eq!(stream(2).0, ["A", "B", "D"]);
    }

    /// Switching between the compact and wide views keeps the selection and the sort, the wide
    /// sections list only the selected symbol's rows (every shown symbol's without a selection),
    /// and a section that is loading, failed or empty says so.
    #[test]
    fn wide_view_keeps_selection_and_sort_and_states_each_section() {
        use crate::data::{CalendarEvent, CalendarKind};
        use crate::ui::widget_bar::MarketDetail;
        let ctx = egui::Context::default();
        let texts = |panel: &mut WatchlistPanel, details: Option<&WatchlistDetails>| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                match details {
                    Some(details) => panel.show_wide(ui, details),
                    None => panel.show(ui),
                };
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
        let time: chrono::DateTime<chrono::Utc> = "2026-10-28T12:00:00Z".parse().unwrap();
        let event = |symbol: &str, title: &str| {
            let mut event = CalendarEvent::new(symbol, CalendarKind::Earnings, time, title);
            event.symbol = Some(symbol.into());
            event
        };
        let mut details = WatchlistDetails::default();
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "AAA,BBB")]);
        panel.set_quote("AAA", Quote::new(10.0, 9.0, time).with_volume(5.0));
        (panel.selected, panel.sort) = (
            Some("BBB".into()),
            Some((WatchlistColumn::Volume.into(), true)),
        );
        let compact = texts(&mut panel, None);
        assert!(
            !compact.contains(&"Vol".into()),
            "compact view omits volume"
        );
        assert!(texts(&mut panel, Some(&details)).contains(&"Vol".into()));
        panel.section = WatchlistSection::Earnings;
        assert!(texts(&mut panel, Some(&details)).contains(&"Not available".into()));
        details.earnings = MarketDetail::Loading;
        assert!(texts(&mut panel, Some(&details)).contains(&"Loading…".into()));
        details.earnings = MarketDetail::Failed("down".into());
        assert!(texts(&mut panel, Some(&details)).contains(&"down".into()));
        details.earnings =
            MarketDetail::Ready(vec![event("AAA", "Alpha Inc"), event("ZZZ", "Other")]);
        assert!(
            texts(&mut panel, Some(&details)).contains(&"No data".into()),
            "BBB has no rows"
        );
        panel.selected = None;
        let shown = texts(&mut panel, Some(&details));
        assert!(shown.contains(&"Alpha Inc".into()) && !shown.contains(&"Other".into()));
        panel.selected = Some("BBB".into());
        panel.section = WatchlistSection::Overview;
        texts(&mut panel, None);
        assert_eq!(
            (panel.selected.as_deref(), panel.sort.clone()),
            (Some("BBB"), Some((WatchlistColumn::Volume.into(), true)))
        );
    }
}
