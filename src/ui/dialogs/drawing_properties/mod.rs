//! Drawing properties dialog - modal popup for editing drawing properties.
//!
//! Provides a properties dialog for drawings with tabs:
//! - Style: Color, line width, line style, fill options
//! - Coordinates: Price and time points
//! - Visibility: Timeframe visibility settings
//! - Text: Labels and font settings

mod actions;
mod config;
mod dialog;
mod tabs;

pub use actions::{DrawingId, DrawingLineStyle, DrawingPropertiesAction, DrawingProps};
pub use config::DrawingPropertiesConfig;
pub use dialog::DrawingPropertiesDialog;
pub use tabs::PropertiesTab;
