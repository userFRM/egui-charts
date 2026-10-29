//! Sizing tokens for UI components, chart elements, and layout dimensions.
//!
//! These structs define dimensions in egui logical points for buttons, icons, toolbars, panels,
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
    /// Target minimum, in egui logical points.
    pub target_min: f32,
    /// Icon extra small, in egui logical points.
    pub icon_xs: f32,
    /// Icon 14, in egui logical points.
    pub icon_14: f32,
    /// Icon small, in egui logical points.
    pub icon_sm: f32,
    /// Icon medium, in egui logical points.
    pub icon_md: f32,
    /// Icon button, in egui logical points.
    pub icon_btn: f32,
    /// Icon large, in egui logical points.
    pub icon_lg: f32,
    /// Icon double extra large, in egui logical points.
    pub icon_xxl: f32,
    /// Button small, in egui logical points.
    pub button_sm: f32,
    /// Button medium, in egui logical points.
    pub button_md: f32,
    /// Button dialog, in egui logical points.
    pub button_dialog: f32,
    /// Button large, in egui logical points.
    pub button_lg: f32,
    /// Button extra large, in egui logical points.
    pub button_xl: f32,
    /// Button double extra large, in egui logical points.
    pub button_xxl: f32,
    /// List item height, in egui logical points.
    pub list_item_height: f32,
    /// Minimum height of egui's built-in controls: buttons, checkboxes, combo boxes and text fields.
    pub control_height: f32,
    // Sub-modules
    /// Toolbar defaults.
    pub toolbar: ToolbarSizingTokens,
    /// Panel defaults.
    pub panel: PanelSizingTokens,
    /// Dialog defaults.
    pub dialog: DialogSizingTokens,
    /// Settings dialog defaults.
    pub settings_dialog: SettingsDialogSizingTokens,
    /// Context menu defaults.
    pub context_menu: ContextMenuSizingTokens,
    /// Symbol header defaults.
    pub symbol_header: SymbolHeaderSizingTokens,
    /// Candle defaults.
    pub candle: CandleSizingTokens,
    // New token categories
    /// Floating toolbar defaults.
    pub floating_toolbar: FloatingToolbarSizingTokens,
    /// Command palette defaults.
    pub command_palette: CommandPaletteSizingTokens,
    /// Replay defaults.
    pub replay: ReplaySizingTokens,
    /// Drawing toolbar additional defaults.
    pub drawing_toolbar_ext: DrawingToolbarExtSizingTokens,
    /// Annotation defaults.
    pub annotation: AnnotationSizingTokens,
    /// Media defaults.
    pub media: MediaSizingTokens,
    /// Crosshair defaults.
    pub crosshair: CrosshairSizingTokens,
    /// Position tool defaults.
    pub position_tool: PositionToolSizingTokens,
    /// Charts additional defaults.
    pub charts_ext: ChartsExtSizingTokens,
    /// Technical labels defaults.
    pub technical_labels: TechnicalLabelsSizingTokens,
    /// Tooltip defaults.
    pub tooltip: TooltipSizingTokens,
    /// Drawing defaults.
    pub drawing: DrawingSizingTokens,
    /// Notification defaults.
    pub notification: NotificationSizingTokens,
    /// Menu defaults.
    pub menu: MenuSizingTokens,
}

/// Toolbar dimensions (left, bottom and right panels and button sizes).
#[derive(Clone, Debug)]
pub struct ToolbarSizingTokens {
    /// Left width, in egui logical points.
    pub left_width: f32,
    /// Bottom height, in egui logical points.
    pub bottom_height: f32,
    /// Right icon width, in egui logical points.
    pub right_icon_width: f32,
    /// Right panel width, in egui logical points.
    pub right_panel_width: f32,

    // Exact toolbar dimensions
    /// Button width, in egui logical points.
    pub button_width: f32,
    /// Button height, in egui logical points.
    pub button_height: f32,
    /// Horizontal hover margin, in egui logical points.
    pub hover_margin_h: f32,
    /// Vertical hover margin, in egui logical points.
    pub hover_margin_v: f32,
    /// Inner rounding, in egui logical points.
    pub inner_rounding: f32,
    /// Icon size, in egui logical points.
    pub icon_size: f32,
    /// Separator margin, in egui logical points.
    pub separator_margin: f32,
    /// Separator gap, in egui logical points.
    pub separator_gap: f32,

    // Right sidebar (widget bar) exact dimensions
    /// Right sidebar width, in egui logical points.
    pub right_sidebar_width: f32,
    /// Right button size, in egui logical points.
    pub right_btn_size: f32,
    /// Right icon size, in egui logical points.
    pub right_icon_size: f32,
    /// Right button spacing, in egui logical points.
    pub right_btn_spacing: f32,
    /// Right hover margin, in egui logical points.
    pub right_hover_margin: f32,
    /// Right hover rounding, in egui logical points.
    pub right_hover_rounding: f32,

    // Top toolbar separator dimensions
    /// Separator height, in egui logical points.
    pub separator_height: f32,
    /// Separator width, in egui logical points.
    pub separator_width: f32,
}

/// Panel dimensions (bottom panel heights, widget default sizes).
#[derive(Clone, Debug)]
pub struct PanelSizingTokens {
    /// Bottom default height, in egui logical points.
    pub bottom_default_height: f32,
}

/// Dialog sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct DialogSizingTokens {
    /// Submenu width, in egui logical points.
    pub submenu_width: f32,
    /// Submenu item height, in egui logical points.
    pub submenu_item_height: f32,
    /// Color picker width, in egui logical points.
    pub color_picker_width: f32,
    /// Color picker height, in egui logical points.
    pub color_picker_height: f32,
    /// Menu settings width, in egui logical points.
    pub menu_settings_width: f32,
    /// Menu settings height, in egui logical points.
    pub menu_settings_height: f32,
    // Extended dialog sizing tokens
    /// Default width, in egui logical points.
    pub default_width: f32,
    /// Default height, in egui logical points.
    pub default_height: f32,
    /// Default minimum height, in egui logical points.
    pub default_min_height: f32,
    /// Confirm width, in egui logical points.
    pub confirm_width: f32,
    /// Prompt width small, in egui logical points.
    pub prompt_width_sm: f32,
    /// Prompt width large, in egui logical points.
    pub prompt_width_lg: f32,
    /// Alert width, in egui logical points.
    pub alert_width: f32,
    /// Properties height, in egui logical points.
    pub properties_height: f32,
    /// Label width, in egui logical points.
    pub label_width: f32,
    /// Input width, in egui logical points.
    pub input_width: f32,
    /// Series sidebar width, in egui logical points.
    pub series_sidebar_width: f32,
    /// Grid dropdown width, in egui logical points.
    pub grid_dropdown_width: f32,
    /// Indicator item width, in egui logical points.
    pub indicator_item_width: f32,
    /// Button minimum width extra small, in egui logical points.
    pub button_min_width_xs: f32,
}

/// Settings dialog sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct SettingsDialogSizingTokens {
    // Dialog frame
    /// Width, in egui logical points.
    pub width: f32,
    /// Height, in egui logical points.
    pub height: f32,
    /// Rounding, in egui logical points.
    pub rounding: f32,
    // Title bar
    /// Title height, in egui logical points.
    pub title_height: f32,
    /// Title font size, in egui logical points.
    pub title_font_size: f32,
    /// Title padding left, in egui logical points.
    pub title_padding_left: f32,
    /// Close button size, in egui logical points.
    pub close_button_size: f32,
    /// Close button margin, in egui logical points.
    pub close_button_margin: f32,
    /// Close icon size, in egui logical points.
    pub close_icon_size: f32,
    // Sidebar
    /// Sidebar width, in egui logical points.
    pub sidebar_width: f32,
    /// Tab height, in egui logical points.
    pub tab_height: f32,
    /// Horizontal tab padding, in egui logical points.
    pub tab_padding_h: f32,
    /// Vertical tab padding, in egui logical points.
    pub tab_padding_v: f32,
    /// Tab icon size, in egui logical points.
    pub tab_icon_size: f32,
    /// Tab icon text gap, in egui logical points.
    pub tab_icon_text_gap: f32,
    /// Tab font size, in egui logical points.
    pub tab_font_size: f32,
    // Content area
    /// Content padding top, in egui logical points.
    pub content_padding_top: f32,
    /// Horizontal content padding, in egui logical points.
    pub content_padding_h: f32,
    /// Content minimum width, in egui logical points.
    pub content_min_width: f32,
    // Row layout
    /// Row height, in egui logical points.
    pub row_height: f32,
    /// Row spacing, in egui logical points.
    pub row_spacing: f32,
    /// Label width, in egui logical points.
    pub label_width: f32,
    // Controls
    /// Dropdown width, in egui logical points.
    pub dropdown_width: f32,
    /// Checkbox size, in egui logical points.
    pub checkbox_size: f32,
    /// Input width margin, in egui logical points.
    pub input_width_margin: f32,
    /// Section spacing, in egui logical points.
    pub section_spacing: f32,
    // Footer
    /// Footer height, in egui logical points.
    pub footer_height: f32,
    /// Horizontal footer padding, in egui logical points.
    pub footer_padding_h: f32,
    /// Vertical footer padding, in egui logical points.
    pub footer_padding_v: f32,
    /// Button height, in egui logical points.
    pub button_height: f32,
    /// Button minimum width, in egui logical points.
    pub button_min_width: f32,
    /// Button gap, in egui logical points.
    pub button_gap: f32,
    // Additional controls
    /// Small dropdown width, in egui logical points.
    pub small_dropdown_width: f32,
    /// Template button width, in egui logical points.
    pub template_button_width: f32,
    /// Template menu width, in egui logical points.
    pub template_menu_width: f32,
    /// Template menu maximum height, in egui logical points.
    pub template_menu_max_height: f32,
    // Tab indicator
    /// Indicator width, in egui logical points.
    pub indicator_width: f32,
    /// Indicator offset, in egui logical points.
    pub indicator_offset: f32,
}

/// Context menu sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct ContextMenuSizingTokens {
    // Menu frame
    /// Minimum width, in egui logical points.
    pub min_width: f32,
    /// Maximum width, in egui logical points.
    pub max_width: f32,
    /// Rounding, in egui logical points.
    pub rounding: f32,
    /// Vertical padding, in egui logical points.
    pub padding_v: f32,
    // Menu items
    /// Item height, in egui logical points.
    pub item_height: f32,
    /// Horizontal item padding, in egui logical points.
    pub item_padding_h: f32,
    /// Icon width, in egui logical points.
    pub icon_width: f32,
    /// Icon size, in egui logical points.
    pub icon_size: f32,
    /// Font size, in egui logical points.
    pub font_size: f32,
    /// Shortcut font size, in egui logical points.
    pub shortcut_font_size: f32,
    // Separator
    /// Separator height, in egui logical points.
    pub separator_height: f32,
    /// Separator thickness, in egui logical points.
    pub separator_thickness: f32,
    /// Horizontal separator margin, in egui logical points.
    pub separator_margin_h: f32,
    // Submenu
    /// Submenu arrow size, in egui logical points.
    pub submenu_arrow_size: f32,
    /// Shortcut width, in egui logical points.
    pub shortcut_width: f32,
    /// Screen edge padding, in egui logical points.
    pub screen_edge_padding: f32,
}

/// Symbol header sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct SymbolHeaderSizingTokens {
    /// Height, in egui logical points.
    pub height: f32,
    /// Quote box height, in egui logical points.
    pub quote_box_height: f32,
}

/// Candlestick rendering dimensions (body width ratio, wick width, min body height).
#[derive(Clone, Debug)]
pub struct CandleSizingTokens {
    /// Body width ratio, as a fraction.
    pub body_width_ratio: f32,
    /// Wick width, in egui logical points.
    pub wick_width: f32,
    /// Wick width on high-density displays, in egui logical points.
    pub wick_width_hidpi: f32,
    /// Minimum body height, in egui logical points.
    pub min_body_height: f32,
    /// Volume alpha, from transparent (0) to opaque (255).
    pub volume_alpha: u8,
}

/// Floating toolbar sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct FloatingToolbarSizingTokens {
    /// Horizontal default, in egui logical points.
    pub default_x: f32,
    /// Vertical default, in egui logical points.
    pub default_y: f32,
    /// Drag handle width, in egui logical points.
    pub drag_handle_width: f32,
    /// Button size, in egui logical points.
    pub button_size: f32,
    /// Color swatch size, in egui logical points.
    pub color_size: f32,
    /// Separator width, in egui logical points.
    pub separator_width: f32,
    /// Drag handle height, in egui logical points.
    pub drag_handle_height: f32,
    /// Height, in egui logical points.
    pub height: f32,
    /// Dot size, in egui logical points.
    pub dot_size: f32,
    /// Dot spacing, in egui logical points.
    pub dot_spacing: f32,
    /// Separator height, in egui logical points.
    pub separator_height: f32,
}

/// Command palette sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct CommandPaletteSizingTokens {
    /// Width, in egui logical points.
    pub width: f32,
    /// Height, in egui logical points.
    pub height: f32,
    /// Horizontal margin, in egui logical points.
    pub margin_x: f32,
    /// Vertical margin, in egui logical points.
    pub margin_y: f32,
    /// Input padding, in egui logical points.
    pub input_padding: f32,
    /// Scroll padding, in egui logical points.
    pub scroll_padding: f32,
}

/// Replay sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct ReplaySizingTokens {
    /// Control bar height, in egui logical points.
    pub control_bar_height: f32,
    /// Button size, in egui logical points.
    pub button_size: f32,
    /// Speed dropdown width, in egui logical points.
    pub speed_dropdown_width: f32,
    /// Date display width, in egui logical points.
    pub date_display_width: f32,
    /// Progress bar width, in egui logical points.
    pub progress_bar_width: f32,
}

/// Drawing toolbar additional sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct DrawingToolbarExtSizingTokens {
    /// Large submenu width, in egui logical points.
    pub submenu_width_lg: f32,
    /// Extra large submenu width, in egui logical points.
    pub submenu_width_xl: f32,
    /// Name offset active, in egui logical points.
    pub name_offset_active: f32,
    /// Name offset inactive, in egui logical points.
    pub name_offset_inactive: f32,
    /// Vertical submenu offset, in egui logical points.
    pub submenu_offset_y: f32,
    /// Submenu margin, in egui logical points.
    pub submenu_margin: f32,
}

/// Annotation sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct AnnotationSizingTokens {
    /// Horizontal label padding, in egui logical points.
    pub label_padding_x: f32,
    /// Vertical label padding, in egui logical points.
    pub label_padding_y: f32,
    /// Horizontal callout padding, in egui logical points.
    pub callout_padding_x: f32,
    /// Vertical callout padding, in egui logical points.
    pub callout_padding_y: f32,
    /// Pointer size, in egui logical points.
    pub pointer_size: f32,
    /// Note size, in egui logical points.
    pub note_size: f32,
    /// Note fold size, in egui logical points.
    pub note_fold_size: f32,
    /// Table cell width, in egui logical points.
    pub table_cell_width: f32,
    /// Table cell height, in egui logical points.
    pub table_cell_height: f32,
    /// Table header height, in egui logical points.
    pub table_header_height: f32,
    /// Horizontal signpost padding, in egui logical points.
    pub signpost_padding_x: f32,
    /// Vertical signpost padding, in egui logical points.
    pub signpost_padding_y: f32,
    /// Signpost bracket, in egui logical points.
    pub signpost_bracket: f32,
    /// Vertical text padding, in egui logical points.
    pub text_padding_y: f32,
}

/// Media sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct MediaSizingTokens {
    /// Image minimum width, in egui logical points.
    pub image_min_width: f32,
    /// Image minimum height, in egui logical points.
    pub image_min_height: f32,
    /// Image default width, in egui logical points.
    pub image_default_width: f32,
    /// Image default height, in egui logical points.
    pub image_default_height: f32,
    /// Checkerboard size, in egui logical points.
    pub checkerboard_size: f32,
    /// Tweet card width, in egui logical points.
    pub tweet_card_width: f32,
    /// Tweet card height, in egui logical points.
    pub tweet_card_height: f32,
    /// Idea card width, in egui logical points.
    pub idea_card_width: f32,
    /// Idea card height, in egui logical points.
    pub idea_card_height: f32,
    /// Profile radius, in egui logical points.
    pub profile_radius: f32,
}

/// Crosshair sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct CrosshairSizingTokens {
    /// Dot radius, in egui logical points.
    pub dot_radius: f32,
    /// Horizontal label offset, in egui logical points.
    pub label_offset_x: f32,
    /// Label rounding, in egui logical points.
    pub label_rounding: f32,
    /// Plus symbol spacing, in egui logical points.
    pub plus_symbol_spacing: f32,
}

/// Position tool sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct PositionToolSizingTokens {
    /// Minimum width, in egui logical points.
    pub min_width: f32,
    /// Vertical label offset, in egui logical points.
    pub label_offset_y: f32,
}

/// Charts additional sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct ChartsExtSizingTokens {
    /// Volume bubbles maximum diameter, in egui logical points.
    pub volume_bubbles_max_diameter: f32,
    /// Heatmap strip height, in egui logical points.
    pub heatmap_strip_height: f32,
    /// Realtime button width, in egui logical points.
    pub realtime_button_width: f32,
}

/// Technical labels sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct TechnicalLabelsSizingTokens {
    /// Gann label width, in egui logical points.
    pub gann_label_width: f32,
    /// Gann label height, in egui logical points.
    pub gann_label_height: f32,
    /// Elliott label size, in egui logical points.
    pub elliott_label_size: f32,
    /// Horizontal fibonacci label offset, in egui logical points.
    pub fib_label_offset_x: f32,
    /// Channel label width, in egui logical points.
    pub channel_label_width: f32,
    /// Channel label height, in egui logical points.
    pub channel_label_height: f32,
    /// Horizontal channel offset, in egui logical points.
    pub channel_offset_x: f32,
    /// Cycle label width, in egui logical points.
    pub cycle_label_width: f32,
    /// Pattern label width, in egui logical points.
    pub pattern_label_width: f32,
    /// Pattern label height, in egui logical points.
    pub pattern_label_height: f32,
    /// Line label width, in egui logical points.
    pub line_label_width: f32,
    /// Line label height, in egui logical points.
    pub line_label_height: f32,
    /// Line arc radius, in egui logical points.
    pub line_arc_radius: f32,
    /// Head and shoulders label width, in egui logical points.
    pub hs_label_width: f32,
}

/// Tooltip sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct TooltipSizingTokens {
    /// Horizontal cursor offset, in egui logical points.
    pub cursor_offset_x: f32,
}

/// Drawing sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct DrawingSizingTokens {
    /// Magnet distance, in egui logical points.
    pub magnet_distance: f32,
}

/// Notification sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct NotificationSizingTokens {
    /// Panel width, in egui logical points.
    pub panel_width: f32,
}

/// Menu sizing defaults in egui logical points.
#[derive(Clone, Debug)]
pub struct MenuSizingTokens {
    /// Item height, in egui logical points.
    pub item_height: f32,
}

/// Corner rounding tokens for consistent border-radius across components.
///
/// Ranges from `none` (0px) through `pill` (fully rounded). Semantic aliases
/// like `button`, `panel`, `dialog`, `input` map to appropriate scale values.
#[derive(Clone, Debug)]
pub struct RoundingTokens {
    /// None, in egui logical points.
    pub none: f32,
    /// Extra small, in egui logical points.
    pub xs: f32,
    /// Small, in egui logical points.
    pub sm: f32,
    /// Medium, in egui logical points.
    pub md: f32,
    /// Large, in egui logical points.
    pub lg: f32,
    /// Extra large, in egui logical points.
    pub xl: f32,
    /// Pill, in egui logical points.
    pub pill: f32,
    /// Button, in egui logical points.
    pub button: f32,
    /// Panel, in egui logical points.
    pub panel: f32,
    /// Dialog, in egui logical points.
    pub dialog: f32,
}

/// Stroke width tokens for lines, borders, and outlines.
///
/// Scale: `extra_thin` (finest) through `thick` (boldest).
#[derive(Clone, Debug)]
pub struct StrokeTokens {
    /// Extra thin, in egui logical points.
    pub extra_thin: f32,
    /// Light, in egui logical points.
    pub light: f32,
    /// Hairline, in egui logical points.
    pub hairline: f32,
    /// Medium, in egui logical points.
    pub medium: f32,
    /// Thick, in egui logical points.
    pub thick: f32,
}

/// Shadow offset tokens for drop shadows on floating panels and menus.
#[derive(Clone, Debug)]
pub struct ShadowTokens {
    /// Offset small, in egui logical points.
    pub offset_sm: f32,
}

/// Layout tokens for menu shadows and label offsets.
#[derive(Clone, Debug)]
pub struct LayoutTokens {
    /// Horizontal menu shadow offset, in egui logical points.
    pub menu_shadow_offset_x: f32,
    /// Vertical menu shadow offset, in egui logical points.
    pub menu_shadow_offset_y: f32,
}
