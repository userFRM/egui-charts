//! Color palettes and per-indicator color schemes.
//!
//! This module provides [`IndicatorPalette`] -- a categorised set of colors
//! (primary, bullish, bearish, neutral, accent) that indicators draw from --
//! and [`IndicatorColorSchemes`] which offers ready-made color sets for the
//! most common indicators (SMA, EMA, RSI, MACD, Bollinger Bands, etc.).
//!
//! Colors are sourced from the crate's design-token system so they stay
//! consistent with the overall chart theme.

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// A categorised color palette for technical indicators.
///
/// Each category contains multiple colors that cycle when more indicators
/// are added than there are distinct colours (see [`primary`](Self::primary)).
///
/// Use [`Default::default`] to get the standard palette derived from the
/// crate's design tokens.
pub struct IndicatorPalette {
    /// Primary indicator colors (blue, green, orange, purple, ...).
    pub primary: Vec<Color32>,
    /// Colors used for bullish / positive signals (greens).
    pub bullish: Vec<Color32>,
    /// Colors used for bearish / negative signals (reds).
    pub bearish: Vec<Color32>,
    /// Neutral / muted colors (grays).
    pub neutral: Vec<Color32>,
    /// Accent colors for multi-line or overlay indicators.
    pub accent: Vec<Color32>,
}

impl Default for IndicatorPalette {
    fn default() -> Self {
        Self {
            primary: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue
                DESIGN_TOKENS.semantic.extended.success, // Green
                DESIGN_TOKENS.semantic.extended.warning, // Orange
                DESIGN_TOKENS.semantic.extended.purple,  // Purple
                DESIGN_TOKENS.semantic.extended.error,   // Red
                DESIGN_TOKENS.semantic.extended.cyan,    // Cyan
                DESIGN_TOKENS.semantic.extended.caution, // Yellow
                DESIGN_TOKENS.semantic.extended.brown,   // Brown
            ],
            bullish: vec![
                DESIGN_TOKENS.semantic.extended.success, // Material Green
                DESIGN_TOKENS.semantic.extended.bullish, // Bullish Green
                DESIGN_TOKENS.semantic.extended.success_light, // Light Green
                DESIGN_TOKENS.semantic.extended.bullish, // Bright Green
            ],
            bearish: vec![
                DESIGN_TOKENS.semantic.extended.error,   // Material Red
                DESIGN_TOKENS.semantic.extended.bearish, // Bearish Red
                DESIGN_TOKENS.semantic.extended.bearish, // Light Red
                DESIGN_TOKENS.semantic.extended.error,   // Bright Red
            ],
            neutral: vec![
                DESIGN_TOKENS.semantic.extended.disabled,             // Gray
                DESIGN_TOKENS.semantic.extended.text_muted,           // Light Gray
                DESIGN_TOKENS.semantic.extended.chart_text_secondary, // Dark Gray
                DESIGN_TOKENS.semantic.extended.bg_active,            // Very Light Gray
            ],
            accent: vec![
                DESIGN_TOKENS.semantic.extended.favorite_gold, // Amber
                DESIGN_TOKENS.semantic.extended.pink,          // Pink
                DESIGN_TOKENS.semantic.extended.deep_purple,   // Deep Purple
                DESIGN_TOKENS.semantic.extended.bullish,       // Teal
                DESIGN_TOKENS.semantic.extended.deep_orange,   // Deep Orange
                DESIGN_TOKENS.semantic.extended.success_light, // Lime
            ],
        }
    }
}

impl IndicatorPalette {
    /// Get a color by category and index
    pub fn get_color(&self, category: ColorCategory, index: usize) -> Color32 {
        let colors = match category {
            ColorCategory::Primary => &self.primary,
            ColorCategory::Bullish => &self.bullish,
            ColorCategory::Bearish => &self.bearish,
            ColorCategory::Neutral => &self.neutral,
            ColorCategory::Accent => &self.accent,
        };

        colors[index % colors.len()]
    }

    /// Get a primary color by index (cycles if index >= length)
    pub fn primary(&self, index: usize) -> Color32 {
        self.primary[index % self.primary.len()]
    }

    /// Get a bullish color by index
    pub fn bullish(&self, index: usize) -> Color32 {
        self.bullish[index % self.bullish.len()]
    }

    /// Get a bearish color by index
    pub fn bearish(&self, index: usize) -> Color32 {
        self.bearish[index % self.bearish.len()]
    }

    /// Get a neutral color by index
    pub fn neutral(&self, index: usize) -> Color32 {
        self.neutral[index % self.neutral.len()]
    }

    /// Get an accent color by index
    pub fn accent(&self, index: usize) -> Color32 {
        self.accent[index % self.accent.len()]
    }
}

/// Categories of indicator colors available in the [`IndicatorPalette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorCategory {
    /// General-purpose indicator colors.
    Primary,
    /// Colors associated with bullish / positive signals.
    Bullish,
    /// Colors associated with bearish / negative signals.
    Bearish,
    /// Muted colors for auxiliary or background elements.
    Neutral,
    /// Vivid accent colors for multi-line indicators.
    Accent,
}

/// Ready-made color schemes for the most popular indicators.
///
/// Each method returns one or more [`Color32`] values sourced from the
/// crate's design tokens, matching the colours that the corresponding
/// built-in indicator uses by default.
pub struct IndicatorColorSchemes;

impl IndicatorColorSchemes {
    /// Simple Moving Avg - Orange
    pub fn sma() -> Color32 {
        DESIGN_TOKENS.semantic.indicators.ma
    }

    /// Exponential Moving Avg - Blue
    pub fn ema() -> Color32 {
        DESIGN_TOKENS.semantic.indicators.ema
    }

    /// Bollinger Bands - Purple (all three lines)
    pub fn bollinger_bands() -> Vec<Color32> {
        vec![
            DESIGN_TOKENS.semantic.indicators.bb_upper,  // Upper
            DESIGN_TOKENS.semantic.indicators.bb_middle, // Middle
            DESIGN_TOKENS.semantic.indicators.bb_lower,  // Lower
        ]
    }

    /// RSI - Purple
    pub fn rsi() -> Color32 {
        DESIGN_TOKENS.semantic.indicators.rsi
    }

    /// MACD - Multi-line scheme
    pub fn macd() -> Vec<Color32> {
        vec![
            DESIGN_TOKENS.semantic.indicators.macd_line, // MACD Line (Blue)
            DESIGN_TOKENS.semantic.indicators.macd_signal, // Signal Line (Orange)
            DESIGN_TOKENS.semantic.extended.disabled,    // Histogram (Gray)
        ]
    }

    /// Volume - Bullish/Bearish scheme
    pub fn volume() -> Vec<Color32> {
        vec![
            DESIGN_TOKENS.semantic.extended.bullish, // Bullish (Green)
            DESIGN_TOKENS.semantic.extended.bearish, // Bearish (Red)
        ]
    }

    /// Stochastic Oscillator
    pub fn stochastic() -> Vec<Color32> {
        vec![
            DESIGN_TOKENS.semantic.extended.info,  // %K (Blue)
            DESIGN_TOKENS.semantic.extended.error, // %D (Red)
        ]
    }

    /// Avg True Range - Cyan
    pub fn atr() -> Color32 {
        DESIGN_TOKENS.semantic.extended.cyan
    }

    /// Ichimoku Cloud
    pub fn ichimoku() -> Vec<Color32> {
        vec![
            DESIGN_TOKENS.semantic.extended.info,    // Tenkan-sen (Blue)
            DESIGN_TOKENS.semantic.extended.error,   // Kijun-sen (Red)
            DESIGN_TOKENS.semantic.extended.success, // Senkou Span A (Green)
            DESIGN_TOKENS.semantic.extended.warning, // Senkou Span B (Orange)
            DESIGN_TOKENS.semantic.extended.purple,  // Chikou Span (Purple)
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_cycling() {
        let palette = IndicatorPalette::default();

        // Test that cycling works
        let color1 = palette.primary(0);
        let color2 = palette.primary(8); // Should cycle back to index 0
        assert_eq!(color1, color2);
    }

    #[test]
    fn test_color_categories() {
        let palette = IndicatorPalette::default();

        assert!(!palette.primary.is_empty());
        assert!(!palette.bullish.is_empty());
        assert!(!palette.bearish.is_empty());
        assert!(!palette.neutral.is_empty());
        assert!(!palette.accent.is_empty());
    }
}
