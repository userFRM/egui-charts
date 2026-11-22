//! Editing the drawing collection: add, delete, layer, group, file, edit, show, hide and lock.

use super::{DrawingEventKind, DrawingManager, DrawingSyncAction, sync_id};
use crate::drawings::domain::{Drawing, DrawingToolType, FolderSync};
use crate::drawings::services::{HistoryService, ZOrderService, in_z_order};
use crate::model::PostContent;
use egui::Rect;
use std::collections::HashSet;

/// Original drawings and history while a series removal changes their owner and layer.
pub(crate) struct DrawingRemoval {
    before: Vec<Drawing>,
    history: HistoryService,
}

impl DrawingManager {
    // === Drawing CRUD ===

    /// Deletes the drawing with the given ID from the drawing list.
    ///
    /// The deleted drawing is pushed to the undo history and a cloud sync
    /// delete action is queued. If the deleted drawing was selected, the
    /// selection is cleared. A locked drawing is not deleted.
    /// [`Drawing::user_edit_enabled`] does not limit it: hosts edit drawings the user cannot.
    pub fn delete_drawing(&mut self, id: usize) {
        if self.drawings.iter().any(|d| d.id == id && d.locked) {
            return;
        }
        if let Some(index) = self.drawings.iter().position(|d| d.id == id) {
            let drawing = self.drawings.remove(index);
            if drawing.saveable {
                self.queue(DrawingSyncAction::Delete(sync_id(&drawing)));
            }
            self.report(id, DrawingEventKind::Removed);
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
                Some(text) => {
                    let _ = self.edit_drawing(id, |d| d.text = Some(text));
                }
                None => self.delete_drawing(id),
            }
            return;
        }
        match text {
            Some(text) => {
                let changed = self.drawings[index].text.as_deref() != Some(text.as_str());
                self.drawings[index].text = Some(text);
                self.history.push_add(self.drawings[index].clone());
                self.queue_update(id);
                if changed {
                    self.report(id, DrawingEventKind::PropertiesChanged);
                }
            }
            None => {
                let drawing = self.drawings.remove(index);
                if drawing.saveable {
                    self.queue(DrawingSyncAction::Delete(sync_id(&drawing)));
                }
                self.report(id, DrawingEventKind::Removed);
                self.selection.deselect(id);
            }
        }
    }

    /// Deletes the selected drawings, keeping locked ones and those whose user edits are off.
    pub fn delete_selected(&mut self) {
        let ids: Vec<_> = self
            .selected_ids()
            .filter(|&id| {
                self.drawings
                    .iter()
                    .any(|d| d.id == id && self.accepts_user_edits(d))
            })
            .collect();
        self.delete_drawings(&ids);
    }

    /// Shows or hides the selected drawings as one undo step, locked ones included, and queues
    /// the ones that change to be saved. Hiding deselects them, as [`Self::edit_drawing`] does.
    /// Drawings already in that state are left alone; without such a drawing nothing is recorded.
    pub fn set_selected_visible(&mut self, visible: bool) {
        self.edit_selected_where(|d| d.visible != visible, |d| d.visible = visible);
    }

    /// Locks or unlocks the selected drawings as one undo step and queues the ones that change to
    /// be saved. Drawings already in that state are left alone; without such a drawing nothing is
    /// recorded. Drawings whose user edits are off stay as they are.
    pub fn set_selected_locked(&mut self, locked: bool) {
        self.edit_selected_where(
            |d| d.locked != locked && d.user_edit_enabled,
            |d| d.locked = locked,
        );
    }

    /// Applies `edit` as one undo step to the selected drawings that `differs` picks, locked ones
    /// included, unlike [`Self::edit_selected`].
    fn edit_selected_where(
        &mut self,
        differs: impl Fn(&Drawing) -> bool,
        edit: impl FnMut(&mut Drawing),
    ) {
        let ids: Vec<_> = self
            .drawings
            .iter()
            .filter(|d| {
                self.shared_filter.selectable(d) && self.selection.is_selected(d.id) && differs(d)
            })
            .map(|d| d.id)
            .collect();
        // Visibility and the lock take no part in validation, so the edit is never refused.
        let _ = self.edit_drawings(&ids, edit);
    }

    /// Deletes the given drawings as one undo step, keeping any locked ones.
    /// [`Drawing::user_edit_enabled`] does not limit it: hosts edit drawings the user cannot.
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
            self.report(drawing.id, DrawingEventKind::Removed);
        }
        self.drawings
            .retain(|d| !before.iter().any(|(_, old)| old.id == d.id));
        self.selection
            .prune(&self.drawings.iter().map(|d| d.id).collect());
        self.history.push_snapshot(before);
    }

    /// Gives every drawing that `from` owns the owner `to`, locked ones included, as one undo
    /// step, and queues each to be saved. See [`Drawing::pane`].
    pub(crate) fn reassign_owner(&mut self, from: usize, to: Option<usize>) {
        let mut before = Vec::new();
        for (index, drawing) in self.drawings.iter_mut().enumerate() {
            if drawing.pane == Some(from) {
                before.push((index, drawing.clone()));
                drawing.pane = to;
            }
        }
        for (_, old) in &before {
            self.queue_update(old.id);
            self.report_change(old.id, Some(old), false);
        }
        self.history.push_snapshot(before);
    }

    /// Gives each drawing in `layers`, as `(id, series_layer, z_order)`, its new place after
    /// its pane's items changed, locked drawings included, and queues it to be saved. With
    /// `record`, the changes form one undo step.
    pub(crate) fn restack(&mut self, layers: &[(usize, usize, i32)], record: bool) {
        let mut before = Vec::new();
        for (index, drawing) in self.drawings.iter_mut().enumerate() {
            if let Some(&(_, layer, z)) = layers.iter().find(|(id, ..)| *id == drawing.id)
                && (drawing.series_layer, drawing.z_order) != (Some(layer), z)
            {
                before.push((index, drawing.clone()));
                drawing.series_layer = Some(layer);
                drawing.z_order = z;
            }
        }
        for (_, old) in &before {
            self.queue_update(old.id);
            self.report(old.id, DrawingEventKind::PropertiesChanged);
        }
        if record {
            self.history.push_snapshot(before);
        }
    }

    /// Holds the intermediate owner, layer and deletion changes of one series removal.
    pub(crate) fn begin_removal(&mut self) -> DrawingRemoval {
        debug_assert!(self.removal.is_none(), "series removals do not nest");
        self.removal = Some(Vec::new());
        DrawingRemoval {
            before: self.drawings.clone(),
            history: std::mem::replace(&mut self.history, HistoryService::with_max_size(0)),
        }
    }

    /// Reports each changed drawing once and keeps its original state in one undo snapshot.
    pub(crate) fn finish_removal(&mut self, removal: DrawingRemoval) {
        let changed = self.removal.take().unwrap_or_default();
        for &id in &changed {
            self.report_change(id, removal.before.iter().find(|d| d.id == id), false);
        }
        let record = self.history.can_undo();
        self.history = removal.history;
        if record {
            self.history.push_snapshot(
                removal
                    .before
                    .into_iter()
                    .enumerate()
                    .filter(|(_, drawing)| changed.contains(&drawing.id))
                    .collect(),
            );
        }
    }

    /// Removes unlocked, user-editable drawings outside the shared-drawing filter and
    /// clears the current drawing and selection.
    ///
    /// Queues a `Delete` for each drawing it removes, so the host deletes their stored records.
    /// To unload drawings without deleting them, clear [`Self::drawings`], then call
    /// [`Self::cancel_curr_drawing`] and [`Self::deselect`]. A bulk removal is one undo step.
    pub fn clear_all(&mut self) {
        self.clear_all_with_locked(false);
    }

    /// Bulk removal with explicit permission to include locked drawings. Preserves locks in
    /// undo and queues deletion of stored records exactly as [`Self::clear_all`] does.
    /// Drawings whose user edits are off or whose scope is filtered out always stay.
    pub fn clear_all_with_locked(&mut self, include_locked: bool) {
        let all_locked = self.locked;
        let filter = self.shared_filter;
        let remove = |d: &Drawing| {
            d.user_edit_enabled
                && (include_locked || (!d.locked && !all_locked))
                && !filter.hides(d)
        };
        self.history.push_snapshot(
            self.drawings
                .iter()
                .enumerate()
                .filter(|(_, d)| remove(d))
                .map(|(index, drawing)| (index, drawing.clone()))
                .collect(),
        );
        let mut removed = Vec::new();
        for drawing in self.drawings.iter().filter(|d| remove(d)) {
            if drawing.saveable {
                self.notify(DrawingSyncAction::Delete(sync_id(drawing)));
            }
            removed.push(drawing.id);
        }
        for id in removed {
            self.report(id, DrawingEventKind::Removed);
        }
        self.drawings.retain(|d| !remove(d));
        self.cancel_curr_drawing();
        self.selection.deselect_all();
    }

    // === Layer ordering ===
    //
    // Drawings paint by ascending series layer (`None` last), then z-order, with list order
    // breaking ties. Hit testing walks that order from the top down.

    /// Bring a drawing to the front: over the series and studies of its pane and above the
    /// highest z-order.
    pub fn bring_to_front(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter().find(|drawing| drawing.id == id) {
            let old = drawing.clone();
            self.history.push_modify(id, old.clone());
            ZOrderService::bring_to_front(&mut self.drawings, id);
            self.drawings
                .iter_mut()
                .find(|d| d.id == id)
                .unwrap()
                .series_layer = None;
            self.queue_update(id);
            self.report_change(id, Some(&old), false);
        }
    }

    /// Send a drawing to the back: under the series and studies of its pane and below the
    /// lowest z-order.
    pub fn send_to_back(&mut self, id: usize) {
        if let Some(drawing) = self.drawings.iter().find(|drawing| drawing.id == id) {
            let old = drawing.clone();
            self.history.push_modify(id, old.clone());
            ZOrderService::send_to_back(&mut self.drawings, id);
            self.drawings
                .iter_mut()
                .find(|d| d.id == id)
                .unwrap()
                .series_layer = Some(0);
            self.queue_update(id);
            self.report_change(id, Some(&old), false);
        }
    }

    /// Apply a layer move as one undo step, saving each changed drawing.
    pub fn reorder_drawings(&mut self, id: usize, layers: &[(usize, i32)]) {
        let layers: Vec<_> = layers
            .iter()
            .filter_map(|&(id, z)| {
                self.drawings
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| (id, d.series_layer, z))
            })
            .collect();
        self.reorder_layers(id, &layers);
    }

    /// Apply a move among the pane's drawings, series and studies as one undo step.
    pub(crate) fn reorder_layers(&mut self, id: usize, layers: &[(usize, Option<usize>, i32)]) {
        let edited = self
            .drawings
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.modified_at);
        let mut previous = Vec::new();
        for &(id, series_layer, z_order) in layers {
            if let Some(drawing) = self.drawings.iter_mut().find(|d| d.id == id)
                && (drawing.series_layer, drawing.z_order) != (series_layer, z_order)
            {
                previous.push((id, drawing.series_layer, drawing.z_order));
                drawing.series_layer = series_layer;
                drawing.z_order = z_order;
                // Only the moved drawing is edited; tied layers around it are just renumbered.
                self.notify(DrawingSyncAction::UpdateDrawing(id));
                self.report(id, DrawingEventKind::PropertiesChanged);
            }
        }
        if !previous.is_empty() {
            self.stamp(id);
            self.history.push_reorder(id, previous, edited);
        }
    }

    /// Duplicate a drawing three bars to the right, recording its creation for undo and saving.
    /// The copy has user edits on. Adds nothing for a drawing of a disabled tool or a copy that
    /// saving would refuse.
    pub fn duplicate_drawing(&mut self, id: usize) {
        let Some(mut copy) = self.drawings.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        copy.stored_id = None;
        copy.created_at = None;
        copy.user_edit_enabled = true;
        copy.screen_anchor = copy.screen_anchor.map(|[x, y]| [x + 0.02, y]);
        let first = copy.first_moved_anchor();
        for point in copy.chart_points.iter_mut().skip(first) {
            point.bar_idx += 3.0;
        }
        if self.validate_created(&copy).is_ok() {
            self.push_synced(copy);
        }
    }

    /// Paste copies three bars to the right with fresh identities and user edits on, selecting
    /// them as one undoable addition.
    /// The caller maps clipboard anchors into this manager's chart coordinates first. Leaves out a
    /// copy of a disabled tool or an unregistered custom tool and a copy that saving would refuse;
    /// when it leaves out every copy, it selects nothing and records no undo step.
    pub fn paste_drawings(&mut self, drawings: &[Drawing]) {
        let mut ids = Vec::new();
        for drawing in in_z_order(drawings) {
            let mut copy = drawing.clone();
            copy.stored_id = None;
            copy.created_at = None;
            copy.user_edit_enabled = true;
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
            copy.series_layer = None;
            ids.push(copy.id);
            let id = copy.id;
            self.drawings.push(copy);
            self.queue(DrawingSyncAction::SaveDrawing(id));
            self.report(id, DrawingEventKind::Created);
        }
        if ids.is_empty() {
            return;
        }
        self.select_drawings(ids.iter().copied());
        self.history
            .push_command(crate::drawings::services::DrawingCommand::Snapshot {
                drawings: Vec::new(),
                ids,
                moved: false,
            });
    }

    /// Replaces every saveable drawing with `drawings`, restored from storage, as one undo step.
    /// Each keeps its stored identity (`stored_id`, `created_at`, `notes`) and layer and gets a
    /// new id; drawings that are never saved stay, and so do those in a top-level folder kept on
    /// one chart or in one layout, which storage does not hold, unless `drawings` holds one of
    /// them again. Queues a `Delete` for each drawing it removes and a save for each it adds.
    pub(crate) fn replace_saved_drawings(&mut self, drawings: Vec<Drawing>) {
        let replaced = |drawing: &Drawing| {
            drawing.saveable
                && (!drawing.confined()
                    || drawings
                        .iter()
                        .any(|other| super::read_again(drawing, other)))
        };
        let removed: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, drawing)| replaced(drawing))
            .map(|(index, drawing)| (index, drawing.clone()))
            .collect();
        // A confined drawing that stays keeps a stored id of its own beside the copy's.
        let mut taken: HashSet<String> = drawings.iter().map(sync_id).collect();
        for drawing in self
            .drawings
            .iter_mut()
            .filter(|d| d.saveable && d.confined())
        {
            if !removed.iter().any(|(_, gone)| gone.id == drawing.id)
                && taken.contains(&sync_id(drawing))
            {
                drawing.stored_id = Some(super::free_stored_id(drawing, &taken));
                taken.insert(sync_id(drawing));
            }
        }
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
            self.report(drawing.id, DrawingEventKind::Removed);
        }
        let gone: HashSet<usize> = removed.iter().map(|(_, drawing)| drawing.id).collect();
        self.drawings.retain(|drawing| !gone.contains(&drawing.id));
        for drawing in added {
            let id = drawing.id;
            self.drawings.push(drawing);
            self.queue(DrawingSyncAction::SaveDrawing(id));
            self.report(id, DrawingEventKind::Created);
        }
        self.selection
            .prune(&self.drawings.iter().map(|d| d.id).collect());
        self.history
            .push_command(crate::drawings::services::DrawingCommand::Snapshot {
                drawings: removed,
                ids,
                moved: false,
            });
    }

    /// Applies `edit` to the drawing with the given ID, records it for undo and queues it to
    /// be saved. Edit a drawing through this rather than [`Self::drawings`] so the change is
    /// saved. A locked drawing accepts only visibility, name and folder changes, or an edit that unlocks it.
    /// An edit that hides the drawing deselects it.
    ///
    /// Refuses an edit after which saving would refuse the drawing, as
    /// [`Self::add_synced_drawing`] refuses such a drawing: a cursor tool, fewer anchors than the
    /// tool draws with, a non-finite coordinate, an anchor the chart cannot map to a time or a
    /// setting out of range, such as a line width of 0 or a NaN font size. It also refuses a
    /// change of `id`, which the manager assigns, and a `stored_id` another saved drawing is
    /// stored under. A manager outside a chart, or on a chart without bars, refuses only anchors
    /// 1e30 bars or more from the first bar. A refused edit, like an unknown id, changes nothing,
    /// records no undo step and returns the reason. A drawing that saving refuses already, such
    /// as one changed directly through [`Self::drawings`], accepts edits to its anchors and
    /// settings even if they remain invalid.
    /// [`Drawing::user_edit_enabled`] does not limit it: hosts edit drawings the user cannot.
    pub fn edit_drawing(
        &mut self,
        id: usize,
        edit: impl FnOnce(&mut Drawing),
    ) -> Result<(), &'static str> {
        let index = self
            .drawings
            .iter()
            .position(|d| d.id == id)
            .ok_or("Unknown drawing")?;
        let drawing = &mut self.drawings[index];
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
                return Ok(());
            }
            drawing.visible = visible;
            drawing.timeframe_visibility = timeframe_visibility;
            drawing.label = label;
            drawing.folder = folder;
            drawing.folder_sync = folder_sync;
        }
        let checked =
            crate::drawings::validation::validate_edit(&old, drawing, self.mapped_bars.as_ref());
        if let Err(reason) = checked.and_then(|()| self.check_identity(index, &old)) {
            self.drawings[index] = old;
            return Err(reason);
        }
        let drawing = &self.drawings[index];
        if !drawing.selectable || (old.visible && !drawing.visible) {
            self.selection.deselect(id);
        }
        self.queue_edit(&old);
        self.history.push_modify(id, old);
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| p.id != id);
        Ok(())
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
            self.report(id, DrawingEventKind::PropertiesChanged);
        }
        !updated.is_empty()
    }

    /// Edits live drawing ids as one undo step and queues their saved changes.
    /// Locked drawings accept visibility, name and folder changes, or an edit that unlocks them.
    /// The drawings the edit hides are deselected. An edit after which saving would refuse one of
    /// the drawings, as [`Self::edit_drawing`] describes, changes none of them, records no undo
    /// step and returns the reason. Ids of no drawing are skipped.
    /// [`Drawing::user_edit_enabled`] does not limit it: hosts edit drawings the user cannot.
    pub fn edit_drawings(
        &mut self,
        ids: &[usize],
        mut edit: impl FnMut(&mut Drawing),
    ) -> Result<(), &'static str> {
        if ids.is_empty() {
            return Ok(());
        }
        let ids: HashSet<_> = ids.iter().copied().collect();
        let mut before = Vec::new();
        let mut refused = None;
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
            if let Err(reason) =
                crate::drawings::validation::validate_edit(&old, drawing, self.mapped_bars.as_ref())
            {
                *drawing = old;
                refused = Some(reason);
                break;
            }
            before.push((index, old));
        }
        let refused = refused.or_else(|| {
            before
                .iter()
                .find_map(|(index, old)| self.check_identity(*index, old).err())
        });
        if let Some(reason) = refused {
            for (index, old) in before {
                self.drawings[index] = old;
            }
            return Err(reason);
        }
        // Only once every drawing took the edit, so that a refused one keeps the selection.
        for (index, old) in &before {
            let drawing = &self.drawings[*index];
            if !drawing.selectable || (old.visible && !drawing.visible) {
                self.selection.deselect(old.id);
            }
        }
        for (_, old) in &before {
            self.queue_edit(old);
        }
        self.history.push_snapshot(before);
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !ids.contains(&p.id));
        Ok(())
    }

    /// Applies `edit` as one frame of a continuous edit, such as dragging a colour picker: the
    /// first frame records the drawing's state and nothing is queued until
    /// [`Self::end_drag_handle`], which makes the whole gesture one undo step and queues one
    /// update. A frame after which saving would refuse the drawing, as [`Self::edit_drawing`]
    /// describes, changes nothing and returns the reason, as do an unknown id and a locked
    /// drawing; the gesture keeps its earlier frames.
    pub fn drag_edit_drawing(
        &mut self,
        id: usize,
        edit: impl FnOnce(&mut Drawing),
    ) -> Result<(), &'static str> {
        if self.drag_old_state.as_ref().is_some_and(|d| d.id != id) {
            self.end_drag_handle();
        }
        let index = self
            .drawings
            .iter()
            .position(|d| d.id == id)
            .ok_or("Unknown drawing")?;
        let drawing = &mut self.drawings[index];
        if drawing.locked {
            return Err("Drawing is locked");
        }
        let old = drawing.clone();
        edit(drawing);
        let checked =
            crate::drawings::validation::validate_edit(&old, drawing, self.mapped_bars.as_ref());
        if let Err(reason) = checked.and_then(|()| self.check_identity(index, &old)) {
            self.drawings[index] = old;
            return Err(reason);
        }
        // Only an applied frame starts the gesture, so a refused first frame leaves nothing for
        // `end_drag_handle` to record.
        self.drag_old_state.get_or_insert(old);
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| p.id != id);
        Ok(())
    }

    /// [`Self::drag_edit_drawing`] for the drawings of `ids` together, skipping ids of no drawing
    /// and locked drawings: the gesture's first frame records each drawing's state, and
    /// [`Self::end_drag_handle`] makes the whole gesture one undo step and queues one update per
    /// drawing. A frame after which saving would refuse one of them, as [`Self::edit_drawing`]
    /// describes, changes none of them and returns the reason; the gesture keeps its earlier frames.
    /// An open gesture on drawings outside `ids` ends first.
    #[cfg(feature = "ui")]
    pub(crate) fn drag_edit_drawings(
        &mut self,
        ids: &[usize],
        mut edit: impl FnMut(&mut Drawing),
    ) -> Result<(), &'static str> {
        if self
            .drag_old_state
            .iter()
            .chain(&self.drag_group)
            .any(|d| !ids.contains(&d.id))
        {
            self.end_drag_handle();
        }
        let before: Vec<_> = self
            .drawings
            .iter()
            .enumerate()
            .filter(|(_, d)| ids.contains(&d.id) && !d.locked)
            .map(|(index, d)| (index, d.clone()))
            .collect();
        if before.is_empty() {
            return Ok(());
        }
        for (index, old) in &before {
            let drawing = &mut self.drawings[*index];
            edit(drawing);
            if let Err(reason) =
                crate::drawings::validation::validate_edit(old, drawing, self.mapped_bars.as_ref())
            {
                for (index, old) in &before {
                    self.drawings[*index] = old.clone();
                }
                return Err(reason);
            }
        }
        if let Some(reason) = before
            .iter()
            .find_map(|(index, old)| self.check_identity(*index, old).err())
        {
            for (index, old) in &before {
                self.drawings[*index] = old.clone();
            }
            return Err(reason);
        }
        for (_, old) in &before {
            if self.drag_old_state.is_none() {
                self.drag_old_state = Some(old.clone());
            } else if !self
                .drag_old_state
                .iter()
                .chain(&self.drag_group)
                .any(|d| d.id == old.id)
            {
                self.drag_group.push(old.clone());
            }
        }
        self.painted
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| !before.iter().any(|(_, d)| d.id == p.id));
        Ok(())
    }

    // === Coordinate updates ===

    /// Recomputes screen coordinates for all drawings (completed and in-progress)
    /// from their persistent chart coordinates using the current pan/zoom transforms.
    ///
    /// `bar_spacing` is the points per bar of `bar_to_x`, as in
    /// [`ChartView::bar_spacing`](crate::drawings::rendering::ChartView::bar_spacing); a Text or
    /// Anchored text without a [`Drawing::text_zoom_spacing`] adopts it.
    ///
    /// Must be called each frame before rendering.
    pub fn update_all_screen_coords<F, G>(&mut self, bar_to_x: F, price_to_y: G, bar_spacing: f32)
    where
        F: Fn(f32) -> f32 + Copy,
        G: Fn(f64) -> f32 + Copy,
    {
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.measurement)
        {
            drawing.update_screen_coords(bar_to_x, price_to_y);
            drawing.adopt_text_zoom_spacing(bar_spacing);
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

    /// Whether the drawing is visible on this interval and its group is shown.
    pub(crate) fn shows(&self, drawing: &Drawing) -> bool {
        !self.hidden
            && !self.shared_filter.hides(drawing)
            && drawing.is_visible_on_timeframe(&self.curr_timeframe)
    }

    pub(crate) fn owns_pane(&self, drawing: &Drawing) -> bool {
        self.pane_members.contains(&drawing.pane)
    }

    pub(crate) fn update_pane_screen_coords(
        &mut self,
        bar_to_x: impl Fn(f32) -> f32 + Copy,
        price_to_y: impl Fn(f64) -> f32 + Copy,
        bar_spacing: f32,
    ) {
        for drawing in self
            .drawings
            .iter_mut()
            .chain(&mut self.curr_drawing)
            .chain(&mut self.measurement)
        {
            if self.pane_members.contains(&drawing.pane) {
                drawing.update_screen_coords(bar_to_x, price_to_y);
                drawing.adopt_text_zoom_spacing(bar_spacing);
                drawing.update_pane_coords(self.pane_rect);
            } else if drawing.reaches_other_panes()
                && let Some(anchor) = drawing.chart_points.first()
            {
                // Its time maps to this pane too; its price belongs to its own pane's scale.
                let y = drawing
                    .points
                    .first()
                    .map_or(self.pane_rect.center().y, |p| p.y);
                drawing.points = vec![egui::Pos2::new(bar_to_x(anchor.bar_idx), y)];
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

    /// Hides or shows the drawings as a group without changing [`Drawing::visible`].
    /// Hidden drawings cannot be painted, hit, dragged, snapped to or selected, including
    /// drawings added or loaded while hidden. Placement previews and text being typed inline
    /// remain visible until completed. Hiding clears the drawing selection.
    ///
    /// This is not a drawing edit: it queues no save or sync and changes no edit timestamp.
    /// [`Chart`](crate::widget::Chart) records the flag in undo history and keeps it across
    /// drawing loads and symbol changes, but does not save it with a layout.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        if hidden {
            self.selection.deselect_all();
        }
    }

    /// Whether [`Self::set_hidden`] hides every drawing.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Hides symbol-wide drawings on this chart only. Off by default. This includes
    /// `AllLayouts` folders and saveable drawings with no folder scope when the chart has
    /// drawing storage. Chart-only and layout-only folders are unaffected.
    ///
    /// Filtered drawings are omitted from object lists and bulk removal, and cannot be
    /// painted, selected, dragged or snapped to. A newly placed text drawing keeps its
    /// initial editor until placement finishes. Storage still receives peer edits and deletions.
    /// Toggling neither edits nor saves drawings. Wrap a user toggle in
    /// [`Chart::user_action`](crate::widget::Chart::user_action) to undo the toggle itself.
    /// The setting survives drawing loads and symbol changes, but is not saved in a layout.
    pub fn set_shared_drawings_hidden(&mut self, hidden: bool) {
        if self.shared_filter.hidden != hidden {
            self.shared_filter.hidden = hidden;
            if hidden {
                self.selection.cancel_area();
                for d in &self.drawings {
                    if self.shared_filter.hides(d) {
                        self.selection.deselect(d.id);
                    }
                }
            }
        }
    }

    /// Whether [`Self::set_shared_drawings_hidden`] filters symbol-wide drawings.
    pub fn shared_drawings_hidden(&self) -> bool {
        self.shared_filter.hidden
    }

    // === Group lock ===

    /// Locks or unlocks every drawing as a group without changing [`Drawing::locked`]. While
    /// locked, the user cannot move, reshape, restyle or delete any drawing, including drawings
    /// added or loaded while locked; [`Self::accepts_user_edits`] tells which drawings the user
    /// can edit. Drawings can still be placed, selected, hidden and reordered, and a text
    /// drawing just placed still takes its first text. [`Self::clear_all_with_locked`] with
    /// `true` still removes unfiltered drawings whose user edits are on. Methods that take drawing
    /// ids, such as [`Self::edit_drawing`], [`Self::delete_drawings`] and
    /// [`Self::set_selected_locked`], follow each drawing's own lock only.
    ///
    /// This is not a drawing edit: it queues no save or sync and changes no edit timestamp.
    /// [`Chart`](crate::widget::Chart) records the flag in undo history and keeps it across
    /// drawing loads and symbol changes, but does not save it with a layout.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// Whether [`Self::set_locked`] locks every drawing.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Whether the user can edit `drawing` now: it
    /// [accepts user edits](Drawing::accepts_user_edits) and [`Self::set_locked`] does not
    /// lock every drawing.
    pub fn accepts_user_edits(&self, drawing: &Drawing) -> bool {
        !self.locked && drawing.accepts_user_edits()
    }

    /// Adds a completed drawing with validated chart coordinates and properties as one undo
    /// step. Assigns a fresh id, retains the supplied layer, and queues a save if `saveable`.
    /// Use `Chart::create_drawing` when the anchors are UTC times rather than bar indices.
    pub fn add_drawing(&mut self, mut drawing: Drawing) -> Result<usize, &'static str> {
        self.validate_drawing(&drawing)?;
        if !self
            .registry
            .creates(drawing.custom_tool.as_deref(), drawing.tool_type)
        {
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
        self.report(id, DrawingEventKind::Created);
        Ok(id)
    }

    pub(crate) fn validate_drawing(&self, drawing: &Drawing) -> Result<(), &'static str> {
        crate::drawings::validation::validate(drawing, self.mapped_bars.as_ref())?;
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
        crate::drawings::validation::validate_stored(drawing, self.mapped_bars.as_ref())?;
        self.validate_custom_tool(drawing)?;
        if !self
            .registry
            .creates(drawing.custom_tool.as_deref(), drawing.tool_type)
        {
            return Err("Drawing tool is disabled");
        }
        Ok(())
    }

    /// Keeps the manager's handle and refuses a newly claimed stored identity already in use.
    pub(super) fn check_identity(&self, index: usize, old: &Drawing) -> Result<(), &'static str> {
        let drawing = &self.drawings[index];
        if drawing.id != old.id {
            return Err("Drawing ids cannot change");
        }
        // With the handle unchanged, only these edits can introduce a stored id collision.
        let moved = drawing.stored_id != old.stored_id || (drawing.saveable && !old.saveable);
        if moved && drawing.saveable {
            let stored = sync_id(drawing);
            if self
                .drawings
                .iter()
                .enumerate()
                .any(|(at, other)| at != index && other.saveable && sync_id(other) == stored)
            {
                return Err("Another drawing is stored under this id");
            }
        }
        Ok(())
    }

    pub(crate) fn unfiltered_drawings(&self) -> impl Iterator<Item = &Drawing> {
        self.drawings
            .iter()
            .filter(|d| !self.shared_filter.hides(d))
    }

    /// Drawings included in host object lists, excluding filtered scopes, in drawing-list order.
    pub fn listed_drawings(&self) -> impl Iterator<Item = &Drawing> {
        self.unfiltered_drawings().filter(|d| d.listed)
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
            self.report_change(old.id, Some(old), false);
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
        })
        .is_ok()
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
        })
        .is_ok()
    }

    /// Add a drawing from another source (for multi-chart sync, or restored from storage)
    /// under a new id, and return the id. A drawing restored from storage keeps its stored
    /// identity (`stored_id`, `created_at`, `notes`) and its z-order. A drawing that was never
    /// stored goes on top, and so does a copy, one stored under the id a drawing already here
    /// is saved under, which is saved as a new drawing.
    ///
    /// Refuses, and adds nothing, a drawing that saving would refuse: a cursor tool, fewer
    /// anchors than the tool draws with, a non-finite coordinate, an anchor the chart cannot map
    /// to a time or a setting out of range. Tool availability does not apply: drawings of a
    /// disabled tool are restored and synced.
    pub fn add_synced_drawing(&mut self, drawing: Drawing) -> Result<usize, &'static str> {
        crate::drawings::validation::validate_stored(&drawing, self.mapped_bars.as_ref())?;
        Ok(self.push_synced(drawing))
    }

    /// Adds a checked drawing under a new id, as [`Self::add_synced_drawing`] describes.
    fn push_synced(&mut self, mut drawing: Drawing) -> usize {
        if let Some(stored) = &drawing.stored_id
            && self.drawings.iter().any(|d| {
                d.stored_id.as_ref() == Some(stored)
                    || (d.saveable && d.stored_id.is_none() && d.id.to_string() == *stored)
            })
        {
            (drawing.stored_id, drawing.created_at, drawing.modified_at) = (None, None, None);
        }
        if drawing.stored_id.is_none() {
            drawing.z_order = self.top_z_order();
        }
        // Assign a new ID to avoid conflicts
        drawing.id = self.take_id();
        let id = drawing.id;
        // A linked copy keeps the time of the edit it copies.
        let stamp =
            drawing.stored_id.is_none() && drawing.saveable && drawing.modified_at.is_none();
        if drawing.stored_id.is_none() {
            if drawing.saveable {
                self.notify(DrawingSyncAction::SaveDrawing(id));
            }
            self.history.push_add(drawing.clone());
        }
        let created = drawing.stored_id.is_none();
        self.drawings.push(drawing);
        if created {
            self.report(id, DrawingEventKind::Created);
        }
        if stamp {
            self.stamp(id);
        }
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

    /// Group hiding keeps own visibility, also for newly added drawings (#3152).
    #[test]
    fn hiding_all_drawings_keeps_each_drawings_own_visibility() {
        use crate::drawings::rendering::ChartView;
        use egui::{Context, Pos2, RawInput};
        let ctx = Context::default();
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let line = |price: f64| {
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![ChartPoint::new(20.0, price)];
            drawing
        };
        let painted = |manager: &mut DrawingManager| {
            // Bar b maps to x = 10b, price p to y = 500 - p.
            manager.update_pane_screen_coords(|bar| bar * 10.0, |price| 500.0 - price as f32, 10.0);
            ctx.run_ui(RawInput::default(), |ui| {
                let view = ChartView {
                    price_formatter: None,
                    thousands_sep: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b * 10.0,
                    bar_spacing: 10.0,
                    price_to_y: &|p| 500.0 - p as f32,
                    y_to_price: &|y| 500.0 - f64::from(y),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                };
                manager.render_all(ui.painter(), pane, &view);
            })
            .drop_without_applying_deltas();
            let mut ids: Vec<_> = manager
                .painted
                .lock()
                .unwrap()
                .iter()
                .map(|p| p.id)
                .collect();
            ids.sort_unstable();
            ids
        };
        let own = |manager: &DrawingManager| -> Vec<bool> {
            manager.drawings.iter().map(|d| d.visible).collect()
        };
        let mut manager = DrawingManager::new();
        let first = manager.add_drawing(line(400.0)).unwrap();
        let second = manager.add_drawing(line(300.0)).unwrap();
        manager.edit_drawing(first, |d| d.visible = false).unwrap();
        assert_eq!(painted(&mut manager), [second]);
        let saved = manager.drawings.clone();
        manager.set_hidden(!manager.is_hidden());
        assert_eq!(own(&manager), [false, true]);
        assert!(painted(&mut manager).is_empty());
        assert_eq!(manager.hit_test(Pos2::new(200.0, 200.0)), None);
        for (before, after) in saved.iter().zip(&manager.drawings) {
            assert_eq!(before.modified_at, after.modified_at);
        }
        let third = manager.add_drawing(line(200.0)).unwrap();
        manager.select(third);
        manager.toggle_selection(second);
        assert!(painted(&mut manager).is_empty());
        assert_eq!(manager.selected_ids().count(), 0);
        manager.set_hidden(!manager.is_hidden());
        assert_eq!(own(&manager), [false, true, true]);
        assert_eq!(painted(&mut manager), [second, third]);
        assert_eq!(manager.hit_test(Pos2::new(200.0, 200.0)), Some(second));
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

    /// An edit after which saving would refuse a drawing it accepted, including an anchor the
    /// chart cannot date or a changed identity, changes no drawing, records no undo step, queues
    /// nothing and returns the reason through every edit path. The chart still saves both drawings.
    #[test]
    fn edits_that_saving_refuses_change_nothing() {
        type Setting = fn(&mut Drawing);
        type Edit = fn(&mut DrawingManager, Setting) -> Result<(), &'static str>;
        // The labelled drawing gets the bad value. The other one, which the bulk paths edit
        // first, gets a valid value that a refused bulk edit must undo too.
        let settings: [(&str, Setting, &str); 5] = [
            (
                "a zero line width",
                |d| {
                    d.stroke_width = if d.label.is_some() { 0.0 } else { 3.0 };
                },
                "Drawing settings must be finite and in range",
            ),
            (
                "a NaN font size",
                |d| {
                    d.font_size = if d.label.is_some() { f32::NAN } else { 20.0 };
                },
                "Drawing settings must be finite and in range",
            ),
            (
                "an anchor off the chart",
                |d| {
                    if d.label.is_some() {
                        d.chart_points[0].bar_idx = 1e9;
                    } else {
                        d.stroke_width = 3.0;
                    }
                },
                "Drawing anchors must map to a time",
            ),
            (
                "a new id",
                |d| {
                    if d.label.is_some() {
                        d.id += 10;
                    } else {
                        d.stroke_width = 3.0;
                    }
                },
                "Drawing ids cannot change",
            ),
            (
                "another drawing's stored id",
                |d| {
                    if d.label.is_some() {
                        // The first drawing has no stored id, so it is saved under its live id.
                        d.stored_id = Some("0".into());
                    } else {
                        d.stroke_width = 3.0;
                    }
                },
                "Another drawing is stored under this id",
            ),
        ];
        let paths: Vec<(&str, Edit)> = vec![
            #[cfg(feature = "ui")]
            ("drag_edit_drawings", |m, setting| {
                let ids: Vec<_> = m.drawings.iter().map(|d| d.id).collect();
                let result = m.drag_edit_drawings(&ids, setting);
                m.end_drag_handle();
                result
            }),
            ("edit_drawing", |m, setting| {
                let id = m.drawings[1].id;
                m.edit_drawing(id, setting)
            }),
            ("edit_drawings", |m, setting| {
                let ids: Vec<_> = m.drawings.iter().map(|d| d.id).collect();
                m.edit_drawings(&ids, setting)
            }),
            ("edit_selected", |m, setting| m.edit_selected(setting)),
            ("drag_edit_drawing", |m, setting| {
                let id = m.drawings[1].id;
                let result = m.drag_edit_drawing(id, setting);
                m.end_drag_handle();
                result
            }),
        ];
        for (path, edit) in paths {
            for (name, setting, reason) in settings {
                let case = format!("{path}, {name}");
                let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
                let manager = chart.drawings_mut();
                for label in [None, Some("B")] {
                    let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
                    drawing.chart_points =
                        vec![ChartPoint::new(1.0, 105.0), ChartPoint::new(2.0, 106.0)];
                    drawing.completed = true;
                    drawing.label = label.map(Into::into);
                    manager.add_drawing(drawing).unwrap();
                }
                let ids: Vec<_> = manager.drawings.iter().map(|d| d.id).collect();
                assert_eq!(ids, [0, 1], "{case}");
                manager.select_drawings(ids);
                manager.history.clear();
                manager.pending_sync.lock().unwrap().clear();
                let before = format!("{:?}", manager.drawings);
                let result = edit(manager, setting);
                assert_eq!(result, Err(reason), "{case}");
                assert_eq!(format!("{:?}", manager.drawings), before, "{case}");
                assert!(!manager.can_undo(), "{case}");
                assert!(manager.pending_sync.lock().unwrap().is_empty(), "{case}");
                assert_eq!(chart.save_drawings().unwrap().drawings.len(), 2, "{case}");
            }
        }
    }

    /// Several colour frames form one undo step, with one update each and locked drawings left.
    #[cfg(feature = "ui")]
    #[test]
    fn a_colour_drag_over_several_drawings_is_one_undo_step() {
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let manager = chart.drawings_mut();
        for _ in 0..3 {
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 105.0), ChartPoint::new(2.0, 106.0)];
            drawing.completed = true;
            manager.add_drawing(drawing).unwrap();
        }
        manager.drawings[2].locked = true;
        let original: Vec<_> = manager.drawings.iter().map(|d| d.color).collect();
        manager.history.clear();
        manager.pending_sync.lock().unwrap().clear();
        for red in 0..3 {
            manager
                .drag_edit_drawings(&[0, 1, 2], |d| d.color = [red, 0, 0, 255])
                .unwrap();
        }
        manager.end_drag_handle();
        assert_eq!(
            manager.drawings.iter().map(|d| d.color).collect::<Vec<_>>(),
            [[2, 0, 0, 255], [2, 0, 0, 255], original[2]]
        );
        assert_eq!(
            manager
                .pending_sync
                .lock()
                .unwrap()
                .iter()
                .map(|action| format!("{action:?}"))
                .collect::<Vec<_>>(),
            ["UpdateDrawing(0)", "UpdateDrawing(1)"]
        );
        assert!(manager.undo());
        assert_eq!(
            manager.drawings.iter().map(|d| d.color).collect::<Vec<_>>(),
            original
        );
        assert!(!manager.can_undo());
    }

    /// A restored id already used by a drawing that has not been stored makes a new drawing,
    /// so that saving does not find that id twice.
    #[test]
    fn a_synced_drawing_under_a_live_id_is_saved_as_new() {
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let manager = chart.drawings_mut();
        let mut line = Drawing::new(0, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(1.0, 105.0), ChartPoint::new(2.0, 106.0)];
        line.completed = true;
        let live = manager.add_drawing(line.clone()).unwrap();
        line.stored_id = Some(live.to_string());
        let added = manager.add_synced_drawing(line).unwrap();
        let copy = manager.drawings.iter().find(|d| d.id == added).unwrap();
        assert_eq!(copy.stored_id, None);
        assert!(chart.save_drawings().is_ok());
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

    /// Redo restores an added drawing as it was when the add was undone, with the bar spacing
    /// its text was first displayed at, which the drawing adopts after its undo step is recorded.
    #[test]
    fn redo_keeps_the_spacing_added_text_was_first_shown_at() {
        let mut manager = DrawingManager::new();
        let mut text = Drawing::new(0, DrawingToolType::TextLabel);
        text.text = Some("Note".into());
        text.text_follows_zoom = true;
        text.chart_points = vec![ChartPoint::new(10.0, 100.0)];
        let id = manager.add_drawing(text).unwrap();
        // First shown at 6 points a bar, then zoomed in to 12.
        manager.update_all_screen_coords(|bar: f32| bar * 6.0, |price: f64| price as f32, 6.0);
        let at_12 = |bar: f32| bar * 12.0;
        manager.update_all_screen_coords(at_12, |price: f64| price as f32, 12.0);
        let zoom = |manager: &DrawingManager| {
            let text = manager.drawings.iter().find(|d| d.id == id).unwrap();
            text.text_zoom(12.0)
        };
        // Sized at 6 points a bar and shown at 12, the text is drawn at 12 / 6 = 2 times its size.
        assert_eq!(zoom(&manager), 2.0, "before the undo");
        assert!(manager.undo() && manager.redo());
        manager.update_all_screen_coords(at_12, |price: f64| price as f32, 12.0);
        assert_eq!(zoom(&manager), 2.0, "after the redo");
    }
}
