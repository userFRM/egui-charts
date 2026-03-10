//! UI Styles - Sizing Constants and Layout Helpers
//!
//! This module provides sizing constants and layout helpers.
//! Core spacing, rounding, and button sizing are now in `DESIGN_TOKENS`.
//!
//! # Architecture
//!
//! ```text
//! styles/
//! ├── sizing.rs      - Fixed desktop sizing constants
//! ├── typography.rs  - Font sizes
//! └── icons.rs       - Icon sizes
//!
//! tokens/
//! └── mod.rs         - DESIGN_TOKENS (spacing, sizing, rounding from RON)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tokens::DESIGN_TOKENS;
//! use crate::styles::{sizing, typography};
//!
//! // Spacing (from DESIGN_TOKENS)
//! let gap = DESIGN_TOKENS.spacing.panel_gap;
//! let padding = DESIGN_TOKENS.spacing.lg;
//!
//! // Sizing (fixed desktop constants)
//! let toolbar_height = sizing::toolbar::TOP_HEIGHT;
//! let btn_size = DESIGN_TOKENS.sizing.button_md;
//!
//! // Rounding (from DESIGN_TOKENS)
//! let corner_radius = DESIGN_TOKENS.rounding.button;
//!
//! // Typography
//! let font_size = typography::MD;
//!
//! // Margin helpers
//! let margin = styles::margin::same(DESIGN_TOKENS.spacing.md);
//! ```
//!
//! # Design Philosophy
//!
//! - **Single source of truth**: DESIGN_TOKENS loaded from RON files
//! - **Semantic naming**: Use `DESIGN_TOKENS.rounding.button` not magic numbers
//! - **IDE-friendly**: Autocomplete shows available options

pub mod focus;
pub mod icons;
pub(crate) mod responsive;
pub mod sizing;
pub mod typography;

// =============================================================================
// Margin Helpers for egui Compatibility
// =============================================================================

/// Helper functions for creating [`egui::Margin`] from `f32` values.
///
/// egui 0.33+ uses `i8` for `Margin` fields. These helpers handle the
/// `f32 -> i8` cast so callers can use design token values directly.
///
/// # Example
///
/// ```rust,ignore
/// use egui_charts::styles::margin;
/// use egui_charts::tokens::DESIGN_TOKENS;
///
/// let m = margin::same(DESIGN_TOKENS.spacing.md);
/// let m2 = margin::symmetric(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.sm);
/// ```
pub mod margin {
    use egui::Margin;

    /// Create uniform margin from f32 value
    #[inline]
    pub fn same(v: f32) -> Margin {
        Margin::same(v as i8)
    }

    /// Create symmetric margin (horizontal, vertical) from f32 values
    #[inline]
    pub fn symmetric(h: f32, v: f32) -> Margin {
        Margin::symmetric(h as i8, v as i8)
    }

    /// Create margin with individual sides from f32 values
    #[inline]
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Margin {
        Margin {
            left: left as i8,
            right: right as i8,
            top: top as i8,
            bottom: bottom as i8,
        }
    }

    /// Create margin with only left side
    #[inline]
    pub fn left(v: f32) -> Margin {
        Margin {
            left: v as i8,
            right: 0,
            top: 0,
            bottom: 0,
        }
    }

    /// Create margin with only right side
    #[inline]
    pub fn right(v: f32) -> Margin {
        Margin {
            left: 0,
            right: v as i8,
            top: 0,
            bottom: 0,
        }
    }

    /// Create margin with only top side
    #[inline]
    pub fn top(v: f32) -> Margin {
        Margin {
            left: 0,
            right: 0,
            top: v as i8,
            bottom: 0,
        }
    }

    /// Create margin with only bottom side
    #[inline]
    pub fn bottom(v: f32) -> Margin {
        Margin {
            left: 0,
            right: 0,
            top: 0,
            bottom: v as i8,
        }
    }
}

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
