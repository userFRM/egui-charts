//! Shared rendering utilities.

use egui::{Pos2, Stroke};

/// Draw a dashed line between two arbitrary points.
///
/// Works for any angle, including horizontal and vertical lines.
pub(crate) fn draw_dashed_line(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let total_len = (dx * dx + dy * dy).sqrt();
    if total_len < 1.0 {
        return;
    }

    let dir_x = dx / total_len;
    let dir_y = dy / total_len;
    let mut pos = 0.0;
    let mut drawing = true;

    while pos < total_len {
        let seg_len = if drawing { dash_len } else { gap_len };
        let next_pos = (pos + seg_len).min(total_len);

        if drawing {
            let p1 = Pos2::new(start.x + dir_x * pos, start.y + dir_y * pos);
            let p2 = Pos2::new(start.x + dir_x * next_pos, start.y + dir_y * next_pos);
            painter.line_segment([p1, p2], stroke);
        }

        pos = next_pos;
        drawing = !drawing;
    }
}
