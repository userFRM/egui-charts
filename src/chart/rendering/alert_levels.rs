//! Interactive alert levels. Changes are host intents; rendering never changes alert status.

use chrono::{DateTime, Utc};
use egui::{Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::model::{Alert, AlertCondition, AlertSource, AlertStatus};

/// Draw threshold markers and optional pane-wide lines, returning a replacement after a drag
/// or a marker menu change. Submit it through the same service as panel edits so storage and undo
/// retain the display choices. A fired marker blinks for a minute unless its blinking option is
/// disabled, and stays drawn that long even with `active_only` set, so a one-shot firing is seen.
/// Supply only alerts for this pane, resolving symbol or watchlist membership in the host.
/// With `active_only` unset, triggered, disabled and expired alerts are drawn dimmed.
/// Call after drawing the chart, then submit the result through the host's alerts service.
/// Non-price sources require the host to resolve their own pane and coordinates.
/// `format` supplies the instrument's precision and currency for labels.
pub fn render_alert_levels<'a>(
    ui: &mut Ui,
    mapping: &ChartMapping,
    alerts: impl IntoIterator<Item = &'a Alert>,
    color: Color32,
    labels: bool,
    active_only: bool,
    format: impl Fn(f64) -> String,
) -> Option<Alert> {
    let mut changed = None;
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let now = Utc::now();
    for alert in alerts
        .into_iter()
        .filter(|a| !active_only || a.status == AlertStatus::Active || blinks(a, now))
    {
        let Some(id) = &alert.id else { continue };
        let active = alert.status == AlertStatus::Active;
        let color = if active {
            color
        } else {
            color.gamma_multiply(0.5)
        };
        let rules = std::iter::once((&alert.source, &alert.condition)).chain(
            alert
                .additional_conditions
                .iter()
                .map(|r| (&r.source, &r.condition)),
        );
        for (index, (source, condition)) in rules.enumerate() {
            if *source != AlertSource::Price {
                continue;
            }
            let levels = match condition {
                AlertCondition::EnteringChannel { lower, upper }
                | AlertCondition::ExitingChannel { lower, upper }
                | AlertCondition::InsideChannel { lower, upper }
                | AlertCondition::OutsideChannel { lower, upper } => vec![*lower, *upper],
                _ => condition.level().into_iter().collect(),
            };
            for (edge, level) in levels
                .into_iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
            {
                let key = ui.scope_id().with(("alert_level", &id.0, index, edge));
                let staged = ui
                    .ctx()
                    .data(|data| data.get_temp::<f64>(key))
                    .unwrap_or(level);
                let y = mapping.price_to_y(staged);
                if !y.is_finite() || !mapping.is_y_visible(y) {
                    continue;
                }
                let marker_x = mapping.rect.right() - 12.0;
                let caption = |value| {
                    let name = if alert.name.is_empty() {
                        "Alert"
                    } else {
                        &alert.name
                    };
                    format!("{name}  {}", format(value))
                };
                let mut galley = labels.then(|| {
                    painter.layout_no_wrap(
                        caption(staged),
                        FontId::proportional(12.0),
                        Color32::PLACEHOLDER,
                    )
                });
                let label_bounds = |y, size: egui::Vec2| {
                    Rect::from_min_size(
                        pos2(
                            (marker_x - size.x - 16.0).max(mapping.rect.left()),
                            y - size.y - 4.0,
                        ),
                        size + vec2(8.0, 2.0),
                    )
                };
                let mut hit = Rect::from_min_max(
                    pos2(marker_x - 12.0, y - 9.0),
                    pos2(mapping.rect.right(), y + 9.0),
                )
                .intersect(mapping.rect);
                if let Some(galley) = &galley {
                    hit = hit.union(label_bounds(y, galley.size()).intersect(mapping.rect));
                }
                // A widget sensing both only starts dragging past the click distance, so they split.
                // The click widgets go on top, or the drag widgets would hide them from hit tests.
                let mut response = ui.interact(hit, key, Sense::drag());
                let mut menu = ui.interact(hit, key.with("menu"), Sense::click());
                if alert.extend_line {
                    let line = Rect::from_min_max(
                        pos2(mapping.rect.left(), y - 6.0),
                        pos2(mapping.rect.right(), y + 6.0),
                    )
                    .intersect(mapping.rect);
                    response |= ui.interact(line, key.with("line"), Sense::drag());
                    menu |= ui.interact(line, key.with("line_menu"), Sense::click());
                }
                let response = response.on_hover_ui(|ui| {
                    ui.label(&alert.symbol);
                    if !alert.name.is_empty() {
                        ui.label(&alert.name);
                    }
                    ui.label(crate::localization::text(ui.ctx(), condition.to_string()));
                    ui.label(crate::localization::text(ui.ctx(), "Drag alert level"));
                });
                menu.context_menu(|ui| {
                    let mut updated = alert.clone();
                    let line = ui
                        .checkbox(
                            &mut updated.extend_line,
                            crate::localization::text(ui.ctx(), "Line across chart"),
                        )
                        .changed();
                    let blink = ui
                        .checkbox(
                            &mut updated.blink_on_trigger,
                            crate::localization::text(ui.ctx(), "Blink after firing"),
                        )
                        .changed();
                    if line || blink {
                        changed = Some(updated);
                    }
                });
                let mut value = staged;
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(pointer) = response.interact_pointer_pos()
                    && let Some(origin) = ui.input(|input| input.pointer.press_origin())
                {
                    // Move by the pointer's travel, so a press beside the level does not change it.
                    let moved = pointer.y - origin.y;
                    value = if moved == 0.0 {
                        level
                    } else {
                        mapping.y_to_price(
                            (mapping.price_to_y(level) + moved)
                                .clamp(mapping.rect.top(), mapping.rect.bottom()),
                        )
                    };
                    if value.is_finite() {
                        ui.ctx().data_mut(|data| data.insert_temp(key, value));
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    ui.ctx().data_mut(|data| data.remove::<f64>(key));
                    if value.is_finite() && value != level {
                        let mut updated = alert.clone();
                        let condition = if index == 0 {
                            &mut updated.condition
                        } else {
                            &mut updated.additional_conditions[index - 1].condition
                        };
                        match condition {
                            AlertCondition::Crossing(v)
                            | AlertCondition::CrossingUp(v)
                            | AlertCondition::CrossingDown(v)
                            | AlertCondition::Above(v)
                            | AlertCondition::Below(v) => *v = value,
                            AlertCondition::EnteringChannel { lower, upper }
                            | AlertCondition::ExitingChannel { lower, upper }
                            | AlertCondition::InsideChannel { lower, upper }
                            | AlertCondition::OutsideChannel { lower, upper } => {
                                if edge == 0 {
                                    *lower = value;
                                } else {
                                    *upper = value;
                                }
                            }
                            _ => continue,
                        }
                        if updated.is_valid() {
                            changed = Some(updated);
                        }
                    }
                }
                let y = mapping.price_to_y(value);
                if alert.extend_line {
                    painter.hline(mapping.rect.x_range(), y, Stroke::new(1.0, color));
                }
                let blinking = blinks(alert, now);
                let marker_color = if blinking {
                    let bright = ui.input(|input| (input.time * 2.0).floor() as i64 % 2 == 0);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                    if bright {
                        color
                    } else {
                        color.gamma_multiply(0.2)
                    }
                } else {
                    color
                };
                let center = pos2(marker_x, y);
                painter.circle_filled(center, 6.0, marker_color);
                #[cfg(feature = "icons")]
                crate::icons::icons::ALERTS
                    .as_image_tinted(vec2(10.0, 10.0), ui.visuals().panel_fill)
                    .paint_at(ui, Rect::from_center_size(center, vec2(10.0, 10.0)));
                #[cfg(not(feature = "icons"))]
                {
                    painter.line_segment(
                        [center - vec2(0.0, 3.0), center + vec2(0.0, 1.0)],
                        Stroke::new(1.5, ui.visuals().panel_fill),
                    );
                    painter.circle_filled(center + vec2(0.0, 3.0), 0.8, ui.visuals().panel_fill);
                }
                if let Some(mut galley) = galley.take() {
                    if value != staged {
                        galley = painter.layout_no_wrap(
                            caption(value),
                            FontId::proportional(12.0),
                            Color32::PLACEHOLDER,
                        );
                    }
                    let text_color = if active {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    let text_color = if blinking && marker_color != color {
                        text_color.gamma_multiply(0.2)
                    } else {
                        text_color
                    };
                    let label = label_bounds(y, galley.size());
                    painter.rect_filled(label, 2.0, ui.visuals().panel_fill);
                    painter.galley_with_override_text_color(
                        label.min + vec2(4.0, 1.0),
                        galley,
                        text_color,
                    );
                }
            }
        }
    }
    changed
}

/// Whether a firing is recent enough to blink. Paused and expired alerts stay still.
fn blinks(alert: &Alert, now: DateTime<Utc>) -> bool {
    alert.blink_on_trigger
        && matches!(alert.status, AlertStatus::Active | AlertStatus::Triggered)
        && alert
            .last_triggered_at
            .is_some_and(|fired| (0..60).contains(&(now - fired).num_seconds()))
}

/// Resolve Alt+A at a hovered chart price without handling keys owned by a field or popup.
/// The host decides whether to open a dialog or save a default alert at the returned price.
pub fn alert_shortcut_price(ui: &mut Ui, mapping: &ChartMapping) -> Option<f64> {
    let pointer = ui.ctx().pointer_hover_pos()?;
    if !ui.rect_contains_pointer(mapping.rect)
        || ui.ctx().egui_wants_keyboard_input()
        || egui::Popup::is_any_open(ui.ctx())
    {
        return None;
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::A)) {
        let price = mapping.y_to_price(pointer.y);
        return price.is_finite().then_some(price);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AlertId;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::theme::Theme;

    /// The shapes of one frame at `time`. A fresh context also exercises reload, without
    /// transient animation state.
    fn frame(
        alert: &Alert,
        theme: &Theme,
        time: f64,
        active_only: bool,
    ) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        #[cfg(feature = "icons")]
        crate::icons::install_loader(&ctx);
        crate::theme::apply_to_egui(&ctx, theme);
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ui| {
                let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(400.0, 200.0));
                let mapping = ChartMapping::new(
                    rect,
                    6.0,
                    0,
                    0,
                    0.0,
                    PriceMap::new(
                        PriceTransform::default(),
                        PriceRange::new(90.0, 110.0),
                        rect.top(),
                        rect.bottom(),
                        false,
                    ),
                );
                render_alert_levels(
                    ui,
                    &mapping,
                    [alert],
                    Color32::GOLD,
                    false,
                    active_only,
                    |p| p.to_string(),
                );
            },
        );
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        shapes
    }

    fn marker(shapes: &[egui::epaint::ClippedShape]) -> Option<&egui::epaint::CircleShape> {
        shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.radius == 6.0 => Some(circle),
            _ => None,
        })
    }

    #[test]
    fn restored_marker_options_control_extension_and_blinking_independently() {
        for theme in [Theme::light(), Theme::dark()] {
            for line in [false, true] {
                for blink in [false, true] {
                    let stored = serde_json::json!({
                        "id": "saved", "symbol": "SAMPLE", "condition": {"Above": 100.0},
                        "last_triggered_at": Utc::now(),
                        "extend_line": line, "blink_on_trigger": blink
                    });
                    let alert: Alert = serde_json::from_value(stored).unwrap();
                    let alert: Alert =
                        serde_json::from_str(&serde_json::to_string(&alert).unwrap()).unwrap();
                    let mut colors = Vec::new();
                    for time in [0.0, 0.75] {
                        let shapes = frame(&alert, &theme, time, false);
                        let lines = shapes.iter().filter(|shape| matches!(&shape.shape,
                            egui::Shape::LineSegment { points, .. } if points[0] == pos2(20.0, 120.0) && points[1] == pos2(420.0, 120.0)
                        )).count();
                        assert_eq!(lines, usize::from(line));
                        let marker =
                            marker(&shapes).expect("a marker remains without the line or labels");
                        // Halfway through [90,110] maps to halfway down the 200-point pane.
                        assert_eq!(marker.center, pos2(408.0, 120.0));
                        colors.push(marker.fill);
                    }
                    assert_eq!(colors[0] != colors[1], blink);
                }
            }
        }
    }

    #[test]
    fn a_firing_blinks_for_a_minute_and_outlasts_the_active_filter() {
        use AlertStatus::{Active, Disabled, Expired, Triggered};
        let theme = Theme::light();
        for (status, fired, enabled, active_only, drawn, blinks) in [
            (Active, Some(1), true, true, true, true),
            // A one-shot firing would otherwise vanish in the frame it fires.
            (Triggered, Some(1), true, true, true, true),
            (Triggered, Some(1), false, true, false, false),
            (Triggered, Some(120), true, true, false, false),
            (Triggered, Some(120), true, false, true, false),
            (Active, Some(120), true, true, true, false),
            (Active, None, true, true, true, false),
            (Disabled, Some(1), true, false, true, false),
            (Disabled, Some(1), true, true, false, false),
            (Expired, Some(1), true, false, true, false),
        ] {
            let mut alert = Alert::new("SAMPLE", AlertCondition::Above(100.0));
            alert.id = Some(AlertId("saved".into()));
            alert.status = status;
            alert.blink_on_trigger = enabled;
            alert.last_triggered_at = fired.map(|s| Utc::now() - chrono::TimeDelta::seconds(s));
            let case = format!("{status:?} {fired:?} {enabled} {active_only}");
            let colors: Vec<_> = [0.0, 0.75]
                .map(|time| marker(&frame(&alert, &theme, time, active_only)).map(|m| m.fill))
                .into();
            assert_eq!(colors[0].is_some(), drawn, "{case}");
            assert_eq!(colors[0] != colors[1], blinks, "{case}");
        }
    }

    #[test]
    fn inactive_levels_follow_the_filter_and_stay_dimmed() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            #[cfg(feature = "icons")]
            crate::icons::install_loader(&ctx);
            crate::theme::apply_to_egui(&ctx, &theme);
            for status in [
                AlertStatus::Active,
                AlertStatus::Triggered,
                AlertStatus::Disabled,
                AlertStatus::Expired,
            ] {
                let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(100.0));
                alert.id = Some(AlertId("level".into()));
                alert.status = status;
                alert.extend_line = true;
                alert.blink_on_trigger = false;
                for active_only in [false, true] {
                    let mut output = ctx.run_ui(Default::default(), |ui| {
                        let rect = ui.max_rect();
                        let map = ChartMapping::new(
                            rect,
                            6.0,
                            0,
                            0,
                            0.0,
                            PriceMap::new(
                                PriceTransform::default(),
                                PriceRange::new(90.0, 110.0),
                                rect.top(),
                                rect.bottom(),
                                false,
                            ),
                        );
                        render_alert_levels(
                            ui,
                            &map,
                            [&alert],
                            Color32::GOLD,
                            true,
                            active_only,
                            |price| price.to_string(),
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let visible = !active_only || status == AlertStatus::Active;
                    let lines: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match s.shape {
                            egui::Shape::LineSegment { points, stroke }
                                if points[0].y == points[1].y =>
                            {
                                Some(stroke.color)
                            }
                            _ => None,
                        })
                        .collect();
                    let expected = if status == AlertStatus::Active {
                        Color32::GOLD
                    } else {
                        Color32::GOLD.gamma_multiply(0.5)
                    };
                    assert_eq!(
                        lines,
                        if visible { vec![expected] } else { vec![] },
                        "{status:?}, active_only={active_only}"
                    );
                    let label = shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(text) if text.galley.text() == "Alert  100"));
                    assert_eq!(label, visible);
                }
            }
        }
    }

    #[test]
    fn short_quick_drag_moves_the_level() {
        use egui::PointerButton::{Primary, Secondary};
        let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(400.0, 200.0));
        // One point is 0.1 on this 200-point pane from 110 to 90, so level 100 sits at y = 120.
        let mapping = ChartMapping::new(
            rect,
            6.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                rect.top(),
                rect.bottom(),
                false,
            ),
        );
        let mut alert = Alert::new("SYM", AlertCondition::Above(100.0));
        alert.id = Some(AlertId("1".into()));
        alert.extend_line = true;
        // x = 408 is the marker and x = 100 the line.
        for (button, x, from, to, level, menu) in [
            (Primary, 408.0, 120.0, 124.0, Some(99.6), false),
            (Primary, 100.0, 120.0, 124.0, Some(99.6), false),
            // A press beside the level keeps its distance from the pointer, without a jump.
            (Primary, 408.0, 123.0, 127.0, Some(99.6), false),
            (Primary, 408.0, 123.0, 123.0, None, false),
            (Secondary, 408.0, 120.0, 120.0, None, true),
            (Secondary, 100.0, 120.0, 120.0, None, true),
            // A hand moving during a secondary click opens the menu and leaves the level.
            (Secondary, 408.0, 120.0, 123.0, None, true),
        ] {
            let ctx = egui::Context::default();
            let (start, end) = (pos2(x, from), pos2(x, to));
            let press = |pos, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            };
            let steps = [
                vec![egui::Event::PointerMoved(start)],
                vec![press(start, true)],
                vec![egui::Event::PointerMoved(end)],
                vec![press(end, false)],
                vec![],
            ];
            let mut changed = None;
            for (step, events) in steps.into_iter().enumerate() {
                let input = egui::RawInput {
                    time: Some(step as f64 * 0.05),
                    events,
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    changed = changed.take().or(render_alert_levels(
                        ui,
                        &mapping,
                        [&alert],
                        Color32::GOLD,
                        false,
                        false,
                        |p| p.to_string(),
                    ));
                })
                .drop_without_applying_deltas();
            }
            let case = format!("{button:?} at {x} from {from} to {to}");
            let moved = changed.and_then(|alert| alert.condition.level());
            assert_eq!(moved.map(|v| (v * 10.0).round() / 10.0), level, "{case}");
            assert_eq!(egui::Popup::is_any_open(&ctx), menu, "{case}");
        }
    }
}
