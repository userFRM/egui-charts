//! Actions emitted by settings UI components

use crate::widget::settings::ChartSettingsState;

/// Settings action emitted by the [`SettingsDialog`](super::SettingsDialog)
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SettingsAction {
    /// No action taken
    None,
    /// Apply the given settings state to the chart
    Apply(Box<ChartSettingsState>),
    /// Apply the given settings state to every chart of the layout, with
    /// [`LayoutCopy::Settings`](crate::widget::LayoutCopy::Settings). Offered after
    /// [`SettingsDialog::offer_apply_to_all`](super::SettingsDialog::offer_apply_to_all).
    ApplyToAll(Box<ChartSettingsState>),
    /// Play a local preview, activating browser audio in response to the user's click.
    PreviewSound {
        /// Host-provided sound name.
        name: String,
        /// Gain from zero (muted) to one.
        volume: f32,
    },
    /// Empty depth columns and heatmap textures through
    /// [`Chart::clear_depth_history`](crate::Chart::clear_depth_history). The Order flow tab's
    /// Release memory button leaves the dialog open; edited settings still apply only with Ok.
    ClearDepthHistory,
    /// Cancel and discard pending settings changes
    Cancel,
    /// Save current settings as a named template
    SaveTemplate(String),
    /// Load a previously saved settings template by name
    LoadTemplate(String),
}
