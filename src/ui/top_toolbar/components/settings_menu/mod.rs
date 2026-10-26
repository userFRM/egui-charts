//! Quick settings popup and tabbed chart settings dialog.
//!
//! Use [`SettingsMenu`] with [`ChartSettings`] for a compact menu, or
//! [`SettingsDialog`] with `widget::settings::ChartSettingsState` for the full editor.
//! Apply returned [`SettingsAction`] values in the host.

// Module declarations
pub mod actions;
pub mod config;
pub mod dialog;
pub mod menu;
pub mod types;

// Public re-exports
pub use actions::SettingsAction;
pub use config::{ChartSettings, SettingsDialogConfig};

pub use dialog::SettingsDialog;
pub use menu::SettingsMenu;
pub use types::SettingsTab;
// ChartType is re-exported from chart_type_selector - use that as single source of truth
