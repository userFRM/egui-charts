use super::*;
use crate::theme::Theme;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

#[test]
fn watchlist_entry_lists_each_list_by_index_and_needs_a_symbol_and_a_list() {
    let mut menu = ChartContextMenu::new();
    let submenu = |menu: &ChartContextMenu| {
        menu.build_menu_items(&Context::default(), &crate::widget::ShortcutMap::default())
            .into_iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu { label, entries, .. } => Some((
                    label,
                    entries
                        .into_iter()
                        .filter_map(|entry| match entry {
                            MenuEntry::Item(item) => Some(item.action),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
    };
    menu.watchlists = vec!["Tech".into(), "Energy".into()];
    assert_eq!(submenu(&menu), None, "no symbol");
    menu.symbol = "AAPL".into();
    let add = |list| ContextMenuAction::AddToWatchlist {
        list,
        symbol: "AAPL".into(),
    };
    assert_eq!(
        submenu(&menu),
        Some(("Add AAPL to watchlist".into(), vec![add(0), add(1)]))
    );
    menu.watchlists.clear();
    assert_eq!(submenu(&menu), None, "no lists");
}

/// Each named control removes its row while leaving unrelated entries available; removing
/// every row closes the popup instead of leaving separators or stale actions.
#[test]
fn hidden_menu_controls_remove_rows_and_empty_sections() {
    let controls = [
        Control::Alerts,
        Control::Trading,
        Control::Settings,
        Control::ObjectTree,
        Control::Indicators,
        Control::ResetView,
        Control::CopyPrice,
        Control::ExportData,
    ];
    for theme in [Theme::light(), Theme::dark()] {
        for series in [false, true] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            // The horizontal line row follows the drawing tool's availability, not a named control.
            chart.drawings_mut().registry.set_availability(
                crate::drawings::DrawingToolType::HorizontalLine,
                crate::drawings::ToolAvailability {
                    visible: false,
                    enabled: true,
                },
            );
            let mut menus = ChartMenus::default();
            let frame = |menus: &mut ChartMenus, chart: &mut Chart| {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    menus.show(ui.ctx(), chart);
                });
                let mut labels = Vec::new();
                fn collect(shape: &egui::Shape, labels: &mut Vec<String>) {
                    match shape {
                        egui::Shape::Vec(shapes) => {
                            for shape in shapes {
                                collect(shape, labels);
                            }
                        }
                        egui::Shape::Text(text) => labels.push(text.galley.text().to_owned()),
                        _ => {}
                    }
                }
                for shape in &output.shapes {
                    collect(&shape.shape, &mut labels);
                }
                output.textures_delta.clear();
                output.drop_without_applying_deltas();
                labels
            };
            for visible in [true, false, true] {
                for control in controls {
                    menus.set_control_visible(control, visible);
                }
                chart.symbol = "SYNTH".into();
                chart.right_click = Some(if series {
                    RightClickTarget::Element {
                        id: ChartElementId::Series(SeriesId::MAIN),
                        bar_idx: 0,
                        pos: Pos2::new(30.0, 30.0),
                        price: 105.0,
                    }
                } else {
                    RightClickTarget::Background {
                        pos: Pos2::new(30.0, 30.0),
                        price: 105.0,
                    }
                });
                frame(&mut menus, &mut chart);
                let labels = frame(&mut menus, &mut chart);
                assert_eq!(!labels.is_empty(), visible, "{labels:?}");
                assert_eq!(menus.chart.is_open() || menus.series.is_open(), visible);
                if !visible {
                    menus
                        .host_entries
                        .push(HostMenuEntry::new("host-only", "Host only"));
                    chart.right_click = Some(RightClickTarget::Background {
                        pos: Pos2::new(30.0, 30.0),
                        price: 105.0,
                    });
                    frame(&mut menus, &mut chart);
                    assert!(
                        frame(&mut menus, &mut chart)
                            .iter()
                            .any(|label| label == "Host only")
                    );
                    assert!(
                        matches!(
                            menus
                                .build_menu_items(&ctx, chart.shortcuts(), menus.context.unwrap())
                                .first(),
                            Some(MenuEntry::Item(_))
                        ),
                        "host-only menus have no leading separator"
                    );
                    menus.host_entries.clear();
                }
            }
            menus.set_control_visible(Control::Settings, false);
            menus.set_control_visible(Control::ObjectTree, false);
            menus.set_control_visible(Control::Indicators, false);
            chart.right_click = Some(RightClickTarget::Background {
                pos: Pos2::new(30.0, 30.0),
                price: 105.0,
            });
            let labels = frame(&mut menus, &mut chart);
            assert!(labels.iter().any(|label| label.starts_with("Copy price")));
            assert!(!labels.iter().any(|label| label.contains("Settings")
                || label.contains("Object Tree")
                || label.contains("indicator")));
            menus.set_control_visible(Control::ContextMenus, false);
            assert!(frame(&mut menus, &mut chart).is_empty());
            assert!(!chart.menu_open);
            menus.set_control_visible(Control::ContextMenus, true);
            assert!(
                frame(&mut menus, &mut chart).is_empty(),
                "closed menus stay closed"
            );
        }
    }
}

#[test]
fn host_menu_entries_filter_targets_and_return_the_original_click() {
    use egui::{Event, PointerButton, RawInput, Shape};
    for theme in [Theme::light(), Theme::dark()] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = crate::widget::test_frames::chart(&[100.0; 10], &[110.0; 10]);
        let mut menus = ChartMenus {
            host_entries: vec![
                HostMenuEntry::new("ticket", "Send to ticket")
                    .when(|context| matches!(context.target, RightClickTarget::Background { .. })),
                HostMenuEntry::new("research", "Open in research"),
            ],
            ..Default::default()
        };
        let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
            let mut action = None;
            let right_click = chart.right_click.take();
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    if right_click.is_some() {
                        chart.right_click = right_click;
                    }
                    action = menus.show(ui.ctx(), chart);
                },
            );
            let text = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            output.drop_without_applying_deltas();
            (action, text)
        };
        frame(&mut chart, &mut menus, vec![]);
        let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(5), 100.0);
        let time = chart.state().shown()[5].time;
        for target in [
            RightClickTarget::Background { pos, price: 123.25 },
            RightClickTarget::Element {
                id: ChartElementId::Series(SeriesId::MAIN),
                pos,
                bar_idx: 5,
                price: 321.5,
            },
            RightClickTarget::Drawing {
                drawing_id: 42,
                pos,
            },
        ] {
            chart.right_click = Some(target);
            frame(&mut chart, &mut menus, vec![]);
            let (_, text) = frame(&mut chart, &mut menus, vec![]);
            let background = matches!(target, RightClickTarget::Background { .. });
            assert_eq!(
                text.iter().any(|(label, _)| label == "Send to ticket"),
                background
            );
            let label = if background {
                "Send to ticket"
            } else {
                "Open in research"
            };
            let pick = text
                .iter()
                .find(|(text, _)| text == label)
                .expect("host row must be rendered")
                .1;
            if background {
                let ticket = text
                    .iter()
                    .position(|(label, _)| label == "Send to ticket")
                    .unwrap();
                let research = text
                    .iter()
                    .position(|(label, _)| label == "Open in research")
                    .unwrap();
                assert!(ticket < research, "registration order");
            }
            let original = menus.context.unwrap();
            assert_eq!(original.time, Some(time));
            assert_eq!(original.target, target);
            match target {
                RightClickTarget::Background { .. } => assert_eq!(original.price, 123.25),
                RightClickTarget::Element { .. } => assert_eq!(original.price, 321.5),
                RightClickTarget::Drawing { .. } => {
                    // Linear scale: the top pixel is max and the bottom pixel is min.
                    let map = chart.get_chart_mapping();
                    let range = chart.state().price_scale().range();
                    let expected = range.max
                        - (pos.y - map.rect.top()) as f64 / map.rect.height() as f64
                            * (range.max - range.min);
                    assert!((original.price - expected).abs() < 0.00001);
                }
            }
            frame(&mut chart, &mut menus, vec![Event::PointerMoved(pick)]);
            let mut chosen = None;
            for pressed in [true, false] {
                let (action, _) = frame(
                    &mut chart,
                    &mut menus,
                    vec![Event::PointerButton {
                        pos: pick,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
                chosen = chosen.or(action);
            }
            assert_eq!(
                chosen,
                Some(ChartContextAction::Host {
                    id: if background { "ticket" } else { "research" }.into(),
                    context: original,
                })
            );
            assert!(!chart.menu_open);
            assert_eq!(
                frame(&mut chart, &mut menus, vec![]).0,
                None,
                "one action per choice"
            );
        }
    }
}

#[test]
fn axis_right_clicks_open_menus_and_apply_choices() {
    use crate::scales::{PriceScaleMode, TimezoneMode};
    use egui::{Event, PointerButton, RawInput, Shape};
    fn paint(
        ctx: &Context,
        chart: &mut Chart,
        menus: &mut ChartMenus,
        events: Vec<Event>,
        size: Vec2,
    ) -> Vec<egui::epaint::ClippedShape> {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chart.show(ui);
                });
                menus.show(ui.ctx(), chart);
            },
        );
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        shapes
    }
    let click = |pos, button| {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: Default::default(),
            },
            Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    };
    for theme in [Theme::light(), Theme::dark()] {
        for (time, left, ratio, short, pane) in [
            (false, false, false, false, false),
            (false, true, false, false, false),
            (true, false, false, false, false),
            (false, false, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let size = if short {
                Vec2::new(320.0, 240.0)
            } else {
                Vec2::new(800.0, 900.0)
            };
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            chart.config_mut().show_left_axis = left;
            chart.config_mut().show_right_axis = !left;
            let pane = pane
                .then(|| chart.add_indicator(Box::new(crate::studies::RSI::new(14))))
                .and_then(|study| chart.series_pane(study));
            let mut menus = ChartMenus::default();
            paint(&ctx, &mut chart, &mut menus, vec![], size);
            let plot = chart.get_rendered_price_rect();
            let anchor = if time {
                Pos2::new(plot.center().x, plot.bottom() + 10.0)
            } else if pane.is_some() {
                Pos2::new(
                    plot.right() + 25.0,
                    chart.get_rendered_indicator_panes()[0]
                        .panel_rect
                        .center()
                        .y,
                )
            } else {
                Pos2::new(
                    if left {
                        plot.left() - 25.0
                    } else {
                        plot.right() + 25.0
                    },
                    plot.center().y,
                )
            };
            paint(
                &ctx,
                &mut chart,
                &mut menus,
                click(anchor, PointerButton::Secondary),
                size,
            );
            if short {
                paint(
                    &ctx,
                    &mut chart,
                    &mut menus,
                    vec![
                        Event::PointerMoved(Pos2::new(150.0, 120.0)),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            source: egui::MouseWheelSource::Wheel,
                            phase: egui::TouchPhase::Move,
                            delta: Vec2::new(0.0, -2000.0),
                            modifiers: Default::default(),
                        },
                    ],
                    size,
                );
                for _ in 0..30 {
                    paint(&ctx, &mut chart, &mut menus, vec![], size);
                }
            }
            let shapes = paint(&ctx, &mut chart, &mut menus, vec![], size);
            if !time && !short {
                let settings = shapes.iter().find(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == "Scale settings…")).expect("bottom scale row");
                if let Shape::Text(text) = &settings.shape {
                    assert!(
                        settings
                            .clip_rect
                            .contains_rect(Rect::from_min_size(text.pos, text.galley.size())),
                        "a menu that fits must show its bottom row without scrolling"
                    );
                }
            }
            if pane.is_some() {
                assert!(chart.menu_open, "a study pane's axis menu is an open menu");
                // One menu, with the rows of a scale that holds no price chart.
                for (label, count) in [
                    ("Invert scale", 1),
                    ("Lock price to bar ratio", 1),
                    ("Hide unused axes", 1),
                    ("Scale price chart only", 0),
                    ("Last price label", 0),
                ] {
                    let rows = shapes
                        .iter()
                        .filter(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == label))
                        .count();
                    assert_eq!(rows, count, "{label} on a study pane's axis");
                }
            }
            let label = if short {
                "High and low"
            } else if time {
                "Local time"
            } else if ratio {
                "Lock price to bar ratio"
            } else {
                "Percent"
            };
            let text = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == label => Some(text),
                    _ => None,
                })
                .expect("axis menu choice");
            let pos = text.pos + text.galley.size() / 2.0;
            assert!(Rect::from_min_size(Pos2::ZERO, size).contains(pos));
            paint(
                &ctx,
                &mut chart,
                &mut menus,
                click(pos, PointerButton::Primary),
                size,
            );
            assert!(!chart.menu_open);
            let config = chart.config().clone();
            let options = chart.chart_options().clone();
            let scale = *chart.state().price_scale().options();
            assert!(chart.undo(), "axis choices enter chart history");
            assert!(!chart.can_undo(), "one action per axis choice");
            assert!(chart.redo());
            assert_eq!(chart.config(), &config);
            assert_eq!(chart.chart_options(), &options);
            assert_eq!(chart.state().price_scale().options(), &scale);
            if short {
                assert!(chart.config().show_high_low);
            } else if ratio {
                let before = chart.state().price_scale().range().length();
                let (spacing, offset) = chart.get_time_scale_state();
                chart.apply_synced_time_scale(spacing * 2.0, offset);
                paint(&ctx, &mut chart, &mut menus, vec![], size);
                // Doubling the pixels per bar halves the price span at the same pane height.
                assert!(
                    (chart.state().price_scale().range().length() - before / 2.0).abs() < 0.00001
                );
            } else if time {
                assert_eq!(
                    chart.chart_options().time_scale.timezone,
                    TimezoneMode::Local
                );
            } else if let Some(pane) = pane {
                let axis = crate::widget::PaneScale::Named(crate::model::SeriesScale::Right);
                assert_eq!(
                    chart.pane_scale(pane, axis).unwrap().options().mode,
                    PriceScaleMode::Percentage
                );
            } else {
                assert_eq!(
                    chart.state().price_scale().options().mode,
                    PriceScaleMode::Percentage
                );
            }
        }
    }
}

#[test]
fn opening_a_menu_suppresses_hover_paint_in_the_same_frame() {
    use egui::{Event, PointerButton, RawInput, Shape};
    fn uses_ink(shape: &Shape, ink: Color32) -> bool {
        match shape {
            Shape::Vec(shapes) => shapes.iter().any(|shape| uses_ink(shape, ink)),
            Shape::LineSegment { stroke, .. } => stroke.color == ink,
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(ink),
            Shape::Rect(rect) => rect.fill == ink,
            _ => false,
        }
    }
    for (theme, pane) in [
        (Theme::light(), false),
        (Theme::dark(), false),
        (Theme::light(), true),
        (Theme::dark(), true),
    ] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
        if pane {
            chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
        }
        let ink = Color32::from_rgb(17, 41, 83);
        let options = &mut chart.chart_options;
        options.crosshair.vert_line_color = ink;
        options.crosshair.horz_line_color = ink;
        options.crosshair.vert_label_background_color = ink;
        options.crosshair.horz_label_background_color = ink;
        options.tooltip.enabled = true;
        options.tooltip.on_long_press = false;
        options.tooltip.background_color = ink;
        let mut menus = ChartMenus::default();
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    menus.show(ui.ctx(), &mut chart);
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            shapes.iter().any(|shape| uses_ink(&shape.shape, ink))
        };
        frame(vec![]);
        let pos = Pos2::new(400.0, 200.0);
        assert!(
            frame(vec![Event::PointerMoved(pos)]),
            "hover must paint before opening"
        );
        let click = |button| {
            vec![
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: Default::default(),
                },
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        assert!(
            !frame(click(PointerButton::Secondary)),
            "opening frame still paints hover"
        );
        assert!(!frame(vec![]), "open menu still paints hover");
        frame(vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        assert!(frame(vec![]), "closing must restore hover");
    }
}

/// Guards #106: actual profile clicks merge neighbours, extend a composite and split it,
/// preserving unrelated host ranges and making each command one undoable edit.
#[test]
fn tpo_profile_menu_merges_and_splits_sessions() {
    use crate::model::{Bar, BarData, ChartType, Timeframe, to_tpo_profiles};
    use chrono::{DateTime, Duration};
    use egui::{Event, PointerButton, RawInput, Shape};
    for (theme, width) in [
        (Theme::dark(), 900.0),
        (Theme::light(), 900.0),
        (Theme::dark(), 320.0),
        (Theme::light(), 320.0),
    ] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let day = |d| DateTime::UNIX_EPOCH + Duration::days(d);
        let bars: Vec<_> = (0..16)
            .map(|i| {
                Bar::new(
                    day(i / 4) + Duration::minutes(i % 4 * 30),
                    100.0,
                    102.0,
                    100.0,
                    102.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let mut params = chart.chart_type_params().clone();
        // The last loaded session belongs to a separate host range extending into the future.
        params.tpo_composites = vec![day(3)..=day(4)];
        chart.set_chart_type_params(params);
        let mut menus = ChartMenus::default();
        let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
            let mut action = None;
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    action = menus.show(ui.ctx(), chart);
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            (action, shapes)
        };
        frame(&mut chart, &mut menus, vec![]);
        chart.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 18.0));
        frame(&mut chart, &mut menus, vec![]);
        let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
            frame(chart, menus, vec![Event::PointerMoved(pos)]);
            let mut action = None;
            for pressed in [true, false] {
                action = action.or(frame(
                    chart,
                    menus,
                    vec![Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Default::default(),
                    }],
                )
                .0);
            }
            action
        };
        let pick = |chart: &mut Chart, menus: &mut ChartMenus, bar, label: &str| {
            frame(chart, menus, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(
                map.idx_to_x(bar).max(map.rect.left()) + 4.0,
                map.price_to_y(101.0),
            );
            click(chart, menus, pos, PointerButton::Secondary);
            let (_, shapes) = frame(chart, menus, vec![]);
            let row = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no {label} row for profile at {bar}"));
            click(chart, menus, row, PointerButton::Primary)
        };
        let counts = |chart: &Chart| {
            let mut config = chart.chart_type_params().tpo_config(None);
            config.tick_size = 1.0;
            to_tpo_profiles(chart.bars(SeriesId::MAIN), &config, &chart.bar_clock())
                .iter()
                .map(|p| p.tpo_count_at(101.0))
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&chart), [4, 4, 4, 4]);
        assert!(matches!(
            pick(&mut chart, &mut menus, 4, "Merge with previous profile"),
            Some(ChartContextAction::Chart(_))
        ));
        // Four half-hour periods per session, one letter at every row per period.
        assert_eq!(counts(&chart), [8, 4, 4]);
        assert!(chart.undo());
        assert_eq!(counts(&chart), [4, 4, 4, 4]);
        assert!(chart.redo());
        assert_eq!(counts(&chart), [8, 4, 4]);
        pick(&mut chart, &mut menus, 0, "Merge with next profile");
        assert_eq!(counts(&chart), [12, 4]);
        // Pan the composite's opening offscreen; its displayed anchor still opens its menu.
        chart.set_visible_logical_range(crate::model::LogicalRange::new(3.0, 18.0));
        frame(&mut chart, &mut menus, vec![]);
        let map = chart.get_chart_mapping();
        for price in [99.9, 102.1] {
            let pos = Pos2::new(map.rect.left() + 4.0, map.price_to_y(price));
            assert!(map.rect.contains(pos));
            assert!(super::super::tpo::TpoMenu::open(&chart, pos).is_none());
        }
        pick(&mut chart, &mut menus, 0, "Split profile");
        assert_eq!(counts(&chart), [4, 4, 4, 4]);
        assert_eq!(chart.chart_type_params().tpo_composites, [day(3)..=day(4)]);
        assert!(chart.undo());
        assert_eq!(counts(&chart), [12, 4]);
    }
}

/// Guards #1540: picking the row adds one selected horizontal line at the clicked price in the
/// clicked pane as one undo step, and the row is absent while the tool is disabled.
#[test]
fn horizontal_line_row_draws_at_the_clicked_price_and_pane() {
    use crate::drawings::{DrawingToolType, ToolAvailability};
    use crate::model::Timeframe;
    use crate::studies::RSI;
    use crate::widget::PaneId;
    use egui::{Event, PointerButton, RawInput, Shape};
    let ctx = Context::default();
    crate::theme::apply_to_egui(&ctx, &Theme::dark());
    let mut chart = crate::widget::test_frames::chart(&[100.0; 10], &[110.0; 10]);
    chart.set_timeframe(Timeframe::Day1);
    let rsi = chart.add_indicator(Box::new(RSI::new(3)));
    let mut menus = ChartMenus::default();
    let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
                menus.show(ui.ctx(), chart);
            },
        );
        fn flatten(shape: Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.into_iter().for_each(|shape| flatten(shape, out)),
                shape => out.push(shape),
            }
        }
        let mut shapes = Vec::new();
        for clipped in std::mem::take(&mut output.shapes) {
            flatten(clipped.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        shapes
    };
    let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
        frame(chart, menus, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                chart,
                menus,
                vec![Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    };
    // The centre of the row, after a secondary click at `pos`.
    let open_row = |chart: &mut Chart, menus: &mut ChartMenus, pos| {
        click(chart, menus, pos, PointerButton::Secondary);
        frame(chart, menus, vec![])
            .iter()
            .find_map(|shape| match shape {
                Shape::Text(text) if text.galley.text().starts_with("Draw horizontal line at") => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
    };
    frame(&mut chart, &mut menus, vec![]);
    frame(&mut chart, &mut menus, vec![]);

    // Above the bars, whose highs are 110: a click on a bar opens the series menu instead.
    let mapping = chart.get_chart_mapping();
    let pos = Pos2::new(mapping.rect.center().x, mapping.price_to_y(110.5));
    let row = open_row(&mut chart, &mut menus, pos).expect("row in the price pane");
    click(&mut chart, &mut menus, row, PointerButton::Primary);
    let lines = chart.drawings().drawings.clone();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].tool_type, DrawingToolType::HorizontalLine);
    let id = lines[0].id;
    assert!((lines[0].chart_points[0].price - 110.5).abs() < 0.001);
    assert_eq!(chart.drawing_pane(id), Some(PaneId::MAIN));
    assert!(chart.drawings().is_selected(id));
    assert!(chart.undo());
    assert!(chart.drawings().drawings.is_empty());

    let pane = &chart.get_rendered_indicator_panes()[0];
    let pos = Pos2::new(pane.chart_rect.center().x, pane.mapping.price_to_y(50.0));
    let row = open_row(&mut chart, &mut menus, pos).expect("row in the study pane");
    click(&mut chart, &mut menus, row, PointerButton::Primary);
    let lines = chart.drawings().drawings.clone();
    assert_eq!(lines.len(), 1);
    assert!((lines[0].chart_points[0].price - 50.0).abs() < 0.001);
    assert_eq!(chart.drawing_pane(lines[0].id), chart.series_pane(rsi));

    chart.drawings_mut().registry.set_availability(
        DrawingToolType::HorizontalLine,
        ToolAvailability {
            visible: true,
            enabled: false,
        },
    );
    let mapping = chart.get_chart_mapping();
    let pos = Pos2::new(mapping.rect.center().x, mapping.price_to_y(110.5));
    assert_eq!(open_row(&mut chart, &mut menus, pos), None);
}

/// Guards #850: a secondary click on a study's legend row opens the study's menu, and each
/// entry changes the study through the chart's public API or returns its host intent.
#[test]
fn study_menu_entries_act_on_the_study() {
    use crate::model::{AlertCondition, AlertSource, Timeframe};
    use crate::studies::{IndicatorFactory, RSI, SMA, StudyScale};
    use crate::ui::dialogs::StudyContextMenuAction as A;
    use egui::{Event, PointerButton, RawInput, Shape};
    let ctx = Context::default();
    crate::theme::apply_to_egui(&ctx, &Theme::dark());
    // Closes alternate 110 and 120; the last bar, 99, closes at 120.
    let mut chart = crate::widget::test_frames::chart(&[100.0, 104.0], &[110.0, 120.0]);
    chart.symbol = "SYNTH".into();
    chart.set_timeframe(Timeframe::Day1);
    let sma = chart.add_indicator(Box::new(SMA::new(3)));
    let rsi = chart.add_indicator(Box::new(RSI::new(3)));
    let mut menus = ChartMenus::default();
    let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
        let mut action = None;
        let right_click = chart.right_click.take();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
                if right_click.is_some() {
                    chart.right_click = right_click;
                }
                action = menus.show(ui.ctx(), chart);
            },
        );
        fn flatten(shape: Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.into_iter().for_each(|shape| flatten(shape, out)),
                shape => out.push(shape),
            }
        }
        let mut shapes = Vec::new();
        for clipped in std::mem::take(&mut output.shapes) {
            flatten(clipped.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        (action, shapes)
    };
    let text_at = |shapes: &[Shape], label: &str| {
        shapes.iter().find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text().starts_with(label) => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
    };
    let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
        frame(chart, menus, vec![Event::PointerMoved(pos)]);
        let mut action = None;
        for pressed in [true, false] {
            let event = Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            };
            action = action.or(frame(chart, menus, vec![event]).0);
        }
        action
    };
    // Opens the menu of `id` and picks the rows of `path`, hovering each submenu row.
    let pick = |chart: &mut Chart, menus: &mut ChartMenus, id, path: &[&str]| {
        chart.right_click = Some(RightClickTarget::Element {
            id: ChartElementId::Indicator(id),
            bar_idx: 99,
            pos: Pos2::new(120.0, 120.0),
            price: 0.0,
        });
        let (label, submenus) = path.split_last().unwrap();
        for row in submenus {
            let (_, shapes) = frame(chart, menus, vec![]);
            let pos = text_at(&shapes, row).unwrap_or_else(|| panic!("no {row} row"));
            frame(chart, menus, vec![Event::PointerMoved(pos)]);
        }
        let (_, shapes) = frame(chart, menus, vec![]);
        let pos = text_at(&shapes, label).unwrap_or_else(|| panic!("no {label} row"));
        match click(chart, menus, pos, PointerButton::Primary) {
            Some(ChartContextAction::Study { id: picked, action }) if picked == id => action,
            other => panic!("{path:?} returned {other:?}"),
        }
    };
    let study = |chart: &Chart, id| {
        chart
            .indicators()
            .find(|(study, _)| *study == id)
            .map(|(_, study)| study.clone_box())
    };

    // The legend row of the overlay opens its menu.
    frame(&mut chart, &mut menus, vec![]);
    let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
    let row = text_at(&shapes, "SMA").expect("the legend shows the overlay");
    click(&mut chart, &mut menus, row, PointerButton::Secondary);
    let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
    assert!(text_at(&shapes, "Add alert on SMA at 116.67").is_some());
    assert_eq!(
        chart.selected_element(),
        Some(ChartElementId::Indicator(sma))
    );
    frame(
        &mut chart,
        &mut menus,
        vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );

    // SMA(3) at the last bar: (120 + 110 + 120) / 3.
    let A::AddAlert(alert) = pick(&mut chart, &mut menus, sma, &["Add alert on SMA at"]) else {
        panic!("the alert row returns an alert");
    };
    assert_eq!(alert.symbol, "SYNTH");
    assert_eq!(
        alert.source,
        AlertSource::Study {
            id: sma.0.to_string(),
            output: 0
        }
    );
    assert!(
        matches!(alert.condition, AlertCondition::Crossing(level) if (level - 350.0 / 3.0).abs() < 1e-9)
    );
    assert_eq!(
        pick(&mut chart, &mut menus, sma, &["Add indicator on SMA"]),
        A::AddIndicator
    );
    assert_eq!(
        chart.take_indicator_source_request(),
        Some(crate::studies::StudySource::Study { id: sma.0, plot: 0 })
    );
    assert_eq!(chart.take_indicator_source_request(), None);
    let A::AddToFavorites(name) = pick(
        &mut chart,
        &mut menus,
        sma,
        &["Add this indicator to favorites"],
    ) else {
        panic!("the favorites row names the study");
    };
    assert!(IndicatorFactory::new().has(&name), "{name}");

    // In the main pane the price series is one of the items of the visual order.
    let sma_color = study(&chart, sma).unwrap().colors()[0];
    let bullish = chart.config().bullish_color;
    let sma_behind = |chart: &mut Chart, menus: &mut ChartMenus| {
        let (_, shapes) = frame(chart, menus, vec![]);
        let first = |color| {
            shapes.iter().position(|shape| match shape {
                Shape::LineSegment { stroke, .. } => stroke.color == color,
                Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
                _ => false,
            })
        };
        first(sma_color).unwrap() < first(bullish).unwrap()
    };
    assert!(!sma_behind(&mut chart, &mut menus));
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Visual order", "Send to back"],
    );
    assert!(sma_behind(&mut chart, &mut menus));
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Visual order", "Bring forward"],
    );
    assert!(!sma_behind(&mut chart, &mut menus));

    // On the daily chart, the daily interval and above hides the study on intraday ones.
    use Timeframe::*;
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Visibility on intervals", "Current interval and above"],
    );
    assert_eq!(
        study(&chart, sma).unwrap().settings().hidden_intervals,
        [
            Ms100, Ms250, Ms500, Sec1, Sec2, Sec5, Sec10, Sec30, Min1, Min5, Min15, Min30, Hour1,
            Hour4
        ]
    );
    chart.set_timeframe(Timeframe::Custom(
        std::num::NonZeroU64::new(7 * 3600).unwrap(),
    ));
    assert!(
        !study(&chart, sma).unwrap().is_visible(),
        "custom intervals obey the saved lower bound"
    );
    chart.set_timeframe(Timeframe::Custom(
        std::num::NonZeroU64::new(2 * 86400).unwrap(),
    ));
    assert!(study(&chart, sma).unwrap().is_visible());
    chart.set_timeframe(Timeframe::Day1);
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Visibility on intervals", "All intervals"],
    );
    assert!(
        study(&chart, sma)
            .unwrap()
            .settings()
            .hidden_intervals
            .is_empty()
    );

    // Moves between the main pane and new and existing panes, in pane order.
    let pane = |chart: &Chart, id| {
        let at = chart
            .indicators()
            .position(|(study, _)| study == id)
            .unwrap();
        chart
            .get_rendered_indicator_panes()
            .iter()
            .find(|pane| pane.indicator_idx == at)
            .map(|pane| pane.chart_rect)
    };
    pick(&mut chart, &mut menus, sma, &["Move to", "New pane below"]);
    frame(&mut chart, &mut menus, vec![]);
    assert!(pane(&chart, sma).unwrap().top() < pane(&chart, rsi).unwrap().top());
    pick(&mut chart, &mut menus, sma, &["Move to", "RSI pane"]);
    frame(&mut chart, &mut menus, vec![]);
    assert_eq!(pane(&chart, sma), pane(&chart, rsi));
    pick(&mut chart, &mut menus, sma, &["Move to", "New pane above"]);
    frame(&mut chart, &mut menus, vec![]);
    assert!(pane(&chart, sma).unwrap().bottom() <= pane(&chart, rsi).unwrap().top());
    pick(&mut chart, &mut menus, sma, &["Move to", "Main pane"]);
    assert!(study(&chart, sma).unwrap().is_overlay());

    // Stable pane handles also reach empty panes, which stay when a study leaves them.
    use crate::widget::PaneId;
    let empty = chart.add_pane();
    assert!(chart.set_preserve_empty_pane(empty, true));
    assert_eq!(
        pick(&mut chart, &mut menus, sma, &["Move to", "empty pane"]),
        A::MoveTo(crate::ui::dialogs::PaneTarget::Pane(empty))
    );
    assert_eq!(chart.series_pane(sma), Some(empty));
    pick(&mut chart, &mut menus, sma, &["Move to", "New pane below"]);
    let panes = chart.panes();
    let at = panes.iter().position(|pane| *pane == empty).unwrap();
    assert_eq!(chart.series_pane(sma), Some(panes[at + 1]));
    assert!(chart.pane_series(empty).is_empty());
    pick(&mut chart, &mut menus, sma, &["Move to", "Main pane"]);
    assert!(chart.remove_pane(empty));

    // Main can sit below another pane; an overlay's new pane is adjacent to main.
    let rsi_pane = chart.series_pane(rsi).unwrap();
    assert!(chart.move_pane(rsi_pane, 0));
    pick(&mut chart, &mut menus, sma, &["Move to", "New pane above"]);
    let sma_pane = chart.series_pane(sma).unwrap();
    assert_eq!(chart.panes(), [rsi_pane, sma_pane, PaneId::MAIN]);
    assert!(chart.undo());
    assert_eq!(chart.series_pane(sma), Some(PaneId::MAIN));
    assert_eq!(chart.panes(), [rsi_pane, PaneId::MAIN]);

    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Pin to scale (now right)", "No scale"],
    );
    assert_eq!(study(&chart, sma).unwrap().scale(), StudyScale::None);
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Pin to scale (now no scale)", "Pin to new scale"],
    );
    assert_eq!(
        study(&chart, sma).unwrap().scale(),
        StudyScale::Shared(crate::model::SeriesScale::Separate(0))
    );

    let volume = chart
        .indicators()
        .find(|(_, study)| study.is_volume())
        .unwrap()
        .0;
    for (label, scale) in [
        ("Pin to left scale", crate::model::SeriesScale::Left),
        ("Pin to right scale", crate::model::SeriesScale::Right),
    ] {
        let now = if label.contains("left") {
            "Pin to scale (now no scale)"
        } else {
            "Pin to scale (now left)"
        };
        pick(&mut chart, &mut menus, volume, &[now, label]);
        assert_eq!(
            study(&chart, volume).unwrap().scale(),
            StudyScale::Shared(scale)
        );
    }
    assert_eq!(pick(&mut chart, &mut menus, sma, &["Copy"]), A::Copy);
    pick(&mut chart, &mut menus, sma, &["Hide"]);
    assert!(!study(&chart, sma).unwrap().is_visible());
    pick(&mut chart, &mut menus, sma, &["Show"]);
    assert!(study(&chart, sma).unwrap().is_visible());
    pick(&mut chart, &mut menus, rsi, &["Settings…"]);
    assert_eq!(chart.take_indicator_settings_request(), Some(rsi));
    assert_eq!(
        pick(&mut chart, &mut menus, rsi, &["Object tree"]),
        A::ObjectTree
    );
    pick(&mut chart, &mut menus, rsi, &["Remove"]);
    assert!(study(&chart, rsi).is_none());
    assert!(chart.can_undo(), "menu edits are undoable");
}

#[test]
fn pointer_button_opens_price_actions_in_each_pane_and_keeps_the_original_price() {
    for pane in ["price", "study", "series", "empty"] {
        for pick in ["line", "alert", "order"] {
            // Only a value in the pane of the main series is a price of the chart symbol.
            if pane != "price" && pick != "line" {
                continue;
            }
            for command in [
                Modifiers::CTRL.plus(Modifiers::COMMAND),
                Modifiers::MAC_CMD.plus(Modifiers::COMMAND),
            ] {
                let ctx = Context::default();
                let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
                chart.set_symbol("TEST");
                chart.config_mut().show_plus_button = false;
                let target = match pane {
                    "study" => {
                        let id = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                        Some(chart.series_pane(id).unwrap())
                    }
                    "series" => {
                        let id = chart.add_series(crate::widget::SeriesOptions::default());
                        chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                        let target = chart.add_pane();
                        chart.move_series_to_pane(id, target);
                        Some(target)
                    }
                    "empty" => {
                        let target = chart.add_pane();
                        chart.set_preserve_empty_pane(target, true);
                        Some(target)
                    }
                    _ => None,
                };
                let mut menus = ChartMenus::default();
                let screen_height = std::cell::Cell::new(600.0);
                let mut frame = |chart: &mut Chart, events| {
                    let mut action = None;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(800.0, screen_height.get()),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                            action = menus.show(ui.ctx(), chart);
                        },
                    );
                    (output, action)
                };
                for _ in 0..3 {
                    frame(&mut chart, vec![]).0.drop_without_applying_deltas();
                }
                // The two panes share the available height in a 3:1 ratio.
                let pos = if target.is_some() {
                    egui::pos2(350.0, 530.0)
                } else {
                    egui::pos2(350.0, 250.0)
                };
                let map = target
                    .and_then(|pane| chart.pane_series(pane).first().copied())
                    .and_then(|id| chart.series_price_map(id))
                    .unwrap_or_else(|| chart.get_chart_mapping().price);
                let range = map.range();
                let top = map.price_to_y(range.max);
                let bottom = map.price_to_y(range.min);
                let price =
                    range.max - f64::from((pos.y - top) / (bottom - top)) * (range.max - range.min);
                frame(&mut chart, vec![Event::PointerMoved(pos)])
                    .0
                    .drop_without_applying_deltas();
                let hovered = chart.get_hover_bar_idx();
                let modifiers = command.plus(Modifiers::ALT);
                for _ in 0..3 {
                    frame(
                        &mut chart,
                        vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(pos)],
                    )
                    .0
                    .drop_without_applying_deltas();
                    if pane == "price" && pick == "line" {
                        assert!(hovered.is_some());
                        assert_eq!(
                            chart.get_hover_bar_idx(),
                            hovered,
                            "revealing the button keeps the readout"
                        );
                    }
                }
                for pressed in [true, false] {
                    frame(
                        &mut chart,
                        vec![Event::PointerButton {
                            pos,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers,
                        }],
                    )
                    .0
                    .drop_without_applying_deltas();
                }
                if pane == "study" && pick == "line" {
                    screen_height.set(900.0);
                }
                let output = frame(
                    &mut chart,
                    vec![
                        Event::ModifiersChanged(Modifiers::NONE),
                        Event::PointerMoved(egui::pos2(20.0, 20.0)),
                    ],
                )
                .0;
                let prefix = match pick {
                    "line" => "Draw horizontal line",
                    "alert" => "Add alert on",
                    _ => "Sell ",
                };
                let mut point = None;
                fn find(shape: &egui::Shape, prefix: &str, point: &mut Option<Pos2>) {
                    match shape {
                        egui::Shape::Text(text) if text.galley.text().starts_with(prefix) => {
                            *point = Some(text.pos + egui::vec2(60.0, 6.0))
                        }
                        egui::Shape::Vec(shapes) => {
                            shapes.iter().for_each(|shape| find(shape, prefix, point))
                        }
                        _ => {}
                    }
                }
                for shape in &output.shapes {
                    find(&shape.shape, prefix, &mut point);
                }
                output.drop_without_applying_deltas();
                let point = point.unwrap_or_else(|| panic!("missing {pick} in {pane}"));
                frame(
                    &mut chart,
                    vec![
                        Event::PointerMoved(point),
                        Event::PointerButton {
                            pos: point,
                            pressed: true,
                            button: PointerButton::Primary,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                )
                .0
                .drop_without_applying_deltas();
                let (output, action) = frame(
                    &mut chart,
                    vec![Event::PointerButton {
                        pos: point,
                        pressed: false,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    }],
                );
                output.drop_without_applying_deltas();
                assert!(action.is_some(), "{pane} {pick}");
                let action_price = match action.unwrap() {
                    ChartContextAction::Chart(
                        ContextMenuAction::DrawHorizontalLine(price)
                        | ContextMenuAction::AddAlert { price, .. },
                    ) => price,
                    ChartContextAction::Chart(ContextMenuAction::Order(request)) => {
                        match request.order_type {
                            crate::model::OrderType::Limit(price)
                            | crate::model::OrderType::Stop(price) => price,
                            _ => panic!("unexpected order"),
                        }
                    }
                    other => panic!("unexpected {other:?}"),
                };
                assert!(action_price.is_finite());
                if pane != "empty" {
                    assert!(
                        (action_price - price).abs() < 1e-4,
                        "{pane}: {action_price} != {price}"
                    );
                }
                if pick == "line" {
                    let drawing = chart.drawings().drawings.last().unwrap();
                    assert_eq!(drawing.chart_points[0].price, action_price);
                    if let Some(member) =
                        target.and_then(|pane| chart.pane_series(pane).first().copied())
                    {
                        assert_eq!(drawing.pane, Some(member.0));
                    } else {
                        assert_eq!(drawing.pane.is_some(), target.is_some());
                    }
                    assert!(chart.undo());
                    assert!(chart.drawings().drawings.is_empty());
                }
            }
        }
    }
    for owner in ["editor", "host", "drawing", "disabled"] {
        let ctx = Context::default();
        let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
        let mut menus = ChartMenus::default();
        let mut text = String::from("draft");
        if owner == "drawing" {
            chart
                .drawings_mut()
                .set_active_tool(Some(crate::drawings::DrawingToolType::TrendLine));
        }
        let pos = egui::pos2(350.0, 250.0);
        let modifiers = Modifiers::ALT
            .plus(Modifiers::CTRL)
            .plus(Modifiers::COMMAND);
        for pressed in [None, None, Some(true), Some(false)] {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    events: vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(pos)]
                        .into_iter()
                        .chain(pressed.map(|pressed| Event::PointerButton {
                            pos,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers,
                        }))
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    if owner == "editor" {
                        ui.text_edit_singleline(&mut text).request_focus();
                    } else if owner == "host" {
                        chart.suppress_shortcuts(ui.ctx());
                    } else if owner == "disabled" {
                        ui.disable();
                    }
                    chart.show(ui);
                    menus.show(ui.ctx(), &mut chart);
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(menus.context.is_none(), "{owner} retains the gesture");
        assert_eq!(text, "draft");
    }
}

#[test]
fn study_and_series_panes_offer_no_main_symbol_alerts_or_orders() {
    // y 250 lies in the upper pane and y 530 in the lower one.
    for (pane, secondary, y, trading) in [
        ("price", false, 250.0, true),
        ("study", false, 530.0, false),
        ("study", true, 530.0, false),
        ("series", false, 530.0, false),
        ("empty", false, 530.0, false),
        ("moved main series", false, 530.0, true),
    ] {
        let ctx = Context::default();
        // TEST trades between 100 and 110; OTHER, in its own pane, between 400 and 410.
        let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
        chart.set_symbol("TEST");
        match pane {
            "study" => {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            }
            "series" => {
                let id = chart.add_series(crate::widget::SeriesOptions {
                    name: "OTHER".into(),
                    ..Default::default()
                });
                let bars: Vec<_> = chart
                    .bars(SeriesId::MAIN)
                    .iter()
                    .map(|bar| {
                        crate::model::Bar::new(
                            bar.time,
                            bar.open + 300.0,
                            bar.high + 300.0,
                            bar.low + 300.0,
                            bar.close + 300.0,
                            bar.volume,
                        )
                    })
                    .collect();
                chart.set_data(id, bars);
                let target = chart.add_pane();
                chart.move_series_to_pane(id, target);
            }
            "empty" => {
                let target = chart.add_pane();
                chart.set_preserve_empty_pane(target, true);
            }
            "moved main series" => {
                let target = chart.add_pane();
                chart.move_series_to_pane(SeriesId::MAIN, target);
            }
            _ => {}
        }
        let mut menus = ChartMenus::default();
        menus.chart.watchlists = vec!["Tech".into()];
        let mut frame = |chart: &mut Chart, events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    menus.show(ui.ctx(), chart);
                },
            )
        };
        for _ in 0..3 {
            frame(&mut chart, vec![]).drop_without_applying_deltas();
        }
        let pos = egui::pos2(350.0, y);
        let modifiers = if secondary {
            Modifiers::NONE
        } else {
            // Alt and the command modifier reveal the price button at the pointer.
            Modifiers::CTRL
                .plus(Modifiers::COMMAND)
                .plus(Modifiers::ALT)
        };
        for _ in 0..3 {
            frame(
                &mut chart,
                vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(pos)],
            )
            .drop_without_applying_deltas();
        }
        for pressed in [true, false] {
            frame(
                &mut chart,
                vec![Event::PointerButton {
                    pos,
                    pressed,
                    button: if secondary {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    },
                    modifiers,
                }],
            )
            .drop_without_applying_deltas();
        }
        let output = frame(
            &mut chart,
            vec![
                Event::ModifiersChanged(Modifiers::NONE),
                Event::PointerMoved(egui::pos2(20.0, 20.0)),
            ],
        );
        fn collect(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => labels.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, labels)),
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut labels);
        }
        output.drop_without_applying_deltas();
        let count = |prefix: &str| {
            labels
                .iter()
                .filter(|label| label.starts_with(prefix))
                .count()
        };
        let case = format!("{pane}, right-click {secondary}: {labels:?}");
        assert_eq!(count("Draw horizontal line"), 1, "{case}");
        // The right-click menu lists the watchlist row, right in any pane.
        assert_eq!(
            count("Add TEST to watchlist"),
            usize::from(secondary),
            "{case}"
        );
        assert_eq!(count("Add alert on"), usize::from(trading), "{case}");
        assert_eq!(
            count("Buy ") + count("Sell "),
            2 * usize::from(trading),
            "{case}"
        );
    }
}
