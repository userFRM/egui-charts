//! Configuration for the alert creation dialog.

use crate::tokens::DESIGN_TOKENS;

/// Configuration for the alert dialog
#[derive(Clone, Debug)]
pub struct AlertDialogConfig {
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
    /// Input width
    pub input_width: f32,
    /// Button height
    pub button_height: f32,
}

impl Default for AlertDialogConfig {
    fn default() -> Self {
        Self {
            width: DESIGN_TOKENS.sizing.dialog.alert_width,
            height: DESIGN_TOKENS.sizing.dialog.default_height,
            tab_height: DESIGN_TOKENS.sizing.settings_dialog.tab_height,
            content_padding: DESIGN_TOKENS.spacing.xl,
            row_spacing: DESIGN_TOKENS.spacing.lg,
            label_width: DESIGN_TOKENS.sizing.settings_dialog.label_width,
            input_width: DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
            button_height: DESIGN_TOKENS.sizing.settings_dialog.button_height,
        }
    }
}
