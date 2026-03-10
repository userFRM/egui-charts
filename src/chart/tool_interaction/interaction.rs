//! Active drawing tool interaction dispatch.
//!
//! Routes pointer events to the appropriate handler based on the active tool's
//! [`DrawingInteractionMode`]:
//!
//! | Mode | Gesture |
//! |------|---------|
//! | `SingleClick` | One click creates a complete drawing (e.g., marker) |
//! | `ClickClick` | First click starts, second click completes (e.g., trend line) |
//! | `DragToDraw` | Click-drag-release creates the drawing (e.g., rectangle) |
//! | `ContinuousDraw` | Freehand drawing while dragging (e.g., brush, highlighter) |
//! | `MultiPoint` | Multiple clicks, double-click or Enter to complete (e.g., path) |
//!
//! Also handles Escape to cancel and Enter to complete multi-point drawings.

use crate::drawings::{DrawingInteractionMode, DrawingManager, DrawingToolType};
use egui::{Rect, Response, Ui};

/// Dispatch pointer events to the active drawing tool based on its interaction mode.
///
/// This function is the single entry point for all drawing tool input handling.
/// It reads pointer state from the egui `Response` and drives the
/// [`DrawingManager`]'s state machine (start, update, add point, complete, cancel).
///
/// # Arguments
///
/// * `x_to_bar` - Converts screen X to bar index (snapped or precise depending on tool)
/// * `y_to_price` - Converts screen Y to price value
/// * `active_tool` - The currently selected drawing tool type
pub fn handle_active_tool<F1, F2>(
    ui: &Ui,
    drawing_manager: &mut DrawingManager,
    response: &Response,
    price_rect: Rect,
    x_to_bar: &F1,
    y_to_price: &F2,
    active_tool: DrawingToolType,
) where
    F1: Fn(f32) -> f32,
    F2: Fn(f32) -> f64,
{
    let interaction_mode = active_tool.interaction_mode();
    let pointer_pos = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos());

    match interaction_mode {
        DrawingInteractionMode::NotDrawing => {}

        DrawingInteractionMode::SingleClick => {
            if response.clicked()
                && let Some(pos) = pointer_pos
                && price_rect.contains(pos)
            {
                drawing_manager.start_drawing_with_coords(active_tool, pos, x_to_bar, y_to_price);
            }
        }

        DrawingInteractionMode::ClickClick => {
            if let Some(pos) = pointer_pos
                && price_rect.contains(pos)
            {
                if response.clicked() {
                    if drawing_manager.curr_drawing.is_none() {
                        drawing_manager.start_drawing_with_coords(
                            active_tool,
                            pos,
                            x_to_bar,
                            y_to_price,
                        );
                    } else {
                        drawing_manager.add_point_with_coords(pos, x_to_bar, y_to_price);
                    }
                } else if drawing_manager.curr_drawing.is_some() {
                    drawing_manager.update_last_point_with_coords(pos, x_to_bar, y_to_price);
                }
            }
        }

        DrawingInteractionMode::DragToDraw => {
            if let Some(pos) = pointer_pos
                && price_rect.contains(pos)
            {
                if response.drag_started() && drawing_manager.curr_drawing.is_none() {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        x_to_bar,
                        y_to_price,
                    );
                } else if response.dragged() && drawing_manager.curr_drawing.is_some() {
                    drawing_manager.update_last_point_with_coords(pos, x_to_bar, y_to_price);
                } else if response.drag_stopped() && drawing_manager.curr_drawing.is_some() {
                    drawing_manager.update_last_point_with_coords(pos, x_to_bar, y_to_price);
                    drawing_manager.complete_drag_drawing();
                }
            }
        }

        DrawingInteractionMode::ContinuousDraw => {
            if let Some(pos) = pointer_pos
                && price_rect.contains(pos)
            {
                if response.drag_started() {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        x_to_bar,
                        y_to_price,
                    );
                } else if response.dragged() && drawing_manager.curr_drawing.is_some() {
                    drawing_manager.add_point_with_coords(pos, x_to_bar, y_to_price);
                } else if response.drag_stopped() && drawing_manager.curr_drawing.is_some() {
                    drawing_manager.complete_curr_drawing();
                }
            }
        }

        DrawingInteractionMode::MultiPoint => {
            if let Some(pos) = pointer_pos
                && price_rect.contains(pos)
            {
                if response.double_clicked() && drawing_manager.curr_drawing.is_some() {
                    drawing_manager.complete_curr_drawing();
                } else if response.clicked() {
                    if drawing_manager.curr_drawing.is_none() {
                        drawing_manager.start_drawing_with_coords(
                            active_tool,
                            pos,
                            x_to_bar,
                            y_to_price,
                        );
                    } else {
                        drawing_manager.add_point_with_coords(pos, x_to_bar, y_to_price);
                    }
                } else if drawing_manager.curr_drawing.is_some() {
                    drawing_manager.update_last_point_with_coords(pos, x_to_bar, y_to_price);
                }
            }
        }
    }

    // Handle Enter to complete multi-point drawings
    if ui.input(|i| i.key_pressed(egui::Key::Enter))
        && matches!(interaction_mode, DrawingInteractionMode::MultiPoint)
        && drawing_manager.curr_drawing.is_some()
    {
        drawing_manager.complete_curr_drawing();
    }

    // Handle Escape to cancel current drawing
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && drawing_manager.curr_drawing.is_some() {
        drawing_manager.cancel_curr_drawing();
    }

    // Request repaint when drawing for live preview
    if drawing_manager.curr_drawing.is_some() {
        ui.ctx().request_repaint();
    }

    // Set crosshair cursor for drag-to-draw tools
    if matches!(
        interaction_mode,
        DrawingInteractionMode::DragToDraw | DrawingInteractionMode::ContinuousDraw
    ) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
}
