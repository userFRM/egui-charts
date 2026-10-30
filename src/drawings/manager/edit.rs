//! Editing the drawing collection: add, delete, layer, group, edit, show, hide and lock.

use super::{DrawingManager, DrawingSyncAction, queue_sync, sync_id};
use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::drawings::services::{ZOrderService, in_z_order};
use egui::Rect;
use std::collections::HashSet;

impl DrawingManager {
    // === Drawing CRUD ===

    /// Deletes the drawing with the given ID from the drawing list.
    ///
    /// The deleted drawing is pushed to the undo history and a cloud sync
    /// delete action is queued. If the deleted drawing was selected, the
    /// selection is cleared. A locked drawing is not deleted.
    pub fn delete_drawing(&mut self, id: usize) {
        if self.drawings.iter().any(|d| d.id == id && d.locked) {
            return;
        }
        if let Some(index) = self.drawings.iter().position(|d| d.id == id) {
            let drawing = self.drawings.remove(index);
            if drawing.saveable {
                self.queue(DrawingSyncAction::Delete(sync_id(&drawing)));
            }
            self.history.push_delete(drawing, index);
        }
        self.selection.deselect(id);
    }

    /// Deletes the selected drawings, keeping any locked ones.
    pub fn delete_selected(&mut self) {
        let before: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, d)| d.selectable && self.selection.is_selected(d.id) && !d.locked)
            .map(|(index, d)| (index, d.clone()))
            .collect();
        if before.is_empty() {
            return;
        }
        for (_, drawing) in &before {
            if drawing.saveable {
                self.queue(DrawingSyncAction::Delete(sync_id(drawing)));
            }
        }
        self.drawings
            .retain(|d| !before.iter().any(|(_, old)| old.id == d.id));
        self.selection
            .prune(&self.drawings.iter().map(|d| d.id).collect());
        self.history.push_snapshot(before);
    }

    /// Removes every drawing that is not locked and clears the current drawing and selection.
    ///
    /// Queues a `Delete` for each drawing it removes, so the host deletes their stored records.
    /// To unload drawings without deleting them, clear [`Self::drawings`], then call
    /// [`Self::cancel_curr_drawing`] and [`Self::deselect`]. A bulk removal is one undo step.
    pub fn clear_all(&mut self) {
        self.history.push_snapshot(
            self.drawings
                .iter()
                .enumerate()
                .filter(|(_, drawing)| !drawing.locked)
                .map(|(index, drawing)| (index, drawing.clone()))
                .collect(),
        );
        for drawing in self.drawings.iter().filter(|d| !d.locked) {
            if drawing.saveable {
                self.queue(DrawingSyncAction::Delete(sync_id(drawing)));
            }
        }
        self.drawings.retain(|d| d.locked);
        self.cancel_curr_drawing();
        self.selection.deselect_all();
    }

    // === Layer ordering ===
    //
    // `Drawing::z_order` decides the layering: drawings are drawn in ascending z-order and hit
    // from the highest down, in list order where it is the same.

    /// Bring a drawing to the front (drawn last, on top), above the highest z-order
    pub fn bring_to_front(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter().find(|drawing| drawing.id == id) {
            self.history.push_modify(id, drawing.clone());
            ZOrderService::bring_to_front(&mut self.drawings, id);
            self.queue_update(id);
        }
    }

    /// Send a drawing to the back (drawn first, behind others), below the lowest z-order
    pub fn send_to_back(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter().find(|drawing| drawing.id == id) {
            self.history.push_modify(id, drawing.clone());
            ZOrderService::send_to_back(&mut self.drawings, id);
            self.queue_update(id);
        }
    }

    /// Apply a layer move as one undo step, saving each changed drawing.
    pub fn reorder_drawings(&mut self, id: usize, layers: &[(usize, i32)]) {
        let mut previous = Vec::new();
        for &(id, layer) in layers {
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id)
                && drawing.z_order != layer
            {
                previous.push((id, drawing.z_order));
                drawing.z_order = layer;
                self.queue(DrawingSyncAction::UpdateDrawing(id));
            }
        }
        if !previous.is_empty() {
            self.history.push_reorder(id, previous);
        }
    }

    /// Duplicate a drawing three bars to the right, recording its creation for undo and saving.
    pub fn duplicate_drawing(&mut self, id: usize) {
        let Some(mut copy) = self.drawings.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        copy.stored_id = None;
        copy.created_at = None;
        copy.screen_anchor = copy.screen_anchor.map(|[x, y]| [x + 0.02, y]);
        for point in &mut copy.chart_points {
            point.bar_idx += 3.0;
        }
        self.add_synced_drawing(copy);
    }

    /// Paste copies three bars to the right with fresh identities, selecting them as one undoable addition.
    /// The caller maps clipboard anchors into this manager's chart coordinates first.
    pub fn paste_drawings(&mut self, drawings: &[Drawing]) {
        if drawings.is_empty() {
            return;
        }
        let mut ids = Vec::new();
        for drawing in in_z_order(drawings) {
            let mut copy = drawing.clone();
            copy.id = self.take_id();
            copy.z_order = self.top_z_order();
            copy.stored_id = None;
            copy.created_at = None;
            copy.screen_anchor = copy.screen_anchor.map(|[x, y]| [x + 0.02, y]);
            copy.points.clear();
            copy.level_ys.clear();
            for point in &mut copy.chart_points {
                *point = point.with_shifted_bar_idx(3.0);
            }
            ids.push(copy.id);
            let id = copy.id;
            self.drawings.push(copy);
            self.queue(DrawingSyncAction::SaveDrawing(id));
        }
        self.select_drawings(ids.iter().copied());
        self.history
            .push_command(crate::drawings::services::DrawingCommand::Snapshot {
                drawings: Vec::new(),
                ids,
            });
    }

    /// Applies `edit` to the drawing with the given ID, records it for undo and queues it to
    /// be saved. Edit a drawing through this rather than [`Self::drawings`] so the change is
    /// saved. A locked drawing accepts only visibility, name and folder changes, or an edit that unlocks it.
    /// Does nothing if there is no such drawing.
    pub fn edit_drawing(&mut self, id: usize, edit: impl FnOnce(&mut Drawing)) {
        if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id) {
            let old = drawing.clone();
            edit(drawing);
            if old.locked && drawing.locked {
                let visible = drawing.visible;
                let label = drawing.label.clone();
                let folder = drawing.folder.clone();
                *drawing = old.clone();
                if visible == old.visible && label == old.label && folder == old.folder {
                    return;
                }
                drawing.visible = visible;
                drawing.label = label;
                drawing.folder = folder;
            }
            if !drawing.selectable {
                self.selection.deselect(id);
            }
            self.queue_edit(&old);
            self.history.push_modify(id, old);
            self.painted
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|p| p.id != id);
        }
    }

    /// Edits live drawing ids as one undo step and queues their saved changes.
    /// Locked drawings accept visibility, name and folder changes, or an edit that unlocks them.
    pub fn edit_drawings(&mut self, ids: &[usize], mut edit: impl FnMut(&mut Drawing)) {
        if ids.is_empty() {
            return;
        }
        let ids: HashSet<_> = ids.iter().copied().collect();
        let mut before = Vec::new();
        for (index, drawing) in self
            .drawings
            .iter_mut()
            .enumerate()
            .filter(|(_, d)| ids.contains(&d.id))
        {
            let old = drawing.clone();
            edit(drawing);
            if old.locked && drawing.locked {
                let (visible, label, folder) = (
                    drawing.visible,
                    drawing.label.clone(),
                    drawing.folder.clone(),
                );
                *drawing = old.clone();
                if visible == old.visible && label == old.label && folder == old.folder {
                    continue;
                }
                drawing.visible = visible;
                drawing.label = label;
                drawing.folder = folder;
            }
            if !drawing.selectable {
                self.selection.deselect(old.id);
            }
            before.push((index, old));
        }
        for (_, old) in &before {
            self.queue_edit(old);
        }
        self.history.push_snapshot(before);
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !ids.contains(&p.id));
    }

    /// Applies `edit` as one frame of a continuous edit, such as dragging a colour picker: the
    /// first frame records the drawing's state and nothing is queued until
    /// [`Self::end_drag_handle`], which makes the whole gesture one undo step and queues one
    /// update. Does nothing if there is no such drawing or it is locked.
    pub fn drag_edit_drawing(&mut self, id: usize, edit: impl FnOnce(&mut Drawing)) {
        if self.drag_old_state.as_ref().is_some_and(|d| d.id != id) {
            self.end_drag_handle();
        }
        if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id && !d.locked) {
            self.drag_old_state.get_or_insert_with(|| drawing.clone());
            edit(drawing);
            self.painted
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|p| p.id != id);
        }
    }

    // === Coordinate updates ===

    /// Recomputes screen coordinates for all drawings (completed and in-progress)
    /// from their persistent chart coordinates using the current pan/zoom transforms.
    ///
    /// Must be called each frame before rendering.
    pub fn update_all_screen_coords<F, G>(&mut self, bar_to_x: F, price_to_y: G)
    where
        F: Fn(f32) -> f32 + Copy,
        G: Fn(f64) -> f32 + Copy,
    {
        for drawing in &mut self.drawings {
            drawing.update_screen_coords(bar_to_x, price_to_y);
        }
        if let Some(ref mut drawing) = self.curr_drawing {
            drawing.update_screen_coords(bar_to_x, price_to_y);
        }
        if let Some(drawing) = &mut self.measurement {
            drawing.update_screen_coords(bar_to_x, price_to_y);
        }
    }

    /// Starts a widget pane pass. Standalone managers keep the default main-pane context.
    pub(crate) fn set_pane(
        &mut self,
        pane: Option<usize>,
        members: Vec<Option<usize>>,
        rect: Rect,
    ) {
        if pane.is_none() {
            self.pane_clips.clear();
        }
        for member in &members {
            self.pane_clips.insert(*member, rect);
        }
        self.pane = pane;
        self.pane_members = members;
        self.pane_rect = rect;
    }

    pub(crate) fn owns_pane(&self, drawing: &Drawing) -> bool {
        self.pane_members.contains(&drawing.pane)
    }

    pub(crate) fn update_pane_screen_coords(
        &mut self,
        bar_to_x: impl Fn(f32) -> f32 + Copy,
        price_to_y: impl Fn(f64) -> f32 + Copy,
    ) {
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.measurement)
        {
            if self.pane_members.contains(&drawing.pane) {
                drawing.update_screen_coords(bar_to_x, price_to_y);
                drawing.update_pane_coords(self.pane_rect);
            }
        }
    }

    /// Projects anchored annotations into `pane` after chart-coordinate updates.
    /// The chart calls this automatically; standalone managers call it before rendering.
    pub fn update_pane_anchors(&mut self, pane: Rect) {
        self.pane_rect = pane;
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.measurement)
        {
            drawing.update_pane_coords(pane);
        }
    }

    /// Shifts every bar index at or after `from` by `shift`, across all
    /// drawings, the in-progress drawing, the state a handle drag restores on
    /// undo, and the undo history.
    ///
    /// This is called when bars are inserted in front of existing ones, causing
    /// the bar indices from there on to shift right. Pass `f32::NEG_INFINITY`
    /// for bars inserted in front of the first bar.
    pub fn shift_bar_indices(&mut self, from: f32, shift: f32) {
        if shift.abs() < 0.001 {
            return;
        }
        for drawing in &mut self.drawings {
            drawing.shift_bar_indices(from, shift);
        }
        if let Some(ref mut curr) = self.curr_drawing {
            curr.shift_bar_indices(from, shift);
        }
        if let Some(drawing) = &mut self.measurement {
            drawing.shift_bar_indices(from, shift);
        }
        if let Some(old) = &mut self.drag_old_state {
            old.shift_bar_indices(from, shift);
        }
        for drawing in &mut self.drag_group {
            drawing.shift_bar_indices(from, shift);
        }
        self.history.shift_bar_indices(from, shift);
    }

    // === Visibility ===

    /// Toggles the visibility of all drawings. If all are visible, hides them
    /// all; otherwise shows them all.
    pub fn toggle_visibility_all(&mut self) {
        let visible = !self.drawings.iter().all(|drawing| drawing.visible);
        self.set_visibility_all(visible);
    }

    /// Set every drawing's visibility as one undoable edit.
    pub fn set_visibility_all(&mut self, visible: bool) {
        let mut before = Vec::new();
        for (index, drawing) in self
            .drawings
            .iter_mut()
            .enumerate()
            .filter(|(_, drawing)| drawing.visible != visible)
        {
            before.push((index, drawing.clone()));
            drawing.visible = visible;
            if self.sync_enabled
                && drawing.saveable
                && let Ok(mut sync) = self.pending_sync.lock()
            {
                queue_sync(
                    &mut sync,
                    DrawingSyncAction::UpdateDrawing(drawing.id),
                    Some(&sync_id(drawing)),
                );
            }
        }
        self.history.push_snapshot(before);
    }

    /// Toggles the lock state of all drawings. If all are locked, unlocks them
    /// all; otherwise locks them all.
    pub fn toggle_lock_all(&mut self) {
        let all_locked = self.drawings.iter().all(|d| d.locked);
        let mut before = Vec::new();
        for (index, drawing) in self
            .drawings
            .iter_mut()
            .enumerate()
            .filter(|(_, d)| d.locked == all_locked)
        {
            before.push((index, drawing.clone()));
            drawing.locked = !all_locked;
            if self.sync_enabled
                && drawing.saveable
                && let Ok(mut sync) = self.pending_sync.lock()
            {
                queue_sync(
                    &mut sync,
                    DrawingSyncAction::UpdateDrawing(drawing.id),
                    Some(&sync_id(drawing)),
                );
            }
        }
        self.history.push_snapshot(before);
    }

    /// Adds a completed drawing with validated chart coordinates and properties as one undo
    /// step. Assigns a fresh id, retains the supplied layer, and queues a save if `saveable`.
    /// Use `Chart::create_drawing` when the anchors are UTC times rather than bar indices.
    pub fn add_drawing(&mut self, mut drawing: Drawing) -> Result<usize, &'static str> {
        self.validate_drawing(&drawing)?;
        if drawing.custom_tool.is_none() && !self.registry.availability(drawing.tool_type).enabled {
            return Err("Drawing tool is disabled");
        }
        drawing.id = self.take_id();
        drawing.stored_id = None;
        drawing.created_at = None;
        drawing.completed = true;
        drawing.points.clear();
        drawing.level_ys.clear();
        let id = drawing.id;
        self.history.push_add(drawing.clone());
        self.drawings.push(drawing);
        self.queue(DrawingSyncAction::SaveDrawing(id));
        Ok(id)
    }

    pub(crate) fn validate_drawing(&self, drawing: &Drawing) -> Result<(), &'static str> {
        crate::drawings::validation::validate(drawing)?;
        if let Some(id) = &drawing.custom_tool {
            let tool = self.registry.get(id).ok_or("Unknown drawing tool")?;
            if drawing.tool_type != DrawingToolType::Polyline
                || drawing.chart_points.len() != tool.anchors
            {
                return Err("Invalid custom drawing anchors");
            }
        }
        Ok(())
    }

    /// Drawings included in host object lists, in drawing-list order.
    pub fn listed_drawings(&self) -> impl Iterator<Item = &Drawing> {
        self.drawings.iter().filter(|drawing| drawing.listed)
    }

    /// Members of a host-defined group, including hidden and locked drawings.
    pub fn group_drawings<'a>(&'a self, group: &'a str) -> impl Iterator<Item = &'a Drawing> {
        self.drawings
            .iter()
            .filter(move |drawing| drawing.group.as_deref() == Some(group))
    }

    /// Assigns or removes group membership for live ids as one undo step. Locked drawings
    /// keep their membership. Group names are host-owned and are saved with each member.
    pub fn set_group(&mut self, ids: &[usize], group: Option<String>) {
        let mut before = Vec::new();
        for (index, drawing) in self.drawings.iter_mut().enumerate() {
            if ids.contains(&drawing.id) && !drawing.locked && drawing.group != group {
                before.push((index, drawing.clone()));
                drawing.group = group.clone();
            }
        }
        for (_, old) in &before {
            self.queue_update(old.id);
        }
        self.history.push_snapshot(before);
    }

    /// Selects all selectable members of a group for the existing move, edit and delete
    /// operations. Those operations retain their lock checks and single undo step.
    pub fn select_group(&mut self, group: &str) {
        let ids: Vec<_> = self.group_drawings(group).map(|d| d.id).collect();
        self.select_drawings(ids);
    }

    /// Add a drawing from another source (for multi-chart sync, or restored from storage)
    /// under a new id. A drawing restored from storage keeps its stored identity (`stored_id`,
    /// `created_at`, `notes`) and its z-order. A drawing that was never stored goes on top, and
    /// so does a copy, one stored under the id of a drawing already here, which is saved as a
    /// new drawing.
    pub fn add_synced_drawing(&mut self, mut drawing: Drawing) {
        if drawing.stored_id.is_some()
            && self
                .drawings
                .iter()
                .any(|d| d.stored_id == drawing.stored_id)
        {
            (drawing.stored_id, drawing.created_at) = (None, None);
        }
        if drawing.stored_id.is_none() {
            drawing.z_order = self.top_z_order();
        }
        // Assign a new ID to avoid conflicts
        drawing.id = self.take_id();
        if drawing.stored_id.is_none() {
            if drawing.saveable {
                self.queue(DrawingSyncAction::SaveDrawing(drawing.id));
            }
            self.history.push_add(drawing.clone());
        }
        self.drawings.push(drawing);
    }
}
