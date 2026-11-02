//! Comparisons: compared symbols, spreads, dividend adjustment and templates.

use super::core::Direction;
use super::layout::Region;
use super::studies::study_input_summary;
use super::{Action, Terminal};
use egui_charts::localization as l10n;
use egui_charts::services::{StoreKind, UserSettings};
use egui_charts::studies::StudyTemplateSet;
use egui_charts::ui::Control;
use egui_charts::ui::dialogs::templates::TemplateDialog;
use egui_charts::ui::top_toolbar::TopToolbarAction;

pub(super) struct State {
    study_templates: TemplateDialog<StudyTemplateSet>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            study_templates: TemplateDialog::new(StoreKind::StudyTemplates),
        }
    }
}

impl Terminal {
    pub(super) fn compare_init(&mut self) {
        let storage = self.services.storage.clone();
        self.set_control_visible(Control::Templates, storage.is_some());
        if let Some(storage) = storage {
            self.compare.study_templates.templates.set_storage(storage);
        }
    }

    pub(super) fn compare_poll(&mut self, _ctx: &egui::Context) {}

    pub(super) fn compare_show(&mut self, ui: &mut egui::Ui, region: Region) {
        if region != Region::Windows {
            return;
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
        self.compare.study_templates.is_open
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MemoryStorage, SIZE, TestFeed, click, frame};
    use egui_charts::services::StoreKind;
    use egui_charts::studies::{
        CustomIndicator, EMA, IndicatorFactory, IndicatorValue, InputValue, RSI, SMA, StudySource,
    };
    use egui_charts::ui::top_toolbar::TopToolbarAction;
    use std::sync::Arc;

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
