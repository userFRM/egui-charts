//! Icons Module - Legacy icon system
//!
//! TEXT: This module is deprecated. Use `crate::icons` instead.
//! The new icon system uses compile-time embedding and the `Icon` struct.
//!
//! Migration:
//! - `SvgIcon::TrendLine` → `icons::TREND_LINE`
//! - `render_svg_icon(ui, icon, size, tint)` → `icon.as_image_tinted(size, tint).paint_at(ui, rect)`

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
    pub fn size(&self) -> f32 {
        match self {
            IconSize::Small => DESIGN_TOKENS.sizing.icon_sm,
            IconSize::Medium => DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            IconSize::Large => DESIGN_TOKENS.sizing.icon_lg,
            IconSize::XLarge => DESIGN_TOKENS.sizing.icon_xxl,
        }
    }
}
