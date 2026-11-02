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
        egui::TopBottomPanel::top("demo_showcase").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                self.show_readout_controls(ui);
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
