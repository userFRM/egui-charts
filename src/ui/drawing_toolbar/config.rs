//! Configuration for drawing toolbar appearance and sizing.
//!
//! All colors are sourced from the egui theme system (`ui.style().visuals`).
//! Fixed desktop dimensions - no responsive behavior.

// =============================================================================
// Fixed Desktop Constants
// =============================================================================

/// Drawing toolbar width: 52px
pub const TOOLBAR_WIDTH: f32 = 52.0;

/// Icon size: 18px
pub const ICON_SIZE: f32 = 18.0;

/// Padding: 4px
pub const PADDING: f32 = 4.0;

/// Submenu width: 200px
pub const SUBMENU_WIDTH: f32 = 200.0;

// =============================================================================
// Config Struct
// =============================================================================

/// Configuration for the drawing toolbar - fixed desktop dimensions
#[derive(Debug, Clone)]
pub struct DrawingToolbarConfig {
    /// Current icon size: 18px
    pub icon_size: f32,
    /// Current sidebar width: 52px
    pub width: f32,
    /// Current padding: 4px
    pub padding: f32,
    /// Current submenu width: 200px
    pub submenu_width: f32,
    /// Whether to show tool names when expanded
    pub show_names: bool,
    /// Expanded width: 52px
    pub expanded_width: f32,
    /// Collapsed width: 52px
    pub collapsed_width: f32,
    // Legacy fields kept for compatibility
    pub base_icon_size: f32,
    pub base_width: f32,
    pub base_padding: f32,
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
    /// Create config from theme - colors come from ui.style().visuals
    pub fn from_theme(_theme: &crate::theme::Theme) -> Self {
        Self::default()
    }

    /// Legacy method - no-op with fixed desktop dimensions
    pub fn update_for_window_width(&mut self, _window_width: f32) {
        // no scaling needed
    }
}
