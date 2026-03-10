//! Component Styles - Pre-computed style defaults for UI components
//!
//! This module provides ready-to-use style configurations for common components.
//! These are derived from semantic tokens and include all styling properties
//! (colors, spacing, radii, etc.) needed to render a component.

use super::semantic::SemanticTokens;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, CornerRadius, Stroke, Vec2};

// ============================================================================
// BUTTON STYLE
// ============================================================================

/// Pre-computed button style for a specific variant (primary, secondary, ghost).
///
/// Contains all colors, strokes, and sizing needed to render a button in its
/// resting, hovered, active, and disabled states. Construct via
/// [`BtnStyle::primary`], [`BtnStyle::secondary`], or [`BtnStyle::ghost`].
#[derive(Clone, Copy, Debug)]
pub struct BtnStyle {
    /// Background color in the resting state.
    pub bg: Color32,
    /// Background color when hovered.
    pub bg_hover: Color32,
    /// Background color when pressed/active.
    pub bg_active: Color32,
    /// Background color when disabled.
    pub bg_disabled: Color32,
    /// Foreground (text/icon) color in the resting state.
    pub fg: Color32,
    /// Foreground color when hovered.
    pub fg_hover: Color32,
    /// Foreground color when disabled.
    pub fg_disabled: Color32,
    /// Border stroke in the resting state.
    pub border: Stroke,
    /// Border stroke when hovered.
    pub border_hover: Stroke,
    /// Border stroke when focused (keyboard navigation).
    pub border_focus: Stroke,
    /// Corner rounding radius.
    pub rounding: CornerRadius,
    /// Internal padding (horizontal, vertical).
    pub padding: Vec2,
    /// Minimum button size.
    pub min_size: Vec2,
}

impl BtnStyle {
    /// Creates a primary (accent-colored) button style from semantic tokens.
    pub fn primary(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.accent,
            bg_hover: semantic.ui.accent_hover,
            bg_active: semantic.ui.accent_active,
            bg_disabled: semantic.ui.btn_bg_disabled,
            fg: semantic.ui.accent_text,
            fg_hover: semantic.ui.accent_text,
            fg_disabled: semantic.ui.text_disabled,
            border: Stroke::NONE,
            border_hover: Stroke::NONE,
            border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.border_focus),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.md as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.md),
            min_size: Vec2::new(
                DESIGN_TOKENS.sizing.dialog.button_min_width_xs,
                DESIGN_TOKENS.sizing.button_md,
            ),
        }
    }

    /// Creates a secondary (bordered, neutral-background) button style from semantic tokens.
    pub fn secondary(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.btn_bg,
            bg_hover: semantic.ui.btn_bg_hover,
            bg_active: semantic.ui.btn_bg_active,
            bg_disabled: semantic.ui.btn_bg_disabled,
            fg: semantic.ui.text,
            fg_hover: semantic.ui.text,
            fg_disabled: semantic.ui.text_disabled,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            border_hover: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.border_focus),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.md as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.md),
            min_size: Vec2::new(
                DESIGN_TOKENS.sizing.dialog.button_min_width_xs,
                DESIGN_TOKENS.sizing.button_md,
            ),
        }
    }

    /// Creates a ghost (transparent background, no border) button style from semantic tokens.
    pub fn ghost(semantic: &SemanticTokens) -> Self {
        Self {
            bg: Color32::TRANSPARENT,
            bg_hover: semantic.ui.btn_bg_hover,
            bg_active: semantic.ui.btn_bg_active,
            bg_disabled: Color32::TRANSPARENT,
            fg: semantic.ui.text_secondary,
            fg_hover: semantic.ui.text,
            fg_disabled: semantic.ui.text_disabled,
            border: Stroke::NONE,
            border_hover: Stroke::NONE,
            border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.border_focus),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.md as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.sm),
            min_size: Vec2::splat(DESIGN_TOKENS.sizing.button_sm),
        }
    }
}

// ============================================================================
// TOOLBAR STYLE
// ============================================================================

/// Pre-computed styling for toolbar components (top bar, side bar).
///
/// Includes background, border, separator, icon colors in multiple states,
/// and sizing constants. Construct via [`ToolbarStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct ToolbarStyle {
    /// Toolbar background color.
    pub bg: Color32,
    /// Bottom/side border color.
    pub border: Color32,
    /// Color for vertical separator lines between toolbar groups.
    pub separator: Color32,
    /// Icon color in the resting state.
    pub icon_color: Color32,
    /// Icon color when hovered.
    pub icon_color_hover: Color32,
    /// Icon color when active/selected.
    pub icon_color_active: Color32,
    /// Item background when hovered.
    pub item_bg_hover: Color32,
    /// Item background when active/selected.
    pub item_bg_active: Color32,
    /// Corner rounding for toolbar items.
    pub item_rounding: CornerRadius,
    /// Internal padding for toolbar items.
    pub item_padding: Vec2,
    /// Gap between toolbar items.
    pub gap: f32,
    /// Total toolbar height in pixels.
    pub height: f32,
}

impl ToolbarStyle {
    /// Derives toolbar styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.toolbar_bg,
            border: semantic.ui.border,
            separator: semantic.ui.toolbar_separator,
            icon_color: semantic.ui.icon,
            icon_color_hover: semantic.ui.icon_hover,
            icon_color_active: semantic.ui.icon_active,
            item_bg_hover: semantic.ui.btn_bg_hover,
            item_bg_active: semantic.ui.btn_bg_active,
            item_rounding: CornerRadius::same(DESIGN_TOKENS.rounding.sm as u8),
            item_padding: Vec2::new(DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.md),
            gap: DESIGN_TOKENS.spacing.xs,
            height: DESIGN_TOKENS.sizing.toolbar.top_height,
        }
    }
}

// ============================================================================
// PANEL STYLE
// ============================================================================

/// Pre-computed styling for panel/container components (side panels, bottom panels).
///
/// Includes background, border, rounding, padding, and drop-shadow properties.
/// Use [`PanelStyle::from_semantic`] for standard panels or
/// [`PanelStyle::floating`] for elevated floating panels (popovers, tooltips).
#[derive(Clone, Copy, Debug)]
pub struct PanelStyle {
    /// Panel background color.
    pub bg: Color32,
    /// Border stroke (width + color).
    pub border: Stroke,
    /// Corner rounding radius.
    pub rounding: CornerRadius,
    /// Internal padding (horizontal, vertical).
    pub padding: Vec2,
    /// Drop-shadow color (alpha controls intensity).
    pub shadow_color: Color32,
    /// Drop-shadow offset (x, y).
    pub shadow_offset: Vec2,
    /// Drop-shadow blur radius in pixels.
    pub shadow_blur: f32,
}

impl PanelStyle {
    /// Derives standard panel styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.panel_bg,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.lg as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.lg),
            shadow_color: Color32::from_black_alpha(40),
            shadow_offset: Vec2::new(
                DESIGN_TOKENS.layout.menu_shadow_offset_x,
                DESIGN_TOKENS.shadow.offset_sm,
            ),
            shadow_blur: 8.0,
        }
    }

    /// Derives floating panel styling with a stronger shadow and elevated background.
    pub fn floating(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.lg as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.lg),
            shadow_color: Color32::from_black_alpha(80),
            shadow_offset: Vec2::new(
                DESIGN_TOKENS.layout.menu_shadow_offset_x,
                DESIGN_TOKENS.shadow.offset_md,
            ),
            shadow_blur: 16.0,
        }
    }
}

// ============================================================================
// MENU STYLE
// ============================================================================

/// Pre-computed styling for dropdown and context menus.
///
/// Covers the menu frame, individual item states, shortcut text, separators,
/// icons, and checkmarks. Construct via [`MenuStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct MenuStyle {
    /// Menu background color.
    pub bg: Color32,
    /// Menu border stroke.
    pub border: Stroke,
    /// Menu corner rounding.
    pub rounding: CornerRadius,
    /// Height of a single menu item row.
    pub item_height: f32,
    /// Internal padding for menu items.
    pub item_padding: Vec2,
    /// Item background when hovered.
    pub item_bg_hover: Color32,
    /// Item background when pressed.
    pub item_bg_active: Color32,
    /// Default menu item text color.
    pub text_color: Color32,
    /// Menu item text color when hovered.
    pub text_color_hover: Color32,
    /// Menu item text color when disabled.
    pub text_color_disabled: Color32,
    /// Color for keyboard shortcut labels.
    pub shortcut_color: Color32,
    /// Color for horizontal separator lines.
    pub separator_color: Color32,
    /// Color for menu item icons.
    pub icon_color: Color32,
    /// Color for check/radio marks.
    pub check_color: Color32,
}

impl MenuStyle {
    /// Derives menu styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.md as u8),
            item_height: DESIGN_TOKENS.sizing.menu.item_height,
            item_padding: Vec2::new(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.sm),
            item_bg_hover: semantic.ui.btn_bg_hover,
            item_bg_active: semantic.ui.btn_bg_active,
            text_color: semantic.ui.text,
            text_color_hover: semantic.ui.text,
            text_color_disabled: semantic.ui.text_disabled,
            shortcut_color: semantic.ui.text_muted,
            separator_color: semantic.ui.border_subtle,
            icon_color: semantic.ui.icon,
            check_color: semantic.ui.accent,
        }
    }
}

// ============================================================================
// INPUT STYLE
// ============================================================================

/// Pre-computed styling for text input fields.
///
/// Covers background, text, border, and cursor colors across resting, hover,
/// focus, disabled, and error states. Construct via [`InputStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct InputStyle {
    /// Background color in the resting state.
    pub bg: Color32,
    /// Background color when hovered.
    pub bg_hover: Color32,
    /// Background color when focused (cursor active).
    pub bg_focus: Color32,
    /// Background color when disabled.
    pub bg_disabled: Color32,
    /// Text color for entered content.
    pub text_color: Color32,
    /// Text color for placeholder hint text.
    pub text_color_placeholder: Color32,
    /// Text color when disabled.
    pub text_color_disabled: Color32,
    /// Border stroke in the resting state.
    pub border: Stroke,
    /// Border stroke when hovered.
    pub border_hover: Stroke,
    /// Border stroke when focused (keyboard navigation or click).
    pub border_focus: Stroke,
    /// Border stroke in the error/validation-failed state.
    pub border_error: Stroke,
    /// Corner rounding radius.
    pub rounding: CornerRadius,
    /// Internal padding (horizontal, vertical).
    pub padding: Vec2,
    /// Background color for selected text.
    pub selection_bg: Color32,
    /// Color of the text cursor (caret).
    pub cursor_color: Color32,
}

impl InputStyle {
    /// Derives input field styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.panel_bg,
            bg_hover: semantic.ui.panel_bg,
            bg_focus: semantic.ui.panel_bg,
            bg_disabled: semantic.ui.btn_bg_disabled,
            text_color: semantic.ui.text,
            text_color_placeholder: semantic.ui.text_muted,
            text_color_disabled: semantic.ui.text_disabled,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            border_hover: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.border_focus),
            border_error: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.error),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.md as u8),
            padding: Vec2::new(DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.sm),
            selection_bg: semantic.ui.selection_bg,
            cursor_color: semantic.ui.accent,
        }
    }
}

// ============================================================================
// CHART COMPONENT STYLES
// ============================================================================

/// Pre-computed styling for the main chart area (background, grid, axes, crosshair).
///
/// Construct via [`ChartStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct ChartStyle {
    /// Chart area background color.
    pub bg: Color32,
    /// Axis gutter background color (price scale, time scale).
    pub axis_bg: Color32,
    /// Minor grid line stroke.
    pub grid_line: Stroke,
    /// Major grid line stroke (thicker or different color).
    pub grid_line_major: Stroke,
    /// Axis label text color.
    pub axis_text_color: Color32,
    /// Axis label font size.
    pub axis_text_size: f32,
    /// Crosshair line stroke.
    pub crosshair_line: Stroke,
    /// Crosshair value label background color.
    pub crosshair_label_bg: Color32,
    /// Crosshair value label text color.
    pub crosshair_label_text: Color32,
    /// Crosshair value label corner rounding.
    pub crosshair_label_rounding: CornerRadius,
}

impl ChartStyle {
    /// Derives chart area styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.chart.bg,
            axis_bg: semantic.chart.bg_axis,
            grid_line: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.chart.grid_line),
            grid_line_major: Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                semantic.chart.grid_line_major,
            ),
            axis_text_color: semantic.chart.axis_text,
            axis_text_size: DESIGN_TOKENS.typography.sm,
            crosshair_line: Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                semantic.chart.crosshair_line,
            ),
            crosshair_label_bg: semantic.chart.crosshair_label_bg,
            crosshair_label_text: semantic.chart.crosshair_label_text,
            crosshair_label_rounding: CornerRadius::same(DESIGN_TOKENS.rounding.sm as u8),
        }
    }
}

/// Pre-computed styling for candlestick rendering.
///
/// Defines separate fill, border, and wick colors for bullish (up) and
/// bearish (down) candles. Construct via [`CandleStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct CandleStyle {
    /// Bullish candle body fill color.
    pub bullish_fill: Color32,
    /// Bullish candle body border stroke.
    pub bullish_border: Stroke,
    /// Bullish candle wick (high-low line) stroke.
    pub bullish_wick: Stroke,
    /// Bearish candle body fill color.
    pub bearish_fill: Color32,
    /// Bearish candle body border stroke.
    pub bearish_border: Stroke,
    /// Bearish candle wick (high-low line) stroke.
    pub bearish_wick: Stroke,
    /// Minimum candle body height in pixels (to keep doji candles visible).
    pub min_body_height: f32,
}

impl CandleStyle {
    /// Derives candlestick styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bullish_fill: semantic.chart.candle_up,
            bullish_border: Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                semantic.chart.candle_up_border,
            ),
            bullish_wick: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.chart.candle_up_wick),
            bearish_fill: semantic.chart.candle_down,
            bearish_border: Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                semantic.chart.candle_down_border,
            ),
            bearish_wick: Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                semantic.chart.candle_down_wick,
            ),
            min_body_height: 1.0,
        }
    }
}

/// Pre-computed styling for volume bars.
///
/// Volume bars are colored based on the corresponding candle's direction.
/// Construct via [`VolumeStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct VolumeStyle {
    /// Volume bar color for bullish (up) candles.
    pub bullish: Color32,
    /// Volume bar color for bearish (down) candles.
    pub bearish: Color32,
    /// Corner rounding for volume bars.
    pub rounding: CornerRadius,
}

impl VolumeStyle {
    /// Derives volume bar styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bullish: semantic.chart.volume_up,
            bearish: semantic.chart.volume_down,
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.none as u8),
        }
    }
}

// ============================================================================
// DIALOG STYLE
// ============================================================================

/// Pre-computed styling for modal dialog windows.
///
/// Covers the dialog frame (background, border, shadow) and the title bar
/// (background, text, close button). Construct via [`DialogStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct DialogStyle {
    /// Dialog background color.
    pub bg: Color32,
    /// Dialog border stroke.
    pub border: Stroke,
    /// Dialog corner rounding.
    pub rounding: CornerRadius,
    /// Drop-shadow color behind the dialog.
    pub shadow_color: Color32,
    /// Title bar background color.
    pub title_bg: Color32,
    /// Title bar text color.
    pub title_text: Color32,
    /// Separator line between title bar and content.
    pub title_border: Color32,
    /// Close button background in resting state.
    pub close_button_bg: Color32,
    /// Close button background when hovered.
    pub close_button_bg_hover: Color32,
    /// Close button icon color in resting state.
    pub close_button_icon: Color32,
    /// Close button icon color when hovered.
    pub close_button_icon_hover: Color32,
}

impl DialogStyle {
    /// Derives dialog styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            bg: semantic.ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.lg as u8),
            shadow_color: Color32::from_black_alpha(100),

            title_bg: semantic.ui.panel_bg_floating,
            title_text: semantic.ui.text,
            title_border: semantic.ui.border_subtle,
            close_button_bg: Color32::TRANSPARENT,
            close_button_bg_hover: semantic.ui.btn_bg_hover,
            close_button_icon: semantic.ui.text_muted,
            close_button_icon_hover: semantic.ui.text,
        }
    }
}

// ============================================================================
// SETTINGS DIALOG STYLE
// ============================================================================

/// Pre-computed styling for the settings/preferences dialog.
///
/// Extends [`DialogStyle`] with sidebar navigation, tab states, form controls
/// (inputs, dropdowns, checkboxes, color swatches), and footer buttons.
/// Construct via [`SettingsDialogStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct SettingsDialogStyle {
    /// Base dialog frame and title bar styling.
    pub dialog: DialogStyle,

    // -- Sidebar --
    /// Sidebar navigation background.
    pub sidebar_bg: Color32,
    /// Sidebar border (separating sidebar from content).
    pub sidebar_border: Color32,

    // -- Tab states --
    /// Tab background in resting state.
    pub tab_bg: Color32,
    /// Tab background when hovered.
    pub tab_bg_hover: Color32,
    /// Tab background when active/selected.
    pub tab_bg_active: Color32,
    /// Tab text color in resting state.
    pub tab_text: Color32,
    /// Tab text color when hovered.
    pub tab_text_hover: Color32,
    /// Tab text color when active/selected.
    pub tab_text_active: Color32,
    /// Tab icon color in resting state.
    pub tab_icon: Color32,
    /// Tab icon color when hovered.
    pub tab_icon_hover: Color32,
    /// Tab icon color when active/selected.
    pub tab_icon_active: Color32,

    // -- Content area --
    /// Content area background.
    pub content_bg: Color32,
    /// Section header text color (e.g., "Appearance", "Trading").
    pub section_header_text: Color32,
    /// Form field label text color.
    pub label_text: Color32,
    /// Form field value text color.
    pub val_text: Color32,

    // -- Controls: Input --
    /// Input field background in resting state.
    pub input_bg: Color32,
    /// Input field background when hovered.
    pub input_bg_hover: Color32,
    /// Input field background when focused.
    pub input_bg_focus: Color32,
    /// Input field border in resting state.
    pub input_border: Stroke,
    /// Input field border when hovered.
    pub input_border_hover: Stroke,
    /// Input field border when focused.
    pub input_border_focus: Stroke,

    // -- Controls: Dropdown --
    /// Dropdown background in resting state.
    pub dropdown_bg: Color32,
    /// Dropdown background when hovered.
    pub dropdown_bg_hover: Color32,
    /// Dropdown border stroke.
    pub dropdown_border: Stroke,
    /// Dropdown arrow icon color.
    pub dropdown_arrow: Color32,

    // -- Controls: Checkbox --
    /// Checkbox background when unchecked.
    pub checkbox_bg: Color32,
    /// Checkbox background when checked.
    pub checkbox_bg_checked: Color32,
    /// Checkbox border stroke.
    pub checkbox_border: Stroke,
    /// Checkbox checkmark color.
    pub checkbox_check: Color32,

    // -- Controls: Color swatch --
    /// Color swatch border in resting state.
    pub swatch_border: Color32,
    /// Color swatch border when hovered.
    pub swatch_border_hover: Color32,

    // -- Footer --
    /// Footer area background.
    pub footer_bg: Color32,
    /// Footer top border.
    pub footer_border: Color32,
    /// Primary action button background.
    pub btn_primary_bg: Color32,
    /// Primary action button background when hovered.
    pub btn_primary_bg_hover: Color32,
    /// Primary action button text color.
    pub btn_primary_text: Color32,
    /// Secondary action button background.
    pub btn_secondary_bg: Color32,
    /// Secondary action button background when hovered.
    pub btn_secondary_bg_hover: Color32,
    /// Secondary action button text color.
    pub btn_secondary_text: Color32,
    /// Secondary action button border stroke.
    pub btn_secondary_border: Stroke,
}

impl SettingsDialogStyle {
    /// Derives settings dialog styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            dialog: DialogStyle::from_semantic(semantic),

            // Sidebar - use panel secondary background (adapts to light/dark)
            sidebar_bg: semantic.ui.panel_bg_secondary,
            sidebar_border: semantic.ui.border,

            // Tab states - use semantic button states
            tab_bg: Color32::TRANSPARENT,
            tab_bg_hover: semantic.ui.btn_bg_hover,
            tab_bg_active: semantic.ui.btn_bg_active,
            tab_text: semantic.ui.text_secondary,
            tab_text_hover: semantic.ui.text,
            tab_text_active: semantic.ui.text,
            tab_icon: semantic.ui.icon,
            tab_icon_hover: semantic.ui.icon_hover,
            tab_icon_active: semantic.ui.icon_active,

            // Content area - use primary panel background
            content_bg: semantic.ui.panel_bg,
            section_header_text: semantic.ui.text_muted,
            label_text: semantic.ui.text_secondary,
            val_text: semantic.ui.text,

            // Controls - Input
            input_bg: semantic.ui.btn_bg,
            input_bg_hover: semantic.ui.btn_bg_hover,
            input_bg_focus: semantic.ui.btn_bg_active,
            input_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            input_border_hover: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            input_border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, semantic.ui.border_focus),

            // Controls - Dropdown
            dropdown_bg: semantic.ui.btn_bg,
            dropdown_bg_hover: semantic.ui.btn_bg_hover,
            dropdown_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            dropdown_arrow: semantic.ui.icon,

            // Controls - Checkbox
            checkbox_bg: Color32::TRANSPARENT,
            checkbox_bg_checked: semantic.ui.accent,
            checkbox_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            checkbox_check: semantic.ui.accent_text,

            // Controls - Color swatch
            swatch_border: semantic.ui.border,
            swatch_border_hover: semantic.ui.text_muted,

            // Footer - use primary panel background
            footer_bg: semantic.ui.panel_bg,
            footer_border: semantic.ui.border,
            btn_primary_bg: semantic.ui.accent,
            btn_primary_bg_hover: semantic.ui.accent_hover,
            btn_primary_text: semantic.ui.accent_text,
            btn_secondary_bg: semantic.ui.btn_bg,
            btn_secondary_bg_hover: semantic.ui.btn_bg_hover,
            btn_secondary_text: semantic.ui.text,
            btn_secondary_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
        }
    }
}

// ============================================================================
// CONTEXT MENU STYLE
// ============================================================================

/// Pre-computed styling for context (right-click) menus.
///
/// Covers the menu frame, item states, icons, keyboard shortcut labels,
/// separators, and submenu arrows. Construct via [`ContextMenuStyle::from_semantic`].
#[derive(Clone, Copy, Debug)]
pub struct ContextMenuStyle {
    /// Menu background color.
    pub bg: Color32,
    /// Menu border stroke.
    pub border: Stroke,
    /// Menu corner rounding radius.
    pub rounding: f32,
    /// Drop-shadow color behind the menu.
    pub shadow_color: Color32,
    /// Menu item background in resting state.
    pub item_bg: Color32,
    /// Menu item background when hovered.
    pub item_bg_hover: Color32,
    /// Menu item background when pressed.
    pub item_bg_active: Color32,
    /// Menu item text color.
    pub item_text: Color32,
    /// Menu item text color when hovered.
    pub item_text_hover: Color32,
    /// Menu item text color when disabled.
    pub item_text_disabled: Color32,
    /// Icon color in resting state.
    pub icon: Color32,
    /// Icon color when hovered.
    pub icon_hover: Color32,
    /// Keyboard shortcut text color.
    pub shortcut_text: Color32,
    /// Separator line color.
    pub separator: Color32,
    /// Submenu expansion arrow color.
    pub submenu_arrow: Color32,
}

impl ContextMenuStyle {
    /// Derives context menu styling from the given semantic tokens.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            // Frame - use semantic floating panel background (adapts to light/dark)
            bg: semantic.ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, semantic.ui.border),
            rounding: DESIGN_TOKENS.rounding.md,
            shadow_color: Color32::from_black_alpha(60),

            // Menu items - use semantic button states
            item_bg: Color32::TRANSPARENT,
            item_bg_hover: semantic.ui.btn_bg_hover,
            item_bg_active: semantic.ui.btn_bg_active,
            item_text: semantic.ui.text,
            item_text_hover: semantic.ui.text,
            item_text_disabled: semantic.ui.text_disabled,

            // Icons - use semantic icon colors
            icon: semantic.ui.icon,
            icon_hover: semantic.ui.icon_hover,

            // Shortcuts - use muted text
            shortcut_text: semantic.ui.text_muted,

            // Separator - use subtle border
            separator: semantic.ui.border_subtle,

            // Submenu arrow - use icon color
            submenu_arrow: semantic.ui.icon,
        }
    }
}

// ============================================================================
// COMPLETE COMPONENT STYLES
// ============================================================================

/// Complete set of component styles pre-computed from a theme's semantic tokens.
///
/// This is the top-level style container that aggregates all component-specific
/// styles. Construct the whole set at once via [`ComponentStyles::from_semantic`].
///
/// # Architecture
///
/// ```text
/// SemanticTokens  ──>  ComponentStyles
///                      ├── btn_primary, btn_secondary, btn_ghost
///                      ├── toolbar, panel, panel_floating, menu, input
///                      ├── dialog, settings_dialog, ctx_menu
///                      └── chart, candle, volume
/// ```
#[derive(Clone, Debug)]
pub struct ComponentStyles {
    /// Primary button style (accent-colored).
    pub btn_primary: BtnStyle,
    /// Secondary button style (bordered, neutral).
    pub btn_secondary: BtnStyle,
    /// Ghost button style (transparent, icon-only).
    pub btn_ghost: BtnStyle,
    /// Toolbar styling (top bar, side bar).
    pub toolbar: ToolbarStyle,
    /// Standard panel styling.
    pub panel: PanelStyle,
    /// Floating/elevated panel styling (popovers, tooltips).
    pub panel_floating: PanelStyle,
    /// Dropdown menu styling.
    pub menu: MenuStyle,
    /// Text input field styling.
    pub input: InputStyle,
    /// General modal dialog styling.
    pub dialog: DialogStyle,
    /// Settings/preferences dialog styling.
    pub settings_dialog: SettingsDialogStyle,
    /// Context (right-click) menu styling.
    pub ctx_menu: ContextMenuStyle,
    /// Main chart area styling (grid, axes, crosshair).
    pub chart: ChartStyle,
    /// Candlestick rendering styling.
    pub candle: CandleStyle,
    /// Volume bar rendering styling.
    pub volume: VolumeStyle,
}

impl ComponentStyles {
    /// Derives all component styles from the given semantic tokens.
    ///
    /// This is typically called once when a theme is applied and the results
    /// are cached in the [`Theme`](super::Theme) struct.
    pub fn from_semantic(semantic: &SemanticTokens) -> Self {
        Self {
            btn_primary: BtnStyle::primary(semantic),
            btn_secondary: BtnStyle::secondary(semantic),
            btn_ghost: BtnStyle::ghost(semantic),
            toolbar: ToolbarStyle::from_semantic(semantic),
            panel: PanelStyle::from_semantic(semantic),
            panel_floating: PanelStyle::floating(semantic),
            menu: MenuStyle::from_semantic(semantic),
            input: InputStyle::from_semantic(semantic),
            dialog: DialogStyle::from_semantic(semantic),
            settings_dialog: SettingsDialogStyle::from_semantic(semantic),
            ctx_menu: ContextMenuStyle::from_semantic(semantic),
            chart: ChartStyle::from_semantic(semantic),
            candle: CandleStyle::from_semantic(semantic),
            volume: VolumeStyle::from_semantic(semantic),
        }
    }
}
