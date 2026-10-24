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
    pub fn size(&self) -> f32 {
        match self {
            IconSize::Small => DESIGN_TOKENS.sizing.icon_sm,
            IconSize::Medium => DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            IconSize::Large => DESIGN_TOKENS.sizing.icon_lg,
            IconSize::XLarge => DESIGN_TOKENS.sizing.icon_xxl,
        }
    }
}

/// Leftward single-point arrow mark.
pub const ARROW_MARK_LEFT: crate::icons::Icon = crate::icons::Icon::new(
    "marks/arrow-mark-left.svg",
    include_bytes!("arrow-mark-left.svg"),
);
/// Rightward single-point arrow mark.
pub const ARROW_MARK_RIGHT: crate::icons::Icon = crate::icons::Icon::new(
    "marks/arrow-mark-right.svg",
    include_bytes!("arrow-mark-right.svg"),
);
/// Circular sector with its center and radius anchors.
pub const SECTOR: crate::icons::Icon =
    crate::icons::Icon::new("marks/sector.svg", include_bytes!("sector.svg"));
/// Sticker with a curled corner.
pub const STICKER: crate::icons::Icon =
    crate::icons::Icon::new("marks/sticker.svg", include_bytes!("sticker.svg"));
