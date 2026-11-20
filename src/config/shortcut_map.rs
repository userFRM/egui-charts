//! Editable key bindings shared by input dispatch, menus and shortcut help.

use super::KeyboardAction;
use egui::{Context, Event, KeyboardShortcut};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Editable bindings shared by input dispatch, menus and shortcut help.
///
/// Remove a binding to disable it, or replace its key or action to rebind it.
/// The first binding wins if several actions use the same key combination.
/// A binding on a punctuation key also runs when the press differs from it only by Shift and
/// types that key's character, as some layouts need, unless another binding names that exact
/// combination.
///
/// Serde writes the bindings in order, as a list of key combinations and actions such as
/// `{"shortcut": "Ctrl+Shift+Z", "action": "Redo"}`. A combination is spelled with egui's English
/// key names after `Ctrl`, `Alt` and `Shift`, as the shortcut help reads it out on Linux and
/// Windows. `Ctrl` is the command key, Cmd on Mac, so a binding on Control alone or on Cmd alone
/// is saved as one on the command key. Reading leaves out a binding whose key or action this
/// release does not know and keeps the order of the others.
///
/// ```
/// use egui::{Key, KeyboardShortcut, Modifiers};
/// use egui_charts::config::KeyboardAction;
/// use egui_charts::widget::ShortcutMap;
///
/// let mut map = ShortcutMap::default();
/// map.bindings.retain(|(_, action)| *action != KeyboardAction::OpenLayout);
/// map.bindings.push((KeyboardShortcut::new(Modifiers::ALT, Key::O), KeyboardAction::OpenLayout));
/// let json = serde_json::to_string(&map).unwrap();
/// assert!(json.ends_with(r#"{"shortcut":"Alt+O","action":"OpenLayout"}]"#));
/// assert_eq!(serde_json::from_str::<ShortcutMap>(&json).unwrap(), map);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShortcutMap {
    /// Key combinations and their actions, in precedence order.
    pub bindings: Vec<(KeyboardShortcut, KeyboardAction)>,
}

impl Default for ShortcutMap {
    fn default() -> Self {
        Self {
            bindings: KeyboardAction::all_by_category()
                .into_iter()
                .flat_map(|(_, actions)| actions)
                .flat_map(|action| action.shortcuts().iter().map(move |key| (*key, action)))
                .collect(),
        }
    }
}

impl ShortcutMap {
    /// The first binding for an action, for a platform-formatted menu hint.
    pub fn shortcut(&self, action: KeyboardAction) -> Option<KeyboardShortcut> {
        self.keys_for(action).next()
    }

    pub(crate) fn keys_for(
        &self,
        action: KeyboardAction,
    ) -> impl Iterator<Item = KeyboardShortcut> + '_ {
        self.bindings
            .iter()
            .enumerate()
            .filter_map(move |(index, (key, bound))| {
                (*bound == action
                    && !self.bindings[..index]
                        .iter()
                        .any(|(earlier, _)| earlier == key))
                .then_some(*key)
            })
    }

    /// The binding a key press that typed `typed` runs. Layouts differ in which punctuation
    /// needs Shift, so a key named by a punctuation character matches with Shift ignored when no
    /// binding names the exact modifiers and the press typed that character or nothing.
    pub(crate) fn binding(
        &self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        typed: Option<&str>,
    ) -> Option<(KeyboardShortcut, KeyboardAction)> {
        let find = |ignore_shift: bool| {
            let held = |modifiers: egui::Modifiers| egui::Modifiers {
                shift: modifiers.shift && !ignore_shift,
                ..modifiers
            };
            self.bindings.iter().copied().find(|(shortcut, _)| {
                key == shortcut.logical_key
                    && held(modifiers).matches_exact(held(shortcut.modifiers))
            })
        };
        find(false).or_else(|| {
            typed_char(key)
                // Punctuation only: Shift with a letter, a digit or space is its own shortcut.
                .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
                // Shift+Period typing `>` on a US layout is not the period.
                .filter(|_| typed.is_none_or(|text| egui::Key::from_name(text) == Some(key)))
                .and_then(|_| find(true))
        })
    }

    /// Consume the first bound key press with the text it typed, leaving other input for its
    /// owner.
    pub(crate) fn consume(
        &self,
        ctx: &Context,
        stop_at_text: bool,
        accepts: impl Fn(KeyboardAction, KeyboardShortcut) -> bool,
    ) -> Option<(KeyboardAction, Event)> {
        ctx.input_mut(|input| {
            let pairs = typed_pairs(&input.events);
            let bound = |index: usize| {
                let (key, modifiers) = pressed(&input.events[index])?;
                let typed = pairs[index].and_then(|text| match &input.events[text] {
                    Event::Text(text) => Some(text.as_str()),
                    _ => None,
                });
                let (shortcut, action) = self.binding(key, modifiers, typed)?;
                accepts(action, shortcut).then_some(action)
            };
            let (index, action) = input
                .events
                .iter()
                .enumerate()
                .take_while(|(index, event)| {
                    // Typed text ends the search, unless the bound key that typed it follows.
                    !stop_at_text
                        || !matches!(event, Event::Text(text) if starts_entry(text))
                        || (pairs[*index] == Some(index + 1) && bound(index + 1).is_some())
                })
                .find_map(|(index, _)| Some((index, bound(index)?)))?;
            let mut event = input.events.remove(index);
            // The press ran a binding, so its text is dropped whatever character it carries.
            if let Some(text) = pairs[index] {
                input.events.remove(text.min(index));
            }
            // A browser reports the paste key and then the clipboard text: one paste, with it.
            if action == KeyboardAction::PasteDrawing
                && matches!(event, Event::Key { .. })
                && let Some(paste) = input
                    .events
                    .iter()
                    .position(|event| matches!(event, Event::Paste(_)))
            {
                event = input.events.remove(paste);
            }
            Some((action, event))
        })
    }

    /// Bound actions grouped for help, including bindings added by the host.
    pub fn by_category(&self) -> Vec<(&'static str, Vec<KeyboardAction>)> {
        let mut categories = KeyboardAction::all_by_category();
        for (_, actions) in &mut categories {
            actions.retain(|action| self.shortcut(*action).is_some());
        }
        let mut custom = Vec::new();
        for (_, action) in &self.bindings {
            if self.shortcut(*action).is_some()
                && !categories
                    .iter()
                    .any(|(_, actions)| actions.contains(action))
                && !custom.contains(action)
            {
                custom.push(*action);
            }
        }
        if !custom.is_empty() {
            categories.push(("Custom", custom));
        }
        categories.retain(|(_, actions)| !actions.is_empty());
        categories
    }
}

/// One binding as saved.
#[derive(Serialize, Deserialize)]
struct SavedBinding {
    shortcut: String,
    action: KeyboardAction,
}

impl Serialize for ShortcutMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.bindings.iter().map(|(shortcut, action)| SavedBinding {
            shortcut: canonical(*shortcut).format(&egui::ModifierNames::NAMES, false),
            action: *action,
        }))
    }
}

impl<'de> Deserialize<'de> for ShortcutMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            bindings: Vec::<serde_json::Value>::deserialize(deserializer)?
                .into_iter()
                .filter_map(|item| {
                    let saved: SavedBinding = serde_json::from_value(item).ok()?;
                    Some((parse_shortcut(&saved.shortcut)?, saved.action))
                })
                .collect(),
        })
    }
}

/// `shortcut` on the command key when it holds Ctrl, Cmd or both, as it is saved.
pub(crate) fn canonical(shortcut: KeyboardShortcut) -> KeyboardShortcut {
    let held = shortcut.modifiers;
    let modifiers = egui::Modifiers {
        alt: held.alt,
        shift: held.shift,
        command: held.command || held.ctrl || held.mac_cmd,
        ..egui::Modifiers::NONE
    };
    KeyboardShortcut::new(modifiers, shortcut.logical_key)
}

/// Reads a saved combination such as `Ctrl+Shift+Z`; `None` for a key or modifier name this
/// release does not know.
fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut names = text.split('+');
    let key = egui::Key::from_name(names.next_back()?)?;
    let mut modifiers = egui::Modifiers::NONE;
    for name in names {
        modifiers = modifiers.plus(match name {
            "Ctrl" => egui::Modifiers::COMMAND,
            "Alt" => egui::Modifiers::ALT,
            "Shift" => egui::Modifiers::SHIFT,
            _ => return None,
        });
    }
    Some(KeyboardShortcut::new(modifiers, key))
}

/// The key and modifiers of a key press, with the clipboard commands the desktop reports in place
/// of Ctrl+C, Ctrl+X and Ctrl+V.
pub(crate) fn pressed(event: &Event) -> Option<(egui::Key, egui::Modifiers)> {
    match event {
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => Some((*key, *modifiers)),
        Event::Copy => Some((egui::Key::C, egui::Modifiers::COMMAND)),
        Event::Cut => Some((egui::Key::X, egui::Modifiers::COMMAND)),
        Event::Paste(_) | Event::PasteImage(_) => Some((egui::Key::V, egui::Modifiers::COMMAND)),
        _ => None,
    }
}

pub(crate) fn starts_entry(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| !c.is_control() && !c.is_whitespace())
}

/// The character of a letter, digit, punctuation or space key, the keys whose press types text.
fn typed_char(key: egui::Key) -> Option<char> {
    let mut symbol = match key {
        // egui names these two by a word, although they type a character.
        egui::Key::Quote => "'",
        egui::Key::Space => " ",
        _ => key.symbol_or_name(),
    }
    .chars();
    let c = symbol.next()?;
    // The arrows are named by a glyph and type nothing; the minus key by the minus sign.
    (symbol.next().is_none() && (c.is_ascii() || key == egui::Key::Minus)).then_some(c)
}

/// For each event, the index of the other half of its key press: the text a key typed, or the
/// key that typed a text.
///
/// The two are adjacent, the key first on the desktop and the text first in a browser. A
/// press types at most one text and none with Ctrl or Command held, so a text belongs to the
/// press before it unless that press already has its text, and otherwise to the press after.
fn typed_pairs(events: &[Event]) -> Vec<Option<usize>> {
    let types = |event: &Event| {
        matches!(event, Event::Key { key, pressed: true, modifiers, .. }
            if typed_char(*key).is_some() && !modifiers.ctrl && !modifiers.command)
    };
    let mut pairs = vec![None; events.len()];
    for (text, event) in events.iter().enumerate() {
        if !matches!(event, Event::Text(_)) {
            continue;
        }
        let key = [text.checked_sub(1), Some(text + 1)]
            .into_iter()
            .flatten()
            .find(|&key| pairs.get(key) == Some(&None) && types(&events[key]));
        if let Some(key) = key {
            pairs[text] = Some(key);
            pairs[key] = Some(text);
        }
    }
    pairs
}
