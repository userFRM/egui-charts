//! Studies: the indicator dialog, chart and series settings, the context menus and symbol search.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::scales::{DefaultPriceFormatter, PriceFormatter, instrument_price_precision};
use egui_charts::services::UserSettings;
use egui_charts::studies::{Indicator, InputKind, InputValue, StudySource};
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{
    KeyboardShortcutsDialog, SeriesContextMenuAction, StudyContextMenuAction,
};
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::top_toolbar::components::indicator_dialog::{
    IndicatorDialog, IndicatorDialogAction, IndicatorPreferences, IndicatorSettingsDialog,
};
use egui_charts::ui::top_toolbar::components::settings_menu::SettingsAction;
use egui_charts::ui::top_toolbar::{SettingsDialog, SymbolSearchDialog, TopToolbarAction};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBarAction};
use egui_charts::widget::settings::ChartSettingsState;
use egui_charts::widget::{ChartElementId, ChartEvent, PointerTarget, SeriesId};

#[derive(Default)]
pub(super) struct State {
    indicators: IndicatorDialog,
    indicator_settings: IndicatorSettingsDialog,
    settings_dialog: SettingsDialog,
    /// The chart settings as last applied. The groups that apply a part of them to their own
    /// components (alerts, trading display, bottom bar) read it.
    pub(super) settings: ChartSettingsState,
    symbol_search: SymbolSearchDialog,
    shortcuts: KeyboardShortcutsDialog,
    /// The symbol last entered in the recent symbols of the search.
    recorded: Option<String>,
}

/// The legend caption of a study: its inputs, in order.
fn study_input_summary(study: &dyn Indicator) -> String {
    study
        .inputs()
        .iter()
        .map(|input| match (&input.kind, &input.value) {
            (InputKind::Choice(choices), InputValue::Choice(index)) => {
                choices.get(*index).cloned().unwrap_or_default()
            }
            (_, InputValue::Length(value)) => value.to_string(),
            (_, InputValue::Integer(value)) => value.to_string(),
            (_, InputValue::Number(value)) => value.to_string(),
            (_, InputValue::Toggle(value)) => value.to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Terminal {
    pub(super) fn studies_init(&mut self) {
        for control in [
            Control::Indicators,
            Control::Settings,
            Control::CopyPrice,
            Control::ResetView,
            Control::Help,
        ] {
            self.set_control_visible(control, true);
        }
        // Without a search service the dialog would list only the symbols already shown.
        let search = self.services.symbol_search.is_some();
        self.set_control_visible(Control::SymbolSearch, search);
        let state = &mut self.studies;
        state.settings_dialog.execution_sounds = self.services.sounds.clone();
        state
            .symbol_search
            .set_search_service(self.services.symbol_search.clone());
        if let Some(storage) = &self.services.storage {
            state.settings_dialog.templates.set_storage(storage.clone());
            state.symbol_search.set_storage(storage.clone());
        }
    }

    pub(super) fn studies_poll(&mut self, _ctx: &egui::Context) {
        // Whichever way the symbol changed, the search lists it among the recent ones.
        let instrument = self.chart().instrument().cloned();
        if let Some(instrument) = instrument
            && self.studies.recorded.as_deref() != Some(instrument.symbol.as_str())
        {
            self.studies.symbol_search.record_symbol(&instrument);
            self.studies.recorded = Some(instrument.symbol);
        }
    }

    pub(super) fn studies_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Chart => self.take_chart_requests(),
            Region::Windows => self.show_study_dialogs(ui.ctx()),
            _ => {}
        }
    }

    pub(super) fn studies_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::TopToolbar(TopToolbarAction::OpenSymbolSearch) => {
                self.studies.symbol_search.open();
            }
            Action::TopToolbar(TopToolbarAction::OpenIndicators) => self.open_indicators(None),
            Action::TopToolbar(TopToolbarAction::AddIndicator(name)) => {
                if let Some(study) = self.studies.indicators.factory().create(&name) {
                    self.add_study(study);
                }
            }
            Action::TopToolbar(TopToolbarAction::OpenSettings) => self.open_chart_settings(),
            Action::DrawingToolbar(
                DrawingToolbarAction::RemoveAllIndicators | DrawingToolbarAction::RemoveAllStudies,
            ) => self.user_action(Self::remove_all_studies),
            Action::DrawingToolbar(DrawingToolbarAction::HideAllIndicators) => {
                // The toolbar belongs to the drawings group, so the toggle follows the chart:
                // hide while any study shows, show once none does.
                let hide = self
                    .chart()
                    .indicators()
                    .any(|(_, study)| study.is_visible());
                let ids: Vec<SeriesId> = self.chart().indicators().map(|(id, _)| id).collect();
                self.user_action(|terminal| {
                    for id in ids {
                        terminal.chart_mut().set_indicator_visible(id, !hide);
                    }
                });
            }
            Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Help)) => {
                self.studies.shortcuts.toggle();
            }
            Action::SymbolPicked(instrument) => self.switch_symbol(&instrument.symbol),
            // The dialog keeps its favourites itself and they are saved with the settings.
            Action::Indicators(IndicatorDialogAction::AddIndicator(study)) => self.add_study(study),
            Action::Indicators(_) => {}
            Action::Settings(SettingsAction::Apply(state)) => self.apply_chart_settings(*state),
            Action::Settings(SettingsAction::PreviewSound { name, volume }) => {
                self.play_sound(&name, volume);
            }
            Action::Settings(SettingsAction::Cancel | SettingsAction::None) => {}
            Action::ChartMenu(ChartContextAction::Chart(menu)) => {
                return self.perform_chart_menu(menu);
            }
            Action::ChartMenu(ChartContextAction::Series(menu)) => {
                return self.perform_series_menu(menu);
            }
            Action::ChartMenu(ChartContextAction::Study { id, action }) => {
                return self.perform_study_menu(id, action);
            }
            action => return Some(action),
        }
        None
    }

    pub(super) fn studies_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        let indicators = &mut self.studies.indicators;
        match direction {
            Direction::Load => indicators.set_preferences(IndicatorPreferences {
                favorites: settings.indicator_favorites.iter().cloned().collect(),
            }),
            Direction::Save => {
                settings.indicator_favorites =
                    indicators.preferences().favorites.into_iter().collect();
            }
        }
    }

    pub(super) fn studies_modal(&self) -> bool {
        let state = &self.studies;
        state.indicators.is_open()
            || state.indicator_settings.is_open()
            || state.settings_dialog.is_open
            || state.symbol_search.is_open()
            || state.shortcuts.is_open
    }

    fn perform_chart_menu(&mut self, menu: ContextMenuAction) -> Option<Action> {
        match menu {
            ContextMenuAction::CopyPrice(price) => self.copy_price(price),
            ContextMenuAction::OpenSettings | ContextMenuAction::OpenSeriesSettings => {
                self.open_chart_settings();
            }
            ContextMenuAction::RemoveAllIndicators => self.user_action(Self::remove_all_studies),
            ContextMenuAction::ResetChart => {
                self.user_action(|terminal| terminal.chart_mut().reset_chart_view());
            }
            // The menus applied these to the chart when they were picked.
            ContextMenuAction::None
            | ContextMenuAction::MergeTpoPrevious
            | ContextMenuAction::MergeTpoNext
            | ContextMenuAction::SplitTpoProfile => {}
            menu => return Some(Action::ChartMenu(ChartContextAction::Chart(menu))),
        }
        None
    }

    fn perform_series_menu(&mut self, menu: SeriesContextMenuAction) -> Option<Action> {
        match menu {
            SeriesContextMenuAction::OpenSettings => self.open_chart_settings(),
            SeriesContextMenuAction::AddIndicator => self.open_indicators(None),
            SeriesContextMenuAction::CopyPrice => {
                let price = self.menus.series_click_price();
                self.copy_price(price);
            }
            SeriesContextMenuAction::None => {}
            menu => return Some(Action::ChartMenu(ChartContextAction::Series(menu))),
        }
        None
    }

    fn perform_study_menu(&mut self, id: SeriesId, menu: StudyContextMenuAction) -> Option<Action> {
        match menu {
            StudyContextMenuAction::AddIndicator => {
                let source = self
                    .chart_mut()
                    .take_indicator_source_request()
                    .unwrap_or(StudySource::Study { id: id.0, plot: 0 });
                self.open_indicators(Some(source));
            }
            StudyContextMenuAction::AddToFavorites(name) => {
                self.studies.indicators.favorites.insert(name);
            }
            StudyContextMenuAction::Copy => {
                if let Some(text) = self.chart().copy_indicator(id) {
                    self.ctx().copy_text(text);
                }
            }
            StudyContextMenuAction::Settings => {
                if let Some(id) = self.chart_mut().take_indicator_settings_request() {
                    self.open_study_settings(id);
                }
            }
            // The menu applied these to the chart when they were picked.
            StudyContextMenuAction::VisualOrder(_)
            | StudyContextMenuAction::Intervals(_)
            | StudyContextMenuAction::MoveTo(_)
            | StudyContextMenuAction::PinTo(_)
            | StudyContextMenuAction::ToggleVisibility
            | StudyContextMenuAction::Remove => {}
            menu => {
                return Some(Action::ChartMenu(ChartContextAction::Study {
                    id,
                    action: menu,
                }));
            }
        }
        None
    }

    /// Opens the indicator dialog, listing the studies that take `source` if there is one.
    fn open_indicators(&mut self, source: Option<StudySource>) {
        let chart = &self.charts[self.active];
        self.studies.indicators.open_for_chart(chart, source);
    }

    /// Adds `study` as one undoable step, captioned with its inputs. A chart at its study cap
    /// refuses it and the user is told.
    fn add_study(&mut self, study: Box<dyn Indicator>) {
        let inputs = study_input_summary(study.as_ref());
        let added = self.user_action(|terminal| {
            let chart = terminal.chart_mut();
            chart
                .try_add_indicator(study)
                .inspect(|id| {
                    chart.set_indicator_inputs(*id, inputs);
                })
                .map(drop)
        });
        if let Err(error) = added {
            self.toasts.error(error);
        }
    }

    fn remove_all_studies(&mut self) {
        let chart = self.chart_mut();
        let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
        for id in ids {
            chart.remove_series(id);
        }
    }

    fn open_chart_settings(&mut self) {
        let last = &self.studies.settings;
        let mut state = ChartSettingsState::from_chart(self.chart());
        state.alerts = last.alerts.clone();
        state.trading = last.trading.clone();
        state.adjust_for_dividends = last.adjust_for_dividends;
        self.studies.settings_dialog.open(state);
    }

    /// Opens the settings of series `id`: the chart's for the main series, the study's for a
    /// study and the compare dialog's for a comparison.
    fn open_study_settings(&mut self, id: SeriesId) {
        if id == SeriesId::MAIN {
            self.open_chart_settings();
        } else if !self
            .studies
            .indicator_settings
            .open_for_chart(&self.charts[self.active], id)
            && self.chart().series().any(|(series, _)| series == id)
        {
            self.emit(Action::TopToolbar(TopToolbarAction::OpenCompare));
        }
    }

    /// Applies the display settings to the chart as one undoable step. Settings the chart
    /// rejects change nothing and are reported.
    fn apply_chart_settings(&mut self, state: ChartSettingsState) {
        let applied = self.user_action(|terminal| state.apply_to_chart(terminal.chart_mut()));
        match applied {
            Ok(()) => self.studies.settings = state,
            Err(error) => self.toasts.error(error),
        }
    }

    fn copy_price(&mut self, price: f64) {
        let decimals = self
            .chart()
            .instrument()
            .and_then(instrument_price_precision)
            .unwrap_or(2);
        self.ctx()
            .copy_text(DefaultPriceFormatter::with_precision(decimals as usize).format(price));
    }

    /// Opens the dialog a click in the chart asked for: a study's settings, the scale settings,
    /// or the chart's settings on a double click of the main series.
    fn take_chart_requests(&mut self) {
        if let Some(id) = self.chart_mut().take_indicator_settings_request() {
            self.open_study_settings(id);
        }
        if self.chart_mut().take_scale_settings_request() {
            self.open_chart_settings();
        }
        let double_clicked = self.chart_events.iter().any(|(slot, event)| {
            *slot == self.active
                && matches!(
                    event,
                    ChartEvent::DoubleClick { position }
                        if matches!(
                            position.target,
                            Some(PointerTarget::Element(ChartElementId::Series(SeriesId::MAIN)))
                        )
                )
        });
        if double_clicked {
            self.open_chart_settings();
        }
        let drawing = self.chart_events.iter().find_map(|(slot, event)| {
            if *slot == self.active
                && let ChartEvent::DoubleClick { position } = event
                && let Some(PointerTarget::Drawing(id)) = position.target
                && self
                    .chart()
                    .drawings()
                    .drawings
                    .iter()
                    .any(|d| d.id == id && (d.locked || !d.tool_type.edits_text_inline()))
            {
                Some(id)
            } else {
                None
            }
        });
        if let Some(id) = drawing {
            self.open_drawing_properties(id);
        }
    }

    fn show_study_dialogs(&mut self, ctx: &egui::Context) {
        let theme = &self.theme;
        let chart = &self.charts[self.active];
        let state = &mut self.studies;
        let mut emitted = Vec::new();

        state
            .indicators
            .set_metric_source(Some(self.feed.clone()), chart.symbol());
        state.indicators.set_full(
            chart
                .study_limit()
                .is_some_and(|limit| chart.indicators().count() >= limit),
        );
        match state.indicators.show(ctx, theme) {
            IndicatorDialogAction::None | IndicatorDialogAction::Close => {}
            action => emitted.push(Action::Indicators(action)),
        }
        let edit = state.indicator_settings.show(ctx, theme);
        match state.settings_dialog.show(ctx, theme) {
            SettingsAction::None => {}
            action => emitted.push(Action::Settings(action)),
        }
        if let Some(instrument) = state.symbol_search.show(ctx, &[]) {
            emitted.push(Action::SymbolPicked(Box::new(instrument)));
        }
        let _ = state.shortcuts.show_with_shortcuts(ctx, chart.shortcuts());

        if let Some(edit) = edit {
            self.apply_study_edit(edit);
        }
        for action in emitted {
            self.emit(action);
        }
    }

    /// Applies the inputs and styles the study settings dialog returned as one undoable step.
    fn apply_study_edit(
        &mut self,
        edit: egui_charts::ui::top_toolbar::components::indicator_dialog::StudyEdit,
    ) {
        let result: Result<(), &'static str> = self.user_action(|terminal| {
            let chart = terminal.chart_mut();
            let inputs = chart
                .indicator_mut(edit.id)
                .ok_or("Study was removed")
                .and_then(|study| {
                    study.apply_settings(&edit.inputs, edit.settings)?;
                    Ok(study_input_summary(study))
                })?;
            chart.set_indicator_inputs(edit.id, inputs);
            Ok(())
        });
        if let Err(error) = result {
            self.toasts.error(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{self, MemoryStorage, SIZE, TestFeed, click, frame};
    use egui::{Color32, Event, PointerButton, Pos2};
    use egui_charts::config::BackgroundStyle;
    use egui_charts::model::LineStyle;
    use egui_charts::studies::SMA;
    use egui_charts::ui::dialogs::SeriesContextMenuAction;
    use egui_charts::widget::settings::{BackgroundType, PrecisionMode, WatermarkMode};
    use std::sync::Arc;

    fn terminal() -> Terminal {
        let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
        for _ in 0..4 {
            frame(&mut terminal, SIZE, vec![]);
        }
        terminal
    }

    fn settings(terminal: &mut Terminal, state: ChartSettingsState) {
        terminal.run(Action::Settings(SettingsAction::Apply(Box::new(state))));
    }

    /// The text the terminal asked the host to copy since the last frame.
    fn copied(terminal: &Terminal) -> Vec<String> {
        terminal.ctx().output(|output| {
            output
                .commands
                .iter()
                .filter_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        })
    }

    fn secondary_click(terminal: &mut Terminal, pos: Pos2) {
        frame(terminal, SIZE, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            let event = Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed,
                modifiers: Default::default(),
            };
            frame(terminal, SIZE, vec![event]);
        }
    }

    /// The point on the main series of the fifth bar from the right.
    fn on_series(terminal: &Terminal) -> Pos2 {
        let chart = terminal.chart();
        let map = chart.get_chart_mapping();
        let index = chart.state().shown().len() - 5;
        Pos2::new(
            map.idx_to_x(index),
            map.price_to_y(chart.state().shown()[index].close),
        )
    }

    /// Guards the study menu's host part: a study added from another study's menu takes that
    /// study as its source, and Copy and Add to favourites reach the clipboard and the dialog.
    #[test]
    fn study_menu_host_actions_keep_sources_and_clipboard_settings() {
        let mut terminal = terminal();
        let parent = terminal.chart_mut().add_indicator(Box::new(SMA::new(7)));
        let menu = |action| Action::ChartMenu(ChartContextAction::Study { id: parent, action });
        assert!(
            terminal
                .run(menu(StudyContextMenuAction::AddToFavorites("SMA".into())))
                .is_none()
        );
        assert!(terminal.studies.indicators.favorites.contains("SMA"));
        assert!(terminal.run(menu(StudyContextMenuAction::Copy)).is_none());
        let [text] = <[String; 1]>::try_from(copied(&terminal)).expect("one copy");
        assert!(text.contains("SMA") && text.contains("7"), "{text}");

        assert!(
            terminal
                .run(menu(StudyContextMenuAction::AddIndicator))
                .is_none()
        );
        for _ in 0..2 {
            frame(&mut terminal, SIZE, vec![]);
        }
        frame(
            &mut terminal,
            SIZE,
            vec![Event::Text("Simple Moving".into())],
        );
        let row = frame(&mut terminal, SIZE, vec![])
            .text_at("Simple Moving Avg")
            .expect("the dialog lists the study");
        click(&mut terminal, SIZE, row);
        let child = terminal
            .chart()
            .indicators()
            .find(|(id, study)| *id != parent && study.name() == "SMA")
            .expect("the dialog adds the selected study")
            .1;
        assert_eq!(
            child.source(),
            Some(StudySource::Study {
                id: parent.0,
                plot: 0
            })
        );
    }

    /// Guards the settings mapping and its validation: a valid state reaches the chart and
    /// is read back by the dialog, and an invalid one changes neither the chart nor the state.
    #[test]
    fn display_settings_apply_styles_and_read_back() {
        let mut terminal = terminal();
        let mut state = terminal.studies.settings.clone();
        state.candle_colors = serde_json::from_str("{}").unwrap();
        state.chart_basic_styles.background_type = BackgroundType::VerticalGradient;
        state.chart_basic_styles.background_gradient_top = Color32::RED;
        state.chart_basic_styles.background_gradient_bottom = Color32::BLUE;
        state.crosshair.line_width = 3.0;
        state.crosshair.line_style = LineStyle::Solid;
        state.crosshair.color = Color32::GREEN;
        state.precision = PrecisionMode::Decimals5;
        state.watermark.mode = WatermarkMode::Symbol;
        state.margins.top_percent = 12.5;
        state.margins.bottom_percent = 7.5;
        state.margins.right_bars = 17;
        settings(&mut terminal, state.clone());
        let chart = terminal.chart();
        assert_eq!(chart.config().candle_border_width, 0.0);
        assert_eq!(
            chart.config().background_style,
            BackgroundStyle::VerticalGradient {
                top_color: Color32::RED,
                bottom_color: Color32::BLUE,
            }
        );
        let crosshair = &chart.chart_options().crosshair;
        assert_eq!(crosshair.vert_line_style, LineStyle::Solid);
        assert_eq!(crosshair.horz_line_width, 3.0);
        assert_eq!(crosshair.vert_line_color, Color32::GREEN);
        assert!(chart.config().show_watermark);
        let scale = chart.state().price_scale();
        assert_eq!(
            scale.map(Default::default(), 0.0, 1.0).price_precision(),
            Some(5)
        );
        assert_eq!(scale.options().scale_margins.top, 0.125);
        assert_eq!(scale.options().scale_margins.bottom, 0.075);
        terminal.run(Action::TopToolbar(TopToolbarAction::OpenSettings));
        let read = terminal.studies.settings_dialog.settings();
        assert_eq!(read.crosshair, state.crosshair);
        assert_eq!(read.precision, state.precision);
        assert_eq!(read.margins, state.margins);

        let before = terminal.studies.settings.clone();
        for (timezone, top, bottom, width) in [
            ("Unknown", 10.0, 10.0, 1.0),
            ("UTC", f32::NAN, 10.0, 1.0),
            ("UTC", 60.0, 40.0, 1.0),
            ("UTC", -1.0, 10.0, 1.0),
            ("UTC", 10.0, 10.0, 0.0),
        ] {
            let mut invalid = state.clone();
            invalid.timezone = timezone.into();
            invalid.margins.top_percent = top;
            invalid.margins.bottom_percent = bottom;
            invalid.crosshair.line_width = width;
            invalid.candle_colors.body_up = Color32::YELLOW;
            settings(&mut terminal, invalid);
            assert_eq!(terminal.studies.settings, before);
            assert_ne!(terminal.chart().config().bullish_color, Color32::YELLOW);
        }
    }

    /// Guards that the dialog opens on what the chart shows now, not on the last applied state,
    /// and that applying it unchanged leaves the chart as it was.
    #[test]
    fn opening_settings_keeps_the_charts_current_display() {
        for (_, theme) in egui_charts::theme::presets() {
            let mut terminal = terminal();
            let mut config = terminal.chart().config().clone().with_theme(&theme);
            config.show_quote_boxes = false;
            config.show_left_axis = true;
            config.show_right_axis = false;
            config.show_countdown = false;
            let chart = terminal.chart_mut();
            chart.price_scale_mut().options_mut().mode =
                egui_charts::scales::PriceScaleMode::Percentage;
            chart.price_scale_mut().set_range(-5.0, 5.0);
            *chart.config_mut() = config.clone();
            terminal.run(Action::TopToolbar(TopToolbarAction::OpenSettings));
            let state = terminal.studies.settings_dialog.settings().clone();
            assert!(state.scales_and_lines.percentage_scale);
            assert!(!state.scales_and_lines.auto_scale);
            settings(&mut terminal, state);
            let applied = terminal.chart().config();
            assert!(!applied.show_quote_boxes && !applied.show_countdown);
            assert!(applied.show_left_axis && !applied.show_right_axis);
            assert_eq!(applied.grid_color, config.grid_color);
            assert_eq!(applied.background_color, config.background_color);
            assert_eq!(applied.bullish_color, config.bullish_color);
            assert_eq!(applied.bearish_color, config.bearish_color);
        }
    }

    /// Guards that the toolbar, the series menu, a double click on the series and a click on the
    /// price axis open the same dialog, on the chart's current display.
    #[test]
    fn settings_entry_points_open_the_same_chart_dialog() {
        for entry in ["top", "series", "double-click", "axis"] {
            let mut terminal = terminal();
            terminal.chart_mut().config_mut().show_quote_boxes = false;
            terminal.chart_mut().config_mut().show_countdown = false;
            match entry {
                "top" => {
                    terminal.run(Action::TopToolbar(TopToolbarAction::OpenSettings));
                }
                "series" => {
                    terminal.run(Action::ChartMenu(ChartContextAction::Series(
                        SeriesContextMenuAction::OpenSettings,
                    )));
                }
                _ => {
                    let pos = if entry == "axis" {
                        let map = terminal.chart().get_chart_mapping();
                        Pos2::new(
                            map.rect.right() + terminal.chart().config().right_axis_width / 2.0,
                            map.rect.bottom() + 14.0,
                        )
                    } else {
                        on_series(&terminal)
                    };
                    frame(&mut terminal, SIZE, vec![Event::PointerMoved(pos)]);
                    for _ in 0..if entry == "axis" { 1 } else { 2 } {
                        for pressed in [true, false] {
                            let event = Event::PointerButton {
                                pos,
                                pressed,
                                button: PointerButton::Primary,
                                modifiers: Default::default(),
                            };
                            frame(&mut terminal, SIZE, vec![event]);
                        }
                    }
                }
            }
            let dialog = &terminal.studies.settings_dialog;
            assert!(dialog.is_open, "{entry} must open chart settings");
            assert!(!dialog.settings().status_line.show_quote_boxes, "{entry}");
            assert_eq!(dialog.settings().chart_type, terminal.chart().chart_type());
        }
    }

    /// Guards that the copied price follows the instrument's precision, from the chart menu
    /// and from the series menu, which the demo showed but never performed.
    #[test]
    fn copy_price_uses_the_selected_instruments_precision() {
        let mut terminal = terminal();
        for (symbol, price, expected) in [
            ("AAPL", 123.45678, "123.46"),
            ("AAPL", -0.001, "0.00"),
            ("EURUSD", 1.12345678, "1.12346"),
        ] {
            terminal.run(Action::SymbolPicked(Box::new(
                egui_charts::model::Instrument::new(symbol),
            )));
            for _ in 0..4 {
                frame(&mut terminal, SIZE, vec![]);
            }
            terminal.run(Action::ChartMenu(ChartContextAction::Chart(
                ContextMenuAction::CopyPrice(price),
            )));
            assert_eq!(copied(&terminal).last().map(String::as_str), Some(expected));
        }
        terminal.run(Action::SymbolPicked(Box::new(
            egui_charts::model::Instrument::new("AAPL"),
        )));
        for _ in 0..4 {
            frame(&mut terminal, SIZE, vec![]);
        }
        let pos = on_series(&terminal);
        secondary_click(&mut terminal, pos);
        let bar = terminal.chart().state().shown().len() - 5;
        let close = terminal.chart().state().shown()[bar].close;
        terminal.run(Action::ChartMenu(ChartContextAction::Series(
            SeriesContextMenuAction::CopyPrice,
        )));
        let text = copied(&terminal).pop().expect("the series menu copies");
        assert_eq!(text, format!("{close:.2}"));
    }

    /// Guards that starred studies are saved with the settings and restored on the next start.
    #[test]
    fn indicator_favourites_survive_a_restart() {
        let storage = Arc::new(MemoryStorage::default());
        let start = || {
            let mut terminal = Terminal::new(Arc::new(TestFeed))
                .with_symbol("AAPL")
                .with_storage(storage.clone());
            test_support::frame(&mut terminal, SIZE, vec![]);
            terminal
        };
        let mut first = start();
        first
            .studies
            .indicators
            .favorites
            .extend(["SMA".to_owned(), "RSI".to_owned()]);
        test_support::frame(&mut first, SIZE, vec![]);
        drop(first);
        let restored = start();
        assert_eq!(
            restored.studies.indicators.preferences().favorites,
            ["RSI".to_owned(), "SMA".to_owned()].into_iter().collect()
        );
    }

    /// Guards that hiding all studies follows what the chart shows: it hides, then shows again.
    #[test]
    fn hide_all_indicators_alternates() {
        let mut terminal = terminal();
        for period in [3, 5] {
            terminal
                .chart_mut()
                .add_indicator(Box::new(SMA::new(period)));
        }
        let visible = |terminal: &Terminal| {
            terminal
                .chart()
                .indicators()
                .map(|(_, study)| study.is_visible())
                .collect::<Vec<_>>()
        };
        for shown in [false, true] {
            terminal.run(Action::DrawingToolbar(
                DrawingToolbarAction::HideAllIndicators,
            ));
            let visible = visible(&terminal);
            assert!(visible.len() >= 2 && visible.iter().all(|v| *v == shown));
        }
    }
}
