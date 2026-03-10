//! History Service - undo/redo for drawing operations
//!
//! Implements command pattern for reversible drawing operations.
//!
//! # Commands
//!
//! - `Add` - A new drawing was created
//! - `Delete` - A drawing was deleted
//! - `Modify` - A drawing was modified (move, resize, style change)
//!
//! # Usage
//!
//! ```ignore
//! let mut history = HistoryService::new();
//!
//! // After creating a drawing
//! history.push_add(drawing.clone());
//!
//! // After modifying a drawing
//! history.push_modify(id, old_state);
//!
//! // After deleting a drawing
//! history.push_delete(drawing);
//!
//! // Undo/redo
//! if history.can_undo() {
//!     history.undo(&mut drawings, &mut selection);
//! }
//! ```

use crate::drawings::Drawing;
use std::collections::HashSet;

/// Command representing a reversible drawing operation
#[derive(Debug, Clone)]
pub enum DrawingCommand {
    /// A drawing was added
    Add(Drawing),
    /// A drawing was deleted
    Delete(Drawing),
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
            DrawingCommand::Add(d) => d.id,
            DrawingCommand::Delete(d) => d.id,
            DrawingCommand::Modify { id, .. } => *id,
        }
    }

    /// Shift bar indices for historical data prepending
    pub fn shift_bar_indices(&mut self, delta: f32) {
        match self {
            DrawingCommand::Add(d) | DrawingCommand::Delete(d) => {
                d.shift_bar_indices(delta);
            }
            DrawingCommand::Modify { old_state, .. } => {
                old_state.shift_bar_indices(delta);
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
}

impl HistoryService {
    /// Create a new history service with default settings
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_stack_size: 100, // Reasonable default
        }
    }

    /// Create with custom max stack size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(max_size.min(100)),
            redo_stack: Vec::with_capacity(max_size.min(100)),
            max_stack_size: max_size,
        }
    }

    /// Record that a drawing was added
    pub fn push_add(&mut self, drawing: Drawing) {
        self.push_command(DrawingCommand::Add(drawing));
    }

    /// Record that a drawing was deleted
    pub fn push_delete(&mut self, drawing: Drawing) {
        self.push_command(DrawingCommand::Delete(drawing));
    }

    /// Record that a drawing was modified
    pub fn push_modify(&mut self, id: usize, old_state: Drawing) {
        self.push_command(DrawingCommand::Modify { id, old_state });
    }

    /// Push a command to the undo stack
    fn push_command(&mut self, command: DrawingCommand) {
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

        match command {
            DrawingCommand::Add(drawing) => {
                // Undo add = remove the drawing
                drawings.retain(|d| d.id != drawing.id);
                self.redo_stack.push(DrawingCommand::Add(drawing));
            }
            DrawingCommand::Delete(drawing) => {
                // Undo delete = restore the drawing
                drawings.push(drawing.clone());
                self.redo_stack.push(DrawingCommand::Delete(drawing));
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

        match command {
            DrawingCommand::Add(drawing) => {
                // Redo add = add the drawing back
                drawings.push(drawing.clone());
                self.undo_stack.push(DrawingCommand::Add(drawing));
            }
            DrawingCommand::Delete(drawing) => {
                // Redo delete = remove the drawing
                drawings.retain(|d| d.id != drawing.id);
                self.undo_stack.push(DrawingCommand::Delete(drawing));
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
    }

    /// Shift all bar indices in history (for historical data prepending)
    pub fn shift_bar_indices(&mut self, delta: f32) {
        for cmd in &mut self.undo_stack {
            cmd.shift_bar_indices(delta);
        }
        for cmd in &mut self.redo_stack {
            cmd.shift_bar_indices(delta);
        }
    }

    /// Prune history entries for deleted drawings
    pub fn prune(&mut self, existing_ids: &HashSet<usize>) {
        self.undo_stack
            .retain(|cmd| existing_ids.contains(&cmd.drawing_id()));
        self.redo_stack
            .retain(|cmd| existing_ids.contains(&cmd.drawing_id()));
    }

    /// Get description of the last undoable action (for UI)
    pub fn last_undo_description(&self) -> Option<&'static str> {
        self.undo_stack.last().map(|cmd| match cmd {
            DrawingCommand::Add(_) => "Add Drawing",
            DrawingCommand::Delete(_) => "Delete Drawing",
            DrawingCommand::Modify { .. } => "Modify Drawing",
        })
    }

    /// Get description of the last redoable action (for UI)
    pub fn last_redo_description(&self) -> Option<&'static str> {
        self.redo_stack.last().map(|cmd| match cmd {
            DrawingCommand::Add(_) => "Add Drawing",
            DrawingCommand::Delete(_) => "Delete Drawing",
            DrawingCommand::Modify { .. } => "Modify Drawing",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;

    fn make_drawing(id: usize) -> Drawing {
        Drawing::new(id, DrawingToolType::TrendLine)
    }

    #[test]
    fn test_undo_add() {
        let mut history = HistoryService::new();
        let mut drawings = Vec::new();

        // Add a drawing
        let drawing = make_drawing(1);
        drawings.push(drawing.clone());
        history.push_add(drawing);

        assert_eq!(drawings.len(), 1);
        assert!(history.can_undo());

        // Undo
        let affected = history.undo(&mut drawings);
        assert_eq!(affected, Some(1));
        assert_eq!(drawings.len(), 0);
        assert!(history.can_redo());
    }

    #[test]
    fn test_undo_delete() {
        let mut history = HistoryService::new();
        let drawing = make_drawing(1);
        let mut drawings = vec![drawing.clone()];

        // Delete the drawing
        drawings.clear();
        history.push_delete(drawing);

        // Undo should restore it
        history.undo(&mut drawings);
        assert_eq!(drawings.len(), 1);
        assert_eq!(drawings[0].id, 1);
    }

    #[test]
    fn test_undo_modify() {
        let mut history = HistoryService::new();
        let mut drawing = make_drawing(1);
        drawing.stroke_width = 2.0;
        let mut drawings = vec![drawing.clone()];

        // Modify
        let old_state = drawing.clone();
        drawings[0].stroke_width = 5.0;
        history.push_modify(1, old_state);

        // Undo should restore old stroke width
        history.undo(&mut drawings);
        assert_eq!(drawings[0].stroke_width, 2.0);
    }

    #[test]
    fn test_redo() {
        let mut history = HistoryService::new();
        let mut drawings = Vec::new();

        let drawing = make_drawing(1);
        drawings.push(drawing.clone());
        history.push_add(drawing);

        // Undo then redo
        history.undo(&mut drawings);
        assert_eq!(drawings.len(), 0);

        history.redo(&mut drawings);
        assert_eq!(drawings.len(), 1);
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

    #[test]
    fn test_max_stack_size() {
        let mut history = HistoryService::with_max_size(3);
        let mut drawings = Vec::new();

        for i in 1..=5 {
            let d = make_drawing(i);
            drawings.push(d.clone());
            history.push_add(d);
        }

        // Only 3 items should be in stack
        assert_eq!(history.undo_count(), 3);
    }

    #[test]
    fn test_shift_bar_indices() {
        let mut history = HistoryService::new();

        let mut drawing = make_drawing(1);
        drawing
            .chart_points
            .push(crate::drawings::ChartPoint::new(100.0, 50.0));
        history.push_add(drawing);

        history.shift_bar_indices(50.0);

        // The drawing in history should have shifted bar indices
        // (We can't easily verify this without exposing internals,
        // but the method should not panic)
    }

    #[test]
    fn test_descriptions() {
        let mut history = HistoryService::new();

        assert_eq!(history.last_undo_description(), None);

        history.push_add(make_drawing(1));
        assert_eq!(history.last_undo_description(), Some("Add Drawing"));

        history.push_delete(make_drawing(2));
        assert_eq!(history.last_undo_description(), Some("Delete Drawing"));
    }
}
