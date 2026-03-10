//! Chart Settings Menu and Dialog
//!
//! This module provides two settings interfaces:
//! 1. `SettingsMenu` - A quick popup menu (bottom-right button)
//! 2. `SettingsDialog` - A modal dialog with tabs
//!
//! Both share the same underlying settings state.
//!
//! ## Module Organization
//! - `types`: Core enums (ScaleMode, ChartType, SettingsTab, PrecisionMode)
//! - `data`: Settings state structures
//! - `config`: Configuration structures
//! - `actions`: Action enum
//! - `menu`: Quick popup menu implementation
//! - `dialog`: Full dialog implementation

// Module declarations
pub mod actions;
pub mod config;
pub mod data;
pub mod dialog;
pub mod menu;
pub mod types;

// Public re-exports
pub use actions::SettingsAction;
pub use config::{ChartSettings, SettingsDialogConfig};
pub use data::{
    AlertsSettings, ButtonsSettings, CandleColorConfig, ChartBasicStylesSettings,
    ChartSettingsState, CrosshairSettings, EventsSettings, GridLinesSettings, LineStyle,
    MarginsSettings, ScalesAndLinesSettings, ScalesAppearanceSettings, StatusLineOptions,
    TradingSettings, WatermarkSettings,
};
pub use dialog::SettingsDialog;
pub use menu::SettingsMenu;
pub use types::{
    BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, ScaleMode, SettingsTab,
    WatermarkMode,
};
// ChartType is re-exported from chart_type_selector - use that as single source of truth
