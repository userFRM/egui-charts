//! Separator component for visual dividers in the toolbar.

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Rect, Sense, Ui, Vec2};

/// Draw a horizontal separator line.
///
/// Creates a thin horizontal line to visually separate toolbar sections.
/// Uses theme-aware coloring.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `width` - Total width of the separator area
pub fn draw_separator(ui: &mut Ui, width: f32) {
    let separator_height = 1.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, separator_height), Sense::hover());
    ui.painter().rect_filled(
        rect,
        0.0,
        ui.visuals().widgets.noninteractive.bg_stroke.color,
    );
}

/// Draw a separator with horizontal margins.
///
/// Same as `draw_separator` but with configurable left/right margins.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `width` - Total width of the separator area
/// * `margin` - Horizontal margin on each side
pub fn draw_separator_with_margin(ui: &mut Ui, width: f32, margin: f32) {
    let sep_rect = Rect::from_min_size(
        ui.cursor().min + Vec2::new(margin, 0.0),
        Vec2::new(width - margin * 2.0, 1.0),
    );
    ui.painter().rect_filled(
        sep_rect,
        0.0,
        ui.visuals().widgets.noninteractive.bg_stroke.color,
    );
    ui.add_space(DESIGN_TOKENS.stroke.hairline);
}

/// Left inset of the drawing toolbar's group separators, in logical points.
const LEFT_INSET: f32 = 9.0;
/// Right inset of the drawing toolbar's group separators, in logical points.
const RIGHT_INSET: f32 = 10.0;

/// Draw a styled separator for the drawing toolbar.
///
/// Creates a thin separator line with proper margins and a `spacing.lg` gap above and below.
/// At the default width it is 33px wide and 1px high, inset 9px left and 10px right.
///
/// # Arguments
///
/// * `ui` - The egui UI context
/// * `toolbar_width` - Total width of the toolbar (e.g., 52px)
pub fn draw_separator_styled(ui: &mut Ui, toolbar_width: f32) {
    let gap = ui.active_theme().spacing.lg;

    // Add gap above
    ui.add_space(gap);

    // Draw 1px separator with margins
    let sep_width = toolbar_width - LEFT_INSET - RIGHT_INSET;
    let sep_rect = Rect::from_min_size(
        ui.cursor().min + Vec2::new(LEFT_INSET, 0.0),
        Vec2::new(sep_width, 1.0),
    );
    let color = if ui.active_theme().dark {
        Color32::from_gray(74)
    } else {
        Color32::from_gray(235)
    };
    ui.painter().rect_filled(sep_rect, 0.0, color);

    // Allocate space for the separator line
    ui.add_space(1.0);

    // Add gap below
    ui.add_space(gap);
}
