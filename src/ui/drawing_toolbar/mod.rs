//! Categorized drawing tool picker and drawing utility controls.
//!
//! Place [`DrawingToolbar`] beside a chart and apply its returned
//! [`DrawingToolbarAction`] values to the chart or host state.

// Core module declarations
pub mod actions;
pub mod config;
pub mod data;
mod glyph_picker;
pub mod icons;
pub mod shortcuts;
pub mod state;
pub mod submenu_builder;
mod toolbar;

// Modular components and utilities
pub mod categories;
pub mod components;
pub mod utilities;

/// Drawing template metadata displayed by toolbar controls.
///
/// The host supplies and applies templates; this record does not store drawing settings.
#[derive(Debug, Clone)]
pub struct DrawingTemplate {
    /// Host identifier for this record.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Drawing tool identifier used by the host for this template.
    pub drawing_type: String,
    /// Whether the host marks this as the default template.
    pub is_default: bool,
}

// Re-export public types for convenient access
pub use actions::DrawingToolbarAction;
pub use components::{PairButton, draw_icon_btn, draw_separator, draw_toggle_btn};
pub use config::DrawingToolbarConfig;
pub use glyph_picker::GlyphPicker;
pub use shortcuts::DrawingToolShortcuts;
pub use state::{MagnetType, ToolbarState};
pub use submenu_builder::{SubmenuBuilder, SubmenuItem, ToggleConfig};
pub use toolbar::{DrawingToolbar, DrawingToolbarDomainState};

// Re-export category trait for extensibility
pub use categories::ToolCategory;
