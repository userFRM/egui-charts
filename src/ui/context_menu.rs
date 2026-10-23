//! Chart context menu
//!
//! Right-click context menu for the chart area. Provides trading actions,
//! clipboard operations, chart management, and navigation options.

use std::collections::HashSet;
use std::sync::Arc;

use crate::chart::selection::{ChartElementId, SeriesId};
use crate::config::KeyboardAction;
use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::{Theme, ThemeExt};
use crate::ui::Control;
use crate::ui::app_state::set_control_visible;
use crate::ui::dialogs::{
    DrawingContextMenu, DrawingContextMenuAction, SeriesContextMenu, SeriesContextMenuAction,
};
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

/// Menu entry - either an item or separator
pub(crate) enum MenuEntry<A> {
    Item(MenuItem<A>),
    Separator,
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
/// anchor on an axis where it would leave the window. `just_opened` skips the dismiss check on
/// the frame of the click that opened it.
pub(crate) fn show_menu<A: Clone>(
    ctx: &Context,
    id: Id,
    theme: &Theme,
    anchor: Pos2,
    gap: f32,
    entries: &[MenuEntry<A>],
    just_opened: bool,
) -> Option<Option<A>> {
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
    let rows: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item(item) => Some((
                layout(item.label.clone(), t.font_size, MAX_LABEL_WIDTH),
                item.shortcut
                    .map(|s| layout(ctx.format_shortcut(&s), t.shortcut_font_size, f32::INFINITY)),
            )),
            MenuEntry::Separator => None,
        })
        .collect();

    let (label_width, trailing_width) = rows.iter().flatten().fold(
        (0.0_f32, 0.0_f32),
        |(widest, widest_trailing), (label, trailing)| {
            let trailing = trailing.as_ref().map_or(0.0, |g| g.size().x);
            (widest.max(label.size().x), widest_trailing.max(trailing))
        },
    );
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
            MenuEntry::Item(_) => t.item_height,
            MenuEntry::Separator => t.separator_height,
        })
        .sum();
    let size = Vec2::new(row_width, height) + chrome;
    let pos = place(
        anchor,
        gap,
        size,
        ctx.content_rect().shrink(theme.spacing.lg),
    );

    let mut picked = None;
    egui::Area::new(id)
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (entry, row) in entries.iter().zip(rows) {
                    match (entry, row) {
                        (MenuEntry::Item(item), Some((label, trailing))) => {
                            if draw_item(ui, theme, item, label, trailing, row_width, pad) {
                                picked = Some(item.action.clone());
                            }
                        }
                        _ => draw_separator(ui, theme, row_width, pad),
                    }
                }
            });
        });

    if picked.is_some() {
        return Some(picked);
    }
    let menu_rect = Rect::from_min_size(pos, size);
    let dismissed = !just_opened
        && ctx.input(|i| {
            let clicked = i.pointer.primary_clicked() || i.pointer.secondary_clicked();
            let outside = i
                .pointer
                .interact_pos()
                .is_some_and(|p| !menu_rect.contains(p));
            (clicked && outside) || i.key_pressed(egui::Key::Escape)
        });
    dismissed.then_some(None)
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

/// Draw one row and return whether it was clicked.
fn draw_item<A>(
    ui: &mut Ui,
    theme: &Theme,
    item: &MenuItem<A>,
    label: Arc<Galley>,
    trailing: Option<Arc<Galley>>,
    width: f32,
    pad: f32,
) -> bool {
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

    let hovered = item.enabled && response.hovered();
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

    response.clicked() && item.enabled
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
    fn build_menu_items(&self) -> Vec<MenuEntry<ContextMenuAction>> {
        let price_str = format_price(self.click_price);
        let symbol = &self.symbol;

        let mut items = vec![
            // Reset chart view at top
            MenuEntry::Item(
                MenuItem::new("Reset chart view", ContextMenuAction::ResetChart)
                    .with_shortcut(KeyboardAction::ResetChart.shortcut()),
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
                .with_shortcut(KeyboardAction::CreateAlert.shortcut()),
            ));
        }
        if !symbol.is_empty() && !self.hidden.contains(&Control::Trading) {
            trading.extend([
                MenuEntry::Item(
                    MenuItem::new(
                        format!("Sell 1 {symbol} @ {price_str} limit"),
                        ContextMenuAction::SellLimit {
                            symbol: symbol.clone(),
                            price: self.click_price,
                            quantity: 1,
                        },
                    )
                    .with_icon(&embedded_icons::SHORT_POS),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!("Buy 1 {symbol} @ {price_str} stop"),
                        ContextMenuAction::BuyStop {
                            symbol: symbol.clone(),
                            price: self.click_price,
                            quantity: 1,
                        },
                    )
                    .with_icon(&embedded_icons::LONG_POS),
                ),
            ]);
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
        items
    }

    /// Show the context menu
    pub fn show(&mut self, ctx: &Context, theme: &Theme) {
        if !self.is_open {
            return;
        }
        let entries = self.build_menu_items();
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

/// A context-menu action surfaced by the turnkey right-click path, tagged with
/// the menu it came from.
///
/// The chart opens one of three menus depending on what the right-click landed
/// on (background, a series/indicator, or a drawing); each menu has its own
/// action enum. [`ChartMenus::show`] returns this unified value so a host can match a single
/// type.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartContextAction {
    /// An action from the chart-background menu.
    Chart(ContextMenuAction),
    /// An action from the series/indicator menu.
    Series(crate::ui::dialogs::SeriesContextMenuAction),
    /// An action from the drawing menu.
    Drawing(crate::ui::dialogs::DrawingContextMenuAction),
}

/// The chart, series and drawing context menus of one [`Chart`].
///
/// Keep one next to the chart and call [`show`](Self::show) after each
/// [`Chart::show`]: a right-click on the chart opens the menu for what it landed on, and the
/// item the user picks comes back as a [`ChartContextAction`]. The menus follow the theme
/// applied with [`apply_to_egui`](crate::theme::apply_to_egui).
#[derive(Default)]
pub struct ChartMenus {
    chart: ChartContextMenu,
    series: SeriesContextMenu,
    drawing: DrawingContextMenu,
}

impl ChartMenus {
    /// Shows or hides `control`: Alerts (the add alert rows of the chart and series menus)
    /// and Trading (the chart menu's sell limit and buy stop rows). The menus ignore the other
    /// controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        set_control_visible(&mut self.chart.hidden, control, visible);
        set_control_visible(&mut self.series.hidden, control, visible);
    }

    /// Opens the menu for the chart's last right-click, shows the open one and returns the
    /// chosen action. While a menu is open the chart draws no crosshair or tooltip under it.
    pub fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ChartContextAction> {
        if let Some(target) = chart.take_right_click() {
            // A new right-click supersedes any menu still open.
            self.chart.close();
            self.series.close();
            self.drawing.close();
            match target {
                RightClickTarget::Element { id, pos, price, .. } => {
                    // The series menu fronts both the price series and indicator lines: it is
                    // the object-level menu (settings, hide, etc.).
                    let label = match id {
                        ChartElementId::Series(_) => chart.symbol.clone(),
                        ChartElementId::Indicator(id) => chart
                            .indicators()
                            .find(|(study, _)| *study == id)
                            .map(|(_, study)| study.name().to_string())
                            .unwrap_or_else(|| chart.symbol.clone()),
                    };
                    let series_id = match id {
                        ChartElementId::Series(sid) => sid,
                        _ => SeriesId::MAIN,
                    };
                    self.series.set_context(label, price);
                    self.series.open(pos, series_id);
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
                    self.chart.open(pos, price);
                }
            }
        }

        let theme = ctx.active_theme();
        self.chart.show(ctx, &theme);
        self.series.show(ctx, &theme);
        self.drawing.show(ctx, &theme);
        chart.menu_open = self.chart.is_open() || self.series.is_open() || self.drawing.is_open();

        // At most one menu is open at a time, so at most one action resolves.
        let chart_action = self.chart.take_action();
        let series_action = self.series.take_action();
        let drawing_action = self.drawing.take_action();
        if chart_action != ContextMenuAction::None {
            Some(ChartContextAction::Chart(chart_action))
        } else if series_action != SeriesContextMenuAction::None {
            Some(ChartContextAction::Series(series_action))
        } else if drawing_action != DrawingContextMenuAction::None {
            Some(ChartContextAction::Drawing(drawing_action))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_row_shows_the_reset_chart_binding() {
        let ctx = Context::default();
        ctx.set_os(egui::os::OperatingSystem::Nix);
        let shortcut = ChartContextMenu::new()
            .build_menu_items()
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
        assert_eq!(ctx.format_shortcut(&shortcut), "Ctrl+R");
    }
}
