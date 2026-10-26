//! Study search, selection, configuration and template controls.
//!
//! Use [`IndicatorDialog`] with an `IndicatorFactory` to browse and add studies,
//! and [`IndicatorSettingsDialog`] to edit an existing study. The host applies the
//! returned actions and retains user preferences and templates.

// Module declarations
pub mod actions;
pub mod config;
pub mod data;
pub mod dialog;
mod settings;
pub mod templates;
pub use settings::{IndicatorSettingsDialog, StudyEdit};
pub mod types;

// Public re-exports
pub use actions::IndicatorDialogAction;
pub use config::IndicatorDialogConfig;
pub use data::{IndicatorInfo, IndicatorParams, IndicatorPreferences};
pub use dialog::IndicatorDialog;
pub use templates::{IndicatorTemplateAction, IndicatorTemplatePanel, IndicatorTemplateState};
pub use types::{IndicatorCategory, IndicatorTab};
