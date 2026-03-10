//! Semantic Tokens - Meaning-based color access
//!
//! Semantic tokens map raw palette colors to their intended USE in the application.
//! This is the layer that components actually consume.
//!
//! # Why Semantic Tokens?
//!
//! Instead of:
//! ```ignore
//! let color = DESIGN_TOKENS.semantic.ui.panel_bg_light;  // Which one to use?
//! ```
//!
//! You write:
//! ```ignore
//! let color = theme.semantic.ui.panel_bg;  // Already resolved for current theme!
//! ```
//!
//! # Architecture
//!
//! This module builds theme-resolved semantic tokens from `DESIGN_TOKENS`,
//! which contains both light and dark variants. Based on the `ThemePreset`'s
//! `is_dark_ui()` and `is_dark_chart()` flags, we select the appropriate colors.

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

// ============================================================================
// SEMANTIC UI TOKENS
// ============================================================================

/// Semantic tokens for UI chrome (toolbars, panels, menus)
/// These are resolved based on the theme's light/dark mode.
#[derive(Clone, Copy, Debug)]
pub struct UiSemanticTokens {
    // Panel backgrounds
    pub panel_bg: Color32,
    pub panel_bg_secondary: Color32,
    pub panel_bg_floating: Color32,

    // Toolbar
    pub toolbar_bg: Color32,
    pub toolbar_separator: Color32,

    // Button states
    pub btn_bg: Color32,
    pub btn_bg_hover: Color32,
    pub btn_bg_active: Color32,
    pub btn_bg_disabled: Color32,

    // Text
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_disabled: Color32,
    pub text_link: Color32,

    // Icons
    pub icon: Color32,
    pub icon_hover: Color32,
    pub icon_active: Color32,
    pub icon_disabled: Color32,

    // Borders
    pub border: Color32,
    pub border_subtle: Color32,
    pub border_focus: Color32,

    // Selection
    pub selection_bg: Color32,
    pub selection_text: Color32,

    // Accent (for primary actions)
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_active: Color32,
    pub accent_text: Color32,

    // Status colors
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub info: Color32,
}

impl UiSemanticTokens {
    /// Build UI semantic tokens from DESIGN_TOKENS based on dark mode flag
    pub fn from_design_tokens(is_dark_ui: bool) -> Self {
        let tokens = &DESIGN_TOKENS.semantic;

        if is_dark_ui {
            Self {
                panel_bg: tokens.ui.panel_bg_dark,
                panel_bg_secondary: tokens.ui.panel_bg_secondary_dark,
                panel_bg_floating: tokens.ui.panel_bg_dark,

                toolbar_bg: tokens.ui.panel_bg_dark,
                toolbar_separator: tokens.ui.border_subtle_dark,

                btn_bg: tokens.ui.btn_bg_dark,
                btn_bg_hover: tokens.ui.btn_bg_hover_dark,
                btn_bg_active: tokens.ui.btn_bg_active_dark,
                btn_bg_disabled: tokens.ui.panel_bg_dark,

                text: tokens.ui.text_dark,
                text_secondary: tokens.ui.text_secondary_dark,
                text_muted: tokens.ui.text_muted_dark,
                text_disabled: tokens.ui.text_muted_dark,
                text_link: tokens.ui.accent,

                icon: tokens.ui.icon_dark,
                icon_hover: tokens.ui.icon_hover_dark,
                icon_active: tokens.ui.icon_active,
                icon_disabled: tokens.ui.text_muted_dark,

                border: tokens.ui.border_dark,
                border_subtle: tokens.ui.border_subtle_dark,
                border_focus: tokens.brand.accent,

                selection_bg: tokens.brand.accent,
                selection_text: tokens.chart.selection_text,

                accent: tokens.brand.accent,
                accent_hover: tokens.brand.accent_hover,
                accent_active: tokens.buttons.primary_bg_active,
                accent_text: tokens.chart.selection_text,

                success: tokens.ui.success,
                warning: tokens.ui.warning,
                error: tokens.status.error,
                info: tokens.status.info,
            }
        } else {
            Self {
                panel_bg: tokens.ui.panel_bg_light,
                panel_bg_secondary: tokens.ui.panel_bg_secondary_light,
                panel_bg_floating: tokens.ui.panel_bg_light,

                toolbar_bg: tokens.ui.panel_bg_light,
                toolbar_separator: tokens.ui.border_subtle_light,

                btn_bg: tokens.ui.btn_bg_light,
                btn_bg_hover: tokens.ui.btn_bg_hover_light,
                btn_bg_active: tokens.ui.btn_bg_active_light,
                btn_bg_disabled: tokens.ui.panel_bg_secondary_light,

                text: tokens.ui.text_light,
                text_secondary: tokens.ui.text_secondary_light,
                text_muted: tokens.ui.text_muted_light,
                text_disabled: tokens.ui.text_muted_light,
                text_link: tokens.ui.accent,

                icon: tokens.ui.icon_light,
                icon_hover: tokens.ui.icon_hover_light,
                icon_active: tokens.ui.icon_active,
                icon_disabled: tokens.ui.text_muted_light,

                border: tokens.ui.border_light,
                border_subtle: tokens.ui.border_subtle_light,
                border_focus: tokens.brand.accent,

                selection_bg: tokens.brand.accent,
                selection_text: tokens.chart.selection_text,

                accent: tokens.brand.accent,
                accent_hover: tokens.brand.accent_hover,
                accent_active: tokens.buttons.primary_bg_active,
                accent_text: tokens.chart.selection_text,

                success: tokens.ui.success,
                warning: tokens.ui.warning,
                error: tokens.status.error,
                info: tokens.status.info,
            }
        }
    }
}

// ============================================================================
// SEMANTIC CHART TOKENS
// ============================================================================

/// Semantic tokens for chart area
#[derive(Clone, Copy, Debug)]
pub struct ChartSemanticTokens {
    // Backgrounds
    pub bg: Color32,
    pub bg_axis: Color32,
    pub bg_tooltip: Color32,
    pub bg_legend: Color32,
    pub bg_selection: Color32,

    // Grid
    pub grid_line: Color32,
    pub grid_line_major: Color32,

    // Axis
    pub axis_text: Color32,
    pub axis_text_secondary: Color32,

    // Crosshair
    pub crosshair_line: Color32,
    pub crosshair_label_bg: Color32,
    pub crosshair_label_text: Color32,

    // Candles/bars
    pub candle_up: Color32,
    pub candle_up_border: Color32,
    pub candle_up_wick: Color32,
    pub candle_down: Color32,
    pub candle_down_border: Color32,
    pub candle_down_wick: Color32,

    // Volume
    pub volume_up: Color32,
    pub volume_down: Color32,

    // Price line
    pub price_line: Color32,
    pub price_label_bg: Color32,
    pub price_label_text: Color32,

    // Watermark
    pub watermark: Color32,
}

impl ChartSemanticTokens {
    /// Build chart semantic tokens from DESIGN_TOKENS
    /// Note: Charts are almost always dark
    pub fn from_design_tokens(is_dark_chart: bool) -> Self {
        let tokens = &DESIGN_TOKENS.semantic;

        if is_dark_chart {
            Self {
                bg: tokens.chart.bg,
                bg_axis: tokens.chart.bg_axis,
                bg_tooltip: tokens.chart.crosshair_label_bg,
                bg_legend: tokens.chart.crosshair_label_bg,
                bg_selection: tokens.chart.bg_selection,

                grid_line: tokens.chart.grid_line,
                grid_line_major: tokens.chart.grid_line_major,

                axis_text: tokens.chart.axis_text,
                axis_text_secondary: tokens.chart.axis_text_secondary,

                crosshair_line: tokens.chart.crosshair_line,
                crosshair_label_bg: tokens.chart.crosshair_label_bg,
                crosshair_label_text: tokens.chart.crosshair_label_text,

                candle_up: tokens.chart.bullish,
                candle_up_border: tokens.chart.bullish,
                candle_up_wick: tokens.chart.bullish,
                candle_down: tokens.chart.bearish,
                candle_down_border: tokens.chart.bearish,
                candle_down_wick: tokens.chart.bearish,

                volume_up: {
                    let [r, g, b, _] = tokens.chart.bullish.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_up_alpha)
                },
                volume_down: {
                    let [r, g, b, _] = tokens.chart.bearish.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_down_alpha)
                },

                price_line: tokens.chart.axis_text,
                price_label_bg: tokens.chart.crosshair_label_bg,
                price_label_text: tokens.chart.crosshair_label_text,

                watermark: {
                    let [r, g, b, _] = tokens.chart.axis_text_secondary.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.watermark_alpha)
                },
            }
        } else {
            // Light chart (rare but supported)
            Self {
                bg: tokens.chart.bg_light,
                bg_axis: tokens.chart.bg_axis_light,
                bg_tooltip: tokens.ui.panel_bg_light,
                bg_legend: tokens.ui.panel_bg_light,
                bg_selection: tokens.chart.bg_selection_light,

                grid_line: tokens.chart.grid_line_light,
                grid_line_major: tokens.chart.grid_line_major_light,

                axis_text: tokens.ui.text_light,
                axis_text_secondary: tokens.ui.text_secondary_light,

                crosshair_line: tokens.chart.crosshair_line_light,
                crosshair_label_bg: tokens.ui.panel_bg_secondary_light,
                crosshair_label_text: tokens.ui.text_light,

                candle_up: tokens.chart.bullish,
                candle_up_border: tokens.chart.bullish,
                candle_up_wick: tokens.chart.bullish,
                candle_down: tokens.chart.bearish,
                candle_down_border: tokens.chart.bearish,
                candle_down_wick: tokens.chart.bearish,

                volume_up: {
                    let [r, g, b, _] = tokens.chart.bullish.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_up_alpha)
                },
                volume_down: {
                    let [r, g, b, _] = tokens.chart.bearish.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.volume_down_alpha)
                },

                price_line: tokens.ui.text_light,
                price_label_bg: tokens.ui.panel_bg_secondary_light,
                price_label_text: tokens.ui.text_light,

                watermark: {
                    let [r, g, b, _] = tokens.ui.text_muted_light.to_array();
                    Color32::from_rgba_unmultiplied(r, g, b, tokens.chart.watermark_alpha)
                },
            }
        }
    }

    /// Get bullish color with custom alpha
    #[inline]
    pub fn candle_up_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.candle_up.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    /// Get bearish color with custom alpha
    #[inline]
    pub fn candle_down_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.candle_down.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }
}

// ============================================================================
// SEMANTIC INDICATOR TOKENS
// ============================================================================

/// Semantic tokens for indicators
#[derive(Clone, Copy, Debug)]
pub struct IndicatorSemanticTokens {
    // Moving avgs
    pub ma_default: Color32,
    pub ema_default: Color32,
    pub sma_default: Color32,

    // Bollinger bands
    pub bb_upper: Color32,
    pub bb_middle: Color32,
    pub bb_lower: Color32,
    pub bb_fill: Color32,

    // Oscillators
    pub rsi_line: Color32,
    pub rsi_overbought: Color32,
    pub rsi_oversold: Color32,

    // MACD
    pub macd_line: Color32,
    pub macd_signal: Color32,
    pub macd_hist_positive: Color32,
    pub macd_hist_negative: Color32,

    // VWAP
    pub vwap: Color32,
}

impl IndicatorSemanticTokens {
    /// Build indicator tokens from DESIGN_TOKENS
    pub fn from_design_tokens() -> Self {
        let tokens = &DESIGN_TOKENS.semantic.indicators;

        Self {
            ma_default: tokens.ma,
            ema_default: tokens.ema,
            sma_default: tokens.sma,

            bb_upper: tokens.bb_upper,
            bb_middle: tokens.bb_middle,
            bb_lower: tokens.bb_lower,
            bb_fill: tokens.bb_fill,

            rsi_line: tokens.rsi,
            rsi_overbought: tokens.rsi_overbought,
            rsi_oversold: tokens.rsi_oversold,

            macd_line: tokens.macd_line,
            macd_signal: tokens.macd_signal,
            macd_hist_positive: tokens.macd_hist_pos,
            macd_hist_negative: tokens.macd_hist_neg,

            vwap: tokens.vwap,
        }
    }
}

// ============================================================================
// SEMANTIC DRAWING TOKENS
// ============================================================================

/// Semantic tokens for drawing tools
#[derive(Clone, Copy, Debug)]
pub struct DrawingSemanticTokens {
    // Lines
    pub line_default: Color32,
    pub line_hover: Color32,
    pub line_sel: Color32,

    // Handles
    pub handle: Color32,
    pub handle_hover: Color32,
    pub handle_active: Color32,

    // Fibonacci
    pub fib_0: Color32,
    pub fib_236: Color32,
    pub fib_382: Color32,
    pub fib_50: Color32,
    pub fib_618: Color32,
    pub fib_100: Color32,

    // Text/labels
    pub label_bg: Color32,
    pub label_text: Color32,
}

impl DrawingSemanticTokens {
    /// Build drawing tokens from DESIGN_TOKENS
    pub fn from_design_tokens() -> Self {
        let tokens = &DESIGN_TOKENS.semantic.drawings;

        Self {
            line_default: tokens.default_line,
            line_hover: tokens.line_hover,
            line_sel: tokens.line_selected,

            handle: tokens.handle,
            handle_hover: tokens.handle_hover,
            handle_active: tokens.handle_active,

            fib_0: tokens.fib_0,
            fib_236: tokens.fib_236,
            fib_382: tokens.fib_382,
            fib_50: tokens.fib_50,
            fib_618: tokens.fib_618,
            fib_100: tokens.fib_100,

            label_bg: tokens.label_bg,
            label_text: tokens.label_text,
        }
    }
}

// ============================================================================
// COMPLETE SEMANTIC TOKENS
// ============================================================================

/// Complete semantic token set - this is what components consume
#[derive(Clone, Copy, Debug)]
pub struct SemanticTokens {
    pub ui: UiSemanticTokens,
    pub chart: ChartSemanticTokens,
    pub indicators: IndicatorSemanticTokens,
    pub drawing: DrawingSemanticTokens,
}

impl SemanticTokens {
    /// Build complete semantic tokens from DESIGN_TOKENS based on theme flags
    pub fn from_design_tokens(is_dark_ui: bool, is_dark_chart: bool) -> Self {
        Self {
            ui: UiSemanticTokens::from_design_tokens(is_dark_ui),
            chart: ChartSemanticTokens::from_design_tokens(is_dark_chart),
            indicators: IndicatorSemanticTokens::from_design_tokens(),
            drawing: DrawingSemanticTokens::from_design_tokens(),
        }
    }
}
