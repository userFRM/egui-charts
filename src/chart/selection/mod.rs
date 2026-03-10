//! Generic selection state for chart elements.
//!
//! Provides a shared selection pattern used by series, indicators, and other
//! selectable chart elements.

use std::marker::PhantomData;

/// Trait bound for IDs that can be used with [`SelectionState`].
///
/// Automatically implemented for any type that is `Copy + Eq + Debug`.
pub trait SelectableId: Copy + Eq + std::fmt::Debug {}

// Blanket implementation for types that meet the requirements
impl<T: Copy + Eq + std::fmt::Debug> SelectableId for T {}

/// Generic selection state for chart elements, parameterized by an ID type.
///
/// Tracks which element is selected, which is hovered, and the bar index
/// where the selection occurred. Used by both series and indicator selection.
#[derive(Clone, Debug)]
pub struct SelectionState<Id: SelectableId> {
    /// Currently selected element
    pub selected: Option<Id>,
    /// Currently hovered element (desktop only)
    pub hovered: Option<Id>,
    /// Additional selection metadata (bar index, etc.)
    pub bar_idx: Option<usize>,
    _phantom: PhantomData<Id>,
}

impl<Id: SelectableId> Default for SelectionState<Id> {
    fn default() -> Self {
        Self {
            selected: None,
            hovered: None,
            bar_idx: None,
            _phantom: PhantomData,
        }
    }
}

impl<Id: SelectableId> SelectionState<Id> {
    /// Create new empty selection state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Select an element.
    pub fn select(&mut self, id: Id, bar_idx: Option<usize>) {
        self.selected = Some(id);
        self.bar_idx = bar_idx;
    }

    /// Deselect all elements.
    pub fn deselect(&mut self) {
        self.selected = None;
        self.bar_idx = None;
    }

    /// Set hovered element.
    pub fn set_hovered(&mut self, id: Option<Id>) {
        self.hovered = id;
    }

    /// Check if a specific element is selected.
    pub fn is_selected(&self, id: Id) -> bool {
        self.selected == Some(id)
    }

    /// Check if a specific element is hovered.
    pub fn is_hovered(&self, id: Id) -> bool {
        self.hovered == Some(id)
    }

    /// Check if any element is selected.
    pub fn has_selection(&self) -> bool {
        self.selected.is_some()
    }

    /// Get the currently selected element ID.
    pub fn selected_id(&self) -> Option<Id> {
        self.selected
    }

    /// Get the bar index where selection occurred.
    pub fn selected_bar(&self) -> Option<usize> {
        self.bar_idx
    }

    /// Check if selection state is empty (nothing selected or hovered).
    pub fn is_empty(&self) -> bool {
        self.selected.is_none() && self.hovered.is_none()
    }

    /// Clear all state (selected, hovered, bar_idx).
    pub fn clear(&mut self) {
        self.selected = None;
        self.hovered = None;
        self.bar_idx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestId(usize);

    #[test]
    fn test_select_deselect() {
        let mut state = SelectionState::<TestId>::new();
        assert!(!state.has_selection());

        state.select(TestId(1), Some(42));
        assert!(state.has_selection());
        assert!(state.is_selected(TestId(1)));
        assert!(!state.is_selected(TestId(2)));
        assert_eq!(state.selected_bar(), Some(42));

        state.deselect();
        assert!(!state.has_selection());
        assert_eq!(state.selected_bar(), None);
    }

    #[test]
    fn test_hover() {
        let mut state = SelectionState::<TestId>::new();
        assert!(!state.is_hovered(TestId(1)));

        state.set_hovered(Some(TestId(1)));
        assert!(state.is_hovered(TestId(1)));
        assert!(!state.is_hovered(TestId(2)));

        state.set_hovered(None);
        assert!(!state.is_hovered(TestId(1)));
    }
}
