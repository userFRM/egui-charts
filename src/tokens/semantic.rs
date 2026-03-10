//! Semantic color tokens -- domain-organized color definitions.
//!
//! These structs hold the raw color values loaded from `design_tokens.ron`.
//! They contain both light and dark variants where applicable (e.g.,
//! `panel_bg_light` / `panel_bg_dark`). The [`theme`](crate::theme) module
//! resolves these into single values based on the active theme preset.
//!
//! For most use cases, prefer accessing colors through the theme system
//! ([`HasDesignTokens`](crate::ext::HasDesignTokens) trait) rather than
//! reading from these structs directly.

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
    pub status: StatusSemanticTokens,
    pub footprint: FootprintSemanticTokens,
    pub indicators: IndicatorSemanticTokens,
    pub drawings: DrawingSemanticTokens,
    pub tiers: TierSemanticTokens,
    /// Button variant colors (rerun-inspired)
    pub buttons: ButtonSemanticTokens,
    /// List item colors (rerun-inspired)
    pub list_item: ListItemSemanticTokens,
    /// Extended flat-access colors - replaces old theme::ui, theme::chart modules
    pub extended: ExtendedSemanticTokens,
    /// Alert component colors
    pub alert: AlertSemanticTokens,
    /// Modal component colors
    pub modal: ModalSemanticTokens,
    /// Toast notification colors
    pub toast: ToastSemanticTokens,
    /// Command palette colors
    pub command_palette: CommandPaletteSemanticTokens,
    /// TPO (Market Profile) chart colors
    pub tpo: TpoSemanticTokens,
    /// Order book heatmap colors
    pub heatmap: HeatmapSemanticTokens,
}

/// UI chrome color tokens with light and dark mode variants.
///
/// Contains panel backgrounds, text colors, button states, icon colors,
/// borders, and accent colors for both light and dark UI modes. The
/// active variant is selected by [`theme::SemanticTokens`](crate::theme::SemanticTokens)
/// based on the theme preset.
#[derive(Clone, Debug)]
pub struct UiSemanticTokens {
    pub panel_bg_light: Color32,
    pub panel_bg_secondary_light: Color32,
    pub text_light: Color32,
    pub text_secondary_light: Color32,
    pub text_muted_light: Color32,
    pub border_light: Color32,
    pub border_subtle_light: Color32,
    pub panel_bg_dark: Color32,
    pub panel_bg_secondary_dark: Color32,
    pub text_dark: Color32,
    pub text_secondary_dark: Color32,
    pub text_muted_dark: Color32,
    pub border_dark: Color32,
    pub border_subtle_dark: Color32,
    pub icon_light: Color32,
    pub icon_hover_light: Color32,
    pub icon_dark: Color32,
    pub icon_hover_dark: Color32,
    pub icon_active: Color32,
    pub btn_bg_light: Color32,
    pub btn_bg_hover_light: Color32,
    pub btn_bg_active_light: Color32,
    pub btn_bg_dark: Color32,
    pub btn_bg_hover_dark: Color32,
    pub btn_bg_active_dark: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub warning: Color32,
    pub success: Color32,
}

/// Chart area color tokens (dark chart is default, light chart variants included).
///
/// Defines colors for the chart background, grid lines, axis text, crosshair,
/// candle/bar colors (bullish/bearish), volume bar alpha, and watermark.
/// Light chart variants (rare) are suffixed with `_light`.
#[derive(Clone, Debug)]
pub struct ChartSemanticTokens {
    pub bg: Color32,
    pub bg_axis: Color32,
    pub bg_selection: Color32,
    pub grid_line: Color32,
    pub grid_line_major: Color32,
    pub axis_text: Color32,
    pub axis_text_secondary: Color32,
    pub crosshair_line: Color32,
    pub crosshair_label_bg: Color32,
    pub crosshair_label_text: Color32,
    pub bullish: Color32,
    pub bearish: Color32,
    pub bullish_fill: Color32,
    pub bearish_fill: Color32,
    pub volume_up_alpha: u8,
    pub volume_down_alpha: u8,
    pub volume: Color32,
    pub session_break: Color32,
    pub watermark_alpha: u8,
    pub selection_text: Color32,
    // Light chart variants
    pub bg_light: Color32,
    pub bg_axis_light: Color32,
    pub bg_selection_light: Color32,
    pub grid_line_light: Color32,
    pub grid_line_major_light: Color32,
    pub crosshair_line_light: Color32,
}

/// Brand accent colors (consistent across light/dark themes).
#[derive(Clone, Debug)]
pub struct BrandSemanticTokens {
    pub accent: Color32,
    pub accent_hover: Color32,
}

/// Status and feedback colors (error, info, disabled, caution).
#[derive(Clone, Debug)]
pub struct StatusSemanticTokens {
    pub error: Color32,
    pub error_dark: Color32,
    pub error_darker: Color32,
    pub info: Color32,
    pub info_light: Color32,
    pub disabled: Color32,
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
    /// Opening range marker color
    pub opening_range: Color32,
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

/// Order book heatmap colors
#[derive(Clone, Debug)]
pub struct HeatmapSemanticTokens {
    /// Bid gradient colors (5 levels: darkest to brightest)
    pub bid_1: Color32,
    pub bid_2: Color32,
    pub bid_3: Color32,
    pub bid_4: Color32,
    pub bid_5: Color32,
    /// Ask gradient colors (5 levels: darkest to brightest)
    pub ask_1: Color32,
    pub ask_2: Color32,
    pub ask_3: Color32,
    pub ask_4: Color32,
    pub ask_5: Color32,
    /// Mid price line color
    pub mid_price: Color32,
    /// Cell border color
    pub border: Color32,
    /// Large order highlight border color
    pub large_order_border: Color32,
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
    /// Bollinger Band fill (between upper and lower) color.
    pub bb_fill: Color32,
    /// RSI line color.
    pub rsi: Color32,
    /// RSI overbought zone line color.
    pub rsi_overbought: Color32,
    /// RSI oversold zone line color.
    pub rsi_oversold: Color32,
    /// MACD main line color.
    pub macd_line: Color32,
    /// MACD signal line color.
    pub macd_signal: Color32,
    /// MACD histogram positive bar color.
    pub macd_hist_pos: Color32,
    /// MACD histogram negative bar color.
    pub macd_hist_neg: Color32,
    /// VWAP line color.
    pub vwap: Color32,
}

/// Drawing tool colors (lines, handles, Fibonacci levels, labels).
#[derive(Clone, Debug)]
pub struct DrawingSemanticTokens {
    /// Default drawing line color.
    pub default_line: Color32,
    /// Drag handle color in resting state.
    pub handle: Color32,
    /// Drag handle color when hovered.
    pub handle_hover: Color32,
    /// Drag handle color when pressed/dragging.
    pub handle_active: Color32,
    /// Line color when hovered (highlight).
    pub line_hover: Color32,
    /// Line color when selected.
    pub line_selected: Color32,
    /// Drawing label background color.
    pub label_bg: Color32,
    /// Drawing label text color.
    pub label_text: Color32,
    /// Fibonacci retracement 0% level color.
    pub fib_0: Color32,
    /// Fibonacci retracement 23.6% level color.
    pub fib_236: Color32,
    /// Fibonacci retracement 38.2% level color.
    pub fib_382: Color32,
    /// Fibonacci retracement 50% level color.
    pub fib_50: Color32,
    /// Fibonacci retracement 61.8% level color.
    pub fib_618: Color32,
    /// Fibonacci retracement 100% level color.
    pub fib_100: Color32,
}

/// Subscription tier badge colors (free, pro, premium).
#[derive(Clone, Debug)]
pub struct TierSemanticTokens {
    /// Free tier badge color.
    pub free: Color32,
    /// Pro tier badge color.
    pub pro: Color32,
    /// Premium tier badge color.
    pub premium: Color32,
}

/// Button variant colors for primary, secondary, outlined, and danger buttons.
///
/// Follows the rerun-inspired variant system where each button type has
/// background, hover, active, and foreground colors.
#[derive(Clone, Debug)]
pub struct ButtonSemanticTokens {
    /// Primary button background (high emphasis, accent-colored).
    pub primary_bg: Color32,
    /// Primary button background when hovered.
    pub primary_bg_hover: Color32,
    /// Primary button background when pressed.
    pub primary_bg_active: Color32,
    /// Primary button foreground (text/icon, typically inverse of background).
    pub primary_fg: Color32,
    /// Secondary button background (medium emphasis).
    pub secondary_bg: Color32,
    /// Secondary button background when hovered.
    pub secondary_bg_hover: Color32,
    /// Secondary button background when pressed.
    pub secondary_bg_active: Color32,
    /// Secondary button foreground.
    pub secondary_fg: Color32,
    /// Outlined button border color.
    pub outlined_border: Color32,
    /// Outlined button border color when hovered.
    pub outlined_border_hover: Color32,
    /// Danger button background (destructive actions).
    pub danger_bg: Color32,
    /// Danger button background when hovered.
    pub danger_bg_hover: Color32,
    /// Danger button background when pressed.
    pub danger_bg_active: Color32,
    /// Danger button foreground.
    pub danger_fg: Color32,
}

/// Colors for list items and selectable rows (rerun-inspired).
///
/// Covers background, text, and icon colors in default, hovered, active, and
/// on-primary-selection states.
#[derive(Clone, Debug)]
pub struct ListItemSemanticTokens {
    /// Background when the item is hovered.
    pub hovered_bg: Color32,
    /// Background when the item is active/selected.
    pub active_bg: Color32,
    /// Text color in the default (resting) state.
    pub default_text: Color32,
    /// Text color when hovered.
    pub hovered_text: Color32,
    /// Text color when active/selected.
    pub active_text: Color32,
    /// Strong (emphasized) text color.
    pub strong_text: Color32,
    /// Text color for non-interactive (read-only) items.
    pub noninteractive_text: Color32,
    /// Text color on a primary (accent) background.
    pub text_on_primary: Color32,
    /// Text color on a hovered primary background.
    pub text_on_primary_hovered: Color32,
    /// Icon color in the default state.
    pub default_icon: Color32,
    /// Icon color when hovered.
    pub hovered_icon: Color32,
    /// Icon color when active/selected.
    pub active_icon: Color32,
    /// Icon color on a primary background.
    pub icon_on_primary: Color32,
    /// Icon color on a hovered primary background.
    pub icon_on_primary_hovered: Color32,
}

/// Extended semantic tokens providing flat access to commonly used colors.
///
/// This is a convenience collection that gathers the most frequently accessed
/// colors across domains (UI backgrounds, borders, text, accent, status,
/// Material Design palette colors, chart colors, and trading colors) into
/// a single flat namespace.
#[derive(Clone, Debug)]
pub struct ExtendedSemanticTokens {
    /// Outermost window/app background.
    pub bg_outer: Color32,
    /// Panel background.
    pub bg_panel: Color32,
    /// Hover state background.
    pub bg_hover: Color32,
    /// Active/pressed state background.
    pub bg_active: Color32,
    /// Standard border color.
    pub border: Color32,
    /// Subtle/secondary border color.
    pub border_subtle: Color32,
    /// Primary text color.
    pub text: Color32,
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

/// Colors for modal dialog overlays and panels.
#[derive(Clone, Debug)]
pub struct ModalSemanticTokens {
    /// Semi-transparent backdrop overlay behind the modal.
    pub overlay_bg: Color32,
    /// Modal panel background.
    pub panel_bg: Color32,
    /// Modal panel border.
    pub panel_border: Color32,
    /// Modal header/title bar background.
    pub header_bg: Color32,
}

/// Colors for toast/snackbar notification popups.
#[derive(Clone, Debug)]
pub struct ToastSemanticTokens {
    /// Info toast background.
    pub info_bg: Color32,
    /// Info toast icon color.
    pub info_icon: Color32,
    /// Success toast background.
    pub success_bg: Color32,
    /// Success toast icon color.
    pub success_icon: Color32,
    /// Warning toast background.
    pub warning_bg: Color32,
    /// Warning toast icon color.
    pub warning_icon: Color32,
    /// Error toast background.
    pub error_bg: Color32,
    /// Error toast icon color.
    pub error_icon: Color32,
    /// Toast message text color.
    pub text: Color32,
    /// Toast drop-shadow color.
    pub shadow: Color32,
    /// Progress bar track background.
    pub progress_bar_bg: Color32,
    /// Progress bar filled portion.
    pub progress_bar_fill: Color32,
}

/// Colors for the command palette (Ctrl+K / Cmd+K) popup.
#[derive(Clone, Debug)]
pub struct CommandPaletteSemanticTokens {
    /// Palette background.
    pub bg: Color32,
    /// Palette border.
    pub border: Color32,
    /// Search input background.
    pub input_bg: Color32,
    /// Result item background when hovered.
    pub item_hover: Color32,
    /// Result item background when selected (keyboard navigation).
    pub item_selected: Color32,
    /// Keyboard shortcut text color.
    pub shortcut_text: Color32,
}
