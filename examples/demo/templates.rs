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
            app.chart
                .apply_chosen_study_template_set_with(template, app.indicator_dialog.factory())?;
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
        let tool = &template.settings;
        let ids: Vec<usize> = manager
            .selected_ids()
            .filter(|id| {
                manager.drawings.iter().any(|drawing| {
                    drawing.id == *id
                        && drawing.accepts_user_edits()
                        && drawing.tool_type == tool.tool_type
                        && drawing.custom_tool == tool.custom_tool
                })
            })
            .collect();
        let Some(first) = ids
            .first()
            .and_then(|id| manager.drawings.iter().find(|drawing| drawing.id == *id))
        else {
            self.notify("Select a drawing of the template's type");
            return;
        };
        // The template's own checks: every selected drawing passes or fails them alike.
        if let Err(error) = template.apply_to(&mut first.clone()) {
            self.notify(error);
            return;
        }
        if let Err(error) = self.chart.user_action(|chart| {
            chart.drawings_mut().edit_drawings(&ids, |drawing| {
                let _ = template.apply_to(drawing);
            })
        }) {
            self.notify(error);
        }
    }

    pub(super) fn show_templates(&mut self, ctx: &egui::Context) {
        if let Some(studies) =
            self.study_templates
                .show_for_chart(ctx, "Study templates", &self.chart)
        {
            self.load_study_template(&studies);
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
        let templates = &self.drawing_props.templates;
        self.drawing_toolbar.state.templates = templates
            .entries()
            .iter()
            .map(
                |(document, template)| egui_charts::ui::drawing_toolbar::DrawingTemplate {
                    id: document.clone(),
                    name: templates.template_name(document).into(),
                    drawing_type: template.settings.tool_type.as_str().into(),
                    is_default: false,
                },
            )
            .collect();
    }
}
