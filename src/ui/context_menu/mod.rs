//! Chart context menu
//!
//! Right-click context menu for the chart area. Provides trading actions,
//! clipboard operations, chart management, and navigation options.

mod axis;
mod chart_menus;
mod menu;
mod tpo;

pub mod drawing;
pub mod series;
pub mod study;
pub(super) mod trade_profile;

pub use chart_menus::{ChartContextAction, ChartMenus, HostMenuContext, HostMenuEntry};
pub use menu::MenuItem;
pub(crate) use menu::{MenuEntry, POINTER_GAP, show_menu, trim_separators};

use std::collections::HashSet;

use crate::config::KeyboardAction;
use crate::icons::icons as embedded_icons;
use crate::model::{Capabilities, OrderRequest, OrderSide, OrderType};
use crate::theme::Theme;
use crate::ui::Control;

use egui::{Context, Id, Pos2, Response};

// ============================================================================
// CONTEXT MENU ACTIONS
// ============================================================================

/// Action triggered by selecting a context menu item
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ContextMenuAction {
    /// No action
    None,
    /// Copy price value to clipboard
    CopyPrice(f64),
    /// Add a price alert on a symbol
    AddAlert {
        /// Instrument identifier used by the host.
        symbol: String,
        /// Price under the pointer when the menu opened.
        price: f64,
    },
    /// Add the symbol to the watchlist at this index of the panel's lists, with
    /// [`WatchlistPanel::add_symbol_to`](crate::ui::widget_bar::WatchlistPanel::add_symbol_to).
    AddToWatchlist {
        /// Index into the lists given to [`ChartMenus::set_watchlists`].
        list: usize,
        /// Instrument identifier used by the host.
        symbol: String,
    },
    /// A limit or stop order at the clicked price, for the user to confirm in the order ticket
    /// ([`Trading::open_ticket`](crate::ui::trading::Trading::open_ticket)).
    Order(OrderRequest),
    /// Open the object tree panel
    ObjectTree,
    /// Remove all indicators from the chart
    RemoveAllIndicators,
    /// Open chart settings dialog
    OpenSettings,
    /// Open series settings dialog
    OpenSeriesSettings,
    /// Open the chart data table
    OpenDataTable,
    /// Export chart data to CSV
    ExportCsv,
    /// Reset the chart view to defaults
    ResetChart,
    /// Merge the clicked TPO profile with its previous neighbour. Already applied by [`ChartMenus`].
    MergeTpoPrevious,
    /// Merge the clicked TPO profile with its next neighbour. Already applied by [`ChartMenus`].
    MergeTpoNext,
    /// Restore the sessions of the clicked composite TPO profile. Already applied by [`ChartMenus`].
    SplitTpoProfile,
    /// Draw a horizontal line at this price in the clicked pane, with the line tool's default
    /// style, and select it. Already applied by [`ChartMenus`].
    DrawHorizontalLine(f64),
}

/// Format a price with two decimals (more below 1) and thousands separators.
pub(crate) fn format_price(price: f64) -> String {
    // Use 2 decimal places for most prices, but handle very small prices
    let formatted = if price.abs() < 0.01 {
        format!("{price:.6}")
    } else if price.abs() < 1.0 {
        format!("{price:.4}")
    } else {
        format!("{price:.2}")
    };

    // Add thousands separators
    let parts: Vec<&str> = formatted.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = parts.get(1).unwrap_or(&"");

    let negative = integer_part.starts_with('-');
    let digits: String = integer_part
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();

    let with_commas: String = digits
        .chars()
        .rev()
        .enumerate()
        .fold(String::new(), |mut acc, (i, c)| {
            if i > 0 && i % 3 == 0 {
                acc.push(',');
            }
            acc.push(c);
            acc
        })
        .chars()
        .rev()
        .collect();

    if decimal_part.is_empty() {
        if negative {
            format!("-{with_commas}")
        } else {
            with_commas
        }
    } else if negative {
        format!("-{with_commas}.{decimal_part}")
    } else {
        format!("{with_commas}.{decimal_part}")
    }
}

// ============================================================================
// CONTEXT MENU STATE
// ============================================================================

/// Chart context menu state and renderer
///
/// Manages the lifecycle of a right-click context menu including
/// positioning, item rendering, and action dispatch.
pub struct ChartContextMenu {
    is_open: bool,
    position: Pos2,
    /// Current symbol for dynamic menu items
    pub symbol: String,
    /// Price at click position
    pub click_price: f64,
    /// Number of indicators on chart (for Remove menu)
    pub indicator_cnt: usize,
    /// The last price. Order entries need it: a click above it offers a sell limit and a buy
    /// stop, a click below it a buy limit and a sell stop.
    pub market_price: Option<f64>,
    /// The symbol's tick size, to which order prices are rounded.
    pub tick_size: Option<f64>,
    /// Names of the watchlists the symbol can be added to; without any, the menu has no such
    /// entry.
    pub watchlists: Vec<String>,
    /// The quantity of the order entries.
    pub order_quantity: f64,
    /// The order types the account accepts; entries for the others are left out.
    pub order_types: Capabilities,
    /// Last action taken
    pub last_action: ContextMenuAction,
    /// Skip close detection on first frame after opening
    just_opened: bool,
    /// Controls the host hid, set through [`ChartMenus::set_control_visible`].
    pub(crate) hidden: HashSet<Control>,
    /// Offer the horizontal line row; [`ChartMenus`] sets it from the drawing tool registry.
    pub(crate) horizontal_line: bool,
}

impl Default for ChartContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartContextMenu {
    /// Create a closed chart context menu with default options.
    pub fn new() -> Self {
        Self {
            is_open: false,
            position: Pos2::ZERO,
            symbol: String::new(),
            click_price: 0.0,
            indicator_cnt: 0,
            market_price: None,
            tick_size: None,
            watchlists: Vec::new(),
            order_quantity: 1.0,
            order_types: Capabilities {
                limit: true,
                stop: true,
                ..Capabilities::default()
            },
            last_action: ContextMenuAction::None,
            just_opened: false,
            hidden: HashSet::new(),
            horizontal_line: false,
        }
    }

    /// Open the menu at `pos`, retaining the clicked `price` for its actions.
    pub fn open(&mut self, pos: Pos2, price: f64) {
        self.is_open = true;
        self.position = pos;
        self.click_price = price;
        self.last_action = ContextMenuAction::None;
        self.just_opened = true; // Skip close detection on first frame
    }

    /// Close the menu.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Whether the menu is currently visible.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Set the current symbol for dynamic menu items
    pub fn set_symbol(&mut self, symbol: impl Into<String>) {
        self.symbol = symbol.into();
    }

    /// Set the indicator count
    pub fn set_indicator_cnt(&mut self, count: usize) {
        self.indicator_cnt = count;
    }

    /// Take and consume the last action (resets to None)
    pub fn take_action(&mut self) -> ContextMenuAction {
        std::mem::replace(&mut self.last_action, ContextMenuAction::None)
    }

    /// Build the menu items based on current state
    fn build_menu_items(
        &self,
        shortcuts: &crate::widget::ShortcutMap,
    ) -> Vec<MenuEntry<ContextMenuAction>> {
        let price_str = format_price(self.click_price);
        let symbol = &self.symbol;

        let mut items = vec![
            // Reset chart view at top
            MenuEntry::Item(
                MenuItem::new("Reset chart view", ContextMenuAction::ResetChart)
                    .with_shortcut(shortcuts.shortcut(KeyboardAction::ResetChart)),
            ),
            MenuEntry::Separator,
            // Clipboard section
            MenuEntry::Item(MenuItem::new(
                format!("Copy price {price_str}"),
                ContextMenuAction::CopyPrice(self.click_price),
            )),
            MenuEntry::Separator,
        ];
        // Price items. Alert, watchlist and order rows need an instrument, so they need a symbol.
        let mut trading = Vec::new();
        if !symbol.is_empty() && !self.hidden.contains(&Control::Alerts) {
            trading.push(MenuEntry::Item(
                MenuItem::new(
                    format!("Add alert on {symbol} at {price_str}…"),
                    ContextMenuAction::AddAlert {
                        symbol: symbol.clone(),
                        price: self.click_price,
                    },
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut(shortcuts.shortcut(KeyboardAction::CreateAlert)),
            ));
        }
        if self.horizontal_line && self.click_price.is_finite() {
            trading.push(MenuEntry::Item(
                MenuItem::new(
                    format!("Draw horizontal line at {price_str}"),
                    ContextMenuAction::DrawHorizontalLine(self.click_price),
                )
                .with_icon(&embedded_icons::HORIZONTAL_LINE)
                .with_shortcut(shortcuts.shortcut(KeyboardAction::DrawingTool(
                    crate::config::DrawingToolType::HorizontalLine,
                ))),
            ));
        }
        if !symbol.is_empty() && !self.watchlists.is_empty() {
            trading.push(MenuEntry::Submenu {
                label: format!("Add {symbol} to watchlist"),
                icon: Some(&embedded_icons::WATCHLIST),
                entries: self
                    .watchlists
                    .iter()
                    .enumerate()
                    .map(|(list, name)| {
                        MenuEntry::Item(MenuItem::new(
                            name.clone(),
                            ContextMenuAction::AddToWatchlist {
                                list,
                                symbol: symbol.clone(),
                            },
                        ))
                    })
                    .collect(),
            });
        }
        if !symbol.is_empty()
            && !self.hidden.contains(&Control::Trading)
            && self.order_quantity.is_finite()
            && self.order_quantity > 0.0
            && self.click_price.is_finite()
            && let Some(market) = self.market_price.filter(|price| price.is_finite())
        {
            let price = self
                .tick_size
                .filter(|tick| tick.is_finite() && *tick > 0.0)
                .map_or(self.click_price, |tick| {
                    crate::chart::to_step(self.click_price, tick, f64::round)
                });
            // Above the market a sell waits as a limit order and a buy as a stop order; below it,
            // the other way round. At the market either order can fill immediately.
            let (limit, stop) = if price >= market {
                (OrderSide::Sell, OrderSide::Buy)
            } else {
                (OrderSide::Buy, OrderSide::Sell)
            };
            for (side, order_type, kind) in [
                (limit, OrderType::Limit(price), "limit"),
                (stop, OrderType::Stop(price), "stop"),
            ] {
                if !self.order_types.accepts(&order_type) {
                    continue;
                }
                let request = OrderRequest::new(symbol, side, self.order_quantity, order_type);
                let (verb, icon) = match side {
                    OrderSide::Buy => ("Buy", &embedded_icons::LONG_POS),
                    OrderSide::Sell => ("Sell", &embedded_icons::SHORT_POS),
                };
                trading.push(MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{verb} {} {symbol} @ {} {kind}",
                            self.order_quantity,
                            format_price(price)
                        ),
                        ContextMenuAction::Order(request),
                    )
                    .with_icon(icon),
                ));
            }
        }
        if !trading.is_empty() {
            items.extend(trading);
            items.push(MenuEntry::Separator);
        }
        items.extend([
            // View section
            MenuEntry::Item(MenuItem::new("Object Tree…", ContextMenuAction::ObjectTree)),
            MenuEntry::Separator,
            // Indicator management
            MenuEntry::Item(if self.indicator_cnt > 0 {
                MenuItem::new(
                    format!(
                        "Remove {} indicator{}",
                        self.indicator_cnt,
                        if self.indicator_cnt > 1 { "s" } else { "" }
                    ),
                    ContextMenuAction::RemoveAllIndicators,
                )
                .danger()
            } else {
                MenuItem::new("Remove indicators", ContextMenuAction::RemoveAllIndicators)
                    .disabled()
            }),
            MenuEntry::Separator,
            // Export data
            MenuEntry::Item(MenuItem::new(
                "View Data Table…",
                ContextMenuAction::OpenDataTable,
            )),
            MenuEntry::Item(MenuItem::new(
                "Export Data to CSV…",
                ContextMenuAction::ExportCsv,
            )),
            MenuEntry::Separator,
            // Series settings (candle colors, price source)
            MenuEntry::Item(
                MenuItem::new("Series Settings…", ContextMenuAction::OpenSeriesSettings)
                    .with_icon(&embedded_icons::CANDLESTICK),
            ),
            // Settings - using existing Settings icon (hexagon)
            MenuEntry::Item(
                MenuItem::new("Chart Settings…", ContextMenuAction::OpenSettings)
                    .with_icon(&embedded_icons::SETTINGS_GEAR),
            ),
        ]);
        items.retain(|entry| {
            let control = match entry {
                MenuEntry::Item(item) => match item.action {
                    ContextMenuAction::ResetChart => Control::ResetView,
                    ContextMenuAction::CopyPrice(_) => Control::CopyPrice,
                    ContextMenuAction::ObjectTree => Control::ObjectTree,
                    ContextMenuAction::RemoveAllIndicators => Control::Indicators,
                    ContextMenuAction::OpenDataTable | ContextMenuAction::ExportCsv => {
                        Control::ExportData
                    }
                    ContextMenuAction::OpenSettings | ContextMenuAction::OpenSeriesSettings => {
                        Control::Settings
                    }
                    _ => return true,
                },
                MenuEntry::Submenu { .. } | MenuEntry::Separator => return true,
            };
            !self.hidden.contains(&control)
        });
        trim_separators(&mut items);
        items
    }

    /// Show the context menu
    pub fn show(&mut self, ctx: &Context, theme: &Theme) {
        self.show_with_shortcuts(ctx, theme, &crate::widget::ShortcutMap::default());
    }

    /// Show menu hints from the same bindings used by chart input.
    pub fn show_with_shortcuts(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        shortcuts: &crate::widget::ShortcutMap,
    ) {
        if !self.is_open {
            return;
        }
        let entries = self.build_menu_items(shortcuts);
        if entries.is_empty() {
            self.close();
            return;
        }
        let just_opened = std::mem::take(&mut self.just_opened);
        let id = Id::unique("chart_ctx_menu");
        if let Some(action) = show_menu(
            ctx,
            id,
            theme,
            self.position,
            POINTER_GAP,
            &entries,
            just_opened,
        ) {
            self.last_action = action.unwrap_or(ContextMenuAction::None);
            self.close();
        }
    }

    /// Check if chart should show context menu on right-click
    pub fn check_for_open(response: &Response, menu: &mut ChartContextMenu, price_at_pos: f64) {
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            menu.open(pos, price_at_pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards #218: entries above the last price offer a sell limit and a buy stop; below it, a
    /// buy limit and a sell stop. They use the configured quantity at the click rounded to the
    /// tick, and leave out order types the account does not accept. Fixed sides send a marketable
    /// order from a click below the market.
    #[test]
    fn order_entries_follow_the_market() {
        use OrderSide::{Buy, Sell};
        use OrderType::{Limit, Stop};
        let mut menu = ChartContextMenu::new();
        menu.symbol = "AAPL".into();
        menu.market_price = Some(100.0);
        menu.tick_size = Some(0.25);
        menu.order_quantity = 3.0;
        let stop_only = Capabilities {
            stop: true,
            ..Capabilities::default()
        };
        // (click, accepted types) -> entries
        for (click, types, expected) in [
            (
                101.1,
                menu.order_types,
                vec![(Sell, Limit(101.0)), (Buy, Stop(101.0))],
            ),
            (
                100.0,
                menu.order_types,
                vec![(Sell, Limit(100.0)), (Buy, Stop(100.0))],
            ),
            (
                98.9,
                menu.order_types,
                vec![(Buy, Limit(99.0)), (Sell, Stop(99.0))],
            ),
            (98.9, stop_only, vec![(Sell, Stop(99.0))]),
            (f64::NAN, menu.order_types, vec![]),
        ] {
            menu.click_price = click;
            menu.order_types = types;
            let entries: Vec<_> = menu
                .build_menu_items(&crate::widget::ShortcutMap::default())
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Item(MenuItem {
                        action: ContextMenuAction::Order(order),
                        ..
                    }) => {
                        assert_eq!((order.symbol.as_str(), order.quantity), ("AAPL", 3.0));
                        Some((order.side, order.order_type))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(entries, expected, "click at {click}");
        }
    }

    #[test]
    fn reset_row_shows_the_reset_chart_binding() {
        let ctx = Context::default();
        ctx.set_os(egui::os::OperatingSystem::Nix);
        let shortcut = ChartContextMenu::new()
            .build_menu_items(&crate::widget::ShortcutMap::default())
            .into_iter()
            .find_map(|entry| match entry {
                MenuEntry::Item(item) if item.action == ContextMenuAction::ResetChart => {
                    item.shortcut
                }
                _ => None,
            })
            .expect("the Reset chart view row shows a shortcut");
        assert_eq!(
            KeyboardAction::from_key_press(shortcut.logical_key, &shortcut.modifiers),
            Some(KeyboardAction::ResetChart)
        );
        assert_eq!(ctx.format_shortcut(&shortcut), "Alt+R");
    }
}
