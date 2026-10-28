//! Host-fed watchlists: named lists of symbols with sections, colour flags, sorting and text
//! import and export. The host's quote service supplies the quotes, and the host persists the
//! lists.

use crate::localization as l10n;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, EventFilter, FontId, Key, Label, Layout, Modifiers, RichText, Sense,
    Shape, Stroke, TextStyle, Ui, pos2, vec2,
};

use super::object_tree::price_decimals;
use crate::data::inbox::Inbox;
use crate::icons::icons;
use crate::model::Quote;
use crate::scales::{CurrencyFormatter, PriceFormatter};
use crate::services::{Listener, Quotes};
use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, IconButton};

/// Height shared by market data rows.
pub(super) const ROW_HEIGHT: f32 = 26.0;
/// Height of a watchlist row, which fits its symbol mark.
const LIST_ROW_HEIGHT: f32 = 30.0;
/// Diameter of a symbol's mark.
const MARK_SIZE: f32 = 18.0;
/// Height of a panel header row, which fits its title and action buttons.
pub(super) const HEADER_HEIGHT: f32 = 40.0;

pub use crate::model::{Watchlist, WatchlistFlag, WatchlistItem};

/// A sortable watchlist column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchlistColumn {
    /// Sort by symbol.
    Symbol,
    /// Sort by last price.
    Last,
    /// Sort by price change.
    Change,
    /// Sort by percentage change.
    ChangePercent,
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
    /// Show the active list in the host's full-size view.
    AdvancedView,
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
    sort: Option<(WatchlistColumn, bool)>,
    selected: Option<String>,
    service: Option<Arc<dyn Quotes>>,
    inbox: Arc<Inbox>,
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
            sort: None,
            selected: None,
            service: None,
            inbox: Arc::default(),
            listener: None,
            generation: 0,
            subscribed: Vec::new(),
        }
    }

    /// Every list, for the host to persist.
    pub fn lists(&self) -> &[Watchlist] {
        &self.lists
    }

    /// The symbols of the active list, which need quotes.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.lists[self.active].symbols()
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

    /// Subscribes again when the symbols to quote changed or the stream lagged, and applies the
    /// quotes the service pushed. The quotes of symbols no longer subscribed are dropped.
    pub fn poll(&mut self, ctx: &egui::Context) {
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

    /// Adds a list read with [`Watchlist::from_text`] and shows it.
    pub fn import(&mut self, name: impl Into<String>, text: &str) {
        self.lists.push(Watchlist::from_text(name, text));
        self.active = self.lists.len() - 1;
    }

    /// Render the active list and return the host action.
    pub fn show(&mut self, ui: &mut Ui) -> WatchlistAction {
        let mut action = WatchlistAction::None;
        let Self {
            lists,
            active,
            quotes,
            decimals,
            marks,
            sort,
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
                .tooltip("Advanced view")
                .show(ui)
                .clicked()
            {
                action = WatchlistAction::AdvancedView;
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
                let title = LayoutJob::single_section(
                    l10n::text(ui.ctx(), lists[*active].name.clone()),
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
                    .selectable_label(index == *active, l10n::text(ui.ctx(), &list.name))
                    .clicked()
                {
                    *active = index;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(l10n::text(ui.ctx(), "Name"));
                ui.text_edit_singleline(&mut lists[*active].name);
            });
            if ui.button(l10n::text(ui.ctx(), "New list")).clicked() {
                lists.push(Watchlist::new(format!("Watchlist {}", lists.len() + 1)));
                *active = lists.len() - 1;
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
        });
        let list = &mut lists[*active];

        let list_id = ui.scope_id().with(("watchlist_rows", *active));
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
        // Each number column takes 23.5% of the row, which leaves the symbol column room for a
        // six-letter name after its mark; the last column's own gap completes the right margin.
        let columns = |rect: egui::Rect| {
            let right = rect.right() - 4.0;
            let number = ((rect.width() - 18.0).max(0.0) * 0.235).min(96.0);
            let edges = [
                rect.left() + 10.0,
                right - 3.0 * number,
                right - 2.0 * number,
                right - number,
                right,
            ];
            std::array::from_fn::<_, 4, _>(|i| {
                egui::Rect::from_min_max(
                    pos2(edges[i], rect.top()),
                    pos2((edges[i + 1] - 4.0).max(edges[i]), rect.bottom()),
                )
            })
        };

        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let cells = columns(header);
        for (index, (column, label)) in [
            (WatchlistColumn::Symbol, "Symbol"),
            (WatchlistColumn::Last, "Last"),
            (WatchlistColumn::Change, "Chg"),
            (WatchlistColumn::ChangePercent, "Chg%"),
        ]
        .into_iter()
        .enumerate()
        {
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
            EmptyState::new("This list is empty")
                .icon(&icons::WATCHLIST)
                .description("Add symbols with the add button above.")
                .fill_height()
                .show(ui);
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
                    let sense = if sort.is_none() {
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
                        && sort.is_none()
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
                        && sort.is_none()
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
                                let change = quote.change();
                                let color = if change > 0.0 {
                                    theme.chart.candle_up
                                } else if change < 0.0 {
                                    theme.chart.candle_down
                                } else {
                                    theme.ui.text
                                };
                                for (column, text, color) in [
                                    (1, grouped(quote.last, usize::from(decimals)), theme.ui.text),
                                    (2, signed(change, usize::from(decimals)), color),
                                    (3, format!("{}%", signed(quote.change_percent(), 2)), color),
                                ] {
                                    ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(cells[column])
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
        if let Some(index) = removed {
            list.items.remove(index);
        }
        action
    }
}

/// Text in the bundled face at `weight`, such as 700 for bold.
pub(super) fn weighted(size: f32, weight: f32, color: Color32) -> TextFormat {
    let mut format = TextFormat::simple(FontId::proportional(size), color);
    format.coords.push("wght", weight);
    format
}

/// `value` with an explicit sign, unsigned when it rounds to zero, or a dash when not finite.
pub(super) fn signed(value: f64, decimals: usize) -> String {
    let text = grouped(value, decimals);
    if value > 0.0 && text.bytes().any(|b| matches!(b, b'1'..=b'9')) {
        format!("+{text}")
    } else {
        text
    }
}

/// A price with thousands separators and `decimals` decimals, or a dash when not finite.
pub(super) fn grouped(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    CurrencyFormatter::new("", decimals).format(value)
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
    let value = |i: usize| {
        let quote = quotes.get(symbol(i))?;
        Some(match column {
            WatchlistColumn::Symbol => 0.0,
            WatchlistColumn::Last => quote.last,
            WatchlistColumn::Change => quote.change(),
            WatchlistColumn::ChangePercent => quote.change_percent(),
        })
        .filter(|v| v.is_finite())
    };
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

    fn symbol(symbol: &str) -> WatchlistItem {
        WatchlistItem::Symbol {
            symbol: symbol.into(),
            flag: None,
        }
    }

    #[test]
    fn panel_numbers_group_digits_and_omit_signs_at_rounded_zero() {
        for (value, decimals, price, change) in [
            (43237.45, 2, "43,237.45", "+43,237.45"),
            (-1234567.0, 0, "-1,234,567", "-1,234,567"),
            (863.83, 2, "863.83", "+863.83"),
            (100000.5, 1, "100,000.5", "+100,000.5"),
            (0.000123, 6, "0.000123", "+0.000123"),
            (-0.0, 2, "0.00", "0.00"),
            (-0.001, 2, "0.00", "0.00"),
            (0.001, 2, "0.00", "0.00"),
            (f64::INFINITY, 2, "-", "-"),
            (f64::NAN, 2, "-", "-"),
        ] {
            assert_eq!(grouped(value, decimals), price);
            assert_eq!(signed(value, decimals), change);
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
        for invalid in ["", " ", "AAPL", "A,B", "A\nB", "A\rB", "###Section"] {
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
}
