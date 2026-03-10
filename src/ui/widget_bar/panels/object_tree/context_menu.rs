//! Context menu for object tree items
//!
//! Right-click menu with full object management options.

use egui::{Area, Frame, Id, Key, Order, Ui};

use super::actions::ObjectTreeAction;
use super::state::ObjectTreeState;
use super::types::{SourceItem, SourceType};

/// Render the context menu
pub fn render_context_menu(
    ui: &mut Ui,
    sources: &[SourceItem],
    state: &mut ObjectTreeState,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    if !state.context_menu_open {
        return action;
    }

    let menu_id = Id::new("object_tree_context_menu");

    Area::new(menu_id)
        .fixed_pos(state.context_menu_pos)
        .order(Order::Foreground)
        .show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);

                if let Some(target_id) = state.context_menu_target {
                    // Context menu for specific item
                    if let Some(item) = sources.iter().find(|s| s.id == target_id) {
                        action = render_item_menu(ui, item, state);
                    }
                } else {
                    // General context menu (no specific item)
                    action = render_general_menu(ui, state);
                }
            });
        });

    // Close on click outside or escape
    if should_close_menu(ui, state) {
        state.close_context_menu();
    }

    action
}

/// Render context menu for a specific item
fn render_item_menu(
    ui: &mut Ui,
    item: &SourceItem,
    state: &mut ObjectTreeState,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;
    let id = item.id;

    // Properties/Settings
    if ui.button("Properties...").clicked() {
        action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::OpenIndicatorSettings(id)
        } else {
            ObjectTreeAction::OpenProperties(id)
        };
        state.close_context_menu();
    }

    // Rename
    if ui.button("Rename...").clicked() {
        state.start_rename(id, item.display_name());
        state.close_context_menu();
    }

    ui.separator();

    // Duplicate (drawings and indicators)
    if item.source_type != SourceType::Template && ui.button("Duplicate").clicked() {
        action = ObjectTreeAction::Duplicate(id);
        state.close_context_menu();
    }

    ui.separator();

    // Z-ordering (drawings only)
    if item.source_type == SourceType::Drawing {
        if ui.button("Bring to Front").clicked() {
            action = ObjectTreeAction::BringToFront(id);
            state.close_context_menu();
        }
        if ui.button("Move Up").clicked() {
            action = ObjectTreeAction::MoveUp(id);
            state.close_context_menu();
        }
        if ui.button("Move Down").clicked() {
            action = ObjectTreeAction::MoveDown(id);
            state.close_context_menu();
        }
        if ui.button("Send to Back").clicked() {
            action = ObjectTreeAction::SendToBack(id);
            state.close_context_menu();
        }

        ui.separator();
    }

    // Visibility toggle
    let vis_label = if item.visible { "Hide" } else { "Show" };
    if ui.button(vis_label).clicked() {
        action = ObjectTreeAction::ToggleVisibility(id);
        state.close_context_menu();
    }

    // Lock toggle (drawings only)
    if item.source_type == SourceType::Drawing {
        let lock_label = if item.locked { "Unlock" } else { "Lock" };
        if ui.button(lock_label).clicked() {
            action = ObjectTreeAction::ToggleLock(id);
            state.close_context_menu();
        }
    }

    ui.separator();

    // Zoom to
    if ui.button("Zoom to Object").clicked() {
        action = ObjectTreeAction::ZoomTo(id);
        state.close_context_menu();
    }

    ui.separator();

    // Delete/Remove
    let delete_label = if item.source_type == SourceType::Indicator {
        "Remove Indicator"
    } else {
        "Delete"
    };
    if ui.button(delete_label).clicked() {
        action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::RemoveIndicator(id)
        } else {
            ObjectTreeAction::Delete(id)
        };
        state.close_context_menu();
    }

    action
}

/// Render general context menu (no specific item)
fn render_general_menu(ui: &mut Ui, state: &mut ObjectTreeState) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    // Show/Hide all
    if ui.button("Show All").clicked() {
        action = ObjectTreeAction::ShowAll;
        state.close_context_menu();
    }
    if ui.button("Hide All").clicked() {
        action = ObjectTreeAction::HideAll;
        state.close_context_menu();
    }

    ui.separator();

    // Lock/Unlock all
    if ui.button("Lock All Drawings").clicked() {
        action = ObjectTreeAction::LockAll;
        state.close_context_menu();
    }
    if ui.button("Unlock All Drawings").clicked() {
        action = ObjectTreeAction::UnlockAll;
        state.close_context_menu();
    }

    ui.separator();

    // Selection operations
    if ui.button("Select All").clicked() {
        action = ObjectTreeAction::SelectAll;
        state.close_context_menu();
    }

    // Delete selected (if any selected)
    if state.selection_count() > 0 {
        ui.separator();

        if ui.button("Delete Selected").clicked() {
            action = ObjectTreeAction::DeleteSelected;
            state.close_context_menu();
        }

        if ui.button("Hide Selected").clicked() {
            action = ObjectTreeAction::HideSelected;
            state.close_context_menu();
        }
    }

    ui.separator();

    // Bulk remove
    ui.menu_button("Remove All...", |ui| {
        if ui.button("Remove All Drawings").clicked() {
            action = ObjectTreeAction::RemoveAllDrawings;
            state.close_context_menu();
            ui.close();
        }
        if ui.button("Remove All Indicators").clicked() {
            action = ObjectTreeAction::RemoveAllIndicators;
            state.close_context_menu();
            ui.close();
        }
    });

    action
}

/// Check if menu should close
fn should_close_menu(ui: &Ui, state: &ObjectTreeState) -> bool {
    // Close on Escape
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        return true;
    }

    // Close on click outside
    // Note: This is a simplified check. In production, you'd want more precise hit testing.
    if ui.input(|i| i.pointer.any_click())
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        // If click is far from menu position, close
        let dist = (pos - state.context_menu_pos).length();
        if dist > 250.0 {
            return true;
        }
    }

    false
}

/// Handle keyboard shortcuts for object tree
pub fn handle_keyboard_shortcuts(
    ui: &Ui,
    state: &mut ObjectTreeState,
    _source_ids: &[usize],
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    ui.input(|i| {
        // Delete key
        if (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
            && state.selection_count() > 0
        {
            action = ObjectTreeAction::DeleteSelected;
        }

        // Ctrl+A - Select all
        if i.modifiers.command && i.key_pressed(Key::A) {
            action = ObjectTreeAction::SelectAll;
        }

        // Ctrl+D - Duplicate
        if i.modifiers.command && i.key_pressed(Key::D) && state.selection_count() > 0 {
            action = ObjectTreeAction::DuplicateSelected;
        }

        // Escape - Clear selection / close menu
        if i.key_pressed(Key::Escape) {
            if state.context_menu_open {
                state.close_context_menu();
            } else if state.renaming_id.is_some() {
                state.cancel_rename();
            } else if state.selection_count() > 0 {
                action = ObjectTreeAction::ClearSelection;
            }
        }

        // Enter - Confirm rename
        if i.key_pressed(Key::Enter)
            && let Some((id, name)) = state.finish_rename()
        {
            action = ObjectTreeAction::Rename(id, name);
        }

        // F2 - Start rename
        if i.key_pressed(Key::F2)
            && let Some(&id) = state.selected_ids.iter().next()
        {
            // Would need to get current name from sources
            // For now, just set renaming_id
            state.renaming_id = Some(id);
        }
    });

    action
}
