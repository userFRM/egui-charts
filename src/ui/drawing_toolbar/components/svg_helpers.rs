//! SVG rendering helpers with theme-aware colors.
//!
//! Provides utilities for rendering SVG icons with proper
//! theme colors based on hover/active state.

use crate::icons::Icon;
use crate::theming;
use egui::{Rect, Ui, Vec2};

/// Render SVG icon centered in a rect with theme-aware colors.
///
/// This enforces square aspect ratio and proper theme colors.
/// The icon color automatically adjusts based on:
/// - Active state (brightest)
/// - Hover state (medium brightness)
/// - Normal state (default icon color)
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The embedded Icon to render
/// * `rect` - Target rect for the icon
/// * `is_hovered` - Whether the icon is being hovered
/// * `is_active` - Whether the icon is in active/selected state
pub fn render_svg_at_rect_themed(
    ui: &mut Ui,
    icon: &Icon,
    rect: Rect,
    is_hovered: bool,
    is_active: bool,
) {
    // Get theme-aware color
    let color = if is_active {
        theming::icon_active(ui)
    } else if is_hovered {
        theming::icon_hover_color(ui)
    } else {
        theming::icon_normal(ui)
    };

    // Render icon with PERFECT SQUARE aspect ratio
    let size = rect.size();
    let image = icon.as_image_tinted(size, color);

    // Place at exact position
    ui.put(rect, image);
}

/// Render SVG icon at a centered position with specified size.
///
/// Convenience wrapper that creates a centered rect from a position.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The embedded Icon to render
/// * `center` - Center position for the icon
/// * `size` - Icon size (square)
/// * `is_hovered` - Whether the icon is being hovered
/// * `is_active` - Whether the icon is in active/selected state
pub fn render_svg_centered(
    ui: &mut Ui,
    icon: &Icon,
    center: egui::Pos2,
    size: f32,
    is_hovered: bool,
    is_active: bool,
) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    render_svg_at_rect_themed(ui, icon, rect, is_hovered, is_active);
}
