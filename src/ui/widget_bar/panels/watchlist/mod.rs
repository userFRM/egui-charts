//! Host-fed watchlists: named lists of symbols with sections, colour flags, sorting and text
//! import and export. The host's quote service supplies the quotes, and the host persists the
//! lists.

mod list;

pub use list::{Watchlist, WatchlistFlag, WatchlistItem};

use crate::localization as l10n;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{
    Align, Align2, Color32, EventFilter, FontId, Key, Label, Layout, Modifiers, RichText, Sense,
    Shape, Stroke, TextStyle, Ui, pos2, vec2,
};

use super::market_details::WatchlistDetails;
use super::symbol_details::detail_section;
use super::{HEADER_HEIGHT, grouped, price_decimals, signed, weighted};
use crate::data::inbox::Inbox;
use crate::data::{CalendarEvent, NewsStory};
use crate::icons::icons;
use crate::model::Quote;
use crate::scales::StudyFormat;
use crate::services::{Listener, Quotes, ShareKind, Sharing};
use crate::theme::ThemeExt;
use crate::ui::share_controls::ShareControls;
use crate::ui_kit::{EmptyState, IconButton};

/// Height of a watchlist row, which fits its symbol mark.
const LIST_ROW_HEIGHT: f32 = 30.0;
/// Diameter of a symbol's mark.
const MARK_SIZE: f32 = 18.0;

/// A sortable watchlist column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WatchlistColumn {
    /// Sort by symbol.
    Symbol,
    /// Sort by last price.
    Last,
    /// Sort by price change.
    Change,
    /// Sort by percentage change.
    ChangePercent,
    /// Sort by the session's volume.
    Volume,
    /// Sort by the last price of the extended-hours session.
    ExtendedLast,
    /// Sort by the extended-hours price change from the regular last price.
    ExtendedChange,
    /// Sort by the extended-hours percentage change from the regular last price.
    ExtendedChangePercent,
}

impl WatchlistColumn {
    /// The columns after the symbol that the panel shows by default.
    pub const DEFAULT: [Self; 3] = [Self::Last, Self::Change, Self::ChangePercent];

    /// The columns after the symbol in the wide view: all of them.
    pub const WIDE: [Self; 7] = [
        Self::Last,
        Self::Change,
        Self::ChangePercent,
        Self::Volume,
        Self::ExtendedLast,
        Self::ExtendedChange,
        Self::ExtendedChangePercent,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Symbol => "Symbol",
            Self::Last => "Last",
            Self::Change => "Chg",
            Self::ChangePercent => "Chg%",
            Self::Volume => "Vol",
            Self::ExtendedLast => "Ext",
            Self::ExtendedChange => "Ext chg",
            Self::ExtendedChangePercent => "Ext chg%",
        }
    }

    /// The figure of `quote` the column shows and sorts by, or `None` when the quote lacks it.
    /// Extended-hours changes are measured from the regular session's last price.
    fn value(self, quote: &Quote) -> Option<f64> {
        let extended = |change: fn(f64, f64) -> f64| {
            quote
                .extended
                .map(|extended| change(extended.last, quote.last))
        };
        match self {
            Self::Symbol => None,
            Self::Last => Some(quote.last),
            Self::Change => Some(quote.change()),
            Self::ChangePercent => Some(quote.change_percent()),
            Self::Volume => quote.volume,
            Self::ExtendedLast => quote.extended.map(|extended| extended.last),
            Self::ExtendedChange => extended(|last, regular| last - regular),
            Self::ExtendedChangePercent => {
                extended(|last, regular| (last - regular) / regular.abs() * 100.0)
            }
        }
        .filter(|value| value.is_finite())
    }
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

/// The source list and row index, so separate panels cannot exchange row indices.
struct RowDrag(egui::Id, usize);

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
/// [`WatchlistAction::SelectSymbol`]. Column headers cycle through ascending, descending and
/// list order; sorting keeps symbols inside their section. Clicking a section collapses or
/// expands its symbols. Unsorted rows are dragged to reorder, and rows are right-clicked to
/// flag, rename or remove. The host persists [`lists`](Self::lists) and handles file intents.
pub struct WatchlistPanel {
    lists: Vec<Watchlist>,
    active: usize,
    quotes: HashMap<String, Quote>,
    /// Decimals of the price columns per symbol, where the host knows them.
    decimals: HashMap<String, u8>,
    /// Marks of the symbols, where the host supplies them.
    marks: HashMap<String, egui::ImageSource<'static>>,
    /// The columns after the symbol.
    columns: Vec<WatchlistColumn>,
    sort: Option<(WatchlistColumn, bool)>,
    section: WatchlistSection,
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
}

impl Default for WatchlistPanel {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl WatchlistPanel {
    /// A panel showing the first of `lists`, or one empty list when there are none.
    pub fn new(mut lists: Vec<Watchlist>) -> Self {
        if lists.is_empty() {
            lists.push(Watchlist::new("Watchlist"));
        }
        Self {
            lists,
            active: 0,
            quotes: HashMap::new(),
            decimals: HashMap::new(),
            marks: HashMap::new(),
            columns: WatchlistColumn::DEFAULT.to_vec(),
            sort: None,
            section: WatchlistSection::default(),
            flag_filter: None,
            selected: None,
            service: None,
            inbox: Arc::default(),
            share: ShareControls::default(),
            listener: None,
            generation: 0,
            subscribed: Vec::new(),
        }
    }

    /// Shows these columns after the symbol, in order; the default is
    /// [`WatchlistColumn::DEFAULT`]. A quote without a column's figure shows a dash, apart from a
    /// symbol without a quote, which shows nothing. A sort by a column no longer shown is dropped.
    pub fn set_columns(&mut self, columns: impl IntoIterator<Item = WatchlistColumn>) {
        self.columns = columns
            .into_iter()
            .filter(|column| *column != WatchlistColumn::Symbol)
            .collect();
        if self.sort.is_some_and(|(column, _)| {
            column != WatchlistColumn::Symbol && !self.columns.contains(&column)
        }) {
            self.sort = None;
        }
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

    /// Shows `mark`, such as the issuer's logo, in a circle before `symbol`. Without one, the
    /// row shows the symbol's first letter on a neutral disc.
    pub fn set_mark(&mut self, symbol: &str, mark: egui::ImageSource<'static>) {
        self.marks.insert(symbol.to_owned(), mark);
    }

    /// Takes quotes from `service` from the next [`poll`](Self::poll) on.
    pub fn set_service(&mut self, service: Arc<dyn Quotes>) {
        self.service = Some(service);
        self.listener = None;
    }

    /// Offers copy-link and save-a-copy on the active list, as `sharing` reports them for
    /// [`ShareKind::Watchlist`]. A saved copy is added to the lists under the name the host
    /// answers. [`poll`](Self::poll) receives the answers.
    pub fn set_sharing(&mut self, sharing: Arc<dyn Sharing>) {
        self.share.set_service(sharing);
    }

    /// Subscribes again when the symbols to quote changed or the stream lagged, and applies the
    /// quotes the service pushed. The quotes of symbols no longer subscribed are dropped.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some((source, name)) = self.share.poll(ctx)
            && let Some(list) = self.lists.iter().find(|list| list.name == source)
        {
            let mut copy = list.clone();
            copy.name = name;
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

    /// Appends `symbol` to the active list as [`Watchlist::add_symbol`] does, such as the symbol
    /// picked after [`WatchlistAction::AddSymbol`]. Returns false when the list is unchanged.
    pub fn add_symbol(&mut self, symbol: &str) -> bool {
        self.lists[self.active].add_symbol(symbol)
    }

    /// Appends `symbol` to the list at `list` in [`lists`](Self::lists) as [`Watchlist::add_symbol`]
    /// does, such as for [`ContextMenuAction::AddToWatchlist`](crate::ui::context_menu::ContextMenuAction::AddToWatchlist).
    /// Returns false when there is no such list or it is unchanged.
    pub fn add_symbol_to(&mut self, list: usize, symbol: &str) -> bool {
        self.lists
            .get_mut(list)
            .is_some_and(|list| list.add_symbol(symbol))
    }

    /// Adds a list read with [`Watchlist::from_text`] and shows it.
    pub fn import(&mut self, name: impl Into<String>, text: &str) {
        self.lists.push(Watchlist::from_text(name, text));
        self.active = self.lists.len() - 1;
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
    /// switching views keeps them. Returns [`WatchlistAction::CompactView`] from its view button.
    pub fn show_wide(&mut self, ui: &mut Ui, details: &WatchlistDetails) -> WatchlistAction {
        self.show_with(ui, Some(details))
    }

    fn show_with(&mut self, ui: &mut Ui, wide: Option<&WatchlistDetails>) -> WatchlistAction {
        let mut action = WatchlistAction::None;
        let shown_symbols: Vec<String> = self.symbols().map(str::to_owned).collect();
        let Self {
            lists,
            share,
            active,
            quotes,
            decimals,
            marks,
            columns: compact_columns,
            sort,
            section,
            flag_filter,
            selected,
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
            for (index, list) in lists.iter().enumerate() {
                if ui
                    .selectable_label(
                        flag_filter.is_none() && index == *active,
                        l10n::text(ui.ctx(), &list.name),
                    )
                    .clicked()
                {
                    (*active, *flag_filter) = (index, None);
                }
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
                ui.text_edit_singleline(&mut lists[*active].name);
            });
            if ui.button(l10n::text(ui.ctx(), "New list")).clicked() {
                lists.push(Watchlist::new(format!("Watchlist {}", lists.len() + 1)));
                (*active, *flag_filter) = (lists.len() - 1, None);
            }
            if ui.button(l10n::text(ui.ctx(), "Duplicate list")).clicked() {
                let copy = lists[*active].duplicate(lists.iter().map(|list| list.name.as_str()));
                lists.push(copy);
                (*active, *flag_filter) = (lists.len() - 1, None);
            }
            if ui
                .add_enabled(
                    lists.len() > 1,
                    egui::Button::new(l10n::text(ui.ctx(), "Delete list")),
                )
                .clicked()
            {
                lists.remove(*active);
                *active = active.saturating_sub(1);
            }
        });
        egui::Popup::menu(&more).show(|ui| {
            if ui.button(l10n::text(ui.ctx(), "Add section")).clicked() {
                lists[*active]
                    .items
                    .push(WatchlistItem::Section("New section".to_owned()));
            }
            if ui.button(l10n::text(ui.ctx(), "Import list…")).clicked() {
                action = WatchlistAction::Import;
            }
            if ui.button(l10n::text(ui.ctx(), "Export list…")).clicked() {
                action = WatchlistAction::Export {
                    name: lists[*active].name.clone(),
                    text: lists[*active].to_text(),
                };
            }
            share.show(ui, ShareKind::Watchlist, &lists[*active].name);
        });
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
        // A flag filter draws copies of the flagged symbols, whose flag changes and removals are
        // applied to their lists after drawing.
        let mut flagged = flag_filter.map(|flag| Watchlist {
            name: String::new(),
            items: flagged_symbols(lists, flag)
                .into_iter()
                .map(|(list, item)| lists[list].items[item].clone())
                .collect(),
        });
        let filtered = flagged.is_some();
        let list = match &mut flagged {
            Some(view) => view,
            None => &mut lists[*active],
        };
        let reorderable = sort.is_none() && !filtered;

        let list_id = match flag_filter {
            Some(flag) => ui.scope_id().with(("watchlist_flagged", *flag)),
            None => ui.scope_id().with(("watchlist_rows", *active)),
        };
        // Keyed by title, so a section stays collapsed when rows move or the list is sorted.
        let collapsed_id = |title: &str| list_id.with(("collapsed", title));
        let collapsed: HashSet<usize> = ui.data(|data| {
            (0..list.items.len())
                .filter(|&i| match &list.items[i] {
                    WatchlistItem::Section(title) => {
                        data.get_temp(collapsed_id(title)).unwrap_or(false)
                    }
                    WatchlistItem::Symbol { .. } => false,
                })
                .collect()
        });
        let order = display_order(&list.items, quotes, *sort, &collapsed);
        // Arrow keys move the selection instead of the keyboard focus.
        let arrows = EventFilter {
            vertical_arrows: true,
            ..Default::default()
        };
        let mut reveal = false;
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
                .filter_map(|&i| match &list.items[i] {
                    WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
                    WatchlistItem::Section(_) => None,
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

        let font = TextStyle::Body.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        // The number columns share 70.5% of the row, 23.5% each by default, which leaves the
        // symbol column room for a six-letter name after its mark; the last column's own gap
        // completes the right margin.
        let shown_columns: &[WatchlistColumn] = if wide.is_some() {
            &WatchlistColumn::WIDE
        } else {
            compact_columns
        };
        let numbers = shown_columns.len();
        let columns = |rect: egui::Rect| {
            let right = rect.right() - 4.0;
            let number = ((rect.width() - 18.0).max(0.0) * 0.705 / numbers.max(1) as f32).min(96.0);
            let edge = |k: usize| {
                if k == 0 {
                    rect.left() + 10.0
                } else {
                    right - (numbers + 1 - k) as f32 * number
                }
            };
            (0..=numbers)
                .map(|i| {
                    egui::Rect::from_min_max(
                        pos2(edge(i), rect.top()),
                        pos2((edge(i + 1) - 4.0).max(edge(i)), rect.bottom()),
                    )
                })
                .collect::<Vec<_>>()
        };
        let all_columns: Vec<WatchlistColumn> = std::iter::once(WatchlistColumn::Symbol)
            .chain(shown_columns.iter().copied())
            .collect();

        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let cells = columns(header);
        for (index, column) in all_columns.iter().copied().enumerate() {
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
                theme.ui.text_secondary,
            );
            if let Some((sorted, descending)) = *sort
                && sorted == column
            {
                // A painted triangle beside the label: the fonts lack arrow glyphs.
                let x = if index == 0 {
                    text.right() + 7.0
                } else {
                    text.left() - 7.0
                };
                let (tip, base) = if descending { (3.0, -2.0) } else { (-3.0, 2.0) };
                let y = text.center().y;
                ui.painter().add(Shape::convex_polygon(
                    vec![
                        pos2(x - 3.5, y + base),
                        pos2(x + 3.5, y + base),
                        pos2(x, y + tip),
                    ],
                    theme.ui.text_secondary,
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
            if let Some((sorted, descending)) = *sort
                && sorted == column
            {
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_sort_direction(if descending {
                        egui::accesskit::SortDirection::Descending
                    } else {
                        egui::accesskit::SortDirection::Ascending
                    });
                });
            }
            if response.clicked() {
                *sort = match *sort {
                    Some((c, false)) if c == column => Some((column, true)),
                    Some((c, true)) if c == column => None,
                    _ => Some((column, false)),
                };
            }
        }
        ui.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0, theme.ui.border),
        );

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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for &index in &order {
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
                    let cells = columns(rect);
                    let painter = ui.painter().clone();
                    match &mut list.items[index] {
                        WatchlistItem::Section(title) => {
                            let is_collapsed = collapsed.contains(&index);
                            let translated_label = l10n::text(ui.ctx(), &*title);
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    response.enabled(),
                                    &translated_label,
                                )
                            });
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_expanded(!is_collapsed)
                            });
                            if response.clicked() {
                                let id = collapsed_id(title);
                                ui.data_mut(|data| data.insert_temp(id, !is_collapsed));
                            }
                            if response.hovered() {
                                painter.rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                            }
                            let chevron = if is_collapsed {
                                &icons::CHEVRON_RIGHT
                            } else {
                                &icons::CHEVRON_DOWN
                            };
                            let icon = egui::Rect::from_center_size(
                                pos2(rect.left() + 16.0, rect.center().y),
                                vec2(14.0, 14.0),
                            );
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
                                        .font(small.clone())
                                        .color(theme.ui.text_secondary),
                                )
                                .truncate()
                                .selectable(false),
                            );
                            response.context_menu(|ui| {
                                ui.text_edit_singleline(title);
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
                            let mark = egui::Rect::from_min_size(
                                pos2(cells[0].left(), rect.center().y - MARK_SIZE / 2.0),
                                vec2(MARK_SIZE, MARK_SIZE),
                            );
                            match marks.get(symbol.as_str()) {
                                Some(source) => egui::Image::new(source.clone())
                                    .corner_radius(MARK_SIZE / 2.0)
                                    .paint_at(ui, mark),
                                None => {
                                    painter.circle_filled(
                                        mark.center(),
                                        MARK_SIZE / 2.0,
                                        theme.ui.text_disabled,
                                    );
                                    painter.text(
                                        mark.center(),
                                        Align2::CENTER_CENTER,
                                        symbol.chars().next().unwrap_or(' ').to_uppercase(),
                                        FontId::proportional(MARK_SIZE / 2.0),
                                        Color32::WHITE,
                                    );
                                }
                            }
                            ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(cells[0].with_min_x(mark.right() + 6.0))
                                    .layout(Layout::left_to_right(Align::Center)),
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
                            if let Some(quote) = quotes.get(symbol.as_str()) {
                                let decimals = (decimals.get(symbol.as_str()).copied())
                                    .unwrap_or_else(|| price_decimals(quote.last));
                                let direction = |value: Option<f64>| match value {
                                    Some(value) if value > 0.0 => theme.chart.candle_up,
                                    Some(value) if value < 0.0 => theme.chart.candle_down,
                                    _ => theme.ui.text,
                                };
                                for (at, column) in all_columns.iter().copied().enumerate().skip(1)
                                {
                                    let text = match column.value(quote) {
                                        None => "-".to_owned(),
                                        Some(value) => match column {
                                            WatchlistColumn::Volume => {
                                                StudyFormat::Volume.format(value, None)
                                            }
                                            WatchlistColumn::ChangePercent
                                            | WatchlistColumn::ExtendedChangePercent => {
                                                format!("{}%", signed(value, 2))
                                            }
                                            WatchlistColumn::Change
                                            | WatchlistColumn::ExtendedChange => {
                                                signed(value, usize::from(decimals))
                                            }
                                            _ => grouped(value, usize::from(decimals)),
                                        },
                                    };
                                    let color = match column {
                                        WatchlistColumn::Last
                                        | WatchlistColumn::Volume
                                        | WatchlistColumn::ExtendedLast => theme.ui.text,
                                        _ => direction(column.value(quote)),
                                    };
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
                                            *flag = Some(choice);
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
                                    *flag = None;
                                    ui.close();
                                }
                                if ui
                                    .button(l10n::text(ui.ctx(), "Remove from list"))
                                    .clicked()
                                {
                                    removed = Some(index);
                                    ui.close();
                                }
                            });
                        }
                    }
                }
            });
        if let Some((from, to)) = moved {
            let item = list.items.remove(from);
            list.items.insert(to, item);
        }
        if let Some(index) = removed
            && !filtered
        {
            list.items.remove(index);
        }
        if let (Some(view), Some(filter)) = (flagged, *flag_filter) {
            apply_flag_view(lists, filter, &view.items, removed);
        }
        action
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

/// Applies the edits of the `filter` flag view to `lists`. Each row stands for its symbol wherever
/// it is flagged `filter`, so a new flag reaches every such entry and removing the row removes them
/// all; otherwise the row would come back from another list.
fn apply_flag_view(
    lists: &mut [Watchlist],
    filter: WatchlistFlag,
    view: &[WatchlistItem],
    removed: Option<usize>,
) {
    for (index, item) in view.iter().enumerate() {
        let WatchlistItem::Symbol { symbol, flag: new } = item else {
            continue;
        };
        let remove = removed == Some(index);
        if *new == Some(filter) && !remove {
            continue;
        }
        for list in lists.iter_mut() {
            list.items.retain_mut(|entry| match entry {
                WatchlistItem::Symbol { symbol: at, flag }
                    if at == symbol && *flag == Some(filter) =>
                {
                    *flag = *new;
                    !remove
                }
                _ => true,
            });
        }
    }
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

/// Item indices in display order: list order, or with `sort`, each run of symbols between
/// sections sorted while sections keep their place. Symbols without a value sort last in both
/// directions; ties keep list order. The symbols of the `collapsed` sections are left out.
fn display_order(
    items: &[WatchlistItem],
    quotes: &HashMap<String, Quote>,
    sort: Option<(WatchlistColumn, bool)>,
    collapsed: &HashSet<usize>,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    let mut hidden = false;
    order.retain(|&i| {
        if matches!(items[i], WatchlistItem::Section(_)) {
            hidden = collapsed.contains(&i);
            return true;
        }
        !hidden
    });
    let Some((column, descending)) = sort else {
        return order;
    };
    let symbol = |i: usize| match &items[i] {
        WatchlistItem::Symbol { symbol, .. } => symbol.as_str(),
        WatchlistItem::Section(_) => "",
    };
    let value = |i: usize| column.value(quotes.get(symbol(i))?);
    for run in order.split_mut(|&i| matches!(items[i], WatchlistItem::Section(_))) {
        run.sort_by(|&a, &b| {
            let ordering = if column == WatchlistColumn::Symbol {
                symbol(a).cmp(symbol(b))
            } else {
                match (value(a), value(b)) {
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
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn symbol(symbol: &str) -> WatchlistItem {
        WatchlistItem::Symbol {
            symbol: symbol.into(),
            flag: None,
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
        let names = |sort, collapsed: &[usize]| {
            display_order(&items, &quotes, sort, &collapsed.iter().copied().collect())
                .into_iter()
                .map(|i| match &items[i] {
                    WatchlistItem::Symbol { symbol, .. } => symbol.as_str(),
                    WatchlistItem::Section(_) => "|",
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
        assert_eq!(quote(-37.63, -10.0).change_percent(), -276.3);
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
        // A row of the red view stands for the symbol in every list that flags it red, so
        // unflagging AAPL or removing XOM there cannot leave a red copy behind in another list.
        let red = Some(WatchlistFlag::Red);
        apply_flag_view(
            &mut panel.lists,
            WatchlistFlag::Red,
            &[flagged("AAPL", None)],
            None,
        );
        apply_flag_view(
            &mut panel.lists,
            WatchlistFlag::Red,
            &[flagged("XOM", red)],
            Some(0),
        );
        panel.set_flag_filter(red);
        assert_eq!(panel.symbols().collect::<Vec<_>>(), ["MSFT"]);
        assert_eq!(
            panel.lists[1].symbols().collect::<Vec<_>>(),
            ["AAPL", "BP", "SHEL"]
        );

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
            assert_eq!(column.value(&quote), expected, "{column:?}");
            assert_eq!(
                column.value(&Quote::new(100.0, 90.0, time)),
                None,
                "{column:?}"
            );
        }
        assert_eq!(
            WatchlistColumn::ExtendedChangePercent
                .value(&Quote::new(0.0, 0.0, time).with_extended(extended(1.0))),
            None,
            "a change from a zero price is undefined"
        );
        panel.sort = Some((WatchlistColumn::Volume, false));
        panel.set_columns([WatchlistColumn::Last]);
        assert_eq!(panel.sort, None, "a sort by a hidden column is dropped");
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
            panel.sort = sorted.then_some((WatchlistColumn::Symbol, false));
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

    /// After a click, the arrow keys move through the displayed order and select each symbol.
    #[test]
    fn arrow_keys_after_a_click_select_the_next_displayed_symbol() {
        let ctx = egui::Context::default();
        let mut panel = WatchlistPanel::new(vec![Watchlist::from_text("List", "B,###Section,A,C")]);
        panel.sort = Some((WatchlistColumn::Symbol, true));
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
        (panel.selected, panel.sort) = (Some("BBB".into()), Some((WatchlistColumn::Volume, true)));
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
            (panel.selected.as_deref(), panel.sort),
            (Some("BBB"), Some((WatchlistColumn::Volume, true)))
        );
    }
}
