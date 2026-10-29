//! The root [`DesignTokens`] struct and its spacing scale.

use super::semantic::SemanticTokens;
use super::sizing::{LayoutTokens, RoundingTokens, ShadowTokens, SizingTokens, StrokeTokens};

/// Shared spacing, dimensions and color defaults for chart controls.
///
/// Access the global instance via [`DESIGN_TOKENS`](super::DESIGN_TOKENS).
/// Contains default spacing, sizing, rounding, stroke widths, shadow parameters
/// and semantic color tokens. UI colors and text, spacing and radius scales come
/// from the applied [`Theme`](crate::theme::Theme).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DesignTokens {
    /// Spacing defaults.
    pub spacing: SpacingTokens,
    /// Sizing defaults.
    pub sizing: SizingTokens,
    /// Rounding defaults.
    pub rounding: RoundingTokens,
    /// Stroke defaults.
    pub stroke: StrokeTokens,
    /// Shadow defaults.
    pub shadow: ShadowTokens,
    /// Layout defaults.
    pub layout: LayoutTokens,
    /// Semantic defaults.
    pub semantic: SemanticTokens,
}

/// Spacing tokens defining consistent gaps, padding, and margins.
///
/// Values are in egui logical points. Use `panel_gap` and `section_lg`
/// for panel and section spacing, or the graduated sizes for control layout.
#[derive(Clone, Debug)]
pub struct SpacingTokens {
    /// Extra small, in egui logical points.
    pub xs: f32,
    /// Small, in egui logical points.
    pub sm: f32,
    /// Medium, in egui logical points.
    pub md: f32,
    /// Large, in egui logical points.
    pub lg: f32,
    /// Extra large, in egui logical points.
    pub xl: f32,
    /// Double extra large, in egui logical points.
    pub xxl: f32,
    /// Triple extra large, in egui logical points.
    pub xxxl: f32,
    /// Hairline, in egui logical points.
    pub hairline: f32,
    /// Section large, in egui logical points.
    pub section_lg: f32,
    /// Panel gap, in egui logical points.
    pub panel_gap: f32,
}
