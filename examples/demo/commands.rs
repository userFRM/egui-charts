//! Demo menu controls and sample commands shared with quick search.

use super::{DemoApp, TopToolbarAction};
use eframe::egui;
use egui_charts::drawings::DrawingToolType;
use egui_charts::icons::{Icon, icons};
use egui_charts::studies::IndicatorFactory;
use egui_charts::theme::ThemeExt;
use egui_charts::ui_kit::command_palette::Command;
use egui_charts::ui_kit::list_item::ListItem;

/// A top-left menu row with an icon and an optional trailing icon at the right edge.
pub(super) fn menu_item(
    ui: &mut egui::Ui,
    label: &str,
    icon: &Icon,
    trailing: Option<&Icon>,
) -> egui::Response {
    let response = ListItem::new().show_with_icon(ui, label, icon);
    if let Some(trailing) = trailing {
        let rect = egui::Rect::from_center_size(
            response.rect.right_center() - egui::vec2(12.0, 0.0),
            egui::Vec2::splat(12.0),
        );
        trailing
            .as_image_tinted(rect.size(), ui.active_theme().ui.text)
            .paint_at(ui, rect);
    }
    response
}

/// A menu row that flips `checked` on click and shows a check mark at the right while on.
pub(super) fn menu_toggle(ui: &mut egui::Ui, label: &str, icon: &Icon, checked: &mut bool) {
    let response = menu_item(ui, label, icon, checked.then_some(&icons::CHECK));
    if response.clicked() {
        *checked = !*checked;
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, response.enabled(), *checked, label)
    });
}

const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "smile",
        "Volatility smile",
        "Synthetic implied volatility by strike with numeric readouts.",
    ),
    (
        "yield",
        "Yield curve",
        "Synthetic yields by maturity with numeric readouts.",
    ),
    (
        "primitives",
        "Primitives and custom series",
        "Toggle session backgrounds, axis labels and a custom range pane.",
    ),
    (
        "sample-study",
        "Sample study editor",
        "Apply a close-difference study and save named source documents locally.",
    ),
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
    (
        "financials",
        "Company financials",
        "Synthetic income statement, balance sheet and statistics for AAPL.",
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
        ("price-workspace", "Return to price chart", "Showcase"),
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
            Command::new(
                format!("appearance:{name}"),
                format!("Appearance: {}", super::formatting::appearance_label(name)),
            )
            .category("Settings"),
        );
    }
    commands.push(Command::new("appearance:system", "Appearance: System").category("Settings"));
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
    pub(super) fn show_demo_controls(&mut self, ui: &mut egui::Ui) -> Option<&'static str> {
        let mut command = None;
        for (label, icon) in [
            ("Price unit", &icons::PRICE_LABEL),
            ("Market recording", &icons::REPLAY),
            ("Readout", &icons::EYE),
            ("Appearance", &icons::BRUSH),
            ("Formatting", &icons::TEXT),
            ("Showcase", &icons::WIDGET_BAR_HIGHLIGHTS),
        ] {
            // Both act on the synthetic feed.
            if self.own_data.is_some() && matches!(label, "Price unit" | "Market recording") {
                continue;
            }
            let response = menu_item(ui, label, icon, Some(&icons::CHEVRON_RIGHT));
            egui::containers::menu::SubMenu::new()
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .show(ui, &response, |ui| match label {
                    "Readout" => self.show_readout_controls(ui),
                    "Appearance" => self.show_appearance_controls(ui),
                    "Formatting" => self.show_formatting_controls(ui),
                    "Price unit" => self.show_price_unit_controls(ui),
                    "Market recording" => self.show_recording_controls(ui),
                    _ => command = sample_menu(ui),
                });
        }
        command
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
        // These switch to a catalog symbol or show the sample annotations.
        if matches!(
            id,
            "options-volume"
                | "targets"
                | "financials"
                | "tape"
                | "markers"
                | "signals"
                | "samples"
        ) && self.synthetic_only("Samples")
        {
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
            "financials" => {
                self.switch_symbol("AAPL");
                self.open_financials();
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
                    self.widget_bar.widgets.clear();
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

/// Sample destinations shared by the application menu and open scenes.
fn sample_menu(ui: &mut egui::Ui) -> Option<&'static str> {
    let mut command = None;
    ui.set_max_width(360.0);
    ui.label("Explore the demo's synthetic data");
    ui.separator();
    for &(id, label, description) in SAMPLES {
        if ui.button(label).clicked() {
            command = Some(id);
            ui.close();
        }
        ui.small(description);
    }
    command
}

impl DemoApp {
    /// Shows the open sample in place of the price workspace. Returns `false` when none is open.
    pub(super) fn show_samples(&mut self, ui: &mut egui::Ui) -> bool {
        if self.sample_scene.is_none() {
            return false;
        }
        let mut command = None;
        egui::Panel::top("sample_chooser").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                command = ui.menu_button("Samples", sample_menu).inner.flatten();
                if ui.button("Return to price chart").clicked() {
                    command = Some("price-workspace");
                }
                ui.label("Synthetic sample data. Pan, zoom and hover to inspect values.");
            });
        });
        if let Some(command) = command {
            self.sample_scene = None;
            self.execute_search_command(ui.ctx(), command);
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
            if self.quick_search.is_open() {
                chart.suppress_shortcuts(ui.ctx());
            }
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
        let mut snapshots = std::collections::BTreeMap::new();
        let draw = |ui: &mut egui::Ui, active: &mut egui_charts::Chart| {
            for (slot, rect) in self
                .layout
                .visible_chart_rects(ui.available_rect_before_wrap(), self.extra_charts.len() + 1)
            {
                let chart = if slot == self.layout_slot {
                    &mut *active
                } else {
                    let index = slot - usize::from(slot > self.layout_slot);
                    snapshots.entry(slot).or_insert_with(|| {
                        self.extra_charts[index].chart.export_snapshot(ctx, options)
                    })
                };
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
                    self.trading.draw_export(ui, chart);
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
        for (mode, count, active) in [
            (ChartLayoutMode::TwoHorizontal, 2),
            (ChartLayoutMode::Four, 4),
        ]
        .into_iter()
        .flat_map(|(mode, count)| [0, count - 1].map(|active| (mode, count, active)))
        {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            app.ensure_chart_slots(mode);
            app.activate_chart_slot(active);
            let colors = [[30, 50, 70], [60, 80, 100], [90, 110, 130], [120, 140, 160]];
            {
                let mut charts: Vec<_> = app
                    .extra_charts
                    .iter_mut()
                    .map(|slot| &mut slot.chart)
                    .collect();
                charts.insert(app.layout_slot, &mut app.chart);
                for (slot, chart) in charts.into_iter().enumerate().take(count) {
                    let bars = (0..3)
                        .map(|i| {
                            egui_charts::model::Bar::from_value(
                                chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i),
                                100.0 + i as f64,
                            )
                        })
                        .collect();
                    *chart = Chart::new(egui_charts::model::BarData::from_bars(bars));
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
            assert!(svg.contains(&format!("SLOT{}", active + 1)));
            for slot in (1..=count).filter(|slot| *slot != active + 1) {
                assert!(!svg.contains(&format!("SLOT{slot}")));
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    fn frame(app: &mut DemoApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        frame_at_width(app, ctx, events, 1440.0)
    }

    fn frame_at_width(
        app: &mut DemoApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        width: f32,
    ) -> egui::FullOutput {
        use eframe::App;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
        )
    }

    fn click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        click_at_width(app, ctx, label, 1440.0);
    }

    fn click_at_width(app: &mut DemoApp, ctx: &egui::Context, label: &str, width: f32) {
        ctx.enable_accesskit();
        let output = frame_at_width(app, ctx, vec![], width);
        let pos = output
            .platform_output
            .accesskit_update
            .as_ref()
            .and_then(|update| {
                update.nodes.iter().find_map(|(_, node)| {
                    if node.label() == Some(label) && node.role() != egui::accesskit::Role::Label {
                        let rect = node.bounds()?;
                        Some(egui::pos2(
                            (rect.x0 + rect.x1) as f32 * 0.5,
                            (rect.y0 + rect.y1) as f32 * 0.5,
                        ))
                    } else {
                        None
                    }
                })
            });
        output.drop_without_applying_deltas();
        let pos = pos.unwrap_or_else(|| panic!("missing control: {label}"));
        for pressed in [true, false] {
            frame_at_width(
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
                width,
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn menu_icons_are_distinct_labels_align_and_toggles_keep_their_state_and_position() {
        for width in [393.0, 1440.0] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            app.enable_samples();
            frame_at_width(&mut app, &ctx, vec![], width).drop_without_applying_deltas();
            click_at_width(&mut app, &ctx, "Menu", width);
            let label_pos = |output: &egui::FullOutput, label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing menu label: {label}"))
            };
            let mut output = frame_at_width(&mut app, &ctx, vec![], width);
            output.textures_delta.clear();
            let origin = label_pos(&output, "Readout").x;
            let pause_pos = label_pos(&output, "Pause synthetic feed");
            let mut menu_icons = std::collections::HashMap::new();
            for label in [
                "Quick Search",
                "Toggle Theme",
                "Price unit",
                "Market recording",
                "Readout",
                "Appearance",
                "Formatting",
                "Showcase",
                "Liquidity heatmap",
                "Largest order profile",
                "Pause synthetic feed",
                "Data source",
                "Order flow",
                "Research",
                "Copy symbol",
            ]
            .into_iter()
            .chain((width < 600.0).then_some("Panels"))
            {
                let pos = label_pos(&output, label);
                assert_eq!(pos.x, origin, "{width}: {label}");
                let icon = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        let bounds = shape.shape.visual_bounding_rect();
                        let texture = shape.shape.texture_id();
                        (texture != egui::TextureId::default()
                            && bounds.right() < origin
                            && bounds.y_range().contains(pos.y))
                        .then_some(texture)
                    })
                    .unwrap_or_else(|| panic!("{width}: missing icon for {label}"));
                assert_eq!(
                    menu_icons.insert(icon, label),
                    None,
                    "{width}: {label} shares its icon with another entry"
                );
            }
            output.drop_without_applying_deltas();
            click_at_width(&mut app, &ctx, "Largest order profile", width);
            assert!(
                app.chart.order_book_view().profile
                    != egui_charts::chart::overlays::BookProfile::LargestOrder,
                "disabled profile must not toggle"
            );
            for paused in [false, true] {
                click_at_width(&mut app, &ctx, "Pause synthetic feed", width);
                assert_eq!(app.live.paused(), paused);
                let mut output = frame_at_width(&mut app, &ctx, vec![], width);
                output.textures_delta.clear();
                assert_eq!(label_pos(&output, "Pause synthetic feed"), pause_pos);
                let node = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some("Pause synthetic feed"))
                    .unwrap();
                assert_eq!(node.1.role(), egui::accesskit::Role::CheckBox);
                assert_eq!(
                    node.1.toggled(),
                    Some(if paused {
                        egui::accesskit::Toggled::True
                    } else {
                        egui::accesskit::Toggled::False
                    })
                );
                output.drop_without_applying_deltas();
            }
            click_at_width(&mut app, &ctx, "Liquidity heatmap", width);
            click_at_width(&mut app, &ctx, "Largest order profile", width);
            assert!(
                super::super::ChartSettingsState::from_chart(&app.chart)
                    .order_flow
                    .show_heatmap
            );
            assert_eq!(
                app.chart.order_book_view().profile,
                egui_charts::chart::overlays::BookProfile::LargestOrder
            );
            click_at_width(&mut app, &ctx, "Copy symbol", width);
            assert!(!egui::Popup::is_any_open(&ctx));
            click_at_width(&mut app, &ctx, "Menu", width);
            click_at_width(&mut app, &ctx, "Data source", width);
            assert!(app.data_source.is_open() && app.keyboard_dialog_open());
            click_at_width(&mut app, &ctx, "Cancel", width);
            assert!(!app.data_source.is_open());
            click_at_width(&mut app, &ctx, "Menu", width);
            click_at_width(&mut app, &ctx, "Order flow", width);
            assert!(app.open_panel == Some(super::super::SidePanel::OrderFlow));
            assert!(app.widget_bar.active_widget.is_none());
            assert!(!egui::Popup::is_any_open(&ctx));
            app.execute_search_command(&ctx, "samples");
            assert!(app.open_panel == Some(super::super::SidePanel::OrderFlow));
            click_at_width(&mut app, &ctx, "Menu", width);
            click_at_width(&mut app, &ctx, "Order flow", width);
            assert!(app.open_panel.is_none());
            if width < 600.0 {
                app.enable_samples();
                let panels = super::super::SidePanel::BUILT_IN
                    .into_iter()
                    .map(|(panel, icon)| (icon.tooltip().to_owned(), Some(panel), None))
                    .chain(std::iter::once((
                        "Data Window".into(),
                        Some(super::super::SidePanel::DataWindow),
                        None,
                    )))
                    .chain(app.widget_bar.widgets.iter().map(|widget| {
                        (
                            widget.tooltip.clone(),
                            super::super::SidePanel::from_widget(widget.id),
                            Some(widget.id),
                        )
                    }))
                    .collect::<Vec<_>>();
                for (label, panel, widget) in panels {
                    for opening in [true, false] {
                        click_at_width(&mut app, &ctx, "Menu", width);
                        click_at_width(&mut app, &ctx, "Panels", width);
                        click_at_width(&mut app, &ctx, &label, width);
                        egui::Popup::close_all(&ctx);
                        assert!(app.open_panel == panel.filter(|_| opening), "{label}");
                        if panel.is_none() {
                            assert_eq!(
                                app.widget_bar.active_widget,
                                widget.filter(|_| opening),
                                "{label}"
                            );
                        } else {
                            assert!(app.widget_bar.active_widget.is_none());
                        }
                        // Both transitions retain one active panel, without drawing it twice.
                        for resized in [1440.0, width] {
                            frame_at_width(&mut app, &ctx, vec![], resized)
                                .drop_without_applying_deltas();
                            assert!(
                                app.open_panel == panel.filter(|_| opening),
                                "resize {label}"
                            );
                        }
                    }
                }
                let size = egui::vec2(width, 320.0);
                let draw = |app: &mut DemoApp, events| {
                    super::super::tests::ui_frame(&ctx, size, events, |ui| {
                        app.show_top_toolbar(ui, app.theme.chart.bg);
                    })
                };
                for _ in 0..3 {
                    draw(&mut app, vec![]);
                }
                super::super::tests::ui_click(&ctx, size, egui::pos2(20.0, 20.0), |ui| {
                    app.show_top_toolbar(ui, app.theme.chart.bg);
                });
                let texts = draw(&mut app, vec![]);
                let panel_menu = texts
                    .iter()
                    .find(|(text, _)| text == "Panels")
                    .expect("landscape panel menu")
                    .1;
                assert!(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(panel_menu)
                );
                super::super::tests::ui_click(&ctx, size, panel_menu.center(), |ui| {
                    app.show_top_toolbar(ui, app.theme.chart.bg);
                });
                let texts = draw(&mut app, vec![]);
                let watchlist = texts
                    .iter()
                    .find(|(text, _)| text == "Watchlist and details")
                    .expect("landscape panel action")
                    .1;
                super::super::tests::ui_click(&ctx, size, watchlist.center(), |ui| {
                    app.show_top_toolbar(ui, app.theme.chart.bg);
                });
                assert!(app.open_panel == Some(super::super::SidePanel::Watchlist));
                egui::Popup::close_all(&ctx);
            }
        }
    }

    #[test]
    fn formatting_samples_survive_restart_and_reach_readouts_and_export() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        super::super::tests::settle(&mut app, &ctx);
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Formatting");
        for label in [
            "Deutsch",
            "Price: 123.456 u",
            "Percentage: +12.34 pct",
            "Volume: 123456 units",
        ] {
            click(&mut app, &ctx, label);
        }
        // The restarted application has storage: its first-visit choice is already answered.
        let answered = super::super::owndata::store::Saved {
            asked: true,
            ..Default::default()
        };
        answered.save(&app.storage);
        let mut storage = super::super::tests::AppStorage::default();
        app.save(&mut storage);
        egui::Popup::close_all(&ctx);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        app.live.set_paused(true);
        super::super::tests::settle(&mut app, &ctx);
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
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Formatting");
        click(&mut app, &ctx, "Restore default formatting");
        assert!(app.chart.format_time(time).contains("Mar"));
    }

    #[test]
    fn annotation_editor_owns_keyboard_input_and_preserves_other_annotations() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        super::super::tests::settle(&mut app, &ctx);
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
    fn financials_sample_opens_populated_pages_and_follows_symbol_availability() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        app.execute_search_command(&ctx, "financials");
        assert_eq!(app.chart.symbol(), "AAPL");
        assert_eq!(app.financials.as_ref().unwrap().0, "AAPL");
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        for (page, row) in [
            ("Income statement", "Total revenue"),
            ("Balance sheet", "Total assets"),
            ("Statistics", "Net margin %"),
        ] {
            click(&mut app, &ctx, page);
            let output = frame(&mut app, &ctx, vec![]);
            assert!(
                output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == row)),
                "{page}"
            );
            output.drop_without_applying_deltas();
        }
        click(&mut app, &ctx, "Back to chart");
        assert!(app.financials.is_none());
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        assert_eq!(app.chart.symbol(), "AAPL");
        assert!(!app.chart.state().shown().is_empty());
        for (symbol, available) in [
            ("MSFT", true),
            ("BTCUSD", false),
            ("EURUSD", false),
            ("SPX", false),
            ("SAMPLE", false),
        ] {
            app.execute_search_command(&ctx, "financials");
            app.switch_symbol(symbol);
            frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
            assert_eq!(app.financials.is_some(), available, "{symbol}");
            if let Some((shown, _)) = &app.financials {
                assert_eq!(shown, symbol);
            }
        }
    }

    #[test]
    fn showcase_routes_enable_samples_once_and_check_target_prerequisite() {
        let commands = quick_search_commands();
        let ids: std::collections::HashSet<_> =
            commands.iter().map(|command| &command.id).collect();
        assert_eq!(ids.len(), commands.len(), "commands must have unique ids");
        for &(id, label, _) in SAMPLES {
            assert!(
                commands
                    .iter()
                    .any(|command| command.id == id && command.label == label)
            );
        }
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Readout");
        click(&mut app, &ctx, "Magnifier");
        click(&mut app, &ctx, "Hold to inspect");
        assert_eq!(
            egui_charts::config::TooltipMode::Magnifier,
            app.chart.chart_options().tooltip.mode
        );
        assert!(app.chart.chart_options().tooltip.on_long_press);
        egui::Popup::close_all(&ctx);
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Appearance");
        click(&mut app, &ctx, "High contrast");
        assert_eq!(app.theme_choice.as_deref(), Some("high-contrast"));
        egui::Popup::close_all(&ctx);
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Showcase");
        click(&mut app, &ctx, "Time and sales");
        assert!(app.api_samples);
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
