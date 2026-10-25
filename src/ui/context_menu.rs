//! Chart context menu
//!
//! Right-click context menu for the chart area. Provides trading actions,
//! clipboard operations, chart management, and navigation options.

use super::tpo_context_menu::TpoMenu;
use std::collections::HashSet;
use std::sync::Arc;

use crate::chart::selection::{ChartElementId, SeriesId};
use crate::config::KeyboardAction;
use crate::icons::{Icon, icons as embedded_icons};
use crate::model::{Capabilities, OrderRequest, OrderSide, OrderType};
use crate::theme::{Theme, ThemeExt};
use crate::ui::Control;
use crate::ui::app_state::set_control_visible;
use crate::ui::dialogs::drawing_context_menu::DRAWING_MENU_OFFSET;
use crate::ui::dialogs::study_context_menu::{self, StudyContextMenu};
use crate::ui::dialogs::{DrawingContextMenu, SeriesContextMenu};
use crate::widget::{Chart, RightClickTarget};

use crate::tokens::DESIGN_TOKENS;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, Context, FontId, Galley, Id, KeyboardShortcut, Pos2, Rect, Response, Sense, Stroke,
    Ui, Vec2, WidgetInfo,
};

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
    /// Add a price alert on a symbol
    AddAlert { symbol: String, price: f64 },
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
}

// ============================================================================
// MENU ITEM DEFINITION
// ============================================================================

/// One row of a context menu, generic over the action it resolves to.
///
/// The chart, series and drawing menus all build their rows from this type.
#[derive(Clone)]
pub struct MenuItem<A = ContextMenuAction> {
    pub label: String,
    /// Shown right-aligned, formatted for the platform with [`Context::format_shortcut`].
    pub shortcut: Option<KeyboardShortcut>,
    pub icon: Option<&'static Icon>,
    pub action: A,
    pub enabled: bool,
    /// Destructive rows paint in the theme's danger colours.
    pub danger: bool,
}

impl<A> MenuItem<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            icon: None,
            action,
            enabled: true,
            danger: false,
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<Option<KeyboardShortcut>>) -> Self {
        self.shortcut = shortcut.into();
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

    /// Mark the row as destructive.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// Menu entry - an item, a submenu or a separator
pub(crate) enum MenuEntry<A> {
    Item(MenuItem<A>),
    /// A row that opens its entries beside the menu while it is hovered, or once clicked.
    Submenu {
        label: String,
        icon: Option<&'static Icon>,
        entries: Vec<MenuEntry<A>>,
    },
    Separator,
}

impl<A> MenuEntry<A> {
    /// The same entry with every action passed through `tag`.
    pub(crate) fn map<B>(self, tag: &impl Fn(A) -> B) -> MenuEntry<B> {
        match self {
            Self::Item(item) => MenuEntry::Item(MenuItem {
                label: item.label,
                shortcut: item.shortcut,
                icon: item.icon,
                action: tag(item.action),
                enabled: item.enabled,
                danger: item.danger,
            }),
            Self::Submenu {
                label,
                icon,
                entries,
            } => MenuEntry::Submenu {
                label,
                icon,
                entries: entries.into_iter().map(|entry| entry.map(tag)).collect(),
            },
            Self::Separator => MenuEntry::Separator,
        }
    }
}

/// Remove empty sections after controls have been filtered out.
pub(crate) fn trim_separators<A>(entries: &mut Vec<MenuEntry<A>>) {
    let mut previous_separator = true;
    entries.retain(|entry| {
        let separator = matches!(entry, MenuEntry::Separator);
        let keep = !separator || !previous_separator;
        previous_separator = separator;
        keep
    });
    if matches!(entries.last(), Some(MenuEntry::Separator)) {
        entries.pop();
    }
}

/// Distance between the pointer and the corner a menu opens at.
pub(crate) const POINTER_GAP: f32 = 2.0;

/// Labels wider than this are elided with "…".
const MAX_LABEL_WIDTH: f32 = 300.0;

/// Space between the label column and the shortcut column.
const COLUMN_GAP: f32 = 24.0;

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

/// Show an open menu next to `anchor` and return `Some` on the frame it closes: with the picked
/// action, or with `None` when a click outside or Escape dismissed it.
///
/// The menu opens `gap` px right of and below the anchor and flips to the other side of the
/// anchor on an axis where it would leave the window. A submenu opens beside its row, on the
/// side with room. `just_opened` skips the dismiss check on the frame of the click that opened
/// it.
pub(crate) fn show_menu<A: Clone>(
    ctx: &Context,
    id: Id,
    theme: &Theme,
    anchor: Pos2,
    gap: f32,
    entries: &[MenuEntry<A>],
    just_opened: bool,
) -> Option<Option<A>> {
    if entries.is_empty() {
        return Some(None);
    }
    if just_opened {
        ctx.data_mut(|data| data.remove::<usize>(id.with("submenu")));
    }
    let bounds = ctx.content_rect().shrink(theme.spacing.lg);
    let mut rects = Vec::new();
    let picked = draw_menu(
        ctx,
        id,
        theme,
        bounds,
        &|size| place(anchor, gap, size, bounds),
        entries,
        &mut rects,
    );
    if picked.is_some() {
        return Some(picked);
    }
    let dismissed = !just_opened
        && ctx.input(|i| {
            let clicked = i.pointer.primary_clicked() || i.pointer.secondary_clicked();
            let outside = i
                .pointer
                .interact_pos()
                .is_some_and(|p| !rects.iter().any(|rect| rect.contains(p)));
            (clicked && outside) || i.key_pressed(egui::Key::Escape)
        });
    dismissed.then_some(None)
}

/// Draw one menu at the corner `place` gives for its size, then its open submenu, and return
/// the picked action. Every drawn menu adds its rect to `rects`.
fn draw_menu<A: Clone>(
    ctx: &Context,
    id: Id,
    theme: &Theme,
    bounds: Rect,
    place: &dyn Fn(Vec2) -> Pos2,
    entries: &[MenuEntry<A>],
    rects: &mut Vec<Rect>,
) -> Option<A> {
    let t = &DESIGN_TOKENS.sizing.context_menu;
    let frame = egui::Frame::menu(&ctx.global_style())
        .inner_margin(theme.spacing.sm)
        .fill(theme.ui.panel_bg_floating)
        .stroke(egui::Stroke::new(
            DESIGN_TOKENS.stroke.hairline,
            theme.ui.border,
        ))
        .corner_radius(theme.radius.sm);
    let chrome = frame.total_margin().sum();
    // Icons, labels and shortcuts keep `item_padding_h` from the menu edge.
    let pad = t.item_padding_h - theme.spacing.sm;

    let layout = |text: String, size: f32, max_width: f32| {
        let mut job =
            LayoutJob::simple_singleline(text, FontId::proportional(size), Color32::PLACEHOLDER);
        job.wrap = TextWrapping::truncate_at_width(max_width);
        ctx.fonts_mut(|f| f.layout_job(job))
    };
    // Each item's label and its shortcut.
    let mut rows: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item(item) => Some((
                layout(item.label.clone(), t.font_size, MAX_LABEL_WIDTH),
                item.shortcut
                    .map(|s| layout(ctx.format_shortcut(&s), t.shortcut_font_size, f32::INFINITY)),
            )),
            MenuEntry::Submenu { label, .. } => {
                Some((layout(label.clone(), t.font_size, MAX_LABEL_WIDTH), None))
            }
            MenuEntry::Separator => None,
        })
        .collect();

    let (label_width, mut trailing_width) = rows.iter().flatten().fold(
        (0.0_f32, 0.0_f32),
        |(widest, widest_trailing), (label, trailing)| {
            let trailing = trailing.as_ref().map_or(0.0, |g| g.size().x);
            (widest.max(label.size().x), widest_trailing.max(trailing))
        },
    );
    if entries
        .iter()
        .any(|entry| matches!(entry, MenuEntry::Submenu { .. }))
    {
        trailing_width = trailing_width.max(t.submenu_arrow_size);
    }
    let trailing_column = if trailing_width > 0.0 {
        COLUMN_GAP + trailing_width
    } else {
        0.0
    };
    let row_width =
        (2.0 * pad + t.icon_width + label_width + trailing_column).max(t.min_width - chrome.x);
    let height: f32 = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item(_) | MenuEntry::Submenu { .. } => t.item_height,
            MenuEntry::Separator => t.separator_height,
        })
        .sum();
    let size = (Vec2::new(row_width, height) + chrome).min(bounds.size());
    let row_width = (size.x - chrome.x).max(0.0);
    let label_width = (row_width - 2.0 * pad - t.icon_width - trailing_column).max(0.0);
    for (label, _) in rows.iter_mut().flatten() {
        if label.size().x > label_width {
            *label = layout(label.text().to_owned(), t.font_size, label_width);
        }
    }
    let pos = place(size);
    let menu_rect = Rect::from_min_size(pos, size);
    rects.push(menu_rect);

    let open_key = id.with("submenu");
    let mut open = ctx.data(|data| data.get_temp::<usize>(open_key));
    let mut open_row = None;
    let mut picked = None;
    egui::Area::new(id)
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_min_size(size);
            frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                egui::ScrollArea::vertical()
                    .max_height((size.y - chrome.y).max(0.0))
                    .show(ui, |ui| {
                        for (index, (entry, row)) in entries.iter().zip(rows).enumerate() {
                            match (entry, row) {
                                (MenuEntry::Item(item), Some(row)) => {
                                    let response =
                                        draw_item(ui, theme, item, row, row_width, pad, false);
                                    if response.hovered() {
                                        open = None;
                                    }
                                    if response.clicked() && item.enabled {
                                        picked = Some(item.action.clone());
                                    }
                                }
                                (MenuEntry::Submenu { label, icon, .. }, Some((galley, _))) => {
                                    let mut item = MenuItem::new(label.clone(), ());
                                    item.icon = *icon;
                                    let response = draw_item(
                                        ui,
                                        theme,
                                        &item,
                                        (galley, None),
                                        row_width,
                                        pad,
                                        open == Some(index),
                                    );
                                    if response.hovered() || response.clicked() {
                                        open = Some(index);
                                    }
                                    if open == Some(index) {
                                        open_row = Some(response.rect);
                                    }
                                    draw_chevron(ui, theme, response.rect, pad);
                                }
                                _ => draw_separator(ui, theme, row_width, pad),
                            }
                        }
                    });
            });
        });
    ctx.data_mut(|data| match open {
        Some(index) => {
            data.insert_temp(open_key, index);
        }
        None => data.remove::<usize>(open_key),
    });
    if let (Some(index), Some(row)) = (open, open_row)
        && let Some(MenuEntry::Submenu { entries, .. }) = entries.get(index)
        && !entries.is_empty()
    {
        let margin = frame.total_margin().top;
        let beside = |size: Vec2| {
            let x = if menu_rect.right() + size.x <= bounds.right() {
                menu_rect.right()
            } else {
                (menu_rect.left() - size.x).max(bounds.left())
            };
            let y = (row.top() - margin)
                .min(bounds.bottom() - size.y)
                .max(bounds.top());
            Pos2::new(x, y)
        };
        picked = picked.or(draw_menu(
            ctx,
            id.with(index),
            theme,
            bounds,
            &beside,
            entries,
            rects,
        ));
    }
    picked
}

/// The corner of a menu of `size`, `gap` px from `anchor` and inside `bounds`.
///
/// On each axis the menu goes after the anchor, or before it when it does not fit after; when
/// it fits on neither side it is clamped into `bounds`.
fn place(anchor: Pos2, gap: f32, size: Vec2, bounds: Rect) -> Pos2 {
    let axis = |at: f32, len: f32, min: f32, max: f32| {
        if at + gap + len <= max {
            at + gap
        } else if at - gap - len >= min {
            at - gap - len
        } else {
            (max - len).max(min)
        }
    };
    Pos2::new(
        axis(anchor.x, size.x, bounds.min.x, bounds.max.x),
        axis(anchor.y, size.y, bounds.min.y, bounds.max.y),
    )
}

/// Draw one row, filled as hovered when `highlighted`, and return its response.
fn draw_item<A>(
    ui: &mut Ui,
    theme: &Theme,
    item: &MenuItem<A>,
    (label, trailing): (Arc<Galley>, Option<Arc<Galley>>),
    width: f32,
    pad: f32,
    highlighted: bool,
) -> Response {
    let t = &DESIGN_TOKENS.sizing.context_menu;
    let c = &theme.ui;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, t.item_height),
        if item.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.sm);
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, item.enabled, &item.label));

    let hovered = item.enabled && (response.hovered() || highlighted);
    // (label, icon, trailing text)
    let (text, icon, trailing_color) = if !item.enabled {
        (c.text_disabled, c.text_disabled, c.text_disabled)
    } else if hovered && item.danger {
        (c.danger_text, c.danger_text, c.danger_text)
    } else if item.danger {
        (c.error, c.error, c.text_secondary)
    } else if hovered {
        (c.text, c.text, c.text_secondary)
    } else {
        (c.text, c.text_secondary, c.text_secondary)
    };
    if hovered {
        let fill = if item.danger {
            c.danger_bg
        } else {
            c.btn_bg_hover
        };
        ui.painter().rect_filled(rect, theme.radius.sm, fill);
    }

    let left = rect.min.x + pad;
    if let Some(glyph) = item.icon {
        let icon_rect = Rect::from_min_size(
            Pos2::new(left, rect.center().y - t.icon_size / 2.0),
            Vec2::splat(t.icon_size),
        );
        glyph
            .as_image_tinted(icon_rect.size(), icon)
            .paint_at(ui, icon_rect);
    }
    let label_pos = Pos2::new(left + t.icon_width, rect.center().y - label.size().y / 2.0);
    ui.painter().galley(label_pos, label, text);
    if let Some(trailing) = trailing {
        let pos = Pos2::new(
            rect.max.x - pad - trailing.size().x,
            rect.center().y - trailing.size().y / 2.0,
        );
        ui.painter().galley(pos, trailing, trailing_color);
    }

    response
}

/// The arrow at the end of a submenu row.
fn draw_chevron(ui: &Ui, theme: &Theme, row: Rect, pad: f32) {
    let size = DESIGN_TOKENS.sizing.context_menu.submenu_arrow_size;
    let tip = Pos2::new(row.max.x - pad, row.center().y);
    ui.painter().add(egui::Shape::line(
        vec![
            tip + Vec2::new(-size / 2.0, -size / 2.0),
            tip,
            tip + Vec2::new(-size / 2.0, size / 2.0),
        ],
        Stroke::new(1.5, theme.ui.text_secondary),
    ));
}

fn draw_separator(ui: &mut Ui, theme: &Theme, width: f32, pad: f32) {
    let t = &DESIGN_TOKENS.sizing.context_menu;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, t.separator_height), Sense::hover());
    ui.painter().hline(
        rect.x_range().shrink(pad),
        rect.center().y,
        Stroke::new(t.separator_thickness, theme.ui.border),
    );
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
            symbol: String::new(),
            click_price: 0.0,
            indicator_cnt: 0,
            market_price: None,
            tick_size: None,
            order_quantity: 1.0,
            order_types: Capabilities {
                limit: true,
                stop: true,
                ..Capabilities::default()
            },
            last_action: ContextMenuAction::None,
            just_opened: false,
            hidden: HashSet::new(),
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
        // Trading items need an instrument, so they are offered only with a symbol.
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
                    ContextMenuAction::ExportCsv => Control::ExportData,
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

/// The original click retained while a host menu action is being chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostMenuContext {
    /// Background, series, study or drawing that was clicked, including its screen position.
    pub target: RightClickTarget,
    /// UTC time of the nearest displayed bar or element; `None` outside loaded data.
    pub time: Option<chrono::DateTime<chrono::Utc>>,
    /// Value at the click, using the clicked pane's scale for drawings.
    pub price: f64,
}

/// A host row appended to the chart, series and drawing menus in registration order.
///
/// Use [`Self::when`] to restrict a row to particular click targets. Host ids must be unique
/// within one [`ChartMenus`]. The host owns any services invoked by the returned action.
pub struct HostMenuEntry {
    /// Stable id returned in [`ChartContextAction::Host`].
    pub id: String,
    /// Text painted with the built-in menu style.
    pub label: String,
    /// Optional icon painted with the built-in tint and size.
    pub icon: Option<&'static Icon>,
    condition: Box<dyn Fn(&HostMenuContext) -> bool>,
}

impl HostMenuEntry {
    /// Create a row visible for every chart, series and drawing click.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            condition: Box::new(|_| true),
        }
    }

    /// Paint an icon beside the label.
    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Omit this row when the predicate is false for the original click.
    pub fn when(mut self, condition: impl Fn(&HostMenuContext) -> bool + 'static) -> Self {
        self.condition = Box::new(condition);
        self
    }
}

/// A context-menu action surfaced by the turnkey right-click path, tagged with
/// the menu it came from.
///
/// The chart opens one of three menus depending on what the right-click landed
/// on (background, a series/indicator, or a drawing); each menu has its own
/// action enum. [`ChartMenus::show`] returns this unified value so a host can match a single
/// type.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartContextAction {
    /// A registered host entry, with the original click context.
    Host {
        /// The registered entry id.
        id: String,
        /// The click that opened the menu, retained even if the pointer or chart moves.
        context: HostMenuContext,
    },
    /// An action from the chart-background menu.
    Chart(ContextMenuAction),
    /// An action from the series menu.
    Series(crate::ui::dialogs::SeriesContextMenuAction),
    /// An action from the menu of study `id`, opened from its legend row or plot.
    Study {
        /// The study the menu acts on.
        id: SeriesId,
        /// The chosen action, already applied to the chart when it is the chart's to apply.
        action: crate::ui::dialogs::StudyContextMenuAction,
    },
    /// An action from the drawing menu.
    Drawing(crate::ui::dialogs::DrawingContextMenuAction),
}

/// The chart, series and drawing context menus of one [`Chart`].
///
/// Keep one next to the chart and call [`show`](Self::show) after each
/// [`Chart::show`]: a right-click on the chart opens the menu for what it landed on, and the
/// item the user picks comes back as a [`ChartContextAction`]. The menus follow the theme
/// applied with [`apply_to_egui`](crate::theme::apply_to_egui).
///
/// ```
/// use egui_charts::ui::context_menu::{ChartContextAction, ChartMenus, HostMenuEntry};
/// use egui_charts::widget::RightClickTarget;
/// let mut menus = ChartMenus::default();
/// menus.host_entries.push(HostMenuEntry::new("ticket", "Send to ticket")
///     .when(|click| matches!(click.target, RightClickTarget::Background { .. })));
/// // After chart.show(ui), handle ChartContextAction::Host { id, context } from menus.show.
/// ```
#[derive(Default)]
pub struct ChartMenus {
    chart: ChartContextMenu,
    series: SeriesContextMenu,
    study: StudyContextMenu,
    drawing: DrawingContextMenu,
    axes: super::axis_context_menu::AxisMenus,
    tpo: Option<TpoMenu>,
    /// Host rows in display order. Empty by default.
    pub host_entries: Vec<HostMenuEntry>,
    context: Option<HostMenuContext>,
    just_opened: bool,
}

impl ChartMenus {
    /// Shows or hides named menu entries: Alerts, Trading, Settings, ObjectTree, Indicators,
    /// ResetView, CopyPrice, ExportData, TimeZone and Session. `ContextMenus` disables all menus,
    /// including drawings and axes. Controls belonging only to toolbars are ignored.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        set_control_visible(&mut self.chart.hidden, control, visible);
        set_control_visible(&mut self.series.hidden, control, visible);
        set_control_visible(&mut self.study.hidden, control, visible);
        set_control_visible(&mut self.axes.hidden, control, visible);
    }

    /// Sets the quantity of the chart menu's order entries and the order types the account
    /// accepts, such as [`Trading::quantity`](crate::ui::trading::Trading::quantity) and
    /// [`Trading::capabilities`](crate::ui::trading::Trading::capabilities). By default the
    /// entries are for 1 unit and offer limit and stop orders.
    pub fn set_order_entries(&mut self, quantity: f64, capabilities: Capabilities) {
        self.chart.order_quantity = quantity;
        self.chart.order_types = capabilities;
    }

    /// Opens the menu for the chart's last right-click, shows the open one and returns the
    /// chosen action. While a menu is open the chart draws no crosshair or tooltip under it.
    pub fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ChartContextAction> {
        if self.chart.hidden.contains(&Control::ContextMenus) {
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            self.axes.close();
            self.context = None;
            chart.take_right_click();
            chart.menu_open = false;
            return None;
        }
        if self.axes.open_from_pointer(ctx, chart) {
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            self.context = None;
        }
        if let Some(target) = chart.take_right_click() {
            self.axes.close();
            let pos = match target {
                RightClickTarget::Element { pos, .. }
                | RightClickTarget::Drawing { pos, .. }
                | RightClickTarget::Background { pos, .. } => pos,
            };
            let mapping = chart.get_chart_mapping();
            let index = mapping.x_to_idx_f32(pos.x).round();
            let time = (index >= 0.0)
                .then(|| chart.state().shown().get(index as usize))
                .flatten()
                .map(|bar| bar.time);
            let price = match target {
                RightClickTarget::Element { price, .. }
                | RightClickTarget::Background { price, .. } => price,
                RightClickTarget::Drawing { .. } => chart
                    .get_rendered_indicator_panes()
                    .iter()
                    .find(|pane| pane.chart_rect.contains(pos))
                    .map_or_else(
                        || mapping.y_to_price(pos.y),
                        |pane| pane.mapping.y_to_price(pos.y),
                    ),
            };
            self.context = Some(HostMenuContext {
                target,
                time,
                price,
            });
            self.tpo = match target {
                RightClickTarget::Background { .. }
                | RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    ..
                } => TpoMenu::open(chart, pos),
                _ => None,
            };
            self.just_opened = true;
            // A new right-click supersedes any menu still open.
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            match target {
                RightClickTarget::Element {
                    id: ChartElementId::Indicator(id),
                    bar_idx,
                    ..
                } => self.study.open(chart, id, bar_idx),
                RightClickTarget::Element {
                    id: ChartElementId::Series(id),
                    pos,
                    price,
                    ..
                } => {
                    self.series.set_context(chart.symbol.clone(), price);
                    self.series.open(pos, id);
                }
                RightClickTarget::Drawing { drawing_id, pos } => {
                    let drawings = &chart.drawings().drawings;
                    if let Some(drawing) = drawings.iter().find(|d| d.id == drawing_id) {
                        let type_name = drawing.tool_type.as_str();
                        self.drawing
                            .set_context(type_name, drawing.locked, drawing.visible);
                    }
                    self.drawing.open(pos, drawing_id);
                }
                RightClickTarget::Background { pos, price } => {
                    self.chart.set_indicator_cnt(chart.indicators().count());
                    self.chart.set_symbol(chart.symbol.clone());
                    self.chart.market_price =
                        crate::model::last_with_value(chart.bars(SeriesId::MAIN)).map(|b| b.close);
                    self.chart.tick_size = chart
                        .instrument()
                        .and_then(|instrument| instrument.tick_size)
                        .filter(|tick| tick.is_finite() && *tick > 0.0);
                    self.chart.open(pos, price);
                }
            }
        }

        let theme = ctx.active_theme();
        let mut menu_action = None;
        if let Some(context) = self.context {
            let (pos, gap) = match context.target {
                RightClickTarget::Element { pos, .. }
                | RightClickTarget::Background { pos, .. } => (pos, POINTER_GAP),
                RightClickTarget::Drawing { pos, .. } => (pos, DRAWING_MENU_OFFSET),
            };
            let entries = self.build_menu_items(chart.shortcuts(), context);
            if let Some(picked) = show_menu(
                ctx,
                Id::unique("chart_host_menu"),
                &theme,
                pos,
                gap,
                &entries,
                std::mem::take(&mut self.just_opened),
            ) {
                if let Some(ChartContextAction::Study { id, action }) = &picked {
                    chart.user_action(|chart| study_context_menu::apply(chart, *id, action));
                }
                if let Some(ChartContextAction::Chart(action)) = &picked
                    && let Some(tpo) = &self.tpo
                {
                    chart.user_action(|chart| tpo.apply(chart, action));
                }
                menu_action = picked;
                self.context = None;
                self.chart.close();
                self.series.close();
                self.study.close();
                self.drawing.close();
            }
        }
        let axis_action = chart.user_action(|chart| self.axes.show(ctx, chart));
        chart.menu_open = self.chart.is_open()
            || self.series.is_open()
            || self.study.is_open()
            || self.drawing.is_open()
            || self.axes.is_open();

        // At most one menu is open at a time, so at most one action resolves.
        axis_action.map(ChartContextAction::Chart).or(menu_action)
    }

    fn build_menu_items(
        &self,
        shortcuts: &crate::widget::ShortcutMap,
        context: HostMenuContext,
    ) -> Vec<MenuEntry<ChartContextAction>> {
        fn tagged<A>(
            entries: Vec<MenuEntry<A>>,
            tag: impl Fn(A) -> ChartContextAction,
        ) -> Vec<MenuEntry<ChartContextAction>> {
            entries.into_iter().map(|entry| entry.map(&tag)).collect()
        }
        let mut entries = match context.target {
            RightClickTarget::Background { .. } => tagged(
                self.chart.build_menu_items(shortcuts),
                ChartContextAction::Chart,
            ),
            RightClickTarget::Element {
                id: ChartElementId::Indicator(id),
                ..
            } => tagged(self.study.build_menu_items(shortcuts), |action| {
                ChartContextAction::Study { id, action }
            }),
            RightClickTarget::Element { .. } => tagged(
                self.series.build_menu_items(shortcuts),
                ChartContextAction::Series,
            ),
            RightClickTarget::Drawing { .. } => tagged(
                self.drawing.build_menu_items(shortcuts),
                ChartContextAction::Drawing,
            ),
        };
        if let Some(tpo) = &self.tpo {
            let mut profile = tagged(tpo.entries(), ChartContextAction::Chart);
            if !entries.is_empty() {
                profile.push(MenuEntry::Separator);
            }
            profile.extend(entries);
            entries = profile;
        }
        let mut separated = false;
        for entry in &self.host_entries {
            if !(entry.condition)(&context) {
                continue;
            }
            if !separated {
                if !entries.is_empty() {
                    entries.push(MenuEntry::Separator);
                }
                separated = true;
            }
            let mut item = MenuItem::new(
                &entry.label,
                ChartContextAction::Host {
                    id: entry.id.clone(),
                    context,
                },
            );
            item.icon = entry.icon;
            entries.push(MenuEntry::Item(item));
        }
        entries
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

    /// Each named control removes its row while leaving unrelated entries available; removing
    /// every row closes the popup instead of leaving separators or stale actions.
    #[test]
    fn hidden_menu_controls_remove_rows_and_empty_sections() {
        let controls = [
            Control::Alerts,
            Control::Trading,
            Control::Settings,
            Control::ObjectTree,
            Control::Indicators,
            Control::ResetView,
            Control::CopyPrice,
            Control::ExportData,
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for series in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
                let mut menus = ChartMenus::default();
                let frame = |menus: &mut ChartMenus, chart: &mut Chart| {
                    let mut output = ctx.run_ui(Default::default(), |ui| {
                        menus.show(ui.ctx(), chart);
                    });
                    let mut labels = Vec::new();
                    fn collect(shape: &egui::Shape, labels: &mut Vec<String>) {
                        match shape {
                            egui::Shape::Vec(shapes) => {
                                for shape in shapes {
                                    collect(shape, labels);
                                }
                            }
                            egui::Shape::Text(text) => labels.push(text.galley.text().to_owned()),
                            _ => {}
                        }
                    }
                    for shape in &output.shapes {
                        collect(&shape.shape, &mut labels);
                    }
                    output.textures_delta.clear();
                    output.drop_without_applying_deltas();
                    labels
                };
                for visible in [true, false, true] {
                    for control in controls {
                        menus.set_control_visible(control, visible);
                    }
                    chart.symbol = "SYNTH".into();
                    chart.right_click = Some(if series {
                        RightClickTarget::Element {
                            id: ChartElementId::Series(SeriesId::MAIN),
                            bar_idx: 0,
                            pos: Pos2::new(30.0, 30.0),
                            price: 105.0,
                        }
                    } else {
                        RightClickTarget::Background {
                            pos: Pos2::new(30.0, 30.0),
                            price: 105.0,
                        }
                    });
                    frame(&mut menus, &mut chart);
                    let labels = frame(&mut menus, &mut chart);
                    assert_eq!(!labels.is_empty(), visible, "{labels:?}");
                    assert_eq!(menus.chart.is_open() || menus.series.is_open(), visible);
                    if !visible {
                        menus
                            .host_entries
                            .push(HostMenuEntry::new("host-only", "Host only"));
                        chart.right_click = Some(RightClickTarget::Background {
                            pos: Pos2::new(30.0, 30.0),
                            price: 105.0,
                        });
                        frame(&mut menus, &mut chart);
                        assert!(
                            frame(&mut menus, &mut chart)
                                .iter()
                                .any(|label| label == "Host only")
                        );
                        assert!(
                            matches!(
                                menus
                                    .build_menu_items(chart.shortcuts(), menus.context.unwrap())
                                    .first(),
                                Some(MenuEntry::Item(_))
                            ),
                            "host-only menus have no leading separator"
                        );
                        menus.host_entries.clear();
                    }
                }
                menus.set_control_visible(Control::Settings, false);
                menus.set_control_visible(Control::ObjectTree, false);
                menus.set_control_visible(Control::Indicators, false);
                chart.right_click = Some(RightClickTarget::Background {
                    pos: Pos2::new(30.0, 30.0),
                    price: 105.0,
                });
                let labels = frame(&mut menus, &mut chart);
                assert!(labels.iter().any(|label| label.starts_with("Copy price")));
                assert!(!labels.iter().any(|label| label.contains("Settings")
                    || label.contains("Object Tree")
                    || label.contains("indicator")));
                menus.set_control_visible(Control::ContextMenus, false);
                assert!(frame(&mut menus, &mut chart).is_empty());
                assert!(!chart.menu_open);
                menus.set_control_visible(Control::ContextMenus, true);
                assert!(
                    frame(&mut menus, &mut chart).is_empty(),
                    "closed menus stay closed"
                );
            }
        }
    }

    #[test]
    fn host_menu_entries_filter_targets_and_return_the_original_click() {
        use egui::{Event, PointerButton, RawInput, Shape};
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[100.0; 10], &[110.0; 10]);
            let mut menus = ChartMenus {
                host_entries: vec![
                    HostMenuEntry::new("ticket", "Send to ticket").when(|context| {
                        matches!(context.target, RightClickTarget::Background { .. })
                    }),
                    HostMenuEntry::new("research", "Open in research"),
                ],
                ..Default::default()
            };
            let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
                let mut action = None;
                let right_click = chart.right_click.take();
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        if right_click.is_some() {
                            chart.right_click = right_click;
                        }
                        action = menus.show(ui.ctx(), chart);
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some((
                            text.galley.text().to_owned(),
                            text.pos + text.galley.size() / 2.0,
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
                (action, text)
            };
            frame(&mut chart, &mut menus, vec![]);
            let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(5), 100.0);
            let time = chart.state().shown()[5].time;
            for target in [
                RightClickTarget::Background { pos, price: 123.25 },
                RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    pos,
                    bar_idx: 5,
                    price: 321.5,
                },
                RightClickTarget::Drawing {
                    drawing_id: 42,
                    pos,
                },
            ] {
                chart.right_click = Some(target);
                frame(&mut chart, &mut menus, vec![]);
                let (_, text) = frame(&mut chart, &mut menus, vec![]);
                let background = matches!(target, RightClickTarget::Background { .. });
                assert_eq!(
                    text.iter().any(|(label, _)| label == "Send to ticket"),
                    background
                );
                let label = if background {
                    "Send to ticket"
                } else {
                    "Open in research"
                };
                let pick = text
                    .iter()
                    .find(|(text, _)| text == label)
                    .expect("host row must be rendered")
                    .1;
                if background {
                    let ticket = text
                        .iter()
                        .position(|(label, _)| label == "Send to ticket")
                        .unwrap();
                    let research = text
                        .iter()
                        .position(|(label, _)| label == "Open in research")
                        .unwrap();
                    assert!(ticket < research, "registration order");
                }
                let original = menus.context.unwrap();
                assert_eq!(original.time, Some(time));
                assert_eq!(original.target, target);
                match target {
                    RightClickTarget::Background { .. } => assert_eq!(original.price, 123.25),
                    RightClickTarget::Element { .. } => assert_eq!(original.price, 321.5),
                    RightClickTarget::Drawing { .. } => {
                        // Linear scale: the top pixel is max and the bottom pixel is min.
                        let map = chart.get_chart_mapping();
                        let range = chart.state().price_scale().range();
                        let expected = range.max
                            - (pos.y - map.rect.top()) as f64 / map.rect.height() as f64
                                * (range.max - range.min);
                        assert!((original.price - expected).abs() < 0.00001);
                    }
                }
                frame(&mut chart, &mut menus, vec![Event::PointerMoved(pick)]);
                let mut chosen = None;
                for pressed in [true, false] {
                    let (action, _) = frame(
                        &mut chart,
                        &mut menus,
                        vec![Event::PointerButton {
                            pos: pick,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                    chosen = chosen.or(action);
                }
                assert_eq!(
                    chosen,
                    Some(ChartContextAction::Host {
                        id: if background { "ticket" } else { "research" }.into(),
                        context: original,
                    })
                );
                assert!(!chart.menu_open);
                assert_eq!(
                    frame(&mut chart, &mut menus, vec![]).0,
                    None,
                    "one action per choice"
                );
            }
        }
    }

    #[test]
    fn axis_right_clicks_open_menus_and_apply_choices() {
        use crate::scales::{PriceScaleMode, TimezoneMode};
        use egui::{Event, PointerButton, RawInput, Shape};
        fn paint(
            ctx: &Context,
            chart: &mut Chart,
            menus: &mut ChartMenus,
            events: Vec<Event>,
            size: Vec2,
        ) -> Vec<egui::epaint::ClippedShape> {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                    menus.show(ui.ctx(), chart);
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            shapes
        }
        let click = |pos, button| {
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: Default::default(),
                },
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        for theme in [Theme::light(), Theme::dark()] {
            for (time, left, ratio, short) in [
                (false, false, false, false),
                (false, true, false, false),
                (true, false, false, false),
                (false, false, true, false),
                (false, false, false, true),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let size = if short {
                    Vec2::new(320.0, 240.0)
                } else {
                    Vec2::new(800.0, 900.0)
                };
                let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
                chart.config_mut().show_left_axis = left;
                chart.config_mut().show_right_axis = !left;
                let mut menus = ChartMenus::default();
                paint(&ctx, &mut chart, &mut menus, vec![], size);
                let plot = chart.get_rendered_price_rect();
                let anchor = if time {
                    Pos2::new(plot.center().x, plot.bottom() + 10.0)
                } else {
                    Pos2::new(
                        if left {
                            plot.left() - 25.0
                        } else {
                            plot.right() + 25.0
                        },
                        plot.center().y,
                    )
                };
                paint(
                    &ctx,
                    &mut chart,
                    &mut menus,
                    click(anchor, PointerButton::Secondary),
                    size,
                );
                if short {
                    paint(
                        &ctx,
                        &mut chart,
                        &mut menus,
                        vec![
                            Event::PointerMoved(Pos2::new(150.0, 120.0)),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                source: egui::MouseWheelSource::Wheel,
                                phase: egui::TouchPhase::Move,
                                delta: Vec2::new(0.0, -2000.0),
                                modifiers: Default::default(),
                            },
                        ],
                        size,
                    );
                    for _ in 0..30 {
                        paint(&ctx, &mut chart, &mut menus, vec![], size);
                    }
                }
                let shapes = paint(&ctx, &mut chart, &mut menus, vec![], size);
                if !time && !short {
                    let settings = shapes.iter().find(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == "Scale settings…")).expect("bottom scale row");
                    if let Shape::Text(text) = &settings.shape {
                        assert!(
                            settings
                                .clip_rect
                                .contains_rect(Rect::from_min_size(text.pos, text.galley.size())),
                            "a menu that fits must show its bottom row without scrolling"
                        );
                    }
                }
                let label = if short {
                    "High and low"
                } else if time {
                    "Local time"
                } else if ratio {
                    "Lock price to bar ratio"
                } else {
                    "Percent"
                };
                let text = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text() == label => Some(text),
                        _ => None,
                    })
                    .expect("axis menu choice");
                let pos = text.pos + text.galley.size() / 2.0;
                assert!(Rect::from_min_size(Pos2::ZERO, size).contains(pos));
                paint(
                    &ctx,
                    &mut chart,
                    &mut menus,
                    click(pos, PointerButton::Primary),
                    size,
                );
                assert!(!chart.menu_open);
                let config = chart.config().clone();
                let options = chart.chart_options().clone();
                let scale = *chart.state().price_scale().options();
                assert!(chart.undo(), "axis choices enter chart history");
                assert!(!chart.can_undo(), "one action per axis choice");
                assert!(chart.redo());
                assert_eq!(chart.config(), &config);
                assert_eq!(chart.chart_options(), &options);
                assert_eq!(chart.state().price_scale().options(), &scale);
                if short {
                    assert!(chart.config().show_high_low);
                } else if ratio {
                    let before = chart.state().price_scale().range().length();
                    let (spacing, offset) = chart.get_time_scale_state();
                    chart.apply_synced_time_scale(spacing * 2.0, offset);
                    paint(&ctx, &mut chart, &mut menus, vec![], size);
                    // Doubling the pixels per bar halves the price span at the same pane height.
                    assert!(
                        (chart.state().price_scale().range().length() - before / 2.0).abs()
                            < 0.00001
                    );
                } else if time {
                    assert_eq!(
                        chart.chart_options().time_scale.timezone,
                        TimezoneMode::Local
                    );
                } else {
                    assert_eq!(
                        chart.state().price_scale().options().mode,
                        PriceScaleMode::Percentage
                    );
                }
            }
        }
    }

    #[test]
    fn opening_a_menu_suppresses_hover_paint_in_the_same_frame() {
        use egui::{Event, PointerButton, RawInput, Shape};
        fn uses_ink(shape: &Shape, ink: Color32) -> bool {
            match shape {
                Shape::Vec(shapes) => shapes.iter().any(|shape| uses_ink(shape, ink)),
                Shape::LineSegment { stroke, .. } => stroke.color == ink,
                Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(ink),
                Shape::Rect(rect) => rect.fill == ink,
                _ => false,
            }
        }
        for (theme, pane) in [
            (Theme::light(), false),
            (Theme::dark(), false),
            (Theme::light(), true),
            (Theme::dark(), true),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
            if pane {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            }
            let ink = Color32::from_rgb(17, 41, 83);
            let options = &mut chart.chart_options;
            options.crosshair.vert_line_color = ink;
            options.crosshair.horz_line_color = ink;
            options.crosshair.vert_label_background_color = ink;
            options.crosshair.horz_label_background_color = ink;
            options.tooltip.enabled = true;
            options.tooltip.on_long_press = false;
            options.tooltip.background_color = ink;
            let mut menus = ChartMenus::default();
            let mut frame = |events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        menus.show(ui.ctx(), &mut chart);
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                shapes.iter().any(|shape| uses_ink(&shape.shape, ink))
            };
            frame(vec![]);
            let pos = Pos2::new(400.0, 200.0);
            assert!(
                frame(vec![Event::PointerMoved(pos)]),
                "hover must paint before opening"
            );
            let click = |button| {
                vec![
                    Event::PointerButton {
                        pos,
                        button,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                    Event::PointerButton {
                        pos,
                        button,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                ]
            };
            assert!(
                !frame(click(PointerButton::Secondary)),
                "opening frame still paints hover"
            );
            assert!(!frame(vec![]), "open menu still paints hover");
            frame(vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }]);
            assert!(frame(vec![]), "closing must restore hover");
        }
    }

    #[test]
    fn narrow_menus_keep_labels_out_of_the_shortcut_column() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.set_os(egui::os::OperatingSystem::Nix);
            let entries = [MenuEntry::Item(
                MenuItem::new("Add alert on VERY_LONG_SYMBOL at 123,456.78…", ())
                    .with_shortcut(KeyboardAction::ResetChart.shortcut()),
            )];
            for width in [800.0, 320.0] {
                for pass in 0..2 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 300.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            show_menu(
                                ui.ctx(),
                                Id::unique("narrow_menu"),
                                &theme,
                                Pos2::new(20.0, 20.0),
                                POINTER_GAP,
                                &entries,
                                true,
                            );
                        },
                    );
                    let text: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| {
                            if let egui::Shape::Text(text) = &shape.shape {
                                Some(text.galley.rect.translate(text.pos.to_vec2()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if pass == 1 {
                        assert_eq!(text.len(), 2);
                        assert!(text[0].right() + COLUMN_GAP <= text[1].left() + 0.5);
                        assert!(text[1].right() <= width - theme.spacing.lg);
                    }
                    output.drop_without_applying_deltas();
                }
            }
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

    /// Guards #106: actual profile clicks merge neighbours, extend a composite and split it,
    /// preserving unrelated host ranges and making each command one undoable edit.
    #[test]
    fn tpo_profile_menu_merges_and_splits_sessions() {
        use crate::model::{Bar, BarData, ChartType, Timeframe, to_tpo_profiles};
        use chrono::{DateTime, Duration};
        use egui::{Event, PointerButton, RawInput, Shape};
        for (theme, width) in [
            (Theme::dark(), 900.0),
            (Theme::light(), 900.0),
            (Theme::dark(), 320.0),
            (Theme::light(), 320.0),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let day = |d| DateTime::UNIX_EPOCH + Duration::days(d);
            let bars: Vec<_> = (0..16)
                .map(|i| {
                    Bar::new(
                        day(i / 4) + Duration::minutes(i % 4 * 30),
                        100.0,
                        102.0,
                        100.0,
                        102.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min30);
            chart.set_chart_type(ChartType::TimePriceOpportunity);
            let mut params = chart.chart_type_params().clone();
            // The last loaded session belongs to a separate host range extending into the future.
            params.tpo_composites = vec![day(3)..=day(4)];
            chart.set_chart_type_params(params);
            let mut menus = ChartMenus::default();
            let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
                let mut action = None;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        action = menus.show(ui.ctx(), chart);
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                (action, shapes)
            };
            frame(&mut chart, &mut menus, vec![]);
            chart.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 18.0));
            frame(&mut chart, &mut menus, vec![]);
            let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
                frame(chart, menus, vec![Event::PointerMoved(pos)]);
                let mut action = None;
                for pressed in [true, false] {
                    action = action.or(frame(
                        chart,
                        menus,
                        vec![Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    )
                    .0);
                }
                action
            };
            let pick = |chart: &mut Chart, menus: &mut ChartMenus, bar, label: &str| {
                frame(chart, menus, vec![]);
                let map = chart.get_chart_mapping();
                let pos = Pos2::new(
                    map.idx_to_x(bar).max(map.rect.left()) + 4.0,
                    map.price_to_y(101.0),
                );
                click(chart, menus, pos, PointerButton::Secondary);
                let (_, shapes) = frame(chart, menus, vec![]);
                let row = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("no {label} row for profile at {bar}"));
                click(chart, menus, row, PointerButton::Primary)
            };
            let counts = |chart: &Chart| {
                let mut config = chart.chart_type_params().tpo_config(None);
                config.tick_size = 1.0;
                to_tpo_profiles(chart.bars(SeriesId::MAIN), &config, &chart.bar_clock())
                    .iter()
                    .map(|p| p.tpo_count_at(101.0, p.tick_size))
                    .collect::<Vec<_>>()
            };
            assert_eq!(counts(&chart), [4, 4, 4, 4]);
            assert!(matches!(
                pick(&mut chart, &mut menus, 4, "Merge with previous profile"),
                Some(ChartContextAction::Chart(_))
            ));
            // Four half-hour periods per session, one letter at every row per period.
            assert_eq!(counts(&chart), [8, 4, 4]);
            assert!(chart.undo());
            assert_eq!(counts(&chart), [4, 4, 4, 4]);
            assert!(chart.redo());
            assert_eq!(counts(&chart), [8, 4, 4]);
            pick(&mut chart, &mut menus, 0, "Merge with next profile");
            assert_eq!(counts(&chart), [12, 4]);
            // Pan the composite's opening offscreen; its displayed anchor still opens its menu.
            chart.set_visible_logical_range(crate::model::LogicalRange::new(3.0, 18.0));
            pick(&mut chart, &mut menus, 0, "Split profile");
            assert_eq!(counts(&chart), [4, 4, 4, 4]);
            assert_eq!(chart.chart_type_params().tpo_composites, [day(3)..=day(4)]);
            assert!(chart.undo());
            assert_eq!(counts(&chart), [12, 4]);
        }
    }

    /// Guards #850: a secondary click on a study's legend row opens the study's menu, and each
    /// entry changes the study through the chart's public API or returns its host intent.
    #[test]
    fn study_menu_entries_act_on_the_study() {
        use crate::model::{AlertCondition, AlertSource, Timeframe};
        use crate::studies::{IndicatorFactory, RSI, SMA, StudyScale};
        use crate::ui::dialogs::StudyContextMenuAction as A;
        use egui::{Event, PointerButton, RawInput, Shape};
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &Theme::dark());
        // Closes alternate 110 and 120; the last bar, 99, closes at 120.
        let mut chart = crate::widget::test_frames::chart(&[100.0, 104.0], &[110.0, 120.0]);
        chart.symbol = "SYNTH".into();
        chart.set_timeframe(Timeframe::Day1);
        let sma = chart.add_indicator(Box::new(SMA::new(3)));
        let rsi = chart.add_indicator(Box::new(RSI::new(3)));
        let mut menus = ChartMenus::default();
        let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
            let mut action = None;
            let right_click = chart.right_click.take();
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    if right_click.is_some() {
                        chart.right_click = right_click;
                    }
                    action = menus.show(ui.ctx(), chart);
                },
            );
            fn flatten(shape: Shape, out: &mut Vec<Shape>) {
                match shape {
                    Shape::Vec(shapes) => shapes.into_iter().for_each(|shape| flatten(shape, out)),
                    shape => out.push(shape),
                }
            }
            let mut shapes = Vec::new();
            for clipped in std::mem::take(&mut output.shapes) {
                flatten(clipped.shape, &mut shapes);
            }
            output.drop_without_applying_deltas();
            (action, shapes)
        };
        let text_at = |shapes: &[Shape], label: &str| {
            shapes.iter().find_map(|shape| match shape {
                Shape::Text(text) if text.galley.text().starts_with(label) => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
        };
        let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
            frame(chart, menus, vec![Event::PointerMoved(pos)]);
            let mut action = None;
            for pressed in [true, false] {
                let event = Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Default::default(),
                };
                action = action.or(frame(chart, menus, vec![event]).0);
            }
            action
        };
        // Opens the menu of `id` and picks the rows of `path`, hovering each submenu row.
        let pick = |chart: &mut Chart, menus: &mut ChartMenus, id, path: &[&str]| {
            chart.right_click = Some(RightClickTarget::Element {
                id: ChartElementId::Indicator(id),
                bar_idx: 99,
                pos: Pos2::new(120.0, 120.0),
                price: 0.0,
            });
            let (label, submenus) = path.split_last().unwrap();
            for row in submenus {
                let (_, shapes) = frame(chart, menus, vec![]);
                let pos = text_at(&shapes, row).unwrap_or_else(|| panic!("no {row} row"));
                frame(chart, menus, vec![Event::PointerMoved(pos)]);
            }
            let (_, shapes) = frame(chart, menus, vec![]);
            let pos = text_at(&shapes, label).unwrap_or_else(|| panic!("no {label} row"));
            match click(chart, menus, pos, PointerButton::Primary) {
                Some(ChartContextAction::Study { id: picked, action }) if picked == id => action,
                other => panic!("{path:?} returned {other:?}"),
            }
        };
        let study = |chart: &Chart, id| {
            chart
                .indicators()
                .find(|(study, _)| *study == id)
                .map(|(_, study)| study.clone_box())
        };

        // The legend row of the overlay opens its menu.
        frame(&mut chart, &mut menus, vec![]);
        let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
        let row = text_at(&shapes, "SMA").expect("the legend shows the overlay");
        click(&mut chart, &mut menus, row, PointerButton::Secondary);
        let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
        assert!(text_at(&shapes, "Add alert on SMA at 116.67").is_some());
        assert_eq!(
            chart.selected_element(),
            Some(ChartElementId::Indicator(sma))
        );
        frame(
            &mut chart,
            &mut menus,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );

        // SMA(3) at the last bar: (120 + 110 + 120) / 3.
        let A::AddAlert(alert) = pick(&mut chart, &mut menus, sma, &["Add alert on SMA at"]) else {
            panic!("the alert row returns an alert");
        };
        assert_eq!(alert.symbol, "SYNTH");
        assert_eq!(
            alert.source,
            AlertSource::Study {
                id: sma.0.to_string(),
                output: 0
            }
        );
        assert!(
            matches!(alert.condition, AlertCondition::Crossing(level) if (level - 350.0 / 3.0).abs() < 1e-9)
        );
        assert_eq!(
            pick(&mut chart, &mut menus, sma, &["Add indicator on SMA"]),
            A::AddIndicator
        );
        let A::AddToFavorites(name) = pick(
            &mut chart,
            &mut menus,
            sma,
            &["Add this indicator to favorites"],
        ) else {
            panic!("the favorites row names the study");
        };
        assert!(IndicatorFactory::new().has(&name), "{name}");

        // In the main pane the price series is one of the items of the visual order.
        let sma_color = study(&chart, sma).unwrap().colors()[0];
        let bullish = chart.config().bullish_color;
        let sma_behind = |chart: &mut Chart, menus: &mut ChartMenus| {
            let (_, shapes) = frame(chart, menus, vec![]);
            let first = |color| {
                shapes.iter().position(|shape| {
                    matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == color)
                })
            };
            first(sma_color).unwrap() < first(bullish).unwrap()
        };
        assert!(!sma_behind(&mut chart, &mut menus));
        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Visual order", "Send to back"],
        );
        assert!(sma_behind(&mut chart, &mut menus));
        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Visual order", "Bring forward"],
        );
        assert!(!sma_behind(&mut chart, &mut menus));

        // On the daily chart, the daily interval and above hides the study on intraday ones.
        use Timeframe::*;
        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Visibility on intervals", "Current interval and above"],
        );
        assert_eq!(
            study(&chart, sma).unwrap().settings().hidden_intervals,
            [
                Ms100, Ms250, Ms500, Sec1, Sec2, Sec5, Sec10, Sec30, Min1, Min5, Min15, Min30,
                Hour1, Hour4
            ]
        );
        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Visibility on intervals", "All intervals"],
        );
        assert!(
            study(&chart, sma)
                .unwrap()
                .settings()
                .hidden_intervals
                .is_empty()
        );

        // Moves between the main pane and new and existing panes, in pane order.
        let pane = |chart: &Chart, id| {
            let at = chart
                .indicators()
                .position(|(study, _)| study == id)
                .unwrap();
            chart
                .get_rendered_indicator_panes()
                .iter()
                .find(|pane| pane.indicator_idx == at)
                .map(|pane| pane.chart_rect)
        };
        pick(&mut chart, &mut menus, sma, &["Move to", "New pane below"]);
        frame(&mut chart, &mut menus, vec![]);
        assert!(pane(&chart, sma).unwrap().top() < pane(&chart, rsi).unwrap().top());
        pick(&mut chart, &mut menus, sma, &["Move to", "RSI pane"]);
        frame(&mut chart, &mut menus, vec![]);
        assert_eq!(pane(&chart, sma), pane(&chart, rsi));
        pick(&mut chart, &mut menus, sma, &["Move to", "New pane above"]);
        frame(&mut chart, &mut menus, vec![]);
        assert!(pane(&chart, sma).unwrap().bottom() <= pane(&chart, rsi).unwrap().top());
        pick(&mut chart, &mut menus, sma, &["Move to", "Main pane"]);
        assert!(study(&chart, sma).unwrap().is_overlay());

        // Stable pane handles also reach empty panes, which stay when a study leaves them.
        use crate::widget::PaneId;
        let empty = chart.add_pane();
        assert!(chart.set_preserve_empty_pane(empty, true));
        assert_eq!(
            pick(&mut chart, &mut menus, sma, &["Move to", "empty pane"]),
            A::MoveTo(crate::ui::dialogs::PaneTarget::Pane(empty))
        );
        assert_eq!(chart.series_pane(sma), Some(empty));
        pick(&mut chart, &mut menus, sma, &["Move to", "New pane below"]);
        let panes = chart.panes();
        let at = panes.iter().position(|pane| *pane == empty).unwrap();
        assert_eq!(chart.series_pane(sma), Some(panes[at + 1]));
        assert!(chart.pane_series(empty).is_empty());
        pick(&mut chart, &mut menus, sma, &["Move to", "Main pane"]);
        assert!(chart.remove_pane(empty));

        // Main can sit below another pane; an overlay's new pane is adjacent to main.
        let rsi_pane = chart.series_pane(rsi).unwrap();
        assert!(chart.move_pane(rsi_pane, 0));
        pick(&mut chart, &mut menus, sma, &["Move to", "New pane above"]);
        let sma_pane = chart.series_pane(sma).unwrap();
        assert_eq!(chart.panes(), [rsi_pane, sma_pane, PaneId::MAIN]);
        assert!(chart.undo());
        assert_eq!(chart.series_pane(sma), Some(PaneId::MAIN));
        assert_eq!(chart.panes(), [rsi_pane, PaneId::MAIN]);

        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Pin to scale (now right)", "No scale"],
        );
        assert_eq!(study(&chart, sma).unwrap().scale(), StudyScale::None);
        pick(
            &mut chart,
            &mut menus,
            sma,
            &["Pin to scale (now no scale)", "Pin to new left scale"],
        );
        assert_eq!(study(&chart, sma).unwrap().scale(), StudyScale::Independent);

        assert_eq!(pick(&mut chart, &mut menus, sma, &["Copy"]), A::Copy);
        pick(&mut chart, &mut menus, sma, &["Hide"]);
        assert!(!study(&chart, sma).unwrap().is_visible());
        pick(&mut chart, &mut menus, sma, &["Show"]);
        assert!(study(&chart, sma).unwrap().is_visible());
        pick(&mut chart, &mut menus, rsi, &["Settings…"]);
        assert_eq!(chart.take_indicator_settings_request(), Some(rsi));
        assert_eq!(
            pick(&mut chart, &mut menus, rsi, &["Object tree"]),
            A::ObjectTree
        );
        pick(&mut chart, &mut menus, rsi, &["Remove"]);
        assert!(study(&chart, rsi).is_none());
        assert!(chart.can_undo(), "menu edits are undoable");
    }
}
