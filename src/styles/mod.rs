//! UI styles: the keyboard focus ring and sizing, typography, icon and stroke constants.
//!
//! Core spacing, rounding, and button sizing are now in `DESIGN_TOKENS`.
//!
//! # Architecture
//!
//! ```text
//! styles/
//! ├── focus.rs       - Keyboard focus ring
//! ├── sizing.rs      - Chart layout constants
//! ├── typography.rs  - Font sizes
//! └── icons.rs       - Icon sizes
//!
//! tokens/
//! └── mod.rs         - DESIGN_TOKENS (spacing, sizing, rounding)
//! ```
//!
//! # Usage
//!
//! ```rust
//! use egui_charts::styles::{sizing, typography};
//! use egui_charts::tokens::DESIGN_TOKENS;
//!
//! // Spacing (from DESIGN_TOKENS)
//! let gap = DESIGN_TOKENS.spacing.panel_gap;
//! let padding = DESIGN_TOKENS.spacing.lg;
//!
//! // Sizing (fixed desktop constants)
//! let axis_width = sizing::chart::RIGHT_AXIS_WIDTH;
//! let btn_size = DESIGN_TOKENS.sizing.button_md;
//!
//! // Rounding (from DESIGN_TOKENS)
//! let corner_radius = DESIGN_TOKENS.rounding.button;
//!
//! // Typography
//! let font_size = typography::MD;
//! ```
//!
//! # Design Philosophy
//!
//! - **Single source of truth**: `DESIGN_TOKENS`, defined as Rust constants
//! - **Semantic naming**: Use `DESIGN_TOKENS.rounding.button` not magic numbers
//! - **IDE-friendly**: Autocomplete shows available options

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
