//! Drawing toolbar dimensions and retained configuration values.
//!
//! Use [`DrawingToolbarConfig`] to set toolbar width and padding; colors come from
//! the active egui visuals and theme.

// =============================================================================
// Fixed Desktop Constants
// =============================================================================

/// Default drawing toolbar width in egui logical points.
pub const TOOLBAR_WIDTH: f32 = 52.0;

/// Default icon side length in egui logical points.
pub const ICON_SIZE: f32 = 18.0;

/// Default padding in egui logical points.
pub const PADDING: f32 = 4.0;

/// Default submenu width in egui logical points.
pub const SUBMENU_WIDTH: f32 = 200.0;

// =============================================================================
// Config Struct
// =============================================================================

/// Toolbar width and padding in egui logical points, with retained compatibility values.
#[derive(Debug, Clone)]
pub struct DrawingToolbarConfig {
    /// Retained for source compatibility; ignored when rendering.
    pub icon_size: f32,
    /// Toolbar width.
    pub width: f32,
    /// Padding before the first toolbar control.
    pub padding: f32,
    /// Retained for source compatibility; ignored when rendering.
    pub submenu_width: f32,
    /// Retained for source compatibility; tool names are shown in flyouts.
    pub show_names: bool,
    /// Retained for source compatibility; rendering uses `width`.
    pub expanded_width: f32,
    /// Retained for source compatibility; rendering uses `width`.
    pub collapsed_width: f32,
    // Legacy fields kept for compatibility
    /// Retained for source compatibility; ignored when rendering.
    pub base_icon_size: f32,
    /// Retained for source compatibility; rendering uses `width`.
    pub base_width: f32,
    /// Retained for source compatibility; rendering uses `padding`.
    pub base_padding: f32,
    /// Retained for source compatibility; ignored when rendering.
    pub base_submenu_width: f32,
}

impl Default for DrawingToolbarConfig {
    fn default() -> Self {
        Self {
            // Fixed desktop values
            icon_size: ICON_SIZE,
            width: TOOLBAR_WIDTH,
            padding: PADDING,
            submenu_width: SUBMENU_WIDTH,
            show_names: false,
            expanded_width: TOOLBAR_WIDTH,
            collapsed_width: TOOLBAR_WIDTH,
            // Legacy fields (same as primary values)
            base_icon_size: ICON_SIZE,
            base_width: TOOLBAR_WIDTH,
            base_padding: PADDING,
            base_submenu_width: SUBMENU_WIDTH,
        }
    }
}

impl DrawingToolbarConfig {
    /// Create default dimensions; the theme argument is unused.
    pub fn from_theme(_theme: &crate::theme::Theme) -> Self {
        Self::default()
    }

    /// Retained for source compatibility; leaves dimensions unchanged.
    pub fn update_for_window_width(&mut self, _window_width: f32) {
        // no scaling needed
    }
}
