//! Drawing properties dialog - modal popup for editing drawing properties.
//!
//! Provides a properties dialog for drawings with tabs:
//! - Style: Color, line width, line style, fill options
//! - Inputs: Saved tool-specific levels, sizing and calculation settings
//! - Coordinates: Price and time points
//! - Visibility: Timeframe visibility settings
//! - Text: Labels and font settings
//!
//! Open with [`DrawingPropertiesDialog::open_for_chart`] and call
//! [`DrawingPropertiesDialog::show_and_apply`] each frame to apply validated edits in the library.
//! Hosts handle Delete and Clone intents and persist [`crate::drawings::DrawingManager::tool_defaults`].

mod actions;
mod config;
mod dialog;
mod tabs;

pub use actions::{DrawingId, DrawingLineStyle, DrawingPropertiesAction, DrawingProps};
pub use config::DrawingPropertiesConfig;
pub use dialog::DrawingPropertiesDialog;
pub use tabs::{CoordinatePoint, CoordinatesState, PropertiesTab};
