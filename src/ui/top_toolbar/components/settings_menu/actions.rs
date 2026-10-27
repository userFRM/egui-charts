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
    /// Play a local preview, activating browser audio in response to the user's click.
    PreviewSound {
        /// Host-provided sound name.
        name: String,
        /// Gain from zero (muted) to one.
        volume: f32,
    },
    /// Cancel and discard pending settings changes
    Cancel,
    /// Save current settings as a named template
    SaveTemplate(String),
    /// Load a previously saved settings template by name
    LoadTemplate(String),
}
