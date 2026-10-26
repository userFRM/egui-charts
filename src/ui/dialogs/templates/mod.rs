//! Named templates stored by the host, without accounts or a storage backend in the library.

mod dialog;
mod store;

pub use dialog::TemplateDialog;
pub use store::TemplateStore;

use crate::studies::{ConfiguredStudy, Indicator, IndicatorFactory, InputValue, StudySettings};

/// Serializable inputs and presentation of one study. Hosts may resolve `name` through their
/// own study factory when they supply custom studies.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StudyTemplate {
    /// Study factory name.
    pub name: String,
    /// Typed inputs keyed by their stable study-local identifiers.
    pub inputs: Vec<(String, InputValue)>,
    /// Plot styles, precision and interval visibility.
    pub settings: StudySettings,
    /// Whether the study is visible.
    pub visible: bool,
}

impl StudyTemplate {
    /// Capture the current calculation inputs and presentation, without computed values.
    /// Hosts remap chart-local study source ids when saving or restoring a set of studies.
    pub fn from_study(study: &dyn Indicator) -> Self {
        Self {
            name: study.name().into(),
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            visible: study.is_visible(),
        }
    }

    /// Reconstruct a built-in study. Unknown names or invalid inputs leave the host chart intact.
    /// Hosts reconstruct the entire set successfully before replacing their current studies.
    pub fn create(&self) -> Result<Box<dyn Indicator>, String> {
        let study = IndicatorFactory::new()
            .create(&self.name)
            .ok_or_else(|| format!("Unknown study: {}", self.name))?;
        let mut study = ConfiguredStudy::new(study);
        study
            .apply_settings(&self.inputs, self.settings.clone())
            .map_err(str::to_owned)?;
        study.set_visible(self.visible);
        Ok(Box::new(study))
    }
}

pub use crate::drawings::DrawingTemplate;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};

    #[test]
    fn templates_round_trip_settings_study_inputs_and_drawing_properties_without_geometry() {
        let mut settings =
            crate::ui::top_toolbar::components::settings_menu::ChartSettingsState::default();
        settings.margins.top_percent = 12.5;
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<
                crate::ui::top_toolbar::components::settings_menu::ChartSettingsState,
            >(&json)
            .unwrap(),
            settings
        );
        let mut study = ConfiguredStudy::new(Box::new(crate::studies::SMA::new(7)));
        let mut style = study.settings();
        style.plots[0].color = egui::Color32::YELLOW;
        study.apply_settings(&[], style.clone()).unwrap();
        study.set_visible(false);
        let recipe = StudyTemplate::from_study(&study);
        let recipe: StudyTemplate =
            serde_json::from_str(&serde_json::to_string(&recipe).unwrap()).unwrap();
        let restored = recipe.create().unwrap();
        assert_eq!(restored.inputs()[0].value, study.inputs()[0].value);
        assert_eq!(restored.settings(), style);
        assert!(!restored.is_visible());
        let mut drawing = Drawing::new(17, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(3.0, 100.0), ChartPoint::new(5.0, 200.0)];
        drawing.completed = true;
        drawing.stored_id = Some("host-id".into());
        drawing.notes = Some("note".into());
        drawing.label = Some("name".into());
        drawing.screen_anchor = Some([0.25, 0.75]);
        drawing.pane = Some(7);
        let mut template = DrawingTemplate::from_drawing(&drawing);
        template.settings.stroke_width = 4.0;
        let template: DrawingTemplate =
            serde_json::from_str(&serde_json::to_string(&template).unwrap()).unwrap();
        assert!(template.settings.chart_points.is_empty());
        assert!(template.settings.screen_anchor.is_none());
        assert!(template.settings.pane.is_none());
        drawing.pane = Some(9);
        template.apply_to(&mut drawing).unwrap();
        assert_eq!(drawing.stroke_width, 4.0);
        assert_eq!(drawing.id, 17);
        assert_eq!(drawing.stored_id.as_deref(), Some("host-id"));
        assert_eq!(drawing.notes.as_deref(), Some("note"));
        assert_eq!(drawing.label.as_deref(), Some("name"));
        assert_eq!(drawing.chart_points.len(), 2);
        assert_eq!(drawing.screen_anchor, Some([0.25, 0.75]));
        assert_eq!(drawing.pane, Some(9));
        assert_eq!(drawing.chart_points[0].bar_idx, 3.0);
        assert!(drawing.completed);
    }
}
