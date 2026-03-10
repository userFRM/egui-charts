//! Configuration types for the settings dialog and quick settings menu.

use super::types::ScaleMode;
use crate::model::ChartType;
use egui::Color32;

// ============================================================================
// Configuration
// ============================================================================

/// Dialog layout configuration (width, height, colors)
pub struct SettingsDialogConfig {
    pub width: f32,
    pub height: f32,
    pub bg_color: Color32,
    pub tab_bg_color: Color32,
    pub tab_active_color: Color32,
    pub text_color: Color32,
    pub muted_text_color: Color32,
}

impl Default for SettingsDialogConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            width: 450.0,
            height: 500.0,
            bg_color: Color32::TRANSPARENT,
            tab_bg_color: Color32::TRANSPARENT,
            tab_active_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_text_color: Color32::TRANSPARENT,
        }
    }
}

// ============================================================================
// Simple Settings (backward compatibility)
// ============================================================================

/// Simplified chart settings for the quick settings popup menu
#[derive(Debug, Clone)]
pub struct ChartSettings {
    pub scale_mode: ScaleMode,
    pub chart_type: ChartType,
    pub show_horizontal_grid: bool,
    pub show_vertical_grid: bool,
    pub show_right_axis: bool,
    pub show_left_axis: bool,
    pub lock_scale: bool,
    pub scale_price_chart_only: bool,
    pub show_symbol_labels: bool,
    pub show_symbol_last_val: bool,
    pub show_symbol_prev_close: bool,
    pub show_indicator_labels: bool,
    pub show_indicator_last_val: bool,
    pub no_overlapping_labels: bool,
    pub show_countdown: bool,
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            scale_mode: ScaleMode::Auto,
            chart_type: ChartType::Candles,
            show_horizontal_grid: true,
            show_vertical_grid: true,
            show_right_axis: true,
            show_left_axis: false,
            lock_scale: false,
            scale_price_chart_only: false,
            show_symbol_labels: true,
            show_symbol_last_val: true,
            show_symbol_prev_close: false,
            show_indicator_labels: true,
            show_indicator_last_val: true,
            no_overlapping_labels: true,
            show_countdown: false,
        }
    }
}

// ============================================================================
// Quick Settings Menu (popup from button)
// ============================================================================
