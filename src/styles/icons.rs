//! Icon size constants
//!
//! Consistent icon sizing across toolbar and UI components.

// =============================================================================
// Icon Size Scale
// =============================================================================

/// Tiny icon (11px) - status indicators, compact badges
pub const TINY: f32 = 11.0;

/// Extra small icon (12px)
pub const XS: f32 = 12.0;

/// Compact icon (14px) - floating controls, compact buttons
pub const COMPACT: f32 = 14.0;

/// Small icon (16px)
pub const SM: f32 = 16.0;

/// Small-medium icon (18px) - bottom toolbar, compact btns
pub const SM_MD: f32 = 18.0;

/// Medium icon (20px)
pub const MD: f32 = 20.0;

/// Large icon (24px)
pub const LG: f32 = 24.0;

/// Extra large icon (28px) - standard toolbar icon
pub const XL: f32 = 28.0;

/// 2X large icon (32px)
pub const XXL: f32 = 32.0;

// =============================================================================
// Semantic Aliases
// =============================================================================

/// Standard toolbar icon size
pub const TOOLBAR: f32 = XL;

/// Menu item icon size
pub const MENU: f32 = MD;

/// Button icon size (small)
pub const BUTTON_SM: f32 = SM;

/// Button icon size (standard)
pub const BUTTON: f32 = MD;

/// Indicator/badge icon size
pub const INDICATOR: f32 = SM;

/// Bottom toolbar icon size
pub const BOTTOM_TOOLBAR: f32 = SM_MD;

/// Status indicator icon size (compact badges)
pub const STATUS_INDICATOR: f32 = TINY;

/// Floating control icon size (compact floating toolbars)
pub const FLOATING_CONTROL: f32 = COMPACT;

// =============================================================================
// UiExt Aliases (for extension traits)
// =============================================================================

/// Small icon size (16px) - for UiExt::small_icon
pub const SMALL: f32 = SM;

/// Medium icon size (20px) - for UiExt::medium_icon
pub const MEDIUM: f32 = MD;

/// Large icon size (24px) - for UiExt::large_icon
pub const LARGE: f32 = LG;
