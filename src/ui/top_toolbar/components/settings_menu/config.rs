//! Configuration types for the settings dialog and quick settings menu.

use crate::model::ChartType;
use crate::scales::PriceScaleMode;
use egui::Color32;

// ============================================================================
// Configuration
// ============================================================================

/// Dialog layout configuration (width, height, colors)
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

// ============================================================================
// Simple Settings (backward compatibility)
// ============================================================================

/// Simplified chart settings for the quick settings popup menu
#[derive(Debug, Clone)]
pub struct ChartSettings {
    /// Price scale transformation.
    pub scale_mode: PriceScaleMode,
    /// Chart representation selected in the menu.
    pub chart_type: ChartType,
    /// Whether to show horizontal grid.
    pub show_horizontal_grid: bool,
    /// Whether to show vertical grid.
    pub show_vertical_grid: bool,
    /// Whether to show right axis.
    pub show_right_axis: bool,
    /// Whether to show left axis.
    pub show_left_axis: bool,
    /// Whether the price-to-bar ratio is locked.
    pub lock_scale: bool,
    /// Whether autoscaling considers only the main price series.
    pub scale_price_chart_only: bool,
    /// Whether to show symbol labels.
    pub show_symbol_labels: bool,
    /// Whether to show symbol last value.
    pub show_symbol_last_val: bool,
    /// Whether to show symbol prev close.
    pub show_symbol_prev_close: bool,
    /// Whether to show indicator labels.
    pub show_indicator_labels: bool,
    /// Whether to show indicator last value.
    pub show_indicator_last_val: bool,
    /// Whether price axis labels should avoid overlapping.
    pub no_overlapping_labels: bool,
    /// Whether to show countdown.
    pub show_countdown: bool,
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            scale_mode: PriceScaleMode::Normal,
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
