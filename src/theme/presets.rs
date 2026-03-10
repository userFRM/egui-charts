//! Theme Presets - Pre-built complete themes
//!
//! This module provides ready-to-use theme presets that match common design languages.
//! Presets define which variant of colors (light/dark UI, light/dark chart) to use,
//! and the actual colors come from DESIGN_TOKENS.

use super::Theme;

// ============================================================================
// THEME PRESET ENUM
// ============================================================================

/// Pre-defined theme presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemePreset {
    /// Classic: Light UI chrome + Dark chart
    #[default]
    Classic,
    /// Full dark theme
    Dark,
    /// Full light theme
    Light,
    /// Midnight blue theme (dark UI + dark chart)
    Midnight,
    /// High contrast for accessibility (dark UI + dark chart)
    HighContrast,
}

impl ThemePreset {
    /// Get all available presets
    pub fn all() -> &'static [ThemePreset] {
        &[
            ThemePreset::Classic,
            ThemePreset::Dark,
            ThemePreset::Light,
            ThemePreset::Midnight,
            ThemePreset::HighContrast,
        ]
    }

    /// Get preset name (for persistence)
    pub fn name(&self) -> &'static str {
        match self {
            ThemePreset::Classic => "classic",
            ThemePreset::Dark => "dark",
            ThemePreset::Light => "light",
            ThemePreset::Midnight => "midnight",
            ThemePreset::HighContrast => "high_contrast",
        }
    }

    /// Get display name (for UI)
    pub fn display_name(&self) -> &'static str {
        match self {
            ThemePreset::Classic => "Classic",
            ThemePreset::Dark => "Dark",
            ThemePreset::Light => "Light",
            ThemePreset::Midnight => "Midnight",
            ThemePreset::HighContrast => "High Contrast",
        }
    }

    /// Parse preset from name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "classic" => Some(ThemePreset::Classic),
            "dark" => Some(ThemePreset::Dark),
            "light" => Some(ThemePreset::Light),
            "midnight" => Some(ThemePreset::Midnight),
            "high_contrast" | "highcontrast" => Some(ThemePreset::HighContrast),
            _ => None,
        }
    }

    /// Whether this preset uses dark UI chrome
    pub fn is_dark_ui(&self) -> bool {
        matches!(
            self,
            ThemePreset::Dark | ThemePreset::Midnight | ThemePreset::HighContrast
        )
    }

    /// Whether this preset uses dark chart background
    pub fn is_dark_chart(&self) -> bool {
        !matches!(self, ThemePreset::Light)
    }

    /// Convert to full Theme
    pub fn to_theme(self) -> Theme {
        Theme::from_preset(self)
    }
}
