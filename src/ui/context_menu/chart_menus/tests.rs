use super::*;
use crate::theme::Theme;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

#[test]
fn stop_limit_orders_keep_their_requests_and_host_text_through_menu_rows() {
    use crate::model::{OrderRequest, OrderSide, OrderType};

    struct Lots;
    impl TradingFormatter for Lots {
        fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
            (symbol == "AAPL").then(|| format!("{} lots", quantity / 100.0))
        }
    }

    let ctx = Context::default();
    let chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
    for (click, side, stop, limit, label) in [
        (101.1, OrderSide::Buy, 101.0, 101.25, "Buy"),
        (98.9, OrderSide::Sell, 99.0, 98.75, "Sell"),
    ] {
        for kind in ["chart", "custom", "pointer"] {
            let mut menus = ChartMenus::default();
            menus.set_order_entries(
                1000.0,
                Capabilities {
                    limit: false,
                    stop: false,
                    stop_limit: true,
                    ..Default::default()
                },
            );
            menus.set_order_formatter(Some(Arc::new(Lots)));
            menus.chart.symbol = "AAPL".into();
            menus.chart.market_price = Some(100.0);
            menus.chart.tick_size = Some(0.25);
            menus.chart.open(Pos2::ZERO, click);
            if kind == "custom" {
                menus.set_menu_rows(|_, rows| {
                    assert!(
                        rows.iter()
                            .any(|row| row.id() == Some(MenuRowId::StopLimitOrder))
                    );
                    vec![MenuRow::BuiltIn(MenuRowId::StopLimitOrder)]
                });
            } else if kind == "pointer" {
                menus.price_menu = true;
                menus.set_menu_rows(|_, _| panic!("pointer price menus keep their fixed rows"));
            }
            let target = RightClickTarget::Background {
                pos: Pos2::ZERO,
                price: click,
            };
            let orders: Vec<_> = menus
                .build_menu_items(&ctx, &chart, HostMenuContext::new(target, None, click))
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Item(MenuItem {
                        label,
                        action: ChartContextAction::Chart(ContextMenuAction::Order(order)),
                        ..
                    }) => Some((label, order)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                orders,
                [(
                    format!("{label} 10 lots AAPL @ {stop:.2} stop limit {limit:.2}"),
                    OrderRequest::new("AAPL", side, 1000.0, OrderType::StopLimit { stop, limit }),
                )],
                "{kind} at {click}"
            );
        }
    }
}

#[test]
fn watchlist_entry_lists_each_list_by_index_and_needs_a_symbol_and_a_list() {
    let mut menu = ChartContextMenu::new();
    let submenu = |menu: &ChartContextMenu, shortcuts: &crate::widget::ShortcutMap| {
        menu.build_menu_items(
            &Context::default(),
            shortcuts,
            &crate::ui::context_menu::format_price,
        )
        .into_iter()
        .find_map(|entry| match entry {
            MenuEntry::Submenu { label, entries, .. } => Some((
                label,
                entries
                    .into_iter()
                    .filter_map(|entry| match entry {
                        MenuEntry::Item(item) => Some((item.action, item.shortcut)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
    };
    let mut shortcuts = crate::widget::ShortcutMap::default();
    menu.watchlists = vec!["Tech".into(), "Energy".into()];
    assert_eq!(submenu(&menu, &shortcuts), None, "no symbol");
    menu.symbol = "AAPL".into();
    let add = |list| ContextMenuAction::AddToWatchlist {
        list,
        symbol: "AAPL".into(),
    };
    assert_eq!(
        submenu(&menu, &shortcuts),
        Some((
            "Add AAPL to watchlist".into(),
            vec![(add(0), None), (add(1), None)]
        ))
    );
    menu.active_watchlist = Some(1);
    assert_eq!(
        submenu(&menu, &shortcuts),
        Some((
            "Add AAPL to watchlist".into(),
            vec![
                (add(0), None),
                (
                    add(1),
                    Some(egui::KeyboardShortcut::new(
                        egui::Modifiers::ALT,
                        egui::Key::W
                    ))
                )
            ]
        ))
    );
    shortcuts
        .bindings
        .retain(|(_, action)| *action != KeyboardAction::AddToWatchlist);
    for binding in [
        None,
        Some(egui::KeyboardShortcut::new(
            egui::Modifiers::ALT,
            egui::Key::G,
        )),
    ] {
        if let Some(binding) = binding {
            shortcuts.bindings.retain(|(key, _)| *key != binding);
            shortcuts
                .bindings
                .push((binding, KeyboardAction::AddToWatchlist));
        }
        assert_eq!(
            submenu(&menu, &shortcuts),
            Some((
                "Add AAPL to watchlist".into(),
                vec![(add(0), None), (add(1), binding)]
            ))
        );
    }
    menu.active_watchlist = None;
    assert_eq!(
        submenu(&menu, &shortcuts),
        Some((
            "Add AAPL to watchlist".into(),
            vec![(add(0), None), (add(1), None)]
        ))
    );
    menu.watchlists.clear();
    assert_eq!(submenu(&menu, &shortcuts), None, "no lists");
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
                                .build_menu_items(&ctx, &chart, menus.context.unwrap())
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

/// Guards #3245 and #3350: hiding `Alerts` drops the drawing menu's alert row with its separator,
/// including the toolbar's More menu when given the same availability. Hiding `Settings` drops
/// the chart's series, study row and scale settings buttons from paint and Tab order, and an
/// added series row's menu then requests nothing. Other buttons stay; showing restores them.
#[test]
fn hidden_alerts_and_settings_leave_the_drawing_menu_and_chart() {
    fn drawing_menu_head(chart: &mut Chart, menus: &mut ChartMenus) -> Vec<String> {
        let ctx = Context::default();
        chart.right_click = Some(RightClickTarget::Drawing {
            drawing_id: 7,
            pos: Pos2::new(30.0, 30.0),
        });
        let output = ctx.run_ui(Default::default(), |ui| {
            menus.show(ui.ctx(), chart);
        });
        output.drop_without_applying_deltas();
        menus
            .build_menu_items(&ctx, chart, menus.context.unwrap())
            .into_iter()
            .take(3)
            .map(|entry| match entry {
                MenuEntry::Item(item) => item.label,
                MenuEntry::Separator => "-".into(),
                MenuEntry::Submenu { label, .. } => label,
            })
            .collect()
    }
    fn settings_focus_order(chart: &mut Chart, menus: &mut ChartMenus) -> Vec<String> {
        const NAMES: [&str; 4] = [
            "Hide series",
            "Series settings",
            "Remove series",
            "Scale settings",
        ];
        let ctx = Context::default();
        let mut order = Vec::new();
        for stop in 0..40 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                time: Some(stop as f64 * 0.01),
                events: vec![Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chart.show(ui);
                });
                menus.show(ui.ctx(), chart);
            });
            for event in &output.platform_output.events {
                if let egui::output::OutputEvent::FocusGained(info) = event
                    && let Some(label) = info.label.as_deref()
                    && NAMES.contains(&label)
                    && !order.iter().any(|seen| seen == label)
                {
                    order.push(label.to_owned());
                }
            }
            output.drop_without_applying_deltas();
        }
        order
    }
    fn study_row_buttons(chart: &mut Chart, _: &mut ChartMenus) -> Vec<String> {
        if chart.indicators().next().is_none() {
            chart.config_mut().show_volume = false;
            chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
            chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            let series = chart.add_series(crate::widget::SeriesOptions {
                name: "Other".into(),
                ..Default::default()
            });
            chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(series, pane));
        }
        const NAMES: [&str; 4] = [
            "Hide study",
            "Study settings",
            "Remove study",
            "More actions",
        ];
        let ctx = Context::default();
        let frame = |chart: &mut Chart, events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                },
            )
        };
        let mut order = Vec::new();
        let mut seen = Vec::new();
        for _ in 0..80 {
            let output = frame(
                chart,
                vec![Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            for event in &output.platform_output.events {
                if let egui::output::OutputEvent::FocusGained(info) = event
                    && let Some(label) = info.label.as_deref()
                    && NAMES.contains(&label)
                    && let Some(id) = ctx.memory(|memory| memory.focused())
                    && !seen.contains(&id)
                {
                    seen.push(id);
                    order.push(label.to_owned());
                }
            }
            output.drop_without_applying_deltas();
        }
        let output = frame(chart, Vec::new());
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("Other") => {
                    Some(text.pos + Vec2::new(4.0, text.galley.size().y / 2.0))
                }
                _ => None,
            })
            .expect("added series legend");
        output.drop_without_applying_deltas();
        let more = ctx
            .read_response(*seen.last().expect("series row actions"))
            .unwrap()
            .rect
            .center();
        // The pane's collapse header can take a secondary click on the title; also exercise
        // More directly so the fixture always reaches the series row's settings dispatch.
        for (pos, button) in [
            (pos, PointerButton::Secondary),
            (more, PointerButton::Primary),
        ] {
            frame(chart, vec![Event::PointerMoved(pos)]).drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(
                    chart,
                    vec![Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    }],
                )
                .drop_without_applying_deltas();
            }
        }
        if chart.take_indicator_settings_request().is_some() {
            order.push("Series settings request".into());
        }
        order
    }
    fn toolbar_more_menu(chart: &mut Chart, menus: &mut ChartMenus) -> Vec<String> {
        let ctx = Context::default();
        chart.drawings_mut().select(7);
        let mut toolbar = crate::ui::floating_toolbar::FloatingSelectionToolbar::new();
        toolbar.set_control_visible(
            Control::Alerts,
            !menus.drawing.hidden.contains(&Control::Alerts),
        );
        let mut frame = |events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    toolbar.show_and_apply(ui, chart);
                },
            )
        };
        for _ in 0..2 {
            frame(Vec::new()).drop_without_applying_deltas();
        }
        for _ in 0..20 {
            let output = frame(vec![Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
            let more = output.platform_output.events.iter().any(|event| {
                matches!(event, egui::output::OutputEvent::FocusGained(info)
                    if info.label.as_deref() == Some("More"))
            });
            output.drop_without_applying_deltas();
            if more {
                break;
            }
        }
        frame(vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }])
        .drop_without_applying_deltas();
        frame(Vec::new()).drop_without_applying_deltas();
        let output = frame(Vec::new());
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if ["Add alert on Horizontal Line…", "Edit…"].contains(&text.galley.text()) =>
                {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        labels
    }
    type Observe = fn(&mut Chart, &mut ChartMenus) -> Vec<String>;
    type Row = (
        Control,
        &'static [&'static str],
        &'static [&'static str],
        Observe,
    );
    let rows: [Row; 4] = [
        (
            Control::Alerts,
            &["Add alert on Horizontal Line…", "-", "Edit…"],
            &["Edit…", "-", "Bring to front"],
            drawing_menu_head,
        ),
        (
            Control::Settings,
            &[
                "Hide series",
                "Series settings",
                "Remove series",
                "Scale settings",
            ],
            &["Hide series", "Remove series"],
            settings_focus_order,
        ),
        (
            Control::Settings,
            &[
                "Hide study",
                "Study settings",
                "Remove study",
                "More actions",
                "Hide study",
                "Study settings",
                "Remove study",
                "More actions",
                "Hide study",
                "Study settings",
                "Remove study",
                "More actions",
                "Series settings request",
            ],
            &[
                "Hide study",
                "Remove study",
                "More actions",
                "Hide study",
                "Remove study",
                "More actions",
                "Hide study",
                "Remove study",
                "More actions",
            ],
            study_row_buttons,
        ),
        (
            Control::Alerts,
            &["Add alert on Horizontal Line…", "Edit…"],
            &["Edit…"],
            toolbar_more_menu,
        ),
    ];
    let mut failures = Vec::new();
    for (control, shown, hidden, observe) in rows {
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        chart.symbol = "SYNTH".into();
        let mut line =
            crate::drawings::Drawing::new(7, crate::drawings::DrawingToolType::HorizontalLine);
        line.completed = true;
        line.chart_points = vec![crate::drawings::ChartPoint::new(0.0, 105.0)];
        chart.drawings_mut().drawings.push(line);
        let mut menus = ChartMenus::default();
        for visible in [true, false, true] {
            menus.set_control_visible(control, visible);
            chart.set_settings_buttons_visible(control != Control::Settings || visible);
            let observed = observe(&mut chart, &mut menus);
            let expected = if visible { shown } else { hidden };
            if observed != expected {
                failures.push(format!(
                    "{control:?} visible: {visible}: {observed:?}, expected {expected:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            assert_eq!(
                original.menu,
                match target {
                    RightClickTarget::Background { .. } => ChartMenuKind::Chart,
                    RightClickTarget::Element { .. } => ChartMenuKind::Series,
                    RightClickTarget::Drawing { .. } => ChartMenuKind::Drawing,
                }
            );
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

/// Guards #3241: hosts place rows among built-ins, omit individual rows, customize axes and
/// discard duplicate or unavailable ids and empty submenus. Moved built-ins keep their actions;
/// an axis host choice returns the original click without changing chart history.
#[test]
fn host_rows_take_any_place_in_the_chart_and_price_axis_menus() {
    use crate::ui::context_menu::{ChartMenuKind, MenuRow, MenuRowId};
    use egui::Shape;

    fn collect(shape: &Shape, texts: &mut Vec<(String, Pos2)>) {
        match shape {
            Shape::Text(text) => texts.push((
                text.galley.text().to_owned(),
                text.pos + text.galley.size() / 2.0,
            )),
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, texts)),
            _ => {}
        }
    }
    let click = |pos, button| {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    };
    for theme in [Theme::light(), Theme::dark()] {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let mut menus = ChartMenus::default();
        menus.set_menu_rows(|click, mut rows| {
            match click.menu {
                ChartMenuKind::Chart => {
                    rows.retain(|row| row.id() != Some(MenuRowId::ViewDataTable));
                    let at = rows
                        .iter()
                        .position(|row| row.id() == Some(MenuRowId::ChartSettings))
                        .expect("chart settings row");
                    rows.insert(
                        at,
                        MenuRow::Host(MenuItem::new("Send to ticket", "ticket".to_owned())),
                    );
                }
                ChartMenuKind::Series => {
                    rows = vec![
                        MenuRow::Separator,
                        MenuRow::BuiltIn(MenuRowId::ViewDataTable),
                        MenuRow::BuiltIn(MenuRowId::CopyPrice),
                        MenuRow::BuiltIn(MenuRowId::CopyPrice),
                        MenuRow::Separator,
                        MenuRow::Separator,
                        MenuRow::Submenu {
                            id: None,
                            label: "Empty".into(),
                            icon: None,
                            rows: vec![MenuRow::Separator],
                        },
                        MenuRow::BuiltIn(MenuRowId::SeriesSettings),
                        MenuRow::Separator,
                    ];
                }
                ChartMenuKind::PriceAxis { .. } => {
                    rows.insert(
                        1,
                        MenuRow::Host(MenuItem::new("Axis ticket", "axis-ticket".to_owned())),
                    );
                }
                _ => {}
            }
            rows
        });
        let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
            let mut action = None;
            let right_click = chart.right_click.take();
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                    if right_click.is_some() {
                        chart.right_click = right_click;
                    }
                    action = menus.show(ui.ctx(), chart);
                },
            );
            let mut texts = Vec::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut texts);
            }
            output.drop_without_applying_deltas();
            (action, texts)
        };
        frame(&mut chart, &mut menus, vec![]);
        chart.right_click = Some(RightClickTarget::Background {
            pos: Pos2::new(100.0, 100.0),
            price: 105.0,
        });
        frame(&mut chart, &mut menus, vec![]);
        let (_, text) = frame(&mut chart, &mut menus, vec![]);
        let tail = [
            "View Data Table…",
            "Export Data to CSV…",
            "Series Settings…",
            "Send to ticket",
            "Chart Settings…",
        ];
        assert_eq!(
            text.iter()
                .filter(|(label, _)| tail.contains(&label.as_str()))
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            &tail[1..]
        );
        let settings = text
            .iter()
            .find(|(label, _)| label == "Chart Settings…")
            .unwrap()
            .1;
        frame(&mut chart, &mut menus, vec![Event::PointerMoved(settings)]);
        let mut chosen = None;
        for pressed in [true, false] {
            chosen = chosen.or(frame(
                &mut chart,
                &mut menus,
                vec![Event::PointerButton {
                    pos: settings,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            )
            .0);
        }
        assert_eq!(
            chosen,
            Some(ChartContextAction::Chart(ContextMenuAction::OpenSettings))
        );

        chart.right_click = Some(RightClickTarget::Element {
            id: ChartElementId::Series(SeriesId::MAIN),
            bar_idx: 0,
            pos: Pos2::new(100.0, 100.0),
            price: 105.0,
        });
        frame(&mut chart, &mut menus, vec![]);
        let actions = menus
            .build_menu_items(&ctx, &chart, menus.context.unwrap())
            .into_iter()
            .map(|entry| match entry {
                MenuEntry::Item(item) => Some(item.action),
                MenuEntry::Separator => None,
                MenuEntry::Submenu { .. } => panic!("empty submenu must be omitted"),
            })
            .collect::<Vec<_>>();
        use crate::ui::dialogs::SeriesContextMenuAction as Series;
        assert_eq!(
            actions,
            vec![
                Some(ChartContextAction::Series(Series::CopyPrice)),
                None,
                Some(ChartContextAction::Series(Series::OpenSettings))
            ]
        );
        frame(
            &mut chart,
            &mut menus,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!chart.menu_open);

        let plot = chart.get_rendered_price_rect();
        let anchor = Pos2::new(plot.right() + 25.0, plot.center().y);
        frame(
            &mut chart,
            &mut menus,
            click(anchor, PointerButton::Secondary),
        );
        let (_, text) = frame(&mut chart, &mut menus, vec![]);
        let head = ["Auto scale", "Axis ticket", "Scale price chart only"];
        assert_eq!(
            text.iter()
                .filter(|(label, _)| head.contains(&label.as_str()))
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            head
        );
        let ticket = text
            .iter()
            .find(|(label, _)| label == "Axis ticket")
            .unwrap()
            .1;
        let (chosen, _) = frame(
            &mut chart,
            &mut menus,
            click(ticket, PointerButton::Primary),
        );
        let Some(ChartContextAction::Host { id, context }) = chosen else {
            panic!("axis host action");
        };
        assert_eq!(id, "axis-ticket");
        assert_eq!(
            context.menu,
            ChartMenuKind::PriceAxis {
                pane: crate::widget::PaneId::MAIN,
                axis: crate::widget::PaneScale::Named(crate::model::SeriesScale::Right),
            }
        );
        assert_eq!(context.time, None);
        assert_eq!(
            context.target,
            RightClickTarget::Background {
                pos: anchor,
                price: context.price
            }
        );
        // A linear scale maps the top to max and bottom to min; its centre is the midpoint.
        let map = chart.get_chart_mapping();
        let range = chart.state().price_scale().range();
        let expected = range.max
            - (anchor.y - map.rect.top()) as f64 / map.rect.height() as f64
                * (range.max - range.min);
        assert!((context.price - expected).abs() < 0.00001);
        assert!(!chart.can_undo());
        assert!(!chart.menu_open);
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
        for (time, left, ratio, short, pane, hours) in [
            (false, false, false, false, false, false),
            (false, true, false, false, false, false),
            (true, false, false, false, false, false),
            (false, false, true, false, false, false),
            (false, false, false, true, false, false),
            (false, false, false, false, true, false),
            (true, false, false, false, false, true),
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
                    ("Indicator values", 1),
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
                "High and low lines"
            } else if hours {
                "Extended hours"
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
                assert_eq!(
                    (
                        chart.config().show_high_low,
                        chart.config().show_high_low_lines
                    ),
                    (false, Some(true))
                );
            } else if ratio {
                let before = chart.state().price_scale().range().length();
                let (spacing, offset) = chart.get_time_scale_state();
                chart.apply_synced_time_scale(spacing * 2.0, offset);
                paint(&ctx, &mut chart, &mut menus, vec![], size);
                // Doubling the pixels per bar halves the price span at the same pane height.
                assert!(
                    (chart.state().price_scale().range().length() - before / 2.0).abs() < 0.00001
                );
            } else if hours {
                assert!(chart.extended_hours(), "redo applies the choice again");
                assert!(chart.undo());
                assert!(!chart.extended_hours(), "undo restores the earlier session");
                chart.set_extended_hours(true);
                assert!(!chart.can_undo(), "a host call is no user action");
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

/// A touch held on a price axis or on the time axis opens that axis' menu, which stays open once
/// the finger lifts. A drag still zooms the scale and a double tap resets it, and neither opens
/// a menu.
#[test]
fn long_press_opens_the_axis_menus() {
    use egui::{Shape, TouchDeviceId, TouchId, TouchPhase};
    // The main price axis, a study pane's axis, an added series' axis and the time axis.
    let key = crate::model::SeriesScale::Separate(1);
    for axis in ["price", "pane", "series", "time"] {
        let ctx = Context::default();
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let study = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
        if axis == "series" {
            let series = chart.add_series(crate::widget::SeriesOptions {
                scale: key,
                ..Default::default()
            });
            chart.set_data(series, chart.state().data().bars.clone());
        }
        let mut menus = ChartMenus::default();
        let mut now = 0.0;
        let mut frame = |chart: &mut Chart, menus: &mut ChartMenus, wait: f64, events| {
            now += wait;
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    time: Some(now),
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
        };
        frame(&mut chart, &mut menus, 0.0, vec![]);
        let pane = chart.series_pane(study).unwrap();
        let plot = chart.get_rendered_price_rect();
        let panel = chart.get_rendered_indicator_panes()[0].panel_rect;
        let (pos, slide) = match axis {
            "price" => (
                Pos2::new(plot.right() + 25.0, plot.center().y),
                Vec2::new(0.0, 40.0),
            ),
            "pane" => (
                Pos2::new(plot.right() + 25.0, panel.center().y),
                Vec2::new(0.0, 40.0),
            ),
            "series" => (
                Pos2::new(plot.left() - 10.0, plot.center().y),
                Vec2::new(0.0, 40.0),
            ),
            _ => (
                Pos2::new(plot.center().x, panel.bottom() + 10.0),
                Vec2::new(40.0, 0.0),
            ),
        };
        let zoomed = |chart: &Chart| match axis {
            "price" => chart.state().price_scale().is_manual(),
            "pane" => {
                let axis = crate::widget::PaneScale::Named(crate::model::SeriesScale::Right);
                chart.pane_scale(pane, axis).unwrap().is_manual()
            }
            "series" => {
                let axis = crate::widget::PaneScale::Named(key);
                let pane = crate::widget::PaneId::MAIN;
                chart.pane_scale(pane, axis).unwrap().is_manual()
            }
            _ => chart.get_time_scale_state().0 != chart.chart_options().time_scale.bar_spacing,
        };
        assert!(!zoomed(&chart), "{axis}: the scale starts fitted");
        let touch = |phase, pos| Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let press = |pos| {
            vec![
                touch(TouchPhase::Start, pos),
                Event::PointerMoved(pos),
                button(pos, true),
            ]
        };
        let slide_to = |pos| vec![touch(TouchPhase::Move, pos), Event::PointerMoved(pos)];
        let lift = |pos| {
            vec![
                touch(TouchPhase::End, pos),
                button(pos, false),
                Event::PointerGone,
            ]
        };

        // A drag, held at its end for longer than a long press takes.
        frame(&mut chart, &mut menus, 0.1, press(pos));
        frame(&mut chart, &mut menus, 0.1, slide_to(pos + slide / 2.0));
        frame(&mut chart, &mut menus, 0.1, slide_to(pos + slide));
        frame(&mut chart, &mut menus, 1.0, vec![]);
        frame(&mut chart, &mut menus, 0.1, lift(pos + slide));
        assert!(!chart.menu_open, "{axis}: a drag opens no menu");
        assert!(zoomed(&chart), "{axis}: a drag zooms the scale");

        for _ in 0..2 {
            frame(&mut chart, &mut menus, 0.1, press(pos));
            frame(&mut chart, &mut menus, 0.05, lift(pos));
        }
        assert!(!chart.menu_open, "{axis}: a double tap opens no menu");
        assert!(!zoomed(&chart), "{axis}: a double tap resets the scale");

        frame(&mut chart, &mut menus, 1.0, press(pos));
        frame(&mut chart, &mut menus, 0.5, vec![]);
        assert!(!chart.menu_open, "{axis}: a touch held for half a second");
        frame(&mut chart, &mut menus, 0.5, vec![]);
        let shapes = frame(&mut chart, &mut menus, 0.1, lift(pos));
        assert!(
            chart.menu_open,
            "{axis}: the menu stays once the finger lifts"
        );
        let row = if axis == "time" {
            "Session breaks"
        } else {
            "Auto scale"
        };
        assert!(
            shapes.iter().any(
                |shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == row)
            ),
            "{axis}: a held touch opens the menu with {row}"
        );
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

/// Guards #106 and #3200: actual profile clicks merge neighbours, extend a composite and split it,
/// preserving unrelated host ranges and making each command one undoable edit. The commands and
/// their undo keep the resolved ATR size of the price-based chart types. Period splits and
/// resets of every edit each take one undoable step.
#[test]
fn tpo_profile_menu_merges_splits_and_resets_sessions() {
    use super::super::tpo::TpoMenu;
    use crate::model::{Bar, BarData, BoxSizeMethod, ChartType, Timeframe, to_tpo_profiles};
    use crate::widget::{ChartLayoutMode, SavedLayout};
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
        // The Renko ATR(14) brick resolves while the last bar reaches 103, a true range of
        // max(3, 1, 2) = 3 after fifteen of 2: 2 + (3 - 2) / 14 = 29 / 14. The live correction
        // back to 102 keeps it, as recalculating would give 2.
        let mut wide = bars.clone();
        wide[15].high = 103.0;
        let mut chart = Chart::new(BarData::from_bars(wide));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let mut params = chart.chart_type_params().clone();
        // The last loaded session belongs to a separate host range extending into the future.
        params.tpo_composites = vec![day(3)..=day(4)];
        params.renko_size_method = BoxSizeMethod::Atr {
            period: 14,
            multiplier: 1.0,
        };
        chart.set_chart_type_params(params);
        chart.update(SeriesId::MAIN, bars[15].clone());
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
        let pick_at = |chart: &mut Chart, menus: &mut ChartMenus, pos, label: &str| {
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
                .unwrap_or_else(|| panic!("no {label} row at {pos:?}"));
            click(chart, menus, row, PointerButton::Primary)
        };
        let pick = |chart: &mut Chart, menus: &mut ChartMenus, bar, label: &str| {
            frame(chart, menus, vec![]);
            let map = chart.get_chart_mapping();
            pick_at(
                chart,
                menus,
                Pos2::new(
                    map.idx_to_x(bar).max(map.rect.left()) + 4.0,
                    map.price_to_y(101.0),
                ),
                label,
            )
        };
        let letter = |shapes: &[egui::epaint::ClippedShape],
                      text: &str,
                      xs: std::ops::Range<f32>,
                      y: f32| {
            shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(t) if t.galley.text() == text => {
                        Some(t.pos + t.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .filter(|p| xs.contains(&p.x))
                .min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
        };
        let enabled = |menu: &TpoMenu, label: &str| {
            menu.entries()
                .iter()
                .any(|e| matches!(e, MenuEntry::Item(item) if item.label == label && item.enabled))
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
        let cells = |shapes: &[egui::epaint::ClippedShape], xs: std::ops::Range<f32>, y: f32| {
            let mut cells: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(cell)
                        if (2.0..=24.0).contains(&cell.rect.height())
                            && xs.contains(&cell.rect.center().x)
                            && (cell.rect.center().y - y).abs() < 1.0 =>
                    {
                        Some(cell.rect.center())
                    }
                    _ => None,
                })
                .collect();
            cells.sort_by(|a, b| a.x.total_cmp(&b.x));
            cells
        };
        let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
        let map = chart.get_chart_mapping();
        let row = cells(
            &shapes,
            map.rect.left()..map.idx_to_x(12),
            map.price_to_y(101.0),
        );
        assert_eq!(row.len(), 12);
        pick_at(
            &mut chart,
            &mut menus,
            row[4],
            "Split profile at selected period",
        );
        assert_eq!(counts(&chart), [4, 8, 4]);
        assert_eq!(
            chart.chart_type_params().tpo_composites,
            [day(3)..=day(4), day(1)..=day(2)]
        );
        assert!(chart.chart_type_params().tpo_splits.is_empty());
        assert!(chart.undo());
        assert_eq!(counts(&chart), [12, 4]);
        assert!((chart.chart_type_params().renko_brick_size - 29.0 / 14.0).abs() < 1e-12);

        assert!(chart.redo());
        assert_eq!(counts(&chart), [4, 8, 4]);
        let (_, shapes) = frame(&mut chart, &mut menus, vec![]);
        let map = chart.get_chart_mapping();
        let xs = map.idx_to_x(4).max(map.rect.left())..map.idx_to_x(12);
        let row = cells(&shapes, xs, map.price_to_y(101.0));
        assert_eq!(row.len(), 8);
        pick_at(
            &mut chart,
            &mut menus,
            row[4],
            "Split profile at selected period",
        );
        assert_eq!(counts(&chart), [4, 4, 4, 4]);
        pick(&mut chart, &mut menus, 4, "Merge with previous profile");
        assert_eq!(counts(&chart), [8, 4, 4]);
        pick(&mut chart, &mut menus, 12, "Merge with previous profile");
        let pairs = [day(0)..=day(1), day(2)..=day(4)];
        assert_eq!(counts(&chart), [8, 8]);
        assert_eq!(chart.chart_type_params().tpo_composites, pairs);
        assert!(matches!(
            pick(&mut chart, &mut menus, 8, "Reset merges and splits"),
            Some(ChartContextAction::Chart(
                ContextMenuAction::ResetProfileEdits
            ))
        ));
        assert_eq!(counts(&chart), [4, 4, 4, 4]);
        assert!(chart.chart_type_params().tpo_composites.is_empty());
        assert!(chart.chart_type_params().tpo_splits.is_empty());
        frame(&mut chart, &mut menus, vec![]);
        let map = chart.get_chart_mapping();
        let menu = TpoMenu::open(
            &chart,
            Pos2::new(map.idx_to_x(8) + 4.0, map.price_to_y(101.0)),
        )
        .unwrap();
        assert!(!enabled(&menu, "Reset merges and splits"));
        assert!(chart.undo());
        assert_eq!(counts(&chart), [8, 8]);
        assert_eq!(chart.chart_type_params().tpo_composites, pairs);

        let at = |m| day(0) + Duration::minutes(m);
        let bars: Vec<_> = [
            Bar::new(at(0), 101.0, 102.0, 101.0, 102.0, 1.0),
            Bar::new(at(30), 101.0, 102.0, 100.0, 101.0, 1.0),
            Bar::new(at(60), 102.0, 103.0, 102.0, 103.0, 1.0),
            Bar::new(at(90), 103.0, 104.0, 103.0, 104.0, 1.0),
        ]
        .into_iter()
        .chain((0..4).map(|i| {
            Bar::new(
                day(1) + Duration::minutes(30 * i),
                100.0,
                102.0,
                100.0,
                102.0,
                1.0,
            )
        }))
        .collect();
        let mut split = Chart::new(BarData::from_bars(bars.clone()));
        split.set_timeframe(Timeframe::Min30);
        split.set_chart_type(ChartType::TimePriceOpportunity);
        let mut split_menus = ChartMenus::default();
        for _ in 0..2 {
            frame(&mut split, &mut split_menus, vec![]);
        }
        split.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 9.0));
        let (_, shapes) = frame(&mut split, &mut split_menus, vec![]);
        let profiles = |chart: &Chart| -> Vec<(DateTime<chrono::Utc>, String, f64)> {
            let mut config = if chart.chart_type() == ChartType::TimePriceOpportunity {
                chart.chart_type_params().tpo_config(None)
            } else {
                chart
                    .indicators()
                    .find_map(|(_, study)| study.tpo())
                    .unwrap()
                    .clone()
            };
            config.tick_size = 1.0;
            let (clock, config) = chart.tpo_division(&config);
            to_tpo_profiles(chart.bars(SeriesId::MAIN), &config, &clock)
                .iter()
                .map(|p| {
                    (
                        p.session_date,
                        p.letters.iter().map(|l| l.letter).collect(),
                        p.poc_price,
                    )
                })
                .collect()
        };
        // Rows 100..=104 count 1,2,3,2,1; splitting at C gives 1,2,2 and 1,2,1.
        // POC ties take the lower row, so the parts have POCs 101 and 103 instead of 102.
        let whole = vec![
            (at(0), "AABBBCCDD".to_owned(), 102.0),
            (day(1), "AAABBBCCCDDD".to_owned(), 100.0),
        ];
        let parts = vec![
            (at(0), "AABBB".to_owned(), 101.0),
            (at(60), "CCDD".to_owned(), 103.0),
            (day(1), "AAABBBCCCDDD".to_owned(), 100.0),
        ];
        assert_eq!(profiles(&split), whole);
        let map = split.get_chart_mapping();
        let day0 = map.rect.left()..map.idx_to_x(4) - map.bar_spacing * 0.5;
        let edge = map.idx_to_x(2) - map.bar_spacing * 0.5;
        let c = letter(&shapes, "C", day0.clone(), map.price_to_y(102.5)).unwrap();
        assert!(c.x < edge);
        let first = letter(&shapes, "A", day0.clone(), map.price_to_y(101.5)).unwrap();
        let menu = TpoMenu::open(&split, first).unwrap();
        assert!(!enabled(&menu, "Split profile at selected period"));
        assert!(!enabled(&menu, "Reset merges and splits"));
        assert!(matches!(
            pick_at(
                &mut split,
                &mut split_menus,
                c,
                "Split profile at selected period"
            ),
            Some(ChartContextAction::Chart(
                ContextMenuAction::SplitTpoProfile
            ))
        ));
        assert_eq!(profiles(&split), parts);
        assert_eq!(split.chart_type_params().tpo_splits, [at(60)]);
        assert!(split.chart_type_params().tpo_composites.is_empty());
        let mut config = split.chart_type_params().tpo_config(None);
        config.tick_size = 1.0;
        let calculated = to_tpo_profiles(split.bars(SeriesId::MAIN), &config, &split.bar_clock());
        // The first part's VA takes rows 101 and 102 (4/5 TPOs); the second takes all three
        // rows (4/4). Only A and B belong to the first hour's initial balance.
        assert_eq!(
            (calculated[0].value_area_low, calculated[0].value_area_high),
            (101.0, 102.0)
        );
        assert_eq!(
            (calculated[1].value_area_low, calculated[1].value_area_high),
            (102.0, 104.0)
        );
        assert_eq!(calculated[0].initial_balance_range(), Some(2.0));
        assert_eq!(calculated[1].initial_balance_range(), None);
        let (_, shapes) = frame(&mut split, &mut split_menus, vec![]);
        let c = letter(&shapes, "C", day0.clone(), map.price_to_y(102.5)).unwrap();
        for shape in &shapes {
            if let Shape::Text(t) = &shape.shape
                && t.galley.text() == "C"
                && day0.contains(&t.pos.x)
            {
                assert!(
                    t.pos.x >= edge,
                    "split C still at the whole profile's anchor"
                );
            }
        }
        let menu = TpoMenu::open(&split, c).unwrap();
        assert!(!enabled(&menu, "Split profile at selected period"));
        assert!(enabled(&menu, "Reset merges and splits"));
        assert!(split.undo());
        assert_eq!(profiles(&split), whole);
        assert!(!split.can_undo());
        assert!(split.redo());
        assert_eq!(profiles(&split), parts);
        let layout = SavedLayout::new(ChartLayoutMode::Single, vec![split.snapshot().unwrap()]);
        let layout = SavedLayout::from_json(&layout.to_json().unwrap()).unwrap();
        let mut restored = Chart::new(BarData::from_bars(bars.clone()));
        restored.restore(&layout.charts[0]).unwrap();
        assert_eq!(restored.chart_type_params().tpo_splits, [at(60)]);
        assert_eq!(profiles(&restored), parts);
        pick_at(
            &mut split,
            &mut split_menus,
            c,
            "Merge with previous profile",
        );
        assert_eq!(profiles(&split), whole);
        assert!(split.chart_type_params().tpo_splits.is_empty());
        assert!(split.chart_type_params().tpo_composites.is_empty());
        assert!(split.undo());
        let (_, shapes) = frame(&mut split, &mut split_menus, vec![]);
        let c = letter(&shapes, "C", day0, map.price_to_y(102.5)).unwrap();
        pick_at(&mut split, &mut split_menus, c, "Merge with next profile");
        assert_eq!(
            profiles(&split),
            [
                (at(0), "AABBB".to_owned(), 101.0),
                (at(60), "CCDDAAABBBCCCDDD".to_owned(), 102.0),
            ]
        );
        let (_, shapes) = frame(&mut split, &mut split_menus, vec![]);
        let a = letter(&shapes, "A", edge..map.rect.right(), map.price_to_y(100.5)).unwrap();
        assert!(
            a.x < map.idx_to_x(4) - map.bar_spacing * 0.5,
            "the next session's letters join the split part's displayed profile"
        );
        let mut merged = Chart::new(BarData::from_bars(bars.clone()));
        merged.set_timeframe(Timeframe::Min30);
        merged.set_chart_type(ChartType::TimePriceOpportunity);
        let mut merged_menus = ChartMenus::default();
        frame(&mut merged, &mut merged_menus, vec![]);
        merged.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 9.0));
        pick(&mut merged, &mut merged_menus, 0, "Merge with next profile");
        let joined = [(at(0), "AABBBCCDDAAABBBCCCDDD".to_owned(), 102.0)];
        assert_eq!(profiles(&merged), joined);
        let (_, shapes) = frame(&mut merged, &mut merged_menus, vec![]);
        let map = merged.get_chart_mapping();
        let a = letter(
            &shapes,
            "A",
            map.rect.left()..map.rect.right(),
            map.price_to_y(101.0),
        )
        .unwrap();
        assert!(!enabled(
            &TpoMenu::open(&merged, a).unwrap(),
            "Split profile at selected period"
        ));
        // Only the second session's C reaches row 101.
        let c = letter(
            &shapes,
            "C",
            map.rect.left()..map.rect.right(),
            map.price_to_y(101.0),
        )
        .unwrap();
        pick_at(
            &mut merged,
            &mut merged_menus,
            c,
            "Split profile at selected period",
        );
        assert_eq!(
            profiles(&merged),
            [
                (at(0), "AABBBCCDDAAABBB".to_owned(), 102.0),
                (day(1) + Duration::minutes(60), "CCCDDD".to_owned(), 100.0)
            ]
        );
        assert_eq!(merged.chart_type_params().tpo_composites, [day(0)..=day(1)]);
        assert_eq!(
            merged.chart_type_params().tpo_splits,
            [day(1) + Duration::minutes(60)]
        );
        let mut config = merged.chart_type_params().tpo_config(Some(1.0));
        config.tick_size = 1.0;
        assert_eq!(
            to_tpo_profiles(&bars, &config, &merged.bar_clock())
                .iter()
                .map(|p| (p.value_area_low, p.value_area_high))
                .collect::<Vec<_>>(),
            [(100.0, 102.0), (100.0, 102.0)]
        );
        assert!(merged.undo());
        assert_eq!(profiles(&merged), joined);

        let mut columns = Chart::new(BarData::from_bars(bars.clone()));
        columns.set_timeframe(Timeframe::Min30);
        columns.set_chart_type(ChartType::TimePriceOpportunity);
        columns.set_chart_type_params(crate::model::ChartTypeParams {
            tpo_split_periods: true,
            ..Default::default()
        });
        columns.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 9.0));
        let mut column_menus = ChartMenus::default();
        frame(&mut columns, &mut column_menus, vec![]);
        let (_, shapes) = frame(&mut columns, &mut column_menus, vec![]);
        let map = columns.get_chart_mapping();
        let day0 = map.rect.left()..map.idx_to_x(4) - map.bar_spacing * 0.5;
        let c = letter(&shapes, "C", day0, map.price_to_y(103.0)).unwrap();
        pick_at(
            &mut columns,
            &mut column_menus,
            c,
            "Split profile at selected period",
        );
        assert_eq!(profiles(&columns), parts);

        let mut study_chart = Chart::new(BarData::from_bars(bars.clone()));
        study_chart.set_timeframe(Timeframe::Min30);
        study_chart.set_instrument(Some(crate::model::Instrument::new("X").with_tick_size(1.0)));
        let study_id =
            study_chart.add_indicator(Box::new(crate::studies::TimePriceOpportunity::default()));
        let mut study_menus = ChartMenus::default();
        frame(&mut study_chart, &mut study_menus, vec![]);
        study_chart.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 9.0));
        assert_eq!(profiles(&study_chart), whole);
        pick(
            &mut study_chart,
            &mut study_menus,
            0,
            "Merge with next profile",
        );
        assert_eq!(profiles(&study_chart), joined);
        let (_, shapes) = frame(&mut study_chart, &mut study_menus, vec![]);
        let map = study_chart.get_chart_mapping();
        // Only the second session's C reaches row 101.
        let c = letter(
            &shapes,
            "C",
            map.rect.left()..map.rect.right(),
            map.price_to_y(101.0),
        )
        .unwrap();
        pick_at(
            &mut study_chart,
            &mut study_menus,
            c,
            "Split profile at selected period",
        );
        let divided = [
            (at(0), "AABBBCCDDAAABBB".to_owned(), 102.0),
            (day(1) + Duration::minutes(60), "CCCDDD".to_owned(), 100.0),
        ];
        assert_eq!(profiles(&study_chart), divided);
        // The menu's edits were made for 30-minute periods: another period drops them, and one
        // undo brings both back.
        study_chart.user_action(|chart| {
            let study = chart.indicator_mut(study_id).unwrap();
            let settings = study.settings();
            let period = [("period".into(), crate::studies::InputValue::Length(60))];
            study.apply_settings(&period, settings).unwrap();
        });
        let config = study_chart.indicators().next().unwrap().1.tpo().unwrap();
        assert!(config.composites.is_empty() && config.splits.is_empty());
        assert!(study_chart.undo());
        assert_eq!(profiles(&study_chart), divided);
        assert!(study_chart.undo());
        assert_eq!(profiles(&study_chart), joined);
        assert!(study_chart.undo());
        assert_eq!(profiles(&study_chart), whole);
        assert!(study_chart.redo());
        assert!(study_chart.redo());
        assert_eq!(profiles(&study_chart), divided);
        let layout = SavedLayout::new(
            ChartLayoutMode::Single,
            vec![study_chart.snapshot().unwrap()],
        );
        let saved = SavedLayout::from_json(&layout.to_json().unwrap()).unwrap();
        let mut restored = Chart::new(BarData::from_bars(bars.clone()));
        restored.restore(&saved.charts[0]).unwrap();
        assert_eq!(profiles(&restored), divided);
        let (restored_id, _) = restored.indicators().next().unwrap();
        let mut restored_menus = ChartMenus::default();
        frame(&mut restored, &mut restored_menus, vec![]);
        let map = restored.get_chart_mapping();
        let stale = TpoMenu::open(
            &restored,
            Pos2::new(
                map.idx_to_x(0).max(map.rect.left()) + 4.0,
                map.price_to_y(101.0),
            ),
        )
        .unwrap();
        // A menu opened before another boundary edit must not overwrite the newer input.
        restored
            .indicator_mut(restored_id)
            .unwrap()
            .set_input(
                "splits",
                crate::studies::InputValue::Text("1970-01-02T01:30:00Z".into()),
            )
            .unwrap();
        restored.user_action(|chart| stale.apply(chart, &ContextMenuAction::ResetProfileEdits));
        assert!(
            !restored
                .indicators()
                .next()
                .unwrap()
                .1
                .tpo()
                .unwrap()
                .splits
                .is_empty()
        );
        let config = study_chart
            .indicators()
            .find(|(id, _)| *id == study_id)
            .unwrap()
            .1
            .tpo()
            .unwrap();
        let before = (config.composites.clone(), config.splits.clone());
        for (id, invalid) in [
            ("splits", "bad"),
            ("splits", "1970-01-02T01:30:00Z;bad"),
            (
                "composites",
                "1970-01-03T00:00:00Z/1970-01-04T00:00:00Z;bad",
            ),
            ("composites", "1970-01-03T00:00:00Z/1970-01-02T00:00:00Z"),
        ] {
            assert!(
                study_chart
                    .indicator_mut(study_id)
                    .unwrap()
                    .set_input(id, crate::studies::InputValue::Text(invalid.into()))
                    .is_err()
            );
            let config = study_chart
                .indicators()
                .find(|(id, _)| *id == study_id)
                .unwrap()
                .1
                .tpo()
                .unwrap();
            assert_eq!(
                (&config.composites, &config.splits),
                (&before.0, &before.1),
                "{id}: {invalid}"
            );
        }
        pick(
            &mut study_chart,
            &mut study_menus,
            0,
            "Reset merges and splits",
        );
        assert_eq!(profiles(&study_chart), whole);
        assert!(study_chart.undo());
        assert_eq!(profiles(&study_chart), divided);
        frame(&mut study_chart, &mut study_menus, vec![]);
        let map = study_chart.get_chart_mapping();
        let pos = Pos2::new(
            map.idx_to_x(0).max(map.rect.left()) + 4.0,
            map.price_to_y(101.0),
        );
        assert!(TpoMenu::open(&study_chart, pos).is_some());
        study_chart.set_indicators_hidden(true);
        assert!(TpoMenu::open(&study_chart, pos).is_none());

        // Custom hours keep only 00:30 and 01:00 of each day. Their first bar starts A,
        // independently of the midnight bar that the chart still shows.
        let mut hours = Chart::new(BarData::from_bars(bars.clone()));
        hours.set_timeframe(Timeframe::Min30);
        hours.set_instrument(Some(crate::model::Instrument::new("X").with_tick_size(1.0)));
        let mut study = crate::studies::TimePriceOpportunity::default();
        for (id, value) in [
            ("hours", crate::studies::InputValue::Choice(2)),
            ("session_start", crate::studies::InputValue::Length(30)),
            ("session_end", crate::studies::InputValue::Length(90)),
        ] {
            crate::studies::Indicator::set_input(&mut study, id, value).unwrap();
        }
        hours.add_indicator(Box::new(study));
        let mut hours_menus = ChartMenus::default();
        frame(&mut hours, &mut hours_menus, vec![]);
        hours.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 9.0));
        let unmerged = [
            (at(30), "AAABB".to_owned(), 102.0),
            (day(1) + Duration::minutes(30), "AAABBB".to_owned(), 100.0),
        ];
        assert_eq!(profiles(&hours), unmerged);
        pick(&mut hours, &mut hours_menus, 1, "Merge with next profile");
        assert_eq!(
            profiles(&hours),
            [(at(30), "AAABBAAABBB".to_owned(), 102.0)]
        );
        let (_, shapes) = frame(&mut hours, &mut hours_menus, vec![]);
        let map = hours.get_chart_mapping();
        let b = letter(
            &shapes,
            "B",
            map.rect.left()..map.rect.right(),
            map.price_to_y(101.0),
        )
        .unwrap();
        pick_at(
            &mut hours,
            &mut hours_menus,
            b,
            "Split profile at selected period",
        );
        assert_eq!(
            profiles(&hours),
            [
                (at(30), "AAABBAAA".to_owned(), 102.0),
                (day(1) + Duration::minutes(60), "BBB".to_owned(), 100.0)
            ]
        );
        assert!(hours.undo());
        assert_eq!(profiles(&hours).len(), 1);

        let bars = (0..14)
            .map(|i| {
                Bar::new(
                    day(i / 2) + Duration::minutes(i % 2 * 30),
                    100.0,
                    102.0,
                    100.0,
                    102.0,
                    1.0,
                )
            })
            .collect();
        let mut weekly = Chart::new(BarData::from_bars(bars));
        weekly.set_timeframe(Timeframe::Min30);
        weekly.set_chart_type(ChartType::TimePriceOpportunity);
        weekly.set_chart_type_params(crate::model::ChartTypeParams {
            tpo_profile_period: crate::model::TPOProfilePeriod::Week,
            ..Default::default()
        });
        let mut weekly_menus = ChartMenus::default();
        weekly.set_visible_logical_range(crate::model::LogicalRange::new(-1.0, 15.0));
        frame(&mut weekly, &mut weekly_menus, vec![]);
        assert_eq!(counts(&weekly), [8, 6]);
        pick(
            &mut weekly,
            &mut weekly_menus,
            8,
            "Merge with previous profile",
        );
        assert_eq!(counts(&weekly), [14]);
        assert_eq!(weekly.chart_type_params().tpo_composites, [day(0)..=day(6)]);
        assert!(weekly.undo());
        assert_eq!(counts(&weekly), [8, 6]);
        let (_, shapes) = frame(&mut weekly, &mut weekly_menus, vec![]);
        let map = weekly.get_chart_mapping();
        let b = letter(
            &shapes,
            "B",
            map.rect.left()..map.rect.right(),
            map.price_to_y(101.0),
        )
        .unwrap();
        // The second period of the weekly profile is day zero's B; split the next session's A.
        let a = Pos2::new(
            b.x + (b.x
                - letter(
                    &shapes,
                    "A",
                    map.rect.left()..map.rect.right(),
                    map.price_to_y(101.0),
                )
                .unwrap()
                .x),
            b.y,
        );
        pick_at(
            &mut weekly,
            &mut weekly_menus,
            a,
            "Split profile at selected period",
        );
        assert_eq!(counts(&weekly), [2, 6, 6]);
        assert_eq!(weekly.chart_type_params().tpo_splits, [day(1)]);
        assert!(weekly.undo());
        assert_eq!(counts(&weekly), [8, 6]);
    }
}

/// Merges and splits are made for a period length, row size and profile period: changing one of
/// them in the settings drops the edits carried over, as one undoable step, while other settings
/// and edits supplied with the new settings keep them. The study records what its lists were
/// made for, so the settings dialog's full input list, which still holds the old lists, drops
/// them too, while lists saved before the record apply whatever the settings.
#[test]
fn tpo_edits_drop_when_their_period_or_rows_change() {
    use crate::model::{Bar, BarData, ChartType, ChartTypeParams, TPOProfilePeriod, Timeframe};
    use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
    use crate::widget::settings::ChartSettingsState;
    use chrono::{DateTime, Duration};
    let day = |d| DateTime::UNIX_EPOCH + Duration::days(d);
    let composites = vec![day(0)..=day(1)];
    let splits = vec![day(2) + Duration::minutes(60)];
    let bars: Vec<_> = (0..12)
        .map(|i| {
            let time = day(i / 4) + Duration::minutes(i % 4 * 30);
            Bar::new(time, 100.0, 102.0, 100.0, 102.0, 1.0)
        })
        .collect();
    let chart = || {
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_timeframe(Timeframe::Min30);
        chart
    };
    type Edit = fn(&mut ChartTypeParams);
    let period: Edit = |params| params.tpo_period_minutes = 60;
    let rows: Edit = |params| params.tpo_row_ticks = 2;
    let week: Edit = |params| params.tpo_profile_period = TPOProfilePeriod::Week;
    let value_area: Edit = |params| params.tpo_value_area = 60.0;
    let supplied: Edit = |params| {
        params.tpo_period_minutes = 60;
        params.tpo_splits = vec![DateTime::UNIX_EPOCH + Duration::days(1) + Duration::minutes(60)];
    };
    // (series dialog or chart dialog, change, edits kept)
    for (series_dialog, edit, kept) in [
        (true, period, false),
        (true, rows, false),
        (true, week, false),
        (true, value_area, true),
        (true, supplied, true),
        (false, period, false),
        (false, value_area, true),
    ] {
        let mut chart = chart();
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let mut params = chart.chart_type_params().clone();
        params.tpo_composites = composites.clone();
        params.tpo_splits = splits.clone();
        chart.set_chart_type_params(params.clone());
        let mut changed = params.clone();
        edit(&mut changed);
        chart.user_action(|chart| {
            if series_dialog {
                let mut settings = chart.series_settings();
                settings.chart_type_params = changed.clone();
                chart.apply_series_settings(&settings);
            } else {
                let mut settings = ChartSettingsState::from_chart(chart);
                settings.chart_type_params = Some(changed.clone());
                settings.apply_to_chart(chart).unwrap();
            }
        });
        let mut expected = changed.clone();
        if !kept {
            (expected.tpo_composites, expected.tpo_splits) = (Vec::new(), Vec::new());
        }
        let row = format!("series dialog {series_dialog}, {changed:?}");
        assert_eq!(chart.chart_type_params(), &expected, "{row}");
        assert!(chart.undo());
        assert_eq!(chart.chart_type_params(), &params, "undo, {row}");
        assert!(chart.redo());
        assert_eq!(chart.chart_type_params(), &expected, "redo, {row}");
    }

    let made_for_30 =
        TimePriceOpportunity::profile_edit_inputs(Some((30, 0)), &composites, &splits);
    // (input changed, its new value, the dialog's whole list or that input alone, edits kept)
    for (id, value, whole_list, kept) in [
        ("period", InputValue::Length(60), true, false),
        ("row_ticks", InputValue::Length(2), true, false),
        ("value_area", InputValue::Number(60.0), true, true),
        ("period", InputValue::Length(60), false, false),
    ] {
        let mut chart = chart();
        let id_of_study = chart.add_indicator(Box::new(TimePriceOpportunity::default()));
        let study = chart.indicator_mut(id_of_study).unwrap();
        let settings = study.settings();
        study.apply_settings(&made_for_30, settings).unwrap();
        let read = |chart: &Chart| {
            let study = chart.indicators().next().unwrap().1;
            let inputs: Vec<_> = study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect();
            let config = study.tpo().unwrap();
            (inputs, config.composites.clone(), config.splits.clone())
        };
        let before = read(&chart);
        chart.user_action(|chart| {
            let study = chart.indicator_mut(id_of_study).unwrap();
            let settings = study.settings();
            let inputs: Vec<_> = if whole_list {
                before
                    .0
                    .iter()
                    .map(|(input, old)| {
                        let new = if input == id {
                            value.clone()
                        } else {
                            old.clone()
                        };
                        (input.clone(), new)
                    })
                    .collect()
            } else {
                vec![(id.to_owned(), value.clone())]
            };
            study.apply_settings(&inputs, settings).unwrap();
        });
        let (inputs, edits) = {
            let (inputs, composites, splits) = read(&chart);
            (inputs, (composites, splits))
        };
        let row = format!("{id} {value:?}, whole list {whole_list}");
        if kept {
            assert_eq!(edits, (composites.clone(), splits.clone()), "{row}");
        } else {
            assert_eq!(edits, (Vec::new(), Vec::new()), "{row}");
            for list in ["composites", "splits"] {
                let text = &inputs.iter().find(|(input, _)| input == list).unwrap().1;
                assert_eq!(text, &InputValue::Text(String::new()), "{list}, {row}");
            }
        }
        let after = read(&chart);
        assert!(chart.undo());
        assert_eq!(read(&chart), before, "undo, {row}");
        assert!(chart.redo());
        assert_eq!(read(&chart), after, "redo, {row}");
    }

    // A study without a chart, its inputs set one at a time in any order.
    let text = |text: &str| InputValue::Text(text.into());
    let merge = "1970-01-01T00:00:00Z/1970-01-02T00:00:00Z";
    let split = "1970-01-03T01:00:00Z";
    // Lists restored before the period they were made for apply once it is set.
    let mut tpo = TimePriceOpportunity::default();
    tpo.set_input("composites", text(&format!("60,0|{merge}")))
        .unwrap();
    tpo.set_input("splits", text(&format!("60,0|{split}")))
        .unwrap();
    assert!(tpo.tpo().unwrap().composites.is_empty());
    tpo.set_input("period", InputValue::Length(60)).unwrap();
    assert_eq!(tpo.tpo().unwrap().composites, composites);
    assert_eq!(tpo.tpo().unwrap().splits, [day(2) + Duration::minutes(60)]);
    // A list saved before the record applies under any period and is saved with the current one.
    let mut tpo = TimePriceOpportunity::default();
    tpo.set_input("splits", text(split)).unwrap();
    tpo.set_input("period", InputValue::Length(60)).unwrap();
    assert_eq!(tpo.tpo().unwrap().splits, [day(2) + Duration::minutes(60)]);
    let saved = |tpo: &TimePriceOpportunity, id: &str| {
        tpo.inputs()
            .into_iter()
            .find(|input| input.id == id)
            .unwrap()
            .value
    };
    assert_eq!(saved(&tpo, "splits"), text(&format!("60,0|{split}")));
    // A list made for another period does not apply and is not saved.
    tpo.set_input("composites", text(&format!("30,0|{merge}")))
        .unwrap();
    assert!(tpo.tpo().unwrap().composites.is_empty());
    assert_eq!(saved(&tpo, "composites"), text(""));
    // An unreadable record is rejected and changes nothing.
    for invalid in [
        format!("x,0|{split}"),
        format!("60|{split}"),
        format!("60,0|bad"),
    ] {
        assert!(
            tpo.set_input("splits", text(&invalid)).is_err(),
            "{invalid}"
        );
        assert_eq!(saved(&tpo, "splits"), text(&format!("60,0|{split}")));
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

    // On the daily chart, the daily interval and above hides the study on intraday ones and
    // replaces a daily interval hidden exactly by settings saved before interval ranges.
    let daily = chart.indicator_mut(sma).unwrap();
    let mut settings = daily.settings();
    settings.hidden_intervals = vec![Timeframe::Day1];
    daily.apply_settings(&[], settings).unwrap();
    assert!(!study(&chart, sma).unwrap().is_visible());
    pick(
        &mut chart,
        &mut menus,
        sma,
        &["Visibility on intervals", "Current interval and above"],
    );
    assert!(
        study(&chart, sma)
            .unwrap()
            .settings()
            .hidden_intervals
            .is_empty()
    );
    assert!(study(&chart, sma).unwrap().is_visible());
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
                    // Lines keep the click; alert and order prices use the menu's two decimals.
                    let tolerance = if pick == "line" { 1e-4 } else { 0.005 + 1e-9 };
                    assert!(
                        (action_price - price).abs() < tolerance,
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
        chart.config_mut().navigation_buttons = crate::config::NavigationButtons::Never;
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

/// Alt+Shift+B and Alt+Shift+S return the order of the chart menu's buy or sell limit row at the
/// pointer, rounded to the tick, once, and nothing where the menu offers no such row.
#[test]
fn limit_order_keys_return_the_menu_limit_row_at_the_pointer() {
    use crate::model::{Bar, Instrument, OrderRequest, OrderSide, OrderType};
    use egui::Key::{B, S};
    type Setup = fn(&mut Chart, &mut ChartMenus);
    // (case, setup, pointer, key, the side of the order returned)
    type Row = (&'static str, Setup, Pos2, egui::Key, Option<OrderSide>);
    let none: Setup = |_, _| {};
    // The last bar closes at 40, below the pointer's price.
    let market_below: Setup = |chart, _| {
        let time = chart.bars(SeriesId::MAIN).last().unwrap().time;
        chart.update(SeriesId::MAIN, Bar::new(time, 30.0, 40.0, 30.0, 40.0, 1.0));
    };
    // y 250 lies in the price pane, at a price between 50 and 150, and y 530 in a study pane.
    let price = egui::pos2(350.0, 250.0);
    let rows: [Row; 9] = [
        ("below the market", none, price, B, Some(OrderSide::Buy)),
        ("sell below the market", none, price, S, None),
        (
            "above the market",
            market_below,
            price,
            S,
            Some(OrderSide::Sell),
        ),
        ("buy above the market", market_below, price, B, None),
        (
            "study pane",
            |chart, _| {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            },
            egui::pos2(350.0, 530.0),
            B,
            None,
        ),
        ("price axis", none, egui::pos2(795.0, 250.0), B, None),
        (
            "trading hidden",
            |_, menus| menus.set_control_visible(Control::Trading, false),
            price,
            B,
            None,
        ),
        (
            "limit orders refused",
            |_, menus| {
                let stop = Capabilities {
                    stop: true,
                    ..Capabilities::default()
                };
                menus.set_order_entries(1.0, stop);
            },
            price,
            B,
            None,
        ),
        (
            "menus hidden",
            |_, menus| menus.set_control_visible(Control::ContextMenus, false),
            price,
            B,
            None,
        ),
    ];
    for (case, setup, pos, key, side) in rows {
        let ctx = Context::default();
        // TEST trades between 100 and 110, so with a tick of 100 the pointer's price is 100.
        let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
        chart.set_symbol("TEST");
        chart.set_instrument(Some(Instrument::new("TEST").with_tick_size(100.0)));
        let mut menus = ChartMenus::default();
        setup(&mut chart, &mut menus);
        let mut frame = |chart: &mut Chart, events| {
            let mut action = None;
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    action = menus.show(ui.ctx(), chart);
                },
            )
            .drop_without_applying_deltas();
            action
        };
        for _ in 0..3 {
            frame(&mut chart, vec![]);
        }
        for _ in 0..3 {
            frame(&mut chart, vec![Event::PointerMoved(pos)]);
        }
        let press = Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT.plus(Modifiers::SHIFT),
        };
        let expected = side.map(|side| {
            let order = OrderRequest::new("TEST", side, 1.0, OrderType::Limit(100.0));
            ChartContextAction::Chart(ContextMenuAction::Order(order))
        });
        assert_eq!(frame(&mut chart, vec![press]), expected, "{case}");
        assert_eq!(frame(&mut chart, vec![]), None, "{case}: the next pass");
    }
}

/// Guards #3427: the chart and series menus write the clicked price, and the order rows their
/// price, as the chart's price scale writes prices: at the instrument's precision, with the
/// chart's thousands separator or the host's price formatter, and as a price on a percentage
/// scale, where the rows still copy, alert and order prices. Guards #3737: alert rows act on
/// their written price, on the tick or at the menu's decimals, and Alt+A takes the same price.
#[test]
fn menu_rows_write_and_act_on_prices_as_the_price_scale_does() {
    use crate::model::Instrument;
    use crate::scales::{CustomPriceFormatter, PriceScaleMode};
    type Setup = fn(&mut Chart);
    // (bar low, bar high, tick size, setup, click, click text, action text, alert price)
    type Row = (
        f64,
        f64,
        Option<f64>,
        Setup,
        f64,
        &'static str,
        &'static str,
        f64,
    );
    let rows: [Row; 7] = [
        (
            1.084,
            1.085,
            Some(0.00001),
            |_| {},
            1.084537,
            "1.08454",
            "1.08454",
            1.08454,
        ),
        (
            1230.0,
            1240.0,
            Some(0.25),
            |chart| chart.config_mut().thousands_sep = Some('\''),
            1234.6,
            "1'234.60",
            "1'234.50",
            1234.5,
        ),
        (
            1.084,
            1.085,
            Some(0.00001),
            |chart| {
                chart.set_price_formatter(Some(std::sync::Arc::new(CustomPriceFormatter::new(
                    |price| format!("<{price:.6}>"),
                ))));
            },
            1.084537,
            "<1.084537>",
            "<1.084540>",
            1.08454,
        ),
        (
            1.084,
            1.085,
            Some(0.00001),
            |chart| chart.price_scale_mut().options_mut().mode = PriceScaleMode::Percentage,
            1.084537,
            "1.08454",
            "1.08454",
            1.08454,
        ),
        (
            1.084,
            1.085,
            None,
            |chart| chart.set_instrument(Some(Instrument::new("EURUSD").with_price_decimals(3))),
            1.084437,
            "1.084",
            "1.084",
            1.084,
        ),
        (
            100.0,
            110.0,
            None,
            |_| {},
            104.5671,
            "104.57",
            "104.57",
            104.57,
        ),
        // Exactly halfway: the action rounds away from zero before its text is written.
        (
            0.11,
            0.14,
            None,
            |chart| chart.price_scale_mut().set_price_precision(Some(2)),
            0.125,
            "0.12",
            "0.13",
            0.13,
        ),
    ];
    let ctx = Context::default();
    for (low, high, tick, setup, click, text, order, alert) in rows {
        let mut chart = crate::widget::test_frames::chart(&[low], &[high]);
        chart.symbol = "EURUSD".into();
        let mut instrument = Instrument::new("EURUSD");
        if let Some(tick) = tick {
            instrument = instrument.with_tick_size(tick);
        }
        chart.set_instrument(Some(instrument));
        setup(&mut chart);
        let mut menus = ChartMenus::default();
        let frame = |chart: &mut Chart, menus: &mut ChartMenus, show_chart: bool| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    if show_chart {
                        chart.show(ui);
                    }
                    menus.show(ui.ctx(), chart);
                },
            )
            .drop_without_applying_deltas();
        };
        frame(&mut chart, &mut menus, true);
        frame(&mut chart, &mut menus, true);
        let pos = chart.get_chart_mapping().rect.center();
        for (target, expected) in [
            (
                RightClickTarget::Background { pos, price: click },
                vec![
                    format!("Copy price {text}"),
                    format!("Add alert on EURUSD at {order}…"),
                    format!("Draw horizontal line at {text}"),
                    format!("Buy 1 EURUSD @ {order} limit"),
                    format!("Sell 1 EURUSD @ {order} stop"),
                ],
            ),
            (
                RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    bar_idx: 0,
                    pos,
                    price: click,
                },
                vec![
                    format!("Add alert on EURUSD at {order}…"),
                    format!("Copy price {text}"),
                ],
            ),
        ] {
            // The chart clears an undrained right-click when it is shown.
            chart.right_click = Some(target);
            frame(&mut chart, &mut menus, false);
            let labels: Vec<_> = menus
                .build_menu_items(&ctx, &chart, menus.context.unwrap())
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Item(item) => Some(item.label),
                    _ => None,
                })
                .collect();
            for row in expected {
                assert!(labels.contains(&row), "{row:?} in {labels:?}");
            }
            if matches!(target, RightClickTarget::Background { .. }) {
                let price =
                    menus
                        .build_menu_items(&ctx, &chart, menus.context.unwrap())
                        .into_iter()
                        .find_map(|entry| match entry {
                            MenuEntry::Item(MenuItem {
                                action:
                                    ChartContextAction::Chart(ContextMenuAction::AddAlert {
                                        price, ..
                                    }),
                                ..
                            }) => Some(price),
                            _ => None,
                        })
                        .unwrap();
                assert_eq!(price, alert, "{text}");
            } else {
                assert_eq!(menus.series_alert_price(), alert, "{text}");
                assert_eq!(menus.series_click_price(), click, "{text}");
            }
        }
        // The shortcut reader must agree with the actual menu at this pointer.
        let (_, at) = chart.pointer_price(pos).unwrap();
        chart.right_click = Some(RightClickTarget::Background { pos, price: at });
        frame(&mut chart, &mut menus, false);
        let price = menus
            .build_menu_items(&ctx, &chart, menus.context.unwrap())
            .into_iter()
            .find_map(|entry| match entry {
                MenuEntry::Item(MenuItem {
                    action: ChartContextAction::Chart(ContextMenuAction::AddAlert { price, .. }),
                    ..
                }) => Some(price),
                _ => None,
            })
            .unwrap();
        assert_eq!(chart.alert_price_at(pos), Some(price));
    }

    // Standalone menus also accept prices through their public field after set_context.
    let mut standalone = SeriesContextMenu::new();
    standalone.set_context("EURUSD", 100.0);
    standalone.click_price = 101.1;
    standalone.open(Pos2::new(100.0, 100.0), SeriesId::MAIN);
    let mut labels = Vec::new();
    for _ in 0..2 {
        let output = ctx.run_ui(RawInput::default(), |ui| {
            standalone.show(ui.ctx(), &Theme::dark());
        });
        labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
    }
    for expected in ["Add alert on EURUSD at 101.10…", "Copy price 101.10"] {
        assert!(labels.iter().any(|label| label == expected), "{labels:?}");
    }
}

/// Guards #3531: with `TooltipOptions::long_press_order`, a touch held on the price pane
/// returns the chart menu's limit row at the touched price once, and shows no values, opens no
/// menu, is no click and does not pan; without the option, in a study pane and where the menu
/// has no limit row, the hold shows its values and returns nothing.
#[test]
fn long_press_asks_for_the_menu_limit_row_only_with_its_option_on() {
    use crate::model::{Bar, Instrument, OrderRequest, OrderSide, OrderType};
    use crate::widget::ChartEvent;
    use egui::{TouchDeviceId, TouchId, TouchPhase};
    type Setup = fn(&mut Chart, &mut ChartMenus);
    // (case, option, setup, touched point, the side of the order returned)
    type Row = (&'static str, bool, Setup, Pos2, Option<OrderSide>);
    let none: Setup = |_, _| {};
    // The last bar closes at 40, below the touched price.
    let market_below: Setup = |chart, _| {
        let time = chart.bars(SeriesId::MAIN).last().unwrap().time;
        chart.update(SeriesId::MAIN, Bar::new(time, 30.0, 40.0, 30.0, 40.0, 1.0));
    };
    // y 250 lies in the price pane, at a price between 50 and 150, and y 530 in a study pane.
    let price = egui::pos2(350.0, 250.0);
    let rows: [Row; 7] = [
        ("option off", false, none, price, None),
        ("below the market", true, none, price, Some(OrderSide::Buy)),
        (
            "above the market",
            true,
            market_below,
            price,
            Some(OrderSide::Sell),
        ),
        (
            "study pane",
            true,
            |chart, _| {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            },
            egui::pos2(350.0, 530.0),
            None,
        ),
        (
            "trading hidden",
            true,
            |_, menus| menus.set_control_visible(Control::Trading, false),
            price,
            None,
        ),
        (
            "limit orders refused",
            true,
            |_, menus| {
                let stop = Capabilities {
                    stop: true,
                    ..Capabilities::default()
                };
                menus.set_order_entries(1.0, stop);
            },
            price,
            None,
        ),
        (
            "menus hidden",
            true,
            |_, menus| menus.set_control_visible(Control::ContextMenus, false),
            price,
            None,
        ),
    ];
    for (case, option, setup, pos, side) in rows {
        let ctx = Context::default();
        // TEST trades between 100 and 110, so with a tick of 100 the touched price is 100.
        let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
        chart.set_symbol("TEST");
        chart.set_instrument(Some(Instrument::new("TEST").with_tick_size(100.0)));
        let mut options = chart.chart_options().clone();
        options.tooltip.long_press_order = option;
        chart.set_chart_options(options);
        let mut menus = ChartMenus::default();
        setup(&mut chart, &mut menus);
        let mut frame = |chart: &mut Chart, time, events| {
            let mut action = None;
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    action = menus.show(ui.ctx(), chart);
                },
            )
            .drop_without_applying_deltas();
            action
        };
        // The menus arm the chart at the end of each pass.
        for time in [0.0, 0.1, 0.2] {
            frame(&mut chart, time, vec![]);
        }
        let view = chart.get_time_scale_state();
        let _ = chart.drain_events().count();
        let touch = |phase, pos| Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let moved = pos + egui::vec2(40.0, 0.0);
        let order = side.map(|side| {
            let order = OrderRequest::new("TEST", side, 1.0, OrderType::Limit(100.0));
            ChartContextAction::Chart(ContextMenuAction::Order(order))
        });
        // (time, events, whether the order is due, whether a value hold shows values); the hold
        // delay is 0.5 s, and egui's own long touch fires at 0.8 s.
        for (time, events, due, values) in [
            (
                1.0,
                vec![
                    touch(TouchPhase::Start, pos),
                    Event::PointerMoved(pos),
                    button(pos, true),
                ],
                false,
                false,
            ),
            (1.4, vec![], false, false),
            (1.6, vec![], true, true),
            (1.7, vec![], false, true),
            (1.9, vec![], false, true),
            (
                2.0,
                vec![touch(TouchPhase::Move, moved), Event::PointerMoved(moved)],
                false,
                true,
            ),
            (
                2.1,
                vec![
                    touch(TouchPhase::End, moved),
                    button(moved, false),
                    Event::PointerGone,
                ],
                false,
                false,
            ),
            (2.2, vec![], false, false),
        ] {
            let action = frame(&mut chart, time, events);
            let expected = if due { order.clone() } else { None };
            assert_eq!(action, expected, "{case} at {time}");
            assert!(!chart.menu_open, "{case} at {time}: the hold opens no menu");
            if order.is_some() {
                assert_eq!(
                    chart.hover_time(),
                    None,
                    "{case} at {time}: an order hold shows no values"
                );
            } else if values {
                assert!(
                    chart.hover_time().is_some(),
                    "{case} at {time}: the hold shows its values"
                );
            }
        }
        assert_eq!(
            chart.get_time_scale_state(),
            view,
            "{case}: the hold does not pan"
        );
        assert!(
            !chart
                .drain_events()
                .any(|event| matches!(event, ChartEvent::Click { .. })),
            "{case}: the hold is no click"
        );
    }
}
