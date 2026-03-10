//! Drawing context menu - right-click menu when drawing is selected.
//!
//! Provides actions for editing, layering, locking, and deleting drawings.

use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::Theme;
use crate::theme::components::ContextMenuStyle;

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Context, FontId, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

/// Actions from the drawing context menu
#[derive(Debug, Clone, PartialEq)]
pub enum DrawingContextMenuAction {
    None,
    /// Open drawing settings dialog
    Edit,
    /// Bring drawing to front of render order
    BringToFront,
    /// Send drawing to back of render order
    SendToBack,
    /// Toggle drawing lock state
    Lock,
    /// Toggle drawing visibility
    Hide,
    /// Copy drawing to clipboard
    Copy,
    /// Clone drawing with offset
    Clone,
    /// Delete drawing
    Delete,
}

/// Menu item with icon, shortcut, and submenu support
#[derive(Clone)]
struct MenuItem {
    label: String,
    shortcut: Option<String>,
    icon: Option<&'static Icon>,
    action: DrawingContextMenuAction,
    enabled: bool,
    is_toggle: bool,
    toggle_state: bool,
}

impl MenuItem {
    fn new(label: impl Into<String>, action: DrawingContextMenuAction) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            icon: None,
            action,
            enabled: true,
            is_toggle: false,
            toggle_state: false,
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

    /// Mark item as disabled (greyed out, non-clickable).
    /// Used for conditional menu items (e.g., "Copy" when clipboard unavailable).
    #[allow(dead_code)]
    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn toggle(mut self, state: bool) -> Self {
        self.is_toggle = true;
        self.toggle_state = state;
        self
    }
}

/// Menu entry - either an item or separator
#[derive(Clone)]
enum MenuEntry {
    Item(MenuItem),
    Separator,
}

/// Drawing context menu state
pub struct DrawingContextMenu {
    is_open: bool,
    position: Pos2,
    /// Drawing ID that was right-clicked
    pub drawing_id: Option<usize>,
    /// Drawing type name for display
    pub drawing_type: String,
    /// Whether the drawing is currently locked
    pub is_locked: bool,
    /// Whether the drawing is currently visible
    pub is_visible: bool,
    /// Last action taken
    pub last_action: DrawingContextMenuAction,
    /// Skip close detection on first frame
    just_opened: bool,
}

impl Default for DrawingContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingContextMenu {
    /// Create new context menu
    pub fn new() -> Self {
        Self {
            is_open: false,
            position: Pos2::ZERO,
            drawing_id: None,
            drawing_type: "Drawing".to_string(),
            is_locked: false,
            is_visible: true,
            last_action: DrawingContextMenuAction::None,
            just_opened: false,
        }
    }

    /// Open the context menu
    pub fn open(&mut self, pos: Pos2, drawing_id: usize) {
        self.is_open = true;
        self.position = pos;
        self.drawing_id = Some(drawing_id);
        self.last_action = DrawingContextMenuAction::None;
        self.just_opened = true;
    }

    /// Set drawing context for dynamic menu items
    pub fn set_context(&mut self, drawing_type: impl Into<String>, locked: bool, visible: bool) {
        self.drawing_type = drawing_type.into();
        self.is_locked = locked;
        self.is_visible = visible;
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
    pub fn take_action(&mut self) -> DrawingContextMenuAction {
        std::mem::replace(&mut self.last_action, DrawingContextMenuAction::None)
    }

    /// Build menu items for the drawing context menu
    fn build_menu_items(&self) -> Vec<MenuEntry> {
        vec![
            // Edit section
            MenuEntry::Item(
                MenuItem::new("Edit...", DrawingContextMenuAction::Edit)
                    .with_icon(&embedded_icons::SETTINGS_GEAR)
                    .with_shortcut("Cmd E"),
            ),
            MenuEntry::Separator,
            // Layer ordering
            MenuEntry::Item(
                MenuItem::new("Bring to front", DrawingContextMenuAction::BringToFront)
                    .with_shortcut("Cmd ]"),
            ),
            MenuEntry::Item(
                MenuItem::new("Send to back", DrawingContextMenuAction::SendToBack)
                    .with_shortcut("Cmd ["),
            ),
            MenuEntry::Separator,
            // State toggles
            MenuEntry::Item(
                MenuItem::new(
                    if self.is_locked { "Unlock" } else { "Lock" },
                    DrawingContextMenuAction::Lock,
                )
                .with_icon(if self.is_locked {
                    &embedded_icons::UNLOCK
                } else {
                    &embedded_icons::LOCK
                })
                .toggle(self.is_locked),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    if self.is_visible { "Hide" } else { "Show" },
                    DrawingContextMenuAction::Hide,
                )
                .with_icon(if self.is_visible {
                    &embedded_icons::EYE_HIDE
                } else {
                    &embedded_icons::HIDE
                })
                .toggle(!self.is_visible),
            ),
            MenuEntry::Separator,
            // Clipboard operations
            MenuEntry::Item(
                MenuItem::new("Copy", DrawingContextMenuAction::Copy).with_shortcut("Cmd C"),
            ),
            MenuEntry::Item(
                MenuItem::new("Clone", DrawingContextMenuAction::Clone).with_shortcut("Cmd D"),
            ),
            MenuEntry::Separator,
            // Delete (last item, potentially destructive)
            MenuEntry::Item(
                MenuItem::new("Delete", DrawingContextMenuAction::Delete)
                    .with_icon(&embedded_icons::TRASH)
                    .with_shortcut("Del"),
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
        let mut sel_action = DrawingContextMenuAction::None;

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
            .unwrap_or(15);

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
        let pos = self.clamp_position_to_screen(screen_rect, menu_width, menu_height);

        egui::Area::new(egui::Id::new("drawing_context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let menu_rect = Rect::from_min_size(pos, Vec2::new(menu_width, menu_height));
                self.draw_menu_background(ui, menu_rect, style);
                self.draw_menu_items(
                    ui,
                    menu_rect,
                    &menu_items,
                    style,
                    menu_width,
                    &mut sel_action,
                    &mut should_close,
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

    fn clamp_position_to_screen(&self, screen_rect: Rect, width: f32, height: f32) -> Pos2 {
        let mut pos = self.position;
        if pos.x + width > screen_rect.max.x {
            pos.x =
                (screen_rect.max.x - width - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                    .max(screen_rect.min.x);
        }
        if pos.y + height > screen_rect.max.y {
            pos.y = (screen_rect.max.y
                - height
                - DESIGN_TOKENS.sizing.context_menu.screen_edge_padding)
                .max(screen_rect.min.y);
        }
        if pos.x < screen_rect.min.x {
            pos.x = screen_rect.min.x + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }
        if pos.y < screen_rect.min.y {
            pos.y = screen_rect.min.y + DESIGN_TOKENS.sizing.context_menu.screen_edge_padding;
        }
        pos
    }

    fn draw_menu_background(&self, ui: &mut Ui, menu_rect: Rect, style: &ContextMenuStyle) {
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
    }

    fn draw_menu_items(
        &self,
        ui: &mut Ui,
        menu_rect: Rect,
        menu_items: &[MenuEntry],
        style: &ContextMenuStyle,
        menu_width: f32,
        sel_action: &mut DrawingContextMenuAction,
        should_close: &mut bool,
    ) {
        ui.scope_builder(
            UiBuilder::new().max_rect(
                menu_rect.shrink2(Vec2::new(0.0, DESIGN_TOKENS.sizing.context_menu.padding_v)),
            ),
            |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    for entry in menu_items {
                        match entry {
                            MenuEntry::Item(item) => {
                                if let Some(action) =
                                    Self::draw_menu_item(ui, item, style, menu_width)
                                {
                                    *sel_action = action;
                                    *should_close = true;
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
    }

    fn draw_menu_item(
        ui: &mut Ui,
        item: &MenuItem,
        style: &ContextMenuStyle,
        menu_width: f32,
    ) -> Option<DrawingContextMenuAction> {
        let desired_size = Vec2::new(menu_width, DESIGN_TOKENS.sizing.context_menu.item_height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

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

        if bg_color != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.none, bg_color);
        }

        // Icon
        if let Some(icon) = &item.icon {
            let icon_rect = Rect::from_min_size(
                Pos2::new(
                    rect.min.x + DESIGN_TOKENS.sizing.context_menu.item_padding_h,
                    rect.center().y - DESIGN_TOKENS.sizing.context_menu.icon_size / 2.0,
                ),
                Vec2::splat(DESIGN_TOKENS.sizing.context_menu.icon_size),
            );
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

        // Shortcut
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

        if response.clicked() && item.enabled {
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
