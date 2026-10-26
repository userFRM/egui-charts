//! Builder framework for dropdown submenus, and the flyout every drawing toolbar menu shares.
//!
//! The toolbar shows each menu in one flyout (`show_flyout`): a popup anchored to the
//! button that opened it, with the theme's menu frame, closing on an outside click or Escape.
//! Its rows are drawn by `flyout_row`, so tool lists and utility menus look alike.
//!
//! ## Usage
//!
//! ```rust
//! use egui::Ui;
//! use egui_charts::icons::icons;
//! use egui_charts::ui::drawing_toolbar::{SubmenuBuilder, ToggleConfig};
//!
//! fn cursor_menu(ui: &mut Ui, feature_enabled: &mut bool) {
//!     let action = SubmenuBuilder::new(ui)
//!         .with_width(240.0)
//!         .add_icon_item(&icons::CROSSHAIR, "Cross", "Cross cursor")
//!         .add_icon_item(&icons::DOT, "Dot", "Dot cursor")
//!         .add_toggle(ToggleConfig {
//!             label: "Enable feature".to_string(),
//!             value: feature_enabled,
//!         })
//!         .show();
//!     if let Some(action) = action {
//!         println!("{action:?}");
//!     }
//! }
//! ```

use super::actions::DrawingToolbarAction;
use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::styles::{focus::focus_ring_color, icons as icon_sizes};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::ext::UiExt;
use egui::{
    Align2, FontId, Frame, Id, Key, Modifiers, Popup, PopupCloseBehavior, Pos2, Rect, RectAlign,
    Response, Role, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
};

/// Height of a flyout row.
const ROW_HEIGHT: f32 = 28.0;

/// Builder for creating dropdown submenus with consistent styling.
///
/// This builder provides a fluent API for constructing submenus with icons,
/// text items, separators, and toggles. [`show`](Self::show) draws the rows into the `Ui` it
/// was created with; the drawing toolbar calls it inside its flyout.
pub struct SubmenuBuilder<'a> {
    ui: &'a mut Ui,
    items: Vec<SubmenuItem>,
    width: f32,
    toggle: Option<ToggleConfig<'a>>,
}

/// A single item in a submenu
#[derive(Default)]
pub struct SubmenuItem {
    /// Glyph displayed by the component.
    pub icon: Option<&'static Icon>,
    /// Text displayed by the component.
    pub label: String,
    /// Help text displayed on hover.
    pub tooltip: String,
    /// Draw the row in the accent fill.
    pub is_sel: bool,
    /// Whether the entry is marked as a favorite.
    pub is_favorite: bool,
    /// Show a check mark in the right column.
    pub is_checked: bool,
    /// Draw the row in the error colour, for an action that destroys data.
    pub is_danger: bool,
    /// Callback invoked when the row is clicked, optionally returning an action.
    pub on_click: Option<Box<dyn Fn() -> Option<DrawingToolbarAction>>>,
}

/// Configuration for a toggle checkbox at the bottom of a submenu
pub struct ToggleConfig<'a> {
    /// Text displayed by the component.
    pub label: String,
    /// Mutable toggle state.
    pub value: &'a mut bool,
}

impl<'a> SubmenuBuilder<'a> {
    /// Create a new submenu builder
    ///
    /// # Arguments
    ///
    /// * `ui` - The `Ui` the rows are drawn into
    pub fn new(ui: &'a mut Ui) -> Self {
        Self {
            ui,
            items: Vec::new(),
            width: DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_xl,
            toggle: None,
        }
    }

    /// Set the minimum submenu width; a longer row widens it.
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Add an item with icon, label, and tooltip
    pub fn add_icon_item(
        mut self,
        icon: &'static Icon,
        label: impl Into<String>,
        tooltip: impl Into<String>,
    ) -> Self {
        self.items.push(SubmenuItem {
            icon: Some(icon),
            label: label.into(),
            tooltip: tooltip.into(),
            ..Default::default()
        });
        self
    }

    /// Add a text-only item (no icon)
    pub fn add_text_item(mut self, label: impl Into<String>, tooltip: impl Into<String>) -> Self {
        self.items.push(SubmenuItem {
            label: label.into(),
            tooltip: tooltip.into(),
            ..Default::default()
        });
        self
    }

    /// Add a text-only item with an action callback
    pub fn add_text_item_with_action<F>(
        mut self,
        label: impl Into<String>,
        tooltip: impl Into<String>,
        action: F,
    ) -> Self
    where
        F: Fn() -> Option<DrawingToolbarAction> + 'static,
    {
        self.items.push(SubmenuItem {
            label: label.into(),
            tooltip: tooltip.into(),
            on_click: Some(Box::new(action)),
            ..Default::default()
        });
        self
    }

    /// Draw the item added last with `icon`.
    #[must_use]
    pub(super) fn icon(mut self, icon: &'static Icon) -> Self {
        if let Some(item) = self.items.last_mut() {
            item.icon = Some(icon);
        }
        self
    }

    /// Show a check mark on the item added last when `checked`.
    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        if let Some(item) = self.items.last_mut() {
            item.is_checked = checked;
        }
        self
    }

    /// Draw the item added last with `icon`, in the error colour.
    #[must_use]
    pub fn danger(mut self, icon: &'static Icon) -> Self {
        if let Some(item) = self.items.last_mut() {
            item.icon = Some(icon);
            item.is_danger = true;
        }
        self
    }

    /// Add a toggle checkbox at the bottom of the submenu
    pub fn add_toggle(mut self, config: ToggleConfig<'a>) -> Self {
        self.toggle = Some(config);
        self
    }

    /// Draw the rows and return the action of the one clicked
    pub fn show(self) -> Option<DrawingToolbarAction> {
        let Self {
            ui,
            items,
            width,
            mut toggle,
        } = self;

        let rows: Vec<FlyoutRow> = items.iter().map(FlyoutRow::from).collect();
        let width = rows
            .iter()
            .map(|row| row_width(ui, row))
            .fold(width, f32::max);
        ui.set_min_width(width);

        let mut result = None;
        for (item, row) in items.iter().zip(rows) {
            let response = flyout_row(ui, width, &row).row;
            if response.clicked()
                && let Some(on_click) = &item.on_click
            {
                result = on_click();
            }
            response.on_hover_text(l10n::text(ui.ctx(), &item.tooltip));
        }

        if let Some(toggle) = &mut toggle {
            ui.spaced_separator();
            ui.checkbox(toggle.value, l10n::text(ui.ctx(), &toggle.label));
        }

        result
    }
}

/// One row of a flyout.
#[derive(Default)]
pub(super) struct FlyoutRow<'a> {
    pub icon: Option<&'a Icon>,
    pub label: &'a str,
    /// Shortcut text, right aligned.
    pub shortcut: Option<&'a str>,
    /// Accent fill, with the text in the accent text colour.
    pub selected: bool,
    /// A check mark in the right column.
    pub check: bool,
    pub danger: bool,
    /// A favourite star in the right column: `Some(is_favorite)`. A favourite always shows
    /// it; any other row shows an outline star while the pointer is over the row.
    pub star: Option<bool>,
}

impl<'a> From<&'a SubmenuItem> for FlyoutRow<'a> {
    fn from(item: &'a SubmenuItem) -> Self {
        Self {
            icon: item.icon,
            label: &item.label,
            selected: item.is_sel,
            check: item.is_checked,
            danger: item.is_danger,
            ..Default::default()
        }
    }
}

/// What a flyout row reports: the row, and its star when it has one.
pub(super) struct RowResponse {
    pub row: Response,
    pub star: Option<Response>,
}

/// Gap between the columns of a row, and its padding at both ends.
fn gap(ui: &Ui) -> f32 {
    ui.active_theme().spacing.lg
}

/// The width a row needs: icon, label, shortcut and the right column, with a gap between
/// each.
pub(super) fn row_width(ui: &Ui, row: &FlyoutRow) -> f32 {
    let gap = gap(ui);
    let theme = ui.active_theme();
    let text_width = |text: &str, size: f32| {
        ui.painter()
            .layout_no_wrap(
                l10n::text(ui.ctx(), text),
                FontId::proportional(size),
                theme.ui.text,
            )
            .size()
            .x
    };
    let icon = row.icon.map_or(0.0, |_| icon_sizes::SM + gap);
    let shortcut = row
        .shortcut
        .map_or(0.0, |s| gap + text_width(s, theme.text.small));
    gap + icon + text_width(row.label, theme.text.body) + shortcut + gap + icon_sizes::SM + gap
}

/// Draw one flyout row `width` wide and return its responses.
pub(super) fn flyout_row(ui: &mut Ui, width: f32, row: &FlyoutRow) -> RowResponse {
    let theme = ui.active_theme();
    let gap = gap(ui);
    let (rect, row_response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let translated_label = l10n::text(ui.ctx(), row.label);
    row_response.widget_info(|| {
        WidgetInfo::selected(
            Role::MenuItem,
            row_response.enabled(),
            row.selected || row.check,
            &translated_label,
        )
    });

    let slot = |right_edge: f32| {
        Rect::from_center_size(
            Pos2::new(right_edge - icon_sizes::SM / 2.0, rect.center().y),
            Vec2::splat(icon_sizes::SM),
        )
    };
    let right_slot = slot(rect.right() - gap);
    // The star answers its own clicks, so the row's click never selects the tool.
    let star_response = row.star.map(|favorite| {
        let response = ui.interact(right_slot, row_response.id.with("star"), Sense::click());
        let translated_label =
            l10n::format_text(ui.ctx(), "Favourite {0}", &[("0", row.label.to_string())]);
        response.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                response.enabled(),
                favorite,
                &translated_label,
            )
        });
        response
    });
    let hovered = row_response.hovered() || star_response.as_ref().is_some_and(Response::hovered);

    if ui.is_rect_visible(rect) {
        let radius = ui.active_theme().radius.sm;
        let painter = ui.painter();
        let fill = match (row.selected, hovered) {
            (true, true) => Some(theme.ui.accent_hover),
            (true, false) => Some(theme.ui.accent),
            (false, true) => Some(ui.visuals().widgets.hovered.bg_fill),
            (false, false) => None,
        };
        if let Some(fill) = fill {
            painter.rect_filled(rect, radius, fill);
        }
        let (text, secondary) = if row.selected {
            (theme.ui.accent_text, theme.ui.accent_text)
        } else if row.danger {
            (theme.ui.error, theme.ui.error)
        } else {
            (theme.ui.text, theme.ui.text_secondary)
        };

        let mut x = rect.left() + gap;
        if let Some(icon) = row.icon {
            let icon_rect = Rect::from_min_size(
                Pos2::new(x, rect.center().y - icon_sizes::SM / 2.0),
                Vec2::splat(icon_sizes::SM),
            );
            icon.as_image_tinted(icon_rect.size(), text)
                .paint_at(ui, icon_rect);
            x += icon_sizes::SM + gap;
        }
        ui.painter().text(
            Pos2::new(x, rect.center().y),
            Align2::LEFT_CENTER,
            l10n::text(ui.ctx(), row.label),
            FontId::proportional(theme.text.body),
            text,
        );
        if let Some(shortcut) = row.shortcut {
            ui.painter().text(
                Pos2::new(right_slot.left() - gap, rect.center().y),
                Align2::RIGHT_CENTER,
                l10n::text(ui.ctx(), shortcut),
                FontId::proportional(theme.text.small),
                secondary,
            );
        }
        match row.star {
            Some(true) => star(ui, &icons::STAR_FILLED, right_slot, theme.ui.warning, row),
            Some(false) if hovered => {
                star(ui, &icons::STAR_EMPTY, right_slot, secondary, row);
            }
            _ if row.check => {
                icons::CHECK
                    .as_image_tinted(right_slot.size(), text)
                    .paint_at(ui, right_slot);
            }
            _ => {}
        }
        if row_response.has_focus() {
            let (ring, ring_rect) = if row.selected {
                (
                    theme.ui.accent_text,
                    rect.shrink(DESIGN_TOKENS.stroke.thick),
                )
            } else {
                (focus_ring_color(ui), rect)
            };
            ui.painter().rect_stroke(
                ring_rect,
                radius,
                Stroke::new(DESIGN_TOKENS.stroke.thick, ring),
                StrokeKind::Inside,
            );
        }
    }

    RowResponse {
        row: row_response,
        star: star_response,
    }
}

/// Paint a favourite star; on a selected row it takes the accent text colour so it stays
/// readable on the accent fill.
fn star(ui: &Ui, icon: &Icon, rect: Rect, color: egui::Color32, row: &FlyoutRow) {
    let color = if row.selected {
        ui.active_theme().ui.accent_text
    } else {
        color
    };
    icon.as_image_tinted(rect.size(), color).paint_at(ui, rect);
}

/// A section header: small upper-case text with a gap above.
pub(super) fn section_header(ui: &mut Ui, text: &str, width: f32) {
    let theme = ui.active_theme();
    let gap = gap(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(width, gap + theme.text.small + gap / 2.0),
        Sense::hover(),
    );
    ui.painter().text(
        Pos2::new(rect.left() + gap, rect.bottom() - gap / 2.0),
        Align2::LEFT_BOTTOM,
        l10n::text(ui.ctx(), text),
        FontId::proportional(theme.text.small),
        theme.ui.text_secondary,
    );
}

/// The popup id of the flyout for `category`.
pub(super) fn flyout_id(category: &str) -> Id {
    Id::unique(("drawing_toolbar_flyout", category))
}

/// Show `content` in the flyout of `category`, right of `anchor` and moved up or down to stay
/// `spacing.lg` inside the window. It scrolls when the window is shorter than its content, and
/// closes on an outside click or Escape; the caller reads that with
/// `Popup::is_id_open(ctx, flyout_id(category))`. Open it with `Popup::open_id` first.
pub(super) fn show_flyout<R>(
    ui: &Ui,
    category: &str,
    anchor: Rect,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let ctx = ui.ctx();
    let lg = ui.active_theme().spacing.lg;
    let window = ctx.content_rect();
    let max_height = window.height() - 2.0 * lg - ui.style().spacing.menu_margin.sum().y;
    let popup = Popup::new(flyout_id(category), ctx.clone(), anchor, ui.layer_id())
        .open_memory(None)
        .align(RectAlign::RIGHT_START)
        .align_alternatives(&[])
        .gap(ui.active_theme().spacing.sm)
        .frame(Frame::menu(ui.style()))
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
    // Known from the last frame: how far down the flyout reaches from the button's top.
    let height = popup.get_expected_size().map_or(0.0, |size| size.y);
    let top = anchor
        .min
        .y
        .min(window.max.y - lg - height)
        .max(window.min.y + lg);
    let shown = popup
        .anchor(anchor.translate(Vec2::new(0.0, top - anchor.min.y)))
        .show(|ui| {
            // Rows sit flush, so a row's pitch is its height.
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // An area offers 400 px by default; a tall list may use the window's height.
            ui.set_max_height(max_height);
            ScrollArea::vertical()
                .max_height(max_height)
                .show(ui, content)
                .inner
        })
        .map(|response| response.inner);
    // Escape closes the flyout and nothing else, so the chart keeps its drawing state.
    if !Popup::is_id_open(ctx, flyout_id(category)) {
        ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    }
    shown
}
