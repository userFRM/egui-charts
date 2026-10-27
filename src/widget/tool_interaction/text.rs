//! Inline editing for text annotations.

use crate::drawings::manager::TextEditState;
use crate::drawings::{DrawingManager, DrawingToolType};
use crate::localization as l10n;
use egui::{Id, Key, Modifiers, Rect, Ui};

pub(super) fn begin_edit(manager: &mut DrawingManager, id: usize) {
    let Some(drawing) = manager
        .drawings
        .iter()
        .find(|d| d.id == id && !d.locked && d.is_visible_on_timeframe(&manager.curr_timeframe))
    else {
        return;
    };
    if !matches!(
        drawing.tool_type,
        DrawingToolType::TextLabel
            | DrawingToolType::AnchoredText
            | DrawingToolType::Note
            | DrawingToolType::AnchoredNote
            | DrawingToolType::Callout
            | DrawingToolType::Comment
            | DrawingToolType::FlagNote
            | DrawingToolType::Signpost
            | DrawingToolType::PriceLabel
            | DrawingToolType::PriceNote
            | DrawingToolType::Table
    ) {
        return;
    }
    manager.text_editor = Some(TextEditState {
        id,
        draft: if drawing.tool_type == DrawingToolType::Table {
            drawing.table_text()
        } else {
            drawing.text.clone().unwrap_or_default()
        },
        focus: true,
        opened_frame: None,
    });
    manager.select(id);
}

pub(super) fn show_editor(ui: &mut Ui, manager: &mut DrawingManager, chart_id: Id, pane: Rect) {
    let Some(mut editor) = manager.text_editor.take() else {
        return;
    };
    let Some(drawing) = manager.drawings.iter().find(|d| {
        d.id == editor.id && !d.locked && d.is_visible_on_timeframe(&manager.curr_timeframe)
    }) else {
        return;
    };
    let Some(mut pos) = drawing
        .points
        .get(usize::from(drawing.tool_type == DrawingToolType::Callout))
        .copied()
    else {
        return;
    };
    let width = 260.0_f32.min(pane.width());
    pos.x = pos.x.min(pane.right() - width).max(pane.left());
    let height = 160.0_f32.min(pane.height());
    pos.y = pos.y.min(pane.bottom() - height).max(pane.top());
    let id = chart_id.with(("drawing_text", editor.id));
    let (cancel, commit) = ui.input_mut(|input| {
        (
            input.consume_key(Modifiers::NONE, Key::Escape),
            input.consume_key(Modifiers::COMMAND, Key::Enter)
                || input.consume_key(Modifiers::CTRL, Key::Enter),
        )
    });
    let frame = ui.ctx().cumulative_frame_nr();
    let opened_frame = *editor.opened_frame.get_or_insert(frame);
    let request_focus = std::mem::take(&mut editor.focus);
    let font_size = drawing.font_size;
    let table = drawing.tool_type == DrawingToolType::Table;
    let mut outside = false;
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .constrain_to(pane)
        .movable(false)
        .show(ui.ctx(), |ui| {
            ui.set_clip_rect(pane);
            if ui.is_sizing_pass() {
                editor.focus = request_focus;
                ui.ctx().request_discard("inline text editor layout");
            }
            let popup = egui::Frame::popup(ui.style());
            let response = popup.show(ui, |ui| {
                ui.set_width((width - popup.total_margin().sum().x).max(1.0));
                let field_id = id.with("field");
                if request_focus {
                    ui.memory_mut(|memory| memory.request_focus(field_id));
                }
                let field = if table {
                    crate::drawings::Drawing::edit_table(
                        ui,
                        &mut editor.draft,
                        field_id,
                        (height - 80.0).max(1.0),
                    )
                } else {
                    egui::ScrollArea::vertical()
                        .max_height((height - 48.0).max(1.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut editor.draft)
                                    .id(field_id)
                                    .font(egui::FontId::proportional(font_size))
                                    .desired_width((width - 16.0).max(1.0))
                                    .desired_rows(3),
                            )
                        })
                        .inner
                };
                if request_focus {
                    field.request_focus();
                }
                ui.small(l10n::text(
                    ui.ctx(),
                    "Ctrl/Cmd+Enter to save, Esc to cancel",
                ));
            });
            outside = frame > opened_frame && response.response.clicked_elsewhere();
        });
    if cancel || commit || outside {
        ui.memory_mut(|memory| memory.surrender_focus(id.with("field")));
        if !cancel && drawing.text.as_deref() != Some(editor.draft.as_str()) {
            manager.edit_drawing(editor.id, |drawing| drawing.text = Some(editor.draft));
        }
    } else {
        manager.text_editor = Some(editor);
    }
}

#[cfg(test)]
mod tests {
    use crate::drawings::DrawingToolType;
    use crate::widget::test_frames::{chart, frame};
    use egui::{Context, Event, Key, Modifiers, PointerButton, pos2};

    /// Narrow editors stay within the pane, and clicks on their chrome keep the draft open.
    #[test]
    fn editor_bounds_and_inside_clicks() {
        use crate::drawings::{Drawing, DrawingManager};
        use egui::{Id, RawInput, Rect, vec2};
        for dark in [false, true] {
            for width in [120.0, 320.0] {
                let ctx = Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                let pane = Rect::from_min_size(pos2(20.0, 20.0), vec2(width, 180.0));
                let mut manager = DrawingManager::new();
                let mut drawing = Drawing::new(7, DrawingToolType::TextLabel);
                drawing.points.push(pane.right_bottom() - vec2(10.0, 10.0));
                drawing.text = Some("Long draft\n".repeat(30));
                manager.drawings.push(drawing);
                super::begin_edit(&mut manager, 7);
                let chart_id = Id::NULL.with("editor_bounds");
                let mut time = 0.0;
                let mut step = |manager: &mut DrawingManager, events| {
                    time += 0.1;
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                pos2(0.0, 0.0),
                                vec2(600.0, 400.0),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ui| super::show_editor(ui, manager, chart_id, pane),
                    )
                    .drop_without_applying_deltas();
                };
                for _ in 0..3 {
                    step(&mut manager, vec![]);
                }
                let area =
                    egui::AreaState::load(&ctx, chart_id.with(("drawing_text", 7_usize))).unwrap();
                let rect = Rect::from_min_size(area.left_top_pos(), area.size.unwrap());
                assert!(
                    pane.expand(1.0).contains_rect(rect),
                    "{width}, dark {dark}: {rect:?}"
                );
                let pos = rect.center_bottom() - vec2(0.0, 8.0);
                for pressed in [true, false] {
                    step(
                        &mut manager,
                        vec![
                            Event::PointerMoved(pos),
                            Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                }
                assert!(
                    manager.text_editor.is_some(),
                    "inside click must keep editing"
                );
                assert!(!manager.can_undo());
            }
        }
    }

    /// Completing a text, note or callout focuses an inline field. Commit is one undoable
    /// edit; double-click reopens it, and Escape discards the draft without deselecting.
    #[test]
    fn placement_and_double_click_edit_annotation_text() {
        for (tool, pane) in [
            DrawingToolType::TextLabel,
            DrawingToolType::AnchoredText,
            DrawingToolType::Note,
            DrawingToolType::AnchoredNote,
            DrawingToolType::Callout,
        ]
        .into_iter()
        .flat_map(|tool| [(tool, false), (tool, true)])
        {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            if pane {
                chart.add_indicator(Box::new(
                    crate::studies::CustomIndicator::new("Pane", |bars| {
                        vec![crate::studies::IndicatorValue::Single(15.0); bars.len()]
                    })
                    .with_overlay(false),
                ));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.drawings_mut().set_active_tool(Some(tool));
            let rect = if pane {
                chart.get_rendered_indicator_panes()[0].chart_rect
            } else {
                chart.get_rendered_price_rect()
            };
            let from = rect.min + rect.size() * egui::vec2(0.4, 0.4);
            let to = rect.min + rect.size() * egui::vec2(0.6, 0.7);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let key = |key, modifiers| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
            let mut time = 0.0;
            let mut step = |chart: &mut crate::widget::Chart, events| {
                time += 0.1;
                frame(&ctx, chart, time, events);
            };
            step(&mut chart, vec![Event::PointerMoved(from)]);
            step(&mut chart, vec![button(from, true)]);
            if tool == DrawingToolType::Callout {
                step(
                    &mut chart,
                    vec![Event::PointerMoved(from + egui::vec2(8.0, 0.0))],
                );
                step(&mut chart, vec![Event::PointerMoved(to)]);
                step(&mut chart, vec![button(to, false)]);
            } else {
                step(&mut chart, vec![button(from, false)]);
            }
            assert_eq!(chart.drawings().drawings.len(), 1, "{tool:?}");
            step(
                &mut chart,
                vec![
                    Event::Text("First".into()),
                    key(Key::Enter, Modifiers::NONE),
                    Event::Text("Second".into()),
                ],
            );
            if tool == DrawingToolType::Note {
                let outside = pos2(700.0, 450.0);
                step(
                    &mut chart,
                    vec![Event::PointerMoved(outside), button(outside, true)],
                );
                step(&mut chart, vec![button(outside, false)]);
            } else {
                step(&mut chart, vec![key(Key::Enter, Modifiers::COMMAND)]);
            }
            assert_eq!(
                chart.drawings().drawings[0].text.as_deref(),
                Some("First\nSecond"),
                "{tool:?}"
            );
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].text, None);
            assert!(chart.redo());
            step(&mut chart, vec![]);
            let id = chart.drawings().drawings[0].id;
            let on_text = chart.drawings().drawings[0].points
                [usize::from(tool == DrawingToolType::Callout)]
                + egui::vec2(10.0, 10.0);
            assert_eq!(
                chart.drawings().hit_test(on_text),
                Some(id),
                "text body for {tool:?}"
            );
            for _ in 0..2 {
                step(
                    &mut chart,
                    vec![Event::PointerMoved(on_text), button(on_text, true)],
                );
                step(&mut chart, vec![button(on_text, false)]);
            }
            assert!(
                chart.take_indicator_settings_request().is_none(),
                "drawing double click owns its pane"
            );
            step(
                &mut chart,
                vec![
                    key(Key::A, Modifiers::COMMAND),
                    Event::Text("Discard".into()),
                ],
            );
            step(&mut chart, vec![key(Key::Escape, Modifiers::NONE)]);
            assert_eq!(
                chart.drawings().drawings[0].text.as_deref(),
                Some("First\nSecond"),
                "{tool:?}"
            );
            assert_eq!(chart.drawings().sel_drawing(), Some(id), "{tool:?}");
            chart
                .drawings_mut()
                .edit_drawing(id, |drawing| drawing.locked = true);
            for _ in 0..2 {
                step(
                    &mut chart,
                    vec![Event::PointerMoved(on_text), button(on_text, true)],
                );
                step(&mut chart, vec![button(on_text, false)]);
            }
            step(
                &mut chart,
                vec![
                    Event::Text("Locked".into()),
                    key(Key::Enter, Modifiers::COMMAND),
                ],
            );
            assert_eq!(
                chart.drawings().drawings[0].text.as_deref(),
                Some("First\nSecond")
            );
        }
    }

    /// Pane-relative anchors survive pan and zoom, and scale with pane size rather than data.
    #[test]
    fn anchored_annotations_keep_their_pane_position() {
        use crate::drawings::{ChartPoint, Drawing};
        for tool in [
            DrawingToolType::AnchoredText,
            DrawingToolType::AnchoredNote,
            DrawingToolType::TextLabel,
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let mut drawing = Drawing::new(7, tool);
            drawing.chart_points.push(ChartPoint::new(50.0, 15.0));
            chart.drawings_mut().drawings.push(drawing);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let original = chart.drawings().drawings[0].points[0];
            chart.set_visible_bars(70);
            chart.set_start_idx(20);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let moved = chart.drawings().drawings[0].points[0];
            if tool == DrawingToolType::TextLabel {
                assert_ne!(moved, original);
            } else {
                assert_eq!(moved, original, "{tool:?}");
                let mut fixed = Drawing::new(9, tool);
                fixed.completed = true;
                fixed.points.push(pos2(100.0, 200.0));
                fixed.chart_points.push(ChartPoint::new(0.0, 0.0));
                fixed.update_pane_coords(egui::Rect::from_min_size(
                    pos2(40.0, 60.0),
                    egui::vec2(800.0, 400.0),
                ));
                // (100-40)/800 = .075, (200-60)/400 = .35; on the new pane: (130,360).
                fixed.update_pane_coords(egui::Rect::from_min_size(
                    pos2(100.0, 80.0),
                    egui::vec2(400.0, 800.0),
                ));
                assert_eq!(fixed.points[0], pos2(130.0, 360.0));
                let mut manager = crate::drawings::DrawingManager::new();
                manager.drawings.push(fixed);
                let pane = egui::Rect::from_min_size(pos2(100.0, 80.0), egui::vec2(400.0, 800.0));
                manager.update_pane_anchors(pane);
                manager.select(9);
                manager.start_drag_body(9, pos2(135.0, 365.0));
                manager.update_drag_handle(pos2(175.0, 405.0), |x| x, f64::from);
                manager.end_drag_handle();
                manager.update_all_screen_coords(|x| x, |y| y as f32);
                manager.update_pane_anchors(pane);
                assert_eq!(manager.drawings[0].points[0], pos2(170.0, 400.0));
                assert!(manager.undo());
                manager.update_all_screen_coords(|x| x, |y| y as f32);
                manager.update_pane_anchors(pane);
                assert_eq!(manager.drawings[0].points[0], pos2(130.0, 360.0));
                let saved = chart.save_drawings().unwrap().to_json().unwrap();
                chart.load_drawings(&saved).unwrap();
                chart.set_visible_bars(50);
                frame(&ctx, &mut chart, 0.2, vec![]);
                assert_eq!(chart.drawings().drawings[0].points[0], original);
            }
        }
    }
}
