//! Chart settings template management.
//!
//! Provides `SettingsTemplate` and `TemplateManager` for saving and
//! restoring named chart configuration presets (indicator sets, chart type,
//! color schemes, etc.).  The actual implementations live in
//! [`crate::ui::stubs`]; this module re-exports them for convenience.

pub use crate::ui::stubs::{SettingsTemplate, TemplateManager};
