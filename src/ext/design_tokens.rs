//! Unified design token access via the [`HasDesignTokens`] trait.
//!
//! This module provides the primary way to access theme-aware colors from
//! within egui widgets. Import [`HasDesignTokens`] and call methods like
//! `ui.bullish_color()`, `ui.chart_bg()`, or `ui.panel_fill()` to get
//! colors that automatically match the current theme.
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::ext::HasDesignTokens;
//!
//! fn render_overlay(ui: &mut egui::Ui) {
//!     let bg = ui.chart_bg();           // Dark chart background
//!     let up = ui.bullish_color();      // Green for up candles
//!     let down = ui.bearish_color();    // Red for down candles
//!     let grid = ui.chart_grid();       // Subtle grid lines
//!     let warn = ui.warning_color();    // Orange for alerts
//! }
//! ```

use egui::{Color32, Context, Ui};

use crate::theme::context::{ThemeContextExt, ThemeData, ThemeUiExt};

/// Runtime design token facade providing unified access to theme-aware colors.
///
/// Created from an [`egui::Context`] via [`DesignTokens::from_context`], this
/// struct wraps the current [`ThemeData`] and exposes typed color accessors
/// for UI elements, chart areas, trading signals, and status indicators.
///
/// Most users should prefer the [`HasDesignTokens`] trait methods on `Ui`/`Context`
/// rather than constructing this directly.
#[derive(Clone, Debug)]
pub struct DesignTokens {
    /// The underlying theme data
    theme_data: ThemeData,
}

impl DesignTokens {
    /// Create design tokens from egui context
    pub fn from_context(ctx: &Context) -> Self {
        Self {
            theme_data: ctx.theme_data(),
        }
    }

    // UI Colors
    /// Panel background color
    pub fn panel_fill(&self) -> Color32 {
        self.theme_data.ui_panel_bg
    }

    /// Secondary panel background
    pub fn panel_fill_secondary(&self) -> Color32 {
        self.theme_data.ui_panel_bg_secondary
    }

    /// Primary text color
    pub fn text_primary(&self) -> Color32 {
        self.theme_data.ui_text
    }

    /// Secondary text color
    pub fn text_secondary(&self) -> Color32 {
        self.theme_data.ui_text_secondary
    }

    /// Muted text color
    pub fn text_muted(&self) -> Color32 {
        self.theme_data.ui_text_muted
    }

    /// Border color
    pub fn border(&self) -> Color32 {
        self.theme_data.ui_border
    }

    /// Subtle border color
    pub fn border_subtle(&self) -> Color32 {
        self.theme_data.ui_border_subtle
    }

    /// Accent/brand color
    pub fn accent(&self) -> Color32 {
        self.theme_data.ui_accent
    }

    /// Icon color
    pub fn icon(&self) -> Color32 {
        self.theme_data.ui_icon
    }

    /// Icon hover color
    pub fn icon_hover(&self) -> Color32 {
        self.theme_data.ui_icon_hover
    }

    /// Icon active color
    pub fn icon_active(&self) -> Color32 {
        self.theme_data.ui_icon_active
    }

    // Chart Colors
    /// Chart background
    pub fn chart_bg(&self) -> Color32 {
        self.theme_data.chart_bg
    }

    /// Chart axis background
    pub fn chart_axis_bg(&self) -> Color32 {
        self.theme_data.chart_bg_axis
    }

    /// Chart grid lines
    pub fn chart_grid(&self) -> Color32 {
        self.theme_data.chart_grid
    }

    /// Chart text
    pub fn chart_text(&self) -> Color32 {
        self.theme_data.chart_text
    }

    /// Crosshair color
    pub fn crosshair(&self) -> Color32 {
        self.theme_data.chart_crosshair
    }

    // Trading Colors
    /// Bullish (up) color
    pub fn bullish(&self) -> Color32 {
        self.theme_data.bullish
    }

    /// Bearish (down) color
    pub fn bearish(&self) -> Color32 {
        self.theme_data.bearish
    }

    /// Volume bullish color
    pub fn volume_bullish(&self) -> Color32 {
        self.theme_data.volume_bullish
    }

    /// Volume bearish color
    pub fn volume_bearish(&self) -> Color32 {
        self.theme_data.volume_bearish
    }

    // Status Colors
    /// Warning color
    pub fn warning(&self) -> Color32 {
        self.theme_data.warning
    }

    /// Success color
    pub fn success(&self) -> Color32 {
        self.theme_data.success
    }

    // Interactive state colors
    /// Hover background color
    pub fn hover_bg(&self) -> Color32 {
        self.theme_data.ui_button_bg_hover
    }

    /// Active background color
    pub fn active_bg(&self) -> Color32 {
        self.theme_data.ui_button_bg_active
    }

    // Footprint chart colors
    /// Point of Control highlight color
    pub fn poc_highlight(&self) -> Color32 {
        self.theme_data.footprint_poc
    }

    /// Value Area fill color
    pub fn value_area_fill(&self) -> Color32 {
        self.theme_data.footprint_value_area
    }

    /// Imbalance buy indicator color
    pub fn imbalance_buy(&self) -> Color32 {
        self.theme_data.footprint_imbalance_buy
    }

    /// Imbalance sell indicator color
    pub fn imbalance_sell(&self) -> Color32 {
        self.theme_data.footprint_imbalance_sell
    }

    // Mode checks
    /// Is dark UI mode
    pub fn is_dark_ui(&self) -> bool {
        self.theme_data.is_dark_ui
    }

    /// Is dark chart mode
    pub fn is_dark_chart(&self) -> bool {
        self.theme_data.is_dark_chart
    }
}

/// Trait for types that can provide design tokens
///
/// Implemented for `egui::Context` and `egui::Ui` to provide convenient
/// access to design tokens from any egui context.
pub trait HasDesignTokens {
    /// Get the design tokens
    fn design_tokens(&self) -> DesignTokens;

    // Convenience methods that delegate to DesignTokens
    /// Get panel fill color
    fn panel_fill(&self) -> Color32 {
        self.design_tokens().panel_fill()
    }

    /// Get primary text color
    fn text_color(&self) -> Color32 {
        self.design_tokens().text_primary()
    }

    /// Get secondary text color
    fn text_secondary(&self) -> Color32 {
        self.design_tokens().text_secondary()
    }

    /// Get muted text color
    fn text_muted(&self) -> Color32 {
        self.design_tokens().text_muted()
    }

    /// Get accent color
    fn accent_color(&self) -> Color32 {
        self.design_tokens().accent()
    }

    /// Get border color
    fn border_color(&self) -> Color32 {
        self.design_tokens().border()
    }

    /// Get bullish (up) color
    fn bullish_color(&self) -> Color32 {
        self.design_tokens().bullish()
    }

    /// Get bearish (down) color
    fn bearish_color(&self) -> Color32 {
        self.design_tokens().bearish()
    }

    /// Get chart background color
    fn chart_bg(&self) -> Color32 {
        self.design_tokens().chart_bg()
    }

    /// Get warning color
    fn warning_color(&self) -> Color32 {
        self.design_tokens().warning()
    }

    /// Get success color
    fn success_color(&self) -> Color32 {
        self.design_tokens().success()
    }

    /// Get chart grid color
    fn chart_grid(&self) -> Color32 {
        self.design_tokens().chart_grid()
    }

    /// Get icon color
    fn icon_color(&self) -> Color32 {
        self.design_tokens().icon()
    }

    /// Get icon hover color
    fn icon_hover(&self) -> Color32 {
        self.design_tokens().icon_hover()
    }

    /// Get icon active color
    fn icon_active(&self) -> Color32 {
        self.design_tokens().icon_active()
    }

    /// Get hover background color
    fn hover_bg(&self) -> Color32 {
        self.design_tokens().hover_bg()
    }

    /// Get active background color
    fn active_bg(&self) -> Color32 {
        self.design_tokens().active_bg()
    }

    /// Get Point of Control highlight color
    fn poc_highlight(&self) -> Color32 {
        self.design_tokens().poc_highlight()
    }

    /// Get Value Area fill color
    fn value_area_fill(&self) -> Color32 {
        self.design_tokens().value_area_fill()
    }

    /// Get imbalance buy color
    fn imbalance_buy(&self) -> Color32 {
        self.design_tokens().imbalance_buy()
    }

    /// Get imbalance sell color
    fn imbalance_sell(&self) -> Color32 {
        self.design_tokens().imbalance_sell()
    }
}

impl HasDesignTokens for Context {
    fn design_tokens(&self) -> DesignTokens {
        DesignTokens::from_context(self)
    }
}

impl HasDesignTokens for Ui {
    fn design_tokens(&self) -> DesignTokens {
        DesignTokens::from_context(self.ctx())
    }

    // Override for Ui to use ThemeUiExt methods
    fn panel_fill(&self) -> Color32 {
        ThemeUiExt::theme_panel_bg(self)
    }

    fn bullish_color(&self) -> Color32 {
        ThemeUiExt::theme_bullish(self)
    }

    fn bearish_color(&self) -> Color32 {
        ThemeUiExt::theme_bearish(self)
    }

    fn chart_bg(&self) -> Color32 {
        ThemeUiExt::theme_chart_bg(self)
    }

    fn warning_color(&self) -> Color32 {
        ThemeUiExt::theme_warning(self)
    }

    fn success_color(&self) -> Color32 {
        ThemeUiExt::theme_success(self)
    }

    fn hover_bg(&self) -> Color32 {
        ThemeUiExt::theme_hover_bg(self)
    }

    fn active_bg(&self) -> Color32 {
        ThemeUiExt::theme_active_bg(self)
    }

    fn poc_highlight(&self) -> Color32 {
        ThemeUiExt::theme_poc_highlight(self)
    }

    fn value_area_fill(&self) -> Color32 {
        ThemeUiExt::theme_value_area_fill(self)
    }

    fn imbalance_buy(&self) -> Color32 {
        ThemeUiExt::theme_imbalance_buy(self)
    }

    fn imbalance_sell(&self) -> Color32 {
        ThemeUiExt::theme_imbalance_sell(self)
    }
}
