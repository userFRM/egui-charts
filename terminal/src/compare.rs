//! Comparisons: compared symbols, spreads, dividend adjustment and templates.

use super::core::Direction;
use super::layout::Region;
use super::studies::study_input_summary;
use super::{Action, Terminal};
use egui_charts::localization as l10n;
use egui_charts::model::{ChartType, SeriesScale, SymbolType};
use egui_charts::scales::PriceScaleMode;
use egui_charts::services::{StoreKind, SymbolExpression, UserSettings};
use egui_charts::studies::StudyTemplateSet;
use egui_charts::ui::Control;
use egui_charts::ui::dialogs::templates::TemplateDialog;
use egui_charts::ui::floating_replay_control::ReplayControlAction;
use egui_charts::ui::timeframe_toolbar::TimeframeToolbarAction;
use egui_charts::ui::top_toolbar::components::settings_menu::SettingsAction;
use egui_charts::ui::top_toolbar::{
    CompareAction, ComparePlacement, CompareSymbol, CompareSymbolsDialog, TopToolbarAction,
};
use egui_charts::widget::{SeriesId, SeriesOptions};

pub(super) struct State {
    dialog: CompareSymbolsDialog,
    adjustment_offered: Option<bool>,
    study_templates: TemplateDialog<StudyTemplateSet>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            dialog: CompareSymbolsDialog::new(),
            adjustment_offered: None,
            study_templates: TemplateDialog::new(StoreKind::StudyTemplates),
        }
    }
}

impl Terminal {
    pub(super) fn compare_init(&mut self) {
        self.set_control_visible(Control::Compare, self.services.symbol_search.is_some());
        self.compare
            .dialog
            .set_search_service(self.services.symbol_search.clone());
        let storage = self.services.storage.clone();
        self.set_control_visible(Control::Templates, storage.is_some());
        if let Some(storage) = storage {
            self.compare.dialog.set_storage(storage.clone());
            self.compare.study_templates.templates.set_storage(storage);
        }
    }

    pub(super) fn compare_poll(&mut self, _ctx: &egui::Context) {
        self.studies.settings.adjust_for_dividends = self.chart().dividend_adjusted();
        // The chart offers the choice only for an instrument whose feed can adjust.
        let offered = self
            .chart()
            .instrument()
            .is_some_and(|instrument| instrument.dividend_adjustment);
        if self.compare.adjustment_offered != Some(offered) {
            self.compare.adjustment_offered = Some(offered);
            self.set_control_visible(Control::DividendAdjustment, offered);
        }
    }

    pub(super) fn compare_show(&mut self, ui: &mut egui::Ui, region: Region) {
        if region != Region::Windows {
            return;
        }
        self.sync_compare_dialog();
        let action = self.compare.dialog.show(ui.ctx(), &[]);
        if action != CompareAction::None {
            self.emit(Action::Compare(action));
        }
        let title = l10n::text(ui.ctx(), "Study templates");
        if let Some(studies) = self.compare.study_templates.show(ui.ctx(), &title) {
            self.load_study_template(&studies);
        }
        self.top_toolbar.indicator_templates = self
            .compare
            .study_templates
            .templates
            .entries()
            .keys()
            .map(|name| (name.clone(), name.clone()))
            .collect();
    }

    pub(super) fn compare_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::TopToolbar(TopToolbarAction::OpenCompare) => self.compare.dialog.open(),
            Action::Compare(action) => {
                self.user_action(|terminal| terminal.perform_comparison(action))
            }
            Action::SymbolPicked(instrument)
                if instrument.symbol_type == Some(SymbolType::Spread) =>
            {
                self.user_action(|terminal| terminal.add_spread(&instrument.symbol));
            }
            Action::BottomBar(TimeframeToolbarAction::AdjustDividendsToggled(adjusted)) => {
                self.adjust_dividends(adjusted);
            }
            Action::Settings(SettingsAction::Apply(_)) => {
                let remaining = self.studies_perform(action);
                self.adjust_dividends(self.studies.settings.adjust_for_dividends);
                return remaining;
            }
            Action::TopToolbar(TopToolbarAction::SaveIndicatorTemplate) => {
                let current = self.chart().study_template_set();
                self.compare.study_templates.open(current);
            }
            Action::TopToolbar(TopToolbarAction::ApplyIndicatorTemplate(name)) => {
                if let Some(template) = self
                    .compare
                    .study_templates
                    .templates
                    .entries()
                    .get(&name)
                    .cloned()
                {
                    self.load_study_template(&template);
                }
            }
            action => return Some(action),
        }
        None
    }

    pub(super) fn compare_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn compare_modal(&self) -> bool {
        self.compare.dialog.is_open || self.compare.study_templates.is_open
    }

    /// The compared symbols of the active chart, as the dialog lists them.
    fn sync_compare_dialog(&mut self) {
        let chart = &self.charts[self.active];
        let compared = chart
            .series()
            .filter(|(id, series)| chart.is_fed(*id) && !is_spread(&series.name))
            .map(|(id, series)| {
                let mut entry = CompareSymbol::new(&series.name, "", "", series.color);
                entry.visible = chart.is_series_visible(id);
                entry
            })
            .collect();
        self.compare.dialog.compared_symbols = compared;
    }

    /// The series of the active chart fed for `symbol`.
    fn compared_series(&self, symbol: &str) -> Option<SeriesId> {
        let chart = self.chart();
        chart
            .series()
            .find_map(|(id, series)| (chart.is_fed(id) && series.name == symbol).then_some(id))
    }

    fn perform_comparison(&mut self, action: CompareAction) {
        match action {
            CompareAction::AddSymbol(symbol, placement) => self.add_comparison(symbol, placement),
            CompareAction::RemoveSymbol(symbol) => {
                if let Some(id) = self.compared_series(&symbol) {
                    self.chart_mut().remove_series(id);
                }
            }
            CompareAction::ToggleVisibility(symbol) => {
                if let Some(id) = self.compared_series(&symbol) {
                    let visible = self.chart().is_series_visible(id);
                    self.chart_mut().set_series_visible(id, !visible);
                }
            }
            CompareAction::Close => self.compare.dialog.is_open = false,
            _ => {}
        }
    }

    fn add_comparison(&mut self, symbol: String, placement: ComparePlacement) {
        if is_spread(&symbol) {
            self.add_spread(&symbol);
            return;
        }
        if symbol.trim().is_empty() || self.compared_series(&symbol).is_some() {
            return;
        }
        let colors = [
            self.theme.ui.info,
            self.theme.ui.warning,
            self.theme.ui.success,
        ];
        let chart = self.chart();
        let count = chart.series().filter(|(id, _)| chart.is_fed(*id)).count();
        let color = colors[count % colors.len()];
        let chart = self.chart_mut();
        let scale = match placement {
            // The lowest additional scale id no series uses yet.
            ComparePlacement::NewPriceScale => (1..).map(SeriesScale::Separate).find(|scale| {
                chart.series_scale(SeriesId::MAIN) != Some(*scale)
                    && !chart.series().any(|(_, series)| series.scale == *scale)
            }),
            ComparePlacement::NewPane => Some(SeriesScale::Right),
            _ => None,
        };
        let scale = scale.unwrap_or_else(|| {
            // The main series' scale in percent, as `Chart::add_comparison` sets it.
            let percent = chart.price_scale_mut();
            percent.options_mut().mode = PriceScaleMode::Percentage;
            percent.reset();
            chart.series_scale(SeriesId::MAIN).unwrap_or_default()
        });
        let id = chart.add_feed_series(SeriesOptions {
            name: symbol,
            color,
            scale,
            ..Default::default()
        });
        if placement == ComparePlacement::NewPane {
            let pane = chart.add_pane();
            chart.move_series_to_pane(id, pane);
        }
    }

    /// Charts a spread such as `AAPL-MSFT` as candles in a pane of its own, from the history of
    /// each of its symbols.
    fn add_spread(&mut self, symbol: &str) {
        if SymbolExpression::parse(symbol).is_err() || self.compared_series(symbol).is_some() {
            return;
        }
        let (up, down) = (self.theme.ui.info, self.theme.ui.warning);
        let chart = self.chart_mut();
        let id = chart.add_feed_series(SeriesOptions {
            name: symbol.to_owned(),
            chart_type: ChartType::Candles,
            color: up,
            up_color: up,
            down_color: down,
            ..Default::default()
        });
        let pane = chart.add_pane();
        chart.move_series_to_pane(id, pane);
    }

    fn adjust_dividends(&mut self, adjusted: bool) {
        if self.chart().dividend_adjusted() != adjusted {
            if self.replay_active {
                self.perform(Action::Replay(ReplayControlAction::Stop));
            }
            self.chart_mut().set_dividend_adjusted(adjusted);
        }
        self.studies.settings.adjust_for_dividends = adjusted;
    }

    fn load_study_template(&mut self, template: &StudyTemplateSet) {
        let applied = self.user_action(|terminal| {
            let chart = &mut terminal.charts[terminal.active];
            chart.apply_study_template_set_with(template, terminal.studies.indicators.factory())?;
            let captions: Vec<_> = chart
                .indicators()
                .map(|(id, study)| (id, study_input_summary(study)))
                .collect();
            for (id, caption) in captions {
                chart.set_indicator_inputs(id, caption);
            }
            Ok::<_, String>(())
        });
        if let Err(error) = applied {
            self.toasts.error(error);
        }
    }
}

fn is_spread(symbol: &str) -> bool {
    SymbolExpression::parse(symbol)
        .is_ok_and(|expression| expression.to_string() != expression.symbols()[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MemoryStorage, SIZE, TestFeed, click, frame};
    use egui_charts::model::Instrument;
    use egui_charts::services::StoreKind;
    use egui_charts::studies::{
        CustomIndicator, EMA, IndicatorFactory, IndicatorValue, InputValue, RSI, SMA, StudySource,
    };
    use egui_charts::ui::top_toolbar::TopToolbarAction;
    use egui_charts::widget::PaneId;
    use std::sync::Arc;

    fn terminal() -> Terminal {
        let mut terminal = Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_symbol_search(Arc::new(vec![Instrument::new("EURUSD")]));
        frame(&mut terminal, SIZE, vec![]);
        terminal
    }

    fn settle(terminal: &mut Terminal) {
        for _ in 0..4 {
            frame(terminal, SIZE, vec![]);
        }
    }

    fn compared(terminal: &Terminal) -> Vec<(SeriesId, String)> {
        let chart = terminal.chart();
        chart
            .series()
            .filter(|(id, _)| chart.is_fed(*id))
            .map(|(id, series)| (id, series.name.clone()))
            .collect()
    }

    #[test]
    fn compare_actions_chart_the_symbol_where_the_user_placed_it_and_hide_and_remove_it() {
        for placement in [
            ComparePlacement::SamePercentScale,
            ComparePlacement::NewPriceScale,
            ComparePlacement::NewPane,
        ] {
            let mut terminal = terminal();
            settle(&mut terminal);
            let main = terminal.chart().bars(SeriesId::MAIN).to_vec();
            let main_scale = SeriesScale::Separate(1);
            terminal
                .chart_mut()
                .set_series_scale(SeriesId::MAIN, main_scale);
            let run = |terminal: &mut Terminal, action| {
                assert!(terminal.run(Action::Compare(action)).is_none());
                settle(terminal);
            };
            assert!(
                terminal
                    .run(Action::TopToolbar(TopToolbarAction::OpenCompare))
                    .is_none()
            );
            settle(&mut terminal);
            let row = frame(&mut terminal, SIZE, vec![])
                .text_at("EURUSD")
                .unwrap();
            frame(&mut terminal, SIZE, vec![egui::Event::PointerMoved(row)]);
            let label = match placement {
                ComparePlacement::SamePercentScale => "EURUSD",
                ComparePlacement::NewPriceScale => "New price scale",
                ComparePlacement::NewPane => "New pane",
                _ => unreachable!(),
            };
            let button = frame(&mut terminal, SIZE, vec![]).text_at(label).unwrap();
            click(&mut terminal, SIZE, button);
            settle(&mut terminal);

            // The same symbol twice is one series.
            run(
                &mut terminal,
                CompareAction::AddSymbol("EURUSD".into(), placement),
            );
            let [(id, name)] = <[_; 1]>::try_from(compared(&terminal)).expect("one comparison");
            assert_eq!(name, "EURUSD");
            let chart = terminal.chart();
            assert!(!chart.bars(id).is_empty(), "{placement:?} is fed");
            match placement {
                ComparePlacement::SamePercentScale => {
                    assert_eq!(chart.series_scale(id), Some(main_scale));
                    let mode = chart
                        .series_price_scale(SeriesId::MAIN)
                        .unwrap()
                        .options()
                        .mode;
                    assert_eq!(mode, PriceScaleMode::Percentage);
                }
                ComparePlacement::NewPriceScale => {
                    assert_ne!(chart.series_scale(id), Some(main_scale));
                    assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
                }
                ComparePlacement::NewPane => assert_ne!(chart.series_pane(id), Some(PaneId::MAIN)),
                _ => unreachable!(),
            }
            assert_eq!(terminal.compare.dialog.compared_symbols.len(), 1);

            run(
                &mut terminal,
                CompareAction::ToggleVisibility("EURUSD".into()),
            );
            assert!(!terminal.chart().is_series_visible(id));
            assert!(!terminal.compare.dialog.compared_symbols[0].visible);
            run(
                &mut terminal,
                CompareAction::ToggleVisibility("EURUSD".into()),
            );
            assert!(terminal.chart().is_series_visible(id));

            run(&mut terminal, CompareAction::RemoveSymbol("EURUSD".into()));
            assert!(compared(&terminal).is_empty());
            assert!(terminal.compare.dialog.compared_symbols.is_empty());
            assert_eq!(terminal.chart().symbol(), "AAPL");
            assert!(
                terminal.chart().bars(SeriesId::MAIN) == main,
                "{placement:?}"
            );
        }
    }

    #[test]
    fn a_picked_spread_is_charted_as_candles_in_its_own_pane() {
        let mut terminal = terminal();
        let spread = |symbol: &str| {
            Action::SymbolPicked(Box::new(
                Instrument::new(symbol).with_symbol_type(SymbolType::Spread),
            ))
        };
        // A symbol whose name holds an operator sign is still a symbol to open.
        let plain = Action::SymbolPicked(Box::new(Instrument::new("BRK-B")));
        assert!(terminal.run(plain).is_none());
        assert_eq!(terminal.chart().symbol(), "BRK-B");
        terminal.chart_mut().set_symbol("AAPL");
        settle(&mut terminal);
        terminal.run(Action::Compare(CompareAction::AddSymbol(
            "EURUSD".into(),
            ComparePlacement::SamePercentScale,
        )));
        assert!(terminal.run(spread("AAPL-EURUSD")).is_none());
        settle(&mut terminal);
        let chart = terminal.chart();
        let [(compared, _), (id, _)] =
            <[_; 2]>::try_from(compared(&terminal)).expect("a comparison and a spread");
        assert_eq!(chart.series().last().unwrap().1.name, "AAPL-EURUSD");
        assert_eq!(
            chart.series().last().unwrap().1.chart_type,
            ChartType::Candles
        );
        assert_ne!(chart.series_pane(id), Some(PaneId::MAIN));
        let (main, leg, bars) = (
            chart.bars(SeriesId::MAIN),
            chart.bars(compared),
            chart.bars(id),
        );
        assert!(!bars.is_empty());
        for bar in bars {
            let close = |series: &[egui_charts::model::Bar]| {
                series
                    .iter()
                    .find(|other| other.time == bar.time)
                    .unwrap()
                    .close
            };
            assert!((bar.close - (close(main) - close(leg))).abs() < 1e-9);
        }
        // The dialog lists compared symbols, not spreads.
        assert_eq!(terminal.compare.dialog.compared_symbols.len(), 1);
    }

    /// Undo restores the removed comparison's ownership, colour and feed; redo permits a
    /// fresh addition of that symbol.
    #[test]
    fn undo_brings_back_a_removed_comparison_with_its_feed() {
        let mut terminal = terminal();
        let action = |action| Action::Compare(action);
        terminal.run(action(CompareAction::AddSymbol(
            "EURUSD".into(),
            Default::default(),
        )));
        settle(&mut terminal);
        let [(id, _)] = <[_; 1]>::try_from(compared(&terminal)).unwrap();
        let color = terminal.chart().series().next().unwrap().1.color;
        terminal.run(action(CompareAction::RemoveSymbol("EURUSD".into())));
        settle(&mut terminal);
        assert!(compared(&terminal).is_empty());
        assert!(terminal.chart_mut().undo());
        terminal.chart_mut().set_data(id, Vec::new());
        settle(&mut terminal);
        assert_eq!(compared(&terminal), [(id, "EURUSD".into())]);
        assert!(!terminal.chart().bars(id).is_empty());
        assert!(terminal.chart_mut().redo());
        settle(&mut terminal);
        terminal.run(action(CompareAction::AddSymbol(
            "EURUSD".into(),
            Default::default(),
        )));
        settle(&mut terminal);
        let [(again, _)] = <[_; 1]>::try_from(compared(&terminal)).unwrap();
        assert_ne!(id, again);
        assert_eq!(terminal.chart().series().next().unwrap().1.color, color);
    }

    /// Settings and bottom-bar dividend choices reload the feed together, and rejected
    /// settings leave the current request unchanged.
    #[test]
    fn dividend_choices_follow_applied_settings() {
        use egui_charts::widget::settings::ChartSettingsState;
        let mut terminal = terminal();
        settle(&mut terminal);
        assert!(!terminal.chart().dividend_adjusted());
        let bottom =
            |adjusted| Action::BottomBar(TimeframeToolbarAction::AdjustDividendsToggled(adjusted));
        assert!(terminal.run(bottom(true)).is_none());
        settle(&mut terminal);
        assert!(terminal.chart().dividend_adjusted());
        let mut settings = ChartSettingsState::from_chart(terminal.chart());
        settings.adjust_for_dividends = false;
        let apply = |settings| Action::Settings(SettingsAction::Apply(Box::new(settings)));
        assert!(terminal.run(apply(settings.clone())).is_none());
        assert!(!terminal.chart().dividend_adjusted());
        settings.adjust_for_dividends = true;
        assert!(terminal.run(apply(settings.clone())).is_none());
        assert!(terminal.chart().dividend_adjusted());
        settings.adjust_for_dividends = false;
        settings.margins.top_percent = -1.0;
        assert!(terminal.run(apply(settings)).is_none());
        assert!(terminal.chart().dividend_adjusted());
    }

    /// Named templates survive a restart and replace studies as one undoable edit, keeping
    /// their inputs, sources and shared panes while leaving comparison series alone.
    #[test]
    fn named_templates_rebuild_studies_after_restart() {
        for host_study in [false, true] {
            let storage = Arc::new(MemoryStorage::default());
            let make_terminal = || {
                let mut factory = IndicatorFactory::new();
                if host_study {
                    factory.register("Host close", || {
                        Box::new(CustomIndicator::new("Host close", |bars| {
                            bars.iter()
                                .map(|bar| IndicatorValue::Single(bar.close))
                                .collect()
                        }))
                    });
                }
                let mut terminal = Terminal::new(Arc::new(TestFeed))
                    .with_symbol("AAPL")
                    .with_storage(storage.clone())
                    .with_indicator_factory(factory);
                for _ in 0..4 {
                    frame(&mut terminal, SIZE, vec![]);
                }
                terminal
            };
            let mut terminal = make_terminal();
            let custom = host_study.then(|| {
                terminal
                    .studies
                    .indicators
                    .factory()
                    .create("Host close")
                    .unwrap()
            });
            let chart = terminal.chart_mut();
            if let Some(custom) = custom {
                chart.add_indicator(custom);
            }
            let mean = chart.add_indicator(Box::new(SMA::new(7)));
            let rsi = chart.add_indicator(Box::new(RSI::new(2)));
            chart
                .set_indicator_source(mean, StudySource::Study { id: rsi.0, plot: 0 })
                .unwrap();
            assert!(chart.move_indicator_to_pane(mean, rsi));
            let saved = chart.study_template_set();
            assert!(
                terminal
                    .run(Action::TopToolbar(TopToolbarAction::SaveIndicatorTemplate))
                    .is_none()
            );
            assert!(terminal.keyboard_dialog_open());
            frame(&mut terminal, SIZE, vec![]);
            let save = frame(&mut terminal, SIZE, vec![])
                .text_at("Save")
                .expect("the save form is shown");
            click(&mut terminal, SIZE, save + egui::vec2(50.0, -28.0));
            frame(
                &mut terminal,
                SIZE,
                vec![egui::Event::Text("Average".into())],
            );
            let save = frame(&mut terminal, SIZE, vec![]).text_at("Save").unwrap();
            click(&mut terminal, SIZE, save);
            frame(&mut terminal, SIZE, vec![]);
            assert!(storage.get(StoreKind::StudyTemplates, "Average").is_some());
            drop(terminal);

            let mut terminal = make_terminal();
            assert_eq!(
                terminal.top_toolbar.indicator_templates,
                [("Average".into(), "Average".into())]
            );
            let chart = terminal.chart_mut();
            chart.add_indicator(Box::new(EMA::new(5)));
            let comparison = chart.add_comparison("EURUSD", egui::Color32::RED);
            let before = chart.study_template_set();
            let apply =
                || Action::TopToolbar(TopToolbarAction::ApplyIndicatorTemplate("Average".into()));
            assert!(terminal.run(apply()).is_none());
            let chart = terminal.chart();
            let studies: Vec<_> = chart.indicators().collect();
            let (mean, rsi) = (
                studies
                    .iter()
                    .find(|(_, study)| study.name() == "SMA")
                    .unwrap(),
                studies
                    .iter()
                    .find(|(_, study)| study.name() == "RSI")
                    .unwrap(),
            );
            assert_eq!(studies.len(), saved.studies.len());
            assert_eq!(mean.1.inputs()[0].value, InputValue::Length(7));
            assert!(chart.indicator_inputs(mean.0).unwrap().starts_with("7"));
            assert_eq!(
                mean.1.source(),
                Some(StudySource::Study {
                    id: rsi.0.0,
                    plot: 0
                })
            );
            assert_eq!(chart.series_pane(mean.0), chart.series_pane(rsi.0));
            assert!(chart.series().any(|(id, _)| id == comparison));
            assert!(terminal.chart_mut().undo());
            assert_eq!(
                serde_json::to_value(terminal.chart().study_template_set()).unwrap(),
                serde_json::to_value(before).unwrap(),
            );
            terminal
                .chart_mut()
                .set_study_limit(Some(saved.studies.len() - 1));
            let before = serde_json::to_value(terminal.chart().study_template_set()).unwrap();
            assert!(terminal.run(apply()).is_none());
            assert_eq!(
                serde_json::to_value(terminal.chart().study_template_set()).unwrap(),
                before
            );
            assert_eq!(
                terminal.toasts.toasts().last().unwrap().message,
                "Study limit reached"
            );
        }
    }
}
