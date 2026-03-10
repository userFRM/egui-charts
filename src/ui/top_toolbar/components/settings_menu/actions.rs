//! Actions emitted by settings UI components

use super::data::ChartSettingsState;
use crate::ui::stubs::LayoutStyle;

/// Settings action (emitted by both SettingsMenu and SettingsDialog)
#[derive(Clone)]
pub enum SettingsAction {
    /// No action taken
    None,
    /// Apply the given settings state to the chart
    Apply(ChartSettingsState),
    /// Cancel and discard pending settings changes
    Cancel,
    /// Save current settings as a named template
    SaveTemplate(String),
    /// Load a previously saved settings template by name
    LoadTemplate(String),
    /// Layout style changed (Modern or Classic)
    LayoutStyleChanged(LayoutStyle),
}
