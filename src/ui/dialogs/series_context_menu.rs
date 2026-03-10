//! Series context menu - right-click menu when series is selected.
//!
//! Provides a full menu structure with icons, shortcuts, and submenus.

use crate::chart::series::SeriesId;
use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::Theme;
use crate::theme::components::ContextMenuStyle;

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Context, FontId, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

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
    /// Add a trading order
    AddOrder,
    /// Add an indicator or strategy
    AddIndicator,
    /// Add a financial metric overlay
    AddFinancialMetric,
    /// Show security information
    SecurityInfo,
    /// Show metrics submenu
    Metrics,
    /// Copy the clicked price to clipboard
    CopyPrice,
    /// Paste from clipboard
    Paste,
    /// Switch to table view
    TableView,
    /// Change visual ordering of overlays
    VisualOrder,
    /// Move series to another pane
    MoveTo,
    /// Pin series to a specific price scale
    PinToScale,
    /// Hide the series
    Hide,
    /// Add symbol to a watchlist
    AddToWatchlist,
    /// Add a text note for the symbol
    AddTextNote,
    /// Open series settings dialog
    OpenSettings,
}

// ============================================================================
// MENU ITEM DEFINITION
// ============================================================================

/// Menu item with icon, shortcut, and submenu support
#[derive(Clone)]
struct MenuItem {
    label: String,
    shortcut: Option<String>,
    icon: Option<&'static Icon>,
    action: SeriesContextMenuAction,
    enabled: bool,
    is_submenu: bool,
}

impl MenuItem {
    fn new(label: impl Into<String>, action: SeriesContextMenuAction) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            icon: None,
            action,
            enabled: true,
            is_submenu: false,
        }
    }

    fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn submenu(mut self) -> Self {
        self.is_submenu = true;
        self
    }
}

/// Menu entry - either an item or separator
#[derive(Clone)]
enum MenuEntry {
    Item(MenuItem),
    Separator,
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
            symbol: "AAPL".to_string(),
            click_price: 0.0,
            last_action: SeriesContextMenuAction::None,
            just_opened: false,
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

    /// Format price with appropriate precision
    fn format_price(price: f64) -> String {
        if price.abs() < 0.01 {
            format!("{price:.6}")
        } else if price.abs() < 1.0 {
            format!("{price:.4}")
        } else {
            format!("{price:.2}")
        }
    }

    /// Build menu items for the series context menu
    fn build_menu_items(&self) -> Vec<MenuEntry> {
        let price_str = Self::format_price(self.click_price);
        let symbol = &self.symbol;

        vec![
            // Trading section
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add alert on {symbol} at {price_str}…"),
                    SeriesContextMenuAction::AddAlert,
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut("Alt A"),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add order on {symbol} at {price_str}…"),
                    SeriesContextMenuAction::AddOrder,
                )
                .with_icon(&embedded_icons::PLUS)
                .with_shortcut("Shift T"),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add indicator/strategy on {symbol}…"),
                    SeriesContextMenuAction::AddIndicator,
                )
                .with_icon(&embedded_icons::INDICATORS),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add financial metric for {symbol}…"),
                    SeriesContextMenuAction::AddFinancialMetric,
                )
                .with_icon(&embedded_icons::BAR_CHART),
            ),
            MenuEntry::Separator,
            // Info section
            MenuEntry::Item(
                MenuItem::new("Security info…", SeriesContextMenuAction::SecurityInfo)
                    .with_icon(&embedded_icons::INFO),
            ),
            MenuEntry::Item(
                MenuItem::new("Metrics", SeriesContextMenuAction::Metrics)
                    .with_icon(&embedded_icons::LAYOUT_GRID)
                    .submenu(),
            ),
            MenuEntry::Separator,
            // Edit section
            MenuEntry::Item(MenuItem::new(
                format!("Copy price {price_str}"),
                SeriesContextMenuAction::CopyPrice,
            )),
            MenuEntry::Item(
                MenuItem::new("Paste", SeriesContextMenuAction::Paste)
                    .with_shortcut("Cmd V")
                    .disabled(),
            ),
            MenuEntry::Separator,
            // View section
            MenuEntry::Item(MenuItem::new(
                "Table view",
                SeriesContextMenuAction::TableView,
            )),
            MenuEntry::Separator,
            // Layout section
            MenuEntry::Item(
                MenuItem::new("Visual order", SeriesContextMenuAction::VisualOrder).submenu(),
            ),
            MenuEntry::Item(
                MenuItem::new("Move to", SeriesContextMenuAction::MoveTo)
                    .with_icon(&embedded_icons::MOVE_PANE)
                    .submenu(),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    "Pin to scale (now right)",
                    SeriesContextMenuAction::PinToScale,
                )
                .submenu(),
            ),
            MenuEntry::Item(
                MenuItem::new("Hide", SeriesContextMenuAction::Hide)
                    .with_icon(&embedded_icons::EYE_HIDE),
            ),
            MenuEntry::Separator,
            // Watchlist & notes section
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add {symbol} to watchlist"),
                    SeriesContextMenuAction::AddToWatchlist,
                )
                .with_icon(&embedded_icons::WATCHLIST)
                .submenu(),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add text note for {symbol}"),
                    SeriesContextMenuAction::AddTextNote,
                )
                .with_icon(&embedded_icons::TEXT)
                .with_shortcut("Alt N"),
            ),
            MenuEntry::Separator,
            // Settings (last item)
            MenuEntry::Item(
                MenuItem::new("Settings…", SeriesContextMenuAction::OpenSettings)
                    .with_icon(&embedded_icons::SETTINGS_GEAR),
            ),
        ]
    }

    /// Show the context menu
    pub fn show(&mut self, ctx: &Context, theme: &Theme) {
        if !self.is_open {
            return;
        }

        let style = &theme.components.ctx_menu;
        let menu_items = self.build_menu_items();
        let mut should_close = false;
        let mut sel_action = SeriesContextMenuAction::None;

        // Calculate menu dimensions
        let menu_height: f32 = menu_items
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item(_) => DESIGN_TOKENS.sizing.context_menu.item_height,
                MenuEntry::Separator => DESIGN_TOKENS.sizing.context_menu.separator_height,
            })
            .sum::<f32>()
            + DESIGN_TOKENS.sizing.context_menu.padding_v * 2.0;

        let max_label_len = menu_items
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item(item) => Some(item.label.len()),
                MenuEntry::Separator => None,
            })
            .max()
            .unwrap_or(20);

        let padding = DESIGN_TOKENS.sizing.context_menu.icon_width
            + DESIGN_TOKENS.sizing.context_menu.item_padding_h * 2.0
            + DESIGN_TOKENS.sizing.context_menu.shortcut_width;
        let text_width = (max_label_len as f32) * DESIGN_TOKENS.sizing.context_menu.char_width;
        let menu_width = (padding + text_width).clamp(
            DESIGN_TOKENS.sizing.context_menu.min_width,
            DESIGN_TOKENS.sizing.context_menu.max_width,
        );

        // Position within screen bounds
        let screen_rect = ctx.content_rect();
        let mut pos = self.position;

        if pos.x + menu_width > screen_rect.max.x {
            pos.x = (screen_rect.max.x
                - menu_width
                - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                .max(screen_rect.min.x);
        }
        if pos.y + menu_height > screen_rect.max.y {
            pos.y = (screen_rect.max.y
                - menu_height
                - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                .max(screen_rect.min.y);
        }
        if pos.x < screen_rect.min.x {
            pos.x = screen_rect.min.x + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }
        if pos.y < screen_rect.min.y {
            pos.y = screen_rect.min.y + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }

        egui::Area::new(egui::Id::new("series_context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let menu_rect = Rect::from_min_size(pos, Vec2::new(menu_width, menu_height));

                // Shadow
                ui.painter().rect_filled(
                    menu_rect.translate(Vec2::new(
                        DESIGN_TOKENS.layout.menu_shadow_offset_x,
                        DESIGN_TOKENS.layout.menu_shadow_offset_y,
                    )),
                    DESIGN_TOKENS.sizing.context_menu.rounding,
                    Color32::from_black_alpha(60),
                );

                // Background
                ui.painter().rect_filled(
                    menu_rect,
                    DESIGN_TOKENS.sizing.context_menu.rounding,
                    style.bg,
                );
                ui.painter().rect_stroke(
                    menu_rect,
                    DESIGN_TOKENS.sizing.context_menu.rounding,
                    style.border,
                    egui::StrokeKind::Inside,
                );

                // Menu items
                ui.scope_builder(
                    UiBuilder::new().max_rect(
                        menu_rect
                            .shrink2(Vec2::new(0.0, DESIGN_TOKENS.sizing.context_menu.padding_v)),
                    ),
                    |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;

                            for entry in &menu_items {
                                match entry {
                                    MenuEntry::Item(item) => {
                                        if let Some(action) =
                                            Self::draw_menu_item(ui, item, style, menu_width)
                                        {
                                            sel_action = action;
                                            should_close = true;
                                        }
                                    }
                                    MenuEntry::Separator => {
                                        Self::draw_separator(ui, style, menu_width);
                                    }
                                }
                            }
                        });
                    },
                );
            });

        // Handle close
        if self.just_opened {
            self.just_opened = false;
        } else {
            should_close = should_close || self.check_for_close(ctx, pos, menu_width, menu_height);
        }

        if should_close {
            self.last_action = sel_action;
            self.close();
        }
    }

    fn draw_menu_item(
        ui: &mut Ui,
        item: &MenuItem,
        style: &ContextMenuStyle,
        menu_width: f32,
    ) -> Option<SeriesContextMenuAction> {
        let desired_size = Vec2::new(menu_width, DESIGN_TOKENS.sizing.context_menu.item_height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Determine colors based on state
        let (bg_color, text_color, icon_color) = if !item.enabled {
            (
                Color32::TRANSPARENT,
                style.item_text_disabled,
                style.item_text_disabled,
            )
        } else if response.hovered() {
            (style.item_bg_hover, style.item_text_hover, style.icon_hover)
        } else {
            (style.item_bg, style.item_text, style.icon)
        };

        // Background
        if bg_color != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.none, bg_color);
        }

        // Icon - using new Icon system
        if let Some(icon) = &item.icon {
            let icon_rect = Rect::from_min_size(
                Pos2::new(
                    rect.min.x + DESIGN_TOKENS.sizing.context_menu.item_padding_h,
                    rect.center().y - DESIGN_TOKENS.sizing.context_menu.icon_size / 2.0,
                ),
                Vec2::splat(DESIGN_TOKENS.sizing.context_menu.icon_size),
            );

            // Render icon with tint color
            icon.as_image_tinted(icon_rect.size(), icon_color)
                .paint_at(ui, icon_rect);
        }

        // Label
        let label_x = rect.min.x
            + DESIGN_TOKENS.sizing.context_menu.item_padding_h
            + DESIGN_TOKENS.sizing.context_menu.icon_width;
        ui.painter().text(
            Pos2::new(label_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &item.label,
            FontId::proportional(DESIGN_TOKENS.sizing.context_menu.font_size),
            text_color,
        );

        // Shortcut (right-aligned)
        if let Some(shortcut) = &item.shortcut {
            ui.painter().text(
                Pos2::new(
                    rect.max.x - DESIGN_TOKENS.sizing.context_menu.item_padding_h,
                    rect.center().y,
                ),
                egui::Align2::RIGHT_CENTER,
                shortcut,
                FontId::proportional(DESIGN_TOKENS.sizing.context_menu.font_size - 1.0),
                style.shortcut_text,
            );
        }

        // Submenu arrow
        if item.is_submenu {
            let arrow_x = rect.max.x
                - DESIGN_TOKENS.sizing.context_menu.item_padding_h
                - DESIGN_TOKENS.sizing.context_menu.submenu_arrow_size / 2.0;
            ui.painter().text(
                Pos2::new(arrow_x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                ">",
                FontId::proportional(DESIGN_TOKENS.sizing.context_menu.submenu_arrow_font_size),
                style.submenu_arrow,
            );
        }

        if response.clicked() && item.enabled && !item.is_submenu {
            Some(item.action.clone())
        } else {
            None
        }
    }

    fn draw_separator(ui: &mut Ui, style: &ContextMenuStyle, menu_width: f32) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(
                menu_width,
                DESIGN_TOKENS.sizing.context_menu.separator_height,
            ),
            Sense::hover(),
        );

        ui.painter().hline(
            (rect.min.x + DESIGN_TOKENS.sizing.context_menu.separator_margin_h)
                ..=(rect.max.x - DESIGN_TOKENS.sizing.context_menu.separator_margin_h),
            rect.center().y,
            Stroke::new(
                DESIGN_TOKENS.sizing.context_menu.separator_thickness,
                style.separator,
            ),
        );
    }

    fn check_for_close(&self, ctx: &Context, pos: Pos2, width: f32, height: f32) -> bool {
        let clicked = ctx.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked());

        if clicked && let Some(click_pos) = ctx.input(|i| i.pointer.interact_pos()) {
            let menu_rect = Rect::from_min_size(pos, Vec2::new(width, height));
            if !menu_rect.contains(click_pos) {
                return true;
            }
        }

        ctx.input(|i| i.key_pressed(egui::Key::Escape))
    }
}
