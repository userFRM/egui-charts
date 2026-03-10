//! Design Token System -- RON-based, compile-time loaded design primitives.
//!
//! This module is the foundation of the `egui-charts` design system. It provides
//! a single global [`DESIGN_TOKENS`] instance containing all spacing, sizing,
//! rounding, typography, stroke widths, shadow offsets, and semantic color
//! definitions used throughout the crate.
//!
//! Tokens are defined in `design_tokens.ron` (embedded at compile time via
//! `include_str!`), parsed on first access with [`once_cell::sync::Lazy`],
//! and stored in resolved Rust structs for zero-cost access at runtime.
//!
//! # Architecture
//!
//! ```text
//! design_tokens.ron  (RON source file, human-editable)
//!       |  include_str! at compile time
//!       v
//! raw.rs    (Serde deserialization structs)
//!       |  resolve palette references
//!       v
//! parser.rs (converts to Color32, builds final structs)
//!       |
//!       v
//! resolved.rs + semantic.rs + sizing.rs  (public API structs)
//!       |
//!       v
//! DESIGN_TOKENS  (global Lazy<DesignTokens>)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use egui_charts::tokens::DESIGN_TOKENS;
//!
//! // Spacing
//! let gap = DESIGN_TOKENS.spacing.md;
//! let padding = DESIGN_TOKENS.spacing.lg;
//!
//! // Colors
//! let bullish = DESIGN_TOKENS.semantic.chart.bullish;
//! let accent = DESIGN_TOKENS.semantic.brand.accent;
//!
//! // Sizing
//! let btn_height = DESIGN_TOKENS.sizing.button_md;
//!
//! // Typography
//! let body_size = DESIGN_TOKENS.typography.md;
//!
//! // Strokes
//! let thin = DESIGN_TOKENS.stroke.hairline;
//! ```
//!
//! # Customization
//!
//! To customize the design system, edit `design_tokens.ron` and rebuild.
//! The RON file supports palette references (e.g., `"green_500"`) that are
//! resolved during parsing, allowing you to define a color once and reuse
//! it across multiple semantic tokens.

use once_cell::sync::Lazy;

mod parser;
mod raw;
mod resolved;
mod semantic;
mod sizing;

pub use resolved::*;
pub use semantic::*;
pub use sizing::*;

/// Raw RON file contents embedded at compile time
const TOKENS_RON: &str = include_str!("design_tokens.ron");

/// Global design tokens instance - loaded once on first access
pub static DESIGN_TOKENS: Lazy<DesignTokens> = Lazy::new(|| {
    parser::parse_tokens(TOKENS_RON).expect(
        "Failed to parse embedded design_tokens.ron — the RON file is malformed. \
         Check syntax in presentation/ui_kit/tokens/design_tokens.ron",
    )
});
