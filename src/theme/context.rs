//! egui Context Extensions - Zero-cost theme access
//!
//! This module provides extension traits for `egui::Context` and `egui::Ui`
//! to access theme colors without passing the theme explicitly everywhere.
//!
//! # Usage
//!
//! ```ignore
//! use egui_charts::theme::context::ThemeExt;
//!
//! fn my_widget(ui: &mut egui::Ui) {
//!     let bg = ui.theme_ui().panel_bg;
//!     let chart_bg = ui.theme_chart().bg;
//! }
//! ```

use egui::{Color32, Context, Ui};

use crate::tokens::DESIGN_TOKENS;

// ============================================================================
// THEME ACCESS VIA EGUI VISUALS (stored in ctx.data())
// ============================================================================

/// Cached theme data stored in egui context
#[derive(Clone, Debug)]
pub struct ThemeData {
    /// Panel background color.
    pub ui_panel_bg: Color32,
    /// Secondary panel background color.
    pub ui_panel_bg_secondary: Color32,
    /// Button background in resting state.
    pub ui_button_bg: Color32,
    /// Button background when hovered.
    pub ui_button_bg_hover: Color32,
    /// Button background when pressed.
    pub ui_button_bg_active: Color32,
    /// Primary text color.
    pub ui_text: Color32,
    /// Secondary text color.
    pub ui_text_secondary: Color32,
    /// Muted/dimmed text color.
    pub ui_text_muted: Color32,
    /// Icon color in resting state.
    pub ui_icon: Color32,
    /// Icon color when hovered.
    pub ui_icon_hover: Color32,
    /// Icon color when active/selected.
    pub ui_icon_active: Color32,
    /// Standard border color.
    pub ui_border: Color32,
    /// Subtle/secondary border color.
    pub ui_border_subtle: Color32,
    /// Accent/brand color.
    pub ui_accent: Color32,
    /// Accent color on hover.
    pub ui_accent_hover: Color32,
    /// Page/gap background (shows through between panels).
    pub page_bg: Color32,
    /// Chart area background.
    pub chart_bg: Color32,
    /// Chart axis gutter background.
    pub chart_bg_axis: Color32,
    /// Chart minor grid line color.
    pub chart_grid: Color32,
    /// Chart major grid line color.
    pub chart_grid_major: Color32,
    /// Chart axis label text color.
    pub chart_text: Color32,
    /// Chart secondary text color.
    pub chart_text_secondary: Color32,
    /// Crosshair line color.
    pub chart_crosshair: Color32,
    /// Crosshair value label background.
    pub chart_crosshair_label_bg: Color32,
    /// Crosshair value label text color.
    pub chart_crosshair_label_text: Color32,
    /// Bullish (up) candle/bar color.
    pub bullish: Color32,
    /// Bearish (down) candle/bar color.
    pub bearish: Color32,
    /// Bullish volume bar color (with alpha).
    pub volume_bullish: Color32,
    /// Bearish volume bar color (with alpha).
    pub volume_bearish: Color32,
    /// Footprint chart Point of Control highlight color.
    pub footprint_poc: Color32,
    /// Footprint chart Value Area fill color.
    pub footprint_value_area: Color32,
    /// Footprint chart buy imbalance indicator color.
    pub footprint_imbalance_buy: Color32,
    /// Footprint chart sell imbalance indicator color.
    pub footprint_imbalance_sell: Color32,
    /// TPO Point of Control line color.
    pub tpo_poc: Color32,
    /// TPO Value Area background color.
    pub tpo_value_area: Color32,
    /// TPO Initial Balance bracket color.
    pub tpo_initial_balance: Color32,
    /// TPO single print highlight color.
    pub tpo_single_print: Color32,
    /// TPO session separator line color.
    pub tpo_session_separator: Color32,
    /// TPO default letter color.
    pub tpo_letter_default: Color32,
    /// Warning status color (orange, for alerts and cautions).
    pub warning: Color32,
    /// Success status color (green, for confirmations).
    pub success: Color32,
    /// Whether the UI is in dark mode.
    pub is_dark_ui: bool,
    /// Whether the chart area is in dark mode.
    pub is_dark_chart: bool,
}

impl Default for ThemeData {
    fn default() -> Self {
        // Defaults - sourced from DESIGN_TOKENS
        let tokens = &DESIGN_TOKENS.semantic;

        Self {
            ui_panel_bg: tokens.ui.panel_bg_light,
            ui_panel_bg_secondary: tokens.ui.panel_bg_secondary_light,
            ui_button_bg: tokens.ui.btn_bg_light,
            ui_button_bg_hover: tokens.ui.btn_bg_hover_light,
            ui_button_bg_active: tokens.ui.btn_bg_active_light,
            ui_text: tokens.ui.text_light,
            ui_text_secondary: tokens.ui.text_secondary_light,
            ui_text_muted: tokens.ui.text_muted_light,
            ui_icon: tokens.ui.icon_light,
            ui_icon_hover: tokens.ui.icon_hover_light,
            ui_icon_active: tokens.ui.icon_active,
            ui_border: tokens.ui.border_light,
            ui_border_subtle: tokens.ui.border_subtle_light,
            ui_accent: tokens.ui.accent,
            ui_accent_hover: tokens.ui.accent_hover,

            // Default: page_bg is chart_bg (dark) for light UI mode
            page_bg: tokens.chart.bg,

            chart_bg: tokens.chart.bg,
            chart_bg_axis: tokens.chart.bg_axis,
            chart_grid: tokens.chart.grid_line,
            chart_grid_major: tokens.chart.grid_line_major,
            chart_text: tokens.chart.axis_text,
            chart_text_secondary: tokens.chart.axis_text_secondary,
            chart_crosshair: tokens.chart.crosshair_line,
            chart_crosshair_label_bg: tokens.chart.crosshair_label_bg,
            chart_crosshair_label_text: tokens.chart.crosshair_label_text,

            bullish: tokens.chart.bullish,
            bearish: tokens.chart.bearish,
            volume_bullish: {
                let [r, g, b, _] = tokens.chart.bullish.to_array();
                Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_up_alpha)
            },
            volume_bearish: {
                let [r, g, b, _] = tokens.chart.bearish.to_array();
                Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_down_alpha)
            },

            // Footprint colors - from DESIGN_TOKENS
            footprint_poc: tokens.footprint.poc,
            footprint_value_area: tokens.footprint.value_area,
            footprint_imbalance_buy: tokens.footprint.imbalance_buy,
            footprint_imbalance_sell: tokens.footprint.imbalance_sell,

            // TPO colors
            tpo_poc: tokens.tpo.poc,
            tpo_value_area: tokens.tpo.value_area,
            tpo_initial_balance: tokens.tpo.initial_balance,
            tpo_single_print: tokens.tpo.single_print,
            tpo_session_separator: tokens.tpo.session_separator,
            tpo_letter_default: tokens.tpo.letter_default,

            warning: tokens.ui.warning,
            success: tokens.ui.success,

            is_dark_ui: false,
            is_dark_chart: true,
        }
    }
}

impl ThemeData {
    /// Create from a Theme
    pub fn from_theme(theme: &super::Theme) -> Self {
        Self {
            ui_panel_bg: theme.semantic.ui.panel_bg,
            ui_panel_bg_secondary: theme.semantic.ui.panel_bg_secondary,
            ui_button_bg: theme.semantic.ui.btn_bg,
            ui_button_bg_hover: theme.semantic.ui.btn_bg_hover,
            ui_button_bg_active: theme.semantic.ui.btn_bg_active,
            ui_text: theme.semantic.ui.text,
            ui_text_secondary: theme.semantic.ui.text_secondary,
            ui_text_muted: theme.semantic.ui.text_muted,
            ui_icon: theme.semantic.ui.icon,
            ui_icon_hover: theme.semantic.ui.icon_hover,
            ui_icon_active: theme.semantic.ui.icon_active,
            ui_border: theme.semantic.ui.border,
            ui_border_subtle: theme.semantic.ui.border_subtle,
            ui_accent: theme.semantic.ui.accent,
            ui_accent_hover: theme.semantic.ui.accent_hover,

            // Page background for gaps between panels
            // In dark UI mode, gaps are LIGHTER than panels for contrast
            // In light UI mode, gaps are DARKER (chart_bg) than white panels
            page_bg: if theme.preset.is_dark_ui() {
                // Gaps are lighter than panels in dark mode - use secondary panel bg
                theme.semantic.ui.panel_bg_secondary
            } else {
                // Use chart background for light UI
                theme.semantic.chart.bg
            },

            chart_bg: theme.semantic.chart.bg,
            chart_bg_axis: theme.semantic.chart.bg_axis,
            chart_grid: theme.semantic.chart.grid_line,
            chart_grid_major: theme.semantic.chart.grid_line_major,
            chart_text: theme.semantic.chart.axis_text,
            chart_text_secondary: theme.semantic.chart.axis_text_secondary,
            chart_crosshair: theme.semantic.chart.crosshair_line,
            chart_crosshair_label_bg: theme.semantic.chart.crosshair_label_bg,
            chart_crosshair_label_text: theme.semantic.chart.crosshair_label_text,

            bullish: theme.semantic.chart.candle_up,
            bearish: theme.semantic.chart.candle_down,
            volume_bullish: theme.semantic.chart.volume_up,
            volume_bearish: theme.semantic.chart.volume_down,

            // Footprint colors - from DESIGN_TOKENS
            footprint_poc: DESIGN_TOKENS.semantic.footprint.poc,
            footprint_value_area: DESIGN_TOKENS.semantic.footprint.value_area,
            footprint_imbalance_buy: DESIGN_TOKENS.semantic.footprint.imbalance_buy,
            footprint_imbalance_sell: DESIGN_TOKENS.semantic.footprint.imbalance_sell,

            // TPO colors - from DESIGN_TOKENS
            tpo_poc: DESIGN_TOKENS.semantic.tpo.poc,
            tpo_value_area: DESIGN_TOKENS.semantic.tpo.value_area,
            tpo_initial_balance: DESIGN_TOKENS.semantic.tpo.initial_balance,
            tpo_single_print: DESIGN_TOKENS.semantic.tpo.single_print,
            tpo_session_separator: DESIGN_TOKENS.semantic.tpo.session_separator,
            tpo_letter_default: DESIGN_TOKENS.semantic.tpo.letter_default,

            // Status colors from semantic tokens (properly flows through theme system)
            warning: theme.semantic.ui.warning,
            success: theme.semantic.ui.success,

            is_dark_ui: theme.preset.is_dark_ui(),
            is_dark_chart: theme.preset.is_dark_chart(),
        }
    }

    /// Bullish color with custom alpha
    #[inline]
    pub fn bullish_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.bullish.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    /// Bearish color with custom alpha
    #[inline]
    pub fn bearish_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.bearish.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }
}

// ============================================================================
// EXTENSION TRAITS
// ============================================================================

/// Extension trait for egui::Context to access theme data
pub trait ThemeContextExt {
    /// Store theme data in context (call once per frame, typically in app.update())
    fn set_theme_data(&self, data: ThemeData);

    /// Get theme data from context
    fn theme_data(&self) -> ThemeData;

    /// Check if current UI is dark mode
    fn is_dark_ui(&self) -> bool;

    /// Check if chart is dark mode
    fn is_dark_chart(&self) -> bool;
}

impl ThemeContextExt for Context {
    fn set_theme_data(&self, data: ThemeData) {
        self.data_mut(|d| d.insert_temp(egui::Id::NULL.with("theme_data"), data));
    }

    fn theme_data(&self) -> ThemeData {
        self.data(|d| d.get_temp::<ThemeData>(egui::Id::NULL.with("theme_data")))
            .unwrap_or_default()
    }

    fn is_dark_ui(&self) -> bool {
        self.theme_data().is_dark_ui
    }

    fn is_dark_chart(&self) -> bool {
        self.theme_data().is_dark_chart
    }
}

/// Extension trait for egui::Ui to access theme colors
pub trait ThemeUiExt {
    /// Get UI panel background
    fn theme_panel_bg(&self) -> Color32;

    /// Get UI hover background
    fn theme_hover_bg(&self) -> Color32;

    /// Get UI active background
    fn theme_active_bg(&self) -> Color32;

    /// Get UI text color
    fn theme_text(&self) -> Color32;

    /// Get UI secondary text color
    fn theme_text_secondary(&self) -> Color32;

    /// Get UI icon color
    fn theme_icon(&self) -> Color32;

    /// Get UI icon hover color
    fn theme_icon_hover(&self) -> Color32;

    /// Get UI icon active color (accent)
    fn theme_icon_active(&self) -> Color32;

    /// Get UI border color
    fn theme_border(&self) -> Color32;

    /// Get UI accent color
    fn theme_accent(&self) -> Color32;

    /// Get chart background
    fn theme_chart_bg(&self) -> Color32;

    /// Get chart grid color
    fn theme_chart_grid(&self) -> Color32;

    /// Get chart text color
    fn theme_chart_text(&self) -> Color32;

    /// Get bullish (up) color
    fn theme_bullish(&self) -> Color32;

    /// Get bearish (down) color
    fn theme_bearish(&self) -> Color32;

    /// Get warning color (orange - for alerts, cautions)
    fn theme_warning(&self) -> Color32;

    /// Get success color (green - for active states, confirmations)
    fn theme_success(&self) -> Color32;

    /// Get footprint POC highlight color (yellow)
    fn theme_poc_highlight(&self) -> Color32;

    /// Get footprint value area fill color (semi-transparent)
    fn theme_value_area_fill(&self) -> Color32;

    /// Get footprint buy imbalance color
    fn theme_imbalance_buy(&self) -> Color32;

    /// Get footprint sell imbalance color
    fn theme_imbalance_sell(&self) -> Color32;

    /// Get full theme data
    fn theme_data(&self) -> ThemeData;
}

impl ThemeUiExt for Ui {
    #[inline]
    fn theme_panel_bg(&self) -> Color32 {
        self.style().visuals.panel_fill
    }

    #[inline]
    fn theme_hover_bg(&self) -> Color32 {
        self.style().visuals.widgets.hovered.bg_fill
    }

    #[inline]
    fn theme_active_bg(&self) -> Color32 {
        self.style().visuals.widgets.active.bg_fill
    }

    #[inline]
    fn theme_text(&self) -> Color32 {
        self.style().visuals.widgets.active.fg_stroke.color
    }

    #[inline]
    fn theme_text_secondary(&self) -> Color32 {
        self.style().visuals.widgets.noninteractive.fg_stroke.color
    }

    #[inline]
    fn theme_icon(&self) -> Color32 {
        self.style().visuals.widgets.noninteractive.fg_stroke.color
    }

    #[inline]
    fn theme_icon_hover(&self) -> Color32 {
        self.style().visuals.widgets.hovered.fg_stroke.color
    }

    #[inline]
    fn theme_icon_active(&self) -> Color32 {
        self.style().visuals.selection.bg_fill
    }

    #[inline]
    fn theme_border(&self) -> Color32 {
        self.style().visuals.widgets.noninteractive.bg_stroke.color
    }

    #[inline]
    fn theme_accent(&self) -> Color32 {
        self.style().visuals.selection.bg_fill
    }

    #[inline]
    fn theme_chart_bg(&self) -> Color32 {
        self.ctx().theme_data().chart_bg
    }

    #[inline]
    fn theme_chart_grid(&self) -> Color32 {
        self.ctx().theme_data().chart_grid
    }

    #[inline]
    fn theme_chart_text(&self) -> Color32 {
        self.ctx().theme_data().chart_text
    }

    #[inline]
    fn theme_bullish(&self) -> Color32 {
        self.ctx().theme_data().bullish
    }

    #[inline]
    fn theme_bearish(&self) -> Color32 {
        self.ctx().theme_data().bearish
    }

    #[inline]
    fn theme_warning(&self) -> Color32 {
        self.ctx().theme_data().warning
    }

    #[inline]
    fn theme_success(&self) -> Color32 {
        self.ctx().theme_data().success
    }

    #[inline]
    fn theme_poc_highlight(&self) -> Color32 {
        self.ctx().theme_data().footprint_poc
    }

    #[inline]
    fn theme_value_area_fill(&self) -> Color32 {
        self.ctx().theme_data().footprint_value_area
    }

    #[inline]
    fn theme_imbalance_buy(&self) -> Color32 {
        self.ctx().theme_data().footprint_imbalance_buy
    }

    #[inline]
    fn theme_imbalance_sell(&self) -> Color32 {
        self.ctx().theme_data().footprint_imbalance_sell
    }

    fn theme_data(&self) -> ThemeData {
        self.ctx().theme_data()
    }
}

// ============================================================================
// CONVENIENCE FUNCTIONS (for use without extension traits)
// ============================================================================

/// Get icon color based on dark/light mode (backward compatible helper)
pub fn get_icon_color(ctx: &Context) -> Color32 {
    let tokens = &DESIGN_TOKENS.semantic.ui;
    if ctx.is_dark_ui() {
        tokens.icon_dark
    } else {
        tokens.icon_light
    }
}

/// Get icon hover color based on dark/light mode
pub fn get_icon_hover_color(ctx: &Context) -> Color32 {
    let tokens = &DESIGN_TOKENS.semantic.ui;
    if ctx.is_dark_ui() {
        tokens.icon_hover_dark
    } else {
        tokens.icon_hover_light
    }
}

/// Get icon active color (accent blue - consistent across themes)
pub fn get_icon_active_color() -> Color32 {
    DESIGN_TOKENS.semantic.ui.icon_active
}
