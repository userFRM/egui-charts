//! Comparisons: compared symbols, spreads, dividend adjustment and templates.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::drawings::DrawingTemplate;
use crate::localization as l10n;
use crate::model::{ChartType, SeriesScale, SymbolType};
use crate::services::{StoreKind, SymbolExpression, UserSettings};
use crate::studies::{Indicator, InputKind, InputValue, StudySource, StudyTemplate};
use crate::ui::Control;
use crate::ui::dialogs::templates::TemplateDialog;
use crate::ui::timeframe_toolbar::TimeframeToolbarAction;
use crate::ui::top_toolbar::{
    CompareAction, ComparePlacement, CompareSymbol, CompareSymbolsDialog, TopToolbarAction,
};
use crate::widget::{SeriesId, SeriesOptions};

pub(super) struct State {
    dialog: CompareSymbolsDialog,
    study_templates: TemplateDialog<Vec<StudyTemplate>>,
    drawing_templates: TemplateDialog<DrawingTemplate>,
    /// Whether the active instrument offers dividend adjusted bars, as last told to the
    /// controls.
    adjustment_offered: Option<bool>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            dialog: CompareSymbolsDialog::new(),
            study_templates: TemplateDialog::new(StoreKind::StudyTemplates),
            drawing_templates: TemplateDialog::new(StoreKind::DrawingTemplates),
            adjustment_offered: None,
        }
    }
}

impl Terminal {
    pub(super) fn compare_init(&mut self) {
        // Without a search service the dialog would list only the symbols already compared.
        let search = self.services.symbol_search.is_some();
        self.set_control_visible(Control::Compare, search);
        self.compare
            .dialog
            .set_search_service(self.services.symbol_search.clone());
        let storage = self.services.storage.clone();
        self.set_control_visible(Control::Templates, storage.is_some());
        if let Some(storage) = storage {
            self.compare.dialog.set_storage(storage.clone());
            self.compare
                .study_templates
                .templates
                .set_storage(storage.clone());
            self.compare
                .drawing_templates
                .templates
                .set_storage(storage);
        }
    }

    pub(super) fn compare_poll(&mut self, _ctx: &egui::Context) {
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
        let ctx = ui.ctx().clone();
        self.sync_compare_dialog();
        let action = self.compare.dialog.show(&ctx, &[]);
        if action != CompareAction::None {
            self.emit(Action::Compare(action));
        }
        let title = l10n::text(&ctx, "Study templates");
        if let Some(studies) = self.compare.study_templates.show(&ctx, &title) {
            self.load_study_template(&studies);
        }
        let title = l10n::text(&ctx, "Drawing templates");
        if let Some(drawing) = self.compare.drawing_templates.show(&ctx, &title) {
            self.load_drawing_template(drawing);
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
            Action::Compare(action) => self.perform_comparison(action),
            Action::SymbolPicked(instrument)
                if instrument.symbol_type == Some(SymbolType::Spread)
                    || is_spread(&instrument.symbol) =>
            {
                self.add_spread(&instrument.symbol);
            }
            Action::TopToolbar(TopToolbarAction::SaveIndicatorTemplate) => {
                let current = self.current_study_template();
                self.compare.study_templates.open(current);
            }
            Action::TopToolbar(TopToolbarAction::ApplyIndicatorTemplate(name)) => {
                let template = self.compare.study_templates.templates.entries().get(&name);
                if let Some(template) = template.cloned() {
                    self.load_study_template(&template);
                }
            }
            Action::DrawingToolbar(action) => return self.perform_template_action(action),
            // The settings dialog keeps its templates itself.
            Action::Settings(
                crate::ui::top_toolbar::components::settings_menu::SettingsAction::SaveTemplate(_)
                | crate::ui::top_toolbar::components::settings_menu::SettingsAction::LoadTemplate(_),
            ) => {}
            Action::BottomBar(TimeframeToolbarAction::AdjustDividendsToggled(adjusted)) => {
                if self.replay_active {
                    self.perform(Action::TopToolbar(TopToolbarAction::ToggleReplay));
                }
                self.chart_mut().set_dividend_adjusted(adjusted);
            }
            action => return Some(action),
        }
        None
    }

    pub(super) fn compare_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn compare_modal(&self) -> bool {
        self.compare.dialog.is_open
            || self.compare.study_templates.is_open
            || self.compare.drawing_templates.is_open
    }

    fn perform_template_action(
        &mut self,
        action: crate::ui::drawing_toolbar::DrawingToolbarAction,
    ) -> Option<Action> {
        use crate::ui::drawing_toolbar::DrawingToolbarAction;
        match action {
            DrawingToolbarAction::ManageTemplates => match self.current_drawing_template() {
                Some(template) => self.compare.drawing_templates.open(template),
                None => self.compare.drawing_templates.open_catalog(),
            },
            DrawingToolbarAction::SaveTemplate { name } => {
                let Some(template) = self.current_drawing_template() else {
                    self.toasts
                        .info(l10n::text(&self.ctx(), "Select a drawing first"));
                    return None;
                };
                if let Err(error) = self
                    .compare
                    .drawing_templates
                    .templates
                    .save(&name, template)
                {
                    self.toasts.error(error);
                }
            }
            DrawingToolbarAction::LoadTemplate { template_id } => {
                let template = self
                    .compare
                    .drawing_templates
                    .templates
                    .entries()
                    .get(&template_id);
                if let Some(template) = template.cloned() {
                    self.load_drawing_template(template);
                }
            }
            // The toolbar opens its own menu.
            DrawingToolbarAction::OpenTemplateMenu => {}
            action => return Some(Action::DrawingToolbar(action)),
        }
        None
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
            CompareAction::None | CompareAction::Close => {}
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
        let id = match scale {
            Some(scale) => chart.add_feed_series(SeriesOptions {
                name: symbol,
                color,
                scale,
                ..Default::default()
            }),
            None => chart.add_comparison(symbol, color),
        };
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

    fn current_study_template(&self) -> Vec<StudyTemplate> {
        let studies: Vec<_> = self.chart().indicators().collect();
        studies
            .iter()
            .map(|(_, study)| {
                let mut template = StudyTemplate::from_study(*study);
                // Saved sources refer to positions in this template, not the old chart's ids.
                for (_, value) in &mut template.inputs {
                    if let InputValue::Source(StudySource::Study { id, .. }) = value {
                        *id = studies
                            .iter()
                            .position(|(source, _)| source.0 == *id)
                            .unwrap_or(usize::MAX);
                    }
                }
                template
            })
            .collect()
    }

    fn load_study_template(&mut self, template: &[StudyTemplate]) {
        match template
            .iter()
            .map(StudyTemplate::create)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(studies) => self.user_action(|terminal| {
                let chart = terminal.chart_mut();
                let old: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
                for id in old {
                    chart.remove_series(id);
                }
                let mut ids = Vec::new();
                for study in studies {
                    let inputs = study_input_summary(study.as_ref());
                    let id = chart.add_indicator(study);
                    chart.set_indicator_inputs(id, inputs);
                    ids.push(id);
                }
                for (id, stored) in ids.iter().zip(template) {
                    for (input, value) in &stored.inputs {
                        if let InputValue::Source(StudySource::Study { id: parent, plot }) = value {
                            let study = chart.indicator_mut(*id).expect("added study");
                            study
                                .set_input(
                                    input,
                                    InputValue::Source(StudySource::Study {
                                        id: ids.get(*parent).map_or(usize::MAX, |id| id.0),
                                        plot: *plot,
                                    }),
                                )
                                .expect("validated study source");
                            let inputs = study_input_summary(study);
                            chart.set_indicator_inputs(*id, inputs);
                        }
                    }
                }
            }),
            Err(error) => self.toasts.error(error),
        }
    }

    fn current_drawing_template(&self) -> Option<DrawingTemplate> {
        let manager = self.chart().drawings();
        let id = manager.sel_drawing()?;
        manager
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .map(DrawingTemplate::from_drawing)
    }

    fn load_drawing_template(&mut self, template: DrawingTemplate) {
        let manager = self.chart().drawings();
        let Some(id) = manager.sel_drawing() else {
            self.toasts
                .info(l10n::text(&self.ctx(), "Select a drawing first"));
            return;
        };
        let Some(drawing) = manager.drawings.iter().find(|drawing| drawing.id == id) else {
            return;
        };
        let mut edited = drawing.clone();
        if let Err(error) = template.apply_to(&mut edited) {
            self.toasts.error(error);
            return;
        }
        self.chart_mut().user_action(|chart| {
            chart
                .drawings_mut()
                .edit_drawing(id, |drawing| *drawing = edited)
        });
    }
}

/// Whether `symbol` is an expression over several symbols, such as `AAPL-MSFT`.
fn is_spread(symbol: &str) -> bool {
    SymbolExpression::parse(symbol).is_ok_and(|expression| expression.is_spread())
}

/// The input values of `study` as the legend caption shows them.
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{MemoryStorage, SIZE, TestFeed, frame};
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::Instrument;
    use crate::scales::PriceScaleMode;
    use crate::studies::{EMA, RSI, SMA};
    use crate::ui::drawing_toolbar::DrawingToolbarAction;
    use crate::widget::PaneId;
    use std::sync::Arc;

    fn terminal() -> Terminal {
        let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
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
            let main_scale = terminal.chart().series_scale(SeriesId::MAIN);
            let run = |terminal: &mut Terminal, action| {
                assert!(terminal.run(Action::Compare(action)).is_none());
                settle(terminal);
            };
            run(
                &mut terminal,
                CompareAction::AddSymbol("EURUSD".into(), placement),
            );
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
                    assert_eq!(chart.series_scale(id), main_scale);
                    let mode = chart
                        .series_price_scale(SeriesId::MAIN)
                        .unwrap()
                        .options()
                        .mode;
                    assert_eq!(mode, PriceScaleMode::Percentage);
                }
                ComparePlacement::NewPriceScale => {
                    assert_ne!(chart.series_scale(id), main_scale);
                    assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
                }
                ComparePlacement::NewPane => assert_ne!(chart.series_pane(id), Some(PaneId::MAIN)),
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
        // A plain pick is the symbol search's to perform.
        let plain = Action::SymbolPicked(Box::new(Instrument::new("MSFT")));
        assert!(matches!(terminal.run(plain), Some(Action::SymbolPicked(_))));
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
            let close = |series: &[crate::model::Bar]| {
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

    #[test]
    fn templates_rebuild_studies_with_their_sources_and_restyle_the_selected_drawing() {
        let mut terminal = Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_storage(Arc::new(MemoryStorage::default()));
        settle(&mut terminal);
        let chart = terminal.chart_mut();
        let mean = chart.add_indicator(Box::new(SMA::new(7)));
        let rsi = chart.add_indicator(Box::new(RSI::new(2)));
        chart
            .set_indicator_source(mean, StudySource::Study { id: rsi.0, plot: 0 })
            .unwrap();
        let comparison = chart.add_comparison("EURUSD", egui::Color32::RED);
        let mut line = Drawing::new(17, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(3.0, 100.0), ChartPoint::new(5.0, 200.0)];
        line.completed = true;
        line.stroke_width = 4.0;
        chart.drawings_mut().drawings.push(line);
        chart.drawings_mut().select(17);
        settle(&mut terminal);

        let study = terminal.current_study_template();
        let drawing = terminal
            .current_drawing_template()
            .expect("selected drawing");
        terminal
            .compare
            .study_templates
            .templates
            .save("Average", study)
            .unwrap();
        settle(&mut terminal);
        assert_eq!(
            terminal.top_toolbar.indicator_templates,
            [("Average".to_owned(), "Average".to_owned())]
        );
        terminal
            .compare
            .drawing_templates
            .templates
            .save("Trend", drawing)
            .unwrap();

        terminal.chart_mut().add_indicator(Box::new(EMA::new(5)));
        let apply = Action::TopToolbar(TopToolbarAction::ApplyIndicatorTemplate("Average".into()));
        assert!(terminal.run(apply).is_none());
        let chart = terminal.chart();
        let named = |name: &str| {
            chart
                .indicators()
                .find(|(_, study)| study.name().starts_with(name))
        };
        // The chart's volume study, the two templated ones and not the EMA added since.
        assert_eq!(chart.indicators().count(), 3);
        assert!(named("EMA").is_none());
        let (mean, rsi) = (named("SMA").unwrap(), named("RSI").unwrap());
        assert_eq!(mean.1.inputs()[0].value, InputValue::Length(7));
        assert_eq!(
            mean.1.source(),
            Some(StudySource::Study {
                id: rsi.0.0,
                plot: 0
            })
        );
        // Series the host or the user compared stay.
        assert!(chart.series().any(|(id, _)| id == comparison));

        let mut target = Drawing::new(23, DrawingToolType::TrendLine);
        target.chart_points = vec![ChartPoint::new(8.0, 300.0), ChartPoint::new(10.0, 400.0)];
        target.completed = true;
        let chart = terminal.chart_mut();
        chart.drawings_mut().drawings.push(target);
        chart.drawings_mut().select(23);
        let load = DrawingToolbarAction::LoadTemplate {
            template_id: "Trend".into(),
        };
        assert!(terminal.run(Action::DrawingToolbar(load)).is_none());
        let chart = terminal.chart_mut();
        let restyled = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == 23)
            .unwrap();
        assert_eq!(restyled.stroke_width, 4.0);
        assert_eq!(restyled.chart_points[0].bar_idx, 8.0);
        chart.undo();
        let undone = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == 23)
            .unwrap();
        assert_ne!(undone.stroke_width, 4.0);
        chart.drawings_mut().deselect();
        let save = DrawingToolbarAction::SaveTemplate {
            name: "Missing".into(),
        };
        assert!(terminal.run(Action::DrawingToolbar(save)).is_none());
        assert_eq!(
            terminal.compare.drawing_templates.templates.entries().len(),
            1
        );
    }

    #[test]
    fn dividend_adjustment_toggle_is_performed_on_the_chart() {
        let mut terminal = terminal();
        for adjusted in [true, false] {
            let toggle =
                Action::BottomBar(TimeframeToolbarAction::AdjustDividendsToggled(adjusted));
            assert!(terminal.run(toggle).is_none());
            assert_eq!(terminal.chart().dividend_adjusted(), adjusted);
        }
    }
}
