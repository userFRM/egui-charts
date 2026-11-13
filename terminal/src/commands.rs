//! Commands: keyboard requests, quick search, export and the clipboard.

use super::core::Direction;
use super::layout::Region;
use super::{Action, ExportContents, ExportFile, Terminal};
use egui_charts::config::KeyboardAction;
use egui_charts::drawings::DrawingToolType;
use egui_charts::scales::PriceScaleMode;
use egui_charts::services::UserSettings;
use egui_charts::studies::IndicatorFactory;
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{DataTableAction, DataTableDialog};
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::export::{ExportDialog, ExportFormat, ExportResponse};
use egui_charts::ui::timeframe_toolbar::TimeframeToolbarAction;
use egui_charts::ui::top_toolbar::{LayoutAction, TopToolbarAction};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBarAction};
use egui_charts::ui_kit::command_palette::{Command, CommandPaletteHandler};
use egui_charts::widget::{ChartElementId, ImageExportOptions, ShortcutRequest};
use std::path::PathBuf;

const DRAWINGS_CLIPBOARD: &str = "egui-charts-drawings\n";
const STUDY_CLIPBOARD: &str = "egui-charts-study\n";

pub(super) struct State {
    palette: CommandPaletteHandler,
    export: ExportDialog,
    data_table: DataTableDialog,
    host_commands: std::collections::HashSet<String>,
    /// The text last copied, for a paste key that arrives without clipboard text.
    copied: Option<String>,
    /// The export dialog opens at the start of the next frame: the Enter that chose the entry
    /// must not also press the dialog's Export button.
    open_export: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            palette: CommandPaletteHandler::new(Vec::new()),
            export: ExportDialog::new(),
            data_table: DataTableDialog::new(),
            host_commands: Default::default(),
            copied: None,
            open_export: false,
        }
    }
}

/// What the quick search offers: fixed entries, every drawing tool and every study.
fn quick_search_commands(factory: &IndicatorFactory) -> Vec<Command> {
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
    let mut names = factory.list();
    names.sort();
    commands.extend(names.into_iter().map(|name| {
        Command::new(format!("study:{name}"), format!("Add {name}"))
            .category("Studies")
            .keywords([factory.category(&name).unwrap_or_default()])
    }));
    commands
}

/// Where a rendered chart image is delivered.
enum ImageTarget {
    /// The host's file export.
    File {
        open: bool,
    },
    Clipboard,
}

/// Decodes the PNG of a chart export into an image for the clipboard.
fn decode_png(png: &[u8]) -> Result<egui::ColorImage, String> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("the chart image is not 8-bit RGBA".to_owned());
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &pixels[..info.buffer_size()],
    ))
}

impl Terminal {
    /// Adds a quick search entry whose id is returned as [`Action::Command`] when chosen.
    /// An entry with the same id replaces the previous entry, including a built-in command.
    pub fn add_command(&mut self, command: Command) {
        self.commands.host_commands.insert(command.id.clone());
        self.commands.palette.remove_command(&command.id);
        self.commands.palette.add_command(command);
    }

    pub(super) fn commands_init(&mut self) {
        for command in quick_search_commands(self.studies.indicators.factory()) {
            if !self.commands.host_commands.contains(&command.id) {
                self.commands.palette.add_command(command);
            }
        }
        for control in [Control::QuickSearch, Control::Snapshot, Control::ExportData] {
            self.set_control_visible(control, true);
        }
        // Files go to the host; without it the clipboard is all an export can reach.
        let mut formats = if self.services.save.is_some() {
            self.services.export_formats.to_vec()
        } else {
            Vec::new()
        };
        if !formats.contains(&ExportFormat::Clipboard) {
            formats.push(ExportFormat::Clipboard);
        }
        self.commands.export = ExportDialog::new().with_formats(&formats);
    }

    pub(super) fn commands_poll(&mut self, _ctx: &egui::Context) {
        if std::mem::take(&mut self.commands.open_export) {
            self.commands.export.open();
        }
    }

    pub(super) fn commands_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Chart => self.open_dialogs_of_keys(),
            Region::Windows => {
                let ctx = ui.ctx().clone();
                if let Some(id) = self.commands.palette.show(&ctx) {
                    self.run_quick_command(&id);
                }
                match self
                    .commands
                    .data_table
                    .show(&ctx, &self.charts[self.active])
                {
                    DataTableAction::None => {}
                    response => self.emit(Action::DataTable(response)),
                }
                match self.commands.export.show(&ctx) {
                    ExportResponse::None => {}
                    response => self.emit(Action::Export(response)),
                }
            }
            _ => {}
        }
    }

    pub(super) fn commands_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::Shortcut(ShortcutRequest::SymbolSearch(text)) => {
                let open = Action::TopToolbar(TopToolbarAction::OpenSymbolSearch);
                if !self.dispatch(open) {
                    return None;
                }
                self.studies.symbol_search.open_with_query(text);
            }
            Action::Shortcut(ShortcutRequest::PasteDrawing(text)) => {
                if text.starts_with(STUDY_CLIPBOARD) {
                    self.paste_study(&text);
                } else if text.starts_with(DRAWINGS_CLIPBOARD) {
                    // The drawings group takes drawings; left over, they are the host's.
                    return Some(Action::Shortcut(ShortcutRequest::PasteDrawing(text)));
                }
            }
            Action::Shortcut(ShortcutRequest::Action(keyboard)) => {
                return self.perform_keyboard(keyboard);
            }
            Action::TopToolbar(TopToolbarAction::OpenQuickSearch) => {
                self.commands.palette.open();
            }
            Action::TopToolbar(
                top @ (TopToolbarAction::TakeScreenshot
                | TopToolbarAction::CopyImage
                | TopToolbarAction::OpenImage),
            ) => {
                let file = self.services.save.is_some();
                let target = match top {
                    TopToolbarAction::CopyImage => ImageTarget::Clipboard,
                    // The library writes no files and has no viewer: without the host's file
                    // export a screenshot goes to the clipboard and opening is the host's.
                    TopToolbarAction::OpenImage if !file => {
                        return Some(Action::TopToolbar(top));
                    }
                    _ if file => ImageTarget::File {
                        open: top == TopToolbarAction::OpenImage,
                    },
                    _ => ImageTarget::Clipboard,
                };
                self.toasts.clear();
                let mut options = ImageExportOptions::default();
                options.include_crosshair = true;
                self.export_image(target, &options, None);
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::OpenDataTable)) => {
                self.commands.data_table.open();
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::ExportCsv)) => {
                self.export_csv(None)
            }
            Action::Export(ExportResponse::Export { config, save_path }) => match config.format {
                ExportFormat::Png => {
                    self.export_image(ImageTarget::File { open: false }, &config.image, save_path);
                }
                ExportFormat::Clipboard => {
                    self.export_image(ImageTarget::Clipboard, &config.image, None);
                }
                ExportFormat::Csv => self.export_csv(save_path),
                ExportFormat::Svg => {
                    let result = self
                        .chart()
                        .export_svg(&self.ctx(), &config.image)
                        .map_err(|error| error.to_string())
                        .and_then(|svg| {
                            self.hand_over(
                                format!("{}_export.svg", self.chart().symbol()),
                                save_path,
                                ExportContents::Text(svg),
                                false,
                            )
                        });
                    self.report_export(result);
                }
            },
            Action::DataTable(DataTableAction::Export(csv)) => {
                self.deliver_text("chart_data.csv".into(), None, csv);
            }
            Action::DataTable(_) => {}
            Action::Export(ExportResponse::Closed | ExportResponse::None) => {}
            action => return Some(action),
        }
        None
    }

    pub(super) fn commands_settings(
        &mut self,
        _settings: &mut UserSettings,
        _direction: Direction,
    ) {
    }

    pub(super) fn commands_modal(&self) -> bool {
        self.commands.palette.is_open()
            || self.commands.export.is_open()
            || self.commands.data_table.is_open
    }

    /// Performs the keys that open a dialog before the dialogs show, so that the keys queued
    /// behind the request in the same frame reach the dialog and not the chart.
    fn open_dialogs_of_keys(&mut self) {
        use KeyboardAction as A;
        let opens = |action: &Action| {
            matches!(
                action,
                Action::Shortcut(
                    ShortcutRequest::SymbolSearch(_)
                        | ShortcutRequest::Action(
                            A::OpenQuickSearch
                                | A::OpenSymbolSearch
                                | A::OpenIndicatorSearch
                                | A::ShowKeyboardHelp
                                | A::GoToDate
                                | A::OpenLayout
                        )
                )
            )
        };
        let (opening, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.emitted)
            .into_iter()
            .partition(opens);
        self.emitted = rest;
        for action in opening {
            self.dispatch(action);
        }
    }

    /// Performs a key the chart hands to its host, by the action of the control that stands for
    /// it. The equivalent action follows the same host intercept as its control.
    fn perform_keyboard(&mut self, keyboard: KeyboardAction) -> Option<Action> {
        use KeyboardAction as A;
        let equivalent = match keyboard {
            A::OpenQuickSearch => Action::TopToolbar(TopToolbarAction::OpenQuickSearch),
            A::ShowKeyboardHelp => Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Help)),
            A::OpenIndicatorSearch => Action::TopToolbar(TopToolbarAction::OpenIndicators),
            A::OpenSymbolSearch => Action::TopToolbar(TopToolbarAction::OpenSymbolSearch),
            A::Snapshot => Action::TopToolbar(TopToolbarAction::TakeScreenshot),
            A::OpenLayout => Action::TopToolbar(TopToolbarAction::Layout(LayoutAction::OpenLayout)),
            A::MaximizeChart => Action::TopToolbar(TopToolbarAction::ToggleFullscreen),
            A::CopyDrawing => {
                self.copy_selection();
                return None;
            }
            A::PasteDrawing => {
                let text = self.commands.copied.clone()?;
                return self
                    .perform(Action::Shortcut(ShortcutRequest::PasteDrawing(text)))
                    .map(|_| Action::Shortcut(ShortcutRequest::Action(keyboard)));
            }
            _ => return Some(Action::Shortcut(ShortcutRequest::Action(keyboard))),
        };
        self.dispatch(equivalent);
        None
    }

    /// Copies the selected drawings, or else the selected study, to the system clipboard.
    fn copy_selection(&mut self) {
        match self.chart().copy_selected_drawings() {
            Ok(Some(text)) => self.copy_text(text),
            Ok(None) => {
                if let Some(ChartElementId::Indicator(id)) = self.chart().selected_element()
                    && let Some(text) = self.chart().copy_indicator(id)
                {
                    self.copy_text(text);
                }
            }
            Err(error) => {
                self.toasts
                    .error(format!("Could not copy drawings: {error}"));
            }
        }
    }

    fn copy_text(&mut self, text: String) {
        self.ctx().copy_text(text.clone());
        self.commands.copied = Some(text);
    }

    fn paste_study(&mut self, text: &str) {
        if let Err(error) =
            self.charts[self.active].paste_indicator(text, self.studies.indicators.factory())
        {
            self.toasts
                .error(format!("Could not paste the study: {error}"));
        }
    }

    /// Runs the quick search entry `id` by the action it stands for.
    fn run_quick_command(&mut self, id: &str) {
        if self.commands.host_commands.contains(id) {
            self.emit(Action::Command(id.to_owned()));
            return;
        }
        if let Some(name) = id.strip_prefix("tool:") {
            if let Some(tool) = DrawingToolType::all()
                .iter()
                .find(|tool| tool.data_name() == name)
            {
                self.emit(Action::DrawingToolbar(DrawingToolbarAction::SelectTool(
                    *tool,
                )));
            }
            return;
        }
        if let Some(name) = id.strip_prefix("study:") {
            self.emit(Action::TopToolbar(TopToolbarAction::AddIndicator(
                name.to_owned(),
            )));
            return;
        }
        let action = match id {
            "symbols" => Action::TopToolbar(TopToolbarAction::OpenSymbolSearch),
            "indicators" => Action::TopToolbar(TopToolbarAction::OpenIndicators),
            "settings" => Action::TopToolbar(TopToolbarAction::OpenSettings),
            "replay" => Action::TopToolbar(TopToolbarAction::ToggleReplay),
            "help" => Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Help)),
            "date" => Action::BottomBar(TimeframeToolbarAction::OpenDatePicker),
            "reset" => Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::ResetChart)),
            "export" => {
                self.commands.open_export = true;
                return;
            }
            "log" => {
                self.user_action(|terminal| {
                    let mode = &mut terminal.chart_mut().price_scale_mut().options_mut().mode;
                    *mode = if *mode == PriceScaleMode::Logarithmic {
                        PriceScaleMode::Normal
                    } else {
                        PriceScaleMode::Logarithmic
                    };
                });
                return;
            }
            "invert" => {
                self.user_action(|terminal| {
                    let options = terminal.chart_mut().price_scale_mut().options_mut();
                    options.invert_scale = !options.invert_scale;
                });
                return;
            }
            "grid" => {
                self.user_action(|terminal| {
                    let config = terminal.chart_mut().config_mut();
                    config.show_grid = !config.show_grid;
                });
                return;
            }
            _ => return,
        };
        self.emit(action);
    }

    /// Renders the active chart as PNG and delivers it. The export dialog, if open, shows how it
    /// went.
    fn export_image(
        &mut self,
        target: ImageTarget,
        options: &ImageExportOptions,
        path: Option<PathBuf>,
    ) {
        let ctx = self.ctx();
        let result = self
            .chart()
            .export_png(&ctx, options)
            .map_err(|error| error.to_string())
            .and_then(|png| match target {
                ImageTarget::File { open } => self.hand_over(
                    format!("{}_chart.png", self.chart().symbol()),
                    path,
                    ExportContents::Png(png),
                    open,
                ),
                ImageTarget::Clipboard => {
                    ctx.copy_image(decode_png(&png)?);
                    Ok("Image sent to clipboard".to_owned())
                }
            });
        self.report_export(result);
    }

    /// Exports loaded bars and visible study plots to the host, or the clipboard without one.
    fn export_csv(&mut self, path: Option<PathBuf>) {
        match self.chart().export_csv() {
            Ok(csv) => {
                self.deliver_text(format!("{}_export.csv", self.chart().symbol()), path, csv)
            }
            Err(error) => self.report_export(Err(error.to_string())),
        }
    }

    fn deliver_text(&mut self, name: String, path: Option<PathBuf>, text: String) {
        let result = if self.services.save.is_some() {
            self.hand_over(name, path, ExportContents::Text(text), false)
        } else {
            self.ctx().copy_text(text);
            Ok("CSV copied to clipboard".to_owned())
        };
        self.report_export(result);
    }

    /// Hands a finished file to the host and reports its delivery result.
    fn hand_over(
        &mut self,
        name: String,
        path: Option<PathBuf>,
        contents: ExportContents,
        open: bool,
    ) -> Result<String, String> {
        let save = self
            .services
            .save
            .as_mut()
            .ok_or("the host has no file export")?;
        let shown = path
            .as_ref()
            .map_or_else(|| name.clone(), |path| path.display().to_string());
        save(ExportFile {
            name,
            path,
            contents,
            open,
        })?;
        Ok(if open {
            "Image opened in viewer".to_owned()
        } else {
            format!("Export successful: {shown}")
        })
    }

    /// Tells the user, and the export dialog if it is open, how an export went.
    fn report_export(&mut self, result: Result<String, String>) {
        if self.commands.export.is_open() {
            match &result {
                Ok(_) => self.commands.export.mark_success(),
                Err(error) => self.commands.export.mark_error(error.clone()),
            }
        }
        match result {
            Ok(message) => self.toast(message),
            Err(error) => self.toasts.error(format!("Export failed: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{SIZE, TestFeed, frame, key};
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};
    use egui_charts::drawings::{ChartPoint, Drawing};
    use egui_charts::studies::IndicatorFactory;
    use egui_charts::ui::export::ExportConfig;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn terminal() -> Terminal {
        Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL")
    }

    /// Runs a frame and returns what it asked the platform to do, such as copying to the
    /// clipboard, with what `show` returned.
    fn frame_output(
        terminal: &mut Terminal,
        events: Vec<Event>,
    ) -> (Vec<egui::OutputCommand>, Vec<Action>) {
        let ctx = terminal.ctx.clone().expect("a frame has run");
        let mut actions = Vec::new();
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                time: Some(ctx.input(|i| i.time) + 0.1),
                events,
                ..Default::default()
            },
            |ui| actions = terminal.show(ui),
        );
        let commands = output.platform_output.commands.clone();
        output.drop_without_applying_deltas();
        (commands, actions)
    }

    fn trend_line(id: usize, price: f64) -> Drawing {
        let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(10.0, price), ChartPoint::new(20.0, price)];
        drawing.completed = true;
        drawing
    }

    /// The quick search types its entry even when the key that opens it and the text share a
    /// frame, and runs the entry as the action the owner performs.
    #[test]
    fn quick_search_runs_the_entry_it_is_given() {
        let ctrl_k = Modifiers::CTRL.plus(Modifiers::COMMAND);
        // The ways to open it: the key with the text in the same frame, the key then the text,
        // and the toolbar action then the text.
        for open in ["same frame", "key", "toolbar"] {
            for (query, expected) in [
                (
                    "RSI",
                    Some(Action::TopToolbar(TopToolbarAction::AddIndicator(
                        "RSI".into(),
                    ))),
                ),
                (
                    "Trend Line",
                    Some(Action::DrawingToolbar(DrawingToolbarAction::SelectTool(
                        DrawingToolType::TrendLine,
                    ))),
                ),
                (
                    "Chart settings",
                    Some(Action::TopToolbar(TopToolbarAction::OpenSettings)),
                ),
                (
                    "Host average",
                    Some(Action::TopToolbar(TopToolbarAction::AddIndicator(
                        "Host average".into(),
                    ))),
                ),
                ("Export chart", None),
                ("Sample backtest", Some(Action::Command("backtest".into()))),
                ("Host drawing", Some(Action::Command("tool:host".into()))),
            ] {
                // The host sees what the palette emits and leaves the keys to the terminal.
                let mut terminal = terminal().with_intercept(|action| {
                    !matches!(
                        action,
                        Action::Shortcut(_) | Action::TopToolbar(TopToolbarAction::OpenQuickSearch)
                    )
                });
                let mut factory = IndicatorFactory::new();
                factory.register_in(IndicatorFactory::PERSONAL, "Host average", || {
                    Box::new(egui_charts::studies::SMA::new(42))
                });
                terminal = terminal.with_indicator_factory(factory);
                terminal.add_command(Command::new("backtest", "Sample backtest"));
                terminal.add_command(Command::new("tool:host", "Host drawing"));
                frame(&mut terminal, SIZE, vec![]);
                let mut seen = Vec::new();
                let typed = Event::Text(query.into());
                let opening = Event::ModifiersChanged(ctrl_k);
                let mut frames = vec![match open {
                    "same frame" => vec![opening, key(Key::K, ctrl_k), typed.clone()],
                    "key" => vec![opening, key(Key::K, ctrl_k)],
                    _ => vec![],
                }];
                if open == "toolbar" {
                    assert!(
                        terminal
                            .run(Action::TopToolbar(TopToolbarAction::OpenQuickSearch))
                            .is_none()
                    );
                }
                if open != "same frame" {
                    frames.push(vec![Event::ModifiersChanged(Modifiers::NONE), typed]);
                }
                // The empty frame shows the dialog that the entry opened.
                frames.push(vec![key(Key::Enter, Modifiers::NONE)]);
                frames.push(vec![]);
                for events in frames {
                    seen.extend(frame(&mut terminal, SIZE, events).actions);
                }
                let label = format!("{query} opened by {open}");
                match expected {
                    Some(expected) => {
                        assert_eq!(format!("{seen:?}"), format!("{:?}", [expected]), "{label}");
                    }
                    None => {
                        assert!(seen.is_empty(), "{label}: {seen:?}");
                        assert!(terminal.commands.export.is_open(), "{label}");
                    }
                }
                assert!(!terminal.commands.palette.is_open(), "{label}");
            }
        }
        // Each search edit is one undo step.
        let flags = |terminal: &mut Terminal| {
            let grid = terminal.chart().config().show_grid;
            let scale = terminal.chart_mut().price_scale_mut().options();
            (
                grid,
                scale.mode == PriceScaleMode::Logarithmic,
                scale.invert_scale,
            )
        };
        for command in ["grid", "log", "invert"] {
            let mut terminal = terminal();
            for _ in 0..3 {
                frame(&mut terminal, SIZE, vec![]);
            }
            let before = flags(&mut terminal);
            terminal.run_quick_command(command);
            assert_ne!(flags(&mut terminal), before, "{command}");
            terminal.run(Action::TopToolbar(TopToolbarAction::Undo));
            assert_eq!(flags(&mut terminal), before, "one undo restores {command}");
        }
    }

    /// An open dialog of this group takes Escape before the chart does.
    #[test]
    fn open_dialogs_keep_the_keys_of_the_chart() {
        for dialog in ["quick search", "export", "data table", "help", "indicators"] {
            let mut terminal = terminal();
            frame(&mut terminal, SIZE, vec![]);
            terminal
                .chart_mut()
                .drawings_mut()
                .drawings
                .push(trend_line(7, 100.0));
            terminal.chart_mut().drawings_mut().select(7);
            terminal
                .chart_mut()
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::TrendLine));
            match dialog {
                "export" => terminal.commands.export.open(),
                "data table" => {
                    terminal.run(Action::ChartMenu(ChartContextAction::Chart(
                        ContextMenuAction::OpenDataTable,
                    )));
                }
                "indicators" => {
                    let slash = frame(&mut terminal, SIZE, vec![key(Key::Slash, Modifiers::NONE)]);
                    assert!(slash.actions.is_empty(), "{:?}", slash.actions);
                    assert!(terminal.keyboard_dialog_open());
                }
                "help" => {
                    terminal.run(Action::Shortcut(ShortcutRequest::Action(
                        KeyboardAction::ShowKeyboardHelp,
                    )));
                }
                _ => terminal.commands.palette.open(),
            }
            for events in [
                vec![],
                vec![key(Key::Delete, Modifiers::NONE)],
                vec![key(Key::Escape, Modifiers::NONE)],
            ] {
                frame(&mut terminal, SIZE, events);
            }
            assert!(!terminal.keyboard_dialog_open(), "{dialog}");
            assert_eq!(terminal.chart().drawings().sel_drawing(), Some(7));
            assert_eq!(
                terminal.chart().drawings().active_tool(),
                Some(DrawingToolType::TrendLine)
            );
        }
    }

    /// Copy puts the selected drawings on the system clipboard, a pasted study is one undo step,
    /// and other text is ignored.
    #[test]
    fn clipboard_copies_drawings_and_pastes_a_study_once() {
        let mut terminal = terminal();
        for _ in 0..3 {
            frame(&mut terminal, SIZE, vec![]);
        }
        let drawings = terminal.chart_mut().drawings_mut();
        drawings.drawings.push(trend_line(7, 100.0));
        drawings.drawings.push(trend_line(8, 110.0));
        drawings.select_drawings([7, 8]);
        let (commands, actions) = frame_output(&mut terminal, vec![Event::Copy]);
        assert!(actions.is_empty(), "{actions:?}");
        let copied = commands.iter().find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
        let copied = copied.expect("system clipboard text");
        assert!(copied.starts_with(DRAWINGS_CLIPBOARD));
        terminal.commands.copied = None;
        for (events, count) in [
            (vec![Event::Paste("unrelated clipboard text".into())], 2),
            (vec![Event::Paste(copied)], 4),
            (vec![key(Key::Z, Modifiers::COMMAND)], 2),
        ] {
            let (_, actions) = frame_output(&mut terminal, events);
            assert!(actions.is_empty(), "{actions:?}");
            assert_eq!(terminal.chart().drawings().drawings.len(), count);
        }

        let studies = |terminal: &Terminal| terminal.chart().indicators().count();
        let rsi = IndicatorFactory::new().create("RSI").unwrap();
        let id = terminal.chart_mut().add_indicator(rsi);
        let text = terminal.chart().copy_indicator(id).unwrap();
        let before = studies(&terminal);
        let paste = |text: &str| vec![Event::Paste(text.into())];
        for (events, count) in [
            (paste("unrelated clipboard text"), before),
            (paste(&text), before + 1),
        ] {
            let (_, returned) = frame_output(&mut terminal, events);
            assert!(returned.is_empty(), "{returned:?}");
            assert_eq!(studies(&terminal), count);
        }
        // One undo removes the pasted study.
        assert!(
            terminal
                .run(Action::TopToolbar(TopToolbarAction::Undo))
                .is_none()
        );
        assert_eq!(studies(&terminal), before);
    }

    /// Images and CSV reach the clipboard without a file export and the host's callback with
    /// one; opening an image is the host's.
    #[test]
    fn exports_go_to_the_clipboard_or_the_hosts_file_export() {
        let mut small = ImageExportOptions::default();
        small.size = [200, 120];
        let export = |format| {
            Action::Export(ExportResponse::Export {
                config: ExportConfig {
                    format,
                    image: small.clone(),
                },
                save_path: Some("out".into()),
            })
        };
        // Without a file export.
        let mut terminal = terminal();
        frame(&mut terminal, SIZE, vec![]);
        let open = Action::TopToolbar(TopToolbarAction::OpenImage);
        assert!(terminal.run(open).is_some());
        assert!(terminal.run(export(ExportFormat::Clipboard)).is_none());
        assert!(terminal.run(export(ExportFormat::Csv)).is_none());
        let (commands, _) = frame_output(&mut terminal, vec![]);
        assert!(commands.iter().any(|command| matches!(command,
            egui::OutputCommand::CopyImage(image) if image.size == [200, 120])));
        assert!(commands.iter().any(|command| matches!(command,
            egui::OutputCommand::CopyText(csv) if csv.starts_with("time,"))));

        // With one.
        let files = Rc::new(RefCell::new(Vec::new()));
        let sink = files.clone();
        let mut terminal = terminal_with_files(sink);
        frame(&mut terminal, SIZE, vec![]);
        assert!(terminal.run(export(ExportFormat::Png)).is_none());
        assert!(terminal.run(export(ExportFormat::Csv)).is_none());
        assert!(terminal.run(export(ExportFormat::Svg)).is_none());
        assert!(
            terminal
                .run(Action::TopToolbar(TopToolbarAction::OpenImage))
                .is_none()
        );
        let files = files.borrow();
        let [png, csv, svg, opened] = files.as_slice() else {
            panic!("four files: {files:?}");
        };
        assert!(
            matches!(&png.contents, ExportContents::Png(bytes) if bytes.starts_with(b"\x89PNG"))
        );
        assert!(matches!(&csv.contents, ExportContents::Text(text) if text.starts_with("time,")));
        assert!(matches!(&svg.contents, ExportContents::Text(text) if text.contains("<svg")));
        assert_eq!(svg.name, "AAPL_export.svg");
        assert!(!png.open);
        assert!(opened.open);
        assert_eq!(png.path.as_deref(), Some(std::path::Path::new("out")));
        assert_eq!(
            (png.name.as_str(), csv.name.as_str()),
            ("AAPL_chart.png", "AAPL_export.csv")
        );
        drop(files);

        // The table's selected columns reach the intercept before any file callback.
        terminal = terminal.with_intercept(|action| matches!(action, Action::DataTable(_)));
        terminal.run(Action::ChartMenu(ChartContextAction::Chart(
            ContextMenuAction::OpenDataTable,
        )));
        frame(&mut terminal, SIZE, vec![]);
        let table = frame(&mut terminal, SIZE, vec![]);
        let returned = super::super::test_support::click(
            &mut terminal,
            SIZE,
            table.text_at("Export CSV").unwrap(),
        );
        let [Action::DataTable(DataTableAction::Export(csv))] = returned.as_slice() else {
            panic!("table export returned to the host: {returned:?}");
        };
        assert!(csv.starts_with("time,"));
    }

    /// A failed host write stays visible in the dialog, and a retry delivers only once.
    #[test]
    fn failed_exports_keep_the_dialog_open_and_show_the_host_error() {
        for format in [ExportFormat::Png, ExportFormat::Csv, ExportFormat::Svg] {
            let attempts = Rc::new(std::cell::Cell::new(0));
            let sink = attempts.clone();
            let mut terminal = terminal().with_file_export(
                &[ExportFormat::Png, ExportFormat::Csv, ExportFormat::Svg],
                move |_| {
                    sink.set(sink.get() + 1);
                    if sink.get() == 1 {
                        Err("Permission denied".into())
                    } else {
                        Ok(())
                    }
                },
            );
            frame(&mut terminal, SIZE, vec![]);
            terminal.commands.export = ExportDialog::new().with_formats(&[format]);
            terminal.commands.export.open();
            frame(&mut terminal, SIZE, vec![]);
            for (attempt, message) in [(1, "Error: Permission denied"), (2, "Export successful")] {
                let mut image = ImageExportOptions::default();
                image.size = [200, 120];
                terminal.run(Action::Export(ExportResponse::Export {
                    config: ExportConfig { format, image },
                    save_path: Some("chart.out".into()),
                }));
                frame(&mut terminal, SIZE, vec![]);
                let ctx = terminal.ctx();
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                        time: Some(ctx.input(|input| input.time) + 0.1),
                        ..Default::default()
                    },
                    |ui| {
                        let _ = terminal.show(ui);
                    },
                );
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                assert!(terminal.commands.export.is_open());
                assert_eq!(attempts.get(), attempt);
                assert!(
                    texts.iter().any(|text| text.starts_with(message)),
                    "{format:?}: {texts:?}"
                );
            }
        }
    }

    /// Text and editing keys queued behind the opening shortcut reach symbol search in order.
    #[test]
    fn typed_symbol_search_keeps_queued_editing_keys() {
        use egui_charts::model::Instrument;
        for (typed, suffix, expected) in [("AAPX", "L", "AAPL"), ("BTCUSX", "D", "BTCUSD")] {
            let symbols = Arc::new(vec![Instrument::new(expected)]);
            let mut terminal = terminal().with_symbol_search(symbols);
            for _ in 0..3 {
                frame(&mut terminal, SIZE, vec![]);
            }
            terminal
                .chart_mut()
                .drawings_mut()
                .drawings
                .push(trend_line(7, 100.0));
            terminal.chart_mut().drawings_mut().select(7);
            frame(
                &mut terminal,
                SIZE,
                vec![
                    Event::Text(typed.into()),
                    key(Key::Backspace, Modifiers::NONE),
                    Event::Text(suffix.into()),
                ],
            );
            assert_eq!(terminal.chart().drawings().drawings.len(), 1);
            frame(&mut terminal, SIZE, vec![key(Key::Enter, Modifiers::NONE)]);
            assert_eq!(terminal.chart().symbol(), expected);
            assert!(!terminal.keyboard_dialog_open());
        }
    }

    fn terminal_with_files(sink: Rc<RefCell<Vec<ExportFile>>>) -> Terminal {
        terminal().with_file_export(
            &[ExportFormat::Png, ExportFormat::Csv, ExportFormat::Svg],
            move |file| {
                sink.borrow_mut().push(file);
                Ok(())
            },
        )
    }
}
