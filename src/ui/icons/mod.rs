//! Icon size presets.

use crate::tokens::DESIGN_TOKENS;

/// Icon size presets
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSize {
    /// Small icon preset.
    Small,
    /// Medium icon preset.
    Medium,
    /// Large icon preset.
    Large,
    /// Extra-large icon preset.
    XLarge,
}

impl IconSize {
    /// Icon side length in egui logical points.
    pub fn size(&self) -> f32 {
        match self {
            IconSize::Small => DESIGN_TOKENS.sizing.icon_sm,
            IconSize::Medium => DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            IconSize::Large => DESIGN_TOKENS.sizing.icon_lg,
            IconSize::XLarge => DESIGN_TOKENS.sizing.icon_xxl,
        }
    }
}
