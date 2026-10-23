//! Series context menu - right-click menu when series is selected.
//!
//! Offers an alert, an indicator, the clicked price and the series settings.

use crate::chart::series::SeriesId;
use crate::config::KeyboardAction;
use crate::icons::icons as embedded_icons;
use crate::theme::Theme;
use crate::ui::Control;
use crate::ui::context_menu::{MenuEntry, MenuItem, POINTER_GAP, format_price, show_menu};

use egui::{Context, Id, Pos2};
use std::collections::HashSet;

// ============================================================================
// SERIES CONTEXT MENU ACTIONS
// ============================================================================

/// Actions from the series context menu
#[derive(Debug, Clone, PartialEq)]
pub enum SeriesContextMenuAction {
    /// No action
    None,
    /// Add a price alert
    AddAlert,
    /// Add an indicator
    AddIndicator,
    /// Copy the clicked price to clipboard
    CopyPrice,
    /// Open series settings dialog
    OpenSettings,
}

// ============================================================================
// SERIES CONTEXT MENU STATE
// ============================================================================

/// Series context menu state
pub struct SeriesContextMenu {
    is_open: bool,
    position: Pos2,
    /// Series ID that was right-clicked
    pub series_id: Option<SeriesId>,
    /// Current symbol for dynamic menu items
    pub symbol: String,
    /// Price at click position
    pub click_price: f64,
    /// Last action taken
    pub last_action: SeriesContextMenuAction,
    /// Skip close detection on first frame
    just_opened: bool,
    /// Controls the host hid, set through
    /// [`ChartMenus::set_control_visible`](crate::ui::context_menu::ChartMenus::set_control_visible).
    pub(crate) hidden: HashSet<Control>,
}

impl Default for SeriesContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl SeriesContextMenu {
    /// Create new context menu
    pub fn new() -> Self {
        Self {
            is_open: false,
            position: Pos2::ZERO,
            series_id: None,
            symbol: String::new(),
            click_price: 0.0,
            last_action: SeriesContextMenuAction::None,
            just_opened: false,
            hidden: HashSet::new(),
        }
    }

    /// Open the context menu
    pub fn open(&mut self, pos: Pos2, series_id: SeriesId) {
        self.is_open = true;
        self.position = pos;
        self.series_id = Some(series_id);
        self.last_action = SeriesContextMenuAction::None;
        self.just_opened = true;
    }

    /// Set symbol and price for dynamic menu items
    pub fn set_context(&mut self, symbol: impl Into<String>, price: f64) {
        self.symbol = symbol.into();
        self.click_price = price;
    }

    /// Close the context menu
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Check if menu is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Take and consume the last action
    pub fn take_action(&mut self) -> SeriesContextMenuAction {
        std::mem::replace(&mut self.last_action, SeriesContextMenuAction::None)
    }

    /// Build menu items for the series context menu
    fn build_menu_items(&self) -> Vec<MenuEntry<SeriesContextMenuAction>> {
        let price_str = format_price(self.click_price);
        let symbol = &self.symbol;
        let has_symbol = !symbol.is_empty();
        let of_symbol = |prep: &str| {
            if has_symbol {
                format!(" {prep} {symbol}")
            } else {
                String::new()
            }
        };

        let mut items = Vec::new();
        // An alert needs an instrument.
        if has_symbol && !self.hidden.contains(&Control::Alerts) {
            items.push(MenuEntry::Item(
                MenuItem::new(
                    format!("Add alert on {symbol} at {price_str}…"),
                    SeriesContextMenuAction::AddAlert,
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut(KeyboardAction::CreateAlert.shortcut()),
            ));
        }
        items.extend([
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add indicator{}…", of_symbol("on")),
                    SeriesContextMenuAction::AddIndicator,
                )
                .with_icon(&embedded_icons::INDICATORS),
            ),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(
                format!("Copy price {price_str}"),
                SeriesContextMenuAction::CopyPrice,
            )),
            MenuEntry::Separator,
            MenuEntry::Item(
                MenuItem::new("Settings…", SeriesContextMenuAction::OpenSettings)
                    .with_icon(&embedded_icons::SETTINGS_GEAR),
            ),
        ]);
        items
    }

    /// Show the context menu
    pub fn show(&mut self, ctx: &Context, theme: &Theme) {
        if !self.is_open {
            return;
        }
        let entries = self.build_menu_items();
        let just_opened = std::mem::take(&mut self.just_opened);
        let id = Id::unique("series_context_menu");
        if let Some(action) = show_menu(
            ctx,
            id,
            theme,
            self.position,
            POINTER_GAP,
            &entries,
            just_opened,
        ) {
            self.last_action = action.unwrap_or(SeriesContextMenuAction::None);
            self.close();
        }
    }
}
