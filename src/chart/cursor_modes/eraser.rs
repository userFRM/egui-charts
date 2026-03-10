//! Eraser Mode - Delete Drawings on Click
//!
//! The Eraser cursor allows quick deletion of drawings.
//! Click on any drawing to remove it from the chart.
//!
//! ## Behavior
//! - Cursor changes to indicate eraser mode
//! - Hovering over a drawing highlights it (shows what will be deleted)
//! - Click to delete the highlighted drawing
//! - Exit eraser mode when selecting a drawing tool

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Visual configuration for the eraser cursor mode.
#[derive(Clone, Debug)]
pub struct EraserConfig {
    /// Color for hover highlight (shows what will be deleted)
    pub highlight_color: Color32,
    /// Stroke width for highlight outline
    pub highlight_stroke: f32,
}

impl Default for EraserConfig {
    fn default() -> Self {
        let bearish = DESIGN_TOKENS.semantic.extended.bearish;
        Self {
            highlight_color: Color32::from_rgba_unmultiplied(
                bearish.r(),
                bearish.g(),
                bearish.b(),
                180,
            ), // Bearish red with alpha
            highlight_stroke: 3.0,
        }
    }
}

/// Runtime state for the eraser cursor mode.
///
/// When active, hovering over a drawing highlights it with a colored outline.
/// Clicking deletes the hovered drawing. The eraser does not affect non-drawing
/// chart elements (series, indicators, overlays).
#[derive(Clone, Debug, Default)]
pub struct EraserMode {
    /// Whether eraser mode is active
    pub active: bool,
    /// Drawing ID currently hovered (for highlight preview)
    pub hover_drawing: Option<usize>,
    /// Configuration
    pub config: EraserConfig,
}

impl EraserMode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable/disable eraser mode
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        if !active {
            self.hover_drawing = None;
        }
    }

    /// Set the currently hovered drawing (for highlight)
    pub fn set_hover(&mut self, drawing_id: Option<usize>) {
        if self.active {
            self.hover_drawing = drawing_id;
        }
    }

    /// Check if a specific drawing should be highlighted
    pub fn should_highlight(&self, drawing_id: usize) -> bool {
        self.active && self.hover_drawing == Some(drawing_id)
    }

    /// Get the highlight color
    pub fn highlight_color(&self) -> Color32 {
        self.config.highlight_color
    }

    /// Get the highlight stroke width
    pub fn highlight_stroke(&self) -> f32 {
        self.config.highlight_stroke
    }

    /// Clear eraser state
    pub fn clear(&mut self) {
        self.hover_drawing = None;
    }
}
