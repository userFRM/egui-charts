//! Chart Settings Menu and Dialog
//!
//! This module provides two settings interfaces:
//! 1. `SettingsMenu` - A quick popup menu (bottom-right button)
//! 2. `SettingsDialog` - A modal dialog with tabs
//!
//! Both share the same underlying settings state.
//!
//! ## Module Organization
//! - `types`: Core enums (ChartType, SettingsTab, PrecisionMode)
//! - `data`: Settings state structures
//! - `config`: Configuration structures
//! - `actions`: Action enum
//! - `menu`: Quick popup menu implementation
//! - `dialog`: Full dialog implementation

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
