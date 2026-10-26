//! UI sizing, typography, icon sizes and keyboard focus helpers.
//!
//! UI colours, text, spacing and radii come from the applied [`Theme`](crate::theme::Theme).
//! These modules retain fixed component dimensions and the default font sizes used by
//! the theme. See `docs/theme.md` for runtime customisation.

pub mod focus;
pub mod icons;
#[cfg(feature = "ui")]
pub(crate) mod responsive;
pub mod sizing;
pub mod typography;

// =============================================================================
// Stroke Helpers
// =============================================================================

/// Common stroke width constants (legacy -- prefer `DESIGN_TOKENS.stroke.*`).
pub mod stroke {
    /// Hairline stroke (1px)
    pub const HAIRLINE: f32 = 1.0;

    /// Thin stroke (1.2px)
    pub const THIN: f32 = 1.2;

    /// Medium stroke (1.5px)
    pub const MEDIUM: f32 = 1.5;

    /// Thick stroke (2px)
    pub const THICK: f32 = 2.0;
}
