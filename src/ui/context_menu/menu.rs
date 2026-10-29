use super::ContextMenuAction;
use crate::icons::Icon;
use crate::localization as l10n;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, Context, FontId, Galley, Id, KeyboardShortcut, Pos2, Rect, Response, Sense, Stroke,
    Ui, Vec2, WidgetInfo,
};
use std::sync::Arc;

// ============================================================================
// MENU ITEM DEFINITION
// ============================================================================

/// One row of a context menu, generic over the action it resolves to.
///
/// The chart, series and drawing menus all build their rows from this type.
#[derive(Clone)]
pub struct MenuItem<A = ContextMenuAction> {
    /// Text displayed by the component.
    pub label: String,
    /// Shown right-aligned, formatted for the platform with [`Context::format_shortcut`].
    pub shortcut: Option<KeyboardShortcut>,
    /// Glyph displayed by the component.
    pub icon: Option<&'static Icon>,
    /// Action returned when this entry is activated.
    pub action: A,
    /// Whether the entry can be activated.
    pub enabled: bool,
    /// Destructive rows paint in the theme's danger colours.
    pub danger: bool,
}

impl<A> MenuItem<A> {
    /// Create an enabled menu entry with a label and the action it returns.
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

    /// Set the keyboard shortcut displayed beside the label.
    pub fn with_shortcut(mut self, shortcut: impl Into<Option<KeyboardShortcut>>) -> Self {
        self.shortcut = shortcut.into();
        self
    }

    /// Set the entry's icon.
    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Prevent this entry from being activated.
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
        let mut job = LayoutJob::simple_singleline(
            l10n::text(ctx, text),
            FontId::proportional(size),
            Color32::PLACEHOLDER,
        );
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
    let translated_label = l10n::text(ui.ctx(), &item.label);
    response.widget_info(|| {
        WidgetInfo::labeled(
            egui::Role::MenuItem,
            response.enabled() && item.enabled,
            &translated_label,
        )
    });

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyboardAction;

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
}
