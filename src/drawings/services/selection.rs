//! Selection Service - manages drawing selection state
//!
//! Handles single and multi-select for chart drawings.
//!
//! # Features
//!
//! - Single click selects, deselects others
//! - Ctrl/Cmd+click toggles in multi-select mode
//! - Click on empty area deselects all
//! - Selected drawings show handles

use std::collections::HashSet;

/// Selection service for managing drawing selection state
#[derive(Debug, Clone, Default)]
pub struct SelectionService {
    /// Currently selected drawing IDs
    selected: HashSet<usize>,
    /// Primary selection (most recently clicked)
    primary: Option<usize>,
    /// Whether multi-select mode is active
    multi_select_mode: bool,
}

impl SelectionService {
    /// Create a new selection service
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a single drawing (deselects all others unless multi-select is active)
    ///
    /// Returns true if selection changed.
    pub fn select(&mut self, id: usize) -> bool {
        if self.multi_select_mode {
            // Add to selection
            let changed = self.selected.insert(id);
            self.primary = Some(id);
            changed
        } else {
            // Single select mode - replace selection
            let was_selected = self.selected.contains(&id) && self.selected.len() == 1;
            self.selected.clear();
            self.selected.insert(id);
            self.primary = Some(id);
            !was_selected
        }
    }

    /// Toggle selection of a drawing (for Ctrl+click behavior)
    ///
    /// Returns true if selection changed.
    pub fn toggle(&mut self, id: usize) -> bool {
        if self.selected.contains(&id) {
            self.selected.remove(&id);
            if self.primary == Some(id) {
                self.primary = self.selected.iter().copied().next();
            }
            true
        } else {
            self.selected.insert(id);
            self.primary = Some(id);
            true
        }
    }

    /// Deselect a specific drawing
    ///
    /// Returns true if the drawing was selected.
    pub fn deselect(&mut self, id: usize) -> bool {
        let removed = self.selected.remove(&id);
        if self.primary == Some(id) {
            self.primary = self.selected.iter().copied().next();
        }
        removed
    }

    /// Deselect all drawings
    ///
    /// Returns true if any drawings were selected.
    pub fn deselect_all(&mut self) -> bool {
        let had_selection = !self.selected.is_empty();
        self.selected.clear();
        self.primary = None;
        had_selection
    }

    /// Check if a specific drawing is selected
    #[inline]
    pub fn is_selected(&self, id: usize) -> bool {
        self.selected.contains(&id)
    }

    /// Check if any drawings are selected
    #[inline]
    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Get the primary (most recently selected) drawing ID
    #[inline]
    pub fn primary(&self) -> Option<usize> {
        self.primary
    }

    /// Get all selected drawing IDs
    #[inline]
    pub fn selected_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected.iter().copied()
    }

    /// Get count of selected drawings
    #[inline]
    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Enable/disable multi-select mode
    pub fn set_multi_select_mode(&mut self, enabled: bool) {
        self.multi_select_mode = enabled;
    }

    /// Check if multi-select mode is active
    #[inline]
    pub fn is_multi_select_mode(&self) -> bool {
        self.multi_select_mode
    }

    /// Select all drawings from a list of IDs
    pub fn select_all(&mut self, ids: impl IntoIterator<Item = usize>) {
        for id in ids {
            self.selected.insert(id);
            if self.primary.is_none() {
                self.primary = Some(id);
            }
        }
    }

    /// Remove selection for drawings that no longer exist
    ///
    /// Call this after deleting drawings to clean up stale selections.
    pub fn prune(&mut self, existing_ids: &HashSet<usize>) {
        self.selected.retain(|id| existing_ids.contains(id));
        if let Some(p) = self.primary
            && !existing_ids.contains(&p)
        {
            self.primary = self.selected.iter().copied().next();
        }
    }

    /// Get single selected drawing ID (legacy compatibility)
    ///
    /// Returns None if multiple drawings are selected.
    pub fn single_selection(&self) -> Option<usize> {
        if self.selected.len() == 1 {
            self.selected.iter().copied().next()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_select() {
        let mut sel = SelectionService::new();

        assert!(!sel.has_selection());
        assert!(sel.select(1));
        assert!(sel.is_selected(1));
        assert_eq!(sel.primary(), Some(1));

        // Select another - first should be deselected
        assert!(sel.select(2));
        assert!(!sel.is_selected(1));
        assert!(sel.is_selected(2));
        assert_eq!(sel.selection_count(), 1);
    }

    #[test]
    fn test_multi_select() {
        let mut sel = SelectionService::new();
        sel.set_multi_select_mode(true);

        sel.select(1);
        sel.select(2);
        sel.select(3);

        assert!(sel.is_selected(1));
        assert!(sel.is_selected(2));
        assert!(sel.is_selected(3));
        assert_eq!(sel.selection_count(), 3);
    }

    #[test]
    fn test_toggle() {
        let mut sel = SelectionService::new();

        sel.toggle(1);
        assert!(sel.is_selected(1));

        sel.toggle(1);
        assert!(!sel.is_selected(1));
    }

    #[test]
    fn test_deselect_all() {
        let mut sel = SelectionService::new();
        sel.set_multi_select_mode(true);
        sel.select(1);
        sel.select(2);

        assert!(sel.deselect_all());
        assert!(!sel.has_selection());
        assert_eq!(sel.primary(), None);
    }

    #[test]
    fn test_prune() {
        let mut sel = SelectionService::new();
        sel.set_multi_select_mode(true);
        sel.select(1);
        sel.select(2);
        sel.select(3);

        // Drawing 2 was deleted
        let existing: HashSet<usize> = [1, 3].into_iter().collect();
        sel.prune(&existing);

        assert!(sel.is_selected(1));
        assert!(!sel.is_selected(2));
        assert!(sel.is_selected(3));
        assert_eq!(sel.selection_count(), 2);
    }

    #[test]
    fn test_select_all() {
        let mut sel = SelectionService::new();
        sel.select_all([1, 2, 3]);

        assert_eq!(sel.selection_count(), 3);
        assert!(sel.primary().is_some());
    }

    #[test]
    fn test_single_selection_legacy() {
        let mut sel = SelectionService::new();

        // No selection
        assert_eq!(sel.single_selection(), None);

        // Single selection
        sel.select(5);
        assert_eq!(sel.single_selection(), Some(5));

        // Multiple selection
        sel.set_multi_select_mode(true);
        sel.select(6);
        assert_eq!(sel.single_selection(), None);
    }
}
