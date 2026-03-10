//! Unified theme-aware color helpers for egui-charts.
//!
//! Consolidates scattered theming code from multiple UI modules into a single
//! source of truth. All colors are sourced from egui's theme system.
//!
//! # Architecture
//!
//! ```text
//! ui_kit/theming.rs
//! ├── Panel colors       - toolbar_bg, separator_color
//! ├── Button states      - btn_bg_*, hover_color, sel_color
//! ├── Icon colors        - icon_color, active_icon_color
//! ├── Text colors        - text_color, muted_color
//! ├── Brand colors       - publish_button_* (accent blue)
//! └── Utilities          - lighten_color, darken_color, pill_bg
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use egui_charts::ui_kit::theming;
//!
//! // Panel backgrounds
//! let bg = theming::toolbar_bg(ui);
//!
//! // Button states
//! let normal = theming::btn_bg_normal(ui);
//! let hover = theming::hover_color(ui);
//! let pressed = theming::sel_color(ui);
//!
//! // Icons
//! let icon = theming::icon_color(ui);
//! ```
//!
//! # Design Principles
//!
//! - **Zero hardcoded colors**: All values from `ui.style().visuals`
//! - **Theme-reactive**: Colors update automatically on theme change
//! - **Single source**: No duplicate theming code across modules

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Ui};

// =============================================================================
// PANEL COLORS
// =============================================================================

/// Get toolbar/panel background color from theme.
///
/// This is the primary background color for toolbars and panels.
///
/// # Examples
///
/// ```ignore
/// let bg = theming::toolbar_bg(ui);
/// ui.painter().rect_filled(rect, 0.0, bg);
/// ```
#[inline]
pub fn toolbar_bg(ui: &Ui) -> Color32 {
    ui.style().visuals.panel_fill
}

/// Get separator/divider color from theme.
///
/// Used for visual dividers between toolbar sections.
///
/// # Examples
///
/// ```ignore
/// let color = theming::separator_color(ui);
/// ui.painter().hline(x_range, y, Stroke::new(1.0, color));
/// ```
#[inline]
pub fn separator_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.noninteractive.bg_stroke.color
}

// =============================================================================
// BUTTON STATE COLORS
// =============================================================================

/// Get button normal/default background (transparent).
///
/// Buttons are transparent by default, showing through to toolbar background.
#[inline]
pub fn btn_bg_normal(_ui: &Ui) -> Color32 {
    Color32::TRANSPARENT
}

/// Get button hover background color.
///
/// # Examples
///
/// ```ignore
/// if response.hovered() {
///     let bg = theming::hover_color(ui);
///     painter.rect_filled(rect, rounding, bg);
/// }
/// ```
#[inline]
pub fn hover_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.hovered.bg_fill
}

/// Alias for `hover_color` - semantic name for button context.
#[inline]
pub fn btn_bg_hover(ui: &Ui) -> Color32 {
    hover_color(ui)
}

/// Get selected/active/pressed background color.
///
/// Used when a button is actively selected or being pressed.
///
/// # Examples
///
/// ```ignore
/// if is_selected {
///     let bg = theming::sel_color(ui);
///     painter.rect_filled(rect, rounding, bg);
/// }
/// ```
#[inline]
pub fn sel_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.active.bg_fill
}

/// Alias for `sel_color` - semantic name for button context.
#[inline]
pub fn btn_bg_pressed(ui: &Ui) -> Color32 {
    sel_color(ui)
}

/// Get active + hover color (slightly lighter than active).
///
/// Used when a button is both selected (active) AND hovered.
/// Provides visual feedback that the selected item can be interacted with.
///
/// # Examples
///
/// ```ignore
/// let bg = if is_selected && response.hovered() {
///     theming::active_hover_color(ui)
/// } else if is_selected {
///     theming::sel_color(ui)
/// } else if response.hovered() {
///     theming::hover_color(ui)
/// } else {
///     theming::btn_bg_normal(ui)
/// };
/// ```
#[inline]
pub fn active_hover_color(ui: &Ui) -> Color32 {
    let active = ui.style().visuals.widgets.active.bg_fill;
    Color32::from_rgb(
        active.r().saturating_add(5),
        active.g().saturating_add(5),
        active.b().saturating_add(5),
    )
}

// =============================================================================
// ICON COLORS
// =============================================================================

/// Get default icon color from theme.
///
/// Used for icons in their normal/inactive state.
///
/// # Examples
///
/// ```ignore
/// let color = theming::icon_color(ui);
/// svg_icon.paint(painter, rect, color);
/// ```
#[inline]
pub fn icon_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.noninteractive.fg_stroke.color
}

/// Alias for `icon_color` - semantic name for normal state.
#[inline]
pub fn icon_normal(ui: &Ui) -> Color32 {
    icon_color(ui)
}

/// Get active/selected icon color from theme.
///
/// Used for icons in selected or pressed states.
#[inline]
pub fn active_icon_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.active.fg_stroke.color
}

/// Get icon hover color from theme.
///
/// Used for icons when their parent element is hovered.
#[inline]
pub fn icon_hover_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.hovered.fg_stroke.color
}

/// Alias for `icon_hover_color` - semantic name for active/hover state.
#[inline]
pub fn icon_active(ui: &Ui) -> Color32 {
    icon_hover_color(ui)
}

// =============================================================================
// TEXT COLORS
// =============================================================================

/// Get primary text color from theme.
///
/// # Examples
///
/// ```ignore
/// let color = theming::text_color(ui);
/// painter.text(pos, align, "Label", font, color);
/// ```
#[inline]
pub fn text_color(ui: &Ui) -> Color32 {
    ui.style().visuals.text_color()
}

/// Get secondary/muted text color from theme.
///
/// Used for less prominent text like hints, timestamps, etc.
#[inline]
pub fn muted_color(ui: &Ui) -> Color32 {
    ui.style().visuals.widgets.noninteractive.fg_stroke.color
}

// =============================================================================
// BRAND COLORS (Accent Blue)
// =============================================================================

/// Get publish button background (brand accent blue).
///
/// This is an intentionally branded color that doesn't change with theme.
#[inline]
pub fn publish_button_bg(_ui: &Ui) -> Color32 {
    DESIGN_TOKENS.semantic.extended.accent
}

/// Get publish button hover background.
#[inline]
pub fn publish_button_bg_hover(_ui: &Ui) -> Color32 {
    DESIGN_TOKENS.semantic.extended.accent_hover
}

/// Get publish button pressed background.
#[inline]
pub fn publish_button_bg_pressed(_ui: &Ui) -> Color32 {
    DESIGN_TOKENS.semantic.extended.accent_active
}

/// Get publish button text color (always white for contrast).
#[inline]
pub fn publish_button_text(_ui: &Ui) -> Color32 {
    Color32::WHITE
}

/// Get notification badge text color (always white for contrast on error bg).
#[inline]
pub fn badge_text_color(_ui: &Ui) -> Color32 {
    Color32::WHITE
}

// =============================================================================
// UTILITY COLORS
// =============================================================================

/// Get pill/badge background color.
///
/// Slightly lighter than toolbar background for visual separation.
#[inline]
pub fn pill_bg(ui: &Ui) -> Color32 {
    let base = ui.style().visuals.panel_fill;
    lighten_color(base, 0.1)
}

// =============================================================================
// COLOR UTILITIES
// =============================================================================

/// Lighten a color by a factor (0.0 = no change, 1.0 = white).
///
/// # Arguments
///
/// * `color` - The base color to lighten
/// * `factor` - How much to lighten (0.0-1.0)
///
/// # Examples
///
/// ```ignore
/// let lighter = theming::lighten_color(base, 0.1);  // 10% lighter
/// ```
#[inline]
pub fn lighten_color(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let r = (r as f32 + (255.0 - r as f32) * factor).min(255.0) as u8;
    let g = (g as f32 + (255.0 - g as f32) * factor).min(255.0) as u8;
    let b = (b as f32 + (255.0 - b as f32) * factor).min(255.0) as u8;
    Color32::from_rgba_premultiplied(r, g, b, a)
}

/// Darken a color by a factor (0.0 = no change, 1.0 = black).
///
/// # Arguments
///
/// * `color` - The base color to darken
/// * `factor` - How much to darken (0.0-1.0)
///
/// # Examples
///
/// ```ignore
/// let darker = theming::darken_color(base, 0.1);  // 10% darker
/// ```
#[inline]
pub fn darken_color(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let r = (r as f32 * (1.0 - factor)) as u8;
    let g = (g as f32 * (1.0 - factor)) as u8;
    let b = (b as f32 * (1.0 - factor)) as u8;
    Color32::from_rgba_premultiplied(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lighten_color() {
        let black = Color32::BLACK;
        let lightened = lighten_color(black, 0.5);
        // Black lightened 50% should be gray (127, 127, 127)
        assert_eq!(lightened.r(), 127);
        assert_eq!(lightened.g(), 127);
        assert_eq!(lightened.b(), 127);
    }

    #[test]
    fn test_darken_color() {
        let white = Color32::WHITE;
        let darkened = darken_color(white, 0.5);
        // White darkened 50% should be gray (127, 127, 127)
        assert_eq!(darkened.r(), 127);
        assert_eq!(darkened.g(), 127);
        assert_eq!(darkened.b(), 127);
    }

    #[test]
    fn test_lighten_preserves_alpha() {
        let semi_transparent = Color32::from_rgba_premultiplied(100, 100, 100, 128);
        let lightened = lighten_color(semi_transparent, 0.1);
        assert_eq!(lightened.a(), 128);
    }

    #[test]
    fn test_darken_preserves_alpha() {
        let semi_transparent = Color32::from_rgba_premultiplied(100, 100, 100, 128);
        let darkened = darken_color(semi_transparent, 0.1);
        assert_eq!(darkened.a(), 128);
    }
}
