//! Sample capabilities and commands shared by the showcase menu and quick search.

use super::{DemoApp, TopToolbarAction};
use eframe::egui;
use egui_charts::drawings::DrawingToolType;
use egui_charts::studies::IndicatorFactory;
use egui_charts::ui_kit::command_palette::Command;

const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "tape",
        "Time and sales",
        "Live synthetic executions, sizes and aggressor sides.",
    ),
    (
        "markers",
        "Markers and price lines",
        "Series annotations and live price changes.",
    ),
    (
        "signals",
        "Host signals",
        "Synthetic signals, levels and zones supplied by the host.",
    ),
    (
        "pane",
        "Additional symbol pane",
        "Another symbol with its own price scale.",
    ),
    (
        "backtest",
        "Sample backtest",
        "Run a strategy on completed bars and inspect its trades.",
    ),
    (
        "options-volume",
        "Options volume profile",
        "Synthetic call and put volume by strike.",
    ),
    (
        "targets",
        "Analyst price targets",
        "Requires SAMPLE on the daily interval.",
    ),
];

pub(super) fn quick_search_commands() -> Vec<Command> {
    let mut commands: Vec<_> = [
        ("symbols", "Symbol search", "Actions"),
        ("indicators", "Indicators", "Actions"),
        ("settings", "Chart settings", "Settings"),
        ("export", "Export chart", "Actions"),
        ("reset", "Reset chart view", "Actions"),
        ("date", "Go to date", "Actions"),
        ("replay", "Toggle replay", "Actions"),
        ("help", "Keyboard shortcuts", "Actions"),
        ("log", "Toggle logarithmic scale", "Settings"),
        ("invert", "Invert price scale", "Settings"),
        ("grid", "Toggle grid", "Settings"),
        ("samples", "Toggle all sample overlays", "Showcase"),
        ("heatmap", "Market heatmap", "Actions"),
    ]
    .into_iter()
    .map(|(id, label, category)| Command::new(id, label).category(category))
    .collect();
    commands.extend(SAMPLES.iter().map(|(id, label, description)| {
        Command::new(*id, *label)
            .category("Showcase")
            .keywords([*description])
    }));
    for (name, _) in egui_charts::theme::presets() {
        commands.push(
            Command::new(format!("appearance:{name}"), format!("Appearance: {name}"))
                .category("Settings"),
        );
    }
    commands.push(Command::new("appearance:system", "Appearance: system").category("Settings"));
    for category in DrawingToolType::categories() {
        for tool in DrawingToolType::by_category(category) {
            if !tool.is_cursor() {
                commands.push(
                    Command::new(format!("tool:{}", tool.data_name()), tool.as_str())
                        .category("Drawing tools")
                        .keywords([*category]),
                );
            }
        }
    }
    let factory = IndicatorFactory::new();
    let mut names = factory.list();
    names.sort();
    commands.extend(names.into_iter().map(|name| {
        Command::new(format!("study:{name}"), format!("Add {name}"))
            .category("Studies")
            .keywords([factory.category(&name).unwrap_or_default()])
    }));
    commands
}

impl DemoApp {
    pub(super) fn show_showcase(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("demo_showcase")
            .resizable(false)
            .default_size(28.0)
            .min_size(28.0)
            .frame(
                super::panel_frame(self.theme.ui.panel_bg)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    self.show_readout_controls(ui);
                    self.show_appearance_controls(ui);
                    self.show_formatting_controls(ui);
                    ui.menu_button("Showcase", |ui| {
                        ui.set_max_width(360.0);
                        ui.label("Explore the demo's synthetic data");
                        ui.separator();
                        for &(id, label, description) in SAMPLES {
                            if ui.button(label).clicked() {
                                self.execute_search_command(ui.ctx(), id);
                                ui.close();
                            }
                            ui.small(description);
                        }
                    });
                });
            });
    }

    fn enable_samples(&mut self) {
        if !self.api_samples {
            self.api_samples = true;
            self.widget_bar.widgets.extend(super::sample_widgets());
            self.annotate();
        }
    }

    pub(super) fn execute_search_command(&mut self, ctx: &egui::Context, id: &str) {
        self.user_action(|app| app.execute_search_edit(ctx, id));
    }

    fn execute_search_edit(&mut self, ctx: &egui::Context, id: &str) {
        if let Some(name) = id.strip_prefix("appearance:") {
            if name == "system"
                || egui_charts::theme::presets()
                    .iter()
                    .any(|(preset, _)| *preset == name)
            {
                self.theme_choice = (name != "system").then(|| name.to_owned());
                self.update_theme(ctx);
            }
            return;
        }
        if let Some(name) = id.strip_prefix("tool:") {
            if let Some(tool) = DrawingToolType::all()
                .iter()
                .find(|tool| tool.data_name() == name)
            {
                self.chart.drawings_mut().set_active_tool(Some(*tool));
            }
            return;
        }
        if let Some(name) = id.strip_prefix("study:") {
            if let Some(study) = IndicatorFactory::new().create(name) {
                let inputs = super::study_input_summary(study.as_ref());
                let id = self.chart.add_indicator(study);
                self.chart.set_indicator_inputs(id, inputs);
            }
            return;
        }
        let action = match id {
            "symbols" => TopToolbarAction::OpenSymbolSearch,
            "indicators" => TopToolbarAction::OpenIndicators,
            "settings" => TopToolbarAction::OpenSettings,
            "export" => {
                self.screenshot_requested = None;
                self.export_dialog.open();
                return;
            }
            "backtest" => {
                if self.chart.state().shown().len() < 22 {
                    self.notify("Load at least 22 bars before running the sample backtest");
                    return;
                }
                self.run_sample_backtest();
                return;
            }
            "replay" => TopToolbarAction::ToggleReplay,
            "help" => {
                self.keyboard_shortcuts.open();
                return;
            }
            "date" => {
                self.date_picker.open();
                return;
            }
            "reset" => {
                self.handle_chart_context(super::ContextMenuAction::ResetChart);
                return;
            }
            "log" => {
                let scale = self.chart.price_scale_mut();
                let mode = &mut scale.options_mut().mode;
                *mode = if *mode == egui_charts::scales::PriceScaleMode::Logarithmic {
                    egui_charts::scales::PriceScaleMode::Normal
                } else {
                    egui_charts::scales::PriceScaleMode::Logarithmic
                };
                return;
            }
            "invert" => {
                let options = self.chart.price_scale_mut().options_mut();
                options.invert_scale = !options.invert_scale;
                return;
            }
            "grid" => {
                let config = self.chart.config_mut();
                config.show_grid = !config.show_grid;
                return;
            }
            "heatmap" => {
                self.open_panel = Some(super::SidePanel::MarketHeatmap);
                return;
            }
            "options-volume" => {
                let expiry = chrono::Utc::now().date_naive() + chrono::TimeDelta::days(30);
                let query = egui_charts::data::OptionsVolumeQuery {
                    symbol: self.chart.symbol().into(),
                    expiries: expiry..=expiry,
                };
                let price = self
                    .chart
                    .state()
                    .shown()
                    .last()
                    .map_or(100.0, |bar| bar.close);
                let row_size = 10_f64.powf((price.abs().max(1.0) * 0.002).log10().floor());
                let repaint = ctx.clone();
                let viewport = ctx.viewport_id();
                if let Some(profile) = egui_charts::studies::OptionsVolumeProfile::new(
                    self.live.as_ref(),
                    query,
                    row_size,
                    move || repaint.request_repaint_of(viewport),
                ) {
                    self.chart.add_indicator(Box::new(profile));
                }
                return;
            }
            "targets" => {
                if self.chart.symbol() != "SAMPLE" {
                    self.notify("Analyst targets require SAMPLE. Select SAMPLE, then try again.");
                    return;
                }
                self.switch_timeframe(egui_charts::model::Timeframe::Day1);
                self.chart.set_price_targets_visible(true);
                if let Some(last) = self.chart.state().shown().last() {
                    let horizon = last.time + chrono::TimeDelta::days(365);
                    if let Some(index) = self.chart.time_to_index(horizon) {
                        self.chart.set_visible_logical_range(
                            egui_charts::model::LogicalRange::new(-10.0, index as f32 + 120.0),
                        );
                    }
                }
                return;
            }
            "tape" | "markers" | "signals" => {
                self.enable_samples();
                if id == "markers" {
                    self.annotation_editor = true;
                }
                if id == "tape" {
                    self.open_panel = Some(super::SidePanel::TimeAndSales);
                }
                return;
            }
            "samples" => {
                self.api_samples = !self.api_samples;
                if self.api_samples {
                    self.widget_bar.widgets.extend(super::sample_widgets());
                } else {
                    self.widget_bar.widgets.retain(|widget| widget.id == 5);
                    self.widget_bar.active_widget = None;
                    if self.open_panel == Some(super::SidePanel::TimeAndSales) {
                        self.open_panel = None;
                    }
                }
                self.annotate();
                return;
            }
            "pane" => {
                if self.comparisons.len() >= super::DemoSymbol::catalog().len() - 1 {
                    self.notify(
                        "All demo symbols are already present. Remove one before adding a pane.",
                    );
                    return;
                }
                self.add_series_pane();
                return;
            }
            _ => return,
        };
        self.handle_top_toolbar(ctx, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut DemoApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        use eframe::App;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
        )
    }

    fn click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let output = frame(app, ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing control: {label}"));
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .drop_without_applying_deltas();
        }
    }

    #[derive(Default)]
    struct Storage(std::collections::HashMap<String, String>);
    impl eframe::Storage for Storage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn formatting_samples_survive_restart_and_reach_readouts_and_export() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        click(&mut app, &ctx, "Formatting");
        for label in [
            "Deutsch",
            "Price: 123.456 u",
            "Percentage: +12.34 pct",
            "Volume: 123456 units",
        ] {
            click(&mut app, &ctx, label);
        }
        let mut storage = Storage::default();
        app.save(&mut storage);
        egui::Popup::close_all(&ctx);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        app.live.set_paused(true);
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        let time = "2024-03-13T14:30:00Z".parse().unwrap();
        assert!(app.chart.format_time(time).contains("Mär"));
        let info = app.build_data_window();
        let output = ctx.run_ui(Default::default(), |ui| {
            egui_charts::ui::widget_bar::panels::object_tree::show_data_window(ui, &info, true);
        });
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        for suffix in [" u", " units", " pct)"] {
            assert!(
                texts.iter().any(|text| text.ends_with(suffix)),
                "{suffix}: {texts:?}"
            );
        }
        output.drop_without_applying_deltas();
        let svg = app.chart.export_svg(&ctx, &Default::default()).unwrap();
        assert!(svg.contains(" u"));
        assert!(svg.contains(" units"));
        click(&mut app, &ctx, "Formatting");
        click(&mut app, &ctx, "Restore default formatting");
        assert!(app.chart.format_time(time).contains("Mar"));
    }

    #[test]
    fn annotation_editor_owns_keyboard_input_and_preserves_other_annotations() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.execute_search_command(&ctx, "markers");
        assert!(app.keyboard_dialog_open());
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        let before =
            serde_json::to_string(app.chart.markers(egui_charts::widget::SeriesId::MAIN)).unwrap();
        click(&mut app, &ctx, "Show marker");
        assert!(
            app.chart
                .markers(egui_charts::widget::SeriesId::MAIN)
                .iter()
                .any(|marker| marker.id.as_deref() == Some("demo-editor"))
        );
        click(&mut app, &ctx, "Show price line");
        click(&mut app, &ctx, "Show marker");
        assert_eq!(
            serde_json::to_string(app.chart.markers(egui_charts::widget::SeriesId::MAIN)).unwrap(),
            before
        );
        let annotations = app.chart.snapshot().unwrap().main_annotations.unwrap();
        assert!(
            annotations
                .price_lines
                .iter()
                .any(|line| line.title.as_deref() == Some("Demo level"))
        );
        assert!(
            annotations
                .price_lines
                .iter()
                .any(|line| line.title.as_deref() == Some("Sample line"))
        );
    }

    #[test]
    fn showcase_routes_enable_samples_once_and_check_target_prerequisite() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        for id in ["tape", "signals", "markers", "tape"] {
            app.execute_search_command(&ctx, id);
            assert!(app.api_samples, "{id}");
            assert_eq!(
                app.widget_bar
                    .widgets
                    .iter()
                    .filter(|widget| widget.id == 4)
                    .count(),
                1
            );
        }
        assert!(app.open_panel == Some(super::super::SidePanel::TimeAndSales));
        app.switch_symbol("BTCUSD");
        let before = app.state.timeframe;
        app.execute_search_command(&ctx, "targets");
        assert_eq!(app.state.timeframe, before);
        assert!(!app.chart.price_targets_visible());
    }
}
