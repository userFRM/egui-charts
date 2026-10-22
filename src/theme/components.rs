//! Component Styles - Pre-computed style defaults for UI components
//!
//! This module provides ready-to-use style configurations for common components.
//! These are derived from the theme's colours and include all styling properties
//! (colors, spacing, radii, etc.) needed to render a component.

use super::Theme;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, CornerRadius, Stroke};

// ============================================================================
// DIALOG STYLE
// ============================================================================

/// Pre-computed styling for modal dialog windows.
///
/// Covers the dialog frame (background, border, shadow) and the title bar
/// (background, text, close button). Construct via [`DialogStyle::from_theme`].
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
    /// Derives dialog styling from `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            bg: ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            rounding: CornerRadius::same(DESIGN_TOKENS.rounding.lg as u8),
            shadow_color: Color32::from_black_alpha(100),

            title_bg: ui.panel_bg_floating,
            title_text: ui.text,
            title_border: ui.border,
            close_button_bg: Color32::TRANSPARENT,
            close_button_bg_hover: ui.btn_bg_hover,
            close_button_icon: ui.text_muted,
            close_button_icon_hover: ui.text,
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
/// Construct via [`SettingsDialogStyle::from_theme`].
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
    /// Derives settings dialog styling from `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            dialog: DialogStyle::from_theme(theme),

            // Sidebar - use panel secondary background (adapts to light/dark)
            sidebar_bg: ui.panel_bg_secondary,
            sidebar_border: ui.border,

            // Tab states - use semantic button states
            tab_bg: Color32::TRANSPARENT,
            tab_bg_hover: ui.btn_bg_hover,
            tab_bg_active: ui.btn_bg_active,
            tab_text: ui.text_secondary,
            tab_text_hover: ui.text,
            tab_text_active: ui.text,
            tab_icon: ui.text_secondary,
            tab_icon_hover: ui.text,
            tab_icon_active: ui.accent,

            // Content area - use primary panel background
            content_bg: ui.panel_bg,
            section_header_text: ui.text_muted,
            label_text: ui.text_secondary,
            val_text: ui.text,

            // Controls - Input
            input_bg: ui.panel_bg,
            input_bg_hover: ui.btn_bg_hover,
            input_bg_focus: ui.btn_bg_active,
            input_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            input_border_hover: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            input_border_focus: Stroke::new(DESIGN_TOKENS.stroke.thick, ui.accent),

            // Controls - Dropdown
            dropdown_bg: ui.panel_bg,
            dropdown_bg_hover: ui.btn_bg_hover,
            dropdown_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            dropdown_arrow: ui.text_secondary,

            // Controls - Checkbox
            checkbox_bg: Color32::TRANSPARENT,
            checkbox_bg_checked: ui.accent,
            checkbox_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            checkbox_check: ui.accent_text,

            // Controls - Color swatch
            swatch_border: ui.border,
            swatch_border_hover: ui.text_muted,

            // Footer - use primary panel background
            footer_bg: ui.panel_bg,
            footer_border: ui.border,
            btn_primary_bg: ui.accent,
            btn_primary_bg_hover: ui.accent_hover,
            btn_primary_text: ui.accent_text,
            btn_secondary_bg: ui.panel_bg,
            btn_secondary_bg_hover: ui.btn_bg_hover,
            btn_secondary_text: ui.text,
            btn_secondary_border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
        }
    }
}

// ============================================================================
// CONTEXT MENU STYLE
// ============================================================================

/// Pre-computed styling for context (right-click) menus.
///
/// Covers the menu frame, item states, icons, keyboard shortcut labels,
/// separators, and submenu arrows. Construct via [`ContextMenuStyle::from_theme`].
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
    /// Derives context menu styling from `theme`.
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            // Frame - use semantic floating panel background (adapts to light/dark)
            bg: ui.panel_bg_floating,
            border: Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border),
            rounding: DESIGN_TOKENS.rounding.md,
            shadow_color: Color32::from_black_alpha(60),

            // Menu items - use semantic button states
            item_bg: Color32::TRANSPARENT,
            item_bg_hover: ui.btn_bg_hover,
            item_bg_active: ui.btn_bg_active,
            item_text: ui.text,
            item_text_hover: ui.text,
            item_text_disabled: ui.text_disabled,

            // Icons - use secondary text
            icon: ui.text_secondary,
            icon_hover: ui.text,

            // Shortcuts - use muted text
            shortcut_text: ui.text_muted,

            // Separator - use border
            separator: ui.border,

            // Submenu arrow - use icon color
            submenu_arrow: ui.text_secondary,
        }
    }
}
