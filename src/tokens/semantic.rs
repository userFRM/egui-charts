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
    /// UI chrome colors (panels, buttons, text, borders) with light/dark variants.
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
    /// Button variant colors.
    pub buttons: ButtonSemanticTokens,
    /// Additional component color defaults.
    pub extended: ExtendedSemanticTokens,
    /// Alert component colors
    pub alert: AlertSemanticTokens,
    /// Modal component colors
    pub modal: ModalSemanticTokens,
    /// Command palette colors
    pub command_palette: CommandPaletteSemanticTokens,
    /// TPO (Market Profile) chart colors
    pub tpo: TpoSemanticTokens,
}

/// UI chrome colors for component defaults and fallbacks.
///
/// Theme presets own their complete palettes; these tokens retain only the
/// panel, text, button, icon, border and accent values read by components.
#[derive(Clone, Debug)]
pub struct UiSemanticTokens {
    /// Panel background light color.
    pub panel_bg_light: Color32,
    /// Panel background secondary light color.
    pub panel_bg_secondary_light: Color32,
    /// Text light color.
    pub text_light: Color32,
    /// Text secondary light color.
    pub text_secondary_light: Color32,
    /// Text muted light color.
    pub text_muted_light: Color32,
    /// Border light color.
    pub border_light: Color32,
    /// Border subtle light color.
    pub border_subtle_light: Color32,
    /// Text muted dark color.
    pub text_muted_dark: Color32,
    /// Border dark color.
    pub border_dark: Color32,
    /// Icon light color.
    pub icon_light: Color32,
    /// Icon hover light color.
    pub icon_hover_light: Color32,
    /// Icon dark color.
    pub icon_dark: Color32,
    /// Icon hover dark color.
    pub icon_hover_dark: Color32,
    /// Icon active color.
    pub icon_active: Color32,
    /// Button background light color.
    pub btn_bg_light: Color32,
    /// Button background hover light color.
    pub btn_bg_hover_light: Color32,
    /// Button background active light color.
    pub btn_bg_active_light: Color32,
    /// Accent color.
    pub accent: Color32,
    /// Accent hover color.
    pub accent_hover: Color32,
    /// Success light color.
    pub success_light: Color32,
    /// Warning light color.
    pub warning_light: Color32,
    /// Info light color.
    pub info_light: Color32,
    /// Success dark color.
    pub success_dark: Color32,
    /// Error dark color.
    pub error_dark: Color32,
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
    /// Period colors used cyclically for time-based coloring.
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

/// Button variant colors for primary, secondary and danger buttons.
///
/// Background and foreground defaults, with hover and active colors for danger buttons.
#[derive(Clone, Debug)]
pub struct ButtonSemanticTokens {
    /// Primary button background (high emphasis, accent-colored).
    pub primary_bg: Color32,
    /// Primary button foreground (text/icon, typically inverse of background).
    pub primary_fg: Color32,
    /// Secondary button background (medium emphasis).
    pub secondary_bg: Color32,
    /// Secondary button foreground.
    pub secondary_fg: Color32,
    /// Danger button background (destructive actions).
    pub danger_bg: Color32,
    /// Danger button background when hovered.
    pub danger_bg_hover: Color32,
    /// Danger button background when pressed.
    pub danger_bg_active: Color32,
    /// Danger button foreground.
    pub danger_fg: Color32,
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

/// Colors for inline alert/banner components (info, success, warning, error).
#[derive(Clone, Debug)]
pub struct AlertSemanticTokens {
    /// Info alert background.
    pub info_bg: Color32,
    /// Info alert border.
    pub info_border: Color32,
    /// Info alert icon color.
    pub info_icon: Color32,
    /// Success alert background.
    pub success_bg: Color32,
    /// Success alert border.
    pub success_border: Color32,
    /// Success alert icon color.
    pub success_icon: Color32,
    /// Warning alert background.
    pub warning_bg: Color32,
    /// Warning alert border.
    pub warning_border: Color32,
    /// Warning alert icon color.
    pub warning_icon: Color32,
    /// Error alert background.
    pub error_bg: Color32,
    /// Error alert border.
    pub error_border: Color32,
    /// Error alert icon color.
    pub error_icon: Color32,
}

/// Colors for modal dialog overlays.
#[derive(Clone, Debug)]
pub struct ModalSemanticTokens {
    /// Semi-transparent backdrop overlay behind the modal.
    pub overlay_bg: Color32,
}

/// Colors for the command palette (Ctrl+K / Cmd+K) popup.
#[derive(Clone, Debug)]
pub struct CommandPaletteSemanticTokens {
    /// Result item background when hovered.
    pub item_hover: Color32,
    /// Result item background when selected (keyboard navigation).
    pub item_selected: Color32,
    /// Keyboard shortcut text color.
    pub shortcut_text: Color32,
}
