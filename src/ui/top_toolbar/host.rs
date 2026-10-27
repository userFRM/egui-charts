//! Host buttons and dropdown entries rendered alongside the built-in toolbar controls.

use crate::icons::Icon;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::list_item::{LabelContent, ListItem};
use egui::Ui;

use super::TopToolbarAction;

/// A host button, or a dropdown when [`Self::items`] is nonempty.
///
/// Entries retain registration order and use the active theme. Ids must be unique within
/// the toolbar, including dropdown children. The host handles [`TopToolbarAction::Host`].
///
/// ```
/// use egui_charts::ui::top_toolbar::{TopToolbar, ToolbarEntry};
/// let mut toolbar = TopToolbar::new();
/// toolbar.host_entries.push(ToolbarEntry::new("research", "Research"));
/// toolbar.host_entries.push(ToolbarEntry::new("tools", "Host tools").with_items(vec![
///     ToolbarEntry::new("ticket", "Order ticket"),
/// ]));
/// // toolbar.show_with_state(ui, app_state) returns TopToolbarAction::Host(id).
/// ```
pub struct ToolbarEntry {
    /// Stable id returned when this entry is chosen.
    pub id: String,
    /// Button or menu label.
    pub label: String,
    /// Optional icon using the built-in size and tint.
    pub icon: Option<&'static Icon>,
    /// Dropdown choices in display order. Only top-level entries open dropdowns.
    /// An empty list makes this entry a button.
    pub items: Vec<ToolbarEntry>,
    /// Whether to show this entry. Hidden entries take no space and emit no actions.
    pub visible: bool,
}

impl ToolbarEntry {
    /// Create a visible host button.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            items: Vec::new(),
            visible: true,
        }
    }

    /// Paint an icon beside the label.
    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Turn this button into a dropdown. Opening it emits no action.
    pub fn with_items(mut self, items: Vec<Self>) -> Self {
        self.items = items;
        self
    }

    pub(super) fn show(&self, ui: &mut Ui, action: &mut TopToolbarAction) {
        if !self.visible {
            return;
        }
        ui.push_id(&self.id, |ui| {
            let response = match self.icon {
                Some(icon) => Button::icon_and_text(icon, &self.label),
                None => Button::new(&self.label),
            }
            .show(ui);
            if self.items.is_empty() {
                if response.clicked() {
                    *action = TopToolbarAction::Host(self.id.clone());
                }
            } else {
                egui::Popup::menu(&response).show(|ui| {
                    ui.set_width(crate::tokens::DESIGN_TOKENS.sizing.dialog.submenu_width);
                    for item in self.items.iter().filter(|item| item.visible) {
                        ui.push_id(&item.id, |ui| {
                            let mut content = LabelContent::new(&item.label);
                            if let Some(icon) = item.icon {
                                content = content.with_icon(icon);
                            }
                            if ListItem::new().show(ui, content).clicked() {
                                *action = TopToolbarAction::Host(item.id.clone());
                                ui.close();
                            }
                        });
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app_state::ChartAppState;
    use crate::ui::top_toolbar::TopToolbar;
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};

    #[test]
    fn registered_buttons_and_dropdowns_emit_ids_in_both_themes() {
        struct State;
        impl ChartAppState for State {
            fn active_symbol(&self) -> &str {
                "SAMPLE"
            }
            fn active_timeframe(&self) -> &crate::model::Timeframe {
                &crate::model::Timeframe::Min1
            }
            fn chart_type(&self) -> crate::model::ChartType {
                crate::model::ChartType::Candles
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn is_replay_active(&self) -> bool {
                false
            }
        }
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [320.0, 600.0, 1440.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar = TopToolbar::new();
                toolbar.host_entries = vec![
                    ToolbarEntry::new("research", "Research"),
                    ToolbarEntry::new("tools", "Host tools")
                        .with_items(vec![ToolbarEntry::new("copy", "Copy symbol")]),
                    ToolbarEntry {
                        visible: false,
                        ..ToolbarEntry::new("hidden", "Hidden entry")
                    },
                ];
                toolbar.call_to_action = Some(("cta".into(), "Get data".into()));
                let mut frame = |events| {
                    let mut action = TopToolbarAction::None;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 200.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = toolbar.show_with_state(ui, &State);
                        },
                    );
                    let labels = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Text(text) => Some((
                                text.galley.text().to_owned(),
                                text.pos + text.galley.size() / 2.0,
                            )),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    output.drop_without_applying_deltas();
                    assert!(!labels.iter().any(|(label, _)| label == "Hidden entry"));
                    (action, labels)
                };
                frame(vec![]);
                let (_, labels) = frame(vec![]);
                let at = |labels: &[(String, Pos2)], name: &str| {
                    labels.iter().find(|(label, _)| label == name).unwrap().1
                };
                let interval = at(&labels, "D");
                // Starred intervals can push the host actions past the window at any width.
                frame(vec![Event::PointerMoved(Pos2::new(150.0, interval.y))]);
                frame(vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(-2000.0, 0.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Default::default(),
                }]);
                for _ in 0..30 {
                    frame(vec![]);
                }
                let (_, labels) = frame(vec![]);
                let research = at(&labels, "Research");
                let tools = at(&labels, "Host tools");
                let cta = at(&labels, "Get data");
                assert!(
                    (research.y - interval.y).abs() <= 1.0,
                    "host buttons share the toolbar centre: {research:?} versus {interval:?}"
                );
                assert!(
                    research.x < tools.x && tools.x < cta.x && cta.x < width,
                    "registration order, the call to action last, and reserved width"
                );
                let click = |pos| {
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        },
                    ]
                };
                frame(vec![Event::PointerMoved(research)]);
                assert_eq!(
                    frame(click(research)).0,
                    TopToolbarAction::Host("research".into())
                );
                frame(vec![Event::PointerMoved(cta)]);
                assert_eq!(frame(click(cta)).0, TopToolbarAction::Host("cta".into()));
                frame(vec![Event::PointerMoved(tools)]);
                assert_eq!(
                    frame(click(tools)).0,
                    TopToolbarAction::None,
                    "opening is not choosing"
                );
                let (_, labels) = frame(vec![]);
                let copy = at(&labels, "Copy symbol");
                frame(vec![Event::PointerMoved(copy)]);
                assert_eq!(frame(click(copy)).0, TopToolbarAction::Host("copy".into()));
                assert_eq!(frame(vec![]).0, TopToolbarAction::None);
            }
        }
    }
}
