//! Commands: keyboard requests, quick search, export and the clipboard.

use super::core::{Direction, SidePanel};
use super::layout::Region;
use super::{Action, ExportContents, ExportFile, Terminal};
use crate::config::KeyboardAction;
use crate::drawings::DrawingToolType;
use crate::model::{LogicalRange, Timeframe};
use crate::scales::PriceScaleMode;
use crate::services::UserSettings;
use crate::studies::IndicatorFactory;
use crate::ui::Control;
use crate::ui::context_menu::{ChartContextAction, ContextMenuAction};
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::export::{ExportDialog, ExportFormat, ExportResponse};
use crate::ui::timeframe_toolbar::TimeframeToolbarAction;
use crate::ui::top_toolbar::{LayoutAction, TopToolbarAction};
use crate::ui::widget_bar::{RightBarIcon, WidgetBarAction};
use crate::ui_kit::command_palette::{Command, CommandPaletteHandler};
use crate::widget::{ChartElementId, ImageExportOptions, ShortcutRequest};
use std::path::PathBuf;

const DRAWINGS_CLIPBOARD: &str = "egui-charts-drawings\n";
const STUDY_CLIPBOARD: &str = "egui-charts-study\n";

pub(super) struct State {
    palette: CommandPaletteHandler,
    export: ExportDialog,
    /// The text last copied, for a paste key that arrives without clipboard text.
    copied: Option<String>,
    /// The export dialog opens at the start of the next frame: the Enter that chose the entry
    /// must not also press the dialog's Export button.
    open_export: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            palette: CommandPaletteHandler::new(quick_search_commands()),
            export: ExportDialog::new(),
            copied: None,
            open_export: false,
        }
    }
}

/// What the quick search offers: fixed entries, every drawing tool and every study.
fn quick_search_commands() -> Vec<Command> {
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
        ("targets", "Show analyst price targets", "Actions"),
        ("heatmap", "Market heatmap", "Actions"),
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

/// Where a rendered chart image is delivered.
enum ImageTarget {
    /// The host's file export.
    File,
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
    pub(super) fn commands_init(&mut self) {
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
                if self.perform(open).is_some() {
                    return Some(Action::Shortcut(ShortcutRequest::SymbolSearch(text)));
                }
                // The chart took the typed text from the input; the dialog, which shows later in
                // this frame, types it ahead of the keys queued behind it.
                self.ctx()
                    .input_mut(|input| input.events.insert(0, egui::Event::Text(text)));
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
                    _ if file => ImageTarget::File,
                    _ => ImageTarget::Clipboard,
                };
                self.commands.export.close();
                self.toasts.clear();
                let options = ImageExportOptions {
                    include_crosshair: true,
                    ..Default::default()
                };
                self.export_image(target, &options, None);
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::ExportCsv)) => {
                self.export_csv(None)
            }
            Action::Export(ExportResponse::Export { config, save_path }) => match config.format {
                ExportFormat::Png => {
                    self.export_image(ImageTarget::File, &config.image, save_path);
                }
                ExportFormat::Clipboard => {
                    self.export_image(ImageTarget::Clipboard, &config.image, None);
                }
                ExportFormat::Csv => self.export_csv(save_path),
            },
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
        self.commands.palette.is_open() || self.commands.export.is_open()
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
    /// it. What nothing performs goes back as the key itself.
    fn perform_keyboard(&mut self, keyboard: KeyboardAction) -> Option<Action> {
        use KeyboardAction as A;
        let equivalent = match keyboard {
            A::OpenQuickSearch => Action::TopToolbar(TopToolbarAction::OpenQuickSearch),
            A::ShowKeyboardHelp => Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Help)),
            A::OpenIndicatorSearch => Action::TopToolbar(TopToolbarAction::OpenIndicators),
            A::OpenSymbolSearch => Action::TopToolbar(TopToolbarAction::OpenSymbolSearch),
            A::OpenIntervalSearch => {
                Action::Shortcut(ShortcutRequest::IntervalEntry(String::new()))
            }
            A::GoToDate => Action::BottomBar(TimeframeToolbarAction::OpenDatePicker),
            A::Snapshot => Action::TopToolbar(TopToolbarAction::TakeScreenshot),
            A::SaveLayout => Action::TopToolbar(TopToolbarAction::Save),
            A::OpenLayout => Action::TopToolbar(TopToolbarAction::Layout(LayoutAction::OpenLayout)),
            A::MaximizeChart => Action::TopToolbar(TopToolbarAction::ToggleFullscreen),
            A::OpenDataWindow => {
                self.open_panel(SidePanel::DataWindow);
                return None;
            }
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
        self.perform(equivalent)
            .map(|_| Action::Shortcut(ShortcutRequest::Action(keyboard)))
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
        if let Err(error) = self
            .chart_mut()
            .paste_indicator(text, &IndicatorFactory::new())
        {
            self.toasts
                .error(format!("Could not paste the study: {error}"));
        }
    }

    /// Runs the quick search entry `id` by the action it stands for.
    fn run_quick_command(&mut self, id: &str) {
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
            "heatmap" => {
                self.open_panel(SidePanel::MarketHeatmap);
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
            "targets" => {
                // The interval changes first, as the view that follows depends on it.
                self.perform(Action::TopToolbar(TopToolbarAction::IntervalSelected(
                    Timeframe::Day1,
                )));
                let chart = self.chart_mut();
                chart.set_price_targets_visible(true);
                if let Some(last) = chart.state().shown().last() {
                    let horizon = last.time + chrono::TimeDelta::days(365);
                    if let Some(index) = chart.time_to_index(horizon) {
                        chart.set_visible_logical_range(LogicalRange::new(
                            -10.0,
                            index as f32 + 120.0,
                        ));
                    }
                }
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
            .and_then(|png| match target {
                ImageTarget::File => {
                    let name = format!("{}_chart.png", self.chart().symbol());
                    let shown = path
                        .as_ref()
                        .map_or_else(|| name.clone(), |path| path.display().to_string());
                    if self.hand_over(name, path, ExportContents::Png(png)) {
                        Ok(format!("Export successful: {shown}"))
                    } else {
                        Err("the host has no file export".to_owned())
                    }
                }
                ImageTarget::Clipboard => {
                    ctx.copy_image(decode_png(&png)?);
                    Ok("Image sent to clipboard".to_owned())
                }
            });
        self.report_export(result);
    }

    /// Exports the loaded bars and visible study plots as CSV: to the host's file export, or to
    /// the clipboard without one.
    fn export_csv(&mut self, path: Option<PathBuf>) {
        let result = self
            .chart()
            .export_csv()
            .map_err(|error| error.to_string())
            .map(|csv| {
                if self.services.save.is_none() {
                    self.ctx().copy_text(csv);
                    return "CSV copied to clipboard".to_owned();
                }
                let name = format!("{}_export.csv", self.chart().symbol());
                let shown = path
                    .as_ref()
                    .map_or_else(|| name.clone(), |path| path.display().to_string());
                self.hand_over(name, path, ExportContents::Text(csv));
                format!("Export successful: {shown}")
            });
        self.report_export(result);
    }

    /// Hands a finished file to the host's file export. False without one.
    fn hand_over(&mut self, name: String, path: Option<PathBuf>, contents: ExportContents) -> bool {
        let Some(save) = &mut self.services.save else {
            return false;
        };
        save(ExportFile {
            name,
            path,
            contents,
        });
        true
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
    use crate::drawings::{ChartPoint, Drawing};
    use crate::studies::IndicatorFactory;
    use crate::ui::export::ExportConfig;
    use egui::{Event, Key, Modifiers, RawInput};
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
                ("Export chart", None),
            ] {
                // The host sees what the palette emits and leaves the keys to the terminal.
                let mut terminal = terminal().with_intercept(|action| {
                    !matches!(
                        action,
                        Action::Shortcut(_) | Action::TopToolbar(TopToolbarAction::OpenQuickSearch)
                    )
                });
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
    }

    /// An open dialog of this group takes Escape before the chart does.
    #[test]
    fn open_dialogs_keep_the_keys_of_the_chart() {
        for dialog in ["quick search", "export"] {
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
            if dialog == "export" {
                terminal.commands.export.open();
            } else {
                terminal.commands.palette.open();
            }
            for events in [vec![], vec![key(Key::Escape, Modifiers::NONE)]] {
                frame(&mut terminal, SIZE, events);
            }
            assert!(!terminal.commands_modal(), "{dialog} closes on Escape");
            assert_eq!(terminal.chart().drawings().sel_drawing(), Some(7));
            assert_eq!(
                terminal.chart().drawings().active_tool,
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
        let (commands, _) = frame_output(&mut terminal, vec![Event::Copy]);
        let copied = commands.iter().find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
        assert!(copied.is_some_and(|text| text.starts_with("egui-charts-drawings\n")));

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
        let small = ImageExportOptions {
            size: [200, 120],
            ..Default::default()
        };
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
        let mut terminal = terminal()
            .with_file_export(&[ExportFormat::Png, ExportFormat::Csv], move |file| {
                sink.borrow_mut().push(file)
            });
        frame(&mut terminal, SIZE, vec![]);
        assert!(terminal.run(export(ExportFormat::Png)).is_none());
        assert!(terminal.run(export(ExportFormat::Csv)).is_none());
        let files = files.borrow();
        let [png, csv] = files.as_slice() else {
            panic!("two files: {files:?}");
        };
        assert!(
            matches!(&png.contents, ExportContents::Png(bytes) if bytes.starts_with(b"\x89PNG"))
        );
        assert!(matches!(&csv.contents, ExportContents::Text(text) if text.starts_with("time,")));
        assert_eq!(png.path.as_deref(), Some(std::path::Path::new("out")));
        assert_eq!(
            (png.name.as_str(), csv.name.as_str()),
            ("AAPL_chart.png", "AAPL_export.csv")
        );
    }
}
