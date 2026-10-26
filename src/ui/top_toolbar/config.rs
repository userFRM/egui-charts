//! Top toolbar dimensions and spacing.
//!
//! Use [`TopToolbarConfig`] to size controls; colors come from egui visuals and
//! the active theme.

// =============================================================================
// Fixed Desktop Constants (exact dimensions)
// =============================================================================

/// Default top toolbar height in egui logical points.
pub const TOOLBAR_HEIGHT: f32 = 38.0;

/// Default button side length in egui logical points.
pub const BUTTON_SIZE: f32 = 32.0;

/// Default icon side length in egui logical points.
pub const ICON_SIZE: f32 = 24.0;

/// Default padding in egui logical points.
pub const PADDING: f32 = 4.0;

/// Default corner radius in egui logical points.
pub const ROUNDING: f32 = 4.0;

// =============================================================================
// Config Struct
// =============================================================================

/// Dimensions for the top toolbar in egui logical points.
#[derive(Clone, Debug)]
pub struct TopToolbarConfig {
    /// Toolbar height.
    pub height: f32,
    /// Icon side length.
    pub small_icon_size: f32,
    /// Button side length.
    pub btn_size: f32,
    /// Padding between button content.
    pub padding: f32,
    /// Button corner radius.
    pub rounding: f32,
    // Legacy fields kept for compatibility
    /// Retained for source compatibility; rendering uses `height`.
    pub base_height: f32,
    /// Retained for source compatibility; rendering uses `small_icon_size`.
    pub base_icon_size: f32,
    /// Retained for source compatibility; rendering uses `btn_size`.
    pub base_btn_size: f32,
    /// Retained for source compatibility; rendering uses `padding`.
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
