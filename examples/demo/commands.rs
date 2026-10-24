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
        ("reset", "Reset chart view", "Actions"),
        ("date", "Go to date", "Actions"),
        ("replay", "Toggle replay", "Actions"),
        ("help", "Keyboard shortcuts", "Actions"),
        ("log", "Toggle logarithmic scale", "Settings"),
        ("invert", "Invert price scale", "Settings"),
        ("grid", "Toggle grid", "Settings"),
        ("pane", "Add a symbol in a new pane", "Actions"),
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
                let inputs = self.indicator_dialog.input_summary(name);
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
                self.export_generation += 1;
                self.export_dialog.open();
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
            "pane" => {
                self.add_series_pane();
                return;
            }
            _ => return,
        };
        self.handle_top_toolbar(ctx, action);
    }
}
