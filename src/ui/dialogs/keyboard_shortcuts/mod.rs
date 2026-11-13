//! Keyboard shortcuts help dialog.
//!
//! Lists the handled keyboard shortcuts by category. The host opens it on `?` or `F1`.

use crate::localization as l10n;
use egui::{
    Color32, Context, FontId, Frame, Id, Key, KeyboardShortcut, Margin, Modifiers, RichText, Role,
    ScrollArea, Stroke, Ui, WidgetInfo,
};

use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};
use crate::widget::ShortcutMap;

/// One key combination of a listed entry.
enum Keys {
    /// A combination of the chart's shortcut map or of a fixed key.
    Shortcut(KeyboardShortcut),
    /// Keys typed as text, which no shortcut describes.
    Typed(&'static str),
}

/// One listed entry: a label and its key combinations.
struct Entry {
    label: String,
    keys: Vec<Keys>,
}

/// The keys the chart and the library's controls read outside the shortcut map, by category.
///
/// They are not rebindable, so the list is fixed. The state an entry needs is in its label or
/// its category.
fn fixed_categories() -> Vec<(&'static str, Vec<Entry>)> {
    let entry = |label: &str, keys: Vec<Keys>| Entry {
        label: label.to_owned(),
        keys,
    };
    let key = |key, modifiers| Keys::Shortcut(KeyboardShortcut::new(modifiers, key));
    let none = Modifiers::NONE;
    vec![
        (
            "Typing",
            vec![
                entry("Change the symbol", vec![Keys::Typed("Type a ticker")]),
                entry(
                    "Change the interval",
                    vec![Keys::Typed("0-9"), Keys::Typed(",")],
                ),
            ],
        ),
        (
            "Drawings",
            vec![
                entry(
                    "Move the selected drawing one bar earlier",
                    vec![key(Key::ArrowLeft, none)],
                ),
                entry(
                    "Move the selected drawing one bar later",
                    vec![key(Key::ArrowRight, none)],
                ),
                entry(
                    "Move the selected drawing up one tick",
                    vec![key(Key::ArrowUp, none)],
                ),
                entry(
                    "Move the selected drawing down one tick",
                    vec![key(Key::ArrowDown, none)],
                ),
            ],
        ),
        (
            "Replay",
            vec![
                entry("Play or pause the replay", vec![key(Key::Space, none)]),
                entry("Go to the replay start", vec![key(Key::Home, none)]),
                entry("Go to the replay end", vec![key(Key::End, none)]),
                entry(
                    "Replay one bar back",
                    vec![
                        key(Key::ArrowLeft, none),
                        key(Key::ArrowLeft, Modifiers::SHIFT),
                    ],
                ),
                entry(
                    "Replay one bar forward",
                    vec![
                        key(Key::ArrowRight, none),
                        key(Key::ArrowRight, Modifiers::SHIFT),
                    ],
                ),
            ],
        ),
    ]
}

/// Action returned by the keyboard shortcuts dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Create a [`KeyboardShortcutsDialog`] with default settings.
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
        self.show_with_shortcuts(ctx, &ShortcutMap::default())
    }

    /// Show only the host's bound shortcuts, using the same map as its chart.
    pub fn show_with_shortcuts(
        &mut self,
        ctx: &Context,
        shortcuts: &ShortcutMap,
    ) -> KeyboardShortcutsAction {
        if !self.is_open {
            return KeyboardShortcutsAction::None;
        }

        let blocked_now = ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);
        let mut action = KeyboardShortcutsAction::None;

        DialogFrame::with_width(
            "Keyboard Shortcuts",
            DESIGN_TOKENS.sizing.dialog.default_width,
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui, shortcuts);
        });

        // Escape belongs to an open popup or a focused text field first. egui closes both on
        // Escape before this runs, so their state is read from the previous frame. Consuming the
        // key keeps whatever reads input after the dialog from acting on the same press.
        let blocked_id = Id::NULL.with("keyboard_shortcuts_escape_blocked");
        let blocked = ctx
            .data(|d| d.get_temp::<bool>(blocked_id))
            .unwrap_or(false);
        if !blocked
            && !blocked_now
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.is_open = false;
            action = KeyboardShortcutsAction::Close;
        }
        let blocked = ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);
        ctx.data_mut(|d| d.insert_temp(blocked_id, blocked));

        action
    }

    fn render_contents(&mut self, ui: &mut Ui, shortcuts: &ShortcutMap) -> KeyboardShortcutsAction {
        let mut action = KeyboardShortcutsAction::None;
        let theme = ui.active_theme();

        if dialog_header(ui, "Keyboard Shortcuts") {
            action = KeyboardShortcutsAction::Close;
            self.is_open = false;
        }
        // The chart reads its keys only while it has keyboard focus.
        ui.label(
            RichText::new(l10n::text(
                ui.ctx(),
                "Click the chart to use the chart keys.",
            ))
            .size(theme.text.small)
            .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(theme.spacing.lg);

        let categories: Vec<(&str, Vec<Entry>)> = shortcuts
            .by_category()
            .into_iter()
            .map(|(name, actions)| {
                let entries = actions
                    .into_iter()
                    .map(|action| Entry {
                        label: action.label().to_owned(),
                        keys: shortcuts.keys_for(action).map(Keys::Shortcut).collect(),
                    })
                    .collect();
                (name, entries)
            })
            .chain(fixed_categories())
            .collect();
        let font = FontId::proportional(theme.text.body);
        let labels: Vec<_> = categories
            .iter()
            .flat_map(|(_, entries)| entries)
            .map(|entry| l10n::text(ui.ctx(), &entry.label))
            .collect();
        let widest = ui.fonts_mut(|f| {
            labels
                .into_iter()
                .map(|label| {
                    f.layout_no_wrap(label, font.clone(), Color32::PLACEHOLDER)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
        });
        let label_w = (((widest + theme.spacing.xxl) / 4.0).ceil() * 4.0)
            .min((ui.available_width() - theme.spacing.sm) / 2.0);

        ui.scope(|ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    for (idx, (name, entries)) in categories.iter().enumerate() {
                        if idx > 0 {
                            ui.add_space(theme.spacing.xxl - ui.spacing().item_spacing.y);
                        }
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), *name))
                                .size(theme.text.small)
                                .strong()
                                .color(theme.ui.text_secondary),
                        );
                        egui::Grid::new(("shortcuts", idx))
                            .num_columns(2)
                            .spacing([theme.spacing.sm, theme.spacing.sm])
                            // With the spacing, a 24 px row pitch.
                            .min_row_height(theme.spacing.xxl + theme.spacing.sm)
                            .min_col_width(label_w)
                            .max_col_width(label_w)
                            .show(ui, |ui| {
                                for entry in entries {
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), &entry.label))
                                            .size(theme.text.body),
                                    );
                                    ui.horizontal_wrapped(|ui| key_badges(ui, &theme, &entry.keys));
                                    ui.end_row();
                                }
                            });
                    }
                });
        });

        action
    }
}

/// Labels of a shortcut's keys: its modifiers, then the key's symbol where `font` has it.
///
/// `format_shortcut` spells every key out ("Plus", "PageUp") with the bundled fonts.
pub(crate) fn shortcut_key_labels(
    ctx: &Context,
    font: &FontId,
    shortcut: &KeyboardShortcut,
) -> Vec<String> {
    let symbol = shortcut.logical_key.symbol_or_name();
    let key = if ctx.fonts_mut(|f| f.has_glyphs(font, symbol)) {
        symbol
    } else {
        shortcut.logical_key.name()
    };
    ctx.format_modifiers(shortcut.modifiers)
        .split('+')
        .filter(|modifier| !modifier.is_empty())
        .chain([key])
        .map(str::to_owned)
        .collect()
}

/// One badge per key combination, joined by a muted "or".
fn key_badges(ui: &mut Ui, theme: &Theme, keys: &[Keys]) {
    for (idx, combination) in keys.iter().enumerate() {
        if idx > 0 {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "or"))
                    .size(theme.text.small)
                    .color(ui.visuals().weak_text_color()),
            );
        }
        // Screen readers get the name.
        let (text, name) = match combination {
            Keys::Shortcut(shortcut) => {
                let font = FontId::proportional(theme.text.small);
                (
                    shortcut_key_labels(ui.ctx(), &font, shortcut).join("+"),
                    ui.ctx().format_shortcut(shortcut),
                )
            }
            Keys::Typed(text) => {
                let text = l10n::text(ui.ctx(), text);
                (text.clone(), text)
            }
        };
        Frame::new()
            .fill(theme.ui.panel_bg_secondary)
            .stroke(Stroke::new(1.0, theme.ui.border_control))
            .corner_radius(theme.radius.sm)
            .inner_margin(Margin::symmetric(
                theme.spacing.sm as i8,
                theme.spacing.xs as i8,
            ))
            .show(ui, |ui| {
                let translated_label = l10n::text(ui.ctx(), &name);
                ui.add(
                    egui::Label::new(
                        RichText::new(l10n::text(ui.ctx(), text))
                            .size(theme.text.small)
                            .color(theme.ui.text),
                    )
                    .wrap_mode(egui::TextWrapMode::Extend),
                )
                .widget_info(|| WidgetInfo::labeled(Role::Label, true, &translated_label));
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable as _;

    #[test]
    fn a_popup_opened_this_frame_gets_escape_before_the_dialog() {
        let ctx = Context::default();
        let mut dialog = KeyboardShortcutsDialog::new();
        dialog.open();
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                egui::Popup::open_id(ui.ctx(), Id::NULL.with("test popup"));
                assert_eq!(dialog.show(ui.ctx()), KeyboardShortcutsAction::None);
                assert!(dialog.is_open);
                assert!(ui.input(|i| i.key_pressed(Key::Escape)));
            },
        )
        .drop_without_applying_deltas();
    }

    /// Guards #2811: the list also shows the keys read outside the shortcut map, so it is a
    /// complete guide to the keyboard.
    #[test]
    fn the_list_shows_typed_entry_drawing_arrows_and_replay_keys() {
        let mut dialog = KeyboardShortcutsDialog::new();
        dialog.open();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 2400.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &Theme::dark());
            })
            .build_ui_state(
                |ui, d: &mut KeyboardShortcutsDialog| _ = d.show(ui.ctx()),
                dialog,
            );
        h.run_steps(3);
        for label in [
            "Change the symbol",
            "Change the interval",
            "Move the selected drawing one bar earlier",
            "Move the selected drawing down one tick",
            "Play or pause the replay",
            "Replay one bar forward",
        ] {
            assert!(h.query_by_label(label).is_some(), "missing {label}");
        }
    }
}
