//! Inline editing for text annotations.

use crate::drawings::manager::TextEditState;
use crate::drawings::{DrawingManager, DrawingToolType};
use crate::localization as l10n;
use crate::model::PostLink;
use egui::{Id, Key, Modifiers, Rect, Ui};

/// Tools whose text can be edited directly over the drawing. Price notes prefix the price;
/// tables, images, posts and ideas retain their separate editors.
fn edits_in_place(tool: DrawingToolType) -> bool {
    use DrawingToolType as T;
    matches!(
        tool,
        T::TextLabel
            | T::AnchoredText
            | T::Note
            | T::AnchoredNote
            | T::FlagNote
            | T::Callout
            | T::Comment
            | T::Signpost
    )
}

pub(super) fn begin_edit(manager: &mut DrawingManager, id: usize, placed: bool) {
    let Some(drawing) = manager
        .drawings
        .iter()
        .find(|d| d.id == id && !d.locked && d.is_visible_on_timeframe(&manager.curr_timeframe))
    else {
        return;
    };
    if !drawing.tool_type.edits_text_inline() {
        return;
    }
    manager.text_editor = Some(TextEditState {
        id,
        draft: if drawing.tool_type == DrawingToolType::Table {
            drawing.table_text()
        } else if let Some(post) = &drawing.post {
            post.link.clone()
        } else {
            drawing.text.clone().unwrap_or_default()
        },
        focus: true,
        opened_frame: None,
        placed,
        inline: edits_in_place(drawing.tool_type),
    });
    manager.select(id);
}

/// `links` says whether the chart has a `Posts` source.
pub(super) fn show_editor(
    ui: &mut Ui,
    manager: &mut DrawingManager,
    chart_id: Id,
    pane: Rect,
    links: bool,
) {
    let Some(mut editor) = manager.text_editor.take() else {
        return;
    };
    let Some(drawing) = manager.drawings.iter().find(|d| {
        d.id == editor.id && !d.locked && d.is_visible_on_timeframe(&manager.curr_timeframe)
    }) else {
        return;
    };
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
    if editor.inline {
        let content = !matches!(
            drawing.tool_type,
            DrawingToolType::AnchoredNote | DrawingToolType::FlagNote
        );
        let old = drawing.text.clone();
        let shown = editor.shown_text(ui.ctx());
        let caret = drawing.text_box_colors(ui.ctx()).text;
        let mut outside = false;
        if let Some((pos, galley)) = manager.painted_text(editor.id, &shown)
            && let Some(section) = galley.job.sections.first()
        {
            let format = egui::TextFormat {
                color: egui::Color32::TRANSPARENT,
                ..section.format.clone()
            };
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                let job =
                    egui::text::LayoutJob::simple_format(text.as_str().to_owned(), format.clone());
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            egui::Area::new(id)
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .constrain(false)
                .movable(false)
                .show(ui.ctx(), |ui| {
                    ui.set_clip_rect(pane);
                    if ui.is_sizing_pass() {
                        editor.focus = request_focus;
                        ui.ctx().request_discard("inline text editor layout");
                    }
                    ui.visuals_mut().text_cursor.stroke.color = caret;
                    ui.visuals_mut().selection.bg_fill =
                        ui.visuals().selection.bg_fill.gamma_multiply(0.5);
                    let field = ui.add(
                        egui::TextEdit::multiline(&mut editor.draft)
                            .id(id.with("field"))
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::ZERO)
                            .desired_width(galley.size().x)
                            .desired_rows(1)
                            .layouter(&mut layouter),
                    );
                    if request_focus {
                        field.request_focus();
                    }
                    if field.changed() {
                        ui.ctx().request_discard("drawing text changed");
                        ui.ctx().request_repaint();
                    }
                    outside = frame > opened_frame && field.clicked_elsewhere();
                });
        } else {
            // Off-pane drafts keep their editor and its initial focus request until visible.
            editor.focus = request_focus;
        }
        if cancel || commit || outside {
            ui.memory_mut(|memory| memory.surrender_focus(id.with("field")));
            let text = (!editor.draft.trim().is_empty()).then_some(editor.draft);
            if editor.placed {
                match (cancel, text) {
                    (false, Some(text)) => manager.finish_new_text(editor.id, Some(text)),
                    _ if content => manager.finish_new_text(editor.id, None),
                    _ => {}
                }
            } else if !cancel && text != old {
                manager.edit_drawing(editor.id, |d| d.text = text);
            }
        } else {
            manager.text_editor = Some(editor);
        }
        return;
    }
    let label = usize::from(drawing.tool_type == DrawingToolType::PriceNote);
    let Some(mut pos) = drawing
        .points
        .get(label)
        .or(drawing.points.first())
        .copied()
    else {
        return;
    };
    let width = 260.0_f32.min(pane.width());
    pos.x = pos.x.min(pane.right() - width).max(pane.left());
    let table = drawing.tool_type == DrawingToolType::Table;
    // A table editor shows about six rows of cells above its buttons.
    let height = if table { 240.0_f32 } else { 160.0 }.min(pane.height());
    pos.y = pos.y.min(pane.bottom() - height).max(pane.top());
    let font_size = crate::styles::typography::font_size(drawing.font_size);
    let hint = match drawing.tool_type {
        DrawingToolType::Image => "Image address (https://… or file://…)",
        DrawingToolType::Tweet if links => {
            "A post address, or the author on the first line, then the post"
        }
        DrawingToolType::Idea if links => {
            "An idea address, or the title on the first line, then the idea"
        }
        DrawingToolType::Tweet => "Author on the first line, then the post",
        DrawingToolType::Idea => "Title on the first line, then the idea",
        _ => "",
    };
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
                let field = egui::ScrollArea::vertical()
                    .max_height((height - 48.0).max(1.0))
                    .show(ui, |ui| {
                        if table {
                            crate::drawings::Drawing::edit_table(ui, &mut editor.draft, field_id)
                        } else {
                            ui.add(
                                egui::TextEdit::multiline(&mut editor.draft)
                                    .id(field_id)
                                    .hint_text(l10n::text(ui.ctx(), hint))
                                    .font(egui::FontId::proportional(font_size))
                                    .desired_width((width - 16.0).max(1.0))
                                    .desired_rows(3),
                            )
                        }
                    })
                    .inner;
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
        let address = editor.draft.trim();
        let unchanged = match &drawing.post {
            Some(post) => post.link.trim() == address,
            None => drawing.text.as_deref() == Some(editor.draft.as_str()),
        };
        if !cancel && !unchanged {
            // With a source, an address typed on its own links a post or idea; anything else
            // is content typed by hand, which replaces the link.
            let post = (links
                && matches!(
                    drawing.tool_type,
                    DrawingToolType::Tweet | DrawingToolType::Idea
                )
                && is_address(address))
            .then(|| PostLink::new(address));
            manager.edit_drawing(editor.id, |drawing| {
                drawing.text = post.is_none().then_some(editor.draft);
                drawing.post = post;
            });
        }
    } else {
        manager.text_editor = Some(editor);
    }
}

/// Whether `draft` is one address with a scheme, such as `https://…`, and nothing else.
fn is_address(draft: &str) -> bool {
    !draft.contains(char::is_whitespace)
        && draft.split_once("://").is_some_and(|(scheme, rest)| {
            !rest.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        })
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
            for (tool, width) in [
                DrawingToolType::PriceNote,
                DrawingToolType::Table,
                DrawingToolType::Image,
                DrawingToolType::Tweet,
                DrawingToolType::Idea,
            ]
            .into_iter()
            .flat_map(|tool| [120.0, 320.0].map(|width| (tool, width)))
            {
                let ctx = Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                let pane = Rect::from_min_size(pos2(20.0, 20.0), vec2(width, 180.0));
                let mut manager = DrawingManager::new();
                let mut drawing = Drawing::new(7, tool);
                drawing.points.push(pane.right_bottom() - vec2(10.0, 10.0));
                drawing.text = Some("Long draft\n".repeat(30));
                manager.drawings.push(drawing);
                super::begin_edit(&mut manager, 7, false);
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
                        |ui| super::show_editor(ui, manager, chart_id, pane, false),
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
                    "{tool:?}, {width}, dark {dark}: {rect:?}"
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

    /// Completing an annotation focuses its editor. Inline text joins the placement undo step;
    /// double-click reopens it, and Escape discards the draft without deselecting.
    #[test]
    fn placement_and_double_click_edit_annotation_text() {
        for (tool, pane) in [
            DrawingToolType::TextLabel,
            DrawingToolType::AnchoredText,
            DrawingToolType::Note,
            DrawingToolType::AnchoredNote,
            DrawingToolType::Callout,
            DrawingToolType::Table,
            DrawingToolType::Image,
            DrawingToolType::Tweet,
            DrawingToolType::Idea,
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
            if matches!(tool, DrawingToolType::Callout | DrawingToolType::Note) {
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
            let expected = if tool == DrawingToolType::Table {
                step(&mut chart, vec![Event::Text("First".into())]);
                step(&mut chart, vec![key(Key::Tab, Modifiers::NONE)]);
                step(&mut chart, vec![Event::Text("Second".into())]);
                "First\tSecond\t\n\t\t\n\t\t\n\t\t"
            } else {
                step(
                    &mut chart,
                    vec![
                        Event::Text("First".into()),
                        key(Key::Enter, Modifiers::NONE),
                        Event::Text("Second".into()),
                    ],
                );
                "First\nSecond"
            };
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
                Some(expected),
                "{tool:?}"
            );
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            let mut restored = crate::Chart::new(crate::model::BarData::default());
            restored.load_drawings(&saved).unwrap();
            assert_eq!(
                restored.drawings().drawings[0].text.as_deref(),
                Some(expected)
            );
            assert!(chart.undo());
            if super::edits_in_place(tool) {
                assert!(chart.drawings().drawings.is_empty());
            } else {
                assert_eq!(chart.drawings().drawings[0].text, None);
            }
            assert!(chart.redo());
            step(&mut chart, vec![]);
            let id = chart.drawings().drawings[0].id;
            let points = &chart.drawings().drawings[0].points;
            let on_text = match tool {
                DrawingToolType::Callout | DrawingToolType::Note => {
                    points[1] + egui::vec2(10.0, 10.0)
                }
                // Inside the pin's head, outside its hole.
                DrawingToolType::AnchoredNote => points[0] - egui::vec2(0.0, 12.0),
                _ => points[0] + egui::vec2(10.0, 10.0),
            };
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
                Some(expected),
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
            assert_eq!(chart.drawings().drawings[0].text.as_deref(), Some(expected));
        }
    }

    /// Drafts and carets align in the input frame. Empty placements leave no undo step,
    /// while empty edits retain existing anchors, on both themes and in narrow windows.
    #[test]
    fn inline_editor_types_into_the_drawing_and_drops_empty_text() {
        use egui::{Color32, RawInput, Shape};
        for (tool, dark, width) in [
            DrawingToolType::TextLabel,
            DrawingToolType::Callout,
            DrawingToolType::Comment,
            DrawingToolType::AnchoredNote,
        ]
        .into_iter()
        .flat_map(|tool| {
            [
                (tool, false, 320.0),
                (tool, true, 320.0),
                (tool, false, 800.0),
                (tool, true, 800.0),
            ]
        }) {
            let ctx = Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            crate::localization::set_translator(&ctx, |s| {
                (s == "Add text").then(|| "Ajouter".into())
            });
            let mut chart = chart(&[10.0], &[20.0]);
            let mut time = 0.0;
            let mut step = |chart: &mut crate::Chart, events| {
                time += 0.1;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            chart.show(ui);
                        });
                    },
                );
                let painted = std::mem::take(&mut output.shapes)
                    .into_iter()
                    .map(|s| s.shape)
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
                painted
            };
            let key = |key, modifiers| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            step(&mut chart, vec![]);
            let rect = chart.get_rendered_price_rect();
            let from = rect.min + rect.size() * egui::vec2(0.4, 0.4);
            let to = rect.min + rect.size() * egui::vec2(0.6, 0.7);
            for (draft, cancel) in [
                ("", true),
                (" \n", false),
                ("Discard", true),
                ("Intervening", false),
                ("Intervening", true),
                ("", false),
            ] {
                chart.drawings_mut().set_active_tool(Some(tool));
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
                let painted = step(&mut chart, vec![]);
                let texts: Vec<_> = painted
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    texts
                        .iter()
                        .filter(|t| t.galley.text() == "Ajouter" && {
                            let c = t.galley.job.sections[0].format.color;
                            c.r() == c.g() && c.g() == c.b() && c.a() > 0
                        })
                        .count(),
                    1,
                    "{tool:?} placeholder"
                );
                assert!(!texts.iter().any(
                    |t| t.galley.text().contains("Ctrl/Cmd+Enter") || t.galley.text() == "Text"
                ));
                if draft.is_empty() && cancel {
                    let on_placeholder = texts
                        .iter()
                        .find(|t| t.galley.text() == "Ajouter")
                        .unwrap()
                        .visual_bounding_rect()
                        .center();
                    step(
                        &mut chart,
                        vec![
                            Event::PointerMoved(on_placeholder),
                            button(on_placeholder, true),
                        ],
                    );
                    step(&mut chart, vec![button(on_placeholder, false)]);
                    assert!(
                        chart.drawings().text_editor.is_some(),
                        "{tool:?} placeholder is part of the field"
                    );
                }
                if draft == "Intervening" {
                    let grid = chart.config.show_grid;
                    chart.user_action(|chart| chart.config_mut().show_grid = !grid);
                    if cancel && tool != DrawingToolType::AnchoredNote {
                        step(&mut chart, vec![key(Key::Escape, Modifiers::NONE)]);
                        assert!(chart.drawings().drawings.is_empty());
                        assert!(chart.undo());
                        assert_eq!(chart.config.show_grid, grid);
                        assert!(
                            chart.drawings().drawings.is_empty(),
                            "undo must not resurrect empty text"
                        );
                        assert!(!chart.can_undo());
                        assert!(chart.redo());
                        assert!(chart.drawings().drawings.is_empty());
                        assert!(chart.undo());
                        continue;
                    }
                    step(&mut chart, vec![Event::Text("Hi".into())]);
                    assert_eq!(
                        chart
                            .drawings()
                            .text_editor
                            .as_ref()
                            .map(|e| e.draft.as_str()),
                        Some("Hi"),
                        "{tool:?} typing after a chart action"
                    );
                    step(&mut chart, vec![key(Key::Enter, Modifiers::COMMAND)]);
                    assert_eq!(chart.drawings().drawings[0].text.as_deref(), Some("Hi"));
                    assert!(chart.undo());
                    assert_eq!(chart.drawings().drawings[0].text, None);
                    assert!(chart.undo());
                    assert_eq!(chart.config.show_grid, grid);
                    assert!(chart.undo());
                    assert!(chart.drawings().drawings.is_empty());
                    assert!(!chart.can_undo());
                    continue;
                }
                if cancel || !draft.is_empty() {
                    if !draft.is_empty() {
                        step(&mut chart, vec![Event::Text(draft.into())]);
                    }
                    let close = if cancel {
                        key(Key::Escape, Modifiers::NONE)
                    } else {
                        key(Key::Enter, Modifiers::CTRL)
                    };
                    step(&mut chart, vec![close]);
                    if tool == DrawingToolType::AnchoredNote {
                        assert_eq!(chart.drawings().drawings.len(), 1);
                        assert_eq!(chart.drawings().drawings[0].text, None);
                        assert!(chart.undo());
                    }
                    assert!(
                        chart.drawings().drawings.is_empty(),
                        "{tool:?} cancelled placement"
                    );
                    assert!(!chart.can_undo());
                    continue;
                }
                chart.drawings_mut().drawings[0].font_size = 24.0;
                chart.drawings_mut().drawings[0].italic = true;
                step(&mut chart, vec![]);
                let painted = step(&mut chart, vec![Event::Text("Hi".into())]);
                let texts: Vec<_> = painted
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Text(t) if t.galley.text() == "Hi" => Some(t),
                        _ => None,
                    })
                    .collect();
                assert_eq!(texts.len(), 2, "{tool:?} drawing and editor");
                let drawing = texts
                    .iter()
                    .find(|t| t.galley.job.sections[0].format.color.a() == 255)
                    .unwrap();
                let editor = texts
                    .iter()
                    .find(|t| t.galley.job.sections[0].format.color == Color32::TRANSPARENT)
                    .unwrap();
                assert!(
                    drawing.pos.distance(editor.pos) <= 0.5,
                    "{tool:?}: {:?} vs {:?}",
                    drawing.pos,
                    editor.pos
                );
                assert!(editor.galley.job.sections[0].format.italics);
                assert_eq!(editor.galley.job.sections[0].format.font_id.size, 24.0);
                let hit = if tool == DrawingToolType::AnchoredNote {
                    chart.drawings().drawings[0].points[0] - egui::vec2(0.0, 12.0)
                } else {
                    drawing.visual_bounding_rect().center()
                };
                step(&mut chart, vec![key(Key::Enter, Modifiers::COMMAND)]);
                assert_eq!(chart.drawings().drawings[0].text.as_deref(), Some("Hi"));
                assert!(chart.undo());
                assert!(chart.drawings().drawings.is_empty());
                assert!(!chart.can_undo());
                assert!(chart.redo());
                assert_eq!(chart.drawings().drawings[0].text.as_deref(), Some("Hi"));
                step(&mut chart, vec![]);
                for _ in 0..2 {
                    step(
                        &mut chart,
                        vec![Event::PointerMoved(hit), button(hit, true)],
                    );
                    step(&mut chart, vec![button(hit, false)]);
                }
                step(
                    &mut chart,
                    vec![
                        key(Key::A, Modifiers::COMMAND),
                        key(Key::Backspace, Modifiers::NONE),
                    ],
                );
                step(&mut chart, vec![key(Key::Enter, Modifiers::COMMAND)]);
                assert_eq!(
                    chart.drawings().drawings.len(),
                    1,
                    "{tool:?} keeps its anchors"
                );
                assert_eq!(chart.drawings().drawings[0].text, None);
                assert!(chart.undo());
                assert_eq!(chart.drawings().drawings[0].text.as_deref(), Some("Hi"));
            }
        }
    }

    /// A price label shows the price and nothing typed, so placing one asks for no text.
    #[test]
    fn placing_a_price_label_opens_no_editor() {
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::PriceLabel));
        let pos = chart.get_rendered_price_rect().center();
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
        frame(&ctx, &mut chart, 0.2, vec![button(true)]);
        frame(&ctx, &mut chart, 0.3, vec![button(false)]);
        assert_eq!(chart.drawings().drawings.len(), 1);
        assert!(chart.drawings().text_editor.is_none());
    }

    /// Pane-relative anchors survive pan and zoom, and scale with pane size rather than data. A
    /// pin saved with one appears where it was shown, and then stays on its bar.
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
            if tool != DrawingToolType::AnchoredText {
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
                manager.update_drag_handle(
                    pos2(175.0, 405.0),
                    |x| x,
                    f64::from,
                    |price, from, to| price + f64::from(to - from),
                );
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

        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let mut pin = Drawing::new(7, DrawingToolType::AnchoredNote);
        pin.chart_points.push(ChartPoint::new(50.0, 15.0));
        pin.screen_anchor = Some([0.25, 0.75]);
        chart.drawings_mut().drawings.push(pin);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let rect = chart.get_rendered_price_rect();
        let shown = rect.min + egui::vec2(0.25 * rect.width(), 0.75 * rect.height());
        let pin = &chart.drawings().drawings[0];
        assert_eq!(pin.screen_anchor, None);
        assert!(pin.points[0].distance(shown) < 0.01, "{:?}", pin.points[0]);
        let (x, anchor) = (pin.points[0].x, pin.chart_points[0]);
        chart.set_visible_bars(70);
        chart.set_start_idx(20);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let pin = &chart.drawings().drawings[0];
        assert_ne!(pin.points[0].x, x);
        assert_eq!(pin.chart_points[0], anchor);
    }
}
