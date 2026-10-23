//! Keyboard shortcuts help dialog.
//!
//! Lists the handled keyboard shortcuts by category. The host opens it on `?` or `F1`.

use egui::{
    Color32, Context, FontId, Frame, Id, Key, KeyboardShortcut, Margin, Modifiers, RichText, Role,
    ScrollArea, Stroke, Ui, WidgetInfo,
};

use crate::config::KeyboardAction;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{DialogFrame, dialog_frame, dialog_header};

/// Action returned by the keyboard shortcuts dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardShortcutsAction {
    /// No action
    None,
    /// Dialog was closed
    Close,
}

/// Keyboard shortcuts help dialog
pub struct KeyboardShortcutsDialog {
    /// Whether the dialog is currently open
    pub is_open: bool,
}

impl Default for KeyboardShortcutsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardShortcutsDialog {
    pub fn new() -> Self {
        Self { is_open: false }
    }

    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Toggle the dialog open/closed
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Show the dialog. Returns the action taken.
    pub fn show(&mut self, ctx: &Context) -> KeyboardShortcutsAction {
        if !self.is_open {
            return KeyboardShortcutsAction::None;
        }

        let mut action = KeyboardShortcutsAction::None;

        DialogFrame::with_width(
            "Keyboard Shortcuts",
            DESIGN_TOKENS.sizing.dialog.default_width,
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

        // Escape belongs to an open popup or a focused text field first. egui closes both on
        // Escape before this runs, so their state is read from the previous frame. Consuming the
        // key keeps whatever reads input after the dialog from acting on the same press.
        let blocked_id = Id::NULL.with("keyboard_shortcuts_escape_blocked");
        let blocked = ctx
            .data(|d| d.get_temp::<bool>(blocked_id))
            .unwrap_or(false);
        if !blocked && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.is_open = false;
            action = KeyboardShortcutsAction::Close;
        }
        let blocked = ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);
        ctx.data_mut(|d| d.insert_temp(blocked_id, blocked));

        action
    }

    fn render_contents(&mut self, ui: &mut Ui) -> KeyboardShortcutsAction {
        let mut action = KeyboardShortcutsAction::None;
        let theme = ui.active_theme();

        if dialog_header(ui, "Keyboard Shortcuts") {
            action = KeyboardShortcutsAction::Close;
            self.is_open = false;
        }
        // The chart reads its keys only while it has keyboard focus.
        ui.label(
            RichText::new("Click the chart to use the chart keys.")
                .size(theme.text.small)
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(theme.spacing.lg);

        let categories = KeyboardAction::all_by_category();
        let font = FontId::proportional(theme.text.body);
        let widest = ui.fonts_mut(|f| {
            categories
                .iter()
                .flat_map(|(_, actions)| actions)
                .map(|a| {
                    f.layout_no_wrap(a.label().to_owned(), font.clone(), Color32::PLACEHOLDER)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
        });
        let label_w = ((widest + theme.spacing.xxl) / 4.0).ceil() * 4.0;

        // The body scrolls once the dialog would pass the frame's screen cap.
        let cap = ui.ctx().content_rect().shrink(theme.spacing.xxxl).height()
            - dialog_frame(ui.style()).total_margin().sum().y
            - ui.min_rect().height()
            - ui.spacing().item_spacing.y;

        ui.scope(|ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(cap)
                // The window starts at egui's default height; this lets the body grow past it.
                .min_scrolled_height(cap)
                .show(ui, |ui| {
                    for (idx, (name, actions)) in categories.iter().enumerate() {
                        if idx > 0 {
                            ui.add_space(theme.spacing.xxl - ui.spacing().item_spacing.y);
                        }
                        ui.label(
                            RichText::new(*name)
                                .size(theme.text.small)
                                .strong()
                                .color(theme.ui.text_secondary),
                        );
                        egui::Grid::new(("shortcuts", idx))
                            .num_columns(2)
                            .spacing([0.0, theme.spacing.sm])
                            // With the spacing, a 24 px row pitch.
                            .min_row_height(theme.spacing.xxl + theme.spacing.sm)
                            .min_col_width(label_w)
                            .show(ui, |ui| {
                                for action in actions {
                                    ui.label(RichText::new(action.label()).size(theme.text.body));
                                    ui.horizontal(|ui| key_badges(ui, &theme, action.shortcuts()));
                                    ui.end_row();
                                }
                            });
                    }
                });
        });

        action
    }
}

/// One badge per key combination, joined by a muted "or".
fn key_badges(ui: &mut Ui, theme: &Theme, shortcuts: &[KeyboardShortcut]) {
    for (idx, shortcut) in shortcuts.iter().enumerate() {
        if idx > 0 {
            ui.label(
                RichText::new("or")
                    .size(theme.text.small)
                    .color(ui.visuals().weak_text_color()),
            );
        }
        // `format_shortcut` spells every key out ("Plus", "PageUp") with the bundled fonts, so
        // the badge shows the key's symbol where a font has it, and screen readers get the name.
        let modifiers = ui.ctx().format_modifiers(shortcut.modifiers);
        let symbol = shortcut.logical_key.symbol_or_name();
        let font = FontId::proportional(theme.text.small);
        let key = if ui.fonts_mut(|f| f.has_glyphs(&font, symbol)) {
            symbol
        } else {
            shortcut.logical_key.name()
        };
        let text = if modifiers.is_empty() {
            key.to_owned()
        } else {
            format!("{modifiers}+{key}")
        };
        let name = ui.ctx().format_shortcut(shortcut);
        Frame::new()
            .fill(theme.ui.panel_bg_secondary)
            .stroke(Stroke::new(1.0, theme.ui.border_control))
            .corner_radius(theme.radius.sm)
            .inner_margin(Margin::symmetric(
                theme.spacing.sm as i8,
                theme.spacing.xs as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(text)
                        .size(theme.text.small)
                        .color(theme.ui.text),
                )
                .widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
            });
    }
}
