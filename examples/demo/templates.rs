//! Study and drawing templates, kept in the demo's storage.

use super::DemoApp;
use egui_charts::ui::dialogs::templates::{DrawingTemplate, StudyTemplate};

impl DemoApp {
    pub(super) fn current_study_template(&self) -> Vec<StudyTemplate> {
        self.chart
            .indicators()
            .map(|(_, study)| StudyTemplate::from_study(study))
            .collect()
    }

    pub(super) fn load_study_template(&mut self, template: &[StudyTemplate]) {
        match template
            .iter()
            .map(StudyTemplate::create)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(studies) => self.user_action(|app| {
                app.remove_all_indicators();
                for study in studies {
                    let inputs = super::study_input_summary(study.as_ref());
                    let id = app.chart.add_indicator(study);
                    app.chart.set_indicator_inputs(id, inputs);
                }
            }),
            Err(error) => self.notify(error),
        }
    }

    pub(super) fn current_drawing_template(&self) -> Option<DrawingTemplate> {
        let manager = self.chart.drawings();
        let id = manager.sel_drawing()?;
        manager
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .map(DrawingTemplate::from_drawing)
    }

    pub(super) fn load_drawing_template(&mut self, template: DrawingTemplate) {
        let manager = self.chart.drawings();
        let Some(id) = manager.sel_drawing() else {
            self.notify("Select a drawing first");
            return;
        };
        let Some(drawing) = manager.drawings.iter().find(|drawing| drawing.id == id) else {
            return;
        };
        let mut edited = drawing.clone();
        if let Err(error) = template.apply_to(&mut edited) {
            self.notify(error);
            return;
        }
        self.chart.user_action(|chart| {
            chart
                .drawings_mut()
                .edit_drawing(id, |drawing| *drawing = edited)
        });
    }

    pub(super) fn show_templates(&mut self, ctx: &egui::Context) {
        if let Some(studies) = self.study_templates.show(ctx, "Study templates") {
            self.load_study_template(&studies);
        }
        if let Some(drawing) = self.drawing_templates.show(ctx, "Drawing templates") {
            self.load_drawing_template(drawing);
        }
        self.top_toolbar.indicator_templates = self
            .study_templates
            .templates
            .entries()
            .keys()
            .map(|name| (name.clone(), name.clone()))
            .collect();
        self.drawing_toolbar.state.templates = self
            .drawing_templates
            .templates
            .entries()
            .iter()
            .map(
                |(name, template)| egui_charts::ui::drawing_toolbar::DrawingTemplate {
                    id: name.clone(),
                    name: name.clone(),
                    drawing_type: template.settings.tool_type.as_str().into(),
                    is_default: false,
                },
            )
            .collect();
    }
}
