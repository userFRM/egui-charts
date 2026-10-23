//! Icon button components with theme-aware styling.
//!
//! Single icon btns used throughout the toolbar for tools,
//! toggles, and actions.

use crate::icons::Icon;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use egui::{
    Color32, Rect, RectAlign, Response, Role, Sense, Stroke, StrokeKind, Tooltip, Ui, Vec2,
    WidgetInfo,
};

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
    crate::styles::focus::draw_focus_ring(ui, &response);

    let bg_color = if is_active && response.hovered() {
        super::active_hover_color(ui)
    } else if is_active {
        ui.visuals().widgets.active.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
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
    crate::styles::focus::draw_focus_ring(ui, &response);

    // Add pressed state for click feedback
    let bg_color = if is_active {
        ui.visuals().widgets.active.bg_fill
    } else if response.is_pointer_button_down_on() {
        super::active_hover_color(ui)
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
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
/// - Icon: 20×20 centered in outer rect
///
/// A selected button is filled with the accent colour and its icon uses the accent text colour.
/// The tooltip opens to the right of the bar, so it never covers the next button.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `icon` - The Icon to display
/// * `tooltip` - Hover tooltip text, also the accessible name
/// * `is_selected` - Whether button is selected/active
///
/// # Returns
///
/// The egui Response for click handling.
pub fn draw_tool_button(ui: &mut Ui, icon: &Icon, tooltip: &str, is_selected: bool) -> Response {
    let tokens = &DESIGN_TOKENS.sizing.toolbar;
    let margin = Vec2::new(tokens.hover_margin_h, tokens.hover_margin_v);

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(tokens.button_width, tokens.button_height),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, is_selected, tooltip));

    if ui.is_rect_visible(rect) {
        let theme = ui.active_theme();
        let inner_rect = rect.shrink2(margin);

        // Background only on hover or selected
        let bg = match (is_selected, response.hovered()) {
            (true, true) => Some(theme.ui.accent_hover),
            (true, false) => Some(theme.ui.accent),
            (false, true) => Some(ui.visuals().widgets.hovered.bg_fill),
            (false, false) => None,
        };
        if let Some(bg) = bg {
            ui.painter()
                .rect_filled(inner_rect, tokens.inner_rounding, bg);
        }
        // The inner rect keeps all four sides of the ring inside the bar.
        // On the accent fill the ring takes the accent text colour and sits inside the fill, so
        // it shows whatever the bar's colour is.
        if response.has_focus() {
            let (ring, ring_rect) = if is_selected {
                (
                    theme.ui.accent_text,
                    inner_rect.shrink(DESIGN_TOKENS.stroke.thick),
                )
            } else {
                (crate::styles::focus::focus_ring_color(ui), inner_rect)
            };
            ui.painter().rect_stroke(
                ring_rect,
                tokens.inner_rounding,
                Stroke::new(DESIGN_TOKENS.stroke.thick, ring),
                StrokeKind::Inside,
            );
        }

        // Icon centered in OUTER rect
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_sizes::MD));
        if is_selected {
            icon.as_image_tinted(icon_rect.size(), theme.ui.accent_text)
                .paint_at(ui, icon_rect);
        } else {
            render_svg_at_rect_themed(ui, icon, icon_rect, response.hovered(), false);
        }
    }

    let mut tip = Tooltip::for_enabled(&response);
    tip.popup = tip.popup.align(RectAlign::RIGHT);
    tip.show(|ui| ui.label(tooltip));
    response
}
