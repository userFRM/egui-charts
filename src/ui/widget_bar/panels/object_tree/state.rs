//! State management for the Object Tree panel
//!
//! Tracks selection, expansion, filtering, and other UI state.

use std::collections::HashSet;

use super::types::SourceType;

/// State for the Object Tree panel
#[derive(Clone, Debug, Default)]
pub struct ObjectTreeState {
    // === Selection ===
    /// Currently selected item IDs
    pub selected_ids: HashSet<usize>,
    /// Last selected ID (for shift-click range selection)
    pub last_selected_id: Option<usize>,
    /// Anchor ID for range selection
    pub selection_anchor: Option<usize>,

    // === Expansion ===
    /// Expanded group headers
    pub expanded_groups: HashSet<String>,
    /// Expanded item IDs (showing properties)
    pub expanded_items: HashSet<usize>,
    /// Data Window section expanded
    pub data_window_expanded: bool,

    // === Filtering ===
    /// Filter text (search query)
    pub filter_text: String,
    /// Filter by source type
    pub filter_type: Option<SourceType>,

    // === Context Menu ===
    /// Context menu is open
    pub context_menu_open: bool,
    /// Context menu position
    pub context_menu_pos: egui::Pos2,
    /// Context menu target item ID
    pub context_menu_target: Option<usize>,

    // === Drag & Drop ===
    /// Currently dragging item ID
    pub dragging_id: Option<usize>,
    /// Drag start position
    pub drag_start_pos: Option<egui::Pos2>,
    /// Drop target position (index)
    pub drop_target: Option<usize>,

    // === Rename ===
    /// Currently renaming item ID
    pub renaming_id: Option<usize>,
    /// Rename text buffer
    pub rename_text: String,

    // === Hover ===
    /// Currently hovered item ID
    pub hovered_id: Option<usize>,
}

impl ObjectTreeState {
    /// Create a new state with default expansions
    pub fn new() -> Self {
        let mut state = Self::default();
        // Expand all groups by default
        state.expanded_groups.insert("Indicators".to_string());
        state.expanded_groups.insert("Drawings".to_string());
        state.expanded_groups.insert("Templates".to_string());
        state.data_window_expanded = true;
        state
    }

    // === Selection Methods ===

    /// Select a single item (clears other selections)
    pub fn select(&mut self, id: usize) {
        self.selected_ids.clear();
        self.selected_ids.insert(id);
        self.last_selected_id = Some(id);
        self.selection_anchor = Some(id);
    }

    /// Add item to selection (toggle if already selected)
    pub fn toggle_selection(&mut self, id: usize) {
        if self.selected_ids.contains(&id) {
            self.selected_ids.remove(&id);
        } else {
            self.selected_ids.insert(id);
        }
        self.last_selected_id = Some(id);
    }

    /// Range select from anchor to id (given a list of ordered IDs)
    pub fn range_select(&mut self, id: usize, ordered_ids: &[usize]) {
        if let Some(anchor) = self.selection_anchor {
            let anchor_idx = ordered_ids.iter().position(|&x| x == anchor);
            let target_idx = ordered_ids.iter().position(|&x| x == id);

            if let (Some(start), Some(end)) = (anchor_idx, target_idx) {
                let (min, max) = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };

                self.selected_ids.clear();
                for &item_id in &ordered_ids[min..=max] {
                    self.selected_ids.insert(item_id);
                }
            }
        } else {
            self.select(id);
        }
        self.last_selected_id = Some(id);
    }

    /// Select all items
    pub fn select_all(&mut self, all_ids: &[usize]) {
        self.selected_ids = all_ids.iter().copied().collect();
    }

    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
        self.last_selected_id = None;
    }

    /// Check if item is selected
    pub fn is_selected(&self, id: usize) -> bool {
        self.selected_ids.contains(&id)
    }

    /// Get number of selected items
    pub fn selection_count(&self) -> usize {
        self.selected_ids.len()
    }

    // === Expansion Methods ===

    /// Toggle group expansion
    pub fn toggle_group(&mut self, group: &str) {
        if self.expanded_groups.contains(group) {
            self.expanded_groups.remove(group);
        } else {
            self.expanded_groups.insert(group.to_string());
        }
    }

    /// Check if group is expanded
    pub fn is_group_expanded(&self, group: &str) -> bool {
        self.expanded_groups.contains(group)
    }

    /// Toggle item expansion (properties)
    pub fn toggle_item(&mut self, id: usize) {
        if self.expanded_items.contains(&id) {
            self.expanded_items.remove(&id);
        } else {
            self.expanded_items.insert(id);
        }
    }

    /// Check if item is expanded
    pub fn is_item_expanded(&self, id: usize) -> bool {
        self.expanded_items.contains(&id)
    }

    /// Expand all groups
    pub fn expand_all_groups(&mut self) {
        self.expanded_groups.insert("Indicators".to_string());
        self.expanded_groups.insert("Drawings".to_string());
        self.expanded_groups.insert("Templates".to_string());
    }

    /// Collapse all groups
    pub fn collapse_all_groups(&mut self) {
        self.expanded_groups.clear();
    }

    // === Filter Methods ===

    /// Set filter text
    pub fn set_filter(&mut self, text: impl Into<String>) {
        self.filter_text = text.into();
    }

    /// Clear filter
    pub fn clear_filter(&mut self) {
        self.filter_text.clear();
        self.filter_type = None;
    }

    /// Check if filter is active
    pub fn has_filter(&self) -> bool {
        !self.filter_text.is_empty() || self.filter_type.is_some()
    }

    // === Context Menu Methods ===

    /// Open context menu at position for optional target
    pub fn open_context_menu(&mut self, pos: egui::Pos2, target: Option<usize>) {
        self.context_menu_open = true;
        self.context_menu_pos = pos;
        self.context_menu_target = target;
    }

    /// Close context menu
    pub fn close_context_menu(&mut self) {
        self.context_menu_open = false;
        self.context_menu_target = None;
    }

    // === Rename Methods ===

    /// Start renaming an item
    pub fn start_rename(&mut self, id: usize, current_name: &str) {
        self.renaming_id = Some(id);
        self.rename_text = current_name.to_string();
    }

    /// Cancel rename
    pub fn cancel_rename(&mut self) {
        self.renaming_id = None;
        self.rename_text.clear();
    }

    /// Finish rename and return new name if changed
    pub fn finish_rename(&mut self) -> Option<(usize, String)> {
        if let Some(id) = self.renaming_id.take() {
            let name = std::mem::take(&mut self.rename_text);
            if !name.is_empty() {
                return Some((id, name));
            }
        }
        None
    }

    // === Drag & Drop Methods ===

    /// Start dragging an item
    pub fn start_drag(&mut self, id: usize, pos: egui::Pos2) {
        self.dragging_id = Some(id);
        self.drag_start_pos = Some(pos);
    }

    /// Update drop target
    pub fn set_drop_target(&mut self, target: Option<usize>) {
        self.drop_target = target;
    }

    /// End drag operation
    pub fn end_drag(&mut self) -> Option<(usize, usize)> {
        let result = match (self.dragging_id, self.drop_target) {
            (Some(src), Some(dst)) if src != dst => Some((src, dst)),
            _ => None,
        };
        self.dragging_id = None;
        self.drag_start_pos = None;
        self.drop_target = None;
        result
    }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.dragging_id.is_some()
    }

    /// Sync selected drawing from an external selection.
    ///
    /// If `selected_drawing` is `Some`, ensures it is the sole selection.
    pub fn sync_selected_drawing(&mut self, selected_drawing: Option<usize>) {
        if let Some(drawing_id) = selected_drawing {
            if !self.selected_ids.contains(&drawing_id) {
                self.selected_ids.clear();
                self.selected_ids.insert(drawing_id);
                self.last_selected_id = Some(drawing_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection() {
        let mut state = ObjectTreeState::new();

        state.select(1);
        assert!(state.is_selected(1));
        assert_eq!(state.selection_count(), 1);

        state.toggle_selection(2);
        assert!(state.is_selected(1));
        assert!(state.is_selected(2));
        assert_eq!(state.selection_count(), 2);

        state.toggle_selection(1);
        assert!(!state.is_selected(1));
        assert!(state.is_selected(2));
    }

    #[test]
    fn test_range_select() {
        let mut state = ObjectTreeState::new();
        let ids = vec![1, 2, 3, 4, 5];

        state.select(2);
        state.range_select(4, &ids);

        assert!(state.is_selected(2));
        assert!(state.is_selected(3));
        assert!(state.is_selected(4));
        assert!(!state.is_selected(1));
        assert!(!state.is_selected(5));
    }
}
