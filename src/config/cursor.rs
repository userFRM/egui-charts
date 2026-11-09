//! Cursor identities shared by drawing controls and saved preferences.

/// Cursor tool types (not drawing tools, but cursor modes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CursorType {
    /// Crosshair cursor.
    Cross,
    /// Dot cursor.
    Dot,
    /// Arrow cursor.
    Arrow,
    /// Pointer with a presentation halo.
    Demonstration,
    /// Trail that follows the pointer while the button is held, then fades.
    Magic,
    /// Click a drawing to delete it.
    Eraser,
}
