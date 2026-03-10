//! Configuration for indicator dialog appearance and behavior

use crate::theme::Theme;
use egui::Color32;

/// Configuration for the indicator dialog
pub struct IndicatorDialogConfig {
    pub width: f32,
    pub height: f32,
    pub bg_color: Color32,
    pub sidebar_color: Color32,
    pub text_color: Color32,
    pub muted_color: Color32,
    pub hover_color: Color32,
    pub sel_color: Color32,
    pub favorite_color: Color32,
}

impl IndicatorDialogConfig {
    /// Create config from theme semantic tokens
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.semantic.ui;
        Self {
            width: 750.0,
            height: 600.0,
            bg_color: ui.panel_bg,
            sidebar_color: ui.panel_bg_secondary,
            text_color: ui.text,
            muted_color: ui.text_muted,
            hover_color: ui.btn_bg_hover,
            sel_color: ui.accent,
            favorite_color: ui.warning, // Gold/yellow for favorites
        }
    }
}

impl Default for IndicatorDialogConfig {
    fn default() -> Self {
        // Default uses standard theme
        Self::from_theme(&Theme::dark())
    }
}
