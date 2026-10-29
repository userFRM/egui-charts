//! History Service - undo/redo for drawing operations
//!
//! Implements command pattern for reversible drawing operations.
//!
//! # Commands
//!
//! - `Add` - A new drawing was created
//! - `Delete` - A drawing was deleted
//! - `Modify` - A drawing was modified (move, resize, style change)
//! - `Reorder` - Several drawing layers changed in one move
//!
//! # Usage
//!
//! ```
//! use egui_charts::drawings::services::HistoryService;
//! use egui_charts::drawings::{Drawing, DrawingToolType};
//!
//! let mut drawings = Vec::new();
//! let mut history = HistoryService::new();
//!
//! // After creating a drawing
//! let drawing = Drawing::new(1, DrawingToolType::TrendLine);
//! drawings.push(drawing.clone());
//! history.push_add(drawing);
//!
//! // Undo removes it again, redo restores it
//! assert_eq!(history.undo(&mut drawings), Some(1));
//! assert!(drawings.is_empty());
//! assert_eq!(history.redo(&mut drawings), Some(1));
//! assert_eq!(drawings.len(), 1);
//!
//! // Before modifying a drawing, record its old state
//! history.push_modify(1, drawings[0].clone());
//!
//! // After deleting a drawing, record the list index it was taken from
//! let removed = drawings.remove(0);
//! history.push_delete(removed, 0);
//! assert!(history.can_undo());
//! ```

use crate::drawings::{ChartPoint, Drawing};
use std::collections::HashSet;

/// Command representing a reversible drawing operation
#[derive(Debug, Clone)]
pub enum DrawingCommand {
    /// Drawings owned by one bulk edit, including their list order.
    Snapshot {
        /// Drawings at this side of the bulk edit, in list order, each with its list index.
        drawings: Vec<(usize, Drawing)>,
        /// Drawings owned by this edit; later host loads remain independent of it.
        ids: Vec<usize>,
    },
    /// A drawing was added
    Add(Drawing),
    /// A drawing was deleted
    Delete {
        /// The deleted drawing
        drawing: Drawing,
        /// List index the drawing was taken from, where undo puts it back
        index: usize,
    },
    /// Several drawing layers changed as one operation.
    Reorder {
        /// Drawing the user moved.
        id: usize,
        /// Previous layers of every affected drawing.
        layers: Vec<(usize, i32)>,
    },
    /// A drawing was modified
    Modify {
        /// Drawing ID
        id: usize,
        /// State before modification
        old_state: Drawing,
    },
}

impl DrawingCommand {
    /// Get the drawing ID affected by this command
    pub fn drawing_id(&self) -> usize {
        match self {
            DrawingCommand::Snapshot { ids, .. } => ids.first().copied().unwrap_or(0),
            DrawingCommand::Add(d) | DrawingCommand::Delete { drawing: d, .. } => d.id,
            DrawingCommand::Modify { id, .. } | DrawingCommand::Reorder { id, .. } => *id,
        }
    }

    /// Shift the bar indices at or after `from` for bars inserted in front of them
    pub fn shift_bar_indices(&mut self, from: f32, delta: f32) {
        match self {
            DrawingCommand::Snapshot { drawings, .. } => {
                for (_, drawing) in drawings {
                    drawing.shift_bar_indices(from, delta);
                }
            }
            DrawingCommand::Reorder { .. } => {}
            DrawingCommand::Add(d) | DrawingCommand::Delete { drawing: d, .. } => {
                d.shift_bar_indices(from, delta);
            }
            DrawingCommand::Modify { old_state, .. } => {
                old_state.shift_bar_indices(from, delta);
            }
        }
    }
}

/// History service for undo/redo operations
#[derive(Debug, Clone, Default)]
pub struct HistoryService {
    /// Commands that can be undone
    undo_stack: Vec<DrawingCommand>,
    /// Commands that can be redone
    redo_stack: Vec<DrawingCommand>,
    /// Maximum stack size (0 = unlimited)
    max_stack_size: usize,
    affected_ids: Vec<usize>,
}

impl HistoryService {
    pub(crate) fn visit_chart_points(&mut self, visit: &mut impl FnMut(&mut ChartPoint)) {
        for command in self.undo_stack.iter_mut().chain(&mut self.redo_stack) {
            match command {
                DrawingCommand::Snapshot { drawings, .. } => {
                    for (_, drawing) in drawings {
                        drawing.chart_points.iter_mut().for_each(&mut *visit);
                    }
                }
                DrawingCommand::Add(drawing)
                | DrawingCommand::Delete { drawing, .. }
                | DrawingCommand::Modify {
                    old_state: drawing, ..
                } => {
                    drawing.chart_points.iter_mut().for_each(&mut *visit);
                }
                DrawingCommand::Reorder { .. } => {}
            }
        }
    }

    /// Create a new history service with default settings
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            affected_ids: Vec::new(),
            max_stack_size: 100, // Reasonable default
        }
    }

    /// Create with custom max stack size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(max_size.min(100)),
            redo_stack: Vec::with_capacity(max_size.min(100)),
            affected_ids: Vec::new(),
            max_stack_size: max_size,
        }
    }

    /// Record the drawings a bulk edit changes or removes, each with its list index before the
    /// edit, as one undo step. Undo restores only these drawings.
    pub fn push_snapshot(&mut self, drawings: Vec<(usize, Drawing)>) {
        if !drawings.is_empty() {
            let ids = drawings.iter().map(|(_, drawing)| drawing.id).collect();
            self.push_command(DrawingCommand::Snapshot { drawings, ids });
        }
    }

    /// Ids touched by the most recent undo or redo, for host synchronization.
    pub(crate) fn affected_ids(&self) -> &[usize] {
        &self.affected_ids
    }

    fn track_affected(&mut self, command: &DrawingCommand) {
        self.affected_ids = match command {
            DrawingCommand::Snapshot { ids, .. } => ids.clone(),
            DrawingCommand::Reorder { layers, .. } => layers.iter().map(|(id, _)| *id).collect(),
            _ => vec![command.drawing_id()],
        };
        self.affected_ids.sort_unstable();
        self.affected_ids.dedup();
    }

    /// Record that a drawing was added
    pub fn push_add(&mut self, drawing: Drawing) {
        self.push_command(DrawingCommand::Add(drawing));
    }

    /// Record that a drawing was deleted from list index `index`
    pub fn push_delete(&mut self, drawing: Drawing, index: usize) {
        self.push_command(DrawingCommand::Delete { drawing, index });
    }

    /// Record that a drawing was modified
    pub fn push_modify(&mut self, id: usize, old_state: Drawing) {
        self.push_command(DrawingCommand::Modify { id, old_state });
    }

    /// Record a layer move, including the neighbours whose layers changed.
    pub(crate) fn push_reorder(&mut self, id: usize, layers: Vec<(usize, i32)>) {
        self.push_command(DrawingCommand::Reorder { id, layers });
    }

    /// Push a command to the undo stack
    pub(crate) fn push_command(&mut self, command: DrawingCommand) {
        // Clear redo stack on new action
        self.redo_stack.clear();

        // Enforce max size
        if self.max_stack_size > 0 && self.undo_stack.len() >= self.max_stack_size {
            self.undo_stack.remove(0);
        }

        self.undo_stack.push(command);
    }

    /// Undo the last operation
    ///
    /// Returns the ID of the affected drawing, if any.
    pub fn undo(&mut self, drawings: &mut Vec<Drawing>) -> Option<usize> {
        let command = self.undo_stack.pop()?;
        let affected_id = command.drawing_id();
        self.track_affected(&command);

        match command {
            DrawingCommand::Snapshot {
                drawings: before,
                ids,
            } => {
                let current = restore_snapshot(drawings, before, &ids);
                self.redo_stack.push(DrawingCommand::Snapshot {
                    drawings: current,
                    ids,
                });
            }
            DrawingCommand::Reorder { id, mut layers } => {
                for (id, layer) in &mut layers {
                    if let Some(drawing) = drawings.iter_mut().find(|d| d.id == *id) {
                        std::mem::swap(&mut drawing.z_order, layer);
                    }
                }
                self.redo_stack.push(DrawingCommand::Reorder { id, layers });
            }
            DrawingCommand::Add(drawing) => {
                // Undo add = remove the drawing
                drawings.retain(|d| d.id != drawing.id);
                self.redo_stack.push(DrawingCommand::Add(drawing));
            }
            DrawingCommand::Delete { drawing, index } => {
                // Undo delete = restore the drawing at its list index
                drawings.insert(index.min(drawings.len()), drawing.clone());
                self.redo_stack
                    .push(DrawingCommand::Delete { drawing, index });
            }
            DrawingCommand::Modify { id, old_state } => {
                // Undo modify = restore old state
                if let Some(current) = drawings.iter_mut().find(|d| d.id == id) {
                    let current_state = current.clone();
                    *current = old_state;
                    self.redo_stack.push(DrawingCommand::Modify {
                        id,
                        old_state: current_state,
                    });
                }
            }
        }

        Some(affected_id)
    }

    /// Redo the last undone operation
    ///
    /// Returns the ID of the affected drawing, if any.
    pub fn redo(&mut self, drawings: &mut Vec<Drawing>) -> Option<usize> {
        let command = self.redo_stack.pop()?;
        let affected_id = command.drawing_id();
        self.track_affected(&command);

        match command {
            DrawingCommand::Snapshot {
                drawings: before,
                ids,
            } => {
                let current = restore_snapshot(drawings, before, &ids);
                self.undo_stack.push(DrawingCommand::Snapshot {
                    drawings: current,
                    ids,
                });
            }
            DrawingCommand::Reorder { id, mut layers } => {
                for (id, layer) in &mut layers {
                    if let Some(drawing) = drawings.iter_mut().find(|d| d.id == *id) {
                        std::mem::swap(&mut drawing.z_order, layer);
                    }
                }
                self.undo_stack.push(DrawingCommand::Reorder { id, layers });
            }
            DrawingCommand::Add(drawing) => {
                // Redo add = add the drawing back
                drawings.push(drawing.clone());
                self.undo_stack.push(DrawingCommand::Add(drawing));
            }
            DrawingCommand::Delete { drawing, mut index } => {
                // Redo delete = remove the drawing, recording where it was
                if let Some(at) = drawings.iter().position(|d| d.id == drawing.id) {
                    drawings.remove(at);
                    index = at;
                }
                self.undo_stack
                    .push(DrawingCommand::Delete { drawing, index });
            }
            DrawingCommand::Modify { id, old_state } => {
                // Redo modify = apply the modification again
                if let Some(current) = drawings.iter_mut().find(|d| d.id == id) {
                    let current_state = current.clone();
                    *current = old_state;
                    self.undo_stack.push(DrawingCommand::Modify {
                        id,
                        old_state: current_state,
                    });
                }
            }
        }

        Some(affected_id)
    }

    /// Check if undo is available
    #[inline]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    #[inline]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get the number of undo operations available
    #[inline]
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of redo operations available
    #[inline]
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.affected_ids.clear();
    }

    /// Shift the bar indices at or after `from` in history (for bars inserted in front of them)
    pub fn shift_bar_indices(&mut self, from: f32, delta: f32) {
        for cmd in &mut self.undo_stack {
            cmd.shift_bar_indices(from, delta);
        }
        for cmd in &mut self.redo_stack {
            cmd.shift_bar_indices(from, delta);
        }
    }

    /// Prune undo history entries for drawings that no longer exist.
    ///
    /// Only the undo stack is pruned against the live id set. The redo stack is
    /// intentionally left intact: a redo entry legitimately references a drawing
    /// that is absent from the live set. After an `Add` is undone, the drawing
    /// is removed from the live set but its re-adding command sits on the redo
    /// stack; pruning the redo stack against live ids would silently drop that
    /// entry and lose the redo. The redo stack is the authoritative record of
    /// what redo would restore, so it is the wrong place to enforce live-id
    /// membership.
    pub fn prune(&mut self, existing_ids: &HashSet<usize>) {
        self.undo_stack.retain(|cmd| {
            matches!(cmd, DrawingCommand::Snapshot { .. })
                || existing_ids.contains(&cmd.drawing_id())
        });
    }

    /// Get description of the last undoable action (for UI)
    pub fn last_undo_description(&self) -> Option<&'static str> {
        self.undo_stack.last().map(|cmd| match cmd {
            DrawingCommand::Snapshot { .. } => "Edit Drawings",
            DrawingCommand::Add(_) => "Add Drawing",
            DrawingCommand::Delete { .. } => "Delete Drawing",
            DrawingCommand::Modify { .. } => "Modify Drawing",
            DrawingCommand::Reorder { .. } => "Reorder Drawings",
        })
    }

    /// Get description of the last redoable action (for UI)
    pub fn last_redo_description(&self) -> Option<&'static str> {
        self.redo_stack.last().map(|cmd| match cmd {
            DrawingCommand::Snapshot { .. } => "Edit Drawings",
            DrawingCommand::Add(_) => "Add Drawing",
            DrawingCommand::Delete { .. } => "Delete Drawing",
            DrawingCommand::Modify { .. } => "Modify Drawing",
            DrawingCommand::Reorder { .. } => "Reorder Drawings",
        })
    }
}

// Keep independent drawings and their list positions outside the bulk command's history.
// Owned drawings still listed are restored in place; the others go back to their recorded
// list index, clamped, and the drawings taken out keep theirs for the inverse command.
fn restore_snapshot(
    live: &mut Vec<Drawing>,
    before: Vec<(usize, Drawing)>,
    ids: &[usize],
) -> Vec<(usize, Drawing)> {
    let mut before: Vec<_> = before.into_iter().map(Some).collect();
    let mut current = Vec::new();
    let mut restored = Vec::new();
    for (index, drawing) in live.drain(..).enumerate() {
        if !ids.contains(&drawing.id) {
            restored.push(drawing);
            continue;
        }
        let id = drawing.id;
        current.push((index, drawing));
        if let Some(old) = before
            .iter_mut()
            .find(|old| old.as_ref().is_some_and(|(_, old)| old.id == id))
            .and_then(Option::take)
        {
            restored.push(old.1);
        }
    }
    for (index, drawing) in before.into_iter().flatten() {
        restored.insert(index.min(restored.len()), drawing);
    }
    *live = restored;
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType};

    fn make_drawing(id: usize) -> Drawing {
        Drawing::new(id, DrawingToolType::TrendLine)
    }

    fn drawing(id: usize, stroke_width: f32) -> Drawing {
        let mut d = make_drawing(id);
        d.stroke_width = stroke_width;
        d
    }

    /// The live list as (id, stroke width, z-order), in list order, so a test sees which
    /// drawings exist, which state each one is in and which is on top among equal z-orders.
    fn state(drawings: &[Drawing]) -> Vec<(usize, f32, i32)> {
        drawings
            .iter()
            .map(|d| (d.id, d.stroke_width, d.z_order))
            .collect()
    }

    /// Undo of each command returns the live list to its state and order before
    /// the action, redo returns it to the state after, and the redone command can
    /// be undone again.
    #[test]
    fn undo_and_redo_restore_each_command() {
        type Record = fn(&mut HistoryService);
        let cases: [(&str, Vec<Drawing>, Vec<Drawing>, Record); 7] = [
            (
                "partial bulk edit",
                vec![drawing(2, 9.0), drawing(1, 2.0)],
                vec![drawing(2, 9.0), drawing(1, 8.0)],
                |h| h.push_snapshot(vec![(1, drawing(1, 2.0))]),
            ),
            (
                "bulk edit and order",
                vec![drawing(1, 2.0), drawing(2, 3.0)],
                vec![drawing(2, 8.0)],
                |h| h.push_snapshot(vec![(0, drawing(1, 2.0)), (1, drawing(2, 3.0))]),
            ),
            (
                "paste",
                vec![drawing(2, 1.0), drawing(3, 1.0)],
                vec![drawing(2, 1.0), drawing(3, 1.0), drawing(1, 1.0)],
                |h| {
                    h.push_command(DrawingCommand::Snapshot {
                        drawings: Vec::new(),
                        ids: vec![1],
                    })
                },
            ),
            (
                "reorder",
                vec![drawing(1, 1.0), drawing(2, 1.0)],
                vec![
                    Drawing {
                        z_order: 1,
                        ..drawing(1, 1.0)
                    },
                    Drawing {
                        z_order: 2,
                        ..drawing(2, 1.0)
                    },
                ],
                |h| h.push_reorder(1, vec![(1, 0), (2, 0)]),
            ),
            (
                "add",
                vec![drawing(2, 1.0)],
                vec![drawing(2, 1.0), drawing(1, 1.0)],
                |h| h.push_add(drawing(1, 1.0)),
            ),
            (
                "delete",
                vec![drawing(2, 1.0), drawing(1, 1.0), drawing(3, 1.0)],
                vec![drawing(2, 1.0), drawing(3, 1.0)],
                |h| h.push_delete(drawing(1, 1.0), 1),
            ),
            (
                "modify",
                vec![drawing(1, 2.0)],
                vec![drawing(1, 5.0)],
                |h| h.push_modify(1, drawing(1, 2.0)),
            ),
        ];

        for (name, mut before, mut after, record) in cases {
            let mut history = HistoryService::new();
            let mut drawings = after.clone();
            record(&mut history);
            if name == "bulk edit and order" {
                // A later host load must survive undo and redo of an earlier user command.
                let independent = drawing(3, 7.0);
                before.push(independent.clone());
                after.push(independent.clone());
                drawings.push(independent);
            }

            assert_eq!(history.undo(&mut drawings), Some(1), "{name}: undo id");
            assert_eq!(state(&drawings), state(&before), "{name}: after undo");
            assert_eq!(history.redo(&mut drawings), Some(1), "{name}: redo id");
            assert_eq!(state(&drawings), state(&after), "{name}: after redo");
            assert_eq!(history.undo(&mut drawings), Some(1), "{name}: undo again");
            assert_eq!(state(&drawings), state(&before), "{name}: after undo again");
        }
    }

    #[test]
    fn test_new_action_clears_redo() {
        let mut history = HistoryService::new();
        let mut drawings = Vec::new();

        // Add, undo, add new - should clear redo
        let d1 = make_drawing(1);
        drawings.push(d1.clone());
        history.push_add(d1);

        history.undo(&mut drawings);
        assert!(history.can_redo());

        let d2 = make_drawing(2);
        drawings.push(d2.clone());
        history.push_add(d2);

        assert!(!history.can_redo()); // Redo cleared
    }

    /// The cap drops the oldest command, so the newest ones stay undoable.
    #[test]
    fn test_max_stack_size() {
        let mut history = HistoryService::with_max_size(3);
        let mut drawings = Vec::new();

        for i in 1..=5 {
            let d = make_drawing(i);
            drawings.push(d.clone());
            history.push_add(d);
        }

        for expected in [5, 4, 3] {
            assert_eq!(history.undo(&mut drawings), Some(expected));
        }
        assert_eq!(history.undo(&mut drawings), None);
    }

    /// When history is prepended, commands on both stacks move with the bars,
    /// so a later undo or redo puts the drawing back at its shifted position.
    #[test]
    fn test_shift_bar_indices() {
        let anchored = |id, bar_idx| {
            let mut d = make_drawing(id);
            d.chart_points.push(ChartPoint::new(bar_idx, 50.0));
            d
        };
        let mut history = HistoryService::new();
        let mut drawings = vec![anchored(1, 120.0), anchored(2, 200.0)];

        history.push_modify(1, anchored(1, 100.0));
        history.push_add(anchored(2, 200.0));
        history.undo(&mut drawings); // Add(2) moves to the redo stack

        history.shift_bar_indices(f32::NEG_INFINITY, 50.0);

        // Modify(1) sat on the undo stack: undo restores its old state, shifted.
        assert_eq!(history.undo(&mut drawings), Some(1));
        assert_eq!(drawings[0].chart_points[0].bar_idx, 150.0);
        history.redo(&mut drawings); // reapplies Modify(1)
        // Add(2) was on the redo stack: redo re-adds it at the shifted bar.
        assert_eq!(history.redo(&mut drawings), Some(2));
        assert_eq!(drawings[1].chart_points[0].bar_idx, 250.0);
    }

    /// Undoing an `Add` parks the re-add command on the redo stack while the
    /// drawing is absent from the live set. A subsequent `prune` against the
    /// live ids must not drop that redo entry, or the redo would be silently
    /// lost. This guards the invariant that redo survives pruning.
    #[test]
    fn test_undo_add_then_prune_preserves_redo() {
        let mut history = HistoryService::new();
        let mut drawings = Vec::new();

        let drawing = make_drawing(1);
        drawings.push(drawing.clone());
        history.push_add(drawing);

        // Undo the add: drawing leaves the live set, redo becomes available.
        history.undo(&mut drawings);
        assert!(drawings.is_empty());
        assert!(history.can_redo());

        // Prune against the (now empty) live id set, as a delete-driven cleanup
        // would. The undone-add redo entry must survive.
        history.prune(&HashSet::new());
        assert!(
            history.can_redo(),
            "redo entry for an undone Add must survive prune"
        );

        // Redo must re-add the drawing.
        let affected = history.redo(&mut drawings);
        assert_eq!(affected, Some(1));
        assert_eq!(drawings.len(), 1);
        assert_eq!(drawings[0].id, 1);
    }
}
