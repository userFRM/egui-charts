//! Box zoom selection rendering

use crate::chart::state::BoxZoomState;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Renders the selection rectangle while dragging in box zoom mode.
pub fn render_box_zoom(painter: &Painter, box_zoom: &BoxZoomState) {
    if !box_zoom.active {
        return;
    }

    let (Some(start), Some(end)) = (box_zoom.start_pos, box_zoom.curr_pos) else {
        return;
    };

    let rect = Rect::from_two_pos(start, end);
    let info = painter.ctx().active_theme().ui.info;
    let border_color = Color32::from_rgba_unmultiplied(info.r(), info.g(), info.b(), 220);

    painter.rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(info.r(), info.g(), info.b(), 30),
    );

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
                painter.ctx().active_theme().chart.crosshair_label_text,
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

    let label_text = format!("{width:.0}x{height:.0}");
    let label_pos = Pos2::new(rect.center().x, rect.min.y - 15.0);
    let theme = painter.ctx().active_theme();

    // Draw label background
    let text_size = painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        &label_text,
        FontId::proportional(theme.text.small),
        Color32::TRANSPARENT,
    );
    painter.rect_filled(text_size.expand(3.0), 2.0, theme.chart.crosshair_label_bg);

    // Draw label text
    painter.text(
        label_pos,
        egui::Align2::CENTER_CENTER,
        label_text,
        FontId::proportional(theme.text.small),
        theme.chart.crosshair_label_text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{apply_to_egui, presets};

    #[test]
    fn box_zoom_uses_the_active_info_colour_with_translucent_fill() {
        let ctx = egui::Context::default();
        let start = Pos2::new(20.0, 20.0);
        let end = Pos2::new(100.0, 100.0);
        for (name, theme) in presets() {
            apply_to_egui(&ctx, &theme);
            let output = ctx.run_ui(Default::default(), |ui| {
                render_box_zoom(
                    ui.painter(),
                    &BoxZoomState {
                        active: true,
                        start_pos: Some(start),
                        curr_pos: Some(end),
                    },
                );
            });
            let rects: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect == Rect::from_two_pos(start, end) => {
                        Some(rect.clone())
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            let info = theme.ui.info;
            assert_eq!(
                rects[0].fill,
                Color32::from_rgba_unmultiplied(info.r(), info.g(), info.b(), 30),
                "{name}: fill"
            );
            assert_eq!(
                rects[1].stroke.color,
                Color32::from_rgba_unmultiplied(info.r(), info.g(), info.b(), 220),
                "{name}: border"
            );
        }
    }
}
