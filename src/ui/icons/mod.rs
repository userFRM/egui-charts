//! Icon size presets.

use crate::tokens::DESIGN_TOKENS;

/// Icon size presets
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSize {
    /// 14x14 pixels
    Small,
    /// 18x18 pixels
    Medium,
    /// 24x24 pixels
    Large,
    /// 32x32 pixels
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
