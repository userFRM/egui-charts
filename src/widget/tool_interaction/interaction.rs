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

        // A drag starts on the pane; once started it follows the pointer, kept on the pane, and
        // its release finishes the drawing wherever the pointer is.
        DrawingInteractionMode::DragToDraw => {
            if response.drag_started() && drawing_manager.curr_drawing.is_none() {
                if let Some(pos) = pointer_pos
                    && price_rect.contains(pos)
                {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        x_to_bar,
                        y_to_price,
                    );
                }
            } else if (response.dragged() || response.drag_stopped())
                && drawing_manager.curr_drawing.is_some()
            {
                if let Some(pos) = pointer_pos {
                    drawing_manager.update_last_point_with_coords(
                        price_rect.clamp(pos),
                        x_to_bar,
                        y_to_price,
                    );
                }
                if response.drag_stopped() {
                    drawing_manager.complete_drag_drawing();
                }
            }
        }

        DrawingInteractionMode::ContinuousDraw => {
            if response.drag_started() {
                if let Some(pos) = pointer_pos
                    && price_rect.contains(pos)
                {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        x_to_bar,
                        y_to_price,
                    );
                }
            } else if response.dragged() && drawing_manager.curr_drawing.is_some() {
                if let Some(pos) = pointer_pos {
                    drawing_manager.add_point_with_coords(
                        price_rect.clamp(pos),
                        x_to_bar,
                        y_to_price,
                    );
                }
            } else if response.drag_stopped() && drawing_manager.curr_drawing.is_some() {
                drawing_manager.complete_curr_drawing();
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
    if super::selection::keyboard_available(ui, response)
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        && matches!(interaction_mode, DrawingInteractionMode::MultiPoint)
        && drawing_manager.curr_drawing.is_some()
    {
        drawing_manager.complete_curr_drawing();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::ChartPoint;
    use egui::{Context, Event, Id, PointerButton, Pos2, RawInput, Sense};

    /// A drag that ends below the price pane, over the time axis, finishes the drawing with
    /// its last anchor on the pane's edge.
    #[test]
    fn a_drag_released_outside_the_pane_finishes_the_drawing() {
        let ctx = Context::default();
        let chart = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let price_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let mut manager = DrawingManager::new();
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frames = [
            vec![Event::PointerMoved(Pos2::new(50.0, 50.0))],
            vec![button(Pos2::new(50.0, 50.0), true)],
            vec![Event::PointerMoved(Pos2::new(100.0, 100.0))],
            vec![Event::PointerMoved(Pos2::new(150.0, 350.0))],
            vec![button(Pos2::new(150.0, 350.0), false)],
        ];
        for events in frames {
            let input = RawInput {
                screen_rect: Some(chart),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                let response = ui.interact(chart, Id::NULL.with("chart"), Sense::click_and_drag());
                handle_active_tool(
                    ui,
                    &mut manager,
                    &response,
                    price_rect,
                    &|x| x,
                    &|y| f64::from(y),
                    DrawingToolType::Rect,
                );
            })
            .drop_without_applying_deltas();
        }
        assert!(manager.curr_drawing.is_none());
        assert_eq!(manager.drawings.len(), 1);
        assert_eq!(
            manager.drawings[0].chart_points.last(),
            Some(&ChartPoint::new(150.0, 300.0))
        );
    }
}
