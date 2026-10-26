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
/// Each field groups colors for a specific domain: UI chrome, chart area,
/// indicators, drawing tools, alerts, modals, etc.
#[derive(Clone, Debug)]
pub struct SemanticTokens {
    /// Neutral engine fallback; UI chrome reads the applied theme.
    pub ui: UiSemanticTokens,
    /// Chart area colors (background, grid, candles, crosshair, volume).
    pub chart: ChartSemanticTokens,
    /// Brand accent colors.
    pub brand: BrandSemanticTokens,
    pub status: StatusSemanticTokens,
    pub footprint: FootprintSemanticTokens,
    pub indicators: IndicatorSemanticTokens,
    pub drawings: DrawingSemanticTokens,
    /// Extended flat-access colors - replaces old theme::ui, theme::chart modules
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
    pub bg: Color32,
    pub bg_axis: Color32,
    pub grid_line: Color32,
    pub grid_line_major: Color32,
    pub axis_text: Color32,
    pub axis_text_secondary: Color32,
    pub crosshair_line: Color32,
    pub crosshair_label_bg: Color32,
    pub crosshair_label_text: Color32,
    pub bullish: Color32,
    pub bearish: Color32,
    pub volume_up_alpha: u8,
    pub volume_down_alpha: u8,
    pub watermark_alpha: u8,
    pub selection_text: Color32,
}

/// Brand accent colors (consistent across light/dark themes).
#[derive(Clone, Debug)]
pub struct BrandSemanticTokens {
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_active: Color32,
}

/// Status and feedback colors (error, info, disabled, caution).
#[derive(Clone, Debug)]
pub struct StatusSemanticTokens {
    pub error: Color32,
    pub caution: Color32,
}

/// Footprint (order flow) chart colors.
#[derive(Clone, Debug)]
pub struct FootprintSemanticTokens {
    pub poc: Color32,
    pub value_area: Color32,
    pub imbalance_buy: Color32,
    pub imbalance_sell: Color32,
}

/// TPO (Market Profile) chart colors
#[derive(Clone, Debug)]
pub struct TpoSemanticTokens {
    /// Point of Control line color
    pub poc: Color32,
    /// Value Area background color
    pub value_area: Color32,
    /// Initial Balance bracket color
    pub initial_balance: Color32,
    /// Single prints highlight color
    pub single_print: Color32,
    /// Session separator line color
    pub session_separator: Color32,
    /// Default TPO letter color
    pub letter_default: Color32,
    /// TPO grid line color
    pub grid: Color32,
    /// Period colors for time-based coloring (12 colors)
    pub period_1: Color32,
    pub period_2: Color32,
    pub period_3: Color32,
    pub period_4: Color32,
    pub period_5: Color32,
    pub period_6: Color32,
    pub period_7: Color32,
    pub period_8: Color32,
    pub period_9: Color32,
    pub period_10: Color32,
    pub period_11: Color32,
    pub period_12: Color32,
}

/// Technical indicator colors (moving averages, Bollinger Bands, RSI, MACD, VWAP).
#[derive(Clone, Debug)]
pub struct IndicatorSemanticTokens {
    /// Generic moving average line color.
    pub ma: Color32,
    /// Exponential moving average line color.
    pub ema: Color32,
    /// Simple moving average line color.
    pub sma: Color32,
    /// Bollinger Band upper line color.
    pub bb_upper: Color32,
    /// Bollinger Band middle (basis) line color.
    pub bb_middle: Color32,
    /// Bollinger Band lower line color.
    pub bb_lower: Color32,
    /// RSI line color.
    pub rsi: Color32,
    /// MACD main line color.
    pub macd_line: Color32,
    /// MACD signal line color.
    pub macd_signal: Color32,
    /// VWAP line color.
    pub vwap: Color32,
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
    /// Active/pressed state background.
    pub bg_active: Color32,
    /// Secondary text color.
    pub text_secondary: Color32,
    /// Muted/dimmed text color.
    pub text_muted: Color32,
    /// Accent/brand color.
    pub accent: Color32,
    /// Accent color on hover.
    pub accent_hover: Color32,
    /// Accent color when active/pressed.
    pub accent_active: Color32,
    /// Success status color.
    pub success: Color32,
    /// Success color (lighter variant).
    pub success_light: Color32,
    /// Success color (darker variant).
    pub success_dark: Color32,
    /// Success color (darkest variant).
    pub success_darker: Color32,
    /// Error status color.
    pub error: Color32,
    /// Error color (darker variant).
    pub error_dark: Color32,
    /// Error color (darkest variant).
    pub error_darker: Color32,
    /// Warning status color.
    pub warning: Color32,
    /// Caution status color (softer warning).
    pub caution: Color32,
    /// Info status color.
    pub info: Color32,
    /// Info color (lighter variant).
    pub info_light: Color32,
    /// Disabled/inactive state color.
    pub disabled: Color32,
    /// Material Design purple.
    pub purple: Color32,
    /// Material Design deep purple.
    pub deep_purple: Color32,
    /// Material Design cyan.
    pub cyan: Color32,
    /// Material Design teal.
    pub teal: Color32,
    /// Material Design indigo.
    pub indigo: Color32,
    /// Material Design pink.
    pub pink: Color32,
    /// Material Design brown.
    pub brown: Color32,
    /// Material Design deep orange.
    pub deep_orange: Color32,
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
