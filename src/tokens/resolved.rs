//! Resolved design token structures -- the public API types.
//!
//! These are the final, fully-resolved structs that compose [`DesignTokens`].
//! All palette references have been resolved to concrete [`egui::Color32`] values
//! and all numeric tokens are plain `f32`.

use super::semantic::SemanticTokens;
use super::sizing::{
    LayoutTokens, RoundingTokens, ShadowTokens, SizingTokens, StrokeTokens, TypographyTokens,
};

/// The root design tokens structure -- single source of truth for all styling primitives.
///
/// Access the global instance via [`DESIGN_TOKENS`](super::DESIGN_TOKENS).
/// Contains spacing, sizing, rounding, typography, stroke widths, shadow
/// parameters, and semantic color tokens organized by domain (UI, chart,
/// indicators, drawings, etc.).
#[derive(Clone, Debug)]
pub struct DesignTokens {
    pub spacing: SpacingTokens,
    pub sizing: SizingTokens,
    pub rounding: RoundingTokens,
    pub typography: TypographyTokens,
    pub stroke: StrokeTokens,
    pub shadow: ShadowTokens,
    pub layout: LayoutTokens,
    pub semantic: SemanticTokens,
}

/// Spacing tokens defining consistent gaps, padding, and margins.
///
/// Values follow a graduated scale: `xs` (2px) through `xxxl` (32px).
/// Use semantic aliases like `panel_gap`, `button_padding`, etc. for
/// domain-specific spacing.
#[derive(Clone, Debug)]
pub struct SpacingTokens {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub xxxl: f32,
    pub hairline: f32,
    pub section_lg: f32,
    pub panel_gap: f32,
    pub button_padding: f32,
    pub toolbar_item_gap: f32,
    pub menu_item_padding: f32,
}
