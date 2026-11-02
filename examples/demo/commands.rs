//! Commands exposed by the demo's quick search.

use super::{DemoApp, TopToolbarAction};
use eframe::egui;
use egui_charts::drawings::DrawingToolType;
use egui_charts::studies::IndicatorFactory;
use egui_charts::ui_kit::command_palette::Command;

pub(super) fn quick_search_commands() -> Vec<Command> {
    let mut commands: Vec<_> = [
        ("symbols", "Symbol search", "Actions"),
        ("indicators", "Indicators", "Actions"),
        ("settings", "Chart settings", "Settings"),
        ("export", "Export chart", "Actions"),
        ("backtest", "Sample backtest", "Actions"),
        ("reset", "Reset chart view", "Actions"),
        ("date", "Go to date", "Actions"),
        ("replay", "Toggle replay", "Actions"),
        ("help", "Keyboard shortcuts", "Actions"),
        ("log", "Toggle logarithmic scale", "Settings"),
        ("invert", "Invert price scale", "Settings"),
        ("grid", "Toggle grid", "Settings"),
        ("samples", "Toggle API samples", "Settings"),
        ("targets", "Show analyst price targets", "Actions"),
        ("options-volume", "Options volume profile", "Actions"),
        ("heatmap", "Market heatmap", "Actions"),
        ("pane", "Add a symbol in a new pane", "Actions"),
        ("smile", "Volatility smile", "Samples"),
        ("yield", "Yield curve", "Samples"),
        ("primitives", "Primitives and custom series", "Samples"),
        ("price-workspace", "Return to price chart", "Samples"),
        ("sample-study", "Sample study editor", "Samples"),
    ]
    .into_iter()
    .map(|(id, label, category)| Command::new(id, label).category(category))
    .collect();
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
    pub(super) fn execute_search_command(&mut self, ctx: &egui::Context, id: &str) {
        self.user_action(|app| app.execute_search_edit(ctx, id));
    }

    fn execute_search_edit(&mut self, ctx: &egui::Context, id: &str) {
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
        if let Some(scene) = match id {
            "smile" => Some(SampleScene::Numeric(
                super::numeric_scenes::volatility_smile(),
            )),
            "yield" => Some(SampleScene::Numeric(super::numeric_scenes::yield_curve())),
            "primitives" => Some(SampleScene::Extensions(
                super::primitive_scene::PrimitiveScene::new(),
            )),
            _ => None,
        } {
            self.sample_scene = Some(scene);
            return;
        }
        if id == "sample-study" {
            self.sample_scene = None;
            self.sample_study.open = true;
            return;
        }
        if id == "price-workspace" {
            self.sample_scene = None;
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
                self.add_series_pane();
                return;
            }
            _ => return,
        };
        self.handle_top_toolbar(ctx, action);
    }
}

pub(super) enum SampleScene {
    Numeric(egui_charts::Chart),
    Extensions(super::primitive_scene::PrimitiveScene),
}

impl DemoApp {
    pub(super) fn show_samples(&mut self, ui: &mut egui::Ui) -> bool {
        let mut command = None;
        egui::Panel::top("sample_chooser").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("Samples", |ui| {
                    for (id, label) in [
                        ("smile", "Volatility smile"),
                        ("yield", "Yield curve"),
                        ("primitives", "Primitives and custom series"),
                        ("sample-study", "Sample study editor"),
                    ] {
                        if ui.button(label).clicked() {
                            command = Some(id);
                            ui.close();
                        }
                    }
                });
                if self.sample_scene.is_some() {
                    if ui.button("Return to price chart").clicked() {
                        command = Some("price-workspace");
                    }
                    ui.label("Synthetic sample data. Pan, zoom and hover to inspect values.");
                }
            });
        });
        if let Some(command) = command {
            self.execute_search_edit(ui.ctx(), command);
        }
        let Some(scene) = &mut self.sample_scene else {
            return false;
        };
        egui::CentralPanel::default().show(ui, |ui| {
            let chart = match scene {
                SampleScene::Numeric(chart) => chart,
                SampleScene::Extensions(scene) => {
                    scene.controls(ui);
                    &mut scene.chart
                }
            };
            chart.set_theme(ui.ctx(), &self.theme);
            chart.show(ui);
        });
        true
    }
}
