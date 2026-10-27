//! Context menu for object tree items
//!
//! Right-click menu with full object management options.

use crate::localization as l10n;
use egui::{Key, Ui};

use super::actions::ObjectTreeAction;
use super::state::ObjectTreeState;
use super::types::{SourceItem, SourceType};

/// Render context menu for a specific item
pub(super) fn render_item_menu(
    ui: &mut Ui,
    item: &SourceItem,
    state: &mut ObjectTreeState,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;
    let id = item.id;

    // Properties/Settings
    if ui
        .selectable_label(false, l10n::text(ui.ctx(), "Properties…"))
        .clicked()
    {
        action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::OpenIndicatorSettings(id)
        } else {
            ObjectTreeAction::OpenProperties(id)
        };
        ui.close();
    }

    // Rename
    if item.is_drawing()
        && ui
            .selectable_label(false, l10n::text(ui.ctx(), "Rename…"))
            .clicked()
    {
        state.start_rename(id, item.display_name());
        ui.close();
    }

    ui.separator();

    // Duplicate (drawings and indicators)
    if (item.is_drawing() || item.is_indicator())
        && ui
            .selectable_label(false, l10n::text(ui.ctx(), "Duplicate"))
            .clicked()
    {
        action = ObjectTreeAction::Duplicate(id);
        ui.close();
    }

    ui.separator();

    // Z-ordering (drawings only)
    if item.source_type == SourceType::Drawing {
        if ui
            .selectable_label(false, l10n::text(ui.ctx(), "Bring to front"))
            .clicked()
        {
            action = ObjectTreeAction::BringToFront(id);
            ui.close();
        }
        if ui
            .selectable_label(false, l10n::text(ui.ctx(), "Move up"))
            .clicked()
        {
            action = ObjectTreeAction::MoveUp(id);
            ui.close();
        }
        if ui
            .selectable_label(false, l10n::text(ui.ctx(), "Move down"))
            .clicked()
        {
            action = ObjectTreeAction::MoveDown(id);
            ui.close();
        }
        if ui
            .selectable_label(false, l10n::text(ui.ctx(), "Send to back"))
            .clicked()
        {
            action = ObjectTreeAction::SendToBack(id);
            ui.close();
        }
        if !item.panes.is_empty() {
            ui.menu_button(l10n::text(ui.ctx(), "Move to"), |ui| {
                for (pane, title) in &item.panes {
                    if ui
                        .selectable_label(false, l10n::text(ui.ctx(), title))
                        .clicked()
                    {
                        action = ObjectTreeAction::MoveToPane(id, *pane);
                        ui.close();
                    }
                }
            });
        }

        ui.separator();
    }

    // Visibility toggle
    let vis_label = if item.visible { "Hide" } else { "Show" };
    if ui
        .selectable_label(false, l10n::text(ui.ctx(), vis_label))
        .clicked()
    {
        action = ObjectTreeAction::ToggleVisibility(id);
        ui.close();
    }

    // Lock toggle (drawings only)
    if item.source_type == SourceType::Drawing {
        let lock_label = if item.locked { "Unlock" } else { "Lock" };
        if ui
            .selectable_label(false, l10n::text(ui.ctx(), lock_label))
            .clicked()
        {
            action = ObjectTreeAction::ToggleLock(id);
            ui.close();
        }
    }

    ui.separator();

    // Zoom to
    if item.is_drawing()
        && ui
            .selectable_label(false, l10n::text(ui.ctx(), "Zoom to object"))
            .clicked()
    {
        action = ObjectTreeAction::ZoomTo(id);
        ui.close();
    }

    ui.separator();

    // Delete/Remove
    let delete_label = if item.source_type == SourceType::Indicator {
        "Remove indicator"
    } else {
        "Delete"
    };
    if item.source_type != SourceType::DataSource
        && ui
            .selectable_label(false, l10n::text(ui.ctx(), delete_label))
            .clicked()
    {
        action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::RemoveIndicator(id)
        } else {
            ObjectTreeAction::Delete(id)
        };
        ui.close();
    }

    action
}

/// Handle keyboard shortcuts for object tree
pub fn handle_keyboard_shortcuts(
    ui: &Ui,
    state: &mut ObjectTreeState,
    sources: &[SourceItem],
) -> ObjectTreeAction {
    if ui.memory(|m| m.focused().is_some()) || egui::Popup::is_any_open(ui.ctx()) {
        return ObjectTreeAction::None;
    }

    ui.input_mut(|i| {
        let modifiers = i.modifiers;
        if state.selection_count() > 0
            && (i.consume_key(modifiers, Key::Delete) || i.consume_key(modifiers, Key::Backspace))
        {
            return ObjectTreeAction::DeleteSelected;
        }
        if modifiers.command && i.consume_key(modifiers, Key::A) {
            return ObjectTreeAction::SelectAll;
        }
        if modifiers.command && state.selection_count() > 0 && i.consume_key(modifiers, Key::D) {
            return ObjectTreeAction::DuplicateSelected;
        }
        if (state.renaming_id.is_some() || state.selection_count() > 0)
            && i.consume_key(modifiers, Key::Escape)
        {
            if state.renaming_id.is_some() {
                state.cancel_rename();
            } else {
                return ObjectTreeAction::ClearSelection;
            }
        }
        if state.renaming_id.is_some()
            && i.consume_key(modifiers, Key::Enter)
            && let Some((id, name)) = state.finish_rename()
        {
            return ObjectTreeAction::Rename(id, name);
        }
        if let Some(&id) = state.selected_ids.iter().next()
            && let Some(source) = sources.iter().find(|s| s.id == id && s.is_drawing())
            && i.consume_key(modifiers, Key::F2)
        {
            state.start_rename(id, source.display_name());
        }
        ObjectTreeAction::None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_respect_focus_and_consume_only_the_keys_they_handle() {
        for focused in [true, false] {
            for key in [
                Key::Delete,
                Key::Backspace,
                Key::A,
                Key::D,
                Key::Escape,
                Key::Enter,
            ] {
                let ctx = egui::Context::default();
                let command = matches!(key, Key::A | Key::D);
                let modifiers = egui::Modifiers {
                    command,
                    ctrl: command,
                    ..Default::default()
                };
                let input = egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| {
                    ui.input_mut(|i| i.modifiers = modifiers);
                    let mut state = ObjectTreeState::new();
                    state.select(1);
                    if key == Key::Enter {
                        state.start_rename(1, "Name");
                    }
                    if focused {
                        ui.memory_mut(|m| m.request_focus(egui::Id::unique("other field")));
                    }
                    let action = handle_keyboard_shortcuts(
                        ui,
                        &mut state,
                        &[SourceItem::data_source(1, "Name")],
                    );
                    assert_eq!(action.is_none(), focused, "{key:?}, focused={focused}");
                    assert_eq!(
                        ui.input(|i| i.key_pressed(key)),
                        focused,
                        "{key:?} consumption"
                    );
                });
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn row_popup_closes_on_a_click_one_hundred_pixels_to_its_left() {
        use super::super::{ObjectTreeConfig, ObjectTreePanel};
        let ctx = egui::Context::default();
        let mut panel = ObjectTreePanel::with_config(ObjectTreeConfig {
            show_data_window: false,
            ..Default::default()
        });
        let mut sources = [SourceItem::indicator(1, "SMA", egui::Color32::YELLOW)];
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(300.0);
                        ui.vertical(|ui| {
                            ui.set_width(280.0);
                            panel.show(ui, None, &mut sources);
                        });
                    });
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            shapes
        };
        frame(vec![]);
        let shapes = frame(vec![]);
        let point = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "SMA" => {
                    Some(text.pos + egui::vec2(1.0, 2.0))
                }
                _ => None,
            })
            .unwrap();
        let button = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        frame(vec![egui::Event::PointerMoved(point)]);
        frame(vec![button(point, egui::PointerButton::Secondary, true)]);
        frame(vec![button(point, egui::PointerButton::Secondary, false)]);
        frame(vec![]);
        assert!(egui::Popup::is_any_open(&ctx));
        let outside = point - egui::vec2(100.0, 0.0);
        frame(vec![
            egui::Event::PointerMoved(outside),
            button(outside, egui::PointerButton::Primary, true),
        ]);
        frame(vec![button(outside, egui::PointerButton::Primary, false)]);
        frame(vec![]);
        assert!(!egui::Popup::is_any_open(&ctx));
    }
}
