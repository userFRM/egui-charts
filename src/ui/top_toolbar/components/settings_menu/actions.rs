//! Actions emitted by settings UI components

use crate::widget::settings::ChartSettingsState;

/// Settings action (emitted by both SettingsMenu and SettingsDialog)
#[derive(Clone)]
#[non_exhaustive]
pub enum SettingsAction {
    /// No action taken
    None,
    /// Apply the given settings state to the chart
    Apply(Box<ChartSettingsState>),
    /// Cancel and discard pending settings changes
    Cancel,
    /// Save current settings as a named template
    SaveTemplate(String),
    /// Load a previously saved settings template by name
    LoadTemplate(String),
}
