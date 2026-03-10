//! Icon button components with theme-aware styling.
//!
//! Single icon btns used throughout the toolbar for tools,
//! toggles, and actions.

use crate::icons::Icon;
use crate::theming;
use egui::{Color32, Response, Sense, Ui, Vec2};

use super::svg_helpers::render_svg_at_rect_themed;
use crate::tokens::DESIGN_TOKENS;

/// Single icon button with theme-aware rendering.
///
/// Draws a square button with:
/// - Hover background
/// - Active/selected background
/// - Centered SVG icon with theme colors
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The Icon to display
/// * `tooltip` - Hover tooltip text
/// * `is_active` - Whether button is in active/selected state
/// * `size` - Button size (square)
///
/// # Returns
///
/// The egui Response for click handling.
pub fn draw_icon_btn(
    ui: &mut Ui,
    icon: &Icon,
    tooltip: &'static str,
    is_active: bool,
    size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());

    let bg_color = if is_active && response.hovered() {
        theming::active_hover_color(ui)
    } else if is_active {
        theming::sel_color(ui)
    } else if response.hovered() {
        theming::hover_color(ui)
    } else {
        Color32::TRANSPARENT // Let parent Frame's background show through
    };

    ui.painter()
        .rect_filled(rect, DESIGN_TOKENS.rounding.button, bg_color);

    render_svg_at_rect_themed(ui, icon, rect, response.hovered(), is_active);

    response.on_hover_text(tooltip)
}

/// Toggle button with icon (alias for draw_icon_btn since functionally identical).
///
/// Used for toggleable states like magnet mode, keep drawing, lock, etc.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The Icon to display
/// * `tooltip` - Hover tooltip text
/// * `is_toggled` - Whether toggle is on
/// * `size` - Button size (square)
///
/// # Returns
///
/// The egui Response for click handling.
pub fn draw_toggle_btn(
    ui: &mut Ui,
    icon: &Icon,
    tooltip: &'static str,
    is_toggled: bool,
    size: f32,
) -> Response {
    draw_icon_btn(ui, icon, tooltip, is_toggled, size)
}

/// Icon button with left padding.
///
/// Layout: [left_padding | icon_btn]
/// Used for tool btns that need consistent alignment with PairButton.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The Icon to display
/// * `tooltip` - Hover tooltip text
/// * `is_active` - Whether button is in active/selected state
/// * `icon_size` - Icon size
/// * `padding` - Left padding amount
///
/// # Returns
///
/// The egui Response for click handling.
pub fn draw_icon_button_padded(
    ui: &mut Ui,
    icon: &Icon,
    tooltip: &str,
    is_active: bool,
    icon_size: f32,
    padding: f32,
) -> Response {
    let left_padding = padding;

    // Full rect for allocation
    let size = Vec2::splat(icon_size + padding * 2.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    // Add pressed state for click feedback
    let bg_color = if is_active {
        theming::sel_color(ui)
    } else if response.is_pointer_button_down_on() {
        theming::active_hover_color(ui)
    } else if response.hovered() {
        theming::hover_color(ui)
    } else {
        Color32::TRANSPARENT // Let parent Frame's background show through
    };

    // Left padding is transparent - parent Frame provides background
    let _left_padding_rect =
        egui::Rect::from_min_size(rect.min, Vec2::new(left_padding, rect.height()));

    // Draw icon button - SQUARE, centered in button
    let icon_button_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
    ui.painter()
        .rect_filled(icon_button_rect, DESIGN_TOKENS.rounding.button, bg_color);

    // Icon perfectly centered in full width
    let icon_center_x = rect.min.x + rect.width() / 2.0;
    let icon_rect = egui::Rect::from_center_size(
        egui::Pos2::new(icon_center_x, rect.center().y),
        Vec2::splat(icon_size),
    );

    let is_hovered = response.hovered() || response.is_pointer_button_down_on();
    render_svg_at_rect_themed(ui, icon, icon_rect, is_hovered, is_active);

    response.on_hover_text(tooltip)
}

/// Left toolbar button (no dropdown arrow).
///
/// Layout:
/// - Outer rect: 52×38px
/// - Inner hover rect: 9px horizontal margin, 2px vertical margin
/// - Inner rounding: 6px
/// - Icon: 28×28 centered in outer rect
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The Icon to display
/// * `tooltip` - Hover tooltip text
/// * `is_selected` - Whether button is selected/active
///
/// # Returns
///
/// The egui Response for click handling.
pub fn draw_tool_button(ui: &mut Ui, icon: &Icon, tooltip: &str, is_selected: bool) -> Response {
    let tokens = &DESIGN_TOKENS.sizing.toolbar;
    let btn_width = tokens.button_width; // 52px
    let btn_height = tokens.button_height; // 38px
    let margin_h = tokens.hover_margin_h; // 9px
    let margin_v = tokens.hover_margin_v; // 2px
    let inner_rounding = tokens.inner_rounding; // 6px
    let icon_size = tokens.icon_size; // 28px

    let (rect, response) = ui.allocate_exact_size(Vec2::new(btn_width, btn_height), Sense::click());

    if ui.is_rect_visible(rect) {
        // Inner hover rect with margins (9px horizontal, 2px vertical)
        let inner_rect = egui::Rect::from_min_max(
            egui::Pos2::new(rect.min.x + margin_h, rect.min.y + margin_v),
            egui::Pos2::new(rect.max.x - margin_h, rect.max.y - margin_v),
        );

        // Background only on hover or selected
        if is_selected || response.hovered() {
            let bg = if is_selected && response.hovered() {
                theming::active_hover_color(ui)
            } else if is_selected {
                theming::sel_color(ui)
            } else {
                theming::hover_color(ui)
            };
            ui.painter().rect_filled(inner_rect, inner_rounding, bg);
        }

        // Icon centered in OUTER rect
        let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
        render_svg_at_rect_themed(ui, icon, icon_rect, response.hovered(), is_selected);
    }

    response.on_hover_text(tooltip)
}
