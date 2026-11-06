//! Editing the drawing collection: add, delete, layer, group, file, edit, show, hide and lock.

use super::{DrawingManager, DrawingSyncAction, queue_sync, sync_id};
use crate::drawings::domain::{Drawing, DrawingToolType, FolderSync};
use crate::drawings::services::{ZOrderService, in_z_order};
use crate::model::PostContent;
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

    /// Stores newly typed text in its placement undo step, or removes an empty placement
    /// without an undo step. If another operation followed placement, records a normal edit
    /// or deletion instead.
    pub(crate) fn finish_new_text(&mut self, id: usize, text: Option<String>) {
        let Some(index) = self.drawings.iter().position(|d| d.id == id) else {
            return;
        };
        if !self.history.pop_add(id) {
            self.finished_text_placement = Some(id);
            match text {
                Some(text) => self.edit_drawing(id, |d| d.text = Some(text)),
                None => self.delete_drawing(id),
            }
            return;
        }
        match text {
            Some(text) => {
                self.drawings[index].text = Some(text);
                self.history.push_add(self.drawings[index].clone());
                self.queue_update(id);
            }
            None => {
                let drawing = self.drawings.remove(index);
                if drawing.saveable {
                    self.queue(DrawingSyncAction::Delete(sync_id(&drawing)));
                }
                self.selection.deselect(id);
            }
        }
    }

    /// Deletes the selected drawings, keeping any locked ones.
    pub fn delete_selected(&mut self) {
        let ids: Vec<_> = self.selected_ids().collect();
        self.delete_drawings(&ids);
    }

    /// Deletes the given drawings as one undo step, keeping any locked ones.
    pub fn delete_drawings(&mut self, ids: &[usize]) {
        let unlocked: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| ids.contains(&d.id) && !d.locked)
            .map(|d| d.id)
            .collect();
        self.remove_drawings(&unlocked);
    }

    /// Deletes the given drawings as one undo step, locked ones included.
    pub(crate) fn remove_drawings(&mut self, ids: &[usize]) {
        let before: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, d)| ids.contains(&d.id))
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
        self.clear_all_with_locked(false);
    }

    /// Bulk removal with explicit permission to include locked drawings. Preserves locks in
    /// undo and queues deletion of stored records exactly as [`Self::clear_all`] does.
    pub fn clear_all_with_locked(&mut self, include_locked: bool) {
        self.history.push_snapshot(
            self.drawings
                .iter()
                .enumerate()
                .filter(|(_, drawing)| include_locked || !drawing.locked)
                .map(|(index, drawing)| (index, drawing.clone()))
                .collect(),
        );
        for drawing in self.drawings.iter().filter(|d| include_locked || !d.locked) {
            if drawing.saveable {
                self.queue(DrawingSyncAction::Delete(sync_id(drawing)));
            }
        }
        self.drawings.retain(|d| d.locked && !include_locked);
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
        let edited = self
            .drawings
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| self.modified_at(d));
        let mut previous = Vec::new();
        for &(id, layer) in layers {
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id)
                && drawing.z_order != layer
            {
                previous.push((id, drawing.z_order));
                drawing.z_order = layer;
                // Only the moved drawing is edited; tied layers around it are just renumbered.
                self.notify(DrawingSyncAction::UpdateDrawing(id));
            }
        }
        if !previous.is_empty() {
            self.stamp(id);
            self.history.push_reorder(id, previous, edited);
        }
    }

    /// Duplicate a drawing three bars to the right, recording its creation for undo and saving.
    /// Adds nothing for a drawing of a disabled tool or a copy that saving would refuse.
    pub fn duplicate_drawing(&mut self, id: usize) {
        let Some(mut copy) = self.drawings.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        copy.stored_id = None;
        copy.created_at = None;
        copy.screen_anchor = copy.screen_anchor.map(|[x, y]| [x + 0.02, y]);
        let first = copy.first_moved_anchor();
        for point in copy.chart_points.iter_mut().skip(first) {
            point.bar_idx += 3.0;
        }
        if self.validate_created(&copy).is_ok() {
            self.push_synced(copy);
        }
    }

    /// Paste copies three bars to the right with fresh identities, selecting them as one undoable addition.
    /// The caller maps clipboard anchors into this manager's chart coordinates first. Leaves out a
    /// copy of a disabled tool or an unregistered custom tool and a copy that saving would refuse;
    /// when it leaves out every copy, it selects nothing and records no undo step.
    pub fn paste_drawings(&mut self, drawings: &[Drawing]) {
        let mut ids = Vec::new();
        for drawing in in_z_order(drawings) {
            let mut copy = drawing.clone();
            copy.stored_id = None;
            copy.created_at = None;
            copy.screen_anchor = copy.screen_anchor.map(|[x, y]| [x + 0.02, y]);
            copy.points.clear();
            copy.level_ys.clear();
            let first = copy.first_moved_anchor();
            for point in copy.chart_points.iter_mut().skip(first) {
                *point = point.with_shifted_bar_idx(3.0);
            }
            if self.validate_created(&copy).is_err() {
                continue;
            }
            copy.id = self.take_id();
            copy.z_order = self.top_z_order();
            ids.push(copy.id);
            let id = copy.id;
            self.drawings.push(copy);
            self.queue(DrawingSyncAction::SaveDrawing(id));
        }
        if ids.is_empty() {
            return;
        }
        self.select_drawings(ids.iter().copied());
        self.history
            .push_command(crate::drawings::services::DrawingCommand::Snapshot {
                drawings: Vec::new(),
                ids,
            });
    }

    /// Replaces every saveable drawing with `drawings`, restored from storage, as one undo step.
    /// Each keeps its stored identity (`stored_id`, `created_at`, `notes`) and layer and gets a
    /// new id; drawings that are never saved stay. Queues a `Delete` for each drawing it removes
    /// and a save for each it adds.
    pub(crate) fn replace_saved_drawings(&mut self, drawings: Vec<Drawing>) {
        let removed: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, drawing)| drawing.saveable)
            .map(|(index, drawing)| (index, drawing.clone()))
            .collect();
        if removed.is_empty() && drawings.is_empty() {
            return;
        }
        let mut ids: Vec<_> = removed.iter().map(|(_, drawing)| drawing.id).collect();
        // Taken while the replaced drawings are listed, so that no id is handed out twice.
        let mut added = Vec::with_capacity(drawings.len());
        for mut drawing in drawings {
            drawing.id = self.take_id();
            ids.push(drawing.id);
            added.push(drawing);
        }
        for (_, drawing) in &removed {
            self.queue(DrawingSyncAction::Delete(sync_id(drawing)));
        }
        self.drawings.retain(|drawing| !drawing.saveable);
        for drawing in added {
            let id = drawing.id;
            self.drawings.push(drawing);
            self.queue(DrawingSyncAction::SaveDrawing(id));
        }
        self.selection
            .prune(&self.drawings.iter().map(|d| d.id).collect());
        self.history
            .push_command(crate::drawings::services::DrawingCommand::Snapshot {
                drawings: removed,
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
                let timeframe_visibility = drawing.timeframe_visibility.clone();
                let label = drawing.label.clone();
                let folder = drawing.folder.clone();
                let folder_sync = drawing.folder_sync.clone();
                *drawing = old.clone();
                if visible == old.visible
                    && timeframe_visibility == old.timeframe_visibility
                    && label == old.label
                    && folder == old.folder
                    && folder_sync == old.folder_sync
                {
                    return;
                }
                drawing.visible = visible;
                drawing.timeframe_visibility = timeframe_visibility;
                drawing.label = label;
                drawing.folder = folder;
                drawing.folder_sync = folder_sync;
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

    /// Gives the drawings whose post waits for `link` the host's answer: its content, or `None`
    /// when the host could not resolve it. Not an undoable edit, also on locked drawings;
    /// queues each drawing that received content to be saved. Returns whether any did.
    pub(crate) fn resolve_link(&mut self, link: &str, content: Option<&PostContent>) -> bool {
        let mut updated = Vec::new();
        for drawing in &mut self.drawings {
            if let Some(post) = &mut drawing.post
                && post.link == link
                && post.content.is_none()
            {
                if let Some(content) = content {
                    post.content = Some(content.clone());
                    post.failed = false;
                    updated.push(drawing.id);
                } else {
                    post.failed = true;
                }
            }
        }
        for &id in &updated {
            self.queue_update(id);
        }
        !updated.is_empty()
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
                let (visible, timeframe_visibility, label, folder, folder_sync) = (
                    drawing.visible,
                    drawing.timeframe_visibility.clone(),
                    drawing.label.clone(),
                    drawing.folder.clone(),
                    drawing.folder_sync.clone(),
                );
                *drawing = old.clone();
                if visible == old.visible
                    && timeframe_visibility == old.timeframe_visibility
                    && label == old.label
                    && folder == old.folder
                    && folder_sync == old.folder_sync
                {
                    continue;
                }
                drawing.visible = visible;
                drawing.timeframe_visibility = timeframe_visibility;
                drawing.label = label;
                drawing.folder = folder;
                drawing.folder_sync = folder_sync;
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
        self.apply_edit_times();
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
        self.apply_edit_times();
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
        self.validate_custom_tool(drawing)
    }

    /// A registered tool's drawing must be a polyline with exactly the tool's anchors.
    fn validate_custom_tool(&self, drawing: &Drawing) -> Result<(), &'static str> {
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

    /// Checks a drawing the user creates by duplicating, pasting, copy dragging or placing it:
    /// what saving needs, a registered tool's anchors and the tool's availability, as
    /// `add_drawing` does. A drawing finished with fewer anchors than the toolbar collects
    /// passes when its tool draws with them.
    pub(super) fn validate_created(&self, drawing: &Drawing) -> Result<(), &'static str> {
        crate::drawings::validation::validate_stored(drawing)?;
        self.validate_custom_tool(drawing)?;
        if drawing.custom_tool.is_none() && !self.registry.availability(drawing.tool_type).enabled {
            return Err("Drawing tool is disabled");
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

    /// Moves `ids` into the folder at `path`, and each of `folders` with its contents under
    /// `path`, as one undo step; an empty `path` is the root. Everything moved follows the
    /// synchronization of its new top-level folder. A folder reaching the root keeps the mode set
    /// on it, or else the one it followed so far.
    ///
    /// Moves nothing and returns `false` if a folder would enter itself or a descendant, or take
    /// the name of a folder already there.
    pub fn move_to_folder(
        &mut self,
        ids: &[usize],
        folders: &[Vec<String>],
        path: &[String],
    ) -> bool {
        let mut names = HashSet::new();
        for folder in folders {
            let Some(name) = folder.last() else {
                return false;
            };
            let target = [path, std::slice::from_ref(name)].concat();
            if path.starts_with(folder)
                || !names.insert(name)
                || self.drawings.iter().any(|d| d.folder.starts_with(&target))
            {
                return false;
            }
        }
        let modes = self.folder_modes(path);
        let moved: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| ids.contains(&d.id) || folders.iter().any(|f| d.folder.starts_with(f)))
            .map(|d| d.id)
            .collect();
        self.edit_drawings(&moved, |d| {
            let (at, own) = folders.iter().find(|f| d.folder.starts_with(f)).map_or(
                (d.folder.len(), Vec::new()),
                |folder| {
                    let at = folder.len() - 1;
                    let mut own = d.folder_sync.get(at..).unwrap_or_default().to_vec();
                    if path.is_empty() && own.first().copied().flatten().is_none() {
                        own.resize(own.len().max(1), None);
                        own[0] = d.folder_sync_mode();
                    }
                    (at, own)
                },
            );
            d.folder = [path, &d.folder[at..]].concat();
            d.folder_sync = [modes.as_slice(), own.as_slice()].concat();
        });
        true
    }

    /// The synchronization set on each folder along `path`.
    fn folder_modes(&self, path: &[String]) -> Vec<Option<FolderSync>> {
        (1..=path.len())
            .map(|depth| {
                self.drawings
                    .iter()
                    .filter(|d| d.folder.starts_with(&path[..depth]))
                    .find_map(|d| d.folder_sync.get(depth - 1).copied().flatten())
            })
            .collect()
    }

    /// Sets the synchronization of a top-level folder, which its descendants follow, as one undo
    /// step. Returns `false` for a nested folder, which follows its top-level folder.
    pub fn set_folder_sync(&mut self, folder: &[String], sync: Option<FolderSync>) -> bool {
        if folder.len() != 1 {
            return false;
        }
        let ids: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| d.folder.starts_with(folder) && d.folder_sync_mode() != sync)
            .map(|d| d.id)
            .collect();
        self.edit_drawings(&ids, |d| {
            d.folder_sync.resize(d.folder_sync.len().max(1), None);
            d.folder_sync[0] = sync;
        });
        true
    }

    /// Add a drawing from another source (for multi-chart sync, or restored from storage)
    /// under a new id, and return the id. A drawing restored from storage keeps its stored
    /// identity (`stored_id`, `created_at`, `notes`) and its z-order. A drawing that was never
    /// stored goes on top, and so does a copy, one stored under the id of a drawing already
    /// here, which is saved as a new drawing.
    ///
    /// Refuses, and adds nothing, a drawing that saving would refuse: a cursor tool, fewer
    /// anchors than the tool draws with, a non-finite coordinate, an anchor too far out to map
    /// to a time or a setting out of range. Tool availability does not apply: drawings of a
    /// disabled tool are restored and synced.
    pub fn add_synced_drawing(&mut self, drawing: Drawing) -> Result<usize, &'static str> {
        crate::drawings::validation::validate_stored(&drawing)?;
        Ok(self.push_synced(drawing))
    }

    /// Adds a checked drawing under a new id, as [`Self::add_synced_drawing`] describes.
    fn push_synced(&mut self, mut drawing: Drawing) -> usize {
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
        let id = drawing.id;
        self.drawings.push(drawing);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::ChartPoint;

    fn path(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn manager(folders: &[&[&str]]) -> DrawingManager {
        let mut manager = DrawingManager::new();
        for folder in folders {
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 1.0), ChartPoint::new(2.0, 2.0)];
            drawing.completed = true;
            drawing.folder = path(folder);
            manager.add_synced_drawing(drawing).unwrap();
        }
        manager
    }

    fn folders(manager: &DrawingManager) -> Vec<Vec<String>> {
        manager.drawings.iter().map(|d| d.folder.clone()).collect()
    }

    /// A folder cannot enter itself or a descendant or overwrite a sibling, and a refused move
    /// changes nothing and leaves no undo step.
    #[test]
    fn refused_folder_moves_change_nothing() {
        let tree: &[&[&str]] = &[&["A", "B", "C"], &["A", "X"], &["X"]];
        for (folder, parent) in [
            (&["A"][..], &["A"][..]),
            (&["A"], &["A", "B"]),
            (&["A", "B"], &["A", "B", "C"]),
            // Its parent already holds it, and the root already holds a folder of that name.
            (&["A", "B"], &["A"]),
            (&["A", "X"], &[]),
        ] {
            let mut manager = manager(tree);
            let before = folders(&manager);
            manager.history.clear();
            assert!(
                !manager.move_to_folder(&[], &[path(folder)], &path(parent)),
                "{folder:?} into {parent:?}"
            );
            assert_eq!(folders(&manager), before);
            assert!(!manager.undo());
        }
    }

    /// A nested folder follows its top-level folder. Back at the root it has the mode set on it,
    /// or else the one it followed.
    #[test]
    fn nesting_inherits_and_unnesting_retains_synchronization() {
        use FolderSync::{Chart, Layout};
        for (own, parent, nested, unnested) in [
            (Some(Layout), Some(Chart), Some(Chart), Some(Layout)),
            (None, Some(Layout), Some(Layout), Some(Layout)),
            (Some(Chart), None, None, Some(Chart)),
            (None, None, None, None),
        ] {
            let mut manager = manager(&[&["Child", "Inner"], &["Top"]]);
            assert!(manager.set_folder_sync(&path(&["Child"]), own));
            assert!(manager.set_folder_sync(&path(&["Top"]), parent));
            assert!(manager.move_to_folder(&[], &[path(&["Child"])], &path(&["Top"])));
            let moved = &manager.drawings[0];
            assert_eq!(moved.folder, path(&["Top", "Child", "Inner"]));
            assert_eq!(moved.folder_sync_mode(), nested);
            // A nested folder follows; only its top-level folder sets the mode.
            assert!(!manager.set_folder_sync(&path(&["Top", "Child"]), Some(Chart)));
            assert!(manager.move_to_folder(&[], &[path(&["Top", "Child"])], &[]));
            let moved = &manager.drawings[0];
            assert_eq!(moved.folder, path(&["Child", "Inner"]));
            assert_eq!(moved.folder_sync_mode(), unnested);
        }
    }

    /// A bulk delete skips locked drawings, and its undo restores the rest with their nesting.
    #[test]
    fn deleting_drawings_keeps_locked_ones_in_one_undo_step() {
        let mut manager = manager(&[&["A"], &["A", "B"], &["A", "B"]]);
        manager.drawings[0].locked = true;
        let before = folders(&manager);
        let ids: Vec<_> = manager.drawings.iter().map(|d| d.id).collect();
        manager.delete_drawings(&ids);
        assert_eq!(manager.drawings.len(), 1);
        assert!(manager.undo());
        assert_eq!(folders(&manager), before);
    }
}
