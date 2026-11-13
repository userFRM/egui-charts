//! A JSON value drawn as a collapsible tree whose nodes can be clicked.

use egui::collapsing_header::CollapsingState;
use egui_charts::theme::ThemeExt as _;
use serde_json::Value;

use super::profile::{FieldPath, Segment};

/// The elements of a list that are drawn. The rest is counted.
const LISTED: usize = 50;
/// The characters of a text that are drawn. The rest is cut.
const SHOWN: usize = 80;

/// Draws `value` and returns the path of the node the user clicked in this frame, from `value`
/// to the node. `marks` names the nodes already in use, such as `rows` or `open`; each name is
/// drawn after its node. `id` keeps the open and closed state apart from other trees.
///
/// A scalar is drawn as `key: value` and a container as `key` with its size. The tree is open
/// down to the second level and along the first element of every list; text over 80 characters
/// is cut with an ellipsis; a list shows its first 50 elements and then how many more it has.
pub(crate) fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &Value,
    marks: &[(FieldPath, &str)],
) -> Option<FieldPath> {
    let mut walk = Walk {
        marks,
        accent: ui.active_theme().ui.accent,
        path: FieldPath::new(),
        clicked: None,
    };
    walk.node(ui, id, "root", value, false);
    walk.clicked
}

/// The drawing of one tree.
struct Walk<'a> {
    marks: &'a [(FieldPath, &'a str)],
    /// The colour of a node that a slot uses.
    accent: egui::Color32,
    /// The way from the root to the node being drawn.
    path: FieldPath,
    clicked: Option<FieldPath>,
}

impl Walk<'_> {
    /// Draws the node called `name` and everything below it. `first` says that it is the
    /// first element of a list, which is drawn open.
    fn node(&mut self, ui: &mut egui::Ui, id: egui::Id, name: &str, value: &Value, first: bool) {
        match value {
            Value::Array(list) => {
                let text = format!("{name} [{}]", list.len());
                self.container(ui, id, &text, first, |walk, ui| {
                    for (index, element) in list.iter().take(LISTED).enumerate() {
                        walk.path.push(Segment::Index(index));
                        let name = format!("[{index}]");
                        walk.node(ui, id.with(index), &name, element, index == 0);
                        walk.path.pop();
                    }
                    if list.len() > LISTED {
                        ui.weak(format!("{} more", list.len() - LISTED));
                    }
                });
            }
            Value::Object(members) => {
                let text = format!("{name} {{{}}}", members.len());
                self.container(ui, id, &text, first, |walk, ui| {
                    for (key, member) in members {
                        walk.path.push(Segment::Key(key.clone()));
                        walk.node(ui, id.with(key.as_str()), key, member, false);
                        walk.path.pop();
                    }
                });
            }
            scalar => {
                ui.horizontal(|ui| {
                    // The width of the arrow of a container, so that the names line up.
                    ui.add_space(ui.spacing().indent);
                    self.label(ui, &format!("{name}: {}", shown(scalar)));
                });
            }
        }
    }

    /// Draws a list or an object: a row with an arrow that opens and closes it and a label
    /// that selects it, then its `children` when it is open.
    fn container(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        text: &str,
        first: bool,
        children: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        let open = self.path.len() < 2 || first;
        let state = CollapsingState::load_with_default_open(ui.ctx(), id, open);
        let header = state.show_header(ui, |ui| self.label(ui, text));
        header.body(|ui| children(self, ui));
    }

    /// Draws the label of the node at `self.path`, which a click selects, and after it the
    /// names of the slots that use the node.
    fn label(&mut self, ui: &mut egui::Ui, text: &str) {
        let slots = self.marks.iter().filter(|(path, _)| *path == self.path);
        let slots: Vec<&str> = slots.map(|(_, slot)| *slot).collect();
        let mut shown = egui::RichText::new(text).monospace();
        if !slots.is_empty() {
            shown = shown.color(self.accent);
        }
        if ui.add(egui::Button::new(shown).frame(false)).clicked() {
            self.clicked = Some(self.path.clone());
        }
        if !slots.is_empty() {
            ui.label(egui::RichText::new(slots.join(", ")).color(self.accent));
        }
    }
}

/// A scalar as JSON prints it, a long text cut.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) if text.chars().count() > SHOWN => {
            let cut: String = text.chars().take(SHOWN).collect();
            let mut shown = Value::String(cut).to_string();
            // Before the closing quote.
            shown.insert(shown.len() - 1, '…');
            shown
        }
        other => other.to_string(),
    }
}
