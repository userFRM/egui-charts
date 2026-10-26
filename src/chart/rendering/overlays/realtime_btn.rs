//! Scroll-to-latest button layout, interaction and painting.

use crate::config::{ChartConfig, RealtimeButtonPos};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, Pos2, Rect, Response, Stroke, Ui, Vec2};

pub(crate) fn realtime_button_rect(
    pane: Rect,
    size: (f32, f32),
    position: RealtimeButtonPos,
    navigation: Option<Rect>,
) -> Option<Rect> {
    let size = Vec2::new(size.0, size.1);
    let bounds = pane.shrink(10.0);
    if !size.is_finite() || size.min_elem() <= 0.0 || !bounds.is_positive() {
        return None;
    }
    let mut rect = Rect::from_min_size(calculate_button_position(pane, size, 10.0, position), size);
    if let Some(nav) = navigation
        && rect.intersects(nav.expand(4.0))
    {
        rect = rect.translate(Vec2::new(0.0, nav.top() - 4.0 - rect.bottom()));
    }
    bounds.contains_rect(rect).then_some(rect)
}

pub(crate) fn realtime_button(
    ui: &Ui,
    rect: Rect,
    chart_id: egui::Id,
    config: &ChartConfig,
) -> Response {
    let response = ui.interact(
        rect,
        chart_id.with("jump_to_latest"),
        egui::Sense::click_and_drag(),
    );
    let theme = ui.active_theme();
    let resolve = |color, fallback| {
        if color == Color32::PLACEHOLDER {
            fallback
        } else {
            color
        }
    };
    let bg = if response.hovered() || response.is_pointer_button_down_on() {
        resolve(config.realtime_button_hover_color, theme.ui.btn_bg_hover)
    } else {
        resolve(config.realtime_button_color, theme.ui.panel_bg)
    };
    let ink = resolve(config.realtime_button_text_color, theme.ui.text);
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect(
        rect,
        theme.radius.sm,
        bg,
        Stroke::new(1.0, theme.ui.border),
        egui::StrokeKind::Inside,
    );
    if let Some(text) = config.realtime_button_text.as_deref() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(theme.text.label),
            ink,
        );
    } else {
        let center = rect.center();
        for x in [-4.0, 2.0] {
            painter.add(egui::Shape::line(
                vec![
                    center + Vec2::new(x - 2.0, -4.0),
                    center + Vec2::new(x + 2.0, 0.0),
                    center + Vec2::new(x - 2.0, 4.0),
                ],
                Stroke::new(1.5, ink),
            ));
        }
    }
    crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.sm);
    let label = l10n::text(ui.ctx(), "Scroll to the most recent bar");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

fn calculate_button_position(
    price_rect: Rect,
    btn_size: Vec2,
    margin: f32,
    position: RealtimeButtonPos,
) -> Pos2 {
    match position {
        RealtimeButtonPos::TopLeft => {
            Pos2::new(price_rect.min.x + margin, price_rect.min.y + margin)
        }
        RealtimeButtonPos::TopCenter => Pos2::new(
            price_rect.center().x - btn_size.x / 2.0,
            price_rect.min.y + margin,
        ),
        RealtimeButtonPos::TopRight => Pos2::new(
            price_rect.max.x - btn_size.x - margin,
            price_rect.min.y + margin,
        ),
        RealtimeButtonPos::BottomLeft => Pos2::new(
            price_rect.min.x + margin,
            price_rect.max.y - btn_size.y - margin,
        ),
        RealtimeButtonPos::BottomCenter => Pos2::new(
            price_rect.center().x - btn_size.x / 2.0,
            price_rect.max.y - btn_size.y - margin,
        ),
        RealtimeButtonPos::BottomRight => Pos2::new(
            price_rect.max.x - btn_size.x - margin,
            price_rect.max.y - btn_size.y - margin,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Event, PointerButton, Shape};

    #[test]
    fn scroll_to_latest_hits_its_painted_rect_and_preserves_zoom() {
        for (position, size, anchor) in [
            (RealtimeButtonPos::BottomRight, (28.0, 28.0), (1.0, 1.0)),
            (RealtimeButtonPos::BottomLeft, (76.0, 32.0), (0.0, 1.0)),
            (RealtimeButtonPos::BottomCenter, (76.0, 32.0), (0.5, 1.0)),
            (RealtimeButtonPos::TopLeft, (76.0, 32.0), (0.0, 0.0)),
            (RealtimeButtonPos::TopCenter, (76.0, 32.0), (0.5, 0.0)),
            (RealtimeButtonPos::TopRight, (76.0, 32.0), (1.0, 0.0)),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            let mut theme = crate::theme::Theme::light();
            theme.text.label = 19.0;
            theme.radius.sm = 7.0;
            chart.set_theme(&ctx, &theme);
            chart.config_mut().realtime_button_text = Some("Latest".into());
            let fill = Color32::from_rgb(29, 71, 123);
            let hover = Color32::from_rgb(39, 81, 133);
            chart.config_mut().realtime_button_pos = position;
            chart.config_mut().realtime_button_size = size;
            chart.config_mut().realtime_button_color = fill;
            chart.config_mut().realtime_button_hover_color = hover;
            frame(&ctx, &mut chart, 0.0, vec![]);
            let (spacing, live) = chart.get_time_scale_state();
            let button = |painted: Vec<Shape>, color| {
                painted.into_iter().find_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == color => Some(rect.rect),
                    _ => None,
                })
            };
            assert!(button(shapes(&ctx, &mut chart, 0.1, vec![]), fill).is_none());
            // Future whitespace can also leave the latest bar outside the pane.
            chart.scroll_to_position(
                2.0 * chart.get_rendered_price_rect().width() / spacing,
                false,
            );
            assert!(button(shapes(&ctx, &mut chart, 0.15, vec![]), fill).is_some());
            assert!(chart.state().visible_data().is_empty());
            // Even a small scroll back from the configured margin offers the control.
            chart.scroll_to_position(live - 1.0, false);
            assert!(button(shapes(&ctx, &mut chart, 0.2, vec![]), fill).is_some());
            chart.scroll_to_position(-20.0, false);
            let painted = shapes(&ctx, &mut chart, 0.3, vec![]);
            let pane = chart.get_rendered_price_rect();
            let expected = Rect::from_min_size(
                pane.min
                    + Vec2::new(
                        10.0 + anchor.0 * (pane.width() - 20.0 - size.0),
                        10.0 + anchor.1 * (pane.height() - 20.0 - size.1),
                    ),
                Vec2::new(size.0, size.1),
            );
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    Shape::Text(text) if text.galley.text() == "Latest"
                        && text.galley.job.sections.iter()
                            .all(|section| section.format.font_id.size == theme.text.label)
                )),
                "custom text uses the active theme"
            );
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    Shape::Rect(rect) if rect.fill == fill
                        && rect.corner_radius == egui::CornerRadius::from(theme.radius.sm)
                )),
                "button radius uses the active theme"
            );
            assert_eq!(button(painted, fill), Some(expected), "{position:?}");
            let pos = expected.center();
            assert_eq!(
                button(
                    shapes(&ctx, &mut chart, 0.4, vec![Event::PointerMoved(pos)]),
                    hover
                ),
                Some(expected),
                "hover must use the drawn rectangle: {position:?}",
            );
            for (time, pressed) in [(0.5, true), (0.6, false)] {
                frame(
                    &ctx,
                    &mut chart,
                    time,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            // A 400 ms linear animation from -20 to the configured live offset.
            for (time, offset) in [(0.7, -20.0), (0.9, (-20.0 + live) / 2.0), (1.11, live)] {
                frame(&ctx, &mut chart, time, vec![]);
                let (actual_spacing, actual_offset) = chart.get_time_scale_state();
                assert!(
                    (actual_offset - offset).abs() < 1e-3,
                    "{position:?}: {actual_offset} != {offset}"
                );
                assert_eq!(actual_spacing, spacing);
            }
            assert!(
                button(
                    shapes(&ctx, &mut chart, 1.2, vec![Event::PointerGone]),
                    fill
                )
                .is_none()
            );
            chart.scroll_to_position(-20.0, false);
            chart.config_mut().show_realtime_btn = false;
            for (time, events) in [
                (1.3, vec![Event::PointerMoved(pos)]),
                (
                    1.4,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    }],
                ),
                (
                    1.5,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                ),
                (2.0, vec![]),
            ] {
                assert!(button(shapes(&ctx, &mut chart, time, events), fill).is_none());
            }
            assert_eq!(
                chart.get_time_scale_state(),
                (spacing, -20.0),
                "hidden button cannot scroll"
            );
            // A fixed right edge can clamp the configured margin to zero.
            let mut options = chart.chart_options().clone();
            options.time_scale.fix_right_edge = true;
            chart = chart.with_chart_options(options);
            chart.config_mut().show_realtime_btn = true;
            frame(&ctx, &mut chart, 2.1, vec![]);
            chart.scroll_to_real_time(false);
            assert!(
                button(
                    shapes(&ctx, &mut chart, 2.2, vec![Event::PointerGone]),
                    fill
                )
                .is_none()
            );
        }
    }

    #[test]
    fn scroll_to_latest_stays_above_overlays_in_both_themes_and_small_panes() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for size in [
                Vec2::new(1440.0, 900.0),
                Vec2::new(420.0, 220.0),
                Vec2::new(180.0, 140.0),
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[100.0], &[110.0]);
                chart.set_theme(&ctx, &theme);
                chart.config_mut().navigation_buttons = crate::config::NavigationButtons::Always;
                chart.scroll_to_position(-20.0, false);
                let mut painted = Vec::new();
                for time in [0.0, 0.1] {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            time: Some(time),
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                chart.show(ui);
                            });
                        },
                    );
                    painted = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                }
                let pane = chart.get_rendered_price_rect();
                let (index, button) = painted
                    .iter()
                    .enumerate()
                    .find_map(|(index, shape)| match &shape.shape {
                        Shape::Rect(rect)
                            if rect.rect.size() == Vec2::splat(28.0)
                                && rect.fill == theme.ui.panel_bg =>
                        {
                            Some((index, rect.rect))
                        }
                        _ => None,
                    })
                    .expect("default 28-point themed button");
                assert!(pane.contains_rect(button));
                assert_eq!(button.right(), pane.right() - 10.0);
                for (other_index, other) in painted.iter().enumerate() {
                    if let Shape::Rect(rect) = &other.shape
                        && rect.rect.width() > 200.0
                        && rect.rect.height() < 60.0
                        && (rect.rect.center().x - pane.center().x).abs() < 1.0
                    {
                        assert!(
                            !button.intersects(rect.rect),
                            "navigation overlaps at {size:?}"
                        );
                        assert!(index > other_index, "button paints after navigation");
                    }
                }
                let chevrons = painted[index + 1..].iter().filter(|shape| matches!(
                    &shape.shape, Shape::Path(path) if path.points.len() == 3
                        && path.stroke.color == egui::epaint::ColorMode::Solid(theme.ui.text)
                        && path.points.iter().all(|point| button.contains(*point))
                )).count();
                assert_eq!(chevrons, 2, "two readable chevrons at {size:?}");
            }
        }
    }
}
