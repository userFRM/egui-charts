//! Keyboard shortcuts help dialog.
//!
//! Lists the handled keyboard shortcuts and pointer gestures by category. The host opens it on
//! `?` or `F1`. With `editable` set it also lets the user rebind the shortcuts and returns the
//! new map.

use crate::config::shortcut_map::{canonical, pressed};
use crate::config::{ChartOptions, KeyboardAction};
use crate::localization as l10n;
use egui::{
    Align, Color32, Context, Event, FontId, Frame, Id, Key, KeyboardShortcut, Layout, Margin,
    Modifiers, RichText, Role, ScrollArea, Stroke, Ui, WidgetInfo,
};

use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};
use crate::widget::keyboard::suppress_shortcuts_this_pass;
use crate::widget::{Chart, ShortcutMap};

/// One key combination of a listed entry.
enum Keys {
    /// A combination of the chart's shortcut map or of a fixed key.
    Shortcut(KeyboardShortcut),
    /// Keys typed as text, which no shortcut describes.
    Typed(&'static str),
    /// A pointer action after the modifiers it needs, such as "Double click". An empty action
    /// is a modifier held during another gesture.
    Pointer(Modifiers, &'static str),
}

/// One listed entry: a label and its key combinations.
struct Entry {
    label: String,
    keys: Vec<Keys>,
    /// The action the edit mode rebinds from this row.
    action: Option<KeyboardAction>,
}

/// The keys the chart and the library's controls read outside the shortcut map, by category.
///
/// They are not rebindable, so the list is fixed. The state an entry needs is in its label or
/// its category. A pointer gesture that `options` turns off is left out.
fn fixed_categories(options: &ChartOptions) -> Vec<(&'static str, Vec<Entry>)> {
    let entry = |label: &str, keys: Vec<Keys>| Entry {
        label: label.to_owned(),
        keys,
        action: None,
    };
    let key = |key, modifiers| Keys::Shortcut(KeyboardShortcut::new(modifiers, key));
    let none = Modifiers::NONE;
    let pointer = Keys::Pointer;
    let (command, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
    let scroll = options.handle_scroll;
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
            "Selected drawing",
            vec![
                entry("Move one bar earlier", vec![key(Key::ArrowLeft, none)]),
                entry("Move one bar later", vec![key(Key::ArrowRight, none)]),
                entry("Move up one tick", vec![key(Key::ArrowUp, none)]),
                entry("Move down one tick", vec![key(Key::ArrowDown, none)]),
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
        (
            "Pointer",
            [
                Some(entry(
                    "Maximize or restore pane",
                    vec![pointer(none, "Double click")],
                )),
                options.keyboard.enabled.then(|| {
                    entry(
                        "Collapse or restore pane",
                        vec![pointer(command, "Double click")],
                    )
                }),
                (scroll.enabled && scroll.mouse_wheel)
                    .then(|| entry("Scroll through time", vec![pointer(shift, "Wheel")])),
                Some(entry(
                    "Add or remove from selection",
                    vec![pointer(shift, "Click"), pointer(command, "Click")],
                )),
                Some(entry(
                    "Select drawings in an area",
                    vec![pointer(command, "Drag")],
                )),
                Some(entry(
                    "Copy dragged drawings",
                    vec![pointer(command, "Drag")],
                )),
                Some(entry("Start a measurement", vec![pointer(shift, "Click")])),
                Some(entry("Snap a line to 45 degrees", vec![pointer(shift, "")])),
                Some(entry("Draw a square or circle", vec![pointer(shift, "")])),
                Some(entry(
                    "Move a drawing on one axis",
                    vec![pointer(shift, "Drag")],
                )),
                Some(entry(
                    "Turn the magnet on or off",
                    vec![pointer(command, "")],
                )),
                Some(entry(
                    "Move aligned chart dividers",
                    vec![pointer(shift, "Drag")],
                )),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
    ]
}

/// Action returned by the keyboard shortcuts dialog
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyboardShortcutsAction {
    /// No action
    None,
    /// Dialog was closed
    Close,
    /// Save was pressed in the edit mode. The edited bindings, for the host to give its charts
    /// with [`ChartLayout::set_shortcuts`](crate::widget::ChartLayout::set_shortcuts) or
    /// [`Chart::shortcuts_mut`] and to keep in
    /// [`UserSettings::shortcuts`](crate::services::UserSettings::shortcuts).
    Apply(ShortcutMap),
}

/// Keyboard shortcuts help dialog
#[derive(Debug)]
pub struct KeyboardShortcutsDialog {
    /// Whether the dialog is currently open
    pub is_open: bool,
    /// Offer an edit mode whose Save returns [`KeyboardShortcutsAction::Apply`]. Off by default,
    /// so a host that does not apply and save the bindings shows the list only.
    pub editable: bool,
    /// The edit in progress; `None` while the dialog lists the bindings.
    editing: Option<Editing>,
}

/// The edit mode: a copy of the bindings being changed.
#[derive(Debug)]
struct Editing {
    /// The bindings as edited, returned by Save.
    draft: ShortcutMap,
    /// The host's actions outside the built-in categories, bound when editing began. They stay
    /// listed when cleared, so they can be bound again.
    custom: Vec<KeyboardAction>,
    step: Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Idle,
    /// Waiting for the combination of an action; read from the pass after its Change click.
    Capturing(KeyboardAction),
    /// `shortcut`, pressed for `action`, is bound to `holder`: ask before moving it.
    Conflict {
        action: KeyboardAction,
        shortcut: KeyboardShortcut,
        holder: KeyboardAction,
    },
}

/// A click on an edit control, applied once the dialog is drawn so that a frame shows one mode.
#[derive(Clone, Copy)]
enum Click {
    Edit,
    Save,
    Cancel,
    Reset,
    Change(KeyboardAction),
    Clear(KeyboardAction),
    Move,
    Keep,
}

impl Editing {
    fn new(shortcuts: &ShortcutMap) -> Self {
        let custom = shortcuts
            .by_category()
            .into_iter()
            .find_map(|(name, actions)| (name == "Custom").then_some(actions))
            .unwrap_or_default();
        Self {
            draft: shortcuts.clone(),
            custom,
            step: Step::Idle,
        }
    }

    /// Takes the press a Change click waits for, and Escape from the question about a held
    /// combination, before the dialog's own Escape and the charts read the keys of this pass.
    fn read_keys(&mut self, ctx: &Context) {
        match self.step {
            Step::Capturing(action) => {
                suppress_shortcuts_this_pass(ctx);
                let Some((key, modifiers)) = ctx.input_mut(|input| {
                    let index = input.events.iter().position(|e| pressed(e).is_some())?;
                    let press = input.events.remove(index);
                    // The text the press typed must not reach a chart's typed entry.
                    input
                        .events
                        .retain(|event| !matches!(event, Event::Text(_)));
                    pressed(&press)
                }) else {
                    return;
                };
                if key == Key::Escape {
                    self.step = Step::Idle;
                    return;
                }
                let shortcut = canonical(KeyboardShortcut::new(modifiers, key));
                let holder = self
                    .draft
                    .bindings
                    .iter()
                    .find(|(key, bound)| *bound != action && canonical(*key) == shortcut)
                    .map(|(_, holder)| *holder);
                self.step = match holder {
                    Some(holder) => Step::Conflict {
                        action,
                        shortcut,
                        holder,
                    },
                    None => {
                        bind(&mut self.draft, action, shortcut);
                        Step::Idle
                    }
                };
            }
            Step::Conflict { .. } => {
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    self.step = Step::Idle;
                }
            }
            Step::Idle => {}
        }
    }

    /// The built-in actions by category, also those without a key, then the host's own.
    fn categories(&self) -> Vec<(&'static str, Vec<KeyboardAction>)> {
        let mut categories = KeyboardAction::all_by_category();
        if !self.custom.is_empty() {
            categories.push(("Custom", self.custom.clone()));
        }
        categories
    }
}

/// Makes `shortcut` the only combination of `action` and takes it from every other action. The
/// action keeps its place in the precedence order, or goes last when it had no combination.
fn bind(map: &mut ShortcutMap, action: KeyboardAction, shortcut: KeyboardShortcut) {
    let keep = |(key, bound): &(KeyboardShortcut, KeyboardAction)| {
        *bound != action && canonical(*key) != shortcut
    };
    let first = (map.bindings.iter())
        .position(|(_, bound)| *bound == action)
        .unwrap_or(map.bindings.len());
    let at = map.bindings[..first]
        .iter()
        .filter(|&binding| keep(binding))
        .count();
    map.bindings.retain(keep);
    map.bindings.insert(at, (shortcut, action));
}

impl Default for KeyboardShortcutsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardShortcutsDialog {
    /// Create a [`KeyboardShortcutsDialog`] with default settings.
    pub fn new() -> Self {
        Self {
            is_open: false,
            editable: false,
            editing: None,
        }
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

    /// Show only the host's bound shortcuts, using the same map as its chart, and the pointer
    /// gestures of a chart with default options. [`Self::show_for_chart`] also leaves out the
    /// gestures the chart's options turn off.
    ///
    /// In the edit mode, Save returns the edited bindings as [`KeyboardShortcutsAction::Apply`].
    /// While it waits for a key combination the dialog takes the press, and charts that dispatch
    /// later in the pass read no keys; a host that dispatches chart keys before showing the
    /// dialog suppresses them while the dialog is open, as for its other dialogs.
    pub fn show_with_shortcuts(
        &mut self,
        ctx: &Context,
        shortcuts: &ShortcutMap,
    ) -> KeyboardShortcutsAction {
        self.show_with(ctx, shortcuts, &ChartOptions::default())
    }

    /// Show the shortcuts `chart` has bound and the pointer gestures its options leave on.
    ///
    /// In the edit mode, Save returns the edited bindings as [`KeyboardShortcutsAction::Apply`].
    /// While it waits for a key combination the dialog takes the press, and charts that dispatch
    /// later in the pass read no keys; a host that dispatches chart keys before showing the
    /// dialog suppresses them while the dialog is open, as for its other dialogs.
    pub fn show_for_chart(&mut self, ctx: &Context, chart: &Chart) -> KeyboardShortcutsAction {
        self.show_with(ctx, chart.shortcuts(), chart.chart_options())
    }

    fn show_with(
        &mut self,
        ctx: &Context,
        shortcuts: &ShortcutMap,
        options: &ChartOptions,
    ) -> KeyboardShortcutsAction {
        if !self.is_open {
            self.editing = None;
            return KeyboardShortcutsAction::None;
        }
        if let Some(editing) = &mut self.editing {
            editing.read_keys(ctx);
        }

        let blocked_now = ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);
        let mut action = KeyboardShortcutsAction::None;

        let width = match self.editing {
            Some(_) => DESIGN_TOKENS.sizing.settings_dialog.width,
            None => DESIGN_TOKENS.sizing.dialog.default_width,
        };
        DialogFrame::with_width(self.title(), width).show(ctx, |ui| {
            action = self.render_contents(ui, shortcuts, options);
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
        if !self.is_open {
            self.editing = None;
        }

        action
    }

    /// The title of the list, or of the edit mode: a window of its own, as a window keeps the
    /// widest size it had and the edit buttons need more room.
    fn title(&self) -> &'static str {
        match self.editing {
            Some(_) => "Edit Keyboard Shortcuts",
            None => "Keyboard Shortcuts",
        }
    }

    /// Applies a click on an edit control. Returns the bindings Save hands back.
    fn apply(&mut self, click: Click, shortcuts: &ShortcutMap) -> Option<ShortcutMap> {
        match (click, &mut self.editing) {
            (Click::Edit, editing) => *editing = Some(Editing::new(shortcuts)),
            (Click::Save, editing) => return editing.take().map(|editing| editing.draft),
            (Click::Cancel, editing) => *editing = None,
            (_, None) => {}
            (Click::Change(action), Some(editing)) => editing.step = Step::Capturing(action),
            (Click::Clear(action), Some(editing)) => {
                editing.draft.bindings.retain(|(_, bound)| *bound != action);
                editing.step = Step::Idle;
            }
            (Click::Reset, Some(editing)) => {
                editing.draft = ShortcutMap::default();
                editing.step = Step::Idle;
            }
            (Click::Move, Some(editing)) => {
                if let Step::Conflict {
                    action, shortcut, ..
                } = editing.step
                {
                    bind(&mut editing.draft, action, shortcut);
                }
                editing.step = Step::Idle;
            }
            (Click::Keep, Some(editing)) => editing.step = Step::Idle,
        }
        None
    }

    fn render_contents(
        &mut self,
        ui: &mut Ui,
        shortcuts: &ShortcutMap,
        options: &ChartOptions,
    ) -> KeyboardShortcutsAction {
        let mut action = KeyboardShortcutsAction::None;
        let theme = ui.active_theme();
        let mut click = None;

        if dialog_header(ui, self.title()) {
            action = KeyboardShortcutsAction::Close;
            self.is_open = false;
        }
        // The chart reads its keys only while it has keyboard focus.
        let hint = RichText::new(l10n::text(
            ui.ctx(),
            "Click the chart to use the chart keys.",
        ))
        .size(theme.text.small)
        .color(ui.visuals().weak_text_color());
        if self.editable && self.editing.is_none() {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Button::new("Edit").secondary().small().show(ui).clicked() {
                        click = Some(Click::Edit);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(hint).truncate());
                    });
                });
            });
        } else {
            ui.label(hint);
        }
        ui.add_space(theme.spacing.lg);
        if let Some(editing) = &self.editing {
            ui.horizontal(|ui| {
                if Button::new("Reset").ghost().show(ui).clicked() {
                    click = Some(Click::Reset);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Button::new("Save").primary().show(ui).clicked() {
                        click = Some(Click::Save);
                    }
                    if Button::new("Cancel").ghost().show(ui).clicked() {
                        click = Some(Click::Cancel);
                    }
                });
            });
            if let Step::Conflict {
                shortcut, holder, ..
            } = editing.step
            {
                ui.add_space(theme.spacing.sm);
                ui.horizontal(|ui| {
                    let question = l10n::format_text(
                        ui.ctx(),
                        "{keys} is used by {action}.",
                        &[
                            ("keys", ui.ctx().format_shortcut(&shortcut)),
                            ("action", l10n::text(ui.ctx(), holder.label())),
                        ],
                    );
                    ui.label(RichText::new(question).size(theme.text.body));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Button::new("Move").primary().small().show(ui).clicked() {
                            click = Some(Click::Move);
                        }
                        if Button::new("Keep").ghost().small().show(ui).clicked() {
                            click = Some(Click::Keep);
                        }
                    });
                });
            }
            ui.add_space(theme.spacing.lg);
        }

        // The edit mode lists every action, also unbound ones, and none of the fixed keys.
        let (map, groups, fixed) = match &self.editing {
            Some(editing) => (&editing.draft, editing.categories(), Vec::new()),
            None => (
                shortcuts,
                shortcuts.by_category(),
                fixed_categories(options),
            ),
        };
        let capturing = self
            .editing
            .as_ref()
            .and_then(|editing| match editing.step {
                Step::Capturing(action) => Some(action),
                _ => None,
            });
        let categories: Vec<(&str, Vec<Entry>)> = groups
            .into_iter()
            .map(|(name, actions)| {
                let entries = actions
                    .into_iter()
                    .map(|action| Entry {
                        label: action.label().to_owned(),
                        keys: if capturing == Some(action) {
                            vec![Keys::Typed("Press keys")]
                        } else {
                            map.keys_for(action).map(Keys::Shortcut).collect()
                        },
                        action: self.editing.is_some().then_some(action),
                    })
                    .collect();
                (name, entries)
            })
            .chain(fixed)
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
        let columns = if self.editing.is_some() { 3.0 } else { 2.0 };
        let label_w = (((widest + theme.spacing.xxl) / 4.0).ceil() * 4.0)
            .min((ui.available_width() - theme.spacing.sm * (columns - 1.0)) / columns);

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
                            .num_columns(columns as usize)
                            .spacing([theme.spacing.sm, theme.spacing.sm])
                            // With the spacing, a 24 px row pitch.
                            .min_row_height(theme.spacing.xxl + theme.spacing.sm)
                            .min_col_width(label_w)
                            .max_col_width(label_w)
                            .show(ui, |ui| {
                                for entry in entries {
                                    let label = l10n::text(ui.ctx(), &entry.label);
                                    ui.label(RichText::new(&label).size(theme.text.body));
                                    if entry.keys.is_empty() {
                                        ui.label(
                                            RichText::new(l10n::text(ui.ctx(), "Not set"))
                                                .size(theme.text.small)
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    } else {
                                        ui.horizontal_wrapped(|ui| {
                                            key_badges(ui, &theme, &entry.keys)
                                        });
                                    }
                                    if let Some(action) = entry.action {
                                        ui.horizontal(|ui| {
                                            let change = "Change {action}";
                                            if row_button(ui, "Change", change, &label, true) {
                                                click = Some(Click::Change(action));
                                            }
                                            let bound = map.keys_for(action).next().is_some();
                                            if row_button(
                                                ui,
                                                "Clear",
                                                "Clear {action}",
                                                &label,
                                                bound,
                                            ) {
                                                click = Some(Click::Clear(action));
                                            }
                                        });
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                });
        });

        if let Some(click) = click
            && let Some(map) = self.apply(click, shortcuts)
        {
            action = KeyboardShortcutsAction::Apply(map);
        }
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

/// A row's Change or Clear button, named after the row's `action` for screen readers.
fn row_button(ui: &mut Ui, text: &str, name: &str, action: &str, enabled: bool) -> bool {
    let response = Button::new(text).ghost().small().enabled(enabled).show(ui);
    let name = l10n::format_text(ui.ctx(), name, &[("action", action)]);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, response.enabled(), &name));
    response.clicked()
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
            Keys::Pointer(modifiers, action) => {
                let action = (!action.is_empty()).then(|| l10n::text(ui.ctx(), action));
                let text = ui
                    .ctx()
                    .format_modifiers(*modifiers)
                    .split('+')
                    .filter(|modifier| !modifier.is_empty())
                    .map(str::to_owned)
                    .chain(action)
                    .collect::<Vec<_>>()
                    .join("+");
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
    /// complete guide to the keyboard, and #3204: the pointer gestures.
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
            "Move one bar earlier",
            "Move down one tick",
            "Play or pause the replay",
            "Replay one bar forward",
            "Maximize or restore pane",
            "Collapse or restore pane",
            "Scroll through time",
            "Add or remove from selection",
            "Select drawings in an area",
            "Copy dragged drawings",
            "Start a measurement",
            "Snap a line to 45 degrees",
            "Draw a square or circle",
            "Move a drawing on one axis",
            "Turn the magnet on or off",
            "Move aligned chart dividers",
        ] {
            assert!(h.query_by_label(label).is_some(), "missing {label}");
        }
    }

    /// Rebinding keeps precedence, conflicts move only after confirmation, Escape cancels a
    /// capture, and Clear and Reset reach the map returned to the host.
    #[test]
    fn edit_mode_rebinds_moves_clears_and_resets() {
        use crate::config::{DrawingToolType, KeyboardAction as A};
        let alt = |key| KeyboardShortcut::new(Modifiers::ALT, key);
        let mut dialog = KeyboardShortcutsDialog::new();
        dialog.editable = true;
        dialog.open();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 2400.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &Theme::dark());
            })
            .build_ui_state(
                |ui,
                 (dialog, map, applied): &mut (
                    KeyboardShortcutsDialog,
                    ShortcutMap,
                    Option<ShortcutMap>,
                )| {
                    if let KeyboardShortcutsAction::Apply(edited) =
                        dialog.show_with_shortcuts(ui.ctx(), map)
                    {
                        *map = edited.clone();
                        *applied = Some(edited);
                    }
                },
                (dialog, ShortcutMap::default(), None),
            );
        h.run();
        h.get_by_role_and_label(Role::Button, "Edit").click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Change Take a chart snapshot")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::ALT, Key::B);
        h.run();
        h.get_by_role_and_label(Role::Button, "Change Go to a date")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::ALT, Key::T);
        h.run();
        let question = format!(
            "{} is used by Trend Line.",
            h.ctx.format_shortcut(&alt(Key::T))
        );
        assert!(h.query_by_label(&question).is_some());
        h.get_by_role_and_label(Role::Button, "Keep").click();
        h.run();
        assert!(h.query_by_label(&question).is_none());
        let trend_line = A::DrawingTool(DrawingToolType::TrendLine);
        assert_eq!(
            h.state()
                .0
                .editing
                .as_ref()
                .unwrap()
                .draft
                .shortcut(trend_line),
            Some(alt(Key::T))
        );
        h.get_by_role_and_label(Role::Button, "Change Go to a date")
            .click();
        h.run();
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().0.is_open);
        assert!(h.query_by_role_and_label(Role::Button, "Save").is_some());
        h.get_by_role_and_label(Role::Button, "Change Go to a date")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::ALT, Key::T);
        h.run();
        h.get_by_role_and_label(Role::Button, "Move").click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Clear Cancel")
            .click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Save").click();
        h.run();
        let mut expected = ShortcutMap::default();
        expected.bindings.retain(|(_, action)| {
            !matches!(
                action,
                A::DrawingTool(DrawingToolType::TrendLine) | A::Cancel
            )
        });
        for (key, action) in &mut expected.bindings {
            match action {
                A::Snapshot => *key = alt(Key::B),
                A::GoToDate => *key = alt(Key::T),
                _ => {}
            }
        }
        assert_eq!(h.state().2.as_ref(), Some(&expected));
        h.get_by_role_and_label(Role::Button, "Edit").click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Reset").click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Save").click();
        h.run();
        assert_eq!(h.state().2.as_ref(), Some(&ShortcutMap::default()));
    }

    /// Guards #3204: a pointer gesture the chart's options turn off is not listed, as an
    /// unbound shortcut is not.
    #[test]
    fn a_pointer_gesture_the_chart_turns_off_leaves_the_list() {
        // The options edit, then whether the collapse and the scroll gestures are listed.
        type Row = (fn(&mut ChartOptions), bool, bool);
        let rows: [Row; 3] = [
            (|o| o.keyboard.enabled = false, false, true),
            (|o| o.handle_scroll.mouse_wheel = false, true, false),
            (|o| o.handle_scroll.enabled = false, true, false),
        ];
        for (idx, (edit, collapse, scroll)) in rows.into_iter().enumerate() {
            let mut dialog = KeyboardShortcutsDialog::new();
            dialog.open();
            let mut chart = crate::widget::ChartBuilder::new().build();
            let mut options = chart.chart_options().clone();
            edit(&mut options);
            chart.set_chart_options(options);
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(900.0, 2400.0))
                .with_context_setup(|ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &Theme::dark());
                })
                .build_ui_state(
                    |ui, (d, chart): &mut (KeyboardShortcutsDialog, Chart)| {
                        _ = d.show_for_chart(ui.ctx(), chart)
                    },
                    (dialog, chart),
                );
            h.run_steps(3);
            assert_eq!(
                h.query_by_label("Collapse or restore pane").is_some(),
                collapse,
                "row {idx}"
            );
            assert_eq!(
                h.query_by_label("Scroll through time").is_some(),
                scroll,
                "row {idx}"
            );
        }
    }
}
