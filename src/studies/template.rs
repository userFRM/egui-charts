//! Serializable inputs and presentation of one study.

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
