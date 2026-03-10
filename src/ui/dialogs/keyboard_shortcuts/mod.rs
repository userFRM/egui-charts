//! Keyboard shortcuts help dialog.
//!
//! Displays all available keyboard shortcuts organized by category
//! in a scrollable grid layout. Triggered by `?` or `F1`.

use egui::{Context, RichText, Vec2, epaint::StrokeKind};

use crate::config::KeyboardAction;
use crate::ext::UiExt;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};

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

        DialogFrame::new(
            "Keyboard Shortcuts",
            Vec2::new(
                DESIGN_TOKENS.sizing.dialog.default_width,
                DESIGN_TOKENS.sizing.dialog.default_height,
            ),
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

        // Close on Escape
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.is_open = false;
            action = KeyboardShortcutsAction::Close;
        }

        action
    }

    fn render_contents(&mut self, ui: &mut egui::Ui) -> KeyboardShortcutsAction {
        let mut action = KeyboardShortcutsAction::None;

        // Title bar
        if dialog_header(ui, "Keyboard Shortcuts") {
            action = KeyboardShortcutsAction::Close;
            self.is_open = false;
        }
        ui.separator();

        // Scrollable content area
        egui::ScrollArea::vertical()
            .max_height(DESIGN_TOKENS.sizing.dialog.default_height - 80.0)
            .show(ui, |ui| {
                ui.space_md();

                let categories = KeyboardAction::all_by_category();

                for (idx, (category_name, actions)) in categories.iter().enumerate() {
                    if idx > 0 {
                        ui.space_md();
                    }

                    self.render_category(ui, category_name, actions);
                }

                ui.space_lg();
            });

        action
    }

    fn render_category(&self, ui: &mut egui::Ui, name: &str, actions: &[KeyboardAction]) {
        // Section header
        ui.horizontal(|ui| {
            ui.space_lg();
            ui.label(
                RichText::new(name)
                    .size(typography::LG)
                    .strong()
                    .color(ui.style().visuals.text_color()),
            );
        });
        ui.space_xs();

        // Shortcut rows in a grid
        egui::Grid::new(format!("shortcuts_{name}"))
            .num_columns(2)
            .spacing([DESIGN_TOKENS.spacing.xl, DESIGN_TOKENS.spacing.sm])
            .min_col_width(DESIGN_TOKENS.sizing.settings_dialog.label_width)
            .show(ui, |ui| {
                for action in actions {
                    // Action label
                    ui.horizontal(|ui| {
                        ui.space_xl();
                        ui.label(
                            RichText::new(action.label())
                                .size(typography::MD)
                                .color(ui.style().visuals.text_color()),
                        );
                    });

                    // Shortcut key badge
                    ui.horizontal(|ui| {
                        self.render_key_badge(ui, action.shortcut_key());
                    });

                    ui.end_row();
                }
            });
    }

    fn render_key_badge(&self, ui: &mut egui::Ui, key_text: &str) {
        // Split compound keys (e.g. "Ctrl+Shift+Z") and render each as
        // a separate badge when separated by " / " (alternative keys).
        let alternatives: Vec<&str> = key_text.split(" / ").collect();

        for (alt_idx, alt) in alternatives.iter().enumerate() {
            if alt_idx > 0 {
                ui.label(
                    RichText::new("/")
                        .size(typography::SM)
                        .color(ui.style().visuals.weak_text_color()),
                );
            }

            let parts: Vec<&str> = alt.split('+').collect();
            for (part_idx, part) in parts.iter().enumerate() {
                if part_idx > 0 {
                    ui.label(
                        RichText::new("+")
                            .size(typography::SM)
                            .color(ui.style().visuals.weak_text_color()),
                    );
                }

                let badge_text = RichText::new(*part)
                    .size(typography::SM)
                    .color(ui.style().visuals.text_color());

                let badge = egui::Label::new(badge_text);
                let response = ui.add(badge);

                // Draw a rounded background behind the key label
                let badge_rect = response.rect.expand(DESIGN_TOKENS.spacing.xs);
                let badge_rounding = DESIGN_TOKENS.rounding.sm;
                let badge_bg = ui.style().visuals.widgets.inactive.bg_fill;
                let badge_stroke = ui.style().visuals.widgets.inactive.bg_stroke;

                // Paint behind the text
                ui.painter().rect(
                    badge_rect,
                    badge_rounding,
                    badge_bg,
                    badge_stroke,
                    StrokeKind::Inside,
                );
            }
        }
    }
}
