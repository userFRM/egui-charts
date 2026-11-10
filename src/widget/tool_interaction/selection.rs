//! Drawing selection and manipulation
//!
//! Handles:
//! - Drawing selection via click or Ctrl/Command-dragged area
//! - Handle dragging for drawing manipulation
//! - Eraser mode (click to delete)
//! - Keyboard shortcuts (Delete, Escape)

use crate::drawings::{DrawingManager, DrawingToolType, HandlePos};
use crate::widget::tool_interaction::cursor_modes::CursorModeState;
use egui::{Painter, Rect, Response, Stroke, Ui, epaint::StrokeKind};

/// Handle drawing selection, handle dragging, and eraser-mode deletion.
///
/// When no drawing tool is active, clicks are interpreted as selection attempts.
/// An unlocked drawing can be selected and moved in one drag. In eraser mode,
/// clicking on a drawing deletes it instead, whatever is selected, unless it is locked.
/// Ctrl/Command-drag on empty space adds drawings intersecting the visible area. Releasing
/// the modifier continues the gesture; Escape cancels it. Locked drawings can be selected,
/// but subsequent editing retains the existing lock rules. Shift+click on empty space measures.
pub fn handle_selection<F1, F2, F3>(
    ui: &Ui,
    drawing_manager: &mut DrawingManager,
    cursor_modes: &mut CursorModeState,
    response: &Response,
    price_rect: Rect,
    pane: Option<usize>,
    x_to_bar: &F1,
    y_to_price: &F2,
    translate_price: &F3,
) where
    F1: Fn(f32) -> f32,
    F2: Fn(f32) -> f64,
    F3: Fn(f64, f32, f32) -> f64,
{
    drawing_manager.interaction_modifiers = ui.input(|i| i.modifiers);
    if handle_selection_area(ui, drawing_manager, response, price_rect, pane) {
        cursor_modes.eraser.set_hover(None);
        return;
    }
    let pointer_pos = if response.clicked() || response.drag_started() || response.dragged() {
        response.interact_pointer_pos()
    } else {
        response.hover_pos()
    };

    // A handle drag follows the pointer, kept on the pane, until it is released anywhere.
    if let Some((id, handle)) = drawing_manager.dragging_handle {
        ui.ctx()
            .set_cursor_icon(handle_cursor(drawing_manager, id, handle));
        if (response.dragged() || response.drag_stopped())
            && let Some(pos) = response.interact_pointer_pos()
        {
            drawing_manager.update_drag_handle_with_coords(
                price_rect.clamp(pos),
                x_to_bar,
                y_to_price,
                translate_price,
            );
        }
        if response.drag_stopped() {
            drawing_manager.end_drag_handle();
        }
        return;
    }

    if let Some(pos) = pointer_pos {
        let origin = if response.drag_started() {
            ui.input(|i| i.pointer.press_origin()).unwrap_or(pos)
        } else {
            pos
        };
        if !price_rect.contains(origin) {
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

        let selected: Vec<_> = drawing_manager.selected_ids().collect();
        let handle = (selected.len() == 1)
            .then(|| drawing_manager.hit_test_handle_by_id(origin, selected[0]))
            .flatten();
        let hit = drawing_manager.hit_test(origin).filter(|&id| {
            drawing_manager
                .drawings
                .iter()
                .any(|d| d.id == id && !d.locked)
        });
        if let Some(handle) = handle {
            ui.ctx().set_cursor_icon(if handle == HandlePos::Middle {
                egui::CursorIcon::Grab
            } else {
                handle_cursor(drawing_manager, selected[0], handle)
            });
        } else if hit.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        if response.drag_started() {
            if let Some(handle) = handle {
                drawing_manager.start_drag_handle(selected[0], handle);
            } else if let Some(id) = hit {
                if !drawing_manager.is_selected(id) {
                    drawing_manager.select(id);
                }
                drawing_manager.start_drag_body(id, origin);
            }
            if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                drawing_manager.copy_dragged_drawings();
            }
            if let Some((id, handle)) = drawing_manager.dragging_handle {
                ui.ctx()
                    .set_cursor_icon(handle_cursor(drawing_manager, id, handle));
            }
            drawing_manager.update_drag_handle_with_coords(
                price_rect.clamp(pos),
                x_to_bar,
                y_to_price,
                translate_price,
            );
        } else if response.clicked() {
            if ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command) {
                if let Some(id) = drawing_manager.hit_test(pos) {
                    drawing_manager.toggle_selection(id);
                } else if ui.input(|i| i.modifiers.shift) {
                    drawing_manager.set_active_tool(Some(DrawingToolType::Measure));
                    drawing_manager.start_drawing_with_coords(
                        DrawingToolType::Measure,
                        pos,
                        |x| x_to_bar(x).round(),
                        y_to_price,
                    );
                }
            } else {
                drawing_manager.select_at(pos);
            }
        }
    } else {
        cursor_modes.eraser.set_hover(None);
    }
}

/// An area starts only on empty space, leaving handle/body drags and their copy shortcut intact.
fn handle_selection_area(
    ui: &Ui,
    manager: &mut DrawingManager,
    response: &Response,
    pane_rect: Rect,
    pane: Option<usize>,
) -> bool {
    let area_input = manager.box_selection_input(ui);
    if manager.selection_area().is_none()
        && area_input
        && !manager.eraser_mode
        && manager.dragging_handle.is_none()
        && response.drag_started_by(egui::PointerButton::Primary)
        && ui.input(|i| i.modifiers.ctrl || i.modifiers.command)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && pane_rect.contains(origin)
        && manager.hit_test(origin).is_none()
        && !manager
            .selected_ids()
            .any(|id| manager.hit_test_handle_by_id(origin, id).is_some())
    {
        manager.begin_selection_area(origin, pane);
    }
    // Escape also cancels while the selection modifier is still held.
    if manager.selection_area().is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        manager.deselect();
    }
    let Some(area) = manager.selection_area() else {
        return false;
    };
    if area.pane != pane {
        return true;
    }
    if !area.cancelled
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        let rect = Rect::from_two_pos(pane_rect.clamp(area.origin), pane_rect.clamp(pointer));
        manager.update_selection_area(ui.ctx(), rect);
        let color = ui.visuals().selection.bg_fill;
        ui.painter().with_clip_rect(pane_rect).rect(
            rect,
            0.0,
            color.gamma_multiply(0.15),
            Stroke::new(1.0, color),
            StrokeKind::Inside,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        manager.end_selection_area();
    }
    true
}

/// The cursor for a handle: a resize arrow along the way it moves its drawing, or a grab for a body move.
fn handle_cursor(manager: &DrawingManager, id: usize, handle: HandlePos) -> egui::CursorIcon {
    use egui::CursorIcon;
    let reversed = manager.drawings.iter().any(|d| {
        d.id == id
            && matches!(
                d.tool_type,
                DrawingToolType::Rect | DrawingToolType::GannBox
            )
            && d.points.len() >= 2
            && (d.points[1].x - d.points[0].x) * (d.points[1].y - d.points[0].y) < 0.0
    });
    match handle {
        HandlePos::TopLeft | HandlePos::BottomRight if reversed => CursorIcon::ResizeNeSw,
        HandlePos::TopRight | HandlePos::BottomLeft if reversed => CursorIcon::ResizeNwSe,
        HandlePos::Middle => CursorIcon::Grabbing,
        HandlePos::TopLeft | HandlePos::BottomRight => CursorIcon::ResizeNwSe,
        HandlePos::TopRight | HandlePos::BottomLeft => CursorIcon::ResizeNeSw,
        HandlePos::Top | HandlePos::Bottom => CursorIcon::ResizeVertical,
        HandlePos::Left | HandlePos::Right => CursorIcon::ResizeHorizontal,
        HandlePos::Start | HandlePos::End | HandlePos::Point(_) => CursorIcon::ResizeNwSe,
    }
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
    use crate::drawings::{ChartPoint, Drawing};
    use egui::{Context, Event, Id, PointerButton, Pos2, RawInput, Sense};

    /// An area gesture adds visible drawings without panning or copying, survives modifier
    /// release, cancels on Escape, and leaves the set ready for one undoable edit.
    #[test]
    fn box_selection_adds_without_pan_and_edits_the_selected_set() {
        use crate::widget::test_frames::{chart, frame};
        use egui::{Modifiers, pos2, vec2};
        for modifiers in [Modifiers::CTRL, Modifiers::COMMAND] {
            for cancel in [false, true] {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                chart.chart_options.crosshair.style = crate::config::CrosshairStyle::Arrow;
                for id in 6..=13 {
                    let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
                    drawing.completed = true;
                    let price = if id == 6 {
                        11.0
                    } else {
                        14.0 + (id % 3) as f64
                    };
                    drawing.chart_points =
                        vec![ChartPoint::new(30.0, price), ChartPoint::new(70.0, price)];
                    drawing.locked = id == 9;
                    drawing.visible = id != 10;
                    drawing.selectable = id != 11;
                    drawing.pane = (id == 12).then_some(999);
                    if id == 13 {
                        drawing.timeframe_visibility =
                            crate::drawings::TimeframeVisibility::specific(&[]);
                    }
                    chart.drawings_mut().drawings.push(drawing);
                }
                chart.drawings_mut().select(6);
                frame(&ctx, &mut chart, 0.0, vec![]);
                let drawing = &chart.drawings().drawings[1];
                let from = pos2(drawing.points[0].x - 20.0, drawing.points[0].y - 70.0);
                let to = pos2(drawing.points[1].x + 20.0, drawing.points[0].y + 60.0);
                let view = chart.get_time_scale_state();
                let before = chart.drawings().drawings.clone();
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                };
                for (step, events) in [
                    vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(to)],
                    vec![button(to, true)],
                    vec![Event::PointerMoved(from)],
                    if cancel {
                        vec![Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        }]
                    } else {
                        vec![Event::ModifiersChanged(Modifiers::NONE)]
                    },
                    vec![Event::PointerMoved(from - vec2(5.0, 5.0))],
                    vec![button(from, false)],
                ]
                .into_iter()
                .enumerate()
                {
                    frame(&ctx, &mut chart, 0.1 + step as f64 * 0.1, events);
                    let after = chart.get_time_scale_state();
                    assert_eq!(after, view, "step {step}");
                    assert_eq!(chart.drawings().drawings.len(), before.len());
                    for (drawing, old) in chart.drawings().drawings.iter().zip(&before) {
                        assert_eq!(drawing.chart_points, old.chart_points);
                    }
                }
                assert!(chart.drawings().selection_area().is_none());
                let selected: Vec<_> = chart.drawings().selected_ids().collect();
                assert_eq!(selected, if cancel { vec![] } else { vec![6, 7, 8, 9] });
                assert!(
                    !chart.drawings_mut().undo(),
                    "selection is not a stored edit"
                );
                if !cancel {
                    chart.drawings_mut().edit_selected(|d| d.stroke_width = 4.0);
                    for drawing in &chart.drawings().drawings {
                        assert_eq!(drawing.stroke_width == 4.0, [6, 7, 8].contains(&drawing.id));
                    }
                    assert!(chart.drawings_mut().undo());
                    for (drawing, old) in chart.drawings().drawings.iter().zip(&before) {
                        assert_eq!(drawing.stroke_width, old.stroke_width);
                    }
                    assert!(!chart.drawings_mut().undo());
                }
            }
        }
    }

    /// Crossing geometry is selected even with both anchors outside; a disjoint bounding box,
    /// an unfilled circle's interior and geometry clipped outside the pane are not selected.
    #[test]
    fn box_selection_intersects_painted_geometry() {
        use crate::drawings::rendering::ChartView;
        use egui::{Modifiers, pos2, vec2};
        let pane = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0));
        for (tool, anchors, area, expected) in [
            (
                DrawingToolType::TrendLine,
                vec![(100., 100.), (300., 300.)],
                [(180., 130.), (220., 270.)],
                true,
            ),
            (
                DrawingToolType::TrendLine,
                vec![(100., 100.), (300., 300.)],
                [(100., 240.), (150., 290.)],
                false,
            ),
            (
                DrawingToolType::Ray,
                vec![(100., 100.), (200., 100.)],
                [(300., 80.), (340., 120.)],
                true,
            ),
            (
                DrawingToolType::HorizontalLine,
                vec![(100., 100.)],
                [(300., 80.), (340., 120.)],
                true,
            ),
            (
                DrawingToolType::Circle,
                vec![(200., 200.), (280., 200.)],
                [(180., 180.), (220., 220.)],
                false,
            ),
            (
                DrawingToolType::Circle,
                vec![(200., 200.), (280., 200.)],
                [(270., 180.), (310., 220.)],
                true,
            ),
            (
                DrawingToolType::TextLabel,
                vec![(50., 100.)],
                [(150., 80.), (200., 120.)],
                true,
            ),
            (
                DrawingToolType::TrendLine,
                vec![(50., 450.), (450., 450.)],
                [(20., 350.), (480., 390.)],
                false,
            ),
        ] {
            let ctx = Context::default();
            let mut manager = DrawingManager::new();
            manager.update_pane_anchors(pane);
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.fill_color = None;
            drawing.text = (tool == DrawingToolType::TextLabel)
                .then(|| "A deliberately long annotation".into());
            drawing.points = anchors.into_iter().map(|(x, y)| pos2(x, y)).collect();
            drawing.chart_points = drawing
                .points
                .iter()
                .map(|p| ChartPoint::new(p.x, f64::from(p.y)))
                .collect();
            manager.drawings.push(drawing);
            let mut cursors = CursorModeState::default();
            let [from, to] = area.map(|(x, y)| pos2(x, y));
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::CTRL,
            };
            for events in [
                vec![],
                vec![Event::PointerMoved(from)],
                vec![button(from, true)],
                vec![Event::PointerMoved(to)],
                vec![button(to, false)],
            ] {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(pane),
                        events: std::iter::once(Event::ModifiersChanged(Modifiers::CTRL))
                            .chain(events)
                            .collect(),
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
                            None,
                            &|x| x,
                            &f64::from,
                            &|price, from, to| price + f64::from(to - from),
                        );
                        manager.render_all(
                            ui.painter(),
                            pane,
                            &ChartView {
                                price_formatter: None,
                                time_formatter: None,
                                bar_seconds: 0,
                                instrument: None,
                                bars: &[],
                                revision: 0,
                                bar_to_x: &|x| x,
                                price_to_y: &|y| y as f32,
                                y_to_price: &f64::from,
                                time_axis: true,
                                executions: None,
                                timeframe: None,
                                finer: &[],
                            },
                        );
                    },
                )
                .drop_without_applying_deltas();
            }
            assert_eq!(manager.is_selected(7), expected, "{tool:?}, {area:?}");
        }
    }

    /// Shift projects a dragged endpoint onto 45 degrees, locks body movement to one axis,
    /// and command drags leave the original untouched with a single undoable copy.
    #[test]
    fn modifier_drags_constrain_points_and_copy_once() {
        use egui::{Modifiers, pos2, vec2};
        for (tool, off_pane, pan) in [
            (DrawingToolType::TrendLine, false, egui::Vec2::ZERO),
            (DrawingToolType::Ray, false, egui::Vec2::ZERO),
            (DrawingToolType::ParallelChannel, false, egui::Vec2::ZERO),
            (DrawingToolType::TrendLine, true, egui::Vec2::ZERO),
            (DrawingToolType::Ray, true, egui::Vec2::ZERO),
            (DrawingToolType::TrendLine, false, vec2(20.0, 0.0)),
            (DrawingToolType::TrendLine, false, vec2(0.0, 20.0)),
        ] {
            for body in [false, true] {
                for modifiers in [Modifiers::SHIFT, Modifiers::CTRL, Modifiers::COMMAND] {
                    if (off_pane || pan != egui::Vec2::ZERO)
                        && (body || modifiers != Modifiers::SHIFT)
                    {
                        continue;
                    }
                    let ctx = Context::default();
                    let pane = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0));
                    let mut manager = DrawingManager::new();
                    manager.update_pane_anchors(pane);
                    let mut cursors = CursorModeState::default();
                    let mut drawing = Drawing::new(7, tool);
                    drawing.points = vec![pos2(80.0, 80.0), pos2(180.0, 180.0)];
                    if tool == DrawingToolType::ParallelChannel {
                        drawing.points.push(pos2(80.0, 260.0));
                    }
                    if off_pane {
                        drawing.points[0] = pos2(
                            if tool == DrawingToolType::Ray {
                                800.0
                            } else {
                                80.0
                            },
                            -100.0,
                        );
                    }
                    drawing.chart_points = drawing
                        .points
                        .iter()
                        .map(|p| ChartPoint::new(p.x, f64::from(p.y)))
                        .collect();
                    drawing.completed = true;
                    drawing.stored_id = Some("original".into());
                    let before = drawing.chart_points.clone();
                    manager.drawings.push(drawing);
                    if modifiers != Modifiers::CTRL {
                        manager.select(7);
                    }
                    let from = if body {
                        pos2(110.0, 110.0)
                    } else {
                        pos2(180.0, 180.0)
                    };
                    let to = if off_pane {
                        pos2(140.0, 0.0)
                    } else if body {
                        pos2(210.5, 160.0)
                    } else {
                        pos2(240.0, 200.0)
                    };
                    let button = |pos, pressed| Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers,
                    };
                    for (frame, events) in [
                        vec![Event::PointerMoved(from)],
                        vec![button(from, true)],
                        vec![Event::PointerMoved(to)],
                        vec![button(to, false)],
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let offset = if frame == 3 { pan } else { egui::Vec2::ZERO };
                        if offset != egui::Vec2::ZERO {
                            manager.update_pane_screen_coords(
                                |bar| bar + offset.x,
                                |price| price as f32 + offset.y,
                            );
                        }
                        ctx.run_ui(
                            RawInput {
                                screen_rect: Some(pane),
                                events: std::iter::once(Event::ModifiersChanged(modifiers))
                                    .chain(events)
                                    .collect(),
                                ..Default::default()
                            },
                            |ui| {
                                let response = ui.interact(
                                    pane,
                                    Id::NULL.with("chart"),
                                    Sense::click_and_drag(),
                                );
                                handle_selection(
                                    ui,
                                    &mut manager,
                                    &mut cursors,
                                    &response,
                                    pane,
                                    None,
                                    &|x| x - offset.x,
                                    &|y: f32| {
                                        f64::from(y.clamp(pane.top(), pane.bottom()) - offset.y)
                                    },
                                    &|price, from, to| price + f64::from(to - from),
                                );
                            },
                        )
                        .drop_without_applying_deltas();
                    }
                    let copying = modifiers != Modifiers::SHIFT;
                    assert_eq!(manager.drawings.len(), if copying { 2 } else { 1 });
                    if copying {
                        assert_eq!(manager.drawings[0].chart_points, before);
                    }
                    let moved = manager.drawings.last().unwrap();
                    let expected = if pan != egui::Vec2::ZERO {
                        pos2(210.0 + pan.x, 210.0 + pan.y)
                    } else if off_pane {
                        if tool == DrawingToolType::Ray {
                            pos2(140.0, -100.0)
                        } else {
                            pos2(180.0, 0.0)
                        }
                    } else if body {
                        if copying {
                            pos2(280.5, 230.0)
                        } else {
                            pos2(280.5, 180.0)
                        }
                    } else if copying {
                        to
                    } else {
                        pos2(220.0, 220.0)
                    };
                    assert_eq!(
                        moved.points[1], expected,
                        "{tool:?}, body {body}, {modifiers:?}"
                    );
                    // A Shift move keeps whole bars, though its converter is precise.
                    let bar = if body && !copying {
                        expected.x.round()
                    } else {
                        expected.x
                    };
                    assert_eq!(
                        moved.chart_points[1],
                        ChartPoint::new(bar - pan.x, f64::from(expected.y - pan.y))
                    );
                    if copying {
                        assert_eq!(moved.stored_id, None);
                    }
                    assert!(manager.undo());
                    assert_eq!(manager.drawings.len(), 1);
                    assert_eq!(manager.drawings[0].chart_points, before);
                    assert!(!manager.undo());
                    assert!(manager.redo());
                    assert_eq!(manager.drawings.last().unwrap().points[1], expected);
                }
            }
        }
    }

    #[test]
    fn body_drag_selects_without_panning_and_copies_with_modifiers() {
        use crate::widget::test_frames::{chart, frame};
        use egui::{Modifiers, vec2};
        for (tool, selected) in [DrawingToolType::TrendLine, DrawingToolType::Rect]
            .into_iter()
            .flat_map(|tool| [false, true].map(|selected| (tool, selected)))
        {
            for (modifiers, locked) in [
                (Modifiers::NONE, false),
                (Modifiers::CTRL, false),
                (Modifiers::COMMAND, false),
                (Modifiers::NONE, true),
                (Modifiers::CTRL, true),
                (Modifiers::COMMAND, true),
            ] {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                chart.chart_options.crosshair.style = crate::config::CrosshairStyle::Arrow;
                let mut drawing = Drawing::new(7, tool);
                drawing.completed = true;
                drawing.locked = locked;
                drawing.chart_points =
                    vec![ChartPoint::new(30.0, 14.0), ChartPoint::new(70.0, 17.0)];
                let before = drawing.chart_points.clone();
                chart.drawings_mut().drawings.push(drawing);
                if selected {
                    chart.drawings_mut().select(7);
                }
                frame(&ctx, &mut chart, 0.0, vec![]);
                let points = &chart.drawings().drawings[0].points;
                let anchor = points[0];
                let from = points[0].lerp(points[1], 0.25);
                let to = from + vec2(40.0, 20.0);
                let view = chart.get_time_scale_state();
                assert_eq!(
                    frame(&ctx, &mut chart, 0.05, vec![Event::PointerMoved(anchor)]),
                    if locked {
                        egui::CursorIcon::Default
                    } else if selected {
                        if tool == DrawingToolType::Rect {
                            egui::CursorIcon::ResizeNeSw
                        } else {
                            egui::CursorIcon::ResizeNwSe
                        }
                    } else {
                        egui::CursorIcon::PointingHand
                    },
                );
                if tool == DrawingToolType::Rect && selected && !locked {
                    let points = chart.drawings().drawings[0].points.clone();
                    for (pos, cursor) in [
                        (
                            Pos2::new((points[0].x + points[1].x) / 2.0, points[0].y),
                            egui::CursorIcon::ResizeVertical,
                        ),
                        (
                            Pos2::new(points[0].x, (points[0].y + points[1].y) / 2.0),
                            egui::CursorIcon::ResizeHorizontal,
                        ),
                    ] {
                        assert_eq!(
                            frame(&ctx, &mut chart, 0.06, vec![Event::PointerMoved(pos)]),
                            cursor
                        );
                    }
                }
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                };
                for (i, events) in [
                    vec![Event::PointerMoved(from)],
                    vec![button(from, true)],
                    vec![Event::PointerMoved(from + vec2(12.0, 6.0))],
                    vec![Event::PointerMoved(to)],
                    vec![button(to, false)],
                ]
                .into_iter()
                .enumerate()
                {
                    let cursor = frame(
                        &ctx,
                        &mut chart,
                        0.1 + i as f64 * 0.1,
                        std::iter::once(Event::ModifiersChanged(modifiers))
                            .chain(events)
                            .collect(),
                    );
                    if i == 0 {
                        assert_eq!(
                            cursor,
                            if locked {
                                egui::CursorIcon::Default
                            } else {
                                egui::CursorIcon::PointingHand
                            }
                        );
                    } else if (2..=3).contains(&i) && !locked {
                        assert_eq!(cursor, egui::CursorIcon::Grabbing);
                    }
                }
                if locked {
                    assert_ne!(chart.get_time_scale_state(), view);
                    assert_eq!(chart.drawings().drawings[0].chart_points, before);
                    assert_eq!(chart.drawings().is_selected(7), selected);
                    assert!(!chart.can_undo());
                    continue;
                }
                assert_eq!(
                    chart.get_time_scale_state(),
                    view,
                    "selected {selected}, {modifiers:?}"
                );
                let copying = modifiers != Modifiers::NONE;
                assert_eq!(chart.drawings().drawings.len(), if copying { 2 } else { 1 });
                if copying {
                    assert_eq!(chart.drawings().drawings[0].chart_points, before);
                }
                let moved = chart.drawings().drawings.last().unwrap();
                assert_ne!(moved.chart_points, before);
                assert!(chart.drawings().is_selected(moved.id));
                assert!(chart.undo());
                assert_eq!(chart.drawings().drawings.len(), 1);
                assert_eq!(chart.drawings().drawings[0].chart_points, before);
                assert!(!chart.can_undo());
                assert!(chart.redo());
                assert_eq!(chart.drawings().drawings.len(), if copying { 2 } else { 1 });
            }
        }
    }

    /// Edge handles and bodies follow an outward drag from its first frame, just as inward drags do.
    #[test]
    fn edge_drags_start_from_the_press_origin_and_clamp_every_update() {
        use egui::{pos2, vec2};
        let pane = Rect::from_min_max(pos2(40.0, 40.0), pos2(540.0, 440.0));
        for (points, outward) in [
            ([pos2(537.0, 140.0), pos2(537.0, 340.0)], vec2(1.0, 0.0)),
            ([pos2(140.0, 43.0), pos2(440.0, 43.0)], vec2(0.0, -1.0)),
            ([pos2(140.0, 437.0), pos2(440.0, 437.0)], vec2(0.0, 1.0)),
        ] {
            for body in [false, true] {
                for direction in [-1.0, 1.0] {
                    let ctx = Context::default();
                    let mut manager = DrawingManager::new();
                    let mut cursors = CursorModeState::default();
                    let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
                    drawing.points = points.to_vec();
                    drawing.chart_points = points
                        .iter()
                        .map(|p| ChartPoint::new(p.x, f64::from(p.y)))
                        .collect();
                    manager.drawings.push(drawing);
                    manager.select(7);
                    let from = if body {
                        points[0].lerp(points[1], 0.25)
                    } else {
                        points[1]
                    };
                    let first = from + outward * (9.0 * direction);
                    let to = from + outward * 30.0 + outward.rot90() * 40.0;
                    let button = |pos, pressed| Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    for (frame, events) in [
                        vec![Event::PointerMoved(from)],
                        vec![button(from, true)],
                        vec![Event::PointerMoved(first)],
                        vec![Event::PointerMoved(to)],
                        vec![button(to, false)],
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ctx.run_ui(
                            RawInput {
                                screen_rect: Some(pane.expand(40.0)),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                let response = ui.interact(
                                    pane,
                                    Id::NULL.with("chart"),
                                    Sense::click_and_drag(),
                                );
                                handle_selection(
                                    ui,
                                    &mut manager,
                                    &mut cursors,
                                    &response,
                                    pane,
                                    None,
                                    &|x| x,
                                    &f64::from,
                                    &|price, from, to| price + f64::from(to - from),
                                );
                            },
                        )
                        .drop_without_applying_deltas();
                        if frame >= 2 {
                            let target = pane.clamp(if frame == 2 { first } else { to });
                            let expected = if body {
                                points[1] + (target - from)
                            } else {
                                target
                            };
                            assert_eq!(
                                manager.drawings[0].points[1], expected,
                                "body {body}, direction {direction}, edge {outward:?}, frame {frame}"
                            );
                        }
                    }
                    assert!(manager.dragging_handle.is_none());
                    assert!(manager.undo());
                    assert_eq!(manager.drawings[0].points, points);
                    assert!(!manager.undo());
                }
            }
        }
    }

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

    /// Shift/Ctrl/Command-click keeps both drawings selected; dragging either body moves the group
    /// without panning, and Delete removes the whole selection in one undo step.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn modifier_selection_moves_and_deletes_as_one_action() {
        use crate::widget::test_frames::{chart, frame};
        use egui::{Key, Modifiers, vec2};
        for modifier in [Modifiers::SHIFT, Modifiers::CTRL, Modifiers::COMMAND] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            for (id, price) in [(7, 14.0), (8, 17.0)] {
                let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
                drawing.completed = true;
                drawing.chart_points =
                    vec![ChartPoint::new(30.0, price), ChartPoint::new(70.0, price)];
                chart.drawings_mut().drawings.push(drawing);
            }
            let mut time = 0.0;
            frame(&ctx, &mut chart, time, vec![]);
            let first = chart.drawings().drawings[0].points[0]
                .lerp(chart.drawings().drawings[0].points[1], 0.5);
            let second = chart.drawings().drawings[1].points[0]
                .lerp(chart.drawings().drawings[1].points[1], 0.5);
            let button = |pos, pressed, modifiers| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            let mut step = |chart: &mut crate::widget::Chart, events| {
                time += 0.5;
                frame(&ctx, chart, time, events);
            };
            for (pos, modifiers) in [(first, Modifiers::NONE), (second, modifier)] {
                step(
                    &mut chart,
                    vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(pos)],
                );
                step(&mut chart, vec![button(pos, true, modifiers)]);
                step(&mut chart, vec![button(pos, false, modifiers)]);
            }
            // A context action targets the clicked member without dropping its peers.
            assert!(chart.drawings().measurement.is_none());
            for pressed in [true, false] {
                step(
                    &mut chart,
                    vec![
                        Event::ModifiersChanged(Modifiers::NONE),
                        Event::PointerMoved(first),
                        Event::PointerButton {
                            pos: first,
                            button: PointerButton::Secondary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(chart.drawings().sel_drawing(), Some(7));
            assert_eq!(chart.drawings().selected_ids().collect::<Vec<_>>(), [7, 8]);
            let view = chart.get_time_scale_state();
            let before: Vec<_> = chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.chart_points.clone())
                .collect();
            let to = first + vec2(40.0, 20.0);
            for events in [
                vec![
                    Event::ModifiersChanged(Modifiers::NONE),
                    Event::PointerMoved(first),
                ],
                vec![button(first, true, Modifiers::NONE)],
                vec![Event::PointerMoved(to)],
                vec![button(to, false, Modifiers::NONE)],
            ] {
                step(&mut chart, events);
            }
            assert_eq!(
                chart.get_time_scale_state(),
                view,
                "a selected body must own the drag"
            );
            let delta = chart.drawings().drawings[0].chart_points[0].bar_idx - before[0][0].bar_idx;
            assert!(delta > 0.0);
            for (drawing, old) in chart.drawings().drawings.iter().zip(&before) {
                assert_eq!(drawing.chart_points[0].bar_idx - old[0].bar_idx, delta);
            }
            assert!(chart.undo());
            for (drawing, old) in chart.drawings().drawings.iter().zip(&before) {
                assert_eq!(&drawing.chart_points, old);
            }
            step(
                &mut chart,
                vec![Event::Key {
                    key: Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert!(chart.drawings().drawings.is_empty());
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings.len(), 2);
            assert!(!chart.can_undo());
        }
    }

    /// Whole drawings keep off-pane anchors and their shape through the widget's scale mapping.
    #[test]
    fn whole_drawing_drags_keep_off_pane_anchors_on_every_scale() {
        use crate::scales::PriceScaleMode::{IndexedTo100, Logarithmic, Normal, Percentage};
        use crate::widget::test_frames::{chart, frame};
        use egui::vec2;
        for mode in [Normal, Percentage, IndexedTo100, Logarithmic] {
            for (inverted, dy, grab) in [false, true].into_iter().flat_map(|inverted| {
                [-60.0, 60.0]
                    .into_iter()
                    .flat_map(move |dy| ["body", "group"].map(|grab| (inverted, dy, grab)))
            }) {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                chart.price_scale_mut().options_mut().mode = mode;
                chart.price_scale_mut().options_mut().invert_scale = inverted;
                frame(&ctx, &mut chart, 0.0, vec![]);
                let pane = chart.get_rendered_price_rect();
                let map = chart.get_chart_mapping();
                let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
                drawing.completed = true;
                drawing.chart_points = [-0.25, 1.25]
                    .into_iter()
                    .enumerate()
                    .map(|(i, y)| {
                        ChartPoint::new(
                            30.0 + i as f32 * 40.0,
                            map.y_to_price(pane.top() + y * pane.height()),
                        )
                    })
                    .collect();
                chart.drawings_mut().drawings.push(drawing.clone());
                chart.drawings_mut().select(7);
                if grab == "group" {
                    drawing.id = 8;
                    for point in &mut drawing.chart_points {
                        point.bar_idx += 5.0;
                    }
                    chart.drawings_mut().drawings.push(drawing);
                    chart.drawings_mut().select_drawings([7, 8]);
                }
                frame(&ctx, &mut chart, 0.1, vec![]);
                let before: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.points.clone())
                    .collect();
                let from = before[0][0].lerp(before[0][1], 0.4);
                let to = from + vec2(0.0, dy);
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                for (i, events) in [
                    vec![Event::PointerMoved(from)],
                    vec![button(from, true)],
                    vec![Event::PointerMoved(from + vec2(0.0, dy / 4.0))],
                    vec![Event::PointerMoved(to)],
                    vec![button(to, false)],
                    vec![],
                ]
                .into_iter()
                .enumerate()
                {
                    frame(&ctx, &mut chart, 0.2 + i as f64 * 0.1, events);
                }
                for (drawing, old) in chart.drawings().drawings.iter().zip(&before) {
                    for (point, before) in drawing.points.iter().zip(old) {
                        assert!(
                            (point.y - before.y - dy).abs() < 0.001,
                            "{mode:?}, inverted {inverted}, {grab}, dy {dy}: {before:?} -> {point:?}"
                        );
                    }
                    if mode == Logarithmic {
                        assert!(drawing.chart_points.iter().all(|p| p.price > 0.0));
                    }
                }
            }
        }
    }

    /// Body grabs preserve scale offsets and horizontal drags preserve exact prices,
    /// with one history step and one host update per drawing.
    #[test]
    fn body_drags_translate_every_anchor_once_unless_locked() {
        use crate::scales::{PriceMap, PriceRange, PriceScaleMode, PriceTransform};
        for tool in [
            DrawingToolType::TrendLine,
            DrawingToolType::Rect,
            DrawingToolType::FibonacciChannel,
        ] {
            for (locked, multi, dy, tight) in [(false, true, 0.0, true)].into_iter().chain(
                [(false, false), (true, false), (false, true)]
                    .into_iter()
                    .flat_map(|(locked, multi)| {
                        [-30.0, 0.0, 30.0].map(move |dy| (locked, multi, dy, false))
                    }),
            ) {
                let ctx = Context::default();
                let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(500.0, 400.0));
                let mut manager = DrawingManager::new();
                let mut cursors = CursorModeState::default();
                let mut drawing = Drawing::new(7, tool);
                drawing.points = vec![Pos2::new(50.0, 60.0), Pos2::new(250.0, 180.0)];
                if tool == DrawingToolType::Rect {
                    drawing.points = vec![Pos2::new(50.0, 50.0), Pos2::new(250.0, 150.0)];
                }
                if tool == DrawingToolType::FibonacciChannel {
                    drawing.points.push(Pos2::new(150.0, 200.0));
                }
                let map = if tight {
                    PriceMap::new(
                        PriceTransform::default(),
                        PriceRange::new(100.0, 100.01),
                        pane.top(),
                        pane.bottom(),
                        false,
                    )
                } else {
                    PriceMap::new(
                        PriceTransform {
                            mode: PriceScaleMode::Logarithmic,
                            base: 0.0,
                        },
                        PriceRange::new(1000.0_f64.ln() - 4.0, 1000.0_f64.ln()),
                        pane.top(),
                        pane.bottom(),
                        false,
                    )
                };
                let price = |y: f32| map.y_to_price(y);
                drawing.chart_points = drawing
                    .points
                    .iter()
                    .map(|p| ChartPoint::new(p.x / 10.0, price(p.y)))
                    .collect();
                if tight {
                    drawing.chart_points[0].price = 100.005123456789;
                    drawing.chart_points[1].price = 1000.0;
                }
                drawing.update_screen_coords(|b| b * 10.0, |p| map.price_to_y(p));
                drawing.locked = locked;
                let before = drawing.chart_points.clone();
                manager.drawings.push(drawing.clone());
                manager.select(7);
                if multi {
                    drawing.id = 8;
                    if !tight {
                        drawing.points.iter_mut().for_each(|p| p.y += 20.0);
                        drawing.chart_points = drawing
                            .points
                            .iter()
                            .map(|p| ChartPoint::new(p.x / 10.0, price(p.y)))
                            .collect();
                    }
                    manager.drawings.push(drawing);
                    manager.select_drawings([8, 7]);
                }
                let before_group: Vec<_> = manager
                    .drawings
                    .iter()
                    .map(|d| d.chart_points.clone())
                    .collect();
                let drawing = &manager.drawings[0];
                let fraction = if tight {
                    30.0 / (drawing.points[1].y - drawing.points[0].y).abs()
                } else {
                    0.25
                };
                let from = drawing.points[0].lerp(drawing.points[1], fraction);
                let to = from + egui::vec2(40.0, dy);
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                for events in [
                    vec![Event::PointerMoved(from)],
                    vec![button(from, true)],
                    vec![Event::PointerMoved(from + egui::vec2(10.0, dy / 4.0))],
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
                                None,
                                &|x| x / 10.0,
                                &|y: f32| price(y.clamp(pane.top(), pane.bottom())),
                                &|price, from, to| map.translate_price(price, from, to),
                            );
                        },
                    )
                    .drop_without_applying_deltas();
                }
                let screen_y = |price: f64| {
                    if tight {
                        f64::from(map.price_to_y(price))
                    } else {
                        -100.0 * (price / 1000.0).ln()
                    }
                };
                for (drawing, original) in manager.drawings.iter().zip(&before_group) {
                    if dy == 0.0 {
                        assert_eq!(
                            drawing
                                .chart_points
                                .iter()
                                .map(|p| p.price)
                                .collect::<Vec<_>>(),
                            original.iter().map(|p| p.price).collect::<Vec<_>>(),
                            "{tool:?}, tight {tight}, member {}: horizontal prices",
                            drawing.id
                        );
                    }
                    for (old, moved) in original.iter().zip(&drawing.chart_points) {
                        assert!(
                            (moved.bar_idx - old.bar_idx - if locked { 0.0 } else { 4.0 }).abs()
                                < 1e-5,
                            "{tool:?}, locked {locked}, member {}",
                            drawing.id
                        );
                        assert!(moved.price > 0.0);
                        assert!(
                            (screen_y(moved.price)
                                - screen_y(old.price)
                                - if locked { 0.0 } else { f64::from(dy) })
                            .abs()
                                < 1e-4,
                            "{tool:?}, locked {locked}, member {}, dy {dy}: {moved:?}",
                            drawing.id
                        );
                    }
                }
                let updates = manager.pending_sync.lock().unwrap();
                assert_eq!(
                    updates.len(),
                    usize::from(!locked) * if multi { 2 } else { 1 },
                    "{tool:?}"
                );
                if !locked {
                    assert!(
                        updates
                            .iter()
                            .any(|update| matches!(update, DrawingSyncAction::UpdateDrawing(7)))
                    );
                }
                drop(updates);
                assert_eq!(manager.undo(), !locked, "{tool:?}");
                assert_eq!(manager.drawings[0].chart_points, before);
                assert_eq!(
                    manager
                        .drawings
                        .iter()
                        .map(|d| d.chart_points.clone())
                        .collect::<Vec<_>>(),
                    before_group
                );
                assert!(!manager.undo());
            }
        }
    }

    /// One horizontal trend line at 15 over bars 30 to 70 on the 10 to 20 test chart, and the
    /// chart after a first frame.
    fn area_chart() -> (Context, crate::widget::Chart) {
        use crate::widget::test_frames::{chart, frame};
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        chart.chart_options.crosshair.style = crate::config::CrosshairStyle::Arrow;
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(30.0, 15.0), ChartPoint::new(70.0, 15.0)];
        chart.drawings_mut().drawings.push(drawing);
        frame(&ctx, &mut chart, 0.0, vec![]);
        (ctx, chart)
    }

    fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        }
    }

    /// Runs a Ctrl-drag area gesture on empty space, applies `change` while it is held, releases,
    /// then clicks the line. Returns whether the pane moved, whether the gesture is still
    /// active after the release, and the selection after the click.
    fn area_gesture_with(
        change: impl FnOnce(&mut crate::widget::Chart),
    ) -> (bool, bool, Vec<usize>) {
        use crate::widget::test_frames::frame;
        use egui::{Modifiers, vec2};
        let (ctx, mut chart) = area_chart();
        let line = chart.drawings().drawings[0].points.clone();
        let a = line[0] + vec2(0.0, -100.0);
        let b = a + vec2(40.0, 30.0);
        let pane = chart.get_rendered_price_rect();
        let ctrl = Modifiers::CTRL;
        frame(
            &ctx,
            &mut chart,
            0.1,
            vec![Event::ModifiersChanged(ctrl), Event::PointerMoved(a)],
        );
        frame(&ctx, &mut chart, 0.2, vec![button(a, true, ctrl)]);
        frame(&ctx, &mut chart, 0.3, vec![Event::PointerMoved(b)]);
        assert!(
            chart.drawings().selection_area().is_some(),
            "the area gesture started"
        );
        change(&mut chart);
        frame(
            &ctx,
            &mut chart,
            0.4,
            vec![Event::PointerMoved(b + vec2(1.0, 1.0))],
        );
        frame(
            &ctx,
            &mut chart,
            0.5,
            vec![Event::PointerMoved(b + vec2(2.0, 2.0))],
        );
        let moved = chart.get_rendered_price_rect() != pane;
        frame(
            &ctx,
            &mut chart,
            0.6,
            vec![
                button(b + vec2(2.0, 2.0), false, ctrl),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        frame(&ctx, &mut chart, 0.7, vec![]);
        frame(&ctx, &mut chart, 0.8, vec![]);
        let stuck = chart.drawings().selection_area().is_some();
        let line = chart.drawings().drawings[0].points.clone();
        let mid = line[0] + (line[1] - line[0]) * 0.5;
        let none = Modifiers::NONE;
        frame(&ctx, &mut chart, 0.9, vec![Event::PointerMoved(mid)]);
        frame(&ctx, &mut chart, 1.0, vec![button(mid, true, none)]);
        frame(&ctx, &mut chart, 1.05, vec![button(mid, false, none)]);
        frame(&ctx, &mut chart, 1.1, vec![]);
        (moved, stuck, chart.drawings().selected_ids().collect())
    }

    /// Both a wider live price axis and a new study pane must leave selection usable on release.
    #[test]
    fn area_gesture_ends_when_the_pane_resizes() {
        let changes: &[fn(&mut crate::widget::Chart)] = &[
            |chart| {
                let t =
                    chrono::TimeZone::timestamp_opt(&chrono::Utc, 1_700_000_000 + 100 * 86_400, 0)
                        .unwrap();
                chart.update(
                    crate::widget::SeriesId::MAIN,
                    crate::model::Bar::new(t, 15.0, 1.0e7, 10.0, 1.0e7, 1.0),
                );
            },
            #[cfg(feature = "studies")]
            |chart| {
                chart.add_indicator(Box::new(crate::studies::RSI::new(5)));
            },
        ];
        let results: Vec<_> = changes.iter().map(area_gesture_with).collect();
        assert_eq!(
            results,
            vec![(true, false, vec![7]); changes.len()],
            "live axis and added pane: (pane moved, gesture still active, selection)"
        );
    }

    /// A release frees input before the next press even when no pane handles that release.
    #[test]
    fn orphaned_area_does_not_capture_the_next_press() {
        use crate::widget::test_frames::frame;
        use egui::{Modifiers, vec2};
        let (ctx, mut chart) = area_chart();
        let from = chart.drawings().drawings[0].points[0] + vec2(0.0, -100.0);
        let to = from + vec2(40.0, 30.0);
        for (step, events) in [
            vec![
                Event::ModifiersChanged(Modifiers::CTRL),
                Event::PointerMoved(from),
            ],
            vec![button(from, true, Modifiers::CTRL)],
            vec![Event::PointerMoved(to)],
        ]
        .into_iter()
        .enumerate()
        {
            frame(&ctx, &mut chart, 0.1 + step as f64 * 0.1, events);
        }
        assert!(chart.drawings().selection_area().is_some());
        for (step, pressed) in [false, true].into_iter().enumerate() {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    time: Some(0.4 + step as f64 * 0.1),
                    events: vec![
                        Event::ModifiersChanged(Modifiers::NONE),
                        button(to, pressed, Modifiers::NONE),
                    ],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(
                        chart.drawings_mut().box_selection_input(ui),
                        !pressed,
                        "primary pressed {pressed}"
                    );
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(chart.drawings().selection_area().is_none());
    }

    /// Main-pane handling must leave a study's release frame to it, or free a removed owner.
    #[cfg(feature = "studies")]
    #[test]
    fn area_gesture_ends_when_its_study_pane_disappears() {
        use crate::widget::test_frames::frame;
        use egui::{Modifiers, vec2};
        for (remove, gone, restore) in [
            (false, false, false),
            (false, true, false),
            (false, false, true),
            (true, false, false),
        ] {
            let (ctx, mut chart) = area_chart();
            let study = chart.add_indicator(Box::new(crate::studies::RSI::new(5)));
            let mut drawing = Drawing::new(8, DrawingToolType::TrendLine);
            drawing.completed = true;
            drawing.pane = Some(study.0);
            drawing.chart_points = vec![ChartPoint::new(30.0, 50.0), ChartPoint::new(70.0, 50.0)];
            chart.drawings_mut().drawings.push(drawing);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let pane = chart.series_pane(study).unwrap();
            let points = chart.drawings().drawings[1].points.clone();
            let from = points[0] + vec2(-20.0, -30.0);
            let near = from + vec2(10.0, 10.0);
            let far = points[1] + vec2(20.0, 30.0);
            let (held, to) = if restore { (far, near) } else { (near, far) };
            let ctrl = Modifiers::CTRL;
            for (i, events) in [
                vec![Event::ModifiersChanged(ctrl), Event::PointerMoved(from)],
                vec![button(from, true, ctrl)],
                vec![Event::PointerMoved(held)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.2 + i as f64 * 0.1, events);
            }
            assert!(chart.drawings().selection_area().is_some());
            assert_eq!(chart.drawings().is_selected(8), restore);
            if gone {
                frame(&ctx, &mut chart, 0.41, vec![Event::PointerGone]);
                frame(&ctx, &mut chart, 0.42, vec![]);
            }
            if restore {
                let layout = chart.pane_layout.clone();
                chart.pane_layout.maximized = Some(crate::widget::PaneId::MAIN);
                frame(&ctx, &mut chart, 0.41, vec![]);
                frame(&ctx, &mut chart, 0.42, vec![]);
                chart.pane_layout = layout;
            }
            if remove {
                chart.remove_pane(pane);
            }
            frame(
                &ctx,
                &mut chart,
                0.5,
                vec![Event::PointerMoved(to), button(to, false, ctrl)],
            );
            if restore {
                assert_eq!(
                    chart.drawings().drawings[1].points,
                    points,
                    "precondition: the restored pane keeps its geometry"
                );
            }
            frame(&ctx, &mut chart, 0.6, vec![]);
            assert!(
                chart.drawings().selection_area().is_none(),
                "removed {remove}"
            );
            assert_eq!(
                chart.drawings().is_selected(8),
                !remove && !restore,
                "the release position counts"
            );
        }
    }

    /// A live bar with the same range arrives on the release frame, moving the line one bar
    /// left but keeping it well inside the rectangle (20 points of slack on each side, 100
    /// above and below). The line must stay selected.
    #[test]
    fn area_selection_survives_a_live_bar_on_release() {
        use crate::widget::test_frames::frame;
        use egui::{Modifiers, vec2};
        for during_drag in [false, true] {
            let (ctx, mut chart) = area_chart();
            let line = chart.drawings().drawings[0].points.clone();
            let from = line[0] + vec2(-20.0, -100.0);
            let to = line[1] + vec2(20.0, 100.0);
            let width = chart.measured_axis_width;
            let ctrl = Modifiers::CTRL;
            frame(
                &ctx,
                &mut chart,
                0.1,
                vec![Event::ModifiersChanged(ctrl), Event::PointerMoved(from)],
            );
            frame(&ctx, &mut chart, 0.2, vec![button(from, true, ctrl)]);
            frame(&ctx, &mut chart, 0.3, vec![Event::PointerMoved(to)]);
            frame(
                &ctx,
                &mut chart,
                0.4,
                vec![Event::PointerMoved(to + vec2(-1.0, 0.0))],
            );
            let selected: Vec<_> = chart.drawings().selected_ids().collect();
            assert_eq!(
                selected,
                vec![7],
                "precondition: the preview selects the line"
            );
            if during_drag {
                let t =
                    chrono::TimeZone::timestamp_opt(&chrono::Utc, 1_700_000_000 + 100 * 86_400, 0)
                        .unwrap();
                chart.update(
                    crate::widget::SeriesId::MAIN,
                    crate::model::Bar::new(t, 10.0, 20.0, 10.0, 20.0, 1.0),
                );
                frame(&ctx, &mut chart, 0.45, vec![Event::PointerMoved(to)]);
                assert!(
                    chart.drawings().is_selected(7),
                    "the moving preview retains the line"
                );
            }
            let index = if during_drag { 101 } else { 100 };
            let t =
                chrono::TimeZone::timestamp_opt(&chrono::Utc, 1_700_000_000 + index * 86_400, 0)
                    .unwrap();
            chart.update(
                crate::widget::SeriesId::MAIN,
                crate::model::Bar::new(t, 10.0, 20.0, 10.0, 20.0, 1.0),
            );
            frame(
                &ctx,
                &mut chart,
                0.5,
                vec![button(to + vec2(-1.0, 0.0), false, ctrl)],
            );
            let moved = chart.drawings().drawings[0].points.clone();
            assert_ne!(moved, line, "precondition: the live bar moved the line");
            let area = Rect::from_two_pos(from, to + vec2(-1.0, 0.0));
            assert!(
                moved.iter().all(|point| area.contains(*point)),
                "precondition: the current line remains inside the area"
            );
            assert_eq!(
                chart.measured_axis_width, width,
                "precondition: the pane kept its width"
            );
            let selected: Vec<_> = chart.drawings().selected_ids().collect();
            assert_eq!(
                selected,
                vec![7],
                "the line inside the released rectangle stays selected"
            );
        }
    }
}
