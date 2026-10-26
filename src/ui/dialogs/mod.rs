//! Modal dialogs.
//!
//! Provides modal dialog components:
//! - Alert creation dialog
//! - Drawing properties dialog
//! - Drawing context menu
//! - Series settings dialog
//! - Series context menu
//! - Study context menu

pub mod alert_dialog;
pub mod drawing_context_menu;
pub mod drawing_properties;
pub mod keyboard_shortcuts;
pub mod series_context_menu;
pub mod series_settings;
pub mod study_context_menu;
pub mod templates;

pub use alert_dialog::{AlertDialog, AlertDialogAction, AlertDialogConfig};
pub use drawing_context_menu::{DrawingContextMenu, DrawingContextMenuAction};
pub use drawing_properties::{
    DrawingId, DrawingPropertiesAction, DrawingPropertiesConfig, DrawingPropertiesDialog,
    DrawingProps, PropertiesTab,
};
pub use keyboard_shortcuts::{KeyboardShortcutsAction, KeyboardShortcutsDialog};
pub use series_context_menu::{SeriesContextMenu, SeriesContextMenuAction};
pub use series_settings::{
    SeriesSettingsAction, SeriesSettingsConfig, SeriesSettingsDialog, SeriesSettingsTab,
};
pub use study_context_menu::{IntervalRange, PaneTarget, StudyContextMenuAction};
