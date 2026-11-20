//! Chart context menu
//!
//! Right-click context menu for the chart area. Provides trading actions,
//! clipboard operations, chart management, and navigation options.

mod axis;
mod chart_menus;
mod menu;
mod rows;
mod tpo;

pub mod drawing;
pub mod series;
pub mod study;
pub(super) mod trade_profile;

pub use chart_menus::{
    ChartContextAction, ChartMenuKind, ChartMenus, HostMenuContext, HostMenuEntry,
};
pub use menu::MenuItem;
pub(crate) use menu::{MenuEntry, POINTER_GAP, show_menu, trim_separators};
pub use rows::{MenuRow, MenuRowId};

use crate::ui::trading::TradingFormatter;
use std::collections::HashSet;
use std::sync::Arc;

use crate::config::KeyboardAction;
use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
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
        /// Price at the pointer when the menu opened, rounded to the instrument's tick,
        /// else the decimals the row uses.
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
    /// A limit, stop or stop-limit order at the clicked price, for the user to confirm in the order
    /// ticket ([`Trading::open_ticket`](crate::ui::trading::Trading::open_ticket)).
    Order(OrderRequest),
    /// Open the object tree panel
    ObjectTree,
    /// Remove all indicators from the chart
    RemoveAllIndicators,
    /// Apply the chart's indicators to the other charts of the layout, with
    /// [`LayoutCopy::Studies`](crate::widget::LayoutCopy::Studies). Offered after
    /// [`ChartMenus::set_layout_copies`].
    ApplyIndicatorsToLayout,
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
    /// Restore a composite TPO profile's sessions, or split a session's profile at the clicked
    /// period. Already applied by [`ChartMenus`].
    SplitTpoProfile,
    /// Merge the clicked execution profile with its previous neighbour. Applied by [`ChartMenus`].
    MergeTradeProfilePrevious,
    /// Merge the clicked execution profile with its next neighbour. Applied by [`ChartMenus`].
    MergeTradeProfileNext,
    /// Split the clicked execution profile at the selected bar. Applied by [`ChartMenus`].
    SplitTradeProfile,
    /// Restore the clicked TPO or execution profile chart's session boundaries by removing all
    /// merges and splits. Already applied by [`ChartMenus`].
    ResetProfileEdits,
    /// Draw a horizontal line at this price in the clicked pane, with the line tool's default
    /// style, and select it. Already applied by [`ChartMenus`].
    DrawHorizontalLine(f64),
}

impl ContextMenuAction {
    fn row_id(&self) -> Option<MenuRowId> {
        Some(match self {
            Self::None => return None,
            Self::CopyPrice(_) => MenuRowId::CopyPrice,
            Self::AddAlert { .. } => MenuRowId::AddAlert,
            Self::AddToWatchlist { list, .. } => MenuRowId::Watchlist(*list),
            Self::Order(order) => match order.order_type {
                OrderType::Limit(_) => MenuRowId::LimitOrder,
                OrderType::Stop(_) => MenuRowId::StopOrder,
                OrderType::StopLimit { .. } => MenuRowId::StopLimitOrder,
                _ => return None,
            },
            Self::ObjectTree => MenuRowId::ObjectTree,
            Self::RemoveAllIndicators => MenuRowId::RemoveIndicators,
            Self::ApplyIndicatorsToLayout => MenuRowId::ApplyIndicatorsToLayout,
            Self::OpenSettings => MenuRowId::ChartSettings,
            Self::OpenSeriesSettings => MenuRowId::SeriesSettings,
            Self::OpenDataTable => MenuRowId::ViewDataTable,
            Self::ExportCsv => MenuRowId::ExportData,
            Self::ResetChart => MenuRowId::ResetChartView,
            Self::MergeTpoPrevious => MenuRowId::TpoMergePrevious,
            Self::MergeTpoNext => MenuRowId::TpoMergeNext,
            Self::SplitTpoProfile => MenuRowId::TpoSplit,
            Self::MergeTradeProfilePrevious => MenuRowId::TradeProfileMergePrevious,
            Self::MergeTradeProfileNext => MenuRowId::TradeProfileMergeNext,
            Self::SplitTradeProfile => MenuRowId::TradeProfileSplit,
            Self::ResetProfileEdits => MenuRowId::ResetProfileEdits,
            Self::DrawHorizontalLine(_) => MenuRowId::DrawHorizontalLine,
        })
    }
}

/// Format a price with two decimals (more below 1) and thousands separators, for a menu shown
/// without its chart.
pub(crate) fn format_price(price: f64) -> String {
    let formatted = format!("{price:.*}", standalone_decimals(price));

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

/// Decimals used by a menu shown without its chart.
fn standalone_decimals(price: f64) -> usize {
    if price.abs() < 0.01 {
        6
    } else if price.abs() < 1.0 {
        4
    } else {
        2
    }
}

// ============================================================================
// CONTEXT MENU STATE
// ============================================================================

/// The host's text for the order rows' quantity.
#[derive(Clone)]
struct OrderFormatter(Arc<dyn TradingFormatter>);

impl std::fmt::Debug for OrderFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TradingFormatter")
    }
}

/// Chart context menu state and renderer
///
/// Manages the lifecycle of a right-click context menu including
/// positioning, item rendering, and action dispatch.
#[derive(Debug)]
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
    /// The symbol's tick size, to which alert and order prices are rounded, and by which a
    /// stop-limit row's limit lies beyond its stop; without it there is no stop-limit row.
    pub tick_size: Option<f64>,
    /// Decimals supplied by [`ChartMenus`] for alert and order prices without a tick size.
    /// `None` uses the decimals of a menu shown without its chart.
    pub(crate) price_decimals: Option<usize>,
    /// Names of the watchlists the symbol can be added to; without any, the menu has no such
    /// entry.
    pub watchlists: Vec<String>,
    /// The quantity of the order entries.
    pub order_quantity: f64,
    /// The order types the account accepts; entries for the others are left out.
    pub order_types: Capabilities,
    /// Writes the order rows' quantity, from [`ChartMenus::set_order_formatter`].
    order_formatter: Option<OrderFormatter>,
    /// Last action taken
    pub last_action: ContextMenuAction,
    /// Skip close detection on first frame after opening
    just_opened: bool,
    /// Controls the host hid, set through [`ChartMenus::set_control_visible`].
    pub(crate) hidden: HashSet<Control>,
    /// Offer the horizontal line row; [`ChartMenus`] sets it from the drawing tool registry.
    pub(crate) horizontal_line: bool,
    /// The clicked price is a price of the chart symbol, so the alert and order rows can act on
    /// it; [`ChartMenus`] clears it for a click in a pane that does not hold the main series.
    symbol_price: bool,
    /// Whether the layout has other charts to copy the studies to.
    pub(crate) layout_copies: bool,
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
            price_decimals: None,
            watchlists: Vec::new(),
            order_quantity: 1.0,
            order_types: Capabilities {
                limit: true,
                stop: true,
                ..Capabilities::default()
            },
            order_formatter: None,
            last_action: ContextMenuAction::None,
            just_opened: false,
            hidden: HashSet::new(),
            horizontal_line: false,
            symbol_price: true,
            layout_copies: false,
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

    /// Build the menu items based on current state, writing prices with `price_text`.
    fn build_menu_items(
        &self,
        ctx: &Context,
        shortcuts: &crate::widget::ShortcutMap,
        price_text: &dyn Fn(f64) -> String,
    ) -> Vec<MenuEntry<ContextMenuAction>> {
        let price_str = price_text(self.click_price);
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
                l10n::format_text(ctx, "Copy price {price}", &[("price", price_str.as_str())]),
                ContextMenuAction::CopyPrice(self.click_price),
            )),
            MenuEntry::Separator,
        ];
        // Trading items need an instrument, so they are offered only with a symbol, but the line row.
        let mut trading = Vec::new();
        let alert_price = self.row_price(self.click_price);
        if self.symbol_price
            && !symbol.is_empty()
            && !self.hidden.contains(&Control::Alerts)
            && alert_price.is_finite()
        {
            trading.push(MenuEntry::Item(
                MenuItem::new(
                    l10n::format_text(
                        ctx,
                        "Add alert on {name} at {price}…",
                        &[
                            ("name", symbol.as_str()),
                            ("price", &price_text(alert_price)),
                        ],
                    ),
                    ContextMenuAction::AddAlert {
                        symbol: symbol.clone(),
                        price: alert_price,
                    },
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut(shortcuts.shortcut(KeyboardAction::CreateAlert)),
            ));
        }
        if self.horizontal_line && self.click_price.is_finite() {
            trading.push(MenuEntry::Item(
                MenuItem::new(
                    l10n::format_text(
                        ctx,
                        "Draw horizontal line at {price}",
                        &[("price", price_str.as_str())],
                    ),
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
                id: Some(MenuRowId::AddToWatchlist),
                label: l10n::format_text(
                    ctx,
                    "Add {symbol} to watchlist",
                    &[("symbol", symbol.as_str())],
                ),
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
        let requests = self.order_requests(self.click_price);
        if !requests.is_empty() {
            let quantity = crate::ui::trading::quantity_text(
                self.order_formatter.as_ref().map(|formatter| &*formatter.0),
                symbol,
                self.order_quantity,
            );
            for request in requests {
                let (price, limit) = match request.order_type {
                    OrderType::Limit(price) | OrderType::Stop(price) => (price, None),
                    OrderType::StopLimit { stop, limit } => (stop, Some(limit)),
                    OrderType::Market => continue,
                };
                let key = match (request.side, request.order_type) {
                    (OrderSide::Buy, OrderType::Limit(_)) => {
                        "Buy {quantity} {symbol} @ {price} limit"
                    }
                    (OrderSide::Buy, OrderType::StopLimit { .. }) => {
                        "Buy {quantity} {symbol} @ {price} stop limit {limit}"
                    }
                    (OrderSide::Buy, _) => "Buy {quantity} {symbol} @ {price} stop",
                    (OrderSide::Sell, OrderType::Limit(_)) => {
                        "Sell {quantity} {symbol} @ {price} limit"
                    }
                    (OrderSide::Sell, OrderType::StopLimit { .. }) => {
                        "Sell {quantity} {symbol} @ {price} stop limit {limit}"
                    }
                    (OrderSide::Sell, _) => "Sell {quantity} {symbol} @ {price} stop",
                };
                let icon = match request.side {
                    OrderSide::Buy => &embedded_icons::LONG_POS,
                    OrderSide::Sell => &embedded_icons::SHORT_POS,
                };
                let price = price_text(price);
                let mut values = vec![
                    ("quantity", quantity.as_str()),
                    ("symbol", symbol.as_str()),
                    ("price", price.as_str()),
                ];
                let limit = limit.map(price_text);
                if let Some(limit) = &limit {
                    values.push(("limit", limit.as_str()));
                }
                trading.push(MenuEntry::Item(
                    MenuItem::new(
                        l10n::format_text(ctx, key, &values),
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
                    if self.indicator_cnt == 1 {
                        l10n::text(ctx, "Remove 1 indicator")
                    } else {
                        l10n::format_text(
                            ctx,
                            "Remove {count} indicators",
                            &[("count", self.indicator_cnt.to_string().as_str())],
                        )
                    },
                    ContextMenuAction::RemoveAllIndicators,
                )
                .danger()
            } else {
                MenuItem::new("Remove indicators", ContextMenuAction::RemoveAllIndicators)
                    .disabled()
            }),
        ]);
        if self.layout_copies {
            let item = MenuItem::new(
                "Apply indicators to all charts",
                ContextMenuAction::ApplyIndicatorsToLayout,
            );
            items.push(MenuEntry::Item(if self.indicator_cnt > 0 {
                item
            } else {
                item.disabled()
            }));
        }
        items.extend([
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
                    ContextMenuAction::RemoveAllIndicators
                    | ContextMenuAction::ApplyIndicatorsToLayout => Control::Indicators,
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

    /// The requests of the order rows at `price`, in menu order: a limit and a stop order, and
    /// a stop-limit order with a known tick size, each only if the account accepts it. `price`
    /// is taken as the alert row takes it: on the tick size, else at the menu's decimals.
    /// Empty where the menu offers no order rows: no symbol, a
    /// price that is not of the chart symbol, trading hidden, no positive finite quantity, or no
    /// finite price or market price.
    fn order_requests(&self, price: f64) -> Vec<OrderRequest> {
        let Some(market) = self.market_price.filter(|price| price.is_finite()) else {
            return Vec::new();
        };
        if !self.symbol_price
            || self.symbol.is_empty()
            || self.hidden.contains(&Control::Trading)
            || !self.order_quantity.is_finite()
            || self.order_quantity <= 0.0
            || !price.is_finite()
        {
            return Vec::new();
        }
        let tick = self
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0);
        let price = self.row_price(price);
        // Above the market a sell waits as a limit order and a buy as a stop order; below it,
        // the other way round. At the market either order can fill immediately.
        let (limit, stop) = if price >= market {
            (OrderSide::Sell, OrderSide::Buy)
        } else {
            (OrderSide::Buy, OrderSide::Sell)
        };
        // The limit lies one tick beyond the stop, in the direction it triggers.
        let stop_limit = tick.map(|tick| {
            let beyond = if stop == OrderSide::Buy {
                price + tick
            } else {
                price - tick
            };
            OrderType::StopLimit {
                stop: price,
                limit: crate::scales::to_step(beyond, tick, f64::round),
            }
        });
        [
            (limit, Some(OrderType::Limit(price))),
            (stop, Some(OrderType::Stop(price))),
            (stop, stop_limit),
        ]
        .into_iter()
        .filter_map(|(side, order_type)| {
            let order_type =
                order_type.filter(|order_type| self.order_types.accepts(order_type))?;
            Some(OrderRequest::new(
                &self.symbol,
                side,
                self.order_quantity,
                order_type,
            ))
        })
        .collect()
    }

    /// Round alert and order prices to the tick, else the decimals the menu uses.
    fn row_price(&self, price: f64) -> f64 {
        crate::scales::snap_price(
            price,
            self.tick_size,
            self.price_decimals
                .unwrap_or_else(|| standalone_decimals(price)),
        )
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
        let entries = self.build_menu_items(ctx, shortcuts, &format_price);
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
        use OrderType::{Limit, Stop, StopLimit};
        let mut menu = ChartContextMenu::new();
        menu.symbol = "AAPL".into();
        menu.market_price = Some(100.0);
        menu.tick_size = Some(0.25);
        menu.order_quantity = 3.0;
        let stop_only = Capabilities {
            stop: true,
            ..Capabilities::default()
        };
        let all = Capabilities {
            stop_limit: true,
            ..menu.order_types
        };
        let stop_limit_only = Capabilities {
            stop_limit: true,
            ..Capabilities::default()
        };
        // (click, tick, accepted types) -> entries
        for (click, tick, types, expected) in [
            (
                101.1,
                Some(0.25),
                menu.order_types,
                vec![(Sell, Limit(101.0)), (Buy, Stop(101.0))],
            ),
            (
                100.0,
                Some(0.25),
                menu.order_types,
                vec![(Sell, Limit(100.0)), (Buy, Stop(100.0))],
            ),
            (
                98.9,
                Some(0.25),
                menu.order_types,
                vec![(Buy, Limit(99.0)), (Sell, Stop(99.0))],
            ),
            (98.9, Some(0.25), stop_only, vec![(Sell, Stop(99.0))]),
            (f64::NAN, Some(0.25), menu.order_types, vec![]),
            (
                101.1,
                Some(0.25),
                all,
                vec![
                    (Sell, Limit(101.0)),
                    (Buy, Stop(101.0)),
                    (
                        Buy,
                        StopLimit {
                            stop: 101.0,
                            limit: 101.25,
                        },
                    ),
                ],
            ),
            (
                100.0,
                Some(0.25),
                all,
                vec![
                    (Sell, Limit(100.0)),
                    (Buy, Stop(100.0)),
                    (
                        Buy,
                        StopLimit {
                            stop: 100.0,
                            limit: 100.25,
                        },
                    ),
                ],
            ),
            (
                98.9,
                Some(0.25),
                all,
                vec![
                    (Buy, Limit(99.0)),
                    (Sell, Stop(99.0)),
                    (
                        Sell,
                        StopLimit {
                            stop: 99.0,
                            limit: 98.75,
                        },
                    ),
                ],
            ),
            (
                98.9,
                Some(0.25),
                stop_limit_only,
                vec![(
                    Sell,
                    StopLimit {
                        stop: 99.0,
                        limit: 98.75,
                    },
                )],
            ),
            (
                98.9137,
                None,
                all,
                vec![(Buy, Limit(98.91)), (Sell, Stop(98.91))],
            ),
        ] {
            menu.click_price = click;
            menu.tick_size = tick;
            menu.order_types = types;
            let rows = menu.build_menu_items(
                &Context::default(),
                &crate::widget::ShortcutMap::default(),
                &format_price,
            );
            assert_eq!(
                rows.iter().any(|entry| matches!(
                    entry,
                    MenuEntry::Item(MenuItem {
                        action: ContextMenuAction::AddAlert { .. },
                        ..
                    })
                )),
                click.is_finite(),
                "alert at {click}"
            );
            let entries: Vec<_> = rows
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

    /// Menu quantities use the host's symbol-specific text, else the account tables' grouping.
    #[test]
    fn order_rows_write_quantities_as_the_trading_panels_do() {
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "AAPL").then(|| format!("{} lots", quantity / 100.0))
            }
        }
        for (formatter, symbol, quantity, text) in [
            (None, "AAPL", 1000.0, "1,000"),
            (
                Some(OrderFormatter(Arc::new(Lots))),
                "AAPL",
                1000.0,
                "10 lots",
            ),
            (
                Some(OrderFormatter(Arc::new(Lots))),
                "MSFT",
                1234.5,
                "1,234.5",
            ),
        ] {
            let mut menu = ChartContextMenu::new();
            menu.symbol = symbol.into();
            menu.market_price = Some(100.0);
            menu.tick_size = Some(1.0);
            menu.click_price = 98.9;
            menu.order_quantity = quantity;
            menu.order_formatter = formatter;
            let labels: Vec<_> = menu
                .build_menu_items(
                    &Context::default(),
                    &crate::widget::ShortcutMap::default(),
                    &format_price,
                )
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Item(item) if matches!(item.action, ContextMenuAction::Order(_)) => {
                        Some(item.label)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels,
                [
                    format!("Buy {text} {symbol} @ 99.00 limit"),
                    format!("Sell {text} {symbol} @ 99.00 stop"),
                ]
            );
        }
    }

    #[test]
    fn reset_row_shows_the_reset_chart_binding() {
        let ctx = Context::default();
        ctx.set_os(egui::os::OperatingSystem::Nix);
        let shortcut = ChartContextMenu::new()
            .build_menu_items(
                &Context::default(),
                &crate::widget::ShortcutMap::default(),
                &format_price,
            )
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
    /// Value rows must reach the lookup as templates, including rounded order prices.
    #[test]
    fn value_rows_reach_the_translator() {
        use crate::model::{Alert, AlertCondition};
        use crate::ui::dialogs::{DrawingContextMenu, SeriesContextMenu};
        use egui::{RawInput, Rect, Shape, Vec2, pos2};

        let mut chart = ChartContextMenu::new();
        chart.symbol = "1301".into();
        chart.watchlists = vec!["7".into()];
        chart.market_price = Some(100.0);
        chart.tick_size = Some(1.0);
        chart.order_quantity = 2.0;
        chart.order_types.stop_limit = true;
        chart.indicator_cnt = 2;
        chart.horizontal_line = true;
        chart.open(pos2(100.0, 100.0), 101.1);
        let mut series = SeriesContextMenu::new();
        series.set_context("1301", 101.1);
        series.open(pos2(100.0, 100.0), crate::chart::series::SeriesId::MAIN);
        let mut drawing = DrawingContextMenu::new();
        drawing.set_context("Trend Line", false, true);
        drawing.alert = Some(Alert::new("1301", AlertCondition::Crossing(100.0)));
        drawing.open(pos2(100.0, 100.0), 1);
        let surfaces: [(&str, &[&str]); 3] = [
            (
                "chart",
                &[
                    "~101.10~",
                    "~1301~101.00~",
                    "~1301~",
                    "~2~1301~101.00~",
                    "~2~1301~101.00~102.00~",
                    "~2~",
                ],
            ),
            ("series", &["~1301~101.10~", "~1301~", "~101.10~"]),
            ("drawing", &["~~~"]),
        ];
        fn collect(shape: &Shape, texts: &mut Vec<String>) {
            match shape {
                Shape::Text(text) => texts.push(text.galley.text().to_owned()),
                Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, texts)),
                _ => {}
            }
        }
        for (name, expected) in surfaces {
            let ctx = Context::default();
            crate::localization::mark_keys(&ctx);
            let mut frame = || {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| match name {
                        "chart" => chart.show(ui.ctx(), &Theme::dark()),
                        "series" => series.show(ui.ctx(), &Theme::dark()),
                        _ => drawing.show(ui.ctx(), &Theme::dark()),
                    },
                )
            };
            frame().drop_without_applying_deltas();
            let output = frame();
            let mut texts = Vec::new();
            for clipped in &output.shapes {
                collect(&clipped.shape, &mut texts);
            }
            output.drop_without_applying_deltas();
            assert!(!texts.is_empty(), "{name}");
            for text in &texts {
                assert!(
                    !text.chars().any(|c| c.is_ascii_alphabetic()),
                    "{name}: {text}"
                );
            }
            for expected in expected {
                assert!(
                    texts.iter().any(|text| text == *expected),
                    "{name}: {expected}: {texts:?}"
                );
            }
            if name == "chart" {
                assert_eq!(
                    texts
                        .iter()
                        .filter(|text| *text == "~2~1301~101.00~")
                        .count(),
                    2
                );
            }
        }
    }
}
