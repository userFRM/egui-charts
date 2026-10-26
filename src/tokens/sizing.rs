//! Sizing tokens for UI components, chart elements, and layout dimensions.
//!
//! These structs define pixel dimensions for buttons, icons, toolbars, panels,
//! dialogs, chart elements, and many specialized components.
//!
//! Access via `DESIGN_TOKENS.sizing.*`, e.g.:
//!
//! ```rust
//! use egui_charts::tokens::DESIGN_TOKENS;
//!
//! let btn_h = DESIGN_TOKENS.sizing.button_md;
//! let toolbar_button_h = DESIGN_TOKENS.sizing.toolbar.button_height;
//! ```

/// Root sizing tokens containing icon sizes, button sizes, and sub-module sizing.
///
/// General sizing values (icons, buttons, list items) are at the top level.
/// Component-specific sizing is nested in sub-structs (toolbar, panel, dialog, etc.).
#[derive(Clone, Debug)]
pub struct SizingTokens {
    pub target_min: f32,
    pub icon_xs: f32,
    pub icon_14: f32,
    pub icon_sm: f32,
    pub icon_md: f32,
    pub icon_btn: f32,
    pub icon_lg: f32,
    pub icon_xxl: f32,
    pub button_sm: f32,
    pub button_md: f32,
    pub button_dialog: f32,
    pub button_lg: f32,
    pub button_xl: f32,
    pub button_xxl: f32,
    pub list_item_height: f32,
    /// Minimum height of egui's built-in controls: buttons, checkboxes, combo boxes and text fields.
    pub control_height: f32,
    // Sub-modules
    pub toolbar: ToolbarSizingTokens,
    pub panel: PanelSizingTokens,
    pub dialog: DialogSizingTokens,
    pub settings_dialog: SettingsDialogSizingTokens,
    pub context_menu: ContextMenuSizingTokens,
    pub symbol_header: SymbolHeaderSizingTokens,
    pub candle: CandleSizingTokens,
    // New token categories
    pub floating_toolbar: FloatingToolbarSizingTokens,
    pub command_palette: CommandPaletteSizingTokens,
    pub replay: ReplaySizingTokens,
    pub drawing_toolbar_ext: DrawingToolbarExtSizingTokens,
    pub annotation: AnnotationSizingTokens,
    pub media: MediaSizingTokens,
    pub crosshair: CrosshairSizingTokens,
    pub position_tool: PositionToolSizingTokens,
    pub charts_ext: ChartsExtSizingTokens,
    pub technical_labels: TechnicalLabelsSizingTokens,
    pub tooltip: TooltipSizingTokens,
    pub drawing: DrawingSizingTokens,
    pub notification: NotificationSizingTokens,
    pub menu: MenuSizingTokens,
}

/// Toolbar dimensions (left, bottom and right panels and button sizes).
#[derive(Clone, Debug)]
pub struct ToolbarSizingTokens {
    pub left_width: f32,
    pub bottom_height: f32,
    pub right_icon_width: f32,
    pub right_panel_width: f32,

    // Exact toolbar dimensions
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

/// Panel dimensions (bottom panel heights, widget default sizes).
#[derive(Clone, Debug)]
pub struct PanelSizingTokens {
    pub bottom_default_height: f32,
}

#[derive(Clone, Debug)]
pub struct DialogSizingTokens {
    pub submenu_width: f32,
    pub submenu_item_height: f32,
    pub color_picker_width: f32,
    pub color_picker_height: f32,
    pub menu_settings_width: f32,
    pub menu_settings_height: f32,
    // Extended dialog sizing tokens
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

#[derive(Clone, Debug)]
pub struct SettingsDialogSizingTokens {
    // Dialog frame
    pub width: f32,
    pub height: f32,
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
    pub content_min_width: f32,
    // Row layout
    pub row_height: f32,
    pub row_spacing: f32,
    pub label_width: f32,
    // Controls
    pub dropdown_width: f32,
    pub checkbox_size: f32,
    pub input_width_margin: f32,
    pub section_spacing: f32,
    // Footer
    pub footer_height: f32,
    pub footer_padding_h: f32,
    pub footer_padding_v: f32,
    pub button_height: f32,
    pub button_min_width: f32,
    pub button_gap: f32,
    // Additional controls
    pub small_dropdown_width: f32,
    pub template_button_width: f32,
    pub template_menu_width: f32,
    pub template_menu_max_height: f32,
    // Tab indicator
    pub indicator_width: f32,
    pub indicator_offset: f32,
}

#[derive(Clone, Debug)]
pub struct ContextMenuSizingTokens {
    // Menu frame
    pub min_width: f32,
    pub max_width: f32,
    pub rounding: f32,
    pub padding_v: f32,
    // Menu items
    pub item_height: f32,
    pub item_padding_h: f32,
    pub icon_width: f32,
    pub icon_size: f32,
    pub font_size: f32,
    pub shortcut_font_size: f32,
    // Separator
    pub separator_height: f32,
    pub separator_thickness: f32,
    pub separator_margin_h: f32,
    // Submenu
    pub submenu_arrow_size: f32,
    pub shortcut_width: f32,
    pub screen_edge_padding: f32,
}

#[derive(Clone, Debug)]
pub struct SymbolHeaderSizingTokens {
    pub height: f32,
    pub quote_box_height: f32,
}

/// Candlestick rendering dimensions (body width ratio, wick width, min body height).
#[derive(Clone, Debug)]
pub struct CandleSizingTokens {
    pub body_width_ratio: f32,
    pub wick_width: f32,
    pub wick_width_hidpi: f32,
    pub min_body_height: f32,
    pub volume_alpha: u8,
}

#[derive(Clone, Debug)]
pub struct FloatingToolbarSizingTokens {
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

#[derive(Clone, Debug)]
pub struct CommandPaletteSizingTokens {
    pub width: f32,
    pub height: f32,
    pub margin_x: f32,
    pub margin_y: f32,
    pub input_padding: f32,
    pub scroll_padding: f32,
}

#[derive(Clone, Debug)]
pub struct ReplaySizingTokens {
    pub control_bar_height: f32,
    pub button_size: f32,
    pub speed_dropdown_width: f32,
    pub date_display_width: f32,
    pub progress_bar_width: f32,
}

#[derive(Clone, Debug)]
pub struct DrawingToolbarExtSizingTokens {
    pub submenu_width_lg: f32,
    pub submenu_width_xl: f32,
    pub name_offset_active: f32,
    pub name_offset_inactive: f32,
    pub submenu_offset_y: f32,
    pub submenu_margin: f32,
}

#[derive(Clone, Debug)]
pub struct AnnotationSizingTokens {
    pub label_padding_x: f32,
    pub label_padding_y: f32,
    pub callout_padding_x: f32,
    pub callout_padding_y: f32,
    pub pointer_size: f32,
    pub note_size: f32,
    pub note_fold_size: f32,
    pub table_cell_width: f32,
    pub table_cell_height: f32,
    pub table_header_height: f32,
    pub signpost_padding_x: f32,
    pub signpost_padding_y: f32,
    pub signpost_bracket: f32,
    pub text_padding_y: f32,
}

#[derive(Clone, Debug)]
pub struct MediaSizingTokens {
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

#[derive(Clone, Debug)]
pub struct CrosshairSizingTokens {
    pub dot_radius: f32,
    pub label_offset_x: f32,
    pub label_rounding: f32,
    pub plus_symbol_spacing: f32,
}

#[derive(Clone, Debug)]
pub struct PositionToolSizingTokens {
    pub min_width: f32,
    pub label_info_width: f32,
    pub label_info_height: f32,
    pub label_target_width: f32,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
    pub text_offset_x: f32,
}

#[derive(Clone, Debug)]
pub struct ChartsExtSizingTokens {
    pub volume_bubbles_max_diameter: f32,
    pub heatmap_strip_height: f32,
    pub realtime_button_width: f32,
}

#[derive(Clone, Debug)]
pub struct TechnicalLabelsSizingTokens {
    pub gann_label_width: f32,
    pub gann_label_height: f32,
    pub elliott_label_size: f32,
    pub fib_label_offset_x: f32,
    pub channel_label_width: f32,
    pub channel_label_height: f32,
    pub channel_offset_x: f32,
    pub cycle_label_width: f32,
    pub pattern_label_width: f32,
    pub pattern_label_height: f32,
    pub line_label_width: f32,
    pub line_label_height: f32,
    pub line_arc_radius: f32,
    pub hs_label_width: f32,
}

#[derive(Clone, Debug)]
pub struct TooltipSizingTokens {
    pub cursor_offset_x: f32,
}

#[derive(Clone, Debug)]
pub struct DrawingSizingTokens {
    pub magnet_distance: f32,
}

#[derive(Clone, Debug)]
pub struct NotificationSizingTokens {
    pub panel_width: f32,
}

#[derive(Clone, Debug)]
pub struct MenuSizingTokens {
    pub item_height: f32,
}

/// Corner rounding tokens for consistent border-radius across components.
///
/// Ranges from `none` (0px) through `pill` (fully rounded). Semantic aliases
/// like `button`, `panel`, `dialog`, `input` map to appropriate scale values.
#[derive(Clone, Debug)]
pub struct RoundingTokens {
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
}

/// Stroke width tokens for lines, borders, and outlines.
///
/// Scale: `extra_thin` (finest) through `thick` (boldest).
#[derive(Clone, Debug)]
pub struct StrokeTokens {
    pub extra_thin: f32,
    pub light: f32,
    pub hairline: f32,
    pub medium: f32,
    pub thick: f32,
}

/// Shadow offset tokens for drop shadows on floating panels and menus.
#[derive(Clone, Debug)]
pub struct ShadowTokens {
    pub offset_sm: f32,
}

/// Layout tokens for menu shadows and label offsets.
#[derive(Clone, Debug)]
pub struct LayoutTokens {
    pub menu_shadow_offset_x: f32,
    pub menu_shadow_offset_y: f32,
}
