use crate::localization as l10n;
use crate::theme::ThemeExt;
#[cfg(not(feature = "icons"))]
use egui::FontId;
use egui::{Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// The row outline and its hide, settings, remove and more-actions buttons. Returns the first
/// three buttons and, when the more-actions button is clicked or the row secondary-clicked, the
/// corner of the study menu.
pub(crate) fn study_controls(
    ui: &mut Ui,
    row: Rect,
    controls: Rect,
    id: egui::Id,
    visible: bool,
) -> ([Response; 3], Option<Pos2>) {
    let theme = ui.active_theme();
    let touch_id = egui::Id::unique("legend_touch_input");
    let input_touch = ui.input(|input| {
        if input.any_touches()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Touch { .. }))
        {
            Some(true)
        } else if input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        {
            Some(false)
        } else {
            None
        }
    });
    let touch = ui.ctx().data_mut(|data| {
        let touch = input_touch.unwrap_or_else(|| data.get_temp::<bool>(touch_id).unwrap_or(false));
        data.insert_temp(touch_id, touch);
        touch
    });
    let row_response = ui.interact(row, id.with("row"), Sense::click());
    let controls_label = l10n::text(ui.ctx(), "Study controls");
    row_response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &controls_label));
    let mut menu = row_response
        .secondary_clicked()
        .then(|| row_response.interact_pointer_pos())
        .flatten();
    let clicked_elsewhere = row_response.clicked_elsewhere();
    let clicked = row_response.clicked();
    let selected = ui.ctx().data_mut(|data| {
        let mut selected = data.get_temp::<bool>(id).unwrap_or(false);
        if clicked_elsewhere || !touch {
            selected = false;
        } else if clicked {
            selected = true;
        }
        data.insert_temp(id, selected);
        selected
    });
    // Keep controls focusable while Tab transfers focus from their row.
    let focused = row_response.has_focus()
        || row_response.lost_focus()
        || (0..4_usize).any(|index| ui.memory(|memory| memory.has_focus(id.with(index))));
    let hovered = !touch
        && ui.input(|input| input.pointer.hover_pos().is_some())
        && ui.rect_contains_pointer(row);
    let revealed = selected || focused || hovered;
    if revealed {
        ui.painter()
            .rect_filled(controls, theme.radius.sm, theme.chart.bg);
        ui.painter().rect_stroke(
            row.union(controls.expand2(Vec2::new(0.0, 2.0))),
            theme.radius.sm,
            Stroke::new(1.0, theme.ui.accent),
            egui::StrokeKind::Inside,
        );
    }
    let buttons: [Response; 4] = std::array::from_fn(|index| {
        let rect = Rect::from_min_size(
            controls.min + Vec2::new(index as f32 * 20.0, 0.0),
            Vec2::splat(20.0),
        )
        .intersect(controls);
        let label = l10n::text(
            ui.ctx(),
            match index {
                0 if visible => "Hide study",
                0 => "Show study",
                1 => "Study settings",
                2 => "Remove study",
                _ => "More actions",
            },
        );
        let response = ui.interact(
            rect,
            id.with(index),
            if revealed {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if !revealed {
            return response;
        }
        let response = response.on_hover_text(&label);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, theme.radius.sm, theme.ui.btn_bg_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        #[cfg(feature = "icons")]
        {
            let icon = match index {
                0 if visible => &crate::icons::icons::EYE,
                0 => &crate::icons::icons::EYE_HIDE,
                1 => &crate::icons::icons::SETTINGS,
                2 => &crate::icons::icons::TRASH,
                _ => &crate::icons::icons::UI_MORE_HORIZONTAL,
            };
            icon.as_image_tinted(Vec2::splat(14.0), theme.ui.text)
                .paint_at(ui, rect.shrink(3.0));
        }
        #[cfg(not(feature = "icons"))]
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            [if visible { "−" } else { "+" }, "...", "×", "•••"][index],
            FontId::proportional(14.0),
            theme.ui.text,
        );
        response
    });
    let [hide, settings, remove, more] = buttons;
    if more.clicked() {
        menu = Some(more.rect.left_bottom());
    }
    ([hide, settings, remove], menu)
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::super::{Bar, Indicator, Legend};
    use super::*;
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Key, PointerButton, Shape};

    #[test]
    fn study_controls_reveal_on_touch_and_dismiss_outside() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            // A translator that changes every key must reach the control labels.
            l10n::set_translator(&ctx, |english| Some(format!("{english}!")));
            let row = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(220.0, 20.0));
            let controls = Rect::from_min_size(Pos2::new(150.0, 10.0), Vec2::new(80.0, 20.0));
            let settings = controls.min + Vec2::new(30.0, 10.0);
            let frame = |events| {
                let mut clicked = [false; 3];
                let mut menu = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let (buttons, opened) =
                            study_controls(ui, row, controls, egui::Id::unique("study"), true);
                        clicked = buttons.map(|response| response.clicked());
                        menu = opened;
                    },
                );
                // An accent outline 24 px tall, 2 px past the 20 px row on each side.
                let outline = Rect::from_min_size(Pos2::new(10.0, 8.0), Vec2::new(220.0, 24.0));
                let painted = output.shapes.iter().any(|s| match &s.shape {
                    Shape::Rect(r) => r.rect == outline && r.stroke.color == theme.ui.accent,
                    _ => false,
                });
                let buttons_filled = output.shapes.iter().any(|s| {
                    matches!(&s.shape, Shape::Rect(r) if r.rect == controls && r.fill == theme.chart.bg)
                });
                assert_eq!(
                    buttons_filled, painted,
                    "only revealed controls mask the chart"
                );
                let focused_label = output.platform_output.events.iter().find_map(|event| {
                    if let egui::output::OutputEvent::FocusGained(info) = event {
                        info.label.clone()
                    } else {
                        None
                    }
                });
                output.drop_without_applying_deltas();
                (painted, clicked, focused_label, menu)
            };
            let touch = |pos, phase| Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(0),
                phase,
                pos,
                force: None,
            };
            let tap = |pos| {
                frame(vec![
                    touch(pos, egui::TouchPhase::Start),
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ]);
                frame(vec![
                    touch(pos, egui::TouchPhase::End),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                    Event::PointerGone,
                ])
            };
            assert!(!frame(vec![Event::PointerGone]).0);
            let first = tap(settings);
            assert_eq!(first.1, [false; 3], "the first tap only reveals controls");
            let second = tap(settings);
            assert_eq!(
                second.1,
                [false, true, false],
                "the second tap opens settings"
            );
            tap(row.left_center() + Vec2::new(10.0, 0.0));
            assert!(frame(vec![]).0, "touch selection must survive pointer exit");
            tap(Pos2::new(250.0, 100.0));
            assert!(!frame(vec![]).0, "outside tap must dismiss controls");
            assert!(
                frame(vec![Event::PointerMoved(row.center())]).0,
                "mouse hover reveals controls"
            );
            // A secondary click on the row opens the menu at the pointer.
            let pos = row.left_center() + Vec2::new(20.0, 0.0);
            frame(vec![Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                let clicked = frame(vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                }]);
                assert_eq!(clicked.3, (!pressed).then_some(pos));
            }
            assert!(!frame(vec![Event::PointerGone]).0);
            for (index, label) in [
                "Study controls",
                "Hide study",
                "Study settings",
                "Remove study",
                "More actions",
            ]
            .into_iter()
            .enumerate()
            {
                let focused = frame(vec![Event::Key {
                    key: Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]);
                assert!(focused.0, "keyboard focus must reveal {label}");
                assert_eq!(focused.2, Some(format!("{label}!")));
                assert!(frame(vec![]).0, "focused controls must stay visible");
                let activated = frame(vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]);
                assert_eq!(
                    activated.1,
                    std::array::from_fn(|button| index == button + 1)
                );
                // The menu opens below the more-actions button.
                assert_eq!(
                    activated.3,
                    (index == 4).then(|| controls.min + Vec2::new(60.0, 20.0))
                );
            }

            // A wrapped row is selectable over its text, even when the controls wrap below it.
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let bar = Bar::new(
                Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                100.0,
                110.0,
                90.0,
                105.0,
                1000.0,
            );
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(
                crate::studies::CustomIndicator::new("A long overlay study title", |_| Vec::new())
                    .with_overlay(true),
            )];
            let pos = Pos2::new(130.0, 16.0);
            let frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let mut legend = Legend::new("SAMPLE", "1h", &bar);
                        legend.show_title = false;
                        legend.show_ohlc = false;
                        legend.show_change = false;
                        legend.show_change_percent = false;
                        legend.show_volume = false;
                        legend.studies = &studies;
                        legend.show(ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(160.0, 400.0)));
                    },
                );
                let selected = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                    Shape::Rect(rect) if rect.stroke.width > 0.0 && rect.rect.contains(pos))
                });
                output.drop_without_applying_deltas();
                selected
            };
            frame(vec![]);
            frame(vec![
                touch(pos, egui::TouchPhase::Start),
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            frame(vec![
                touch(pos, egui::TouchPhase::End),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
                Event::PointerGone,
            ]);
            assert!(
                frame(vec![]),
                "the full wrapped study row must respond to a tap"
            );
        }
    }
}
