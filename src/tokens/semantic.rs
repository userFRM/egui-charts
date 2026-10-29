//! Semantic color tokens -- domain-organized color definitions.
//!
//! These structs hold the color values of [`DESIGN_TOKENS`](super::DESIGN_TOKENS).
//! They supply component defaults and fallbacks. Theme presets define their
//! own palettes in the [`theme`](crate::theme) module.
//!
//! For most use cases, prefer reading colors from the applied theme
//! ([`ThemeExt::active_theme`](crate::theme::ThemeExt::active_theme)) rather than
//! from these structs directly.

use egui::Color32;

/// Complete set of semantic color tokens, organized by domain.
///
/// This is the top-level color token structure within [`DesignTokens`](super::DesignTokens).
/// Each field groups defaults and fallbacks for a specific domain, including chart
/// rendering and drawing tools. UI chrome reads the applied theme.
#[derive(Clone, Debug)]
pub struct SemanticTokens {
    /// Neutral engine fallback; UI chrome reads the applied theme.
    pub ui: UiSemanticTokens,
    /// Chart area colors (background, grid, candles, crosshair, volume).
    pub chart: ChartSemanticTokens,
    /// Brand accent colors.
    pub brand: BrandSemanticTokens,
    /// Status defaults.
    pub status: StatusSemanticTokens,
    /// Footprint defaults.
    pub footprint: FootprintSemanticTokens,
    /// Indicators defaults.
    pub indicators: IndicatorSemanticTokens,
    /// Drawings defaults.
    pub drawings: DrawingSemanticTokens,
    /// Additional component color defaults.
    pub extended: ExtendedSemanticTokens,
    /// TPO (Market Profile) chart colors
    pub tpo: TpoSemanticTokens,
}

/// Neutral fallback still used by volume bubbles. UI chrome reads the applied theme.
#[derive(Clone, Debug)]
pub struct UiSemanticTokens {
    /// Neutral default retained for engine volume bubbles.
    pub text_muted_dark: Color32,
    /// Tooltip outline fallback when no theme has been applied.
    pub border_dark: Color32,
}

/// Chart area color tokens for default and fallback rendering.
///
/// Defines colors for the chart background, grid lines, axis text, crosshair,
/// candle/bar colors (bullish/bearish), volume bar alpha, and watermark.
#[derive(Clone, Debug)]
pub struct ChartSemanticTokens {
    /// Background color.
    pub bg: Color32,
    /// Background axis color.
    pub bg_axis: Color32,
    /// Grid line color.
    pub grid_line: Color32,
    /// Grid line major color.
    pub grid_line_major: Color32,
    /// Axis text color.
    pub axis_text: Color32,
    /// Axis text secondary color.
    pub axis_text_secondary: Color32,
    /// Crosshair line color.
    pub crosshair_line: Color32,
    /// Crosshair label background color.
    pub crosshair_label_bg: Color32,
    /// Crosshair label text color.
    pub crosshair_label_text: Color32,
    /// Bullish color.
    pub bullish: Color32,
    /// Bearish color.
    pub bearish: Color32,
    /// Volume up alpha, from transparent (0) to opaque (255).
    pub volume_up_alpha: u8,
    /// Volume down alpha, from transparent (0) to opaque (255).
    pub volume_down_alpha: u8,
    /// Watermark alpha, from transparent (0) to opaque (255).
    pub watermark_alpha: u8,
    /// Selection text color.
    pub selection_text: Color32,
}

/// Brand accent colors (consistent across light/dark themes).
#[derive(Clone, Debug)]
pub struct BrandSemanticTokens {
    /// Accent color.
    pub accent: Color32,
    /// Accent hover color.
    pub accent_hover: Color32,
    /// Accent active color.
    pub accent_active: Color32,
}

/// Status and feedback colors (error, info, disabled, caution).
#[derive(Clone, Debug)]
pub struct StatusSemanticTokens {
    /// Error color.
    pub error: Color32,
    /// Caution color.
    pub caution: Color32,
}

/// Footprint (order flow) chart colors.
#[derive(Clone, Debug)]
pub struct FootprintSemanticTokens {
    /// Point of control color.
    pub poc: Color32,
    /// Value area color.
    pub value_area: Color32,
    /// Imbalance buy color.
    pub imbalance_buy: Color32,
    /// Imbalance sell color.
    pub imbalance_sell: Color32,
}

/// TPO (Market Profile) chart colors
#[derive(Clone, Debug)]
pub struct TpoSemanticTokens {
    /// Point of Control row color
    pub poc: Color32,
    /// Value Area tone for value-area coloring
    pub value_area: Color32,
    /// Initial Balance bar color
    pub initial_balance: Color32,
    /// Single prints highlight color
    pub single_print: Color32,
    /// Session separator line color
    pub session_separator: Color32,
    /// Default TPO letter color
    pub letter_default: Color32,
    /// TPO grid line color
    pub grid: Color32,
    /// First stop of the gradient across a session's periods.
    pub period_1: Color32,
    /// Period 2 color.
    pub period_2: Color32,
    /// Period 3 color.
    pub period_3: Color32,
    /// Period 4 color.
    pub period_4: Color32,
    /// Period 5 color.
    pub period_5: Color32,
    /// Period 6 color.
    pub period_6: Color32,
    /// Period 7 color.
    pub period_7: Color32,
    /// Period 8 color.
    pub period_8: Color32,
    /// Period 9 color.
    pub period_9: Color32,
    /// Period 10 color.
    pub period_10: Color32,
    /// Period 11 color.
    pub period_11: Color32,
    /// Period 12 color.
    pub period_12: Color32,
}

/// Legacy moving average colours still used by drawing defaults. Studies use `studies::palette`.
#[derive(Clone, Debug)]
pub struct IndicatorSemanticTokens {
    /// Generic moving average line color.
    pub ma: Color32,
    /// Exponential moving average line color.
    pub ema: Color32,
}

/// Drawing label and Fibonacci level colors.
#[derive(Clone, Debug)]
pub struct DrawingSemanticTokens {
    /// Drawing label text color.
    pub label_text: Color32,
    /// Fibonacci retracement 50% level color.
    pub fib_50: Color32,
    /// Fibonacci retracement 100% level color.
    pub fib_100: Color32,
}

/// Extended semantic tokens providing flat access to commonly used colors.
///
/// This is a convenience collection that gathers the most frequently accessed
/// colors across domains (UI backgrounds, borders, text, accent, status,
/// Material Design palette colors, chart colors, and trading colors) into
/// a single flat namespace.
#[derive(Clone, Debug)]
pub struct ExtendedSemanticTokens {
    /// Accent/brand color.
    pub accent: Color32,
    /// Success status color.
    pub success: Color32,
    /// Error status color.
    pub error: Color32,
    /// Caution status color (softer warning).
    pub caution: Color32,
    /// Info status color.
    pub info: Color32,
    /// Disabled/inactive state color.
    pub disabled: Color32,
    /// Material Design purple.
    pub purple: Color32,
    /// Gray color.
    pub gray: Color32,
    /// Light gray color.
    pub light_gray: Color32,
    /// Favorite/star gold color.
    pub favorite_gold: Color32,
    /// Chart area background.
    pub chart_bg: Color32,
    /// Chart axis background.
    pub chart_axis_bg: Color32,
    /// Chart tooltip background.
    pub chart_tooltip_bg: Color32,
    /// Chart text color.
    pub chart_text: Color32,
    /// Chart secondary text color.
    pub chart_text_secondary: Color32,
    /// Chart muted text color.
    pub chart_text_muted: Color32,
    /// Chart crosshair label background.
    pub chart_crosshair_label_bg: Color32,
    /// Bullish (up) trading color.
    pub bullish: Color32,
    /// Bearish (down) trading color.
    pub bearish: Color32,
}
