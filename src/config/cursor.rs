//! Cursor identities shared by drawing controls and saved preferences.

#[cfg(feature = "ui")]
use crate::icons::{Icon, icons};

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

#[cfg(feature = "ui")]
impl CursorType {
    /// Values in display order.
    pub fn all() -> &'static [CursorType] {
        &[
            CursorType::Cross,
            CursorType::Dot,
            CursorType::Arrow,
            CursorType::Demonstration,
            CursorType::Magic,
            CursorType::Eraser,
        ]
    }

    /// Display name for this value.
    pub fn name(&self) -> &'static str {
        match self {
            CursorType::Cross => "Cross",
            CursorType::Dot => "Dot",
            CursorType::Arrow => "Arrow",
            CursorType::Demonstration => "Demonstration",
            CursorType::Magic => "Magic",
            CursorType::Eraser => "Eraser",
        }
    }

    /// Icon representing this value.
    pub fn icon(&self) -> &'static Icon {
        match self {
            CursorType::Cross => &icons::CROSSHAIR,
            CursorType::Dot => &icons::DOT,
            CursorType::Arrow => &icons::ARROW,
            CursorType::Demonstration => &icons::DEMONSTRATION,
            CursorType::Magic => &icons::MAGIC_CURSOR,
            CursorType::Eraser => &icons::ERASER,
        }
    }

    /// Keyboard shortcut label, when one is assigned.
    pub fn shortcut(&self) -> Option<&'static str> {
        match self {
            CursorType::Cross => None,
            CursorType::Dot => None,
            CursorType::Arrow => None,
            CursorType::Demonstration => None,
            CursorType::Magic => None,
            CursorType::Eraser => None,
        }
    }
}
