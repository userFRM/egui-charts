//! Cursor Modes Module
//!
//! Cursor modes for chart interaction.
//! Each cursor type provides different visual feedback and behavior.
//!
//! ## Cursor Types
//! - **Cross**: Full crosshair with vertical + horizontal lines (default)
//! - **Dot**: Simple dot at cursor position
//! - **Arrow**: Standard pointer, no crosshair overlay
//! - **Eraser**: Click to delete drawings

pub mod eraser;

pub use eraser::EraserMode;

/// Aggregate state for all cursor modes.
///
/// Holds the state for each cursor mode variant (eraser).
/// The chart widget owns a single `CursorModeState` and delegates input events
/// and rendering to the individual mode states.
#[derive(Clone, Debug, Default)]
pub struct CursorModeState {
    /// Eraser mode: click on drawings to delete them.
    pub eraser: EraserMode,
}

impl CursorModeState {
    /// Create a new state with all cursor modes inactive.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance all cursor mode animations by `dt` seconds (call once per frame).
    pub fn update(&mut self, _dt: f32) {
        // No animated modes remaining
    }

    /// Render all active cursor mode visual effects.
    pub fn render(&self, _ui: &mut egui::Ui, _rect: egui::Rect) {
        // No visual effects remaining
    }

    /// Reset all cursor mode states.
    pub fn clear_all(&mut self) {
        self.eraser.clear();
    }
}
