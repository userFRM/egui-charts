//! SVG rendering helpers with theme-aware colors.
//!
//! Provides utilities for rendering SVG icons with proper
//! theme colors based on hover/active state.

use crate::icons::{Icon, icons};
use crate::theme::ThemeExt;
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
    let color = if is_active || is_hovered {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.noninteractive.fg_stroke.color
    };

    // `paint_at` leaves the layout cursor alone; `put` would move it back to the icon's bottom edge.
    icon.as_image_tinted(rect.size(), color).paint_at(ui, rect);
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

/// Match visible rail ink rather than the transparent padding of each SVG viewport.
pub(super) fn rail_icon_size(icon: &Icon) -> Vec2 {
    if [icons::CROSSHAIR, icons::TREND_LINE].contains(icon) {
        Vec2::splat(29.0)
    } else if icon == &icons::FIB_RETRACEMENT {
        Vec2::splat(27.0)
    } else if [
        icons::ARROW,
        icons::XABCD_PATTERN,
        icons::LONG_POSITION,
        icons::MEASURE,
        icons::ZOOM_IN,
        icons::ZOOM_OUT,
        icons::DRAWING_KEEP,
        icons::DRAWING_MAGNET,
    ]
    .contains(icon)
    {
        Vec2::splat(28.0)
    } else if icon == &icons::BRUSH {
        Vec2::splat(31.0)
    } else if icon == &icons::EMOJI_ICON {
        Vec2::splat(25.0)
    } else if [
        icons::LOCK,
        icons::UNLOCK,
        icons::EYE,
        icons::STAR_EMPTY,
        icons::TRASH,
    ]
    .contains(icon)
    {
        Vec2::splat(26.0)
    } else {
        Vec2::splat(crate::tokens::DESIGN_TOKENS.sizing.toolbar.icon_size)
    }
}

/// Default light rail ink is black; custom theme colours and disabled contrast stay intact.
pub(super) fn rail_icon_color(ui: &Ui) -> egui::Color32 {
    let theme = ui.active_theme();
    if !ui.is_enabled() {
        theme.ui.text_disabled
    } else if !theme.dark && theme.ui.text == egui::Color32::from_gray(26) {
        egui::Color32::BLACK
    } else {
        theme.ui.text
    }
}
