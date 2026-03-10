//! Configuration for top toolbar layout.
//!
//! Colors are NOT stored here - use theming.rs instead for theme-aware colors.
//! Fixed desktop dimensions - no responsive behavior.

// =============================================================================
// Fixed Desktop Constants (exact dimensions)
// =============================================================================

/// Top toolbar height: 38px
pub const TOOLBAR_HEIGHT: f32 = 38.0;

/// Button size: 32px
pub const BUTTON_SIZE: f32 = 32.0;

/// Icon size: 18px
pub const ICON_SIZE: f32 = 18.0;

/// Padding: 4px
pub const PADDING: f32 = 4.0;

/// Corner rounding: 4px
pub const ROUNDING: f32 = 4.0;

// =============================================================================
// Config Struct
// =============================================================================

/// Configuration for the top toolbar - fixed desktop dimensions
#[derive(Clone, Debug)]
pub struct TopToolbarConfig {
    /// Toolbar height: 38px
    pub height: f32,
    /// Icon size: 18px
    pub small_icon_size: f32,
    /// Button size: 32px
    pub btn_size: f32,
    /// Padding: 4px
    pub padding: f32,
    /// Border radius: 4px
    pub rounding: f32,
    // Legacy fields kept for compatibility
    pub base_height: f32,
    pub base_icon_size: f32,
    pub base_btn_size: f32,
    pub base_padding: f32,
}

impl Default for TopToolbarConfig {
    fn default() -> Self {
        Self {
            // Fixed desktop values
            height: TOOLBAR_HEIGHT,
            small_icon_size: ICON_SIZE,
            btn_size: BUTTON_SIZE,
            padding: PADDING,
            rounding: ROUNDING,
            // Legacy fields (same as primary values)
            base_height: TOOLBAR_HEIGHT,
            base_icon_size: ICON_SIZE,
            base_btn_size: BUTTON_SIZE,
            base_padding: PADDING,
        }
    }
}

impl TopToolbarConfig {
    /// Create new config with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set toolbar height
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self.base_height = height;
        self
    }

    /// Builder: set icon size
    pub fn with_icon_size(mut self, size: f32) -> Self {
        self.small_icon_size = size;
        self.base_icon_size = size;
        self
    }

    /// Builder: set button size
    pub fn with_button_size(mut self, size: f32) -> Self {
        self.btn_size = size;
        self.base_btn_size = size;
        self
    }
}
