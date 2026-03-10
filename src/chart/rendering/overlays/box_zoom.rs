//! Box zoom selection rendering

use crate::chart::state::BoxZoomState;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Renders box zoom selection rect (right-click drag)
pub fn render_box_zoom(painter: &Painter, box_zoom: &BoxZoomState) {
    if !box_zoom.active {
        return;
    }

    let (Some(start), Some(end)) = (box_zoom.start_pos, box_zoom.curr_pos) else {
        return;
    };

    let rect = Rect::from_two_pos(start, end);
    let border_color = Color32::from_rgba_premultiplied(
        DESIGN_TOKENS.semantic.extended.info.r(),
        DESIGN_TOKENS.semantic.extended.info.g(),
        DESIGN_TOKENS.semantic.extended.info.b(),
        220,
    );

    // Semi-transparent blue fill
    painter.rect_filled(
        rect,
        0.0,
        Color32::from_rgba_premultiplied(
            DESIGN_TOKENS.semantic.extended.info.r(),
            DESIGN_TOKENS.semantic.extended.info.g(),
            DESIGN_TOKENS.semantic.extended.info.b(),
            30,
        ),
    );

    // Blue stroke border (2px solid line)
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(DESIGN_TOKENS.stroke.thick, border_color),
        egui::epaint::StrokeKind::Inside,
    );

    render_corner_handles(painter, rect, border_color);
    render_dimension_labels(painter, rect);
}

fn render_corner_handles(painter: &Painter, rect: Rect, border_color: Color32) {
    let handle_size = 6.0;
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ];

    for corner in corners {
        let handle_rect = Rect::from_center_size(corner, Vec2::splat(handle_size));
        painter.rect_filled(handle_rect, 0.0, border_color);
        painter.rect_stroke(
            handle_rect,
            0.0,
            Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            ),
            egui::epaint::StrokeKind::Outside,
        );
    }
}

fn render_dimension_labels(painter: &Painter, rect: Rect) {
    let width = rect.width().abs();
    let height = rect.height().abs();

    if width <= 50.0 || height <= 30.0 {
        return;
    }

    let label_text = format!("{:.0}x{:.0}", width, height);
    let label_pos = Pos2::new(rect.center().x, rect.min.y - 15.0);

    // Draw label background
    let text_size = painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        &label_text,
        FontId::proportional(typography::SM),
        Color32::TRANSPARENT,
    );
    painter.rect_filled(
        text_size.expand(3.0),
        2.0,
        DESIGN_TOKENS.semantic.extended.chart_crosshair_label_bg,
    );

    // Draw label text
    painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        label_text,
        FontId::proportional(typography::SM),
        DESIGN_TOKENS.semantic.chart.crosshair_label_text,
    );
}
