//! Configuration for the series settings dialog.

use crate::tokens::DESIGN_TOKENS;

/// Configuration for the series settings dialog
#[derive(Clone, Debug)]
pub struct SeriesSettingsConfig {
    /// Dialog width
    pub width: f32,
    /// Dialog height
    pub height: f32,
    /// Tab height
    pub tab_height: f32,
    /// Content padding
    pub content_padding: f32,
    /// Row spacing
    pub row_spacing: f32,
    /// Label width
    pub label_width: f32,
    /// Control width
    pub control_width: f32,
}

impl Default for SeriesSettingsConfig {
    fn default() -> Self {
        Self {
            width: 520.0,
            height: 480.0,
            tab_height: DESIGN_TOKENS.sizing.settings_dialog.tab_height,
            content_padding: DESIGN_TOKENS.spacing.lg,
            row_spacing: DESIGN_TOKENS.spacing.md,
            label_width: 110.0,
            control_width: DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
        }
    }
}
