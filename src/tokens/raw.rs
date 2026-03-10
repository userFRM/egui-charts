//! Raw Serde deserialization structures for the RON token file.
//!
//! These types mirror the structure of `design_tokens.ron` and are used only
//! during parsing. After deserialization, [`parser::parse_tokens`](super::parser)
//! resolves palette references and converts everything into the public
//! [`DesignTokens`](super::DesignTokens) type. These types are `pub(super)`
//! and not part of the public API.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub(super) struct RawDesignTokens {
    pub palette: HashMap<String, RawColor>,
    pub semantic: RawSemanticTokens,
    pub spacing: RawSpacingTokens,
    pub sizing: RawSizingTokens,
    pub rounding: RawRoundingTokens,
    pub typography: RawTypographyTokens,
    pub stroke: RawStrokeTokens,
    pub shadow: RawShadowTokens,
    pub layout: RawLayoutTokens,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawColor {
    Reference(String),
    Rgb(u8, u8, u8),
    Rgba(u8, u8, u8, u8),
}

#[derive(Deserialize)]
pub(super) struct RawSemanticTokens {
    pub ui: RawUiSemanticTokens,
    pub chart: RawChartSemanticTokens,
    pub brand: RawBrandSemanticTokens,
    pub status: RawStatusSemanticTokens,
    pub footprint: RawFootprintSemanticTokens,
    pub indicators: RawIndicatorSemanticTokens,
    pub drawings: RawDrawingSemanticTokens,
    pub tiers: RawTierSemanticTokens,
    pub buttons: RawButtonSemanticTokens,
    pub list_item: RawListItemSemanticTokens,
    pub extended: RawExtendedSemanticTokens,
    pub alert: RawAlertSemanticTokens,
    pub modal: RawModalSemanticTokens,
    pub toast: RawToastSemanticTokens,
    pub command_palette: RawCommandPaletteSemanticTokens,
    pub tpo: RawTpoSemanticTokens,
    pub heatmap: RawHeatmapSemanticTokens,
}

#[derive(Deserialize)]
pub(super) struct RawUiSemanticTokens {
    pub panel_bg_light: RawColor,
    pub panel_bg_secondary_light: RawColor,
    pub text_light: RawColor,
    pub text_secondary_light: RawColor,
    pub text_muted_light: RawColor,
    pub border_light: RawColor,
    pub border_subtle_light: RawColor,
    pub panel_bg_dark: RawColor,
    pub panel_bg_secondary_dark: RawColor,
    pub text_dark: RawColor,
    pub text_secondary_dark: RawColor,
    pub text_muted_dark: RawColor,
    pub border_dark: RawColor,
    pub border_subtle_dark: RawColor,
    pub icon_light: RawColor,
    pub icon_hover_light: RawColor,
    pub icon_dark: RawColor,
    pub icon_hover_dark: RawColor,
    pub icon_active: RawColor,
    pub btn_bg_light: RawColor,
    pub btn_bg_hover_light: RawColor,
    pub btn_bg_active_light: RawColor,
    pub btn_bg_dark: RawColor,
    pub btn_bg_hover_dark: RawColor,
    pub btn_bg_active_dark: RawColor,
    pub accent: RawColor,
    pub accent_hover: RawColor,
    pub warning: RawColor,
    pub success: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawChartSemanticTokens {
    pub bg: RawColor,
    pub bg_axis: RawColor,
    pub bg_selection: RawColor,
    pub grid_line: RawColor,
    pub grid_line_major: RawColor,
    pub axis_text: RawColor,
    pub axis_text_secondary: RawColor,
    pub crosshair_line: RawColor,
    pub crosshair_label_bg: RawColor,
    pub crosshair_label_text: RawColor,
    pub bullish: RawColor,
    pub bearish: RawColor,
    pub bullish_fill: RawColor,
    pub bearish_fill: RawColor,
    pub volume_up_alpha: u8,
    pub volume_down_alpha: u8,
    pub volume: RawColor,
    pub session_break: RawColor,
    pub watermark_alpha: u8,
    pub selection_text: RawColor,
    // Light chart variants
    pub bg_light: RawColor,
    pub bg_axis_light: RawColor,
    pub bg_selection_light: RawColor,
    pub grid_line_light: RawColor,
    pub grid_line_major_light: RawColor,
    pub crosshair_line_light: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawBrandSemanticTokens {
    pub accent: RawColor,
    pub accent_hover: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawStatusSemanticTokens {
    pub error: RawColor,
    pub error_dark: RawColor,
    pub error_darker: RawColor,
    pub info: RawColor,
    pub info_light: RawColor,
    pub disabled: RawColor,
    pub caution: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawFootprintSemanticTokens {
    pub poc: RawColor,
    pub value_area: RawColor,
    pub imbalance_buy: RawColor,
    pub imbalance_sell: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawTpoSemanticTokens {
    pub poc: RawColor,
    pub value_area: RawColor,
    pub initial_balance: RawColor,
    pub single_print: RawColor,
    pub session_separator: RawColor,
    pub letter_default: RawColor,
    pub opening_range: RawColor,
    pub grid: RawColor,
    pub period_1: RawColor,
    pub period_2: RawColor,
    pub period_3: RawColor,
    pub period_4: RawColor,
    pub period_5: RawColor,
    pub period_6: RawColor,
    pub period_7: RawColor,
    pub period_8: RawColor,
    pub period_9: RawColor,
    pub period_10: RawColor,
    pub period_11: RawColor,
    pub period_12: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawHeatmapSemanticTokens {
    pub bid_1: RawColor,
    pub bid_2: RawColor,
    pub bid_3: RawColor,
    pub bid_4: RawColor,
    pub bid_5: RawColor,
    pub ask_1: RawColor,
    pub ask_2: RawColor,
    pub ask_3: RawColor,
    pub ask_4: RawColor,
    pub ask_5: RawColor,
    pub mid_price: RawColor,
    pub border: RawColor,
    pub large_order_border: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawIndicatorSemanticTokens {
    pub ma: RawColor,
    pub ema: RawColor,
    pub sma: RawColor,
    pub bb_upper: RawColor,
    pub bb_middle: RawColor,
    pub bb_lower: RawColor,
    pub bb_fill: RawColor,
    pub rsi: RawColor,
    pub rsi_overbought: RawColor,
    pub rsi_oversold: RawColor,
    pub macd_line: RawColor,
    pub macd_signal: RawColor,
    pub macd_hist_pos: RawColor,
    pub macd_hist_neg: RawColor,
    pub vwap: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawDrawingSemanticTokens {
    pub default_line: RawColor,
    pub handle: RawColor,
    pub handle_hover: RawColor,
    pub handle_active: RawColor,
    pub line_hover: RawColor,
    pub line_selected: RawColor,
    pub label_bg: RawColor,
    pub label_text: RawColor,
    pub fib_0: RawColor,
    pub fib_236: RawColor,
    pub fib_382: RawColor,
    pub fib_50: RawColor,
    pub fib_618: RawColor,
    pub fib_100: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawTierSemanticTokens {
    pub free: RawColor,
    pub pro: RawColor,
    pub premium: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawButtonSemanticTokens {
    // Primary variant
    pub primary_bg: RawColor,
    pub primary_bg_hover: RawColor,
    pub primary_bg_active: RawColor,
    pub primary_fg: RawColor,
    // Secondary variant
    pub secondary_bg: RawColor,
    pub secondary_bg_hover: RawColor,
    pub secondary_bg_active: RawColor,
    pub secondary_fg: RawColor,
    // Outlined variant
    pub outlined_border: RawColor,
    pub outlined_border_hover: RawColor,
    // Danger variant
    pub danger_bg: RawColor,
    pub danger_bg_hover: RawColor,
    pub danger_bg_active: RawColor,
    pub danger_fg: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawListItemSemanticTokens {
    // Backgrounds
    pub hovered_bg: RawColor,
    pub active_bg: RawColor,
    // Text colors
    pub default_text: RawColor,
    pub hovered_text: RawColor,
    pub active_text: RawColor,
    pub strong_text: RawColor,
    pub noninteractive_text: RawColor,
    // Text on selection
    pub text_on_primary: RawColor,
    pub text_on_primary_hovered: RawColor,
    // Icon colors
    pub default_icon: RawColor,
    pub hovered_icon: RawColor,
    pub active_icon: RawColor,
    pub icon_on_primary: RawColor,
    pub icon_on_primary_hovered: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawExtendedSemanticTokens {
    // UI backgrounds
    pub bg_outer: RawColor,
    pub bg_panel: RawColor,
    pub bg_hover: RawColor,
    pub bg_active: RawColor,
    // UI borders
    pub border: RawColor,
    pub border_subtle: RawColor,
    // UI text
    pub text: RawColor,
    pub text_secondary: RawColor,
    pub text_muted: RawColor,
    // Accent
    pub accent: RawColor,
    pub accent_hover: RawColor,
    pub accent_active: RawColor,
    // Status colors
    pub success: RawColor,
    pub success_light: RawColor,
    pub success_dark: RawColor,
    pub success_darker: RawColor,
    pub error: RawColor,
    pub error_dark: RawColor,
    pub error_darker: RawColor,
    pub warning: RawColor,
    pub caution: RawColor,
    pub info: RawColor,
    pub info_light: RawColor,
    pub disabled: RawColor,
    // Material design colors
    pub purple: RawColor,
    pub deep_purple: RawColor,
    pub cyan: RawColor,
    pub teal: RawColor,
    pub indigo: RawColor,
    pub pink: RawColor,
    pub brown: RawColor,
    pub deep_orange: RawColor,
    pub gray: RawColor,
    pub light_gray: RawColor,
    pub favorite_gold: RawColor,
    // Chart colors
    pub chart_bg: RawColor,
    pub chart_axis_bg: RawColor,
    pub chart_tooltip_bg: RawColor,
    pub chart_text: RawColor,
    pub chart_text_secondary: RawColor,
    pub chart_text_muted: RawColor,
    pub chart_crosshair_label_bg: RawColor,
    // Trading colors
    pub bullish: RawColor,
    pub bearish: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawAlertSemanticTokens {
    pub info_bg: RawColor,
    pub info_border: RawColor,
    pub info_icon: RawColor,
    pub success_bg: RawColor,
    pub success_border: RawColor,
    pub success_icon: RawColor,
    pub warning_bg: RawColor,
    pub warning_border: RawColor,
    pub warning_icon: RawColor,
    pub error_bg: RawColor,
    pub error_border: RawColor,
    pub error_icon: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawModalSemanticTokens {
    pub overlay_bg: RawColor,
    pub panel_bg: RawColor,
    pub panel_border: RawColor,
    pub header_bg: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawToastSemanticTokens {
    pub info_bg: RawColor,
    pub info_icon: RawColor,
    pub success_bg: RawColor,
    pub success_icon: RawColor,
    pub warning_bg: RawColor,
    pub warning_icon: RawColor,
    pub error_bg: RawColor,
    pub error_icon: RawColor,
    pub text: RawColor,
    pub shadow: RawColor,
    pub progress_bar_bg: RawColor,
    pub progress_bar_fill: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawCommandPaletteSemanticTokens {
    pub bg: RawColor,
    pub border: RawColor,
    pub input_bg: RawColor,
    pub item_hover: RawColor,
    pub item_selected: RawColor,
    pub shortcut_text: RawColor,
}

#[derive(Deserialize)]
pub(super) struct RawSpacingTokens {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub xxxl: f32,
    pub hairline: f32,
    pub section_lg: f32,
    pub panel_gap: f32,
    pub button_padding: f32,
    pub toolbar_item_gap: f32,
    pub menu_item_padding: f32,
}

#[derive(Deserialize)]
pub(super) struct RawSizingTokens {
    pub target_min: f32,
    pub icon_xs: f32,
    pub icon_14: f32,
    pub icon_sm: f32,
    pub icon_md: f32,
    pub icon_btn: f32,
    pub icon_lg: f32,
    pub icon_xl: f32,
    pub icon_xxl: f32,
    pub button_sm: f32,
    pub button_md: f32,
    pub button_dialog: f32,
    pub button_lg: f32,
    pub button_xl: f32,
    pub button_xxl: f32,
    pub button_min_width_xs: f32,
    pub button_min_width_sm: f32,
    pub button_min_width_md: f32,
    pub button_min_width_lg: f32,
    pub list_item_height: f32,
    pub menu_item_height: f32,
    // Sub-modules
    pub toolbar: RawToolbarSizingTokens,
    pub panel: RawPanelSizingTokens,
    pub dialog: RawDialogSizingTokens,
    pub auth_dialog: RawAuthDialogSizingTokens,
    pub settings_dialog: RawSettingsDialogSizingTokens,
    pub context_menu: RawContextMenuSizingTokens,
    pub right_sidebar: RawRightSidebarSizingTokens,
    pub watermark: RawWatermarkSizingTokens,
    pub burger_menu: RawBurgerMenuSizingTokens,
    pub symbol_header: RawSymbolHeaderSizingTokens,
    pub timeframe_toolbar: RawTimeframeToolbarSizingTokens,
    pub footprint: RawFootprintSizingTokens,
    pub chart: RawChartSizingTokens,
    pub candle: RawCandleSizingTokens,
    // New token categories
    pub floating_toolbar: RawFloatingToolbarSizingTokens,
    pub command_palette: RawCommandPaletteSizingTokens,
    pub replay: RawReplaySizingTokens,
    pub emoji_picker: RawEmojiPickerSizingTokens,
    pub drawing_toolbar_ext: RawDrawingToolbarExtSizingTokens,
    pub annotation: RawAnnotationSizingTokens,
    pub media: RawMediaSizingTokens,
    pub crosshair: RawCrosshairSizingTokens,
    pub position_tool: RawPositionToolSizingTokens,
    pub charts_ext: RawChartsExtSizingTokens,
    pub technical_labels: RawTechnicalLabelsSizingTokens,
    pub tooltip: RawTooltipSizingTokens,
    pub cursor: RawCursorSizingTokens,
    pub drawing: RawDrawingSizingTokens,
    pub notification: RawNotificationSizingTokens,
    pub widget: RawWidgetSizingTokens,
    pub menu: RawMenuSizingTokens,
}

#[derive(Deserialize)]
pub(super) struct RawToolbarSizingTokens {
    pub top_height: f32,
    pub left_width: f32,
    pub bottom_height: f32,
    pub right_icon_width: f32,
    pub right_panel_width: f32,
    pub right_panel_min_width: f32,
    pub right_panel_max_width: f32,

    // Exact toolbar dimensions
    pub left_toolbar_width: f32,
    pub button_width: f32,
    pub button_height: f32,
    pub hover_margin_h: f32,
    pub hover_margin_v: f32,
    pub inner_rounding: f32,
    pub icon_size: f32,
    pub separator_margin: f32,
    pub separator_gap: f32,

    // Right sidebar (widget bar) exact dimensions
    pub right_sidebar_width: f32,
    pub right_btn_size: f32,
    pub right_icon_size: f32,
    pub right_btn_spacing: f32,
    pub right_hover_margin: f32,
    pub right_hover_rounding: f32,

    // Top toolbar separator dimensions
    pub separator_height: f32,
    pub separator_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawPanelSizingTokens {
    pub bottom_default_height: f32,
    pub bottom_min_height: f32,
    pub bottom_max_height: f32,
    pub bottom_toolbar_height: f32,
    pub widget_default_width: f32,
    pub widget_default_height: f32,
    pub widget_min_width: f32,
    pub widget_min_height: f32,

    // Trading panel row dimensions
    pub trading_panel_row_height: f32,
    pub trading_panel_row_icon_size: f32,
    pub trading_panel_row_btn_size: f32,
    pub trading_panel_row_icon_size_touch: f32,
    pub trading_panel_row_btn_size_touch: f32,
}

#[derive(Deserialize)]
pub(super) struct RawDialogSizingTokens {
    pub symbol_search_width: f32,
    pub symbol_search_height: f32,
    pub submenu_width: f32,
    pub submenu_item_height: f32,
    pub color_picker_width: f32,
    pub color_picker_height: f32,
    pub menu_settings_width: f32,
    pub menu_settings_height: f32,
    // Extended modal dialog fields
    pub default_width: f32,
    pub default_height: f32,
    pub default_min_height: f32,
    pub confirm_width: f32,
    pub prompt_width_sm: f32,
    pub prompt_width_lg: f32,
    pub alert_width: f32,
    pub properties_height: f32,
    pub label_width: f32,
    pub input_width: f32,
    pub series_sidebar_width: f32,
    pub grid_dropdown_width: f32,
    pub indicator_item_width: f32,
    pub button_min_width_xs: f32,
}

#[derive(Deserialize)]
pub(super) struct RawAuthDialogSizingTokens {
    pub user_menu_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawSettingsDialogSizingTokens {
    // Dialog frame
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub rounding: f32,
    // Title bar
    pub title_height: f32,
    pub title_font_size: f32,
    pub title_padding_left: f32,
    pub close_button_size: f32,
    pub close_button_margin: f32,
    pub close_icon_size: f32,
    // Sidebar
    pub sidebar_width: f32,
    pub tab_height: f32,
    pub tab_padding_h: f32,
    pub tab_padding_v: f32,
    pub tab_icon_size: f32,
    pub tab_icon_text_gap: f32,
    pub tab_font_size: f32,
    // Content area
    pub content_padding_top: f32,
    pub content_padding_h: f32,
    pub scroll_area_height: f32,
    pub content_min_width: f32,
    // Section headers
    pub section_margin_top: f32,
    pub section_margin_bottom: f32,
    pub section_font_size: f32,
    // Row layout
    pub row_height: f32,
    pub row_spacing: f32,
    pub label_width: f32,
    // Controls
    pub control_height: f32,
    pub dropdown_width: f32,
    pub input_width: f32,
    pub swatch_size: f32,
    pub checkbox_size: f32,
    pub slider_width: f32,
    pub small_input_width: f32,
    pub input_width_margin: f32,
    pub section_spacing: f32,
    // Footer
    pub footer_height: f32,
    pub footer_padding_h: f32,
    pub footer_padding_v: f32,
    pub button_height: f32,
    pub button_min_width: f32,
    pub button_padding_h: f32,
    pub button_gap: f32,
    // Additional controls
    pub small_dropdown_width: f32,
    pub xs_dropdown_width: f32,
    pub template_button_width: f32,
    pub template_menu_width: f32,
    pub template_menu_max_height: f32,
    // Tab indicator
    pub indicator_width: f32,
    pub indicator_offset: f32,
    // Grid layout
    pub grid_col_spacing: f32,
    pub grid_row_spacing: f32,
}

#[derive(Deserialize)]
pub(super) struct RawContextMenuSizingTokens {
    // Menu frame
    pub min_width: f32,
    pub max_width: f32,
    pub rounding: f32,
    pub padding_v: f32,
    // Menu items
    pub item_height: f32,
    pub item_padding_h: f32,
    pub item_padding_v: f32,
    pub icon_width: f32,
    pub icon_size: f32,
    pub icon_label_gap: f32,
    pub font_size: f32,
    pub shortcut_font_size: f32,
    // Separator
    pub separator_height: f32,
    pub separator_thickness: f32,
    pub separator_margin_h: f32,
    // Submenu
    pub submenu_arrow_size: f32,
    pub submenu_offset: f32,
    pub shortcut_width: f32,
    pub char_width: f32,
    pub screen_edge_padding: f32,
    pub submenu_arrow_font_size: f32,
}

#[derive(Deserialize)]
pub(super) struct RawRightSidebarSizingTokens {
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawWatermarkSizingTokens {
    pub font_size: f32,
    pub padding: f32,
    pub line_spacing: f32,
}

#[derive(Deserialize)]
pub(super) struct RawBurgerMenuSizingTokens {
    // Button
    pub button_size: f32,
    pub line_width_ratio: f32,
    pub line_height: f32,
    pub line_spacing_ratio: f32,
    // Panel
    pub panel_width: f32,
    pub header_padding: f32,
    pub header_spacing: f32,
    pub separator_padding: f32,
    // Menu items
    pub item_height: f32,
    pub item_padding: f32,
    pub icon_size: f32,
    pub font_size: f32,
}

#[derive(Deserialize)]
pub(super) struct RawSymbolHeaderSizingTokens {
    pub height: f32,
    pub quote_box_height: f32,
}

#[derive(Deserialize)]
pub(super) struct RawTimeframeToolbarSizingTokens {
    pub right_section_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawFootprintSizingTokens {
    pub bar_width_ratio: f32,
    pub poc_height_multiplier: f32,
    pub value_area_alpha: u8,
    pub stacked_imbalance_alpha: u8,
}

#[derive(Deserialize)]
pub(super) struct RawChartSizingTokens {
    pub padding: f32,
    pub right_axis_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawCandleSizingTokens {
    pub body_width_ratio: f32,
    pub wick_width: f32,
    pub wick_width_hidpi: f32,
    pub min_body_height: f32,
    pub volume_alpha: u8,
    pub grid_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawFloatingToolbarSizingTokens {
    pub default_x: f32,
    pub default_y: f32,
    pub drag_handle_width: f32,
    pub button_size: f32,
    pub color_size: f32,
    pub separator_width: f32,
    pub drag_handle_height: f32,
    pub height: f32,
    pub dot_size: f32,
    pub dot_spacing: f32,
    pub separator_height: f32,
}

#[derive(Deserialize)]
pub(super) struct RawCommandPaletteSizingTokens {
    pub width: f32,
    pub height: f32,
    pub margin_x: f32,
    pub margin_y: f32,
    pub input_padding: f32,
    pub scroll_padding: f32,
}

#[derive(Deserialize)]
pub(super) struct RawReplaySizingTokens {
    pub control_bar_height: f32,
    pub button_size: f32,
    pub speed_dropdown_width: f32,
    pub date_display_width: f32,
    pub progress_bar_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawEmojiPickerSizingTokens {
    pub width: f32,
    pub cell_size: f32,
    pub cell_size_lg: f32,
    pub category_width: f32,
    pub submenu_width: f32,
    pub input_padding: f32,
}

#[derive(Deserialize)]
pub(super) struct RawDrawingToolbarExtSizingTokens {
    pub submenu_width_sm: f32,
    pub submenu_width_lg: f32,
    pub submenu_width_xl: f32,
    pub min_icon: f32,
    pub min_width: f32,
    pub name_offset_active: f32,
    pub name_offset_inactive: f32,
    pub submenu_offset_y: f32,
    pub submenu_margin: f32,
}

#[derive(Deserialize)]
pub(super) struct RawAnnotationSizingTokens {
    pub label_padding_x: f32,
    pub label_padding_y: f32,
    pub callout_padding_x: f32,
    pub callout_padding_y: f32,
    pub pointer_size: f32,
    pub comment_bubble_width: f32,
    pub comment_bubble_height: f32,
    pub note_size: f32,
    pub note_fold_size: f32,
    pub table_cell_width: f32,
    pub table_cell_height: f32,
    pub table_header_height: f32,
    pub flag_pole_length: f32,
    pub flag_width: f32,
    pub flag_height: f32,
    pub signpost_padding_x: f32,
    pub signpost_padding_y: f32,
    pub signpost_post_width: f32,
    pub signpost_post_height: f32,
    pub signpost_bracket: f32,
    pub text_padding_x: f32,
    pub text_padding_y: f32,
    pub text_line_height: f32,
    pub text_inner_padding: f32,
    pub text_anchored_size: f32,
}

#[derive(Deserialize)]
pub(super) struct RawMediaSizingTokens {
    pub image_min_width: f32,
    pub image_min_height: f32,
    pub image_default_width: f32,
    pub image_default_height: f32,
    pub checkerboard_size: f32,
    pub tweet_card_width: f32,
    pub tweet_card_height: f32,
    pub idea_card_width: f32,
    pub idea_card_height: f32,
    pub profile_radius: f32,
}

#[derive(Deserialize)]
pub(super) struct RawCrosshairSizingTokens {
    pub dot_radius: f32,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
    pub label_rounding: f32,
    pub plus_symbol_spacing: f32,
    pub dash_on: f32,
    pub dash_off: f32,
}

#[derive(Deserialize)]
pub(super) struct RawPositionToolSizingTokens {
    pub min_width: f32,
    pub label_info_width: f32,
    pub label_info_height: f32,
    pub label_target_width: f32,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
    pub text_offset_x: f32,
    pub text_offset_y: f32,
}

#[derive(Deserialize)]
pub(super) struct RawChartsExtSizingTokens {
    pub min_width: f32,
    pub min_height: f32,
    pub indicator_pane_min: f32,
    pub indicator_pane_default: f32,
    pub volume_bubbles_max_diameter: f32,
    pub heatmap_strip_height: f32,
    pub order_line_label_width: f32,
    pub order_line_label_height: f32,
    pub time_label_min_spacing: f32,
    pub time_label_max_spacing: f32,
    pub price_label_min_spacing: f32,
    pub realtime_button_width: f32,
    pub price_scale_width: f32,
    pub time_scale_height: f32,
}

#[derive(Deserialize)]
pub(super) struct RawTechnicalLabelsSizingTokens {
    pub gann_label_width: f32,
    pub gann_label_height: f32,
    pub elliott_label_size: f32,
    pub fib_label_offset_x: f32,
    pub fib_timezone_width: f32,
    pub fib_timezone_height: f32,
    pub channel_label_width: f32,
    pub channel_label_height: f32,
    pub channel_offset_x: f32,
    pub cycle_label_width: f32,
    pub cycle_label_height: f32,
    pub pattern_label_width: f32,
    pub pattern_label_height: f32,
    pub line_label_width: f32,
    pub line_label_height: f32,
    pub line_arc_radius: f32,
    pub hs_label_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawTooltipSizingTokens {
    pub cursor_offset_x: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

#[derive(Deserialize)]
pub(super) struct RawCursorSizingTokens {}

#[derive(Deserialize)]
pub(super) struct RawDrawingSizingTokens {
    pub magnet_distance: f32,
}

#[derive(Deserialize)]
pub(super) struct RawNotificationSizingTokens {
    pub panel_width: f32,
}

#[derive(Deserialize)]
pub(super) struct RawWidgetSizingTokens {
    pub bid_ask_height: f32,
}

#[derive(Deserialize)]
pub(super) struct RawMenuSizingTokens {
    pub item_height: f32,
}

#[derive(Deserialize)]
pub(super) struct RawRoundingTokens {
    pub none: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub pill: f32,
    pub button: f32,
    pub panel: f32,
    pub dialog: f32,
    pub input: f32,
}

#[derive(Deserialize)]
pub(super) struct RawTypographyTokens {
    pub micro: f32,
    pub tiny: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub xxxl: f32,
    pub body: f32,
    pub small: f32,
    pub heading: f32,
    pub title: f32,
}

#[derive(Deserialize)]
pub(super) struct RawStrokeTokens {
    pub extra_thin: f32,
    pub light: f32,
    pub hairline: f32,
    pub thin: f32,
    pub medium: f32,
    pub thick: f32,
}

#[derive(Deserialize)]
pub(super) struct RawShadowTokens {
    pub offset_sm: f32,
    pub offset_md: f32,
}

#[derive(Deserialize)]
pub(super) struct RawLayoutTokens {
    pub menu_shadow_offset_x: f32,
    pub menu_shadow_offset_y: f32,
    pub label_offset_sm: f32,
    pub label_offset_md: f32,
}
