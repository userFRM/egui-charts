//! Instrument search and comparison dialogs
//!
//! Provides symbol search with keyboard navigation
//! and multi-symbol comparison functionality.
//!
//! # Example - Instrument Search
//!
//! ```no_run
//! use egui_charts::model::Instrument;
//! use egui_charts::ui::top_toolbar::SymbolSearchDialog;
//!
//! # fn update(ctx: &egui::Context) {
//! let mut dialog = SymbolSearchDialog::new();
//! let symbols = vec![
//!     Instrument::new("BTCUSDT").with_description("Bitcoin / Tether"),
//!     Instrument::new("ETHUSDT").with_description("Ethereum / Tether"),
//! ];
//!
//! // In your egui update loop
//! if let Some(selected) = dialog.show(ctx, &symbols) {
//!     println!("Selected: {}", selected.symbol);
//! }
//! # }
//! ```
//!
//! # Example - Compare Symbols
//!
//! ```no_run
//! use egui_charts::model::Instrument;
//! use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbolsDialog};
//!
//! # fn update(ctx: &egui::Context) {
//! let mut compare = CompareSymbolsDialog::new();
//! compare.open();
//!
//! // The symbols your data source offers
//! let symbols = vec![Instrument::new("ETHUSDT").with_description("Ethereum / Tether")];
//! match compare.show(ctx, &symbols) {
//!     CompareAction::AddSymbol(sym) => println!("Add: {}", sym),
//!     CompareAction::RemoveSymbol(sym) => println!("Remove: {}", sym),
//!     _ => {}
//! }
//! # }
//! ```

use crate::localization as l10n;
mod service;

use crate::icons::icons;
use crate::model::{Instrument, SymbolType};
use crate::services::{Storage, SymbolSearch};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::EmptyState;
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::tab_bar::{TabBar, TabLabel};
use egui::{Color32, Context, Pos2, Response, RichText, Sense, Ui, Vec2};
use service::Search;
use std::sync::Arc;

// =============================================================================
// Instrument Search Dialog
// =============================================================================

#[derive(Clone, Copy, PartialEq)]
struct Category(Option<SymbolType>);

impl TabLabel for Category {
    fn tab_label(&self) -> &str {
        match self.0 {
            None => "All",
            Some(SymbolType::Stock) => "Stocks",
            Some(SymbolType::Index) => "Indices",
            Some(SymbolType::Forex) => "Forex",
            Some(SymbolType::Futures) => "Futures",
            Some(SymbolType::Crypto) => "Crypto",
            Some(SymbolType::Cfd) => "CFDs",
            Some(SymbolType::Bond) => "Bonds",
            Some(SymbolType::Fund) => "Funds",
            Some(SymbolType::Option) => "Options",
            Some(SymbolType::Commodity) => "Commodities",
            Some(SymbolType::Economic) => "Economy",
            Some(SymbolType::Dr) => "Depositary receipts",
            Some(SymbolType::Right) => "Rights",
            Some(SymbolType::Warrant) => "Warrants",
            Some(SymbolType::Expression) => "Expressions",
            Some(SymbolType::Spread) => "Spreads",
        }
    }
}

/// Configuration for symbol search dialog
///
/// Customize the behavior and appearance of the symbol search.
///
/// # Example
///
/// ```
/// use egui_charts::ui::top_toolbar::SymbolSearchConfig;
///
/// let config = SymbolSearchConfig {
///     show_exchange: true,
///     show_description: true,
///     max_results: 50,
///     min_search_len: 2,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct SymbolSearchConfig {
    /// Show exchange/desc information below symbol name
    pub show_exchange: bool,
    /// Show full symbol desc
    pub show_description: bool,
    /// Results per service page, or the maximum displayed from the supplied list
    pub max_results: usize,
    /// Min characters for a nonempty query; an empty query lists the catalog
    pub min_search_len: usize,
}

impl Default for SymbolSearchConfig {
    fn default() -> Self {
        Self {
            show_exchange: true,
            show_description: true,
            max_results: 20,
            min_search_len: 1,
        }
    }
}

/// Instrument search dialog with keyboard navigation
///
/// A modal dialog for searching and selecting symbols. Features:
/// - Real-time filtering as you type
/// - Keyboard navigation (Up/Down to move, Enter to select, Esc to close)
/// - Case-insensitive search
/// - Searches both symbol names and descriptions
///
/// # Example
///
/// ```no_run
/// use egui_charts::model::Instrument;
/// use egui_charts::ui::top_toolbar::{SymbolSearchConfig, SymbolSearchDialog};
///
/// # fn update(ctx: &egui::Context) {
/// let mut dialog = SymbolSearchDialog::new()
///     .with_config(SymbolSearchConfig {
///         max_results: 20,
///         ..Default::default()
///     });
///
/// // Open the dialog (e.g., on Ctrl+K)
/// dialog.open();
///
/// // In your egui update loop
/// let symbols = vec![Instrument::new("BTCUSDT").with_description("Bitcoin")];
/// if let Some(symbol) = dialog.show(ctx, &symbols) {
///     // User selected a symbol
///     println!("Switching to: {}", symbol.symbol);
/// }
/// # }
/// ```
pub struct SymbolSearchDialog {
    is_open: bool,
    focus_search: bool,
    search_query: String,
    sel_idx: usize,
    filtered_symbols: Vec<Instrument>,
    category: Category,
    recent_symbols: Vec<String>,
    config: SymbolSearchConfig,
    search: Search,
}

impl Default for SymbolSearchDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolSearchDialog {
    /// Creates a new symbol search dialog with default configuration
    pub fn new() -> Self {
        Self {
            is_open: false,
            focus_search: true,
            search_query: String::new(),
            sel_idx: 0,
            filtered_symbols: Vec::new(),
            category: Category(None),
            recent_symbols: Vec::new(),
            config: SymbolSearchConfig::default(),
            search: Search::default(),
        }
    }

    /// Sets a custom configuration for the dialog
    ///
    /// # Example
    ///
    /// ```
    /// use egui_charts::ui::top_toolbar::{SymbolSearchConfig, SymbolSearchDialog};
    ///
    /// let dialog = SymbolSearchDialog::new()
    ///     .with_config(SymbolSearchConfig {
    ///         max_results: 50,
    ///         min_search_len: 2,
    ///         ..Default::default()
    ///     });
    /// ```
    pub fn with_config(mut self, config: SymbolSearchConfig) -> Self {
        self.config = config;
        self
    }

    /// Connects host symbol search. Without a service, `show` searches its supplied list.
    pub fn set_search_service(&mut self, service: Option<Arc<dyn SymbolSearch>>) {
        self.search.set_service(service);
    }

    /// Keeps recently selected instruments through the host's storage service.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.search.recents.set_storage(storage);
    }

    /// Restricts service results to an exchange, or removes that filter.
    pub fn set_exchange_filter(&mut self, exchange: Option<String>) {
        self.search.exchange = exchange;
    }

    /// Records an instrument opened outside this dialog in the recent list.
    pub fn record_symbol(&mut self, symbol: &Instrument) {
        self.search.record(symbol);
    }

    /// Supply recent symbol names, newest first. Unknown names are ignored.
    pub fn set_recent_symbols(&mut self, symbols: Vec<String>) {
        self.recent_symbols = symbols;
    }

    /// Opens the symbol search dialog
    ///
    /// Clears any previous search state and shows the dialog.
    pub fn open(&mut self) {
        self.is_open = true;
        self.focus_search = true;
        self.search_query.clear();
        self.sel_idx = 0;
        self.category = Category(None);
        self.filtered_symbols.clear();
        self.search.reset();
    }

    /// Open symbol search with text supplied by the host's keyboard dispatcher.
    pub fn open_with_query(&mut self, query: String) {
        self.open();
        self.search_query = query;
    }

    /// Closes the symbol search dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Returns whether the dialog is currently open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Shows the symbol search dialog and returns the selected symbol
    ///
    /// Displays a modal search dialog with real-time filtering and keyboard navigation.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The egui context
    /// * `all_symbols` - List of all available symbols to search from
    ///
    /// # Returns
    ///
    /// * `Some(Instrument)` - The symbol selected by the user
    /// * `None` - Dialog closed without selection or still searching
    ///
    /// # Keyboard Shortcuts
    ///
    /// - **Up/Down**: Navigate results
    /// - **Enter**: Select highlighted symbol
    /// - **Esc**: Close dialog
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use egui_charts::model::Instrument;
    /// # use egui_charts::ui::top_toolbar::SymbolSearchDialog;
    /// # fn update(ctx: &egui::Context, dialog: &mut SymbolSearchDialog) {
    /// let symbols = vec![
    ///     Instrument::new("BTCUSDT").with_description("Bitcoin / Tether"),
    ///     Instrument::new("ETHUSDT").with_description("Ethereum / Tether"),
    /// ];
    ///
    /// if let Some(symbol) = dialog.show(ctx, &symbols) {
    ///     println!("User selected: {}", symbol.symbol);
    /// }
    /// # }
    /// ```
    #[must_use = "Instrument selection should be handled"]
    pub fn show(&mut self, ctx: &Context, all_symbols: &[Instrument]) -> Option<Instrument> {
        self.search.poll(ctx);
        if self.search.has_recents() {
            self.recent_symbols = self.search.recent_names();
        }
        if !self.is_open {
            return None;
        }

        self.refresh_results(ctx, all_symbols);
        let mut sel_symbol = None;
        let mut should_close = false;
        let search_id = egui::Id::unique("symbol_search_input");
        let focus_search = std::mem::take(&mut self.focus_search);
        let search_focused = ctx.memory(|m| m.has_focus(search_id));
        let (up, down) = ctx.input(|input| {
            (
                search_focused && input.key_pressed(egui::Key::ArrowUp),
                search_focused && input.key_pressed(egui::Key::ArrowDown),
            )
        });
        let keyboard_move = up || down;
        if !self.filtered_symbols.is_empty() {
            if down {
                self.sel_idx = (self.sel_idx + 1) % self.filtered_symbols.len();
            }
            if up {
                self.sel_idx =
                    (self.sel_idx + self.filtered_symbols.len() - 1) % self.filtered_symbols.len();
            }
        }

        let mut open = true;
        Modal::new("symbol_search", "")
            .width(840.0)
            .height(680.0)
            .rounding(SEARCH_RADIUS)
            .backdrop(search_backdrop(ctx))
            .show(ctx, &mut open, |ui| {
                let translated_label = l10n::text(ui.ctx(), "Search symbols");
                ui.response().widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Dialog, true, &translated_label)
                });
                ui.ctx()
                    .accesskit_node_builder(ui.response().id, |node| node.set_modal());
                ui.horizontal(|ui| {
                    let theme = ui.active_theme();
                    ui.add(
                        icons::QUICK_SEARCH
                            .as_image_tinted(Vec2::splat(24.0), theme.ui.text_secondary),
                    );
                    let search_id = egui::Id::unique("symbol_search_input");
                    if focus_search {
                        ui.memory_mut(|memory| memory.request_focus(search_id));
                    }
                    let response = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.search_query)
                                .id(search_id)
                                .font(egui::FontId::proportional(16.0))
                                .hint_text(
                                    RichText::new(l10n::text(ui.ctx(), "Search symbols"))
                                        .size(16.0),
                                )
                                .frame(egui::Frame::NONE)
                                .desired_width((ui.available_width() - 40.0).max(0.0)),
                        )
                        .accessible_name(l10n::text(ui.ctx(), "Search symbols"));
                    if IconButton::new(&icons::CLOSE)
                        .tooltip("Close")
                        .show(ui)
                        .clicked()
                    {
                        should_close = true;
                    }
                    // Sizing passes clear invisible widgets' focus while building the field.
                    if focus_search {
                        response.request_focus();
                    }
                    if response.changed() {
                        self.sel_idx = 0;
                        self.refresh_results(ctx, all_symbols);
                    }
                });
                ui.add_space(16.0);
                let mut categories = vec![Category(None)];
                if self.search.connected() {
                    categories.extend(
                        [
                            SymbolType::Stock,
                            SymbolType::Index,
                            SymbolType::Forex,
                            SymbolType::Futures,
                            SymbolType::Crypto,
                            SymbolType::Cfd,
                            SymbolType::Bond,
                            SymbolType::Fund,
                            SymbolType::Option,
                            SymbolType::Commodity,
                            SymbolType::Economic,
                            SymbolType::Dr,
                            SymbolType::Right,
                            SymbolType::Warrant,
                            SymbolType::Expression,
                            SymbolType::Spread,
                        ]
                        .map(|kind| Category(Some(kind))),
                    );
                } else {
                    for symbol in all_symbols {
                        let category = Category(symbol.symbol_type);
                        if !categories.contains(&category) {
                            categories.push(category);
                        }
                    }
                }
                if !categories.contains(&self.category) {
                    self.category = Category(None);
                }
                if TabBar::new(&categories, &mut self.category)
                    .pills()
                    .show(ui)
                {
                    self.sel_idx = 0;
                    self.refresh_results(ctx, all_symbols);
                }
                ui.add_space(4.0);

                self.search.status(ui);
                if self.search_query.is_empty()
                    && self.category.0.is_none()
                    && self.search.exchange.is_none()
                    && self.search.has_recents()
                {
                    ui.weak(l10n::text(ui.ctx(), "Recently opened"));
                }
                let mut next_page = false;
                // Results list
                egui::ScrollArea::vertical()
                    .id_salt("symbol_results")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 36.0).max(0.0))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if self.filtered_symbols.is_empty() {
                            if !self.search_query.is_empty()
                                && self.search_query.len() < self.config.min_search_len
                            {
                                ui.label(l10n::format_text(
                                    ui.ctx(),
                                    "Type at least {0} {1} to search",
                                    &[
                                        ("0", format!("{}", self.config.min_search_len)),
                                        (
                                            "1",
                                            (if self.config.min_search_len == 1 {
                                                "character"
                                            } else {
                                                "characters"
                                            })
                                            .to_string(),
                                        ),
                                    ],
                                ));
                            } else {
                                ui.label(l10n::text(ui.ctx(), "No symbols found"));
                            }
                        } else {
                            for (idx, symbol) in self.filtered_symbols.iter().enumerate() {
                                let is_sel = idx == self.sel_idx;
                                let response = self.render_symbol_item(ui, symbol, is_sel);
                                next_page |= idx + 1 == self.filtered_symbols.len()
                                    && ui.is_rect_visible(response.rect);

                                if response.clicked() {
                                    sel_symbol = Some(symbol.clone());
                                    should_close = true;
                                }

                                if keyboard_move && is_sel {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if !keyboard_move
                                    && response.hovered()
                                    && ui.input(|input| input.pointer.delta() != Vec2::ZERO)
                                {
                                    self.sel_idx = idx;
                                }
                            }
                        }
                    });

                if next_page {
                    self.search.next_page();
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (key, action) in
                        [("↑ ↓", "Navigate"), ("Enter", "Select"), ("Esc", "Close")]
                    {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), key))
                                .monospace()
                                .size(11.0),
                        );
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), action))
                                .size(11.0)
                                .weak(),
                        );
                        ui.add_space(12.0);
                    }
                });

                // Keyboard navigation
                ui.input(|i| {
                    if search_focused
                        && i.key_pressed(egui::Key::Enter)
                        && !self.filtered_symbols.is_empty()
                    {
                        sel_symbol = Some(self.filtered_symbols[self.sel_idx].clone());
                        should_close = true;
                    }
                });
            });

        if should_close || !open {
            self.close();
        }

        if let Some(symbol) = &sel_symbol {
            self.search.record(symbol);
        }
        sel_symbol
    }

    fn refresh_results(&mut self, ctx: &Context, all_symbols: &[Instrument]) {
        if self.search.connected() {
            if self.search_query.is_empty()
                || self.search_query.chars().count() >= self.config.min_search_len
            {
                self.search.query(
                    ctx,
                    &self.search_query,
                    self.category.0,
                    self.config.max_results,
                );
                self.filtered_symbols = self.search.results();
            } else {
                self.search.reset();
                self.filtered_symbols.clear();
            }
            self.sel_idx = self
                .sel_idx
                .min(self.filtered_symbols.len().saturating_sub(1));
        } else {
            self.update_filtered_symbols(all_symbols);
        }
    }

    fn update_filtered_symbols(&mut self, all_symbols: &[Instrument]) {
        if !self.search_query.is_empty() && self.search_query.len() < self.config.min_search_len {
            self.filtered_symbols.clear();
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        let mut filtered: Vec<_> = all_symbols
            .iter()
            .filter(|symbol| {
                (self.category.0.is_none() || symbol.symbol_type == self.category.0)
                    && (symbol.symbol.to_lowercase().contains(&query_lower)
                        || symbol
                            .description
                            .as_deref()
                            .unwrap_or(&symbol.symbol)
                            .to_lowercase()
                            .contains(&query_lower))
            })
            .collect();

        if self.search_query.is_empty() {
            filtered.sort_by_key(|symbol| {
                self.recent_symbols
                    .iter()
                    .position(|name| name == &symbol.symbol)
                    .unwrap_or(usize::MAX)
            });
        }
        self.filtered_symbols = filtered
            .into_iter()
            .take(self.config.max_results)
            .cloned()
            .collect();
        self.sel_idx = self
            .sel_idx
            .min(self.filtered_symbols.len().saturating_sub(1));
    }

    fn render_symbol_item(&self, ui: &mut Ui, symbol: &Instrument, is_sel: bool) -> Response {
        let theme = ui.active_theme();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, 4.0);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::ListBoxOption,
                response.enabled(),
                &symbol.symbol,
            )
        });
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(is_sel));
        if is_sel || response.hovered() {
            ui.painter().rect_filled(rect, 4.0, theme.ui.btn_bg_hover);
        }
        let mut right = rect.right() - 8.0;
        // The exchange mark column exists once any row has one, so the other columns line up.
        if self
            .filtered_symbols
            .iter()
            .any(|symbol| symbol.exchange_logo.is_some())
        {
            let mark = egui::Rect::from_min_max(
                Pos2::new(right - 20.0, rect.center().y - 10.0),
                Pos2::new(right, rect.center().y + 10.0),
            );
            paint_image(ui, symbol.exchange_logo.as_deref(), mark);
            right = mark.left() - 12.0;
        }
        let kind = symbol
            .symbol_type
            .map(|kind| kind.to_string())
            .unwrap_or_default();
        let exchange = if self.config.show_exchange {
            symbol.exchange.as_deref().unwrap_or_default()
        } else {
            ""
        };
        let column_width = |kind_column: bool| {
            self.filtered_symbols
                .iter()
                .map(|symbol| {
                    let text = if kind_column {
                        symbol
                            .symbol_type
                            .map(|kind| kind.to_string())
                            .unwrap_or_default()
                    } else if self.config.show_exchange {
                        symbol.exchange.clone().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    ui.painter()
                        .layout_no_wrap(
                            l10n::text(ui.ctx(), text),
                            egui::FontId::proportional(11.0),
                            theme.ui.text_secondary,
                        )
                        .size()
                        .x
                })
                .fold(0.0_f32, f32::max)
                .min(rect.width() * 0.2)
        };
        let kind_width = column_width(true);
        let exchange_width = column_width(false);
        let exchange_right = right - kind_width - 16.0;
        for (text, edge, width) in [
            (kind.as_str(), right, kind_width),
            (exchange, exchange_right, exchange_width),
        ] {
            let galley = egui::WidgetText::from(
                RichText::new(l10n::text(ui.ctx(), text))
                    .size(11.0)
                    .color(theme.ui.text_secondary),
            )
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                width,
                egui::TextStyle::Body,
            );
            ui.painter().galley(
                Pos2::new(
                    edge - galley.size().x,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                theme.ui.text_secondary,
            );
        }
        let mark = egui::Rect::from_center_size(
            Pos2::new(rect.left() + 20.0, rect.center().y),
            Vec2::splat(24.0),
        );
        paint_symbol_mark(ui, symbol, mark);
        let left = mark.right() + 12.0;
        let width = (exchange_right - exchange_width - 16.0 - left).max(0.0);
        let name = egui::WidgetText::from(
            RichText::new(&symbol.symbol)
                .size(13.0)
                .strong()
                .color(theme.ui.text),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            width,
            egui::TextStyle::Body,
        );
        let top = if self.config.show_description {
            rect.top() + 1.0
        } else {
            rect.center().y - name.size().y / 2.0
        };
        ui.painter()
            .galley(Pos2::new(left, top), name, theme.ui.text);
        if self.config.show_description {
            let description = egui::WidgetText::from(
                RichText::new(symbol.description.as_deref().unwrap_or(&symbol.symbol))
                    .size(11.0)
                    .color(theme.ui.text_secondary),
            )
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                width,
                egui::TextStyle::Body,
            );
            ui.painter().galley(
                Pos2::new(left, rect.bottom() - description.size().y - 1.0),
                description,
                theme.ui.text_secondary,
            );
        }

        response
    }
}

/// Corner radius of the symbol search and comparison dialogs.
const SEARCH_RADIUS: f32 = 20.0;

/// The backdrop of the search dialogs: a faint veil, #F1F1F1 over white, on light themes, and the
/// theme's modal overlay on dark ones.
fn search_backdrop(ctx: &Context) -> Color32 {
    let theme = ctx.active_theme();
    if theme.dark {
        theme.ui.modal_overlay
    } else {
        Color32::from_black_alpha(14)
    }
}

/// Paints the image at `uri` into `rect` as a circle. Returns `false`, painting nothing, without
/// a URI or while the image is loading or unreadable.
fn paint_image(ui: &Ui, uri: Option<&str>, rect: egui::Rect) -> bool {
    let Some(uri) = uri else {
        return false;
    };
    let pixels = (rect.size() * ui.ctx().pixels_per_point()).round();
    let size = egui::SizeHint::Size {
        width: pixels.x as u32,
        height: pixels.y as u32,
        maintain_aspect_ratio: true,
    };
    match ui
        .ctx()
        .try_load_texture(uri, egui::TextureOptions::LINEAR, size)
    {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            egui::Image::from_texture(texture)
                .corner_radius(rect.width() / 2.0)
                .paint_at(ui, rect);
            true
        }
        _ => false,
    }
}

/// Paints the instrument's logo in `rect`, or its initial in a neutral circle without one.
fn paint_symbol_mark(ui: &Ui, symbol: &Instrument, rect: egui::Rect) {
    if paint_image(ui, symbol.logo.as_deref(), rect) {
        return;
    }
    let theme = ui.active_theme();
    ui.painter()
        .circle_filled(rect.center(), rect.width() / 2.0, theme.ui.btn_bg_hover);
    let initial: String = symbol
        .symbol
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .collect();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(rect.height() * 0.5),
        theme.ui.text_secondary,
    );
}

// =============================================================================
// Compare Symbols Dialog
// =============================================================================

/// Instrument information for comparison
#[derive(Debug, Clone)]
pub struct CompareSymbol {
    /// Instrument ticker
    pub symbol: String,
    /// Full name/desc
    pub name: String,
    /// Exchange name
    pub exchange: String,
    /// Color for this symbol on chart
    pub color: Color32,
    /// Whether this symbol is visible on chart
    pub visible: bool,
}

impl CompareSymbol {
    /// Create a new compare symbol
    pub fn new(
        symbol: impl Into<String>,
        name: impl Into<String>,
        exchange: impl Into<String>,
        color: Color32,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            name: name.into(),
            exchange: exchange.into(),
            color,
            visible: true,
        }
    }
}

/// Configuration for compare symbols dialog
#[derive(Debug, Clone)]
pub struct CompareSymbolsConfig {
    /// Dialog width
    pub width: f32,
    /// Dialog height
    pub height: f32,
    /// Background color
    pub bg_color: Color32,
    /// Text color
    pub text_color: Color32,
    /// Muted/secondary text color
    pub muted_color: Color32,
    /// Hover background color
    pub hover_color: Color32,
}

impl Default for CompareSymbolsConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            width: 840.0,
            height: 560.0,
            bg_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
        }
    }
}

impl CompareSymbolsConfig {
    /// The configured colors, each `TRANSPARENT` one taken from `visuals`.
    fn resolved(&self, visuals: &egui::Visuals) -> Self {
        let or = |color: Color32, theme: Color32| {
            if color == Color32::TRANSPARENT {
                theme
            } else {
                color
            }
        };
        Self {
            bg_color: or(self.bg_color, visuals.panel_fill),
            text_color: or(self.text_color, visuals.text_color()),
            muted_color: or(self.muted_color, visuals.weak_text_color()),
            hover_color: or(self.hover_color, visuals.widgets.hovered.weak_bg_fill),
            ..self.clone()
        }
    }
}

/// Action from compare symbols dialog
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompareAction {
    /// No action
    None,
    /// Add symbol to comparison
    AddSymbol(String),
    /// Remove symbol from comparison
    RemoveSymbol(String),
    /// Toggle symbol visibility
    ToggleVisibility(String),
    /// Dialog closed
    Close,
}

/// Compare symbols dialog
///
/// Allows comparing multiple symbols on the same chart with:
/// - Search for symbols to add
/// - Suggestions from the symbols the host passes in, with an empty state when there are none
/// - Color-coded symbols
/// - Visibility toggles
///
/// # Example
///
/// ```no_run
/// use egui_charts::model::Instrument;
/// use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbolsDialog};
///
/// # fn update(ctx: &egui::Context) {
/// let mut compare = CompareSymbolsDialog::new();
/// compare.open();
///
/// let symbols = vec![Instrument::new("ETHUSDT").with_description("Ethereum / Tether")];
/// match compare.show(ctx, &symbols) {
///     CompareAction::AddSymbol(sym) => {
///         // Add symbol to chart overlay
///     }
///     CompareAction::RemoveSymbol(sym) => {
///         // Remove symbol from chart
///     }
///     _ => {}
/// }
/// # }
/// ```
pub struct CompareSymbolsDialog {
    /// Is dialog open
    pub is_open: bool,
    /// Search query
    search_query: String,
    /// Current comparison series on the host's chart. Update after accepting an intent,
    /// including changes made outside this dialog.
    pub compared_symbols: Vec<CompareSymbol>,
    /// Configuration
    config: CompareSymbolsConfig,
    search: Search,
}

impl Default for CompareSymbolsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl CompareSymbolsDialog {
    /// Create a new compare symbols dialog
    pub fn new() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            compared_symbols: Vec::new(),
            config: CompareSymbolsConfig::default(),
            search: Search::default(),
        }
    }

    /// Connects the same host service used by symbol search.
    pub fn set_search_service(&mut self, service: Option<Arc<dyn SymbolSearch>>) {
        self.search.set_service(service);
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: CompareSymbolsConfig) -> Self {
        self.config = config;
        self
    }

    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
        self.search_query.clear();
        self.search.reset();
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Show the dialog, suggesting `symbols`: the symbols the host's data source offers
    pub fn show(&mut self, ctx: &egui::Context, symbols: &[Instrument]) -> CompareAction {
        let mut action = CompareAction::None;
        self.search.poll(ctx);

        if !self.is_open {
            return action;
        }

        let mut is_open = self.is_open;
        let config = self.config.resolved(&ctx.global_style().visuals);
        Modal::new("compare_symbols", "")
            .width(config.width)
            .height(config.height)
            .fill(config.bg_color)
            .rounding(SEARCH_RADIUS)
            .backdrop(search_backdrop(ctx))
            .show(ctx, &mut is_open, |ui| {
                let translated_label = l10n::text(ui.ctx(), "Compare symbols");
                ui.response().widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Dialog, true, &translated_label)
                });
                ui.ctx()
                    .accesskit_node_builder(ui.response().id, |node| node.set_modal());
                action = self.draw_content(ui, symbols, &config);
            });

        self.is_open = is_open && action != CompareAction::Close;
        if !self.is_open {
            action = CompareAction::Close;
        }

        action
    }

    fn draw_content(
        &mut self,
        ui: &mut Ui,
        symbols: &[Instrument],
        config: &CompareSymbolsConfig,
    ) -> CompareAction {
        let mut action = CompareAction::None;

        ui.horizontal(|ui| {
            ui.add(icons::QUICK_SEARCH.as_image_tinted(Vec2::splat(24.0), config.muted_color));
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(RichText::new(l10n::text(ui.ctx(), "Search symbols")).size(16.0))
                    .font(egui::FontId::proportional(16.0))
                    .frame(egui::Frame::NONE)
                    .desired_width((ui.available_width() - 40.0).max(0.0)),
            )
            .accessible_name(l10n::text(ui.ctx(), "Search symbols"));
            if IconButton::new(&icons::CLOSE)
                .tooltip("Close")
                .show(ui)
                .clicked()
            {
                action = CompareAction::Close;
            }
        });
        ui.add_space(24.0);

        // Currently compared symbols
        if !self.compared_symbols.is_empty() {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Currently comparing:"))
                    .color(config.muted_color),
            );
            ui.space_sm();

            egui::ScrollArea::vertical()
                .id_salt("current_comparisons")
                .max_height(ui.available_height() * 0.4)
                .show(ui, |ui| {
                    for symbol in &self.compared_symbols {
                        ui.horizontal(|ui| {
                            // Color indicator
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::splat(DESIGN_TOKENS.sizing.icon_xs),
                                Sense::hover(),
                            );
                            ui.painter().circle_filled(rect.center(), 5.0, symbol.color);

                            ui.label(l10n::text(ui.ctx(), &symbol.symbol));
                            ui.label(
                                RichText::new(&symbol.name)
                                    .color(config.muted_color)
                                    .size(ui.active_theme().text.small),
                            );

                            ui.right_aligned(|ui| {
                                if ui.small_button(l10n::text(ui.ctx(), "X")).clicked() {
                                    action = CompareAction::RemoveSymbol(symbol.symbol.clone());
                                }

                                let eye_label = if symbol.visible { "Hide" } else { "Show" };
                                if ui.small_button(l10n::text(ui.ctx(), eye_label)).clicked() {
                                    action = CompareAction::ToggleVisibility(symbol.symbol.clone());
                                }
                            });
                        });
                    }
                });
            ui.separator_with_margin(ui.active_theme().spacing.lg);
        }

        self.search.query(ui.ctx(), &self.search_query, None, 20);
        let found = self.search.results();
        let symbols = if self.search.connected() {
            found.as_slice()
        } else {
            symbols
        };
        self.search.status(ui);
        let mut next_page = false;
        // Suggestions
        ui.label(RichText::new(l10n::text(ui.ctx(), "Symbols")).color(config.muted_color));
        ui.space_lg();

        if symbols.is_empty() {
            EmptyState::new("No symbols to compare").show(ui);
        }

        egui::ScrollArea::vertical()
            .id_salt("comparison_suggestions")
            .max_height(ui.available_height())
            .show(ui, |ui| {
                let query = self.search_query.to_lowercase();
                for symbol in symbols.iter().filter(|symbol| {
                    self.search.connected()
                        || symbol.symbol.to_lowercase().contains(&query)
                        || symbol
                            .description
                            .as_deref()
                            .unwrap_or(&symbol.symbol)
                            .to_lowercase()
                            .contains(&query)
                }) {
                    next_page |= ui.clip_rect().contains(ui.cursor().min)
                        && symbols
                            .last()
                            .is_some_and(|last| last.symbol == symbol.symbol);
                    if let Some(a) = self.draw_symbol_item(ui, symbol, config) {
                        action = a;
                    }
                }
            });

        if next_page {
            self.search.next_page();
        }
        action
    }

    fn draw_symbol_item(
        &self,
        ui: &mut Ui,
        symbol: &Instrument,
        config: &CompareSymbolsConfig,
    ) -> Option<CompareAction> {
        let is_compared = self
            .compared_symbols
            .iter()
            .any(|s| s.symbol == symbol.symbol);

        let desired_size = Vec2::new(ui.available_width(), 56.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::ListBoxOption,
                response.enabled() && !is_compared,
                &symbol.symbol,
            )
        });
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(is_compared));
        crate::styles::focus::draw_focus_ring(ui, &response, ui.active_theme().radius.md);

        // Background on hover
        if response.hovered() && !is_compared {
            ui.painter()
                .rect_filled(rect, ui.active_theme().radius.md, config.hover_color);
        }

        // Checkmark if compared
        let indicator_pos = Pos2::new(rect.min.x + 12.0, rect.center().y);
        if is_compared {
            ui.painter()
                .circle_filled(indicator_pos, 5.0, ui.active_theme().ui.success);
            ui.painter().text(
                indicator_pos,
                egui::Align2::CENTER_CENTER,
                l10n::text(ui.ctx(), "v"),
                egui::FontId::proportional(ui.active_theme().text.micro),
                ui.active_theme().ui.panel_bg,
            );
        }

        // Instrument
        ui.painter().text(
            Pos2::new(rect.min.x + 40.0, rect.center().y - 10.0),
            egui::Align2::LEFT_CENTER,
            &symbol.symbol,
            egui::FontId::proportional(16.0),
            config.text_color,
        );

        // Description
        ui.painter().text(
            Pos2::new(rect.min.x + 40.0, rect.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            symbol.description.as_deref().unwrap_or(&symbol.symbol),
            egui::FontId::proportional(12.0),
            config.muted_color,
        );

        if response.clicked() && !is_compared {
            return Some(CompareAction::AddSymbol(symbol.symbol.clone()));
        }

        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_symbols() -> Vec<Instrument> {
        vec![
            Instrument::new("BTCUSDT").with_description("Bitcoin / Tether"),
            Instrument::new("ETHUSDT").with_description("Ethereum / Tether"),
            Instrument::new("AAPL").with_description("Apple Inc."),
            Instrument::new("TSLA").with_description("Tesla Inc."),
        ]
    }

    #[test]
    fn service_results_reach_both_symbol_choosers_without_a_local_catalog() {
        for compare in [false, true] {
            let ctx = Context::default();
            let source: Arc<dyn SymbolSearch> = Arc::new(vec![
                crate::model::Instrument::new("HOST").with_description("Host instrument"),
            ]);
            let mut search = SymbolSearchDialog::new();
            search.set_search_service(Some(source.clone()));
            search.open_with_query("host".into());
            let mut comparison = CompareSymbolsDialog::new();
            comparison.set_search_service(Some(source));
            comparison.open();
            comparison.search_query = "host".into();
            for step in 0..3 {
                let output = ctx.run_ui(Default::default(), |ui| {
                    if compare {
                        let _ = comparison.show(ui.ctx(), &[]);
                    } else {
                        let _ = search.show(ui.ctx(), &[]);
                    }
                });
                let painted = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "HOST"));
                output.drop_without_applying_deltas();
                if step > 0 {
                    assert!(painted, "service result absent from chooser {compare}");
                }
            }
        }
    }
    #[test]
    fn test_symbol_search_creation() {
        let dialog = SymbolSearchDialog::new();
        assert!(!dialog.is_open());
        assert_eq!(dialog.search_query, "");
    }

    #[test]
    fn test_symbol_search_open_close() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for keyboard in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut dialog = SymbolSearchDialog::new();
                assert!(!dialog.is_open());
                dialog.open();
                let mut frame = |events| {
                    let mut picked = None;
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1440.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            picked = dialog.show(ui.ctx(), &create_test_symbols());
                        },
                    )
                    .drop_without_applying_deltas();
                    picked
                };
                frame(vec![]);
                frame(vec![]);
                if keyboard {
                    for key in [egui::Key::Tab, egui::Key::Enter] {
                        assert!(
                            frame(vec![egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            }])
                            .is_none(),
                            "closing must not pick a symbol"
                        );
                    }
                } else {
                    let pos = egui::pos2(10.0, 850.0);
                    frame(vec![egui::Event::PointerMoved(pos)]);
                    for pressed in [true, false] {
                        frame(vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }]);
                    }
                }
                assert!(
                    !dialog.is_open(),
                    "backdrop and keyboard dismissal close symbol search"
                );
                dialog.open();
                assert!(dialog.is_open());
                dialog.close();
                assert!(!dialog.is_open());
            }
        }
    }

    #[test]
    fn keyboard_selection_survives_a_stationary_pointer_and_scrolls() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let symbols: Vec<_> = (0..20)
                .map(|i| Instrument::new(format!("S{i}")).with_description("Sample"))
                .collect();
            let mut dialog = SymbolSearchDialog::new();
            ctx.run_ui(Default::default(), |ui| {
                let response = dialog.render_symbol_item(ui, &symbols[0], false);
                assert_eq!(response.rect.height(), 32.0);
            })
            .drop_without_applying_deltas();
            dialog.open();
            let mut frame = |events| {
                let mut picked = None;
                let mut visible = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(600.0, 400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        picked = dialog.show(ui.ctx(), &symbols);
                    },
                );
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        if shape.clip_rect.contains_rect(rect) {
                            visible.push((text.galley.text().to_owned(), rect.center()));
                        }
                    }
                }
                output.drop_without_applying_deltas();
                (picked, visible, dialog.sel_idx)
            };
            frame(vec![]);
            let (_, visible, _) = frame(vec![]);
            let pos = visible.iter().find(|(text, _)| text == "S0").unwrap().1;
            frame(vec![egui::Event::PointerMoved(pos)]);
            let key = |key, pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            };
            for _ in 0..18 {
                frame(vec![key(egui::Key::ArrowDown, true)]);
                frame(vec![key(egui::Key::ArrowDown, false)]);
            }
            for _ in 0..30 {
                frame(vec![]);
            }
            let visible = frame(vec![]);
            assert!(
                visible.1.iter().any(|(text, _)| text == "S18"),
                "{visible:?}"
            );
            let picked = frame(vec![key(egui::Key::Enter, true)]).0.unwrap();
            assert_eq!(picked.symbol, "S18");
            dialog.open();
            ctx.run_ui(
                egui::RawInput {
                    events: vec![key(egui::Key::Escape, true)],
                    ..Default::default()
                },
                |ui| {
                    let _ = dialog.show(ui.ctx(), &symbols);
                },
            )
            .drop_without_applying_deltas();
            assert!(!dialog.is_open());
        }
    }

    #[test]
    fn test_symbol_filtering() {
        let symbols: Vec<_> = create_test_symbols()
            .into_iter()
            .map(|symbol| {
                let kind = if symbol.symbol.ends_with("USDT") {
                    SymbolType::Crypto
                } else {
                    SymbolType::Stock
                };
                symbol.with_exchange("Test").with_symbol_type(kind)
            })
            .collect();
        for (query, category, recents, limit, expected) in [
            ("BTC", None, vec![], 20, vec!["BTCUSDT"]),
            (
                "",
                None,
                vec![],
                20,
                vec!["BTCUSDT", "ETHUSDT", "AAPL", "TSLA"],
            ),
            (
                "",
                None,
                vec!["TSLA", "missing", "ETHUSDT"],
                2,
                vec!["TSLA", "ETHUSDT"],
            ),
            (
                "",
                Some(SymbolType::Stock),
                vec!["ETHUSDT", "TSLA"],
                20,
                vec!["TSLA", "AAPL"],
            ),
            ("bitcoin", Some(SymbolType::Stock), vec![], 20, vec![]),
            ("a", None, vec![], 20, vec![]),
        ] {
            let mut dialog = SymbolSearchDialog::new();
            dialog.config.min_search_len = 2;
            dialog.config.max_results = limit;
            dialog.category = Category(category);
            dialog.set_recent_symbols(recents.into_iter().map(str::to_owned).collect());
            dialog.search_query = query.to_owned();
            dialog.update_filtered_symbols(&symbols);
            assert_eq!(
                dialog
                    .filtered_symbols
                    .iter()
                    .map(|s| s.symbol.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{query:?} / {category:?}"
            );
        }
    }

    #[test]
    fn test_symbol_filtering_by_name() {
        let mut dialog = SymbolSearchDialog::new();
        let symbols = create_test_symbols();

        dialog.search_query = "Apple".to_string();
        dialog.update_filtered_symbols(&symbols);

        assert_eq!(dialog.filtered_symbols.len(), 1);
        assert_eq!(dialog.filtered_symbols[0].symbol, "AAPL");
    }

    #[test]
    fn test_symbol_filtering_case_insensitive() {
        let symbols = create_test_symbols();
        for (query, expected) in [("btc", "BTCUSDT"), ("BTC", "BTCUSDT"), ("apple", "AAPL")] {
            let mut dialog = SymbolSearchDialog::new();
            dialog.search_query = query.to_string();
            dialog.update_filtered_symbols(&symbols);
            assert_eq!(dialog.filtered_symbols.len(), 1);
            assert_eq!(dialog.filtered_symbols[0].symbol, expected);

            let ctx = Context::default();
            let mut compare = CompareSymbolsDialog::new();
            compare.search_query = query.to_string();
            let config = compare.config.resolved(&ctx.global_style().visuals);
            let mut output = ctx.run_ui(Default::default(), |ui| {
                compare.draw_content(ui, &symbols, &config);
            });
            let names: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if symbols
                            .iter()
                            .any(|symbol| symbol.symbol == text.galley.text()) =>
                    {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            assert_eq!(names, [expected], "compare query: {query}");
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn test_max_results_limit() {
        let mut dialog = SymbolSearchDialog::new();
        dialog.config.max_results = 2;

        let symbols = create_test_symbols();
        dialog.search_query = "USD".to_string();
        dialog.update_filtered_symbols(&symbols);

        assert!(dialog.filtered_symbols.len() <= 2);
    }

    #[test]
    fn test_compare_symbol_creation() {
        let sym = CompareSymbol::new("BTCUSD", "Bitcoin", "CRYPTO", Color32::GOLD);
        assert_eq!(sym.symbol, "BTCUSD");
        assert_eq!(sym.name, "Bitcoin");
        assert_eq!(sym.exchange, "CRYPTO");
        assert!(sym.visible);
    }

    #[test]
    fn comparison_actions_wait_for_host_acceptance() {
        let mut dialog = CompareSymbolsDialog::new();
        assert!(!dialog.is_open);

        dialog.open();
        assert!(dialog.is_open);

        dialog.close();
        assert!(!dialog.is_open);
        let symbols = create_test_symbols();
        for (label, expected, populated) in [
            ("AAPL", CompareAction::AddSymbol("AAPL".into()), false),
            ("X", CompareAction::RemoveSymbol("AAPL".into()), true),
            ("Hide", CompareAction::ToggleVisibility("AAPL".into()), true),
        ] {
            let ctx = Context::default();
            let mut dialog = CompareSymbolsDialog::new();
            if populated {
                dialog.compared_symbols.push(CompareSymbol::new(
                    "AAPL",
                    "Apple",
                    "Test",
                    Color32::GOLD,
                ));
            }
            let config = dialog.config.resolved(&ctx.global_style().visuals);
            let mut frame = |events| {
                let mut action = CompareAction::None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        action = dialog.draw_content(ui, &symbols, &config);
                    },
                );
                let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                });
                output.drop_without_applying_deltas();
                (action, pos)
            };
            frame(vec![]);
            let pos = frame(vec![]).1.unwrap();
            frame(vec![egui::Event::PointerMoved(pos)]);
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(vec![button(true)]);
            assert_eq!(frame(vec![button(false)]).0, expected);
            assert_eq!(
                dialog.compared_symbols.len(),
                usize::from(populated),
                "only the host updates chart membership"
            );
            if populated {
                assert!(
                    dialog.compared_symbols[0].visible,
                    "only the host updates chart visibility"
                );
            }
        }
    }
}
