//! Active drawing tool interaction dispatch.
//!
//! Routes pointer events to the appropriate handler based on the active tool's
//! [`DrawingInteractionMode`]:
//!
//! | Mode | Gesture |
//! |------|---------|
//! | `SingleClick` | One click creates a complete drawing (e.g., marker) |
//! | `ClickClick` | Two clicks or a drag create the drawing (e.g., trend line) |
//! | `DragToDraw` | Two clicks or a drag create the drawing (e.g., rectangle) |
//! | `ContinuousDraw` | Freehand drawing while dragging (e.g., brush, highlighter) |
//! | `MultiPoint` | Multiple clicks, double-click or Enter to complete (e.g., path) |
//!
//! Keyboard completion and cancellation use the shared chart dispatcher.

use crate::drawings::{DrawingInteractionMode, DrawingManager, DrawingToolType};
use egui::{Rect, Response, Ui};

// Input batches can already include movement after the press.
pub(super) fn primary_press_pos(ui: &Ui) -> Option<egui::Pos2> {
    ui.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } => Some(*pos),
            _ => None,
        })
    })
}

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
    drawing_manager.interaction_modifiers = ui.input(|i| i.modifiers);
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

        DrawingInteractionMode::ClickClick | DrawingInteractionMode::DragToDraw => {
            // Capture the press before the drag threshold, while snapping targets are still
            // gathered around that position. The first release must not place a second anchor.
            let first_press = response.id.with("drawing_first_press");
            let pressed = ui.input(|i| i.pointer.primary_pressed());
            let released = ui.input(|i| i.pointer.primary_released());
            let starting = drawing_manager.curr_drawing.is_none()
                && (response.clicked() || (pressed && response.is_pointer_button_down_on()));
            if starting {
                if let Some(pos) = primary_press_pos(ui)
                    .or(pointer_pos)
                    .filter(|p| price_rect.contains(*p))
                {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        |x| x_to_bar(x).round(),
                        y_to_price,
                    );
                    ui.ctx().data_mut(|d| d.insert_temp(first_press, true));
                }
            } else if drawing_manager.curr_drawing.is_some()
                && let Some(pos) = pointer_pos
                && (price_rect.contains(pos) || response.dragged() || response.drag_stopped())
            {
                drawing_manager.update_last_point_with_coords(
                    price_rect.clamp(pos),
                    x_to_bar,
                    y_to_price,
                );
                if matches!(
                    active_tool,
                    DrawingToolType::Rect | DrawingToolType::Ellipse
                ) && ui.input(|i| i.modifiers.shift)
                    && let Some(drawing) = &mut drawing_manager.curr_drawing
                    && let [anchor, end] = drawing.points.as_slice()
                {
                    let delta = *end - *anchor;
                    let direction = egui::vec2(
                        if delta.x < 0.0 { -1.0 } else { 1.0 },
                        if delta.y < 0.0 { -1.0 } else { 1.0 },
                    );
                    let room = egui::vec2(
                        if direction.x < 0.0 {
                            anchor.x - price_rect.left()
                        } else {
                            price_rect.right() - anchor.x
                        },
                        if direction.y < 0.0 {
                            anchor.y - price_rect.top()
                        } else {
                            price_rect.bottom() - anchor.y
                        },
                    );
                    let side = delta
                        .x
                        .abs()
                        .max(delta.y.abs())
                        .min(room.x)
                        .min(room.y)
                        .max(0.0);
                    drawing.update_last_point_with_chart_coords(
                        *anchor + direction * side,
                        x_to_bar,
                        y_to_price,
                    );
                }
                let second_click = response.clicked()
                    && !ui
                        .ctx()
                        .data(|d| d.get_temp::<bool>(first_press).unwrap_or(false));
                if response.drag_stopped_by(egui::PointerButton::Primary) || second_click {
                    // The preview is already snapped and constrained; keep it as the endpoint.
                    drawing_manager.complete_drag_drawing();
                }
            }
            if released {
                ui.ctx().data_mut(|d| d.remove::<bool>(first_press));
            }
        }

        DrawingInteractionMode::ContinuousDraw => {
            if ui.input(|i| i.pointer.primary_pressed()) && response.is_pointer_button_down_on() {
                if let Some(pos) = primary_press_pos(ui).filter(|p| price_rect.contains(*p)) {
                    drawing_manager.start_drawing_with_coords(
                        active_tool,
                        pos,
                        x_to_bar,
                        y_to_price,
                    );
                    if let Some(current) = pointer_pos.filter(|current| *current != pos) {
                        drawing_manager.add_point_with_coords(
                            price_rect.clamp(current),
                            x_to_bar,
                            y_to_price,
                        );
                    }
                }
            } else if (response.dragged() || ui.input(|i| i.pointer.primary_released()))
                && drawing_manager.curr_drawing.is_some()
            {
                if let Some(pos) = pointer_pos {
                    drawing_manager.add_point_with_coords(
                        price_rect.clamp(pos),
                        x_to_bar,
                        y_to_price,
                    );
                }
                if ui.input(|i| i.pointer.primary_released()) {
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

    #[test]
    fn modifier_placement_constrains_shapes_and_temporarily_inverts_magnet() {
        use egui::{Modifiers, pos2, vec2};
        for (tool, modifiers, magnet, expected) in [
            (
                DrawingToolType::TrendLine,
                Modifiers::SHIFT,
                false,
                pos2(210.0, 210.0),
            ),
            (
                DrawingToolType::Ray,
                Modifiers::SHIFT,
                false,
                pos2(210.0, 210.0),
            ),
            (
                DrawingToolType::Ellipse,
                Modifiers::SHIFT,
                false,
                pos2(250.0, 250.0),
            ),
            (
                DrawingToolType::Rect,
                Modifiers::SHIFT,
                false,
                pos2(250.0, 250.0),
            ),
            (
                DrawingToolType::TrendLine,
                Modifiers::CTRL,
                false,
                pos2(251.0, 172.0),
            ),
            (
                DrawingToolType::TrendLine,
                Modifiers::COMMAND,
                true,
                pos2(250.0, 170.0),
            ),
        ] {
            let ctx = Context::default();
            let pane = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0));
            let mut manager = DrawingManager::new();
            manager.options.magnet_mode = magnet;
            manager.update_snap_options();
            let from = pos2(100.0, 100.0);
            let to = pos2(250.0, 170.0);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            for events in [
                vec![Event::PointerMoved(from)],
                vec![button(from, true)],
                vec![Event::PointerMoved(to)],
                vec![button(to, false)],
            ] {
                manager.snap_targets = vec![pos2(251.0, 172.0)];
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(pane),
                        events: std::iter::once(Event::ModifiersChanged(modifiers))
                            .chain(events)
                            .collect(),
                        ..Default::default()
                    },
                    |ui| {
                        let response =
                            ui.interact(pane, Id::NULL.with("chart"), Sense::click_and_drag());
                        handle_active_tool(
                            ui,
                            &mut manager,
                            &response,
                            pane,
                            &|x| x,
                            &f64::from,
                            tool,
                        );
                    },
                )
                .drop_without_applying_deltas();
            }
            assert_eq!(
                manager.drawings[0].points[1], expected,
                "{tool:?}, {modifiers:?}, magnet {magnet}"
            );
            assert_eq!(manager.options.magnet_mode, magnet);
        }
    }

    /// A drag that ends below the price pane, over the time axis, finishes the drawing with
    /// its last anchor on the pane's edge.
    #[test]
    fn a_drag_preserves_the_press_and_finishes_outside_the_pane() {
        for tool in [
            DrawingToolType::Rect,
            DrawingToolType::Brush,
            DrawingToolType::Highlighter,
        ] {
            for (batched, magnet) in [(false, false), (true, false), (false, true), (true, true)] {
                let ctx = Context::default();
                let chart = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
                let price_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
                let mut manager = DrawingManager::new();
                manager.options.magnet_mode = magnet;
                manager.update_snap_options();
                manager.snap_targets = vec![Pos2::new(52.0, 51.0)];
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                let mut frames = vec![
                    vec![Event::PointerMoved(Pos2::new(50.0, 50.0))],
                    vec![button(Pos2::new(50.0, 50.0), true)],
                    vec![Event::PointerMoved(Pos2::new(100.0, 100.0))],
                    vec![Event::PointerMoved(Pos2::new(150.0, 350.0))],
                    vec![button(Pos2::new(150.0, 350.0), false)],
                ];
                if batched {
                    let motion = frames.remove(2);
                    frames[1].extend(motion);
                }
                for events in frames {
                    let input = RawInput {
                        screen_rect: Some(chart),
                        events,
                        ..Default::default()
                    };
                    ctx.run_ui(input, |ui| {
                        let response =
                            ui.interact(chart, Id::NULL.with("chart"), Sense::click_and_drag());
                        handle_active_tool(
                            ui,
                            &mut manager,
                            &response,
                            price_rect,
                            &|x| x,
                            &|y| f64::from(y),
                            tool,
                        );
                    })
                    .drop_without_applying_deltas();
                }
                assert!(manager.curr_drawing.is_none());
                assert_eq!(
                    manager.drawings[0].chart_points[0],
                    if magnet && tool == DrawingToolType::Rect {
                        ChartPoint::new(52.0, 51.0)
                    } else {
                        ChartPoint::new(50.0, 50.0)
                    },
                    "{tool:?}"
                );
                if tool != DrawingToolType::Rect {
                    assert_eq!(
                        manager.drawings[0].chart_points[1],
                        ChartPoint::new(100.0, 100.0),
                        "{tool:?}"
                    );
                }
                assert_eq!(manager.drawings.len(), 1);
                assert_eq!(
                    manager.drawings[0].chart_points.last(),
                    Some(&ChartPoint::new(150.0, 300.0))
                );
            }
        }
    }
}
