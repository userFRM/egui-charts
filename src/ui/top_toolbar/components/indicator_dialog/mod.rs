//! Indicators Dialog
//!
//! A modal dialog for searching, browsing, configuring and adding indicators to the chart.
//! Features categories, favorites, search, tabs, and param configuration.
//!
//! ## Module Organization
//! - `types`: Enum definitions (IndicatorTab, IndicatorCategory)
//! - `data`: Data structures (IndicatorInfo, IndicatorParams)
//! - `config`: Configuration struct
//! - `actions`: Action enum
//! - `dialog`: Main IndicatorDialog implementation

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
