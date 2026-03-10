//! Color abstraction for chart elements.
//!
//! [`ColorType`] lets users specify solid colors or gradients in a
//! serialization-friendly format that integrates with both egui's `Color32`
//! and CSS-style hex strings.

/// Color type abstraction.
/// Provides a generic way for users to specify colors in various formats.
use egui::Color32;
use serde::{Deserialize, Serialize};

/// A color specification that can be a solid RGBA value or a gradient.
///
/// Most chart elements accept a `ColorType` so users can choose between flat
/// colors and gradients without changing APIs.
///
/// # Example
///
/// ```
/// use egui_charts::model::ColorType;
///
/// let green = ColorType::from_hex("#26a69a").unwrap();
/// assert!(green.is_solid());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorType {
    /// Solid color (RGBA)
    Solid(Color32),

    /// Vertical gradient from top to bottom
    VerticalGradient {
        top_color: Color32,
        bottom_color: Color32,
    },

    /// Horizontal gradient from left to right (rarely used)
    HorizontalGradient {
        left_color: Color32,
        right_color: Color32,
    },
}

impl ColorType {
    /// Create a solid color from RGB values
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Solid(Color32::from_rgb(r, g, b))
    }

    /// Create a solid color from RGBA values
    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::Solid(Color32::from_rgba_premultiplied(r, g, b, a))
    }

    /// Create a solid color from hex string (e.g., "#FF5733" or "FF5733")
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let hex = hex.trim_start_matches('#');

        if hex.len() != 6 && hex.len() != 8 {
            return Err(format!("Invalid hex color: {hex}"));
        }

        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| format!("Invalid red component: {}", &hex[0..2]))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| format!("Invalid green component: {}", &hex[2..4]))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| format!("Invalid blue component: {}", &hex[4..6]))?;

        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| format!("Invalid alpha component: {}", &hex[6..8]))?
        } else {
            255
        };

        Ok(Self::Solid(Color32::from_rgba_premultiplied(r, g, b, a)))
    }

    /// Create a vertical gradient
    pub fn vertical_gradient(top: Color32, bottom: Color32) -> Self {
        Self::VerticalGradient {
            top_color: top,
            bottom_color: bottom,
        }
    }

    /// Create a horizontal gradient
    pub fn horizontal_gradient(left: Color32, right: Color32) -> Self {
        Self::HorizontalGradient {
            left_color: left,
            right_color: right,
        }
    }

    /// Get the primary color (for solid) or top/left color (for gradients)
    pub fn primary_color(&self) -> Color32 {
        match self {
            Self::Solid(color) => *color,
            Self::VerticalGradient { top_color, .. } => *top_color,
            Self::HorizontalGradient { left_color, .. } => *left_color,
        }
    }

    /// Check if this is a solid color
    pub fn is_solid(&self) -> bool {
        matches!(self, Self::Solid(_))
    }

    /// Check if this is a gradient
    pub fn is_gradient(&self) -> bool {
        !self.is_solid()
    }

    /// Convert to Color32 (uses primary color for gradients)
    pub fn to_color32(&self) -> Color32 {
        self.primary_color()
    }
}

impl From<Color32> for ColorType {
    fn from(color: Color32) -> Self {
        Self::Solid(color)
    }
}

impl From<ColorType> for Color32 {
    fn from(color_type: ColorType) -> Self {
        color_type.to_color32()
    }
}

impl Default for ColorType {
    fn default() -> Self {
        Self::Solid(Color32::WHITE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_hex() {
        let color = ColorType::from_hex("#FF5733").unwrap();
        assert!(color.is_solid());
        assert_eq!(color.primary_color(), Color32::from_rgb(255, 87, 51));

        let color_no_hash = ColorType::from_hex("FF5733").unwrap();
        assert_eq!(color, color_no_hash);
    }

    #[test]
    fn test_gradient() {
        let gradient = ColorType::vertical_gradient(
            Color32::from_rgb(255, 0, 0),
            Color32::from_rgb(0, 0, 255),
        );
        assert!(gradient.is_gradient());
        assert!(!gradient.is_solid());
    }

    #[test]
    fn test_conversions() {
        let color32 = Color32::from_rgb(100, 150, 200);
        let color_type: ColorType = color32.into();
        let back: Color32 = color_type.into();
        assert_eq!(color32, back);
    }
}
