//! Modal dialogs.
//!
//! Provides modal dialog components:
//! - Alert creation dialog
//! - Chart settings dialog
//! - Indicator dialog
//! - Symbol search and compare dialogs
//! - Chart data table
//! - Drawing properties dialog
//! - Drawing context menu
//! - Series settings dialog
//! - Series context menu
//! - Study context menu
//! - Storage conflict and kept copies dialogs

pub mod alert_dialog;
pub mod chart_settings;
pub mod data_table;
pub mod drawing_properties;
pub mod indicator_dialog;
pub mod keyboard_shortcuts;
pub mod series_settings;
pub mod storage_conflict;
pub mod symbol_search;
pub mod templates;

pub use crate::ui::context_menu::{
    drawing as drawing_context_menu, series as series_context_menu, study as study_context_menu,
};

pub use crate::ui::context_menu::drawing::{DrawingContextMenu, DrawingContextMenuAction};
pub use crate::ui::context_menu::series::{SeriesContextMenu, SeriesContextMenuAction};
pub use crate::ui::context_menu::study::{IntervalRange, PaneTarget, StudyContextMenuAction};
pub use alert_dialog::{AlertDialog, AlertDialogAction, AlertDialogConfig};
pub use data_table::{DataTableAction, DataTableDialog};
pub use drawing_properties::{
    DrawingId, DrawingPropertiesAction, DrawingPropertiesConfig, DrawingPropertiesDialog,
    DrawingProps, PropertiesTab,
};
pub use keyboard_shortcuts::{KeyboardShortcutsAction, KeyboardShortcutsDialog};
pub use series_settings::{
    SeriesSettingsAction, SeriesSettingsConfig, SeriesSettingsDialog, SeriesSettingsTab,
};
pub use storage_conflict::{LocalCopiesDialog, StorageConflictDialog};
