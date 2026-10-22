//! Unified Theme System for egui-charts
//!
//! This module is the SINGLE SOURCE OF TRUTH for all styling in the application.
//! It provides a comprehensive design system with:
//!
//! - **Design Tokens**: Raw primitives from `ui_kit/tokens/DESIGN_TOKENS` (colors, spacing, typography)
//! - **Semantic Tokens**: Meaning-based color access resolved for light/dark mode
//! - **Component Styles**: Pre-computed styles for common components
//! - **Theme Manager**: Runtime switching and persistence
//! - **Context Extensions**: Zero-cost theme access from egui::Context and egui::Ui
//!
//! # Architecture
//!
//! ```text
//! DESIGN_TOKENS (RON-based, compile-time)
//!              ↓
//!          ThemePreset (selects light/dark mode)
//!              ↓
//!          SemanticTokens (resolved colors)
//!              ↓
//!          ComponentStyles (applied styles)
//!              ↓
//!           Theme (complete)
//!              ↓
//!      apply_to_egui() + set_theme_data()
//!              ↓
//!    Accessible via HasDesignTokens / ThemeContextExt
//! ```
//!
//! # Quick Start
//!
//! ```ignore
//! use egui_charts::theme::{Theme, ThemePreset, apply_to_egui};
//! use egui_charts::ui_kit::ext::HasDesignTokens;
//!
//! // In your app's update() function:
//! fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//!     let theme = Theme::from_preset(ThemePreset::Classic);
//!     apply_to_egui(ctx, &theme);
//!
//!     // In your widgets, access colors via HasDesignTokens:
//!     egui::CentralPanel::default().show(ctx, |ui| {
//!         let bg = ui.panel_fill();
//!         let bullish = ui.bullish_color();
//!     });
//! }
//! ```

// Sub-modules
pub mod components;
pub mod context;
pub mod manager;
pub mod presets;
pub mod semantic;

// Re-exports for convenient access
pub use components::{
    BtnStyle, CandleStyle, ChartStyle, ComponentStyles, ContextMenuStyle, DialogStyle, InputStyle,
    MenuStyle, PanelStyle, SettingsDialogStyle, ToolbarStyle, VolumeStyle,
};
pub use context::{ThemeContextExt, ThemeData, ThemeUiExt};
#[cfg(feature = "ui")]
pub(crate) use context::{ThemeDerived, follow_active_theme, theme_derived};
pub use manager::ThemeManager;
pub use presets::ThemePreset;
pub use semantic::{
    ChartSemanticTokens, DrawingSemanticTokens, IndicatorSemanticTokens, SemanticTokens,
    UiSemanticTokens,
};

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

// ============================================================================
// MAIN THEME STRUCT
// ============================================================================

/// Complete theme containing all styling information
///
/// This is the main entry point for the theme system. Create a Theme from a
/// preset or build one manually, then apply it with `apply_to_egui()`.
///
/// # Usage
///
/// Access colors via semantic tokens:
/// ```ignore
/// // UI colors
/// theme.semantic.ui.panel_bg
/// theme.semantic.ui.accent
///
/// // Chart colors
/// theme.semantic.chart.bg
/// theme.semantic.chart.candle_up
/// theme.semantic.chart.grid_line
///
/// // Or use convenience accessors
/// theme.chart_bg()
/// theme.bullish()
/// ```
#[derive(Clone, Debug)]
pub struct Theme {
    /// Which preset this theme is based on
    pub preset: ThemePreset,
    /// Semantic tokens (meaning-based colors) - resolved for light/dark mode
    pub semantic: SemanticTokens,
    /// Pre-computed component styles
    pub components: ComponentStyles,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_preset(ThemePreset::Classic)
    }
}

impl Theme {
    /// Create a theme from a preset
    pub fn from_preset(preset: ThemePreset) -> Self {
        // Build semantic tokens from DESIGN_TOKENS based on preset's light/dark mode
        let semantic =
            SemanticTokens::from_design_tokens(preset.is_dark_ui(), preset.is_dark_chart());

        // Build component styles from semantic tokens
        let components = ComponentStyles::from_semantic(&semantic);

        Self {
            preset,
            semantic,
            components,
        }
    }

    /// Classic theme (light UI chrome + dark chart)
    pub fn classic() -> Self {
        Self::from_preset(ThemePreset::Classic)
    }

    /// Dark theme
    pub fn dark() -> Self {
        Self::from_preset(ThemePreset::Dark)
    }

    /// Light theme
    pub fn light() -> Self {
        Self::from_preset(ThemePreset::Light)
    }

    /// Midnight theme
    pub fn midnight() -> Self {
        Self::from_preset(ThemePreset::Midnight)
    }

    /// High contrast theme
    pub fn high_contrast() -> Self {
        Self::from_preset(ThemePreset::HighContrast)
    }

    /// Get the theme name
    #[inline]
    pub fn name(&self) -> &'static str {
        self.preset.display_name()
    }

    /// Whether this theme has dark UI chrome
    #[inline]
    pub fn is_dark_ui(&self) -> bool {
        self.preset.is_dark_ui()
    }

    /// Whether this theme has dark chart background
    #[inline]
    pub fn is_dark_chart(&self) -> bool {
        self.preset.is_dark_chart()
    }

    // =========================================================================
    // CONVENIENCE ACCESSORS (shorthand for common semantic tokens)
    // =========================================================================

    /// Chart background color
    #[inline]
    pub fn background(&self) -> Color32 {
        self.semantic.chart.bg
    }

    /// Grid line color
    #[inline]
    pub fn grid(&self) -> Color32 {
        self.semantic.chart.grid_line
    }

    /// Chart text color
    #[inline]
    pub fn text(&self) -> Color32 {
        self.semantic.chart.axis_text
    }

    /// Bullish/up color
    #[inline]
    pub fn bullish(&self) -> Color32 {
        self.semantic.chart.candle_up
    }

    /// Bearish/down color
    #[inline]
    pub fn bearish(&self) -> Color32 {
        self.semantic.chart.candle_down
    }

    /// Crosshair color
    #[inline]
    pub fn crosshair(&self) -> Color32 {
        self.semantic.chart.crosshair_line
    }

    /// Volume bullish color
    #[inline]
    pub fn volume_bullish(&self) -> Color32 {
        self.semantic.chart.volume_up
    }

    /// Volume bearish color
    #[inline]
    pub fn volume_bearish(&self) -> Color32 {
        self.semantic.chart.volume_down
    }

    /// Axis background color
    #[inline]
    pub fn axis_background(&self) -> Color32 {
        self.semantic.chart.bg_axis
    }

    // =========================================================================
    // FOOTPRINT CHART COLOR ACCESSORS
    // =========================================================================

    /// Footprint Point of Control (POC) highlight color (yellow)
    #[inline]
    pub fn footprint_poc(&self) -> Color32 {
        DESIGN_TOKENS.semantic.footprint.poc
    }

    /// Footprint value area fill color (semi-transparent amber)
    #[inline]
    pub fn footprint_value_area(&self) -> Color32 {
        DESIGN_TOKENS.semantic.footprint.value_area
    }

    /// Bullish color with custom alpha (for footprint buy imbalances)
    #[inline]
    pub fn bullish_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.semantic.chart.candle_up.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    /// Bearish color with custom alpha (for footprint sell imbalances)
    #[inline]
    pub fn bearish_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.semantic.chart.candle_down.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    // =========================================================================
    // TPO (MARKET PROFILE) COLOR ACCESSORS
    // =========================================================================

    /// TPO Point of Control line color
    #[inline]
    pub fn tpo_poc(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.poc
    }

    /// TPO Value Area background color
    #[inline]
    pub fn tpo_value_area(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.value_area
    }

    /// TPO Initial Balance bracket color
    #[inline]
    pub fn tpo_initial_balance(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.initial_balance
    }

    /// TPO single print highlight color
    #[inline]
    pub fn tpo_single_print(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.single_print
    }

    /// TPO session separator line color
    #[inline]
    pub fn tpo_session_separator(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.session_separator
    }

    /// TPO default letter color
    #[inline]
    pub fn tpo_letter_default(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.letter_default
    }

    /// TPO opening range marker color
    #[inline]
    pub fn tpo_opening_range(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.opening_range
    }

    /// TPO grid line color
    #[inline]
    pub fn tpo_grid(&self) -> Color32 {
        DESIGN_TOKENS.semantic.tpo.grid
    }

    /// TPO period color by index (0-based, cycles through 12 period colors)
    pub fn tpo_period_color(&self, period_idx: usize) -> Color32 {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        match period_idx % 12 {
            0 => tpo.period_1,
            1 => tpo.period_2,
            2 => tpo.period_3,
            3 => tpo.period_4,
            4 => tpo.period_5,
            5 => tpo.period_6,
            6 => tpo.period_7,
            7 => tpo.period_8,
            8 => tpo.period_9,
            9 => tpo.period_10,
            10 => tpo.period_11,
            _ => tpo.period_12,
        }
    }

    /// Warning/cumulative delta line color (orange)
    #[inline]
    pub fn warning(&self) -> Color32 {
        self.semantic.ui.warning
    }

    /// Chart text color with alpha
    #[inline]
    pub fn text_alpha(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.semantic.chart.axis_text.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    /// Apply theme to chart config
    pub fn apply_to_config(
        &self,
        mut config: crate::config::ChartConfig,
    ) -> crate::config::ChartConfig {
        config.background_color = self.semantic.chart.bg;
        config.grid_color = self.semantic.chart.grid_line;
        config.text_color = self.semantic.chart.axis_text;
        config.bullish_color = self.semantic.chart.candle_up;
        config.bearish_color = self.semantic.chart.candle_down;
        config.bullish_border_color = Some(self.semantic.chart.candle_up_border);
        config.bearish_border_color = Some(self.semantic.chart.candle_down_border);
        config.bullish_wick_color = Some(self.semantic.chart.candle_up_wick);
        config.bearish_wick_color = Some(self.semantic.chart.candle_down_wick);
        config
    }
}

// ============================================================================
// APPLY THEME TO EGUI
// ============================================================================

/// Apply theme to egui's visual system
///
/// This should be called at the start of each frame (in your app's update() function).
/// It configures egui's built-in styling to match the theme.
///
/// After calling this, also call `ctx.set_theme_data(ThemeData::from_theme(&theme))`
/// to make chart colors available via the extension traits.
pub fn apply_to_egui(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = if theme.is_dark_ui() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let ui = &theme.semantic.ui;

    // Window/panel backgrounds
    visuals.panel_fill = ui.panel_bg;
    // Windows, popups, menus and tooltips sit one elevation step above panels.
    visuals.window_fill = ui.panel_bg_floating;
    visuals.extreme_bg_color = ui.panel_bg;
    // faint_bg_color must match panel_fill to avoid thin line at window top
    // Previously used a different color for "visible gaps" but this caused artifacts
    visuals.faint_bg_color = ui.panel_bg;

    // Widget backgrounds
    visuals.widgets.noninteractive.bg_fill = ui.panel_bg;
    visuals.widgets.inactive.bg_fill = ui.panel_bg;
    visuals.widgets.hovered.bg_fill = ui.btn_bg_hover;
    visuals.widgets.active.bg_fill = ui.btn_bg_active;
    visuals.widgets.open.bg_fill = ui.btn_bg_active;

    // Widget borders
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border);
    // egui outlines checkboxes, radios, text inputs and combo boxes with these strokes.
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.accent);

    // Text/foreground colors
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text_secondary);
    visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text_secondary);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);

    // Selection: egui paints selected labels in `selection.stroke`, on top of `bg_fill`, and
    // outlines a focused text input with `selection.stroke`. Dark themes use white on the accent,
    // light themes accent text on a pale accent fill.
    visuals.selection.bg_fill = ui.selection_bg;
    visuals.selection.stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.selection_text);

    // egui fades weak text by alpha by default, which drops it below a readable contrast.
    visuals.weak_text_color = Some(ui.text_muted);
    visuals.error_fg_color = ui.error;
    visuals.warn_fg_color = ui.warning;

    // Hyperlinks
    visuals.hyperlink_color = ui.accent;

    // Floating chrome: hairline border, small radius and a tight shadow that stays close to
    // the surface instead of smearing over neighbouring toolbars.
    let shadow_alpha = if theme.is_dark_ui() { 96 } else { 32 };
    visuals.window_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border);
    visuals.window_corner_radius = DESIGN_TOKENS.rounding.dialog.into();
    visuals.menu_corner_radius = DESIGN_TOKENS.rounding.sm.into();
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha / 2),
    };

    ctx.set_visuals(visuals);

    let spacing = &DESIGN_TOKENS.spacing;
    let mut style = (*ctx.style()).clone();
    style.spacing.window_margin = egui::Margin::same(spacing.xl as i8);
    style.spacing.menu_margin = egui::Margin::symmetric(spacing.lg as i8, spacing.sm as i8);
    style.spacing.item_spacing = egui::vec2(spacing.lg, spacing.sm);
    ctx.set_style(style);

    // Store the theme so components resolve their colours from it while painting.
    ctx.set_theme_data(ThemeData::from_theme(theme));
    context::set_active_theme(ctx, theme.clone());
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Get all predefined theme presets
pub fn all_presets() -> &'static [ThemePreset] {
    ThemePreset::all()
}

/// Get all predefined themes
pub fn all_themes() -> Vec<Theme> {
    ThemePreset::all()
        .iter()
        .map(|p| Theme::from_preset(*p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 contrast ratio of `fg` painted over the opaque `bg`.
    fn contrast(fg: Color32, bg: Color32) -> f32 {
        let luminance = |c: Color32| {
            let c = egui::Rgba::from(c);
            0.2126 * c.r() + 0.7152 * c.g() + 0.0722 * c.b()
        };
        let (a, b) = (luminance(bg.blend(fg)), luminance(bg));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_and_control_outlines_are_readable_in_every_preset() {
        let mut failures = Vec::new();
        for theme in all_themes() {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            let visuals = ctx.style().visuals.clone();
            let ui = &theme.semantic.ui;
            let mut focus_ring = Color32::TRANSPARENT;
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    focus_ring = crate::styles::focus::focus_ring_color(ui);
                });
            });
            let mut check = |role: &str, fg: Color32, surface: &str, bg: Color32, min: f32| {
                let ratio = contrast(fg, bg);
                if ratio < min {
                    failures.push(format!(
                        "{}: {role} on {surface} is {ratio:.2}:1, below {min}:1",
                        theme.name()
                    ));
                }
            };
            check(
                "selected text",
                visuals.selection.stroke.color,
                "selection",
                visuals.selection.bg_fill,
                4.5,
            );
            for (fill, bg) in [
                ("accent", ui.accent),
                ("hovered accent", ui.accent_hover),
                ("pressed accent", ui.accent_active),
                ("danger", DESIGN_TOKENS.semantic.buttons.danger_bg),
                (
                    "hovered danger",
                    DESIGN_TOKENS.semantic.buttons.danger_bg_hover,
                ),
                (
                    "pressed danger",
                    DESIGN_TOKENS.semantic.buttons.danger_bg_active,
                ),
            ] {
                check("on-accent text", ui.accent_text, fill, bg, 4.5);
            }
            // Hover must read on a panel, and pressed must sit further from it than hover.
            let (panel, hovered) = (visuals.panel_fill, visuals.widgets.hovered.bg_fill);
            check("hover fill", hovered, "panels", panel, 1.2);
            let pressed_min = contrast(hovered, panel) + 0.05;
            check(
                "pressed fill",
                visuals.widgets.active.bg_fill,
                "panels",
                panel,
                pressed_min,
            );
            // egui outlines a focused text input with the selection stroke.
            check(
                "focused input outline",
                visuals.selection.stroke.color,
                "inputs",
                visuals.text_edit_bg_color(),
                3.0,
            );
            for (surface, bg) in [
                ("panels", visuals.panel_fill),
                ("windows", visuals.window_fill),
            ] {
                check("text", ui.text, surface, bg, 4.5);
                check("label text", visuals.text_color(), surface, bg, 4.5);
                check("muted text", ui.text_muted, surface, bg, 4.5);
                check("weak text", visuals.weak_text_color(), surface, bg, 4.5);
                check("error text", visuals.error_fg_color, surface, bg, 4.5);
                check("warning text", visuals.warn_fg_color, surface, bg, 4.5);
                check(
                    "control outline",
                    visuals.widgets.inactive.bg_stroke.color,
                    surface,
                    bg,
                    3.0,
                );
                check("keyboard focus ring", focus_ring, surface, bg, 3.0);
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
