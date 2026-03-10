//! Chart context menu
//!
//! Right-click context menu for the chart area. Provides trading actions,
//! clipboard operations, chart management, and navigation options.

use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::Theme;
use crate::theme::components::ContextMenuStyle;

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Context, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, UiBuilder, Vec2};

// ============================================================================
// CONTEXT MENU ACTIONS
// ============================================================================

/// Action triggered by selecting a context menu item
#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuAction {
    /// No action
    None,
    /// Copy price value to clipboard
    CopyPrice(f64),
    /// Paste from clipboard
    Paste,
    /// Add a price alert on a symbol
    AddAlert { symbol: String, price: f64 },
    /// Place a sell limit order
    SellLimit {
        symbol: String,
        price: f64,
        quantity: u32,
    },
    /// Place a buy stop order
    BuyStop {
        symbol: String,
        price: f64,
        quantity: u32,
    },
    /// Add a generic order
    AddOrder { symbol: String, price: f64 },
    /// Lock the vertical cursor line at a time position
    LockVerticalCursor,
    /// Switch to table view
    TableView,
    /// Open the object tree panel
    ObjectTree,
    /// Remove a specific indicator by index
    RemoveIndicator(usize),
    /// Remove all indicators from the chart
    RemoveAllIndicators,
    /// Open chart settings dialog
    OpenSettings,
    /// Open series settings dialog
    OpenSeriesSettings,
    /// Export chart data to CSV
    ExportCsv,
    /// Navigate to a specific date
    GoToDate,
    /// Reset the chart view to defaults
    ResetChart,
    /// Open symbol change dialog
    ChangeSymbol,
    /// Open interval/timeframe change dialog
    ChangeInterval,
    /// Show symbol information
    SymbolInfo,
    /// Open indicator insertion dialog
    InsertIndicator,
    /// Compare or overlay another symbol
    CompareOrAddSymbol,
    /// Open drawing tools panel
    DrawingTools,
    /// Toggle mark visibility on bars
    HideMarksOnBars,
    /// Open price scale settings
    PriceScale,
    /// Open time scale settings
    TimeScale,
}

// ============================================================================
// MENU ITEM DEFINITION
// ============================================================================

/// Represents a single menu item with all its properties
#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub icon: Option<&'static Icon>,
    pub action: ContextMenuAction,
    pub enabled: bool,
    pub is_submenu: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: ContextMenuAction) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            icon: None,
            action,
            enabled: true,
            is_submenu: false,
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    #[allow(dead_code)]
    pub fn submenu(mut self) -> Self {
        self.is_submenu = true;
        self
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
    /// Last action taken
    pub last_action: ContextMenuAction,
    /// Skip close detection on first frame after opening
    just_opened: bool,
}

impl Default for ChartContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartContextMenu {
    pub fn new() -> Self {
        Self {
            is_open: false,
            position: Pos2::ZERO,
            symbol: "AAPL".to_string(),
            click_price: 0.0,
            indicator_cnt: 0,
            last_action: ContextMenuAction::None,
            just_opened: false,
        }
    }

    pub fn open(&mut self, pos: Pos2, price: f64) {
        self.is_open = true;
        self.position = pos;
        self.click_price = price;
        self.last_action = ContextMenuAction::None;
        self.just_opened = true; // Skip close detection on first frame
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

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

    /// Format price with appropriate precision and thousands separators
    fn format_price(price: f64) -> String {
        // Use 2 decimal places for most prices, but handle very small prices
        let formatted = if price.abs() < 0.01 {
            format!("{:.6}", price)
        } else if price.abs() < 1.0 {
            format!("{:.4}", price)
        } else {
            format!("{:.2}", price)
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
                format!("-{}", with_commas)
            } else {
                with_commas
            }
        } else if negative {
            format!("-{}.{}", with_commas, decimal_part)
        } else {
            format!("{}.{}", with_commas, decimal_part)
        }
    }

    /// Build the menu items based on current state
    fn build_menu_items(&self) -> Vec<MenuEntry> {
        let price_str = Self::format_price(self.click_price);
        let symbol = &self.symbol;

        vec![
            // Reset chart view at top
            MenuEntry::Item(
                MenuItem::new("Reset chart view", ContextMenuAction::ResetChart)
                    .with_shortcut("Cmd R"),
            ),
            MenuEntry::Separator,
            // Clipboard section
            MenuEntry::Item(MenuItem::new(
                format!("Copy price {}", price_str),
                ContextMenuAction::CopyPrice(self.click_price),
            )),
            MenuEntry::Item(
                MenuItem::new("Paste", ContextMenuAction::Paste).with_shortcut("Cmd V"),
            ),
            MenuEntry::Separator,
            // Trading section - using exact SVG icons
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add alert on {} at {}…", symbol, price_str),
                    ContextMenuAction::AddAlert {
                        symbol: symbol.clone(),
                        price: self.click_price,
                    },
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut("Alt A"),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Sell 1 {} @ {} limit", symbol, price_str),
                    ContextMenuAction::SellLimit {
                        symbol: symbol.clone(),
                        price: self.click_price,
                        quantity: 1,
                    },
                )
                .with_icon(&embedded_icons::SHORT_POS)
                .with_shortcut("Alt Shift S"),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Buy 1 {} @ {} stop", symbol, price_str),
                    ContextMenuAction::BuyStop {
                        symbol: symbol.clone(),
                        price: self.click_price,
                        quantity: 1,
                    },
                )
                .with_icon(&embedded_icons::LONG_POS),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    format!("Add order on {} at {}…", symbol, price_str),
                    ContextMenuAction::AddOrder {
                        symbol: symbol.clone(),
                        price: self.click_price,
                    },
                )
                .with_icon(&embedded_icons::PLUS)
                .with_shortcut("Shift T"),
            ),
            MenuEntry::Separator,
            // Cursor section
            MenuEntry::Item(MenuItem::new(
                "Lock vertical cursor line by time",
                ContextMenuAction::LockVerticalCursor,
            )),
            MenuEntry::Separator,
            // View section
            MenuEntry::Item(MenuItem::new("Table view", ContextMenuAction::TableView)),
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
            } else {
                MenuItem::new("Remove indicators", ContextMenuAction::RemoveAllIndicators)
                    .disabled()
            }),
            MenuEntry::Separator,
            // Export data
            MenuEntry::Item(MenuItem::new(
                "Export Data to CSV…",
                ContextMenuAction::ExportCsv,
            )),
            MenuEntry::Separator,
            // Series settings (candle colors, price source)
            MenuEntry::Item(MenuItem::new(
                "Series Settings…",
                ContextMenuAction::OpenSeriesSettings,
            )),
            // Settings - using existing Settings icon (hexagon)
            MenuEntry::Item(
                MenuItem::new("Chart Settings…", ContextMenuAction::OpenSettings)
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
        let (menu_width, menu_height) = Self::calculate_menu_dimensions(&menu_items);
        let pos = Self::clamp_position_to_screen(self.position, menu_width, menu_height, ctx);

        let mut should_close = false;
        let mut sel_action = ContextMenuAction::None;

        egui::Area::new(egui::Id::new("chart_ctx_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let menu_rect = Rect::from_min_size(pos, Vec2::new(menu_width, menu_height));
                Self::draw_menu_background(ui, menu_rect, style);
                self.draw_menu_items(
                    ui,
                    &menu_items,
                    style,
                    menu_width,
                    menu_rect,
                    &mut sel_action,
                    &mut should_close,
                );
            });

        should_close |= self.handle_close_events(ctx, pos, menu_width, menu_height);

        if should_close {
            self.last_action = sel_action;
            self.close();
        }
    }

    fn calculate_menu_dimensions(menu_items: &[MenuEntry]) -> (f32, f32) {
        let height: f32 = menu_items
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
        let width = (padding + text_width).clamp(
            DESIGN_TOKENS.sizing.context_menu.min_width,
            DESIGN_TOKENS.sizing.context_menu.max_width,
        );

        (width, height)
    }

    fn clamp_position_to_screen(pos: Pos2, width: f32, height: f32, ctx: &Context) -> Pos2 {
        let screen = ctx.content_rect();
        let mut result = pos;

        if result.x + width > screen.max.x {
            result.x =
                (screen.max.x - width - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                    .max(screen.min.x);
        }
        if result.y + height > screen.max.y {
            result.y =
                (screen.max.y - height - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                    .max(screen.min.y);
        }
        if result.x < screen.min.x {
            result.x = screen.min.x + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }
        if result.y < screen.min.y {
            result.y = screen.min.y + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }
        result
    }

    fn draw_menu_background(ui: &mut Ui, rect: Rect, style: &ContextMenuStyle) {
        // Shadow
        ui.painter().rect_filled(
            rect.translate(Vec2::new(
                DESIGN_TOKENS.layout.menu_shadow_offset_x,
                DESIGN_TOKENS.layout.menu_shadow_offset_y,
            )),
            DESIGN_TOKENS.sizing.context_menu.rounding,
            Color32::from_black_alpha(60),
        );
        // Background
        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.sizing.context_menu.rounding, style.bg);
        ui.painter().rect_stroke(
            rect,
            DESIGN_TOKENS.sizing.context_menu.rounding,
            style.border,
            egui::StrokeKind::Inside,
        );
    }

    fn draw_menu_items(
        &self,
        ui: &mut Ui,
        items: &[MenuEntry],
        style: &ContextMenuStyle,
        width: f32,
        rect: Rect,
        action: &mut ContextMenuAction,
        should_close: &mut bool,
    ) {
        let inner_rect = rect.shrink2(Vec2::new(0.0, DESIGN_TOKENS.sizing.context_menu.padding_v));
        ui.scope_builder(UiBuilder::new().max_rect(inner_rect), |ui| {
            self.render_menu_entries(ui, items, style, width, action, should_close);
        });
    }

    /// Render all menu entries in a vertical layout.
    /// Extracted to reduce nesting depth in draw_menu_items.
    fn render_menu_entries(
        &self,
        ui: &mut Ui,
        items: &[MenuEntry],
        style: &ContextMenuStyle,
        width: f32,
        action: &mut ContextMenuAction,
        should_close: &mut bool,
    ) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for entry in items {
                self.render_single_entry(ui, entry, style, width, action, should_close);
            }
        });
    }

    /// Render a single menu entry (item or separator).
    /// Extracted to reduce nesting depth.
    fn render_single_entry(
        &self,
        ui: &mut Ui,
        entry: &MenuEntry,
        style: &ContextMenuStyle,
        width: f32,
        action: &mut ContextMenuAction,
        should_close: &mut bool,
    ) {
        match entry {
            MenuEntry::Item(item) => {
                let clicked_action = self.draw_menu_item(ui, item, style, width);
                if let Some(a) = clicked_action {
                    *action = a;
                    *should_close = true;
                }
            }
            MenuEntry::Separator => self.draw_separator(ui, style, width),
        }
    }

    fn handle_close_events(&mut self, ctx: &Context, pos: Pos2, width: f32, height: f32) -> bool {
        if self.just_opened {
            self.just_opened = false;
            return false;
        }

        let clicked = ctx.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked());
        if clicked && let Some(click_pos) = ctx.input(|i| i.pointer.interact_pos()) {
            let menu_rect = Rect::from_min_size(pos, Vec2::new(width, height));
            if !menu_rect.contains(click_pos) {
                return true;
            }
        }

        ctx.input(|i| i.key_pressed(egui::Key::Escape))
    }

    /// Draw a single menu item
    fn draw_menu_item(
        &self,
        ui: &mut Ui,
        item: &MenuItem,
        style: &ContextMenuStyle,
        menu_width: f32,
    ) -> Option<ContextMenuAction> {
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

        // Draw background
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
                FontId::proportional(DESIGN_TOKENS.sizing.context_menu.shortcut_font_size),
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

        // Return action if clicked and enabled
        if response.clicked() && item.enabled {
            Some(item.action.clone())
        } else {
            None
        }
    }

    /// Draw menu separator
    fn draw_separator(&self, ui: &mut Ui, style: &ContextMenuStyle, menu_width: f32) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(
                menu_width,
                DESIGN_TOKENS.sizing.context_menu.separator_height,
            ),
            Sense::hover(),
        );

        let line_y = rect.center().y;
        ui.painter().hline(
            (rect.min.x + DESIGN_TOKENS.sizing.context_menu.separator_margin_h)
                ..=(rect.max.x - DESIGN_TOKENS.sizing.context_menu.separator_margin_h),
            line_y,
            Stroke::new(
                DESIGN_TOKENS.sizing.context_menu.separator_thickness,
                style.separator,
            ),
        );
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

/// Menu entry - either an item or separator
enum MenuEntry {
    Item(MenuItem),
    Separator,
}
