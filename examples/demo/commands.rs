//! Quick-search commands, sample scenes and image layout composition for the demo.

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
        let mut search = false;
        egui::CentralPanel::default().show(ui, |ui| {
            let chart = match scene {
                SampleScene::Numeric(chart) => chart,
                SampleScene::Extensions(scene) => {
                    scene.controls(ui);
                    &mut scene.chart
                }
            };
            chart.set_theme_override(Some(self.theme.clone()));
            chart.show(ui);
            search = chart.take_shortcut_requests().iter().any(|request| {
                matches!(
                    request,
                    egui_charts::widget::ShortcutRequest::Action(
                        egui_charts::config::KeyboardAction::OpenQuickSearch
                    )
                )
            });
        });
        if search {
            self.quick_search.open();
        }
        if let Some(id) = self.quick_search.show(ui.ctx()) {
            self.sample_scene = None;
            self.execute_search_command(ui.ctx(), &id);
        }
        true
    }
}

impl DemoApp {
    pub(super) fn export_image_bytes(
        &self,
        ctx: &egui::Context,
        options: &egui_charts::widget::ImageExportOptions,
        format: super::ExportFormat,
        arrangement: bool,
    ) -> Result<Vec<u8>, egui_charts::widget::ExportError> {
        if !arrangement {
            return if format == super::ExportFormat::Svg {
                self.trading
                    .export_svg(&self.chart, ctx, options)
                    .map(String::into_bytes)
            } else {
                self.trading.export_png(&self.chart, ctx, options)
            };
        }
        let mut extra: Vec<_> = self
            .extra_charts
            .iter()
            .map(|slot| slot.chart.export_snapshot(ctx, options))
            .collect();
        let draw = |ui: &mut egui::Ui, active: &mut egui_charts::Chart| {
            let mut charts: Vec<_> = extra.iter_mut().collect();
            charts.insert(self.layout_slot, active);
            for (slot, rect) in self
                .layout
                .visible_chart_rects(ui.available_rect_before_wrap(), charts.len())
            {
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
                    self.trading.draw_export(ui, charts[slot]);
                });
            }
        };
        if format == super::ExportFormat::Svg {
            self.chart
                .export_layout_svg(ctx, options, draw)
                .map(String::into_bytes)
        } else {
            self.chart.export_layout_png(ctx, options, draw)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, RawInput};
    use egui_charts::widget::{Chart, ChartLayoutMode, ImageExportOptions};

    fn choose_scope(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let mut point = None;
        for _ in 0..3 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show_dialogs(ui.ctx()),
            );
            point = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
        }
        let point = point.expect("image scope must be selectable in the export dialog");
        ctx.run_ui(
            RawInput {
                events: vec![
                    Event::PointerMoved(point),
                    Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| app.show_dialogs(ui.ctx()),
        )
        .drop_without_applying_deltas();
    }

    #[test]
    fn arrangement_exports_keep_slot_order_backgrounds_and_the_active_chart_scope() {
        for (mode, count) in [
            (ChartLayoutMode::TwoHorizontal, 2),
            (ChartLayoutMode::Four, 4),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            app.ensure_chart_slots(mode);
            app.activate_chart_slot(count - 1);
            let colors = [[30, 50, 70], [60, 80, 100], [90, 110, 130], [120, 140, 160]];
            {
                let mut charts: Vec<_> = app
                    .extra_charts
                    .iter_mut()
                    .map(|slot| &mut slot.chart)
                    .collect();
                charts.insert(app.layout_slot, &mut app.chart);
                for (slot, chart) in charts.into_iter().enumerate().take(count) {
                    *chart = Chart::new(Default::default());
                    chart.set_symbol(format!("SLOT{}", slot + 1));
                    let mut theme = app.theme.clone();
                    let [r, g, b] = colors[slot];
                    theme.chart.bg = egui::Color32::from_rgb(r, g, b);
                    chart.set_theme_override(Some(theme));
                    chart.config_mut().show_volume = false;
                    chart.config_mut().show_grid = false;
                }
            }
            app.execute_search_command(&ctx, "export");
            choose_scope(&mut app, &ctx, "Visible arrangement");
            let before = app.chart.get_time_scale_state();
            for transparent in [false, true] {
                let mut options = ImageExportOptions::default();
                options.size = [640, 400];
                options.include_timestamp = false;
                options.transparent_background = transparent;
                let svg_path = std::env::temp_dir()
                    .join(format!("demo-arrangement-{count}-{transparent}.svg"));
                app.export_svg(&ctx, &options, Some(svg_path.clone()));
                let svg = std::fs::read_to_string(&svg_path).unwrap();
                for slot in 1..=count {
                    assert!(svg.contains(&format!("SLOT{slot}")), "missing slot {slot}");
                }
                for chrome in ["Export chart", "Image scope", "Samples"] {
                    assert!(!svg.contains(chrome));
                }
                let png_path = std::env::temp_dir()
                    .join(format!("demo-arrangement-{count}-{transparent}.png"));
                app.run_export(
                    &ctx,
                    egui_charts::ui::export::ExportConfig {
                        format: super::super::ExportFormat::Png,
                        image: options,
                    },
                    Some(png_path.clone()),
                );
                app.handle_screenshot(&ctx);
                let image = image::open(&png_path).unwrap().into_rgba8();
                assert_eq!(image.dimensions(), (640, 400));
                for (slot, color) in colors.iter().enumerate().take(count) {
                    let (column, row) = (slot % 2, slot / 2);
                    let pixel = image
                        .get_pixel(5 + column as u32 * 320, 150 + row as u32 * 200)
                        .0;
                    if transparent {
                        assert_eq!(pixel[3], 0);
                    } else {
                        assert_eq!(&pixel[..3], color);
                    }
                }
                std::fs::remove_file(svg_path).unwrap();
                std::fs::remove_file(png_path).unwrap();
            }
            assert_eq!(app.chart.get_time_scale_state(), before);
            choose_scope(&mut app, &ctx, "Active chart");
            let path = std::env::temp_dir().join(format!("demo-active-{count}.svg"));
            app.export_svg(&ctx, &ImageExportOptions::default(), Some(path.clone()));
            let svg = std::fs::read_to_string(&path).unwrap();
            assert!(svg.contains(&format!("SLOT{count}")));
            for slot in 1..count {
                assert!(!svg.contains(&format!("SLOT{slot}")));
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}
