//! Demo menu controls and sample commands shared with quick search.

use super::{
    DemoApp, DrawingToolbarAction, RightBarIcon, TimeframeToolbarAction, TopToolbarAction,
};
use eframe::egui;
use egui_charts::config::KeyboardAction;
use egui_charts::drawings::DrawingToolType;
use egui_charts::icons::{Icon, icons};
use egui_charts::studies::IndicatorFactory;
use egui_charts::theme::ThemeExt;
use egui_charts::ui_kit::command_palette::{Command, CommandPaletteHandler};
use egui_charts::ui_kit::list_item::ListItem;
use egui_charts::widget::ShortcutMap;

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
        "Synthetic high, average and low targets for SAMPLE on the daily interval.",
    ),
    (
        "financials",
        "Company financials",
        "Synthetic income statement, balance sheet and statistics for STOCKA.",
    ),
];

/// The quick search rows whose action has a key binding.
const ROW_KEYS: &[(&str, KeyboardAction)] = &[
    ("symbols", KeyboardAction::OpenSymbolSearch),
    ("indicators", KeyboardAction::OpenIndicatorSearch),
    ("reset", KeyboardAction::ResetChart),
    ("date", KeyboardAction::GoToDate),
    ("help", KeyboardAction::ShowKeyboardHelp),
    ("log", KeyboardAction::ToggleLogScale),
    ("invert", KeyboardAction::InvertScale),
    ("grid", KeyboardAction::ToggleGrid),
    ("undo", KeyboardAction::Undo),
    ("redo", KeyboardAction::Redo),
    ("snapshot", KeyboardAction::Snapshot),
    ("alert", KeyboardAction::CreateAlert),
    ("interval", KeyboardAction::OpenIntervalSearch),
    ("percent", KeyboardAction::TogglePercentageScale),
    ("auto-scale", KeyboardAction::ToggleAutoScale),
    ("data-window", KeyboardAction::OpenDataWindow),
    ("fullscreen", KeyboardAction::MaximizeChart),
    ("save-layout", KeyboardAction::SaveLayout),
    ("open-layout", KeyboardAction::OpenLayout),
    ("hide-drawings", KeyboardAction::ToggleDrawings),
];

/// The quick search rows. A row whose action has a key shows the first one bound in `shortcuts`,
/// and each saved indicator template of `templates` (id and name) has a row of its own,
/// as does each study `factory` lists. Each drawing tool row has a star, filled for the tools
/// in `favorites`.
pub(super) fn quick_search_commands(
    shortcuts: &ShortcutMap,
    templates: &[(String, String)],
    factory: &IndicatorFactory,
    favorites: &[DrawingToolType],
) -> Vec<Command> {
    let keyed = |command: Command, action: Option<KeyboardAction>| match action
        .and_then(|action| shortcuts.shortcut(action))
    {
        Some(shortcut) => command.shortcut(shortcut),
        None => command,
    };
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
        ("undo", "Undo", "Actions"),
        ("redo", "Redo", "Actions"),
        ("snapshot", "Take a snapshot", "Actions"),
        ("alert", "Create alert", "Actions"),
        ("compare", "Compare or add symbol", "Actions"),
        ("interval", "Change interval", "Actions"),
        ("timezone", "Time zone", "Settings"),
        ("session", "Trading session", "Settings"),
        ("percent", "Toggle percent scale", "Settings"),
        ("auto-scale", "Toggle auto scale", "Settings"),
        ("data-window", "Data window", "Actions"),
        ("object-tree", "Object tree", "Actions"),
        ("watchlist", "Watchlist", "Actions"),
        ("fullscreen", "Toggle fullscreen", "Actions"),
        ("save-layout", "Save layout", "Actions"),
        ("open-layout", "Open layout", "Actions"),
        ("hide-drawings", "Hide all drawings", "Drawing tools"),
        ("remove-drawings", "Remove all drawings", "Drawing tools"),
        ("lock-drawings", "Lock all drawings", "Drawing tools"),
        ("magnet", "Toggle magnet", "Drawing tools"),
        ("remove-indicators", "Remove all indicators", "Studies"),
        ("save-template", "Save indicator template", "Studies"),
    ]
    .into_iter()
    .map(|(id, label, category)| {
        let action = ROW_KEYS
            .iter()
            .find(|(row, _)| *row == id)
            .map(|(_, key)| *key);
        keyed(Command::new(id, label).category(category), action)
    })
    .collect();
    commands.extend(templates.iter().map(|(id, name)| {
        Command::new(
            format!("template:{id}"),
            format!("Indicator template: {name}"),
        )
        .category("Studies")
    }));
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
                commands.push(keyed(
                    Command::new(format!("tool:{}", tool.data_name()), tool.as_str())
                        .category("Drawing tools")
                        .keywords([*category])
                        .starred(favorites.contains(&tool)),
                    Some(KeyboardAction::DrawingTool(tool)),
                ));
            }
        }
    }
    let mut names = factory.list();
    names.sort();
    commands.extend(names.into_iter().map(|name| {
        Command::new(format!("study:{name}"), format!("Add {name}"))
            .category("Studies")
            .keywords([factory.category(&name).unwrap_or_default()])
    }));
    commands
}

/// The drawing tool whose quick search id is `tool:<name>`.
fn search_tool(id: &str) -> Option<DrawingToolType> {
    let name = id.strip_prefix("tool:")?;
    DrawingToolType::all()
        .iter()
        .copied()
        .find(|tool| tool.data_name() == name)
}

impl DemoApp {
    pub(super) fn show_demo_controls(&mut self, ui: &mut egui::Ui) -> Option<&'static str> {
        let mut command = None;
        for (label, icon) in [
            ("Price unit", &icons::PRICE_LABEL),
            ("Market recording", &icons::REPLAY),
            ("Readout", &icons::EYE),
            ("Scrolling", &icons::UI_DRAG_HANDLE),
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
                    "Scrolling" => self.show_scrolling_controls(ui),
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

    /// Opens quick search with the chart's current key bindings and saved templates.
    pub(super) fn open_quick_search(&mut self) {
        let recent = self.quick_search.recent().to_vec();
        self.quick_search = CommandPaletteHandler::new(quick_search_commands(
            self.chart.shortcuts(),
            &self.top_toolbar.indicator_templates,
            self.indicator_dialog.factory(),
            &self.drawing_toolbar.state.favorites,
        ));
        self.quick_search.set_recent(recent);
        self.quick_search.open();
    }

    /// Shows quick search and applies its star presses to the drawing favourites.
    pub(super) fn show_quick_search(&mut self, ctx: &egui::Context) -> Option<String> {
        let id = self.quick_search.show(ctx);
        for star in self.quick_search.take_star_toggles() {
            if let Some(tool) = search_tool(&star) {
                self.drawing_toolbar.toggle_favorite(tool);
            }
        }
        id
    }

    pub(super) fn execute_search_command(&mut self, ctx: &egui::Context, id: &str) {
        // The appearance is the interface's, on every chart: choosing it is no chart's edit.
        if id.starts_with("appearance:") {
            return self.execute_search_edit(ctx, id);
        }
        // Undo and redo step through the history, so they cannot run inside an edit.
        match id {
            "undo" => self.handle_top_toolbar(ctx, TopToolbarAction::Undo),
            "redo" => self.handle_top_toolbar(ctx, TopToolbarAction::Redo),
            _ => self.user_action(|app| app.execute_search_edit(ctx, id)),
        }
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
        if id.starts_with("tool:") {
            if let Some(tool) = search_tool(id) {
                self.chart.drawings_mut().set_active_tool(Some(tool));
            }
            return;
        }
        if let Some(template) = id.strip_prefix("template:") {
            self.handle_top_toolbar(
                ctx,
                TopToolbarAction::ApplyIndicatorTemplate(template.to_owned()),
            );
            return;
        }
        if let Some(name) = id.strip_prefix("study:") {
            if let Some(study) = self.indicator_dialog.factory().create(name) {
                self.chart.add_indicator(study);
            }
            return;
        }
        if let Some(scene) = match id {
            "smile" => Some(Box::new(SampleScene::Numeric(
                super::numeric_scenes::volatility_smile(),
            ))),
            "yield" => Some(Box::new(SampleScene::Numeric(
                super::numeric_scenes::yield_curve(),
            ))),
            "primitives" => Some(Box::new(SampleScene::Extensions(
                super::primitive_scene::PrimitiveScene::new(),
            ))),
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
        let drawing_action = match id {
            "hide-drawings" => {
                // The toolbar's hide menu flips its state before it reports the choice.
                let state = &mut self.drawing_toolbar.state;
                state.hide_drawings = !state.hide_drawings;
                Some(DrawingToolbarAction::HideAllDrawings)
            }
            "remove-drawings" => Some(DrawingToolbarAction::RemoveAllDrawings),
            "lock-drawings" => {
                // The toolbar's lock button flips its state before it reports the click.
                let state = &mut self.drawing_toolbar.state;
                state.lock_all = !state.lock_all;
                Some(DrawingToolbarAction::LockAllDrawings)
            }
            "magnet" => Some(DrawingToolbarAction::ToggleMagnet),
            "remove-indicators" => Some(DrawingToolbarAction::RemoveAllIndicators),
            _ => None,
        };
        if let Some(action) = drawing_action {
            self.handle_drawing_toolbar(action);
            return;
        }
        let shortcut = match id {
            "interval" => Some(KeyboardAction::OpenIntervalSearch),
            "data-window" => Some(KeyboardAction::OpenDataWindow),
            "open-layout" => Some(KeyboardAction::OpenLayout),
            _ => None,
        };
        if let Some(action) = shortcut {
            self.handle_shortcut_action(ctx, action);
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
            "snapshot" => TopToolbarAction::TakeScreenshot,
            "alert" => TopToolbarAction::CreateAlert,
            "compare" => TopToolbarAction::OpenCompare,
            "fullscreen" => TopToolbarAction::ToggleFullscreen,
            "save-layout" => TopToolbarAction::Save,
            "save-template" => TopToolbarAction::SaveIndicatorTemplate,
            "timezone" | "session" => {
                self.handle_timeframe_toolbar(if id == "timezone" {
                    TimeframeToolbarAction::OpenTimezoneMenu
                } else {
                    TimeframeToolbarAction::OpenSessionMenu
                });
                return;
            }
            "percent" | "auto-scale" => {
                // The scale boxes of the chart settings, applied as their Ok button does.
                let mut settings = self.chart_settings_state();
                let scales = &mut settings.scales_and_lines;
                if id == "percent" {
                    scales.percentage_scale = !scales.percentage_scale;
                    scales.log_scale &= !scales.percentage_scale;
                } else {
                    scales.auto_scale = !scales.auto_scale;
                }
                self.apply_chart_settings(settings);
                return;
            }
            "object-tree" | "watchlist" => {
                self.toggle_panel(if id == "watchlist" {
                    RightBarIcon::Watchlist
                } else {
                    RightBarIcon::ObjectTree
                });
                return;
            }
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
                self.date_picker.open_for_chart(&self.chart);
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
                let query = egui_charts::data::OptionsVolumeQuery::new(
                    self.chart.symbol(),
                    expiry..=expiry,
                );
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
                let symbol = self.chart.symbol().to_owned();
                if symbol != super::live::TARGETS_SYMBOL {
                    self.notify(format!(
                        "{symbol} has no analyst targets in this demo. Select {}, then try again.",
                        super::live::TARGETS_SYMBOL
                    ));
                    return;
                }
                if self.chart.chart_type().is_time_independent() {
                    self.notify(
                        "Analyst targets need a time-based chart type, such as candles. Change \
                         the chart type, then try again.",
                    );
                    return;
                }
                if !self.chart.is_main_series_visible() {
                    self.notify(
                        "Analyst targets start at the close on their publication date. Show the price \
                         series, then try again.",
                    );
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
                self.switch_symbol("STOCKA");
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
        let Some(scene) = self.sample_scene.as_deref_mut() else {
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
            self.open_quick_search();
        }
        if let Some(id) = self.show_quick_search(ui.ctx()) {
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
    use egui_charts::model::ChartType;
    use egui_charts::widget::{Chart, ChartLayoutMode};

    fn export_frame(
        app: &mut DemoApp,
        ctx: &egui::Context,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        ctx.enable_accesskit();
        ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.4, 800.4),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.show_chart(ui, app.theme.chart.bg);
                app.show_dialogs(ui.ctx());
            },
        )
    }

    fn set_export_field(
        app: &mut DemoApp,
        ctx: &egui::Context,
        role: egui::accesskit::Role,
        index: usize,
        value: egui::accesskit::ActionData,
    ) {
        let output = export_frame(app, ctx, vec![]);
        let target_node = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == role)
            .nth(index)
            .unwrap()
            .0;
        output.drop_without_applying_deltas();
        export_frame(
            app,
            ctx,
            vec![Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::SetValue,
                    target_tree: egui::accesskit::TreeId::ROOT,
                    target_node,
                    data: Some(value),
                },
            )],
        )
        .drop_without_applying_deltas();
    }

    fn choose_scope(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let mut point = None;
        for _ in 0..3 {
            let output = export_frame(app, ctx, vec![]);
            point = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
        }
        let point = point.expect("image scope must be selectable in the export dialog");
        export_frame(
            app,
            ctx,
            vec![
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
                    .map(|slot| &mut *slot.chart)
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
            choose_scope(&mut app, &ctx, "Whole layout");
            choose_scope(&mut app, &ctx, "Current size");
            choose_scope(&mut app, &ctx, "Clipboard");
            choose_scope(&mut app, &ctx, "Export");
            let (_, options, ..) = app.screenshot_requested.as_ref().unwrap();
            assert_eq!(options.size, [1280, 800]);
            let output = ctx.run_ui(Default::default(), |ui| app.handle_screenshot(ui.ctx()));
            let image = output
                .platform_output
                .commands
                .iter()
                .find_map(|command| {
                    if let egui::OutputCommand::CopyImage(image) = command {
                        Some(image)
                    } else {
                        None
                    }
                })
                .unwrap();
            for (slot, color) in colors.iter().enumerate().take(count) {
                let (column, row) = (slot % 2, slot / 2);
                assert_eq!(
                    &image.pixels[(150 + row * 400) * image.width() + 5 + column * 640]
                        .to_srgba_unmultiplied()[..3],
                    color
                );
            }
            output.drop_without_applying_deltas();
            choose_scope(&mut app, &ctx, "This chart");
            choose_scope(&mut app, &ctx, "Current size");
            choose_scope(&mut app, &ctx, "Export");
            let (_, options, ..) = app.screenshot_requested.take().unwrap();
            // Stacked slots leave a two-point gap: (800.4 - 2) / 2 rounds to 399.
            assert_eq!(options.size, [640, if count == 2 { 800 } else { 399 }]);
            app.export_dialog.mark_success();
            choose_scope(&mut app, &ctx, "Whole layout");
            choose_scope(&mut app, &ctx, "PNG");
            for (index, side) in [640.0, 400.0].into_iter().enumerate() {
                set_export_field(
                    &mut app,
                    &ctx,
                    egui::accesskit::Role::SpinButton,
                    index,
                    egui::accesskit::ActionData::NumericValue(side),
                );
            }
            choose_scope(&mut app, &ctx, "Export timestamp");
            let before = app.chart.get_time_scale_state();
            for transparent in [false, true] {
                for format in ["SVG", "PNG"] {
                    choose_scope(&mut app, &ctx, format);
                    let path = std::env::temp_dir().join(format!(
                        "demo-arrangement-{count}-{active}-{transparent}.{format}"
                    ));
                    set_export_field(
                        &mut app,
                        &ctx,
                        egui::accesskit::Role::TextInput,
                        0,
                        egui::accesskit::ActionData::Value(
                            path.to_string_lossy().into_owned().into(),
                        ),
                    );
                    choose_scope(&mut app, &ctx, "Export");
                    app.handle_screenshot(&ctx);
                    if format == "SVG" {
                        let svg = std::fs::read_to_string(&path).unwrap();
                        for slot in 1..=count {
                            assert!(svg.contains(&format!("SLOT{slot}")), "missing slot {slot}");
                        }
                        for chrome in ["Export chart", "Scope", "Samples"] {
                            assert!(!svg.contains(chrome));
                        }
                    } else {
                        let image = image::open(&path).unwrap().into_rgba8();
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
                    }
                    std::fs::remove_file(path).unwrap();
                }
                if !transparent {
                    choose_scope(&mut app, &ctx, "Transparent background");
                }
            }
            assert_eq!(app.chart.get_time_scale_state(), before);
            choose_scope(&mut app, &ctx, "This chart");
            choose_scope(&mut app, &ctx, "SVG");
            let path = std::env::temp_dir().join(format!("demo-active-{count}-{active}.svg"));
            set_export_field(
                &mut app,
                &ctx,
                egui::accesskit::Role::TextInput,
                0,
                egui::accesskit::ActionData::Value(path.to_string_lossy().into_owned().into()),
            );
            choose_scope(&mut app, &ctx, "Export");
            let svg = std::fs::read_to_string(&path).unwrap();
            assert!(svg.contains(&format!("SLOT{}", active + 1)));
            for slot in (1..=count).filter(|slot| *slot != active + 1) {
                assert!(!svg.contains(&format!("SLOT{slot}")));
            }
            std::fs::remove_file(path).unwrap();

            use egui_charts::chart::primitives::{Primitive, PrimitivePass, PrimitiveView};
            struct Overlay;
            const OVERLAY: egui::Color32 = egui::Color32::from_rgb(217, 31, 173);
            impl Primitive for Overlay {
                fn draw(
                    &self,
                    painter: &egui::Painter,
                    view: &PrimitiveView<'_>,
                    pass: PrimitivePass,
                ) {
                    if pass == PrimitivePass::Above {
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                view.mapping.rect.center(),
                                egui::vec2(20.0, 20.0),
                            ),
                            0.0,
                            OVERLAY,
                        );
                    }
                }
            }
            app.chart
                .attach_primitive(egui_charts::widget::SeriesId::MAIN, Overlay)
                .unwrap();
            choose_scope(&mut app, &ctx, "Clipboard");
            choose_scope(&mut app, &ctx, "2×");
            for overlays in [true, false] {
                if !overlays {
                    choose_scope(&mut app, &ctx, "Overlays");
                }
                choose_scope(&mut app, &ctx, "Export");
                let output = ctx.run_ui(Default::default(), |ui| app.handle_screenshot(ui.ctx()));
                let image = output
                    .platform_output
                    .commands
                    .iter()
                    .find_map(|command| {
                        if let egui::OutputCommand::CopyImage(image) = command {
                            Some(image)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_eq!(image.size, [1280, 800]);
                assert_eq!(image.pixels.contains(&OVERLAY), overlays);
                output.drop_without_applying_deltas();
            }
            app.financials = Some((
                app.chart.symbol().to_owned(),
                egui_charts::ui::financials::FinancialsPanel::new("Financials", None),
            ));
            let output = export_frame(&mut app, &ctx, vec![]);
            let offered = ["Whole layout", "Current size"].map(|label| {
                (label, output.shapes.iter().any(|shape|
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)))
            });
            output.drop_without_applying_deltas();
            for (label, visible) in offered {
                assert!(
                    !visible,
                    "{label} must not reuse the hidden chart's geometry"
                );
            }
            app.financials = None;
            app.ensure_chart_slots(ChartLayoutMode::Single);
            let output = export_frame(&mut app, &ctx, vec![]);
            assert!(!output.shapes.iter().any(|shape|
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Whole layout")));
            output.drop_without_applying_deltas();
            choose_scope(&mut app, &ctx, "Current size");
            choose_scope(&mut app, &ctx, "Export");
            let (_, options, ..) = app.screenshot_requested.take().unwrap();
            assert_eq!(options.size, [1280, 800]);
        }
    }

    #[test]
    fn toolbar_snapshots_capture_the_visible_layout_for_every_target() {
        use super::super::{ScreenshotTarget, TopToolbarAction};
        for mode in [ChartLayoutMode::TwoHorizontal, ChartLayoutMode::Single] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            app.ensure_chart_slots(mode);
            let colors = [[30, 50, 70], [60, 80, 100]];
            let mut charts: Vec<_> = app
                .extra_charts
                .iter_mut()
                .map(|slot| &mut slot.chart)
                .collect();
            charts.insert(app.layout_slot, &mut app.chart);
            for (slot, chart) in charts.into_iter().enumerate() {
                let mut theme = app.theme.clone();
                let [r, g, b] = colors[slot];
                theme.chart.bg = egui::Color32::from_rgb(r, g, b);
                chart.set_theme_override(Some(theme));
                chart.config_mut().show_grid = false;
                chart.config_mut().show_volume = false;
            }
            export_frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
            let mut expected = None;
            for action in [
                Some(TopToolbarAction::TakeScreenshot),
                Some(TopToolbarAction::CopyImage),
                Some(TopToolbarAction::OpenImage),
                None,
            ] {
                if let Some(action) = action {
                    app.handle_top_toolbar(&ctx, action);
                } else {
                    app.handle_shortcut_action(&ctx, egui_charts::config::KeyboardAction::Snapshot);
                }
                let path = std::env::temp_dir().join(format!("demo-snapshot-{mode:?}.png"));
                let (target, options, ..) = app.screenshot_requested.as_mut().unwrap();
                if matches!(target, ScreenshotTarget::File(_)) {
                    *target = ScreenshotTarget::File(Some(path.clone()));
                }
                options.size = [640, 400];
                options.include_timestamp = false;
                let options = options.clone();
                let output = ctx.run_ui(Default::default(), |ui| app.handle_screenshot(ui.ctx()));
                let image = if path.exists() {
                    let png = std::fs::read(&path).unwrap();
                    std::fs::remove_file(&path).unwrap();
                    if mode == ChartLayoutMode::Single {
                        assert_eq!(
                            png,
                            app.trading.export_png(&app.chart, &ctx, &options).unwrap()
                        );
                    }
                    image::load_from_memory(&png).unwrap().into_rgba8()
                } else {
                    output
                        .platform_output
                        .commands
                        .iter()
                        .find_map(|command| match command {
                            egui::OutputCommand::CopyImage(image) => Some(
                                image::RgbaImage::from_raw(
                                    image.width() as u32,
                                    image.height() as u32,
                                    image
                                        .pixels
                                        .iter()
                                        .flat_map(egui::Color32::to_srgba_unmultiplied)
                                        .collect(),
                                )
                                .unwrap(),
                            ),
                            egui::OutputCommand::OpenUrl(url) => {
                                let image = image::open(&url.url).unwrap().into_rgba8();
                                std::fs::remove_file(&url.url).unwrap();
                                Some(image)
                            }
                            _ => None,
                        })
                        .expect("snapshot must reach its target")
                };
                assert_eq!(image.dimensions(), (640, 400));
                assert_eq!(&image.get_pixel(5, 150).0[..3], &colors[0]);
                assert_eq!(
                    &image.get_pixel(325, 150).0[..3],
                    &colors[usize::from(mode == ChartLayoutMode::TwoHorizontal)]
                );
                if mode == ChartLayoutMode::Single {
                    let png = app.trading.export_png(&app.chart, &ctx, &options).unwrap();
                    assert_eq!(image, image::load_from_memory(&png).unwrap().into_rgba8());
                }
                if let Some(expected) = &expected {
                    assert_eq!(&image, expected);
                } else {
                    expected = Some(image.clone());
                }
                output.drop_without_applying_deltas();
            }
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
    fn quick_search_stars_drawing_tools_and_lists_recent_entries() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        assert!(!app.drawing_toolbar.is_favorite(&DrawingToolType::TrendLine));
        app.open_quick_search();
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        frame(&mut app, &ctx, vec![egui::Event::Text("Trend Line".into())])
            .drop_without_applying_deltas();
        click(&mut app, &ctx, "Favourite Trend Line");
        assert!(app.drawing_toolbar.is_favorite(&DrawingToolType::TrendLine));
        assert!(app.quick_search.is_open());
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
        assert_eq!(
            app.chart.drawings().active_tool(),
            Some(DrawingToolType::TrendLine)
        );
        assert!(!app.quick_search.is_open());
        app.open_quick_search();
        frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
        let output = frame(&mut app, &ctx, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Recent")));
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(
            nodes
                .iter()
                .any(|(_, node)| node.label() == Some("Trend Line")
                    && node.role() == egui::accesskit::Role::Button)
        );
        let star = &nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Favourite Trend Line"))
            .unwrap()
            .1;
        assert_eq!(star.toggled(), Some(egui::accesskit::Toggled::True));
        output.drop_without_applying_deltas();
        click(&mut app, &ctx, "Favourite Trend Line");
        assert!(!app.drawing_toolbar.is_favorite(&DrawingToolType::TrendLine));
        assert!(app.quick_search.is_open());
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
                "Scrolling",
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
            egui_charts::ui::widget_bar::show_data_window(ui, &info, true);
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
        assert_eq!(app.chart.symbol(), "STOCKA");
        assert_eq!(app.financials.as_ref().unwrap().0, "STOCKA");
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
        assert_eq!(app.chart.symbol(), "STOCKA");
        assert!(!app.chart.state().shown().is_empty());
        for (symbol, available) in [
            ("STOCKB", true),
            ("TOKENA", false),
            ("FOREXA", false),
            ("INDEXA", false),
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
        // A row shows the key its action is bound to now, and no key once the binding is gone.
        let mut shortcuts = ShortcutMap::default();
        let edited = egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::U);
        for (key, action) in &mut shortcuts.bindings {
            if *action == KeyboardAction::Undo {
                *key = edited;
            }
        }
        shortcuts
            .bindings
            .retain(|(_, action)| *action != KeyboardAction::Snapshot);
        let commands = quick_search_commands(&shortcuts, &[], &IndicatorFactory::new(), &[]);
        let shortcut = |id: &str| commands.iter().find(|row| row.id == id).unwrap().shortcut;
        assert_eq!(shortcut("undo"), Some(edited));
        assert_eq!(shortcut("snapshot"), None);
        assert!(shortcut("percent").is_some());
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
        click(&mut app, &ctx, "Clear touch values on release");
        assert_eq!(
            app.chart.chart_options().tooltip.touch_exit_mode,
            egui_charts::config::TouchTrackingExitMode::OnTouchEnd
        );
        egui::Popup::close_all(&ctx);
        click(&mut app, &ctx, "Menu");
        click(&mut app, &ctx, "Scrolling");
        click(&mut app, &ctx, "Coast after a touch swipe");
        assert!(!app.chart.chart_options().kinetic_scroll.enabled);
        click(&mut app, &ctx, "Coast after a mouse drag");
        assert!(app.chart.chart_options().kinetic_scroll.mouse);
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
        let refused = [
            (
                "TOKENA",
                ChartType::Candles,
                true,
                "TOKENA has no analyst targets",
            ),
            ("SAMPLE", ChartType::Renko, true, "time-based chart type"),
            ("SAMPLE", ChartType::Candles, false, "Show the price series"),
        ];
        for (symbol, kind, series, reason) in refused {
            app.switch_symbol(symbol);
            app.chart.set_chart_type(kind);
            app.chart.set_main_series_visible(series);
            frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
            let (interval, range) = (app.state.timeframe, app.chart.visible_logical_range());
            app.execute_search_command(&ctx, "targets");
            assert_eq!(app.state.timeframe, interval, "{symbol} {kind:?}");
            assert_eq!(
                app.chart.visible_logical_range(),
                range,
                "{symbol} {kind:?}"
            );
            assert!(!app.chart.price_targets_visible(), "{symbol} {kind:?}");
            assert!(
                app.toasts
                    .toasts()
                    .last()
                    .is_some_and(|toast| toast.message.contains(reason)),
                "{reason}"
            );
        }
        app.chart.set_main_series_visible(true);
        app.execute_search_command(&ctx, "targets");
        let output = frame(&mut app, &ctx, vec![]);
        for name in ["High ", "Average ", "Low "] {
            assert!(
                output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text().starts_with(name))),
                "{name}"
            );
        }
        output.drop_without_applying_deltas();
        let horizon = app.chart.price_targets().unwrap().horizon;
        let horizon = horizon.and_hms_opt(0, 0, 0).unwrap().and_utc();
        assert!(
            f64::from(app.chart.visible_logical_range().unwrap().right)
                >= app.chart.time_to_index(horizon).unwrap()
        );
    }
}
