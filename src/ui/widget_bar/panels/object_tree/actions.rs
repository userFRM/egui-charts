//! Actions emitted by the Object Tree panel
//!
//! These actions should be handled by the parent component to
//! affect the actual chart state.

use super::types::DrawingProperties;

/// Actions that can be performed on objects in the tree
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ObjectTreeAction {
    /// No action
    #[default]
    None,

    // === Selection ===
    /// Select a single object (clears other selections)
    Select(usize),
    /// Add to selection (Ctrl+click)
    AddToSelection(usize),
    /// Range select (Shift+click)
    RangeSelect(usize),
    /// Clear all selections
    ClearSelection,
    /// Select all objects
    SelectAll,

    // === Visibility ===
    /// Toggle visibility of an object
    ToggleVisibility(usize),
    /// Show all objects
    ShowAll,
    /// Hide all objects
    HideAll,
    /// Hide selected objects
    HideSelected,
    /// Show selected objects
    ShowSelected,

    // === Locking (drawings only) ===
    /// Toggle lock state of an object
    ToggleLock(usize),
    /// Lock all drawings
    LockAll,
    /// Unlock all drawings
    UnlockAll,
    /// Lock selected drawings
    LockSelected,
    /// Unlock selected drawings
    UnlockSelected,

    // === Object Manipulation ===
    /// Delete an object
    Delete(usize),
    /// Delete all selected objects
    DeleteSelected,
    /// Duplicate an object
    Duplicate(usize),
    /// Duplicate all selected objects
    DuplicateSelected,

    // === Properties ===
    /// Open properties dialog for object
    OpenProperties(usize),
    /// Rename an object (id, new_name)
    Rename(usize, String),
    /// Update drawing properties
    UpdateProperties(usize, DrawingProperties),
    /// Change object color
    ChangeColor(usize, egui::Color32),

    // === Z-ordering ===
    /// Bring object to front
    BringToFront(usize),
    /// Send object to back
    SendToBack(usize),
    /// Move object up one level
    MoveUp(usize),
    /// Move object down one level
    MoveDown(usize),
    /// Reorder via drag-drop (item_id, new_index)
    Reorder(usize, usize),
    /// Bring selected to front
    BringSelectedToFront,
    /// Send selected to back
    SendSelectedToBack,

    // === Navigation ===
    /// Zoom to object on chart (focus view)
    ZoomTo(usize),
    /// Pan chart to show object
    PanTo(usize),

    // === Indicators ===
    /// Open indicator settings dialog
    OpenIndicatorSettings(usize),
    /// Remove indicator
    RemoveIndicator(usize),

    // === Templates ===
    /// Save current drawings as template
    SaveAsTemplate(String),
    /// Load template
    LoadTemplate(String),
    /// Delete template
    DeleteTemplate(String),

    // === Bulk Operations ===
    /// Remove all drawings
    RemoveAllDrawings,
    /// Remove all indicators
    RemoveAllIndicators,

    // === Data Window ===
    /// Toggle Data Window expansion
    ToggleDataWindow,
}

impl ObjectTreeAction {
    /// Check if this is a no-op action
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Check if this action modifies selection
    pub fn is_selection_action(&self) -> bool {
        matches!(
            self,
            Self::Select(_)
                | Self::AddToSelection(_)
                | Self::RangeSelect(_)
                | Self::ClearSelection
                | Self::SelectAll
        )
    }

    /// Check if this action modifies objects
    pub fn is_modification_action(&self) -> bool {
        matches!(
            self,
            Self::Delete(_)
                | Self::DeleteSelected
                | Self::Duplicate(_)
                | Self::DuplicateSelected
                | Self::Rename(_, _)
                | Self::UpdateProperties(_, _)
                | Self::ChangeColor(_, _)
                | Self::BringToFront(_)
                | Self::SendToBack(_)
                | Self::MoveUp(_)
                | Self::MoveDown(_)
                | Self::Reorder(_, _)
                | Self::RemoveIndicator(_)
                | Self::RemoveAllDrawings
                | Self::RemoveAllIndicators
        )
    }

    /// Check if this action requires confirmation
    pub fn requires_confirmation(&self) -> bool {
        matches!(
            self,
            Self::DeleteSelected
                | Self::RemoveAllDrawings
                | Self::RemoveAllIndicators
                | Self::DeleteTemplate(_)
        )
    }

    /// Get a human-readable description of the action
    pub fn description(&self) -> String {
        match self {
            Self::None => "No action".to_string(),
            Self::Select(id) => format!("Select object {}", id),
            Self::Delete(id) => format!("Delete object {}", id),
            Self::DeleteSelected => "Delete selected objects".to_string(),
            Self::Duplicate(id) => format!("Duplicate object {}", id),
            Self::ToggleVisibility(id) => format!("Toggle visibility for {}", id),
            Self::ToggleLock(id) => format!("Toggle lock for {}", id),
            Self::Rename(id, name) => format!("Rename {} to '{}'", id, name),
            Self::BringToFront(id) => format!("Bring {} to front", id),
            Self::SendToBack(id) => format!("Send {} to back", id),
            Self::ZoomTo(id) => format!("Zoom to object {}", id),
            Self::RemoveAllDrawings => "Remove all drawings".to_string(),
            Self::RemoveAllIndicators => "Remove all indicators".to_string(),
            _ => format!("{:?}", self),
        }
    }
}
