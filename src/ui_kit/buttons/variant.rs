//! ButtonVariant - Button emphasis levels (rerun-inspired)
//!
//! Provides semantic button variants that communicate different levels
//! of importance and action types.

use egui::{Color32, Style};

use crate::tokens::DESIGN_TOKENS;

/// Button variant for different emphasis levels
///
/// Following rerun's pattern, buttons have semantic variants that
/// communicate importance and action type visually.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// High emphasis - inverse colors (Save, Submit, Confirm)
    ///
    /// Use for primary actions that are the main call-to-action on a screen.
    Primary,

    /// Medium emphasis - subtle background (Cancel, Reset)
    ///
    /// Use for secondary actions that support the primary action.
    Secondary,

    /// Low emphasis - transparent background (default toolbar buttons)
    ///
    /// Use for tertiary actions or when you need buttons to blend in.
    #[default]
    Ghost,

    /// Border stroke - alternative to Secondary
    ///
    /// Use when you need a button that's more prominent than Ghost
    /// but less prominent than Secondary.
    Outlined,

    /// Destructive action - red colors (Delete, Remove)
    ///
    /// Use for actions that are destructive or irreversible.
    Danger,
}

impl ButtonVariant {
    /// Get the background color for this variant in the given state
    pub fn bg_color(&self, hovered: bool, active: bool) -> Color32 {
        let tokens = &DESIGN_TOKENS.semantic.buttons;

        match self {
            Self::Primary => {
                if active {
                    tokens.primary_bg_active
                } else if hovered {
                    tokens.primary_bg_hover
                } else {
                    tokens.primary_bg
                }
            }
            Self::Secondary => {
                if active {
                    tokens.secondary_bg_active
                } else if hovered {
                    tokens.secondary_bg_hover
                } else {
                    tokens.secondary_bg
                }
            }
            Self::Ghost => Color32::TRANSPARENT,
            Self::Outlined => Color32::TRANSPARENT,
            Self::Danger => {
                if active {
                    tokens.danger_bg_active
                } else if hovered {
                    tokens.danger_bg_hover
                } else {
                    tokens.danger_bg
                }
            }
        }
    }

    /// Get the foreground (text/icon) color for this variant
    pub fn fg_color(&self) -> Color32 {
        let tokens = &DESIGN_TOKENS.semantic.buttons;

        match self {
            Self::Primary => tokens.primary_fg,
            Self::Secondary => tokens.secondary_fg,
            Self::Ghost => Color32::PLACEHOLDER, // Will use visuals
            Self::Outlined => Color32::PLACEHOLDER, // Will use visuals
            Self::Danger => tokens.danger_fg,
        }
    }

    /// Get the border color for this variant (only meaningful for Outlined)
    pub fn border_color(&self, hovered: bool) -> Option<Color32> {
        let tokens = &DESIGN_TOKENS.semantic.buttons;

        match self {
            Self::Outlined => Some(if hovered {
                tokens.outlined_border_hover
            } else {
                tokens.outlined_border
            }),
            _ => None,
        }
    }

    /// Apply this variant's styling to an egui Style
    ///
    /// This modifies the style's widget colors to match the variant.
    /// The style should be restored after rendering the button.
    pub fn apply_to_style(&self, style: &mut Style, hovered: bool, active: bool) {
        let bg = self.bg_color(hovered, active);
        let fg = self.fg_color();

        // Only apply non-placeholder colors
        if fg != Color32::PLACEHOLDER {
            style.visuals.widgets.inactive.fg_stroke.color = fg;
            style.visuals.widgets.hovered.fg_stroke.color = fg;
            style.visuals.widgets.active.fg_stroke.color = fg;
        }

        style.visuals.widgets.inactive.bg_fill = bg;
        style.visuals.widgets.hovered.bg_fill = bg;
        style.visuals.widgets.active.bg_fill = bg;

        // Apply border for Outlined variant
        if let Some(border_color) = self.border_color(hovered) {
            style.visuals.widgets.inactive.bg_stroke.color = border_color;
            style.visuals.widgets.inactive.bg_stroke.width = 1.0;
            style.visuals.widgets.hovered.bg_stroke.color = border_color;
            style.visuals.widgets.hovered.bg_stroke.width = 1.0;
        }
    }

    /// Check if this variant uses inverse colors (light text on dark bg)
    pub fn is_inverse(&self) -> bool {
        matches!(self, Self::Primary | Self::Danger)
    }

    /// Check if this variant has a visible background
    pub fn has_background(&self) -> bool {
        !matches!(self, Self::Ghost | Self::Outlined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_ghost() {
        assert_eq!(ButtonVariant::default(), ButtonVariant::Ghost);
    }

    #[test]
    fn test_inverse_variants() {
        assert!(ButtonVariant::Primary.is_inverse());
        assert!(ButtonVariant::Danger.is_inverse());
        assert!(!ButtonVariant::Ghost.is_inverse());
        assert!(!ButtonVariant::Secondary.is_inverse());
        assert!(!ButtonVariant::Outlined.is_inverse());
    }

    #[test]
    fn test_background_variants() {
        assert!(ButtonVariant::Primary.has_background());
        assert!(ButtonVariant::Secondary.has_background());
        assert!(ButtonVariant::Danger.has_background());
        assert!(!ButtonVariant::Ghost.has_background());
        assert!(!ButtonVariant::Outlined.has_background());
    }

    #[test]
    fn test_ghost_transparent() {
        assert_eq!(
            ButtonVariant::Ghost.bg_color(false, false),
            Color32::TRANSPARENT
        );
        assert_eq!(
            ButtonVariant::Ghost.bg_color(true, false),
            Color32::TRANSPARENT
        );
    }

    #[test]
    fn test_outlined_has_border() {
        assert!(ButtonVariant::Outlined.border_color(false).is_some());
        assert!(ButtonVariant::Outlined.border_color(true).is_some());
        assert!(ButtonVariant::Ghost.border_color(false).is_none());
    }
}
