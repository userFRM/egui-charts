//! Tabbed chart settings dialog.
//!
//! Use [`SettingsDialog`] with `widget::settings::ChartSettingsState` to edit a chart's
//! settings, and apply the returned [`SettingsAction`] values in the host.

// Module declarations
pub mod actions;
pub mod config;
pub mod types;

// Public re-exports
pub use actions::SettingsAction;
pub use config::SettingsDialogConfig;

pub use crate::ui::dialogs::chart_settings as dialog;
pub use crate::ui::dialogs::chart_settings::SettingsDialog;
pub use types::SettingsTab;
