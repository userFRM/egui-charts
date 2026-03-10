//! Symbol search and comparison dialogs
//!
//! Provides symbol search with keyboard navigation
//! and multi-symbol comparison functionality.
//!
//! # Example - Symbol Search
//!
//! ```no_run
//! use egui_open_trading_charts_rs::{SymbolSearchDialog, Symbol};
//!
//! let mut dialog = SymbolSearchDialog::new();
//! let symbols = vec![
//!     Symbol::new("BTCUSDT", "Bitcoin / Tether"),
//!     Symbol::new("ETHUSDT", "Ethereum / Tether"),
//! ];
//!
//! // In your egui update loop
//! if let Some(selected) = dialog.show(ctx, &symbols) {
//!     println!("Selected: {}", selected.name);
//! }
//! ```
//!
//! # Example - Compare Symbols
//!
//! ```no_run
//! use egui_open_trading_charts_rs::{CompareSymbolsDialog, CompareAction};
//!
//! let mut compare = CompareSymbolsDialog::new();
//! compare.open();
//!
//! match compare.show(ctx) {
//!     CompareAction::AddSymbol(sym) => println!("Add: {}", sym),
//!     CompareAction::RemoveSymbol(sym) => println!("Remove: {}", sym),
//!     _ => {}
//! }
//! ```

use crate::ext::UiExt;
use crate::model::Symbol;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Context, Pos2, Response, RichText, Sense, Ui, Vec2, Window};

// =============================================================================
// Symbol Search Dialog
// =============================================================================

/// Configuration for symbol search dialog
///
/// Customize the behavior and appearance of the symbol search.
///
/// # Example
///
/// ```
/// use egui_open_trading_charts_rs::SymbolSearchConfig;
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
    /// Max number of results to display
    pub max_results: usize,
    /// Min characters required before showing results
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

/// Symbol search dialog with keyboard navigation
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
/// use egui_open_trading_charts_rs::{SymbolSearchDialog, SymbolSearchConfig, Symbol};
///
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
/// let symbols = vec![Symbol::new("BTCUSDT", "Bitcoin")];
/// if let Some(symbol) = dialog.show(ctx, &symbols) {
///     // User selected a symbol
///     println!("Switching to: {}", symbol.name);
/// }
/// ```
pub struct SymbolSearchDialog {
    is_open: bool,
    search_query: String,
    sel_idx: usize,
    filtered_symbols: Vec<Symbol>,
    config: SymbolSearchConfig,
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
            search_query: String::new(),
            sel_idx: 0,
            filtered_symbols: Vec::new(),
            config: SymbolSearchConfig::default(),
        }
    }

    /// Sets a custom configuration for the dialog
    ///
    /// # Example
    ///
    /// ```
    /// use egui_open_trading_charts_rs::{SymbolSearchDialog, SymbolSearchConfig};
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

    /// Opens the symbol search dialog
    ///
    /// Clears any previous search state and shows the dialog.
    pub fn open(&mut self) {
        self.is_open = true;
        self.search_query.clear();
        self.sel_idx = 0;
        self.filtered_symbols.clear();
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
    /// * `Some(Symbol)` - The symbol selected by the user
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
    /// # use egui_open_trading_charts_rs::{SymbolSearchDialog, Symbol};
    /// # let ctx = todo!();
    /// # let mut dialog = SymbolSearchDialog::new();
    /// let symbols = vec![
    ///     Symbol::new("BTCUSDT", "Bitcoin / Tether"),
    ///     Symbol::new("ETHUSDT", "Ethereum / Tether"),
    /// ];
    ///
    /// if let Some(symbol) = dialog.show(ctx, &symbols) {
    ///     println!("User selected: {}", symbol.name);
    /// }
    /// ```
    #[must_use = "Symbol selection should be handled"]
    pub fn show(&mut self, ctx: &Context, all_symbols: &[Symbol]) -> Option<Symbol> {
        if !self.is_open {
            return None;
        }

        let mut sel_symbol = None;
        let mut should_close = false;

        egui::Window::new("Symbol Search")
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                // Search input
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let response = ui.text_edit_singleline(&mut self.search_query);

                    // Auto-focus on open
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        should_close = true;
                    }

                    if response.changed() || self.filtered_symbols.is_empty() {
                        self.update_filtered_symbols(all_symbols);
                    }

                    // Request focus on first frame
                    if self.search_query.is_empty() && !response.has_focus() {
                        response.request_focus();
                    }
                });

                ui.separator();

                // Results list
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        if self.filtered_symbols.is_empty() {
                            if self.search_query.len() < self.config.min_search_len {
                                ui.label(format!(
                                    "Type at least {} characters to search",
                                    self.config.min_search_len
                                ));
                            } else {
                                ui.label("No symbols found");
                            }
                        } else {
                            for (idx, symbol) in self.filtered_symbols.iter().enumerate() {
                                let is_sel = idx == self.sel_idx;
                                let response = self.render_symbol_item(ui, symbol, is_sel);

                                if response.clicked() {
                                    sel_symbol = Some(symbol.clone());
                                    should_close = true;
                                }

                                if response.hovered() {
                                    self.sel_idx = idx;
                                }
                            }
                        }
                    });

                // Keyboard navigation
                ui.input(|i| {
                    if i.key_pressed(egui::Key::ArrowDown) && !self.filtered_symbols.is_empty() {
                        self.sel_idx = (self.sel_idx + 1) % self.filtered_symbols.len();
                    }
                    if i.key_pressed(egui::Key::ArrowUp) && !self.filtered_symbols.is_empty() {
                        self.sel_idx = if self.sel_idx == 0 {
                            self.filtered_symbols.len() - 1
                        } else {
                            self.sel_idx - 1
                        };
                    }
                    if i.key_pressed(egui::Key::Enter) && !self.filtered_symbols.is_empty() {
                        sel_symbol = Some(self.filtered_symbols[self.sel_idx].clone());
                        should_close = true;
                    }
                });

                // Footer with hint
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Up/Down Navigate");
                    ui.separator();
                    ui.label("Enter Select");
                    ui.separator();
                    ui.label("Esc Close");
                });
            });

        if should_close {
            self.close();
        }

        sel_symbol
    }

    fn update_filtered_symbols(&mut self, all_symbols: &[Symbol]) {
        if self.search_query.len() < self.config.min_search_len {
            self.filtered_symbols.clear();
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        self.filtered_symbols = all_symbols
            .iter()
            .filter(|symbol| {
                symbol.name.to_lowercase().contains(&query_lower)
                    || symbol.display_name.to_lowercase().contains(&query_lower)
            })
            .take(self.config.max_results)
            .cloned()
            .collect();

        self.sel_idx = 0;
    }

    fn render_symbol_item(&self, ui: &mut Ui, symbol: &Symbol, is_sel: bool) -> Response {
        let frame = if is_sel {
            egui::Frame::default()
                .fill(ui.visuals().selection.bg_fill)
                .inner_margin(DESIGN_TOKENS.spacing.sm)
        } else {
            egui::Frame::default().inner_margin(DESIGN_TOKENS.spacing.sm)
        };

        frame
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                ui.horizontal(|ui| {
                    // Symbol name (bold)
                    ui.label(egui::RichText::new(&symbol.name).strong());
                });

                // Description
                if self.config.show_description && !symbol.display_name.is_empty() {
                    ui.hint_label(&symbol.display_name);
                }
            })
            .response
            .interact(egui::Sense::click())
    }
}

// =============================================================================
// Compare Symbols Dialog
// =============================================================================

/// Symbol information for comparison
#[derive(Debug, Clone)]
pub struct CompareSymbol {
    /// Symbol ticker
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

/// Popular symbols by category for comparison
pub fn popular_symbols() -> Vec<CompareSymbol> {
    vec![
        // Indices
        CompareSymbol::new(
            "SPY",
            "S&P 500 ETF",
            "NYSE",
            DESIGN_TOKENS.semantic.extended.warning,
        ),
        CompareSymbol::new(
            "QQQ",
            "NASDAQ 100 ETF",
            "NASDAQ",
            DESIGN_TOKENS.semantic.extended.info,
        ),
        CompareSymbol::new(
            "DIA",
            "Dow Jones ETF",
            "NYSE",
            DESIGN_TOKENS.semantic.extended.success,
        ),
        CompareSymbol::new(
            "IWM",
            "Russell 2000 ETF",
            "NYSE",
            DESIGN_TOKENS.semantic.extended.purple,
        ),
        CompareSymbol::new(
            "VIX",
            "Volatility Index",
            "CBOE",
            DESIGN_TOKENS.semantic.extended.error,
        ),
        // Crypto
        CompareSymbol::new(
            "BTCUSD",
            "Bitcoin",
            "CRYPTO",
            DESIGN_TOKENS.semantic.extended.favorite_gold,
        ),
        CompareSymbol::new(
            "ETHUSD",
            "Ethereum",
            "CRYPTO",
            DESIGN_TOKENS.semantic.extended.purple,
        ),
        // Commodities
        CompareSymbol::new(
            "GC",
            "Gold Futures",
            "COMEX",
            DESIGN_TOKENS.semantic.extended.favorite_gold,
        ),
        CompareSymbol::new(
            "CL",
            "Crude Oil",
            "NYMEX",
            DESIGN_TOKENS.semantic.extended.brown,
        ),
        // Forex
        CompareSymbol::new(
            "EURUSD",
            "Euro/US Dollar",
            "FOREX",
            DESIGN_TOKENS.semantic.extended.cyan,
        ),
        CompareSymbol::new(
            "USDJPY",
            "Dollar/Yen",
            "FOREX",
            DESIGN_TOKENS.semantic.extended.warning,
        ),
    ]
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
            width: 500.0,
            height: 400.0,
            bg_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
        }
    }
}

/// Action from compare symbols dialog
#[derive(Debug, Clone, PartialEq)]
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
/// - Popular symbol suggestions
/// - Color-coded symbols
/// - Visibility toggles
///
/// # Example
///
/// ```no_run
/// use egui_open_trading_charts_rs::{CompareSymbolsDialog, CompareAction};
///
/// let mut compare = CompareSymbolsDialog::new();
/// compare.open();
///
/// match compare.show(ctx) {
///     CompareAction::AddSymbol(sym) => {
///         // Add symbol to chart overlay
///     }
///     CompareAction::RemoveSymbol(sym) => {
///         // Remove symbol from chart
///     }
///     _ => {}
/// }
/// ```
pub struct CompareSymbolsDialog {
    /// Is dialog open
    pub is_open: bool,
    /// Search query
    search_query: String,
    /// Currently compared symbols
    pub compared_symbols: Vec<CompareSymbol>,
    /// Popular/suggested symbols
    popular: Vec<CompareSymbol>,
    /// Configuration
    config: CompareSymbolsConfig,
    /// Color palette for new symbols
    color_palette: Vec<Color32>,
    /// Next color index
    next_color_idx: usize,
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
            popular: popular_symbols(),
            config: CompareSymbolsConfig::default(),
            color_palette: vec![
                DESIGN_TOKENS.semantic.extended.warning,       // Orange
                DESIGN_TOKENS.semantic.extended.info,          // Blue
                DESIGN_TOKENS.semantic.extended.success,       // Green
                DESIGN_TOKENS.semantic.extended.purple,        // Purple
                DESIGN_TOKENS.semantic.extended.error,         // Red
                DESIGN_TOKENS.semantic.extended.cyan,          // Cyan
                DESIGN_TOKENS.semantic.extended.favorite_gold, // Yellow
                DESIGN_TOKENS.semantic.extended.brown,         // Brown
            ],
            next_color_idx: 0,
        }
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
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Get next color from palette
    fn next_color(&mut self) -> Color32 {
        let color = self.color_palette[self.next_color_idx % self.color_palette.len()];
        self.next_color_idx += 1;
        color
    }

    /// Show the dialog
    pub fn show(&mut self, ctx: &egui::Context) -> CompareAction {
        let mut action = CompareAction::None;

        if !self.is_open {
            return action;
        }

        let mut is_open = self.is_open;

        Window::new("Compare symbol")
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(self.config.width, self.config.height))
            .frame(egui::Frame::window(&ctx.style()).fill(self.config.bg_color))
            .show(ctx, |ui| {
                action = self.draw_content(ui);
            });

        self.is_open = is_open;
        if !is_open {
            action = CompareAction::Close;
        }

        action
    }

    fn draw_content(&mut self, ui: &mut Ui) -> CompareAction {
        let mut action = CompareAction::None;

        // Search bar
        ui.horizontal(|ui| {
            ui.space_lg();
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Symbol, SSE or CCSE")
                    .desired_width(self.config.width - 50.0),
            );
        });

        ui.space_lg();

        // Currently compared symbols
        if !self.compared_symbols.is_empty() {
            ui.label(RichText::new("Currently comparing:").color(self.config.muted_color));
            ui.space_sm();

            let symbols_to_remove: Vec<String> = Vec::new();
            for symbol in &self.compared_symbols {
                ui.horizontal(|ui| {
                    // Color indicator
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::splat(DESIGN_TOKENS.sizing.icon_xs),
                        Sense::hover(),
                    );
                    ui.painter().circle_filled(rect.center(), 5.0, symbol.color);

                    ui.label(&symbol.symbol);
                    ui.label(
                        RichText::new(&symbol.name)
                            .color(self.config.muted_color)
                            .size(typography::SM),
                    );

                    ui.right_aligned(|ui| {
                        if ui.small_button("X").clicked() {
                            action = CompareAction::RemoveSymbol(symbol.symbol.clone());
                        }

                        let eye_label = if symbol.visible { "Hide" } else { "Show" };
                        if ui.small_button(eye_label).clicked() {
                            action = CompareAction::ToggleVisibility(symbol.symbol.clone());
                        }
                    });
                });
            }

            // Remove symbols after iteration
            for symbol in symbols_to_remove {
                self.compared_symbols.retain(|s| s.symbol != symbol);
            }

            ui.separator_with_margin(DESIGN_TOKENS.spacing.lg);
        }

        // Popular symbols
        ui.label(RichText::new("Popular symbols").color(self.config.muted_color));
        ui.space_lg();

        // Two-column layout
        ui.horizontal(|ui| {
            // Left column
            ui.vertical(|ui| {
                ui.set_width(self.config.width / 2.0 - 16.0);

                let half = self.popular.len() / 2;
                for (i, symbol) in self.popular.iter().enumerate() {
                    if i >= half {
                        break;
                    }

                    if let Some(a) = self.draw_symbol_item(ui, symbol) {
                        action = a;
                    }
                }
            });

            // Right column
            ui.vertical(|ui| {
                ui.set_width(self.config.width / 2.0 - 16.0);

                let half = self.popular.len() / 2;
                for (i, symbol) in self.popular.iter().enumerate() {
                    if i < half {
                        continue;
                    }

                    if let Some(a) = self.draw_symbol_item(ui, symbol) {
                        action = a;
                    }
                }
            });
        });

        // Handle add action
        if let CompareAction::AddSymbol(ref sym) = action
            && !self.compared_symbols.iter().any(|s| s.symbol == *sym)
        {
            let color = self.next_color();
            if let Some(popular) = self.popular.iter().find(|s| s.symbol == *sym) {
                self.compared_symbols.push(CompareSymbol {
                    symbol: popular.symbol.clone(),
                    name: popular.name.clone(),
                    exchange: popular.exchange.clone(),
                    color,
                    visible: true,
                });
            } else {
                self.compared_symbols
                    .push(CompareSymbol::new(sym.clone(), "", "", color));
            }
        }

        // Handle remove action
        if let CompareAction::RemoveSymbol(ref sym) = action {
            self.compared_symbols.retain(|s| s.symbol != *sym);
        }

        // Handle toggle visibility
        if let CompareAction::ToggleVisibility(ref sym) = action
            && let Some(s) = self.compared_symbols.iter_mut().find(|s| s.symbol == *sym)
        {
            s.visible = !s.visible;
        }

        action
    }

    fn draw_symbol_item(&self, ui: &mut Ui, symbol: &CompareSymbol) -> Option<CompareAction> {
        let is_compared = self
            .compared_symbols
            .iter()
            .any(|s| s.symbol == symbol.symbol);

        let desired_size = Vec2::new(ui.available_width(), 32.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Background on hover
        if response.hovered() && !is_compared {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, self.config.hover_color);
        }

        // Color indicator
        let indicator_pos = Pos2::new(rect.min.x + 12.0, rect.center().y);
        ui.painter().circle_filled(indicator_pos, 5.0, symbol.color);

        // Checkmark if compared
        if is_compared {
            ui.painter()
                .circle_filled(indicator_pos, 5.0, DESIGN_TOKENS.semantic.extended.success);
            ui.painter().text(
                indicator_pos,
                egui::Align2::CENTER_CENTER,
                "v",
                egui::FontId::proportional(typography::MICRO),
                DESIGN_TOKENS.semantic.ui.text_light,
            );
        }

        // Symbol
        ui.painter().text(
            Pos2::new(rect.min.x + 28.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &symbol.symbol,
            egui::FontId::proportional(typography::MD),
            self.config.text_color,
        );

        // Exchange
        ui.painter().text(
            Pos2::new(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &symbol.exchange,
            egui::FontId::proportional(typography::XS),
            self.config.muted_color,
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

    fn create_test_symbols() -> Vec<Symbol> {
        vec![
            Symbol::new("BTCUSDT", "Bitcoin / Tether"),
            Symbol::new("ETHUSDT", "Ethereum / Tether"),
            Symbol::new("AAPL", "Apple Inc."),
            Symbol::new("TSLA", "Tesla Inc."),
        ]
    }

    #[test]
    fn test_symbol_search_creation() {
        let dialog = SymbolSearchDialog::new();
        assert!(!dialog.is_open());
        assert_eq!(dialog.search_query, "");
    }

    #[test]
    fn test_symbol_search_open_close() {
        let mut dialog = SymbolSearchDialog::new();
        assert!(!dialog.is_open());

        dialog.open();
        assert!(dialog.is_open());

        dialog.close();
        assert!(!dialog.is_open());
    }

    #[test]
    fn test_symbol_filtering() {
        let mut dialog = SymbolSearchDialog::new();
        let symbols = create_test_symbols();

        dialog.search_query = "BTC".to_string();
        dialog.update_filtered_symbols(&symbols);

        assert_eq!(dialog.filtered_symbols.len(), 1);
        assert_eq!(dialog.filtered_symbols[0].name, "BTCUSDT");
    }

    #[test]
    fn test_symbol_filtering_by_name() {
        let mut dialog = SymbolSearchDialog::new();
        let symbols = create_test_symbols();

        dialog.search_query = "Apple".to_string();
        dialog.update_filtered_symbols(&symbols);

        assert_eq!(dialog.filtered_symbols.len(), 1);
        assert_eq!(dialog.filtered_symbols[0].name, "AAPL");
    }

    #[test]
    fn test_symbol_filtering_case_insensitive() {
        let mut dialog = SymbolSearchDialog::new();
        let symbols = create_test_symbols();

        dialog.search_query = "btc".to_string();
        dialog.update_filtered_symbols(&symbols);

        assert_eq!(dialog.filtered_symbols.len(), 1);
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
    fn test_compare_dialog_creation() {
        let dialog = CompareSymbolsDialog::new();
        assert!(!dialog.is_open);
        assert!(dialog.compared_symbols.is_empty());
        assert!(!dialog.popular.is_empty());
    }

    #[test]
    fn test_compare_dialog_open_close() {
        let mut dialog = CompareSymbolsDialog::new();
        assert!(!dialog.is_open);

        dialog.open();
        assert!(dialog.is_open);

        dialog.close();
        assert!(!dialog.is_open);
    }
}
