use crate::studies::{MACD, RSI};
use crate::widget::test_frames::chart;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

#[cfg(feature = "drawings")]
#[test]
fn position_tags_in_a_series_pane_follow_its_display_format() {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::{SeriesDisplayFormat, SeriesFormat};
    use crate::scales::CustomPriceFormatter;
    use crate::widget::test_frames::{frame, shapes};
    use crate::widget::{SeriesId, SeriesOptions};
    use std::sync::Arc;

    for (theme, width) in [
        (crate::theme::Theme::light(), 760.0),
        (crate::theme::Theme::dark(), 320.0),
    ] {
        for host in ["empty", "none", "series", "chart"] {
            let ctx = Context::default();
            let mut chart = chart(&[90.0], &[90.0, 110.0]);
            chart.set_theme(&ctx, &theme);
            chart.set_size(Some(Vec2::new(width, 580.0)));
            let id = chart.add_series(SeriesOptions::default());
            chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
            chart.series_style_mut(id).unwrap().display_format = Some(SeriesDisplayFormat {
                kind: SeriesFormat::Price,
                precision: 2,
                min_move: 0.25,
            });
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(id, pane));
            if host != "none" {
                let formatter = Arc::new(CustomPriceFormatter::new(|value| {
                    format!("host {value:.2}")
                }));
                if host == "series" {
                    chart.set_series_formatter(id, Some(formatter));
                } else {
                    chart.set_price_formatter(Some(formatter));
                }
            }
            if host == "empty" {
                chart.set_preserve_empty_pane(pane, true);
                chart.remove_series(id);
                let mut options = chart.pane_axis_options(pane).unwrap();
                options.hide_unused = false;
                chart.set_pane_axis_options(pane, options);
                let scale = chart.pane_price_scale_mut(pane, crate::model::SeriesScale::Right);
                scale.set_range(90.0, 110.0);
            }
            let mut drawing = Drawing::new(1, DrawingToolType::LongPos);
            drawing.chart_points = vec![ChartPoint::new(30.0, 100.1), ChartPoint::new(60.0, 106.0)];
            drawing.completed = true;
            drawing.pane = Some(pane.0);
            chart.drawings_mut().drawings.push(drawing);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let panel = chart
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == pane)
                .unwrap()
                .rect;
            let tag_texts = |painted: &[egui::Shape]| -> Vec<String> {
                painted
                    .iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Text(text)
                            if text.pos.x >= plot.right()
                                && panel.y_range().contains(text.pos.y)
                                && painted.iter().any(|shape| {
                                    matches!(shape,
                                        egui::Shape::Rect(tag) if tag.rect.contains(text.pos)
                                            && tag.rect.right() > plot.right()
                                            && tag.rect.height() <= text.galley.size().y * 2.0
                                    )
                                }) =>
                        {
                            Some(text.galley.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect()
            };
            let visible = tag_texts(&painted);
            chart.drawings_mut().drawings[0].price_axis_label = false;
            let hidden = tag_texts(&shapes(&ctx, &mut chart, 0.2, vec![]));
            for (price, expected) in [(100.1, "100.00"), (106.0, "106.00"), (94.2, "94.25")] {
                let expected = if host != "none" {
                    format!("host {price:.2}")
                } else {
                    expected.into()
                };
                assert!(
                    visible.contains(&expected),
                    "{host}: {expected}: {visible:?}"
                );
                assert!(
                    !hidden.contains(&expected),
                    "disabled tag {expected}: {hidden:?}"
                );
            }
        }
    }
}

fn pointer(
    ctx: &Context,
    chart: &mut crate::widget::Chart,
    pos: Pos2,
    pressed: Option<bool>,
) -> egui::CursorIcon {
    let mut events = vec![Event::PointerMoved(pos)];
    if let Some(pressed) = pressed {
        events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    let time = ctx.input(|input| input.time) + 0.1;
    crate::widget::test_frames::frame(ctx, chart, time, events)
}

fn click(ctx: &Context, chart: &mut crate::widget::Chart, pos: Pos2) {
    for pressed in [None, Some(true), Some(false), None] {
        pointer(ctx, chart, pos, pressed);
    }
}

fn drag(ctx: &Context, chart: &mut crate::widget::Chart, from: Pos2, to: Pos2) {
    pointer(ctx, chart, from, None);
    pointer(ctx, chart, from, Some(true));
    pointer(ctx, chart, from + (to - from) * 0.5, None);
    pointer(ctx, chart, to, None);
    pointer(ctx, chart, to, Some(false));
    pointer(ctx, chart, to, None);
}

/// Maximized panes retain shared navigation and leave the hidden price range alone.
#[test]
fn maximized_panes_keep_pointer_navigation() {
    use crate::widget::test_frames::{frame, shapes};
    for (theme, width, left, above) in [
        (crate::theme::Theme::light(), 760.0, false, false),
        (crate::theme::Theme::dark(), 760.0, true, true),
        (crate::theme::Theme::light(), 320.0, true, false),
        (crate::theme::Theme::dark(), 320.0, false, true),
    ] {
        let ctx = Context::default();
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        chart.set_theme(&ctx, &theme);
        chart.set_size(Some(Vec2::new(width, 580.0)));
        chart.config.show_right_axis = !left;
        chart.config.show_left_axis = left;
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
        chart.chart_options.crosshair.horz_line_style = crate::model::LineStyle::Solid;
        chart.chart_options.crosshair.horz_line_color = egui::Color32::BLUE;
        let study = chart.add_indicator(Box::new(RSI::new(14)));
        frame(&ctx, &mut chart, 0.0, Vec::new());
        if above {
            chart.move_pane(chart.series_pane(study).unwrap(), 0);
        }
        chart.pane_layout.maximized = chart.series_pane(study);
        frame(&ctx, &mut chart, 0.1, Vec::new());
        frame(&ctx, &mut chart, 0.2, Vec::new());
        let pane = chart.get_rendered_indicator_panes()[0].clone();
        let pos = pane.chart_rect.center();
        let before = chart.state.time_scale().right_offset_precise();
        assert_eq!(chart.state.time_scale().bar_spacing(), 6.0);
        chart.state.price_scale_mut().set_range(90.0, 120.0);
        drag(&ctx, &mut chart, pos, pos + egui::vec2(40.0, 20.0));
        // Forty pixels at six pixels per bar move history by 6 2/3 bars.
        assert!(
            (chart.state.time_scale().right_offset_precise() - (before - 20.0 / 3.0)).abs() < 1e-5
        );
        assert_eq!(chart.state.price_scale().range().min, 90.0);
        assert_eq!(chart.state.price_scale().range().max, 120.0);

        for event in [
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 20.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            },
            Event::Zoom(1.1),
        ] {
            let before = chart.state.time_scale().bar_spacing();
            let time = ctx.input(|input| input.time) + 1.0;
            frame(
                &ctx,
                &mut chart,
                time,
                vec![Event::PointerMoved(pos), event],
            );
            assert!(chart.state.time_scale().bar_spacing() > before);
        }
        let price_axis = Pos2::new(
            if left {
                pane.panel_rect.left() + 5.0
            } else {
                pane.panel_rect.right() - 5.0
            },
            pos.y,
        );
        let time_axis = chart.time_axis_rect.center();
        let before = chart.state.time_scale().bar_spacing();
        drag(
            &ctx,
            &mut chart,
            time_axis,
            time_axis + egui::vec2(30.0, 0.0),
        );
        assert!(chart.state.time_scale().bar_spacing() > before);
        let at = chart
            .pane_layout
            .panes
            .iter()
            .position(|pane| Some(pane.id) == chart.series_pane(study))
            .unwrap();
        drag(
            &ctx,
            &mut chart,
            price_axis,
            price_axis + egui::vec2(0.0, 30.0),
        );
        assert!(chart.pane_layout.panes[at].scale.is_manual());
        for axis in [price_axis, time_axis] {
            let time = ctx.input(|input| input.time) + 1.0;
            frame(&ctx, &mut chart, time, Vec::new());
            for pressed in [None, Some(true), Some(false), Some(true), Some(false)] {
                pointer(&ctx, &mut chart, axis, pressed);
            }
        }
        assert!(!chart.pane_layout.panes[at].scale.is_manual());
        assert_eq!(
            chart.state.time_scale().bar_spacing(),
            chart.chart_options.time_scale.bar_spacing
        );

        let time = ctx.input(|input| input.time) + 1.0;
        let painted = shapes(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
        assert!(chart.get_hover_bar_idx().is_some());
        assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
            if stroke.color == egui::Color32::BLUE && points.iter().all(|p| (p.y - pos.y).abs() < 0.1))));
        frame(&ctx, &mut chart, time + 0.1, vec![Event::Zoom(1.1)]);
        drag(&ctx, &mut chart, pos, pos + egui::vec2(40.0, 0.0));
        let offset = chart.state.time_scale().right_offset_precise();
        let spacing = chart.state.time_scale().bar_spacing();
        chart.pane_layout.maximized = None;
        pointer(&ctx, &mut chart, Pos2::ZERO, None);
        assert_eq!(chart.state.time_scale().right_offset_precise(), offset);
        assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
    }
}

/// Pane drawings retain study units through editing, layout changes and storage.
#[test]
#[cfg(feature = "drawings")]
fn drawings_use_the_owning_study_pane_scale() {
    use crate::drawings::DrawingToolType;
    use crate::scales::PriceScaleMode;
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for (mode, inverted, low, high, middle, maximized) in [
            (PriceScaleMode::Normal, false, 0.0, 100.0, 50.0, false),
            (PriceScaleMode::Normal, true, 0.0, 100.0, 50.0, false),
            (
                PriceScaleMode::Logarithmic,
                false,
                10.0,
                1000.0,
                100.0,
                false,
            ),
            (PriceScaleMode::Normal, false, 0.0, 100.0, 50.0, true),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[1000.0], &[1100.0]);
            chart.set_theme(&ctx, &theme);
            let study = chart.add_indicator(Box::new(RSI::new(14)));
            chart.sync_panes();
            if maximized {
                chart.pane_layout.maximized = chart.series_pane(study);
            }
            let scale = &mut chart.pane_layout.panes[1].scale;
            scale.options_mut().mode = mode;
            scale.options_mut().invert_scale = inverted;
            let transform = crate::scales::PriceTransform { mode, base: 1.0 };
            scale.rebase(transform);
            scale.set_range(transform.to_scale(low), transform.to_scale(high));
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
            let pane = chart.get_rendered_indicator_panes()[0].clone();
            // Halfway between 0 and 100 is 50; halfway between ln(10) and ln(1000) is ln(100).
            let first = Pos2::new(pane.mapping.idx_to_x(30), pane.chart_rect.center().y);
            let second = Pos2::new(pane.mapping.idx_to_x(70), pane.chart_rect.center().y);
            let viewport = chart.get_time_scale_state();
            chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::TrendLine));
            click(&ctx, &mut chart, first);
            click(&ctx, &mut chart, second);
            assert_eq!(
                chart.drawings().drawings.len(),
                1,
                "drawing on a study pane"
            );
            let id = chart.drawings().drawings[0].id;
            for point in &chart.drawings().drawings[0].chart_points {
                assert!((point.price - middle).abs() < 1e-3);
            }
            assert_eq!(chart.drawings().hit_test(first.lerp(second, 0.5)), Some(id));
            assert_eq!(
                chart
                    .drawings()
                    .hit_test(chart.get_rendered_price_rect().center()),
                None
            );
            assert!(chart.drawings().is_selected(id));
            chart.drawings_mut().deselect();
            let body = first.lerp(second, 0.25);
            assert_eq!(
                pointer(&ctx, &mut chart, body, None),
                egui::CursorIcon::PointingHand
            );
            pointer(&ctx, &mut chart, body, Some(true));
            let body_to = body + egui::vec2(24.0, 10.0);
            assert_eq!(
                pointer(&ctx, &mut chart, body_to, None),
                egui::CursorIcon::Grabbing
            );
            pointer(&ctx, &mut chart, body_to, Some(false));
            assert!(chart.drawings().is_selected(id));
            assert_eq!(chart.get_time_scale_state(), viewport);
            assert!(chart.undo());
            pointer(&ctx, &mut chart, first, None);
            assert_eq!(
                pointer(&ctx, &mut chart, first, None),
                egui::CursorIcon::ResizeNwSe
            );
            let to = first + egui::vec2(0.0, 15.0);
            drag(&ctx, &mut chart, first, to);
            let moved = chart.drawings().drawings[0].chart_points[0].price;
            assert!((moved - pane.mapping.y_to_price(to.y)).abs() < 1e-3);
            assert_eq!(chart.get_time_scale_state(), viewport);
            assert!(chart.undo());
            assert!((chart.drawings().drawings[0].chart_points[0].price - middle).abs() < 1e-3);
            assert!(chart.redo());
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            chart.load_drawings(&saved).unwrap();
            chart.pane_layout.maximized = chart.series_pane(study);
            pointer(&ctx, &mut chart, Pos2::ZERO, None);
            let drawing = &chart.drawings().drawings[0];
            let pane = &chart.get_rendered_indicator_panes()[0];
            assert!((drawing.points[0].y - pane.mapping.price_to_y(moved)).abs() < 0.01);
            let on_line = drawing.points[0].lerp(drawing.points[1], 0.5);
            assert_eq!(chart.drawings().hit_test(on_line), Some(drawing.id));
            for width in [320.0, 360.0] {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
                    },
                )
                .drop_without_applying_deltas();
                let pane = &chart.get_rendered_indicator_panes()[0];
                let drawing = &chart.drawings().drawings[0];
                assert!(pane.chart_rect.width() > 0.0 && pane.chart_rect.right() <= width);
                assert!((drawing.points[0].y - pane.mapping.price_to_y(moved)).abs() < 0.01);
                let on_line = drawing.points[0].lerp(drawing.points[1], 0.5);
                if pane.chart_rect.contains(on_line) {
                    assert_eq!(chart.drawings().hit_test(on_line), Some(drawing.id));
                }
            }
            chart.set_indicator_visible(study, false);
            pointer(&ctx, &mut chart, Pos2::ZERO, None);
            assert_eq!(
                chart.drawings().hit_test(on_line),
                None,
                "hidden panes have no drawing hits"
            );
        }
    }
}

#[test]
fn pane_separators_resize_neighbors_and_maximize_restores_the_layout() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        chart.add_indicator(Box::new(RSI::new(14)));
        chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        let main = chart.get_rendered_price_rect();
        let before = chart.get_rendered_indicator_panes().to_vec();
        let separator = Pos2::new(main.center().x, before[1].panel_rect.top() - 0.5);
        drag(
            &ctx,
            &mut chart,
            separator,
            separator + egui::vec2(0.0, 25.0),
        );
        let resized = chart.get_rendered_indicator_panes().to_vec();
        assert!(
            (resized[0].panel_rect.height() - before[0].panel_rect.height() - 25.0).abs() < 0.1
        );
        assert!(
            (resized[1].panel_rect.height() - before[1].panel_rect.height() + 25.0).abs() < 0.1
        );
        assert_eq!(chart.get_rendered_price_rect(), main);
        let maximize = Pos2::new(
            resized[0].chart_rect.right() - 12.0,
            resized[0].panel_rect.top() + 12.0,
        );
        click(&ctx, &mut chart, maximize);
        let maximized = chart.get_rendered_indicator_panes();
        assert_eq!(maximized.len(), 1);
        assert!(maximized[0].panel_rect.height() > 500.0);
        assert_eq!(maximized[0].chart_rect.x_range(), main.x_range());
        let restore = Pos2::new(
            maximized[0].chart_rect.right() - 12.0,
            maximized[0].panel_rect.top() + 12.0,
        );
        click(&ctx, &mut chart, restore);
        for (restored, resized) in chart.get_rendered_indicator_panes().iter().zip(&resized) {
            assert_eq!(restored.panel_rect, resized.panel_rect);
        }
        // Double-clicking a pane maximizes it and again restores it, without opening settings.
        let inside = resized[0].chart_rect.center();
        for maximized in [true, false] {
            for pressed in [None, Some(true), Some(false), Some(true), Some(false), None] {
                pointer(&ctx, &mut chart, inside, pressed);
            }
            assert_eq!(
                chart.get_rendered_indicator_panes().len(),
                if maximized { 1 } else { 2 }
            );
            assert_eq!(chart.take_indicator_settings_request(), None);
        }
        let separator = Pos2::new(main.center().x, resized[0].panel_rect.top() - 0.5);
        drag(
            &ctx,
            &mut chart,
            separator,
            separator - egui::vec2(0.0, 10_000.0),
        );
        assert!(chart.get_rendered_price_rect().height() >= 39.9);
        assert!(
            chart
                .get_rendered_indicator_panes()
                .iter()
                .all(|pane| pane.panel_rect.height() >= 39.9)
        );
        let last = chart.get_rendered_indicator_panes().last().unwrap();
        assert!((last.panel_rect.bottom() - 557.0).abs() < 0.1);
        chart.pane_options_mut().resizable = false;
        let fixed = chart.get_rendered_indicator_panes().to_vec();
        let separator = Pos2::new(main.center().x, fixed[1].panel_rect.top() - 0.5);
        drag(
            &ctx,
            &mut chart,
            separator,
            separator + egui::vec2(0.0, 25.0),
        );
        for (pane, fixed) in chart.get_rendered_indicator_panes().iter().zip(&fixed) {
            assert_eq!(pane.panel_rect, fixed.panel_rect, "a fixed separator");
        }
        chart.pane_options_mut().resizable = true;
        let mut layout = crate::widget::layout::ChartLayout::default();
        let mut show = |chart: &mut crate::widget::Chart, pos, pressed: Option<bool>| {
            let mut events = vec![Event::PointerMoved(pos)];
            if let Some(pressed) = pressed {
                events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    time: Some(ctx.input(|input| input.time) + 0.1),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        layout.show(ui, &mut [chart]);
                    });
                },
            )
            .drop_without_applying_deltas();
        };
        show(&mut chart, Pos2::ZERO, None);
        for maximized in [true, false] {
            let pane = &chart.get_rendered_indicator_panes()[0];
            let button = Pos2::new(pane.chart_rect.right() - 12.0, pane.panel_rect.top() + 12.0);
            for pressed in [None, Some(true), Some(false), None] {
                show(&mut chart, button, pressed);
            }
            assert_eq!(
                chart.get_rendered_indicator_panes().len(),
                if maximized { 1 } else { 2 },
                "the layout menu must leave pane maximize and restore reachable"
            );
        }
    }
}

/// The Hide and Remove controls in an added series' legend are one undo step each, and undo
/// brings the series back with its price formatter and bar limit.
#[test]
fn added_series_legend_controls_undo_as_one_step() {
    use crate::scales::PriceScaleMode;
    use crate::widget::{SeriesId, SeriesOptions};
    let limit = std::num::NonZeroUsize::new(1000);
    // A name wider than the pane holds the controls at their rightmost place: 120 px from
    // the plot's right edge, 20 px each, in the order hide, settings, remove.
    for (control, removed) in [(110.0, false), (70.0, true)] {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        let series = chart.add_series(SeriesOptions {
            name: "S".repeat(200),
            ..Default::default()
        });
        chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
        chart.set_bar_limit(series, limit);
        assert!(chart.set_series_formatter(
            series,
            Some(std::sync::Arc::new(crate::scales::CurrencyFormatter::usd())),
        ));
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(series, pane));
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        let state = |chart: &crate::widget::Chart| {
            (
                chart.is_series_visible(series),
                chart.bars(series).len(),
                chart.bar_limit(series),
                chart
                    .series_label_formatter(series, PriceScaleMode::Normal)
                    .is_some(),
            )
        };
        let before = state(&chart);
        assert_eq!(before, (true, 100, limit, true));

        let row = chart.pane_layout.panes.iter().find(|item| item.id == pane);
        let target = Pos2::new(
            chart.get_rendered_price_rect().right() - control,
            row.unwrap().rect.top() + 12.0,
        );
        click(&ctx, &mut chart, target);
        assert!(!chart.is_series_visible(series), "control {control}");
        assert_eq!(chart.series().any(|(id, _)| id == series), !removed);

        assert!(chart.undo(), "control {control} left no undo step");
        assert_eq!(state(&chart), before);
        assert_eq!(chart.pane_series(pane), [series]);
        assert!(!chart.can_undo(), "control {control} took more than a step");
    }
}

#[test]
fn merged_panes_share_scales_and_studies_can_return_to_the_main_pane() {
    use crate::scales::PriceScaleMode;
    let ctx = Context::default();
    let mut chart = chart(&[100.0], &[110.0]);
    let first = chart.add_indicator(Box::new(RSI::new(14)));
    let second = chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
    crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
    // Reach the pane menu through pointer input; the old pane has no such menu.
    let pane = &chart.get_rendered_indicator_panes()[1];
    let menu = Pos2::new(pane.chart_rect.right() - 32.0, pane.panel_rect.top() + 12.0);
    click(&ctx, &mut chart, menu);
    let shapes = crate::widget::test_frames::shapes(&ctx, &mut chart, 2.0, Vec::new());
    let target = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Text(text) if text.galley.text() == "Merge with pane above" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("pane merge action");
    click(&ctx, &mut chart, target);
    crate::widget::test_frames::frame(
        &ctx,
        &mut chart,
        ctx.input(|input| input.time) + 0.1,
        Vec::new(),
    );
    let panes = chart.get_rendered_indicator_panes();
    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0].panel_rect, panes[1].panel_rect);
    assert_eq!(
        panes[0].mapping.price.range(),
        panes[1].mapping.price.range()
    );
    let axis = Pos2::new(
        panes[0].chart_rect.right() + 10.0,
        panes[0].panel_rect.center().y,
    );
    let before = panes[0].mapping.price.range();
    drag(&ctx, &mut chart, axis, axis + egui::vec2(0.0, 20.0));
    let after = chart.get_rendered_indicator_panes()[0]
        .mapping
        .price
        .range();
    assert!(after.max - after.min > before.max - before.min);
    assert!(chart.pane_layout.panes[1].scale.is_manual());
    for enabled in [false, true] {
        chart
            .chart_options
            .handle_scale
            .axis_double_click_reset
            .price = enabled;
        for pressed in [None, Some(true), Some(false), Some(true), Some(false)] {
            pointer(&ctx, &mut chart, axis, pressed);
        }
        assert_eq!(chart.pane_layout.panes[1].scale.is_manual(), !enabled);
    }
    // The same engine maps 10 and 1000 around 100 at the midpoint in logarithmic units.
    let scale = &mut chart.pane_layout.panes[1].scale;
    scale.options_mut().mode = PriceScaleMode::Logarithmic;
    scale.reset();
    scale.rebase(crate::scales::PriceTransform {
        mode: PriceScaleMode::Logarithmic,
        base: 1.0,
    });
    scale.set_range(10.0_f64.ln(), 1000.0_f64.ln());
    crate::widget::test_frames::frame(
        &ctx,
        &mut chart,
        ctx.input(|input| input.time) + 0.1,
        Vec::new(),
    );
    let pane = &chart.get_rendered_indicator_panes()[0];
    assert!((pane.mapping.price_to_y(100.0) - pane.chart_rect.center().y).abs() < 0.01);
    assert!(chart.move_indicator_to_pane(second, second));
    crate::widget::test_frames::frame(
        &ctx,
        &mut chart,
        ctx.input(|input| input.time) + 0.1,
        Vec::new(),
    );
    assert_ne!(
        chart.get_rendered_indicator_panes()[0].panel_rect,
        chart.get_rendered_indicator_panes()[1].panel_rect
    );
    assert!(chart.move_indicator_to_pane(second, crate::widget::SeriesId::MAIN));
    crate::widget::test_frames::frame(
        &ctx,
        &mut chart,
        ctx.input(|input| input.time) + 0.1,
        Vec::new(),
    );
    assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
    assert!(
        chart
            .indicators()
            .find(|(id, _)| *id == second)
            .unwrap()
            .1
            .is_overlay()
    );
    assert!(chart.move_indicator_to_pane(second, first));
    crate::widget::test_frames::frame(
        &ctx,
        &mut chart,
        ctx.input(|input| input.time) + 0.1,
        Vec::new(),
    );
    assert_eq!(
        chart.get_rendered_indicator_panes()[0].panel_rect,
        chart.get_rendered_indicator_panes()[1].panel_rect
    );
    assert!(!chart.move_indicator_to_pane(first, crate::widget::SeriesId(usize::MAX)));
    // Splitting the original member must leave independent pane controls.
    assert!(chart.move_indicator_to_pane(first, first));
    assert_ne!(chart.pane_layout.panes[1].id, chart.pane_layout.panes[2].id);
    crate::widget::test_frames::frame(&ctx, &mut chart, 20.0, Vec::new());
    let pane = &chart.get_rendered_indicator_panes()[1];
    let maximize = Pos2::new(pane.chart_rect.right() - 12.0, pane.panel_rect.top() + 12.0);
    click(&ctx, &mut chart, maximize);
    assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
    assert_eq!(chart.get_rendered_indicator_panes()[0].indicator_idx, 0);
    for id in [first, second] {
        chart.set_indicator_visible(id, false);
    }
    crate::widget::test_frames::frame(&ctx, &mut chart, 25.0, Vec::new());
    assert_eq!(chart.pane_layout.maximized, None);
    assert!(chart.get_rendered_price_rect().height() > 500.0);
}

#[test]
fn merged_panes_select_visible_lines_and_scale_from_visible_values() {
    use crate::studies::{CustomIndicator, IndicatorValue};
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = chart(&[100.0], &[110.0]);
        let studies: Vec<_> = [20.0, 40.0]
            .into_iter()
            .map(|value| {
                chart.add_indicator(Box::new(
                    CustomIndicator::new("Line", move |bars| {
                        vec![IndicatorValue::Single(value); bars.len()]
                    })
                    .with_overlay(false),
                ))
            })
            .collect();
        assert!(chart.move_indicator_to_pane(studies[1], studies[0]));
        chart.pane_layout.panes[1].scale.set_range(0.0, 60.0);
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        for (id, value) in studies.iter().copied().zip([20.0, 40.0]) {
            let pane = &chart.get_rendered_indicator_panes()[0];
            let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.mapping.price_to_y(value));
            click(&ctx, &mut chart, pos);
            assert_eq!(
                chart.selection.selected_id(),
                Some(crate::chart::selection::ChartElementId::Indicator(id))
            );
        }
        chart.set_indicator_visible(studies[0], false);
        chart.pane_layout.panes[1].scale.options_mut().mode =
            crate::scales::PriceScaleMode::Percentage;
        chart.pane_layout.panes[1].scale.reset();
        crate::widget::test_frames::frame(&ctx, &mut chart, 5.0, Vec::new());
        let pane = &chart.get_rendered_indicator_panes()[0];
        assert_eq!(pane.mapping.price.transform().to_scale(40.0), 0.0);
    }
}

#[test]
fn panes_fit_and_share_the_lowest_time_axis() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for size in [
            (1440.0, 900.0),
            (1440.0, 868.0),
            (720.0, 450.0),
            (640.0, 450.0),
            (240.0, 450.0),
            (320.0, 450.0),
            (600.0, 400.0),
            (600.0, 180.0),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            chart.state.time_scale_mut().set_right_offset(0.0);
            let rsi = chart.add_indicator(Box::new(RSI::new(14)));
            chart.config.show_indicator_inputs = true;
            chart.study_inputs[0] =
                "A long study input caption that must leave room for controls ".repeat(3);
            chart.selection.select(
                crate::chart::selection::ChartElementId::Indicator(rsi),
                None,
            );
            chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
            let mut bounds = Rect::NOTHING;
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        bounds = ui.available_rect_before_wrap();
                        chart.show(ui);
                    });
                },
            );
            let painted = output.shapes.clone();
            output.drop_without_applying_deltas();
            let price = chart.get_rendered_price_rect();
            let panes = chart.get_rendered_indicator_panes();
            assert_eq!(panes.len(), 2, "{size:?}");
            for pane in panes {
                assert!(
                    bounds.contains_rect(pane.panel_rect),
                    "{size:?}: {:?} in {bounds:?}",
                    pane.panel_rect
                );
                assert_eq!(pane.chart_rect.x_range(), price.x_range());
            }
            let mut dots = 0;
            for shape in &painted {
                if let egui::Shape::Circle(dot) = &shape.shape
                    && panes[0].chart_rect.contains(dot.center)
                {
                    assert!(
                        panes[0].chart_rect.contains_rect(shape.clip_rect),
                        "selection dot escapes pane at {size:?}: {:?}",
                        shape.clip_rect
                    );
                    dots += 1;
                }
            }
            if size.1 > 200.0 {
                assert!(dots > 0, "missing selection dots");
            }
            let mut legend_seen = false;
            for shape in &painted {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text().starts_with("RSI ")
                {
                    legend_seen = true;
                    let visible = Rect::from_min_size(text.pos, text.galley.size())
                        .intersect(shape.clip_rect);
                    let controls_left = panes[0].chart_rect.right() - 60.0;
                    assert!(
                        visible.right() <= controls_left,
                        "legend overlaps controls at {size:?}"
                    );
                }
            }

            assert!(legend_seen, "missing pane legend");
            let last = panes.last().unwrap().panel_rect;
            assert!(
                (bounds.bottom() - last.bottom() - 35.0).abs() < 1.0,
                "{size:?}: {last:?} in {bounds:?}"
            );
            if size.1 > 200.0 {
                assert_eq!(panes[0].panel_rect.height(), 138.0, "{size:?}");
                assert!(price.height() >= 40.0, "{size:?}");
            } else {
                assert_eq!(last.height(), 24.0);
            }
            fn check(shape: &egui::Shape, bottom: f32, clip: Rect, count: &mut usize) {
                match shape {
                    egui::Shape::Vec(shapes) => {
                        shapes.iter().for_each(|s| check(s, bottom, clip, count))
                    }
                    egui::Shape::Text(text)
                        if text.galley.text().contains("Jan")
                            || text.galley.text().contains("Dec")
                            || text.galley.text().contains("Feb")
                            || (text.galley.text().contains(':')
                                && text
                                    .galley
                                    .text()
                                    .chars()
                                    .all(|c| c.is_ascii_digit() || c == ':'))
                            || (text.pos.y >= bottom
                                && text.galley.text().chars().all(|c| c.is_ascii_digit())) =>
                    {
                        assert!(
                            text.pos.y >= bottom,
                            "time label above lowest pane: {:?}",
                            text.pos
                        );
                        assert!(
                            text.pos.y >= clip.top()
                                && text.pos.y + text.galley.size().y <= clip.bottom(),
                            "time label outside painter clip: {:?} in {clip:?}",
                            text.pos
                        );
                        *count += 1;
                    }
                    _ => {}
                }
            }
            let mut count = 0;
            for shape in &painted {
                check(&shape.shape, last.bottom(), shape.clip_rect, &mut count);
            }
            assert!(count > 0, "missing time labels at {size:?}");
            let header =
                chart.get_rendered_indicator_panes()[0].chart_rect.min + egui::vec2(10.0, 12.0);
            let was_collapsed = size.1 <= 200.0;
            for events in [
                vec![Event::PointerMoved(header)],
                vec![Event::PointerButton {
                    pos: header,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
                vec![Event::PointerButton {
                    pos: header,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                Vec::new(),
            ] {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            chart.show(ui);
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
            let panes = chart.get_rendered_indicator_panes();
            assert_eq!(panes.len(), 2, "header click removed a study");
            if was_collapsed {
                assert!(
                    panes[0].panel_rect.height() >= 40.0,
                    "pane did not restore at {size:?}"
                );
            } else {
                assert_eq!(
                    panes[0].panel_rect.height(),
                    24.0,
                    "pane did not collapse at {size:?}"
                );
            }
            assert!(
                panes
                    .iter()
                    .all(|pane| bounds.contains_rect(pane.panel_rect))
            );
            // A saved expanded state must still fit after the host becomes shorter.
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(size.0, 107.0))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        bounds = ui.available_rect_before_wrap();
                        chart.show(ui);
                    });
                },
            )
            .drop_without_applying_deltas();
            for pane in chart.get_rendered_indicator_panes() {
                assert!(bounds.contains_rect(pane.panel_rect));
                assert_eq!(
                    pane.panel_rect.height(),
                    24.0,
                    "short panes collapse after resize"
                );
            }
        }
    }
}
#[test]
fn every_pane_shares_hover_readouts_and_uses_its_own_horizontal_scale() {
    use crate::studies::{CustomIndicator, IndicatorValue};
    use crate::widget::test_frames::{frame, shapes};
    for (left, theme) in [
        (false, crate::theme::Theme::light()),
        (true, crate::theme::Theme::light()),
        (false, crate::theme::Theme::dark()),
        (true, crate::theme::Theme::dark()),
    ] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = chart(&[100.0], &[110.0]);
        chart.set_instrument(Some(
            crate::model::Instrument::new("SAMPLE").with_volume_decimals(3),
        ));
        chart.config.show_right_axis = !left;
        chart.config.show_left_axis = left;
        for (name, base) in [("Readout", 20.0), ("Other", 30.0)] {
            chart.add_indicator(Box::new(
                CustomIndicator::new(name, move |bars| {
                    (0..bars.len())
                        .map(|index| IndicatorValue::Single(base + index as f64 / 10.0))
                        .collect()
                })
                .with_overlay(false),
            ));
        }
        let overlay = chart.add_indicator(Box::new(CustomIndicator::new("Overlay", |bars| {
            vec![IndicatorValue::Single(107.25); bars.len()]
        })));
        chart.chart_options.crosshair.vert_line_style = crate::model::LineStyle::Solid;
        chart.chart_options.crosshair.horz_line_style = crate::model::LineStyle::Solid;
        chart.chart_options.crosshair.vert_line_color = egui::Color32::RED;
        chart.chart_options.crosshair.horz_line_color = egui::Color32::BLUE;
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        frame(&ctx, &mut chart, 0.2, vec![]);
        let map = chart.get_chart_mapping();
        let panes = chart.get_rendered_indicator_panes();
        let rects = [
            chart.last_rendered_price_rect,
            chart.last_rendered_volume_rect,
            panes[0].chart_rect,
            panes[1].chart_rect,
        ];
        for (step, rect) in rects.iter().enumerate() {
            let pos = Pos2::new(map.idx_to_x(70) + 1.0, rect.center().y);
            let painted = shapes(
                &ctx,
                &mut chart,
                1.0 + step as f64,
                vec![Event::PointerMoved(pos)],
            );
            assert_eq!(
                chart
                    .get_hover_bar_idx()
                    .map(|index| index.round() as usize),
                Some(70)
            );
            let text: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            if step == 1 {
                assert!(
                    text.contains(&"0.500"),
                    "volume cursor lost its precision: {text:?}"
                );
            }
            for expected in ["Readout 27.00", "Other 37.00", "Open"] {
                assert!(
                    text.iter()
                        .any(|text| text.split_whitespace().collect::<Vec<_>>().join(" ")
                            == expected),
                    "{expected}: {text:?}"
                );
            }
            assert!(
                painted.iter().any(
                    |shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
            if stroke.color == egui::Color32::BLUE && points[0] == Pos2::new(rect.left(), pos.y)
                && points[1] == Pos2::new(rect.right(), pos.y))
                ),
                "horizontal in {rect:?}"
            );
            for pane in &rects[2..] {
                assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::RED && points[0] == Pos2::new(map.idx_to_x(70), pane.top())
                    && points[1] == Pos2::new(map.idx_to_x(70), pane.bottom()))), "vertical in {pane:?}");
            }
        }

        // Empty space still has a cursor in every pane, without selecting a bar.
        let pos = Pos2::new(map.idx_to_x(102), rects[2].center().y);
        assert!(rects[2].contains(pos));
        let painted = shapes(&ctx, &mut chart, 9.0, vec![Event::PointerMoved(pos)]);
        assert_eq!(chart.get_hover_bar_idx(), None);
        for pane in &rects[2..] {
            assert!(
                painted.iter().any(
                    |shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::RED && points[0] == Pos2::new(pos.x, pane.top())
                    && points[1] == Pos2::new(pos.x, pane.bottom()))
                ),
                "missing cursor in empty pane space"
            );
        }

        chart.set_synced_crosshair_bar_idx(Some(70.0));
        let painted = shapes(&ctx, &mut chart, 9.5, vec![Event::PointerGone]);
        for pane in &rects[2..] {
            assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::RED && points[0] == Pos2::new(map.idx_to_x(70), pane.top())
                    && points[1] == Pos2::new(map.idx_to_x(70), pane.bottom()))), "missing synced cursor in pane");
        }
        chart.set_synced_crosshair_bar_idx(None);
        let painted = shapes(&ctx, &mut chart, 10.0, vec![Event::PointerGone]);
        assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text().split_whitespace().collect::<Vec<_>>().join(" ") == "Readout 29.90")));
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
        // With OHLC at 100 and 110, 107.25 can only come from the overlay.
        // Each pane contains 20 or 30 plus i/10, so bar 70 reads 27 or 37.
        for (step, (rect, value, y)) in [
            (rects[0], 107.25, map.price_to_y(107.25)),
            (
                rects[2],
                27.0,
                rects[2].bottom() - (27.0 - 19.01) / 11.88 * rects[2].height(),
            ),
            (
                rects[3],
                37.0,
                rects[3].bottom() - (37.0 - 29.01) / 11.88 * rects[3].height(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let painted = shapes(
                &ctx,
                &mut chart,
                20.0 + step as f64,
                vec![Event::PointerMoved(Pos2::new(map.idx_to_x(70), y + 1.0))],
            );
            assert!(
                painted.iter().any(
                    |shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
            if stroke.color == egui::Color32::BLUE && (points[0].y - y).abs() < 0.5
                && points[0].x == rect.left())
                ),
                "snap to {value}"
            );
        }
        chart.set_indicator_visible(overlay, false);
        let painted = shapes(
            &ctx,
            &mut chart,
            30.0,
            vec![Event::PointerMoved(Pos2::new(
                map.idx_to_x(70),
                map.price_to_y(107.25),
            ))],
        );
        assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
        if stroke.color == egui::Color32::BLUE && (points[0].y - map.price_to_y(110.0)).abs() < 0.01)), "hidden study must not attract the cursor");
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
        for (offset, action) in [(30.0, "settings"), (50.0, "remove"), (10.0, "hide")] {
            let painted = shapes(&ctx, &mut chart, 40.0, vec![Event::PointerGone]);
            let pane = &chart.get_rendered_indicator_panes()[0];
            let study = chart.study_ids[pane.indicator_idx];
            let title = painted
                .iter()
                .find_map(|shape| match shape {
                    egui::Shape::Text(text)
                        if text.galley.text().starts_with("Readout ")
                            || text.galley.text().starts_with("Other ") =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .unwrap();
            let pos = title.pos
                + egui::vec2(
                    title.galley.size().x + crate::tokens::DESIGN_TOKENS.spacing.md + offset,
                    10.0,
                );
            for pressed in [None, Some(true), Some(false)] {
                let mut events = vec![Event::PointerMoved(pos)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                frame(&ctx, &mut chart, 41.0, events);
            }
            match action {
                "settings" => {
                    assert_eq!(chart.take_indicator_settings_request(), Some(study));
                    assert!(chart.get_rendered_indicator_panes()[0].chart_rect.height() > 24.0);
                }
                "remove" => assert!(!chart.indicators().any(|(id, _)| id == study)),
                _ => assert!(
                    !chart
                        .indicators()
                        .find(|(id, _)| *id == study)
                        .unwrap()
                        .1
                        .is_visible()
                ),
            }
            if action != "settings" {
                assert!(chart.undo());
                assert!(
                    chart
                        .indicators()
                        .any(|(id, indicator)| id == study && indicator.is_visible())
                );
                assert!(chart.redo());
            }
        }
    }
}

/// Wrapped headers keep their space when studies share a pane or change paint order.
#[test]
fn merged_pane_legends_reserve_their_wrapped_height() {
    use crate::studies::{CustomIndicator, IndicatorValue};
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for width in [320.0, 800.0, 1440.0] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            chart.set_size(Some(Vec2::new(width, 600.0)));
            let first = chart.add_indicator(Box::new(
                CustomIndicator::new("Ranges", |bars| {
                    vec![IndicatorValue::Multiple(vec![12.0, 34.0, 56.0]); bars.len()]
                })
                .with_overlay(false)
                .with_line_count(3)
                .with_line_names(vec![
                    "Delta [0, 2] lots".into(),
                    "Delta [3, 5] lots".into(),
                    "Delta [10, ∞] lots".into(),
                ]),
            ));
            let second = chart.add_indicator(Box::new(
                CustomIndicator::new("Following", |bars| {
                    vec![IndicatorValue::Single(20.0); bars.len()]
                })
                .with_overlay(false),
            ));
            assert!(chart.move_indicator_to_pane(second, first));
            chart.set_pane_height(chart.series_pane(first).unwrap(), 260.0);
            chart.chart_options.hovered_series_on_top = true;
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            for value in [12.0, 20.0] {
                let pane = &chart.get_rendered_indicator_panes()[0];
                let pos = Pos2::new(pane.chart_rect.center().x, pane.mapping.price_to_y(value));
                let shapes = crate::widget::test_frames::shapes(
                    &ctx,
                    &mut chart,
                    1.0,
                    vec![Event::PointerMoved(pos)],
                );
                let bounds = |name: &str| {
                    shapes
                        .iter()
                        .find_map(|shape| match shape {
                            egui::Shape::Text(text)
                                if text.galley.text().starts_with(&format!("{name} ")) =>
                            {
                                assert!(
                                    !text.galley.elided,
                                    "{width}: {name}: {:?}",
                                    text.galley.text()
                                );
                                Some(Rect::from_min_size(text.pos, text.galley.size()))
                            }
                            _ => None,
                        })
                        .expect(name)
                };
                let first = bounds("Ranges");
                let second = bounds("Following");
                assert!(
                    first.bottom() <= second.top(),
                    "{width}: {first:?}, {second:?}"
                );
                let pane = chart.get_rendered_indicator_panes()[0].chart_rect;
                assert!(pane.contains_rect(first) && pane.contains_rect(second));
            }
        }
    }
}

#[test]
fn collapsed_price_pane_keeps_pointer_navigation_in_the_visible_pane() {
    use crate::widget::PaneId;
    let mut results = Vec::new();
    for layout in ["maximized", "collapsed"] {
        let ctx = Context::default();
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        let study = chart.add_indicator(Box::new(RSI::new(14)));
        let study_pane = chart.series_pane(study).unwrap();
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        // Either way the study fills the chart and the price frame has no height.
        if layout == "maximized" {
            chart.pane_layout.maximized = Some(study_pane);
        } else {
            for pane in &mut chart.pane_layout.panes {
                if pane.id == PaneId::MAIN {
                    pane.collapsed = Some(true);
                }
            }
        }
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.1, Vec::new());
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.2, Vec::new());
        let pos = chart.get_rendered_indicator_panes()[0].chart_rect.center();
        assert_eq!(chart.state.time_scale().bar_spacing(), 6.0);
        let before = chart.state.time_scale().right_offset_precise();
        drag(&ctx, &mut chart, pos, pos + egui::vec2(40.0, 0.0));
        let moved = chart.state.time_scale().right_offset_precise() - before;
        // Forty pixels at six pixels per bar move history by 6 2/3 bars.
        assert!((moved + 20.0 / 3.0).abs() < 1e-5, "{layout}: moved {moved}");
        let time = ctx.input(|input| input.time) + 1.0;
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            time,
            vec![
                Event::PointerMoved(pos),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 20.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        // Let the eased zoom finish before reading where it ends.
        for step in 1..=60 {
            let time = time + f64::from(step) * 0.1;
            crate::widget::test_frames::frame(&ctx, &mut chart, time, Vec::new());
        }
        let spacing = chart.state.time_scale().bar_spacing();
        assert!(spacing > 6.0, "{layout}: spacing {spacing}");
        let offset = chart.state.time_scale().right_offset_precise();
        results.push((moved, spacing));
        if layout == "collapsed" {
            // Restoring the price pane keeps the scroll and zoom reached without it.
            let header = chart.pane_layout.panes[0].rect.center();
            click(&ctx, &mut chart, header);
            assert!(chart.pane_height(PaneId::MAIN).unwrap() > 24.0);
            assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
            assert_eq!(chart.state.time_scale().right_offset_precise(), offset);
        }
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn collapsing_the_price_pane_of_a_short_chart_leaves_no_blank_area() {
    use crate::widget::PaneId;
    let ctx = Context::default();
    let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
    chart.set_size(Some(Vec2::new(800.0, 150.0)));
    let study = chart.add_indicator(Box::new(RSI::new(14)));
    let study_pane = chart.series_pane(study).unwrap();
    for time in [0.0, 0.1, 0.2] {
        crate::widget::test_frames::frame(&ctx, &mut chart, time, Vec::new());
    }
    let heights = |chart: &crate::widget::Chart| {
        [
            chart.pane_height(PaneId::MAIN).unwrap(),
            chart.pane_height(study_pane).unwrap(),
        ]
    };
    // Beside the price pane the study is too short to expand and shows only its header.
    assert_eq!(heights(&chart), [90.0, 24.0]);
    let set_collapsed = |chart: &mut crate::widget::Chart, collapsed| {
        for pane in &mut chart.pane_layout.panes {
            if pane.id == PaneId::MAIN {
                pane.collapsed = collapsed;
            }
        }
    };
    set_collapsed(&mut chart, Some(true));
    for time in [0.3, 0.4] {
        crate::widget::test_frames::frame(&ctx, &mut chart, time, Vec::new());
    }
    // The price header takes 24 px of the 114 px pane area; the study fills the rest.
    assert_eq!(heights(&chart), [24.0, 90.0]);
    set_collapsed(&mut chart, Some(false));
    for time in [0.5, 0.6] {
        crate::widget::test_frames::frame(&ctx, &mut chart, time, Vec::new());
    }
    assert_eq!(heights(&chart), [90.0, 24.0]);
}

/// A chart's own background covers its study panes. Solid, neither a pane nor the fills that
/// keep legend text and the labels of a study's own axis readable paint the theme's background,
/// nor does the plate under the controls of a hovered study pane row. A gradient spans the chart
/// frame, from 8 to 592 points down the 600 point test frame, and each pane paints the part
/// behind it. A chart that leaves its background to a container paints none behind the price
/// pane and still fills its study panes, which hides the crosshair line the price frame draws
/// through them.
#[test]
fn custom_background_reaches_study_panes() {
    use crate::config::BackgroundStyle;
    use crate::studies::{SMA, StudyScale};
    use crate::widget::test_frames::{frame, shapes};
    use egui::{Color32, Shape};
    let solid = Color32::from_rgb(120, 10, 10);
    let (top, bottom) = (Color32::from_rgb(200, 0, 0), Color32::from_rgb(0, 0, 200));
    let gradient = BackgroundStyle::VerticalGradient {
        top_color: top,
        bottom_color: bottom,
    };
    let at = |y: f32| top.lerp_to_gamma(bottom, (y - 8.0) / 584.0);
    let theme = crate::theme::Theme::dark();
    for (style, skip) in [
        (BackgroundStyle::Solid, false),
        (BackgroundStyle::Solid, true),
        (gradient, false),
        (gradient, true),
    ] {
        let ctx = Context::default();
        let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
        chart.set_theme(&ctx, &theme);
        chart.add_indicator(Box::new(RSI::new(14)));
        let average = chart.add_indicator(Box::new(SMA::new(5)));
        assert!(chart.set_indicator_scale(average, StudyScale::Independent));
        chart.config_mut().background_color = solid;
        chart.config_mut().background_style = style;
        chart.config_mut().skip_background = skip;
        frame(&ctx, &mut chart, 0.0, vec![]);
        // The pointer rests on the study pane's legend row, which reveals the row's controls.
        let row = chart.get_rendered_indicator_panes()[0].chart_rect.min + Vec2::new(20.0, 12.0);
        frame(&ctx, &mut chart, 0.05, vec![Event::PointerMoved(row)]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(row)]);
        let pane = chart.get_rendered_indicator_panes()[0].panel_rect;
        // A background fill is one colour, or a mesh's corner colours clockwise from the top left.
        let fills = |rect: Rect| -> Vec<Vec<Color32>> {
            let corners = [
                rect.left_top(),
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
            ];
            painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(shape) if shape.rect == rect && shape.stroke.width == 0.0 => {
                        Some(vec![shape.fill])
                    }
                    Shape::Mesh(mesh) if mesh.vertices.iter().map(|v| v.pos).eq(corners) => {
                        Some(mesh.vertices.iter().map(|v| v.color).collect())
                    }
                    _ => None,
                })
                .collect()
        };
        let slice = |rect: Rect| {
            if style == BackgroundStyle::Solid {
                vec![solid]
            } else {
                let (top, bottom) = (at(rect.top()), at(rect.bottom()));
                vec![top, top, bottom, bottom]
            }
        };
        let label = format!("{style:?}, skip_background {skip}");
        assert_eq!(fills(pane), [slice(pane)], "{label}");
        let price_height = chart.pane_height(crate::widget::PaneId::MAIN).unwrap();
        let price_frame = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(784.0, price_height));
        let price_fills = if skip {
            Vec::new()
        } else {
            vec![slice(price_frame)]
        };
        assert_eq!(fills(price_frame), price_fills, "{label}");
        if style == BackgroundStyle::Solid {
            // The theme's background, opaque and at the 60% opacity of a study's legend row.
            let stale: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(shape)
                        if shape.stroke.width == 0.0
                            && (shape.fill == Color32::BLACK
                                || shape.fill == Color32::from_black_alpha(153)) =>
                    {
                        Some(shape.rect)
                    }
                    _ => None,
                })
                .filter(|rect| *rect != Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)))
                .collect();
            assert!(stale.is_empty(), "theme background at {stale:?}");
        }
    }
}
