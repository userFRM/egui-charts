//! Drawing selection and manipulation
//!
//! Handles:
//! - Drawing selection via click
//! - Handle dragging for drawing manipulation
//! - Eraser mode (click to delete)
//! - Keyboard shortcuts (Delete, Escape)

use crate::chart::cursor_modes::CursorModeState;
use crate::drawings::DrawingManager;
use egui::{Painter, Rect, Response, Stroke, Ui, epaint::StrokeKind};

/// Handle drawing selection, handle dragging, and eraser-mode deletion.
///
/// When no drawing tool is active, clicks are interpreted as selection attempts.
/// If a drawing is already selected, its handles can be dragged. In eraser mode,
/// clicking on a drawing deletes it.
pub fn handle_selection<F1, F2>(
    ui: &Ui,
    drawing_manager: &mut DrawingManager,
    cursor_modes: &mut CursorModeState,
    response: &Response,
    price_rect: Rect,
    x_to_bar: &F1,
    y_to_price: &F2,
) where
    F1: Fn(f32) -> f32,
    F2: Fn(f32) -> f64,
{
    let pointer_pos = if response.clicked() || response.drag_started() || response.dragged() {
        response.interact_pointer_pos()
    } else {
        response.hover_pos()
    };

    if let Some(pos) = pointer_pos {
        if !price_rect.contains(pos) {
            cursor_modes.eraser.set_hover(None);
            return;
        }

        // Handle eraser mode hover preview
        if cursor_modes.eraser.active {
            let hit = drawing_manager.hit_test(pos);
            cursor_modes.eraser.set_hover(hit);

            if hit.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
            }
        } else {
            cursor_modes.eraser.set_hover(None);
        }

        // Handle dragging
        if let Some((_drawing_id, _handle)) = drawing_manager.dragging_handle {
            if response.dragged() {
                drawing_manager.update_drag_handle_with_coords(pos, x_to_bar, y_to_price);
            } else if response.drag_stopped() {
                drawing_manager.end_drag_handle();
            }
        } else if let Some(sel_id) = drawing_manager.sel_drawing() {
            if response.drag_started() {
                if let Some(handle) = drawing_manager.hit_test_handle_by_id(pos, sel_id) {
                    drawing_manager.start_drag_handle(sel_id, handle);
                }
            } else if response.clicked() {
                if drawing_manager.hit_test(pos) == Some(sel_id) {
                    if drawing_manager.hit_test_handle_by_id(pos, sel_id).is_none() {
                        drawing_manager.deselect();
                    }
                } else {
                    drawing_manager.select_at(pos);
                }
            }
        } else if response.clicked() {
            // Eraser mode: delete drawing on click
            if cursor_modes.eraser.active {
                if let Some(hit_id) = drawing_manager.hit_test(pos) {
                    log::info!("Eraser: deleting drawing {}", hit_id);
                    drawing_manager.delete_drawing(hit_id);
                    cursor_modes.eraser.set_hover(None);
                }
            } else {
                drawing_manager.select_at(pos);
            }
        }
    } else {
        cursor_modes.eraser.set_hover(None);
    }
}

/// Handle Delete/Backspace to remove selected drawing, and Escape to deselect/cancel.
pub fn handle_keyboard_shortcuts(ui: &Ui, drawing_manager: &mut DrawingManager) {
    ui.input(|i| {
        if (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && let Some(sel_id) = drawing_manager.sel_drawing()
        {
            drawing_manager.delete_drawing(sel_id);
        }
        if i.key_pressed(egui::Key::Escape) {
            drawing_manager.deselect();
            drawing_manager.active_tool = None;
        }
    });
}

/// Render a highlight outline around the drawing currently hovered by the eraser cursor.
pub fn render_eraser_highlight(
    painter: &Painter,
    drawing_manager: &DrawingManager,
    cursor_modes: &CursorModeState,
) {
    if !cursor_modes.eraser.active {
        return;
    }

    if let Some(hover_id) = cursor_modes.eraser.hover_drawing
        && let Some(drawing) = drawing_manager.drawings.iter().find(|d| d.id == hover_id)
    {
        let eraser_color = cursor_modes.eraser.highlight_color();
        let stroke = Stroke::new(cursor_modes.eraser.highlight_stroke(), eraser_color);

        if drawing.points.len() >= 2 {
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;

            for point in &drawing.points {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }

            let padding = 6.0;
            let rect = egui::Rect::from_min_max(
                egui::Pos2::new(min_x - padding, min_y - padding),
                egui::Pos2::new(max_x + padding, max_y + padding),
            );

            painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Outside);

            for i in 0..(drawing.points.len() - 1) {
                painter.line_segment([drawing.points[i], drawing.points[i + 1]], stroke);
            }
        } else if drawing.points.len() == 1 {
            painter.circle_stroke(drawing.points[0], 16.0, stroke);
        }
    }
}
