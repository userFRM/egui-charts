//! Modal dialogs.
//!
//! Provides modal dialog components:
//! - Alert creation dialog
//! - Drawing properties dialog
//! - Drawing context menu
//! - Series settings dialog
//! - Series context menu

pub mod alert_dialog;
pub mod drawing_context_menu;
pub mod drawing_properties;
pub mod keyboard_shortcuts;
#[cfg(feature = "scripting")]
pub mod pine_editor;
pub mod series_context_menu;
pub mod series_settings;

pub use alert_dialog::{AlertData, AlertDialog, AlertDialogAction, AlertDialogConfig, AlertTab};
pub use drawing_context_menu::{DrawingContextMenu, DrawingContextMenuAction};
pub use drawing_properties::{
    DrawingId, DrawingLineStyle, DrawingPropertiesAction, DrawingPropertiesConfig,
    DrawingPropertiesDialog, DrawingProps, PropertiesTab,
};
pub use keyboard_shortcuts::{KeyboardShortcutsAction, KeyboardShortcutsDialog};
#[cfg(feature = "scripting")]
pub use pine_editor::{PineEditor, PineEditorAction};
pub use series_context_menu::{SeriesContextMenu, SeriesContextMenuAction};
pub use series_settings::{
    SeriesSettingsAction, SeriesSettingsConfig, SeriesSettingsDialog, SeriesSettingsTab,
};
