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
/// If a drawing is already selected, its body and handles can be dragged. In eraser mode,
/// clicking on a drawing deletes it instead, whatever is selected, unless it is locked.
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

    // A handle drag follows the pointer, kept on the pane, until it is released anywhere.
    if drawing_manager.dragging_handle.is_some() {
        if (response.dragged() || response.drag_stopped())
            && let Some(pos) = response.interact_pointer_pos()
        {
            drawing_manager.update_drag_handle_with_coords(
                price_rect.clamp(pos),
                x_to_bar,
                y_to_price,
            );
        }
        if response.drag_stopped() {
            drawing_manager.end_drag_handle();
        }
        return;
    }

    if let Some(pos) = pointer_pos {
        if !price_rect.contains(pos) {
            cursor_modes.eraser.set_hover(None);
            return;
        }

        // Eraser mode: a click deletes the drawing under the pointer, selected or not.
        if drawing_manager.eraser_mode {
            // A locked drawing cannot be erased, so it is not offered.
            let hit = drawing_manager.hit_test(pos).filter(|&id| {
                drawing_manager
                    .drawings
                    .iter()
                    .any(|d| d.id == id && !d.locked)
            });
            cursor_modes.eraser.set_hover(hit);
            ui.ctx().set_cursor_icon(if hit.is_some() {
                egui::CursorIcon::NotAllowed
            } else {
                egui::CursorIcon::Default
            });
            if response.clicked()
                && let Some(hit_id) = hit
            {
                drawing_manager.delete_drawing(hit_id);
                cursor_modes.eraser.set_hover(None);
            }
            return;
        }
        cursor_modes.eraser.set_hover(None);

        if let Some(sel_id) = drawing_manager.sel_drawing() {
            if response.drag_started() {
                let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                if let Some(handle) = drawing_manager.hit_test_handle_by_id(origin, sel_id) {
                    drawing_manager.start_drag_handle(sel_id, handle);
                } else if drawing_manager.hit_test(origin) == Some(sel_id) {
                    drawing_manager.start_drag_body(sel_id, origin);
                }
                drawing_manager.update_drag_handle_with_coords(pos, x_to_bar, y_to_price);
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
            drawing_manager.select_at(pos);
        }
    } else {
        cursor_modes.eraser.set_hover(None);
    }
}

/// Drawing keys belong to this chart or to an otherwise unfocused canvas.
pub(super) fn keyboard_available(ui: &Ui, response: &Response) -> bool {
    !egui::Popup::is_any_open(ui.ctx())
        && ui.memory(|m| m.focused().is_none_or(|id| id == response.id))
}

/// Handle Delete/Backspace to remove selected drawing, and Escape to deselect/cancel.
pub fn handle_keyboard_shortcuts(
    ui: &Ui,
    response: &Response,
    drawing_manager: &mut DrawingManager,
) {
    // egui releases a text field's focus before delivering its Escape frame.
    let focus_id = response.id.with("drawing_keyboard_focus");
    let was_elsewhere = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(focus_id))
        .unwrap_or(false);
    let elsewhere = ui.memory(|m| m.focused().is_some_and(|id| id != response.id));
    ui.ctx().data_mut(|d| d.insert_temp(focus_id, elsewhere));
    if !keyboard_available(ui, response)
        || (was_elsewhere && ui.input(|i| i.key_pressed(egui::Key::Escape)))
    {
        return;
    }
    ui.input_mut(|i| {
        if (i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
            && let Some(sel_id) = drawing_manager.sel_drawing()
        {
            drawing_manager.delete_drawing(sel_id);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            drawing_manager.cancel_curr_drawing();
            drawing_manager.deselect();
            drawing_manager.active_tool = None;
            drawing_manager.set_eraser_mode(false);
        }
    });
}

/// Render a highlight outline around the drawing currently hovered by the eraser cursor.
pub fn render_eraser_highlight(
    painter: &Painter,
    drawing_manager: &DrawingManager,
    cursor_modes: &CursorModeState,
) {
    if !drawing_manager.eraser_mode {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::manager::DrawingSyncAction;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use egui::{Context, Event, Id, PointerButton, Pos2, RawInput, Sense};

    /// Editing a field must leave the selection and armed tool alone, while chart keys
    /// still delete and cancel. Drive the real widget so focus ownership is exercised.
    #[test]
    fn drawing_keys_respect_text_field_focus() {
        use crate::widget::test_frames::chart;
        use egui::{Key, Modifiers};

        for key in [Key::Backspace, Key::Delete, Key::Escape] {
            for field_focused in [true, false] {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                chart
                    .drawings_mut()
                    .drawings
                    .push(Drawing::new(7, DrawingToolType::TrendLine));
                chart.drawings_mut().select(7);
                chart
                    .drawings_mut()
                    .set_active_tool(Some(DrawingToolType::TrendLine));
                let mut text = "label".to_owned();
                for pass in 0..3 {
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(800.0, 600.0),
                            )),
                            events: if pass == 2 {
                                vec![Event::Key {
                                    key,
                                    physical_key: None,
                                    pressed: true,
                                    repeat: false,
                                    modifiers: Modifiers::NONE,
                                }]
                            } else {
                                vec![]
                            },
                            ..Default::default()
                        },
                        |ui| {
                            let field = ui.text_edit_singleline(&mut text);
                            let response = chart.show(ui);
                            if pass < 2 {
                                if field_focused {
                                    field.request_focus();
                                } else {
                                    response.request_focus();
                                }
                            }
                        },
                    )
                    .drop_without_applying_deltas();
                }
                let kept = field_focused || key == Key::Escape;
                assert_eq!(
                    chart.drawings().drawings.len(),
                    usize::from(kept),
                    "{key:?}, field {field_focused}"
                );
                if field_focused {
                    assert_eq!(chart.drawings().sel_drawing(), Some(7), "{key:?}");
                    assert_eq!(
                        chart.drawings().active_tool,
                        Some(DrawingToolType::TrendLine)
                    );
                    if key == Key::Backspace {
                        assert_eq!(text, "labe");
                    }
                } else if key == Key::Escape {
                    assert_eq!(chart.drawings().sel_drawing(), None);
                    assert_eq!(chart.drawings().active_tool, None);
                }
            }
        }
    }

    /// A body grab keeps its offset from the anchors, translates every price and bar by
    /// the pointer's data delta, and produces one history step and one host update.
    #[test]
    fn body_drags_translate_every_anchor_once_unless_locked() {
        for tool in [
            DrawingToolType::TrendLine,
            DrawingToolType::Rect,
            DrawingToolType::FibonacciChannel,
        ] {
            for locked in [false, true] {
                let ctx = Context::default();
                let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0));
                let mut manager = DrawingManager::new();
                let mut cursors = CursorModeState::default();
                let mut drawing = Drawing::new(7, tool);
                drawing.points = vec![Pos2::new(50.0, 100.0), Pos2::new(250.0, 100.0)];
                if tool == DrawingToolType::Rect {
                    drawing.points = vec![Pos2::new(50.0, 50.0), Pos2::new(250.0, 150.0)];
                }
                if tool == DrawingToolType::FibonacciChannel {
                    drawing.points.push(Pos2::new(150.0, 200.0));
                }
                let price = |y: f32| 1000.0 * (-(y as f64) / 100.0).exp();
                drawing.chart_points = drawing
                    .points
                    .iter()
                    .map(|p| ChartPoint::new(p.x / 10.0, price(p.y)))
                    .collect();
                drawing.update_screen_coords(|b| b * 10.0, |p| (-100.0 * (p / 1000.0).ln()) as f32);
                drawing.locked = locked;
                let before = drawing.chart_points.clone();
                manager.drawings.push(drawing);
                manager.select(7);
                let from = Pos2::new(100.0, 100.0);
                let to = Pos2::new(140.0, 130.0);
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                for events in [
                    vec![Event::PointerMoved(from)],
                    vec![button(from, true)],
                    vec![Event::PointerMoved(Pos2::new(110.0, 105.0))],
                    vec![Event::PointerMoved(to)],
                    vec![button(to, false)],
                ] {
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(pane),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let response =
                                ui.interact(pane, Id::NULL.with("chart"), Sense::click_and_drag());
                            handle_selection(
                                ui,
                                &mut manager,
                                &mut cursors,
                                &response,
                                pane,
                                &|x| x / 10.0,
                                &price,
                            );
                        },
                    )
                    .drop_without_applying_deltas();
                }
                for (old, moved) in before.iter().zip(&manager.drawings[0].chart_points) {
                    assert!(
                        (moved.bar_idx - old.bar_idx - if locked { 0.0 } else { 4.0 }).abs() < 1e-5,
                        "{tool:?}, locked {locked}: {moved:?}"
                    );
                    let delta = if locked {
                        0.0
                    } else {
                        price(to.y) - price(from.y)
                    };
                    assert!(
                        (moved.price - old.price - delta).abs() < 1e-8,
                        "{tool:?}, locked {locked}: {moved:?}"
                    );
                }
                let updates = manager.pending_sync.lock().unwrap();
                assert_eq!(updates.len(), usize::from(!locked), "{tool:?}");
                if !locked {
                    assert!(matches!(updates[0], DrawingSyncAction::UpdateDrawing(7)));
                }
                drop(updates);
                assert_eq!(manager.undo(), !locked, "{tool:?}");
                assert_eq!(manager.drawings[0].chart_points, before);
                assert!(!manager.undo());
            }
        }
    }
}
