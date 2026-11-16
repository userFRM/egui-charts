//! Layout configuration for the chart settings dialog.

use egui::Color32;

// ============================================================================
// Configuration
// ============================================================================

/// Dialog layout configuration (width, height, colors)
#[derive(Debug)]
pub struct SettingsDialogConfig {
    /// Width, in egui logical points.
    pub width: f32,
    /// Outer height override; zero sizes the dialog for the active tab.
    pub height: f32,
    /// Background color value; the settings dialog currently uses the supplied theme instead.
    pub bg_color: Color32,
    /// Tab background color value; the settings dialog currently uses the supplied theme instead.
    pub tab_bg_color: Color32,
    /// Tab active color value; the settings dialog currently uses the supplied theme instead.
    pub tab_active_color: Color32,
    /// Text color value; the settings dialog currently uses the supplied theme instead.
    pub text_color: Color32,
    /// Muted text color value; the settings dialog currently uses the supplied theme instead.
    pub muted_text_color: Color32,
}

impl Default for SettingsDialogConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            width: 752.0,
            height: 0.0,
            bg_color: Color32::TRANSPARENT,
            tab_bg_color: Color32::TRANSPARENT,
            tab_active_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_text_color: Color32::TRANSPARENT,
        }
    }
}
