//! Floating Selection Toolbar
//!
//! A draggable toolbar that appears when selecting objects (series, indicators, drawings)
//! for quick customization (color, settings, delete, etc.)

mod toolbar;

pub use toolbar::{ColorSlot, FloatingSelectionToolbar, FloatingToolbarAction};
