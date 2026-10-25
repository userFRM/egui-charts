//! Host-fed watchlists: named lists of symbols with sections, colour flags, sorting and text
//! import and export. The host supplies quotes and persists the lists.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, EventFilter, FontId, Key, Label, Layout, Modifiers, RichText, Sense,
    Shape, Stroke, TextStyle, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use super::object_tree::{format_price, price_decimals};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, IconButton};

const ROW_HEIGHT: f32 = 26.0;
/// Height of a panel header row, which fits its title and action buttons.
pub(super) const HEADER_HEIGHT: f32 = 40.0;

/// A colour flag marking a watchlist symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WatchlistFlag {
    Red,
    Blue,
    Green,
    Orange,
    Purple,
    Cyan,
    Pink,
}

impl WatchlistFlag {
    /// Every flag, in menu order.
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Blue,
        Self::Green,
        Self::Orange,
        Self::Purple,
        Self::Cyan,
        Self::Pink,
    ];

    /// The flag colour, the same in every theme.
    pub fn color(self) -> Color32 {
        match self {
            Self::Red => Color32::from_rgb(229, 57, 53),
            Self::Blue => Color32::from_rgb(30, 136, 229),
            Self::Green => Color32::from_rgb(67, 160, 71),
            Self::Orange => Color32::from_rgb(251, 140, 0),
            Self::Purple => Color32::from_rgb(142, 36, 170),
            Self::Cyan => Color32::from_rgb(0, 172, 193),
            Self::Pink => Color32::from_rgb(216, 27, 96),
        }
    }
}

/// One row of a watchlist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WatchlistItem {
    /// A symbol, optionally flagged.
    Symbol {
        symbol: String,
        flag: Option<WatchlistFlag>,
    },
    /// A section title grouping the symbols below it, up to the next section.
    Section(String),
}

/// A named list of symbols and sections, in the order the user arranged them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watchlist {
    pub name: String,
    pub items: Vec<WatchlistItem>,
}

impl Watchlist {
    /// An empty list.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
        }
    }

    /// The symbols of the list, in list order.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|item| match item {
            WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
            WatchlistItem::Section(_) => None,
        })
    }

    /// Appends `symbol`, trimmed. Returns false, leaving the list unchanged, for an empty symbol,
    /// one containing a text separator or starting with `###`, or one the list already holds.
    pub fn add_symbol(&mut self, symbol: &str) -> bool {
        let symbol = symbol.trim();
        if symbol.is_empty()
            || symbol.contains([',', '\n', '\r'])
            || symbol.starts_with("###")
            || self.symbols().any(|s| s == symbol)
        {
            return false;
        }
        self.items.push(WatchlistItem::Symbol {
            symbol: symbol.to_owned(),
            flag: None,
        });
        true
    }

    /// The list as comma-separated text, each section written as `###` and its title, the format
    /// [`from_text`](Self::from_text) reads. Flags are not exported. Commas and line breaks in a
    /// section title become spaces.
    pub fn to_text(&self) -> String {
        let entries: Vec<String> = self
            .items
            .iter()
            .map(|item| match item {
                WatchlistItem::Symbol { symbol, .. } => symbol.clone(),
                WatchlistItem::Section(title) => {
                    format!("###{}", title.replace([',', '\n', '\r'], " "))
                }
            })
            .collect();
        entries.join(",")
    }

    /// A list named `name` from text separated by commas or line breaks. An entry starting with
    /// `###` is a section title; every other entry is a symbol. Entries are trimmed; empty
    /// entries and symbols already listed are skipped.
    pub fn from_text(name: impl Into<String>, text: &str) -> Self {
        let mut list = Self::new(name);
        for entry in text.split([',', '\n', '\r']).map(str::trim) {
            if let Some(title) = entry.strip_prefix("###") {
                list.items
                    .push(WatchlistItem::Section(title.trim().to_owned()));
            } else {
                list.add_symbol(entry);
            }
        }
        list
    }
}

/// The latest prices of one watchlist symbol, supplied by the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WatchlistQuote {
    /// Last traded price.
    pub last: f64,
    /// Close of the previous session. The change is `last - prev_close`, and the change percent
    /// is that difference divided by the absolute previous close, times 100.
    pub prev_close: f64,
    /// Decimals of the price columns, as `Instrument::price_decimals`. `None` chooses them from
    /// the size of the last price.
    pub price_decimals: Option<u8>,
}

impl WatchlistQuote {
    pub(super) fn change(&self) -> f64 {
        self.last - self.prev_close
    }

    pub(super) fn change_percent(&self) -> f64 {
        if self.prev_close == 0.0 {
            f64::NAN
        } else {
            self.change() / self.prev_close.abs() * 100.0
        }
    }
}

/// A sortable watchlist column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchlistColumn {
    Symbol,
    Last,
    Change,
    ChangePercent,
}

/// An action the host handles.
#[derive(Clone, Debug, PartialEq, Default)]
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
    Export { name: String, text: String },
}

/// The source list and row index, so separate panels cannot exchange row indices.
struct RowDrag(egui::Id, usize);

/// Watchlists fed by host quotes.
///
/// Feed quotes with [`set_quote`](Self::set_quote) for the [`symbols`](Self::symbols) of the
/// active list, and keep the chart symbol selected with [`set_selected`](Self::set_selected).
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
    quotes: HashMap<String, WatchlistQuote>,
    sort: Option<(WatchlistColumn, bool)>,
    selected: Option<String>,
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
            sort: None,
            selected: None,
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

    /// Sets the latest quote of `symbol`.
    pub fn set_quote(&mut self, symbol: &str, quote: WatchlistQuote) {
        self.quotes.insert(symbol.to_owned(), quote);
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
            sort,
            selected,
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
                    lists[*active].name.clone(),
                    weighted(theme.text.title, 700.0, theme.ui.text),
                );
                menu = Some(
                    ui.add(
                        egui::Button::new((title, chevron))
                            .frame_when_inactive(false)
                            .image_tint_follows_text_color(true)
                            .truncate(),
                    )
                    .on_hover_text("Lists"),
                );
            });
        });
        let (Some(menu), Some(more)) = (menu, more) else {
            return action;
        };
        egui::Popup::menu(&menu).show(|ui| {
            for (index, list) in lists.iter().enumerate() {
                if ui.selectable_label(index == *active, &list.name).clicked() {
                    *active = index;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut lists[*active].name);
            });
            if ui.button("New list").clicked() {
                lists.push(Watchlist::new(format!("Watchlist {}", lists.len() + 1)));
                *active = lists.len() - 1;
            }
            if ui
                .add_enabled(lists.len() > 1, egui::Button::new("Delete list"))
                .clicked()
            {
                lists.remove(*active);
                *active = active.saturating_sub(1);
            }
        });
        egui::Popup::menu(&more).show(|ui| {
            if ui.button("Add section").clicked() {
                lists[*active]
                    .items
                    .push(WatchlistItem::Section("New section".to_owned()));
            }
            if ui.button("Import list…").clicked() {
                action = WatchlistAction::Import;
            }
            if ui.button("Export list…").clicked() {
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
        let columns = |rect: egui::Rect| {
            let right = rect.right() - 8.0;
            let number = ((rect.width() - 18.0).max(0.0) * 0.24).min(96.0);
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
                label,
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
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::ColumnHeader, response.enabled(), label)
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
                        vec2(ui.available_width(), ROW_HEIGHT),
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
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    response.enabled(),
                                    &*title,
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
                                    RichText::new(title.to_uppercase())
                                        .font(small.clone())
                                        .color(theme.ui.text_secondary),
                                )
                                .truncate()
                                .selectable(false),
                            );
                            response.context_menu(|ui| {
                                ui.text_edit_singleline(title);
                                if ui.button("Remove section").clicked() {
                                    removed = Some(index);
                                    ui.close();
                                }
                            });
                        }
                        WatchlistItem::Symbol { symbol, flag } => {
                            let is_selected = selected.as_deref() == Some(symbol.as_str());
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::ListBoxOption,
                                    response.enabled(),
                                    symbol.as_str(),
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
                            ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(cells[0])
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
                                let decimals = quote
                                    .price_decimals
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
                                    (1, format_price(quote.last, Some(decimals)), theme.ui.text),
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
                                            RichText::new(text).font(font.clone()).color(color),
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
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::Role::RadioButton,
                                                response.enabled(),
                                                *flag == Some(choice),
                                                format!("{choice:?}"),
                                            )
                                        });
                                        ui.painter().circle_filled(
                                            swatch.center(),
                                            if *flag == Some(choice) { 8.0 } else { 6.0 },
                                            choice.color(),
                                        );
                                        if response.on_hover_text(format!("{choice:?}")).clicked() {
                                            *flag = Some(choice);
                                            ui.close();
                                        }
                                    }
                                });
                                if ui
                                    .add_enabled(flag.is_some(), egui::Button::new("Remove flag"))
                                    .clicked()
                                {
                                    *flag = None;
                                    ui.close();
                                }
                                if ui.button("Remove from list").clicked() {
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

/// `value` with an explicit sign, unsigned when zero, or a dash when not finite.
pub(super) fn signed(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        "-".to_owned()
    } else if value == 0.0 {
        format!("{:.*}", decimals, 0.0)
    } else {
        format!("{value:+.decimals$}")
    }
}

/// Item indices in display order: list order, or with `sort`, each run of symbols between
/// sections sorted while sections keep their place. Symbols without a value sort last in both
/// directions; ties keep list order. The symbols of the `collapsed` sections are left out.
fn display_order(
    items: &[WatchlistItem],
    quotes: &HashMap<String, WatchlistQuote>,
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
        let quote = |last, prev_close| WatchlistQuote {
            last,
            prev_close,
            price_decimals: None,
        };
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
}
