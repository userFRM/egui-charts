//! Study and drawing templates, kept in the demo's storage.

use super::{DemoApp, DemoSymbol};
use egui_charts::studies::StudyTemplateSet;
use egui_charts::ui::dialogs::templates::DrawingTemplate;

impl DemoApp {
    pub(super) fn load_study_template(&mut self, template: &StudyTemplateSet) {
        // A kept symbol or interval is refused as a switch to it is, before anything changes, so
        // that a visitor's own service is asked only for what its profile offers.
        let own = self.own_data.as_ref();
        let offered = |symbol: &String| match own {
            Some(own) => own.profile.symbols.contains(symbol),
            None => DemoSymbol::catalog().iter().any(|row| row.symbol == symbol),
        };
        if let Some(symbol) = &template.symbol
            && !offered(symbol)
        {
            self.toasts
                .error(format!("{symbol} is not a symbol of this data source"));
            return;
        }
        if let (Some(own), Some(tf)) = (own, template.interval)
            && own.profile.service_interval(tf).is_none()
        {
            self.toasts
                .error(format!("The data source has no interval {tf}"));
            return;
        }
        let applied = self.user_action(|app| {
            app.chart.apply_study_template_set(template)?;
            let captions: Vec<_> = app
                .chart
                .indicators()
                .map(|(id, study)| (id, super::study_input_summary(study)))
                .collect();
            for (id, caption) in captions {
                app.chart.set_indicator_inputs(id, caption);
            }
            app.follow_chart_symbol(false);
            Ok::<_, String>(())
        });
        if let Err(error) = applied {
            self.notify(error);
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
        if let Some(studies) =
            self.study_templates
                .show_for_chart(ctx, "Study templates", &self.chart)
        {
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
        let dialog = &self.indicator_dialog;
        self.top_toolbar.favorite_indicators = dialog
            .indicators()
            .iter()
            .filter(|info| dialog.favorites.contains(&info.id))
            .map(|info| (info.id.clone(), info.name.clone()))
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
