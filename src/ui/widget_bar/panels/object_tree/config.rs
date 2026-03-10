//! Configuration for the Object Tree panel
//!
//! Provides customization options for display and behavior.

use crate::tokens::DESIGN_TOKENS;

/// Configuration for the Object Tree panel
#[derive(Clone, Debug)]
pub struct ObjectTreeConfig {
    // === Display Options ===
    /// Show the Data Window section
    pub show_data_window: bool,
    /// Show hidden objects in the tree
    pub show_hidden_objects: bool,
    /// Show locked indicator icon
    pub show_lock_indicator: bool,
    /// Show color indicators next to items
    pub show_color_indicators: bool,
    /// Group objects by type (indicators, drawings)
    pub group_by_type: bool,
    /// Show item count badges on group headers
    pub show_item_counts: bool,
    /// Show expand/collapse chevron for items with properties
    pub show_expand_chevron: bool,

    // === Behavior Options ===
    /// Enable drag-and-drop reordering
    pub enable_drag_drop: bool,
    /// Enable multi-selection (Ctrl+click, Shift+click)
    pub enable_multi_select: bool,
    /// Enable inline property editing
    pub enable_inline_edit: bool,
    /// Enable context menu on right-click
    pub enable_context_menu: bool,
    /// Enable keyboard shortcuts
    pub enable_keyboard_shortcuts: bool,

    // === Dimensions ===
    /// Row height for tree items
    pub row_height: f32,
    /// Indent width for nested items
    pub indent_width: f32,
    /// Maximum panel height (for scroll area)
    pub max_height: f32,
    /// Icon size in rows
    pub icon_size: f32,
    /// Color indicator size
    pub color_indicator_size: f32,
    /// Toggle button size
    pub toggle_size: f32,

    // === Animation ===
    /// Animate expand/collapse transitions
    pub animate_transitions: bool,
}

impl Default for ObjectTreeConfig {
    fn default() -> Self {
        Self {
            // Display
            show_data_window: true,
            show_hidden_objects: true,
            show_lock_indicator: true,
            show_color_indicators: true,
            group_by_type: true,
            show_item_counts: true,
            show_expand_chevron: true,

            // Behavior
            enable_drag_drop: true,
            enable_multi_select: true,
            enable_inline_edit: false, // Uses dialogs instead
            enable_context_menu: true,
            enable_keyboard_shortcuts: true,

            // Dimensions
            row_height: DESIGN_TOKENS.sizing.button_sm, // 24px
            indent_width: DESIGN_TOKENS.spacing.xl,     // 12px
            max_height: 400.0,
            icon_size: 16.0,
            color_indicator_size: 10.0,
            toggle_size: 16.0,

            // Animation
            animate_transitions: true,
        }
    }
}

impl ObjectTreeConfig {
    /// Create a minimal configuration (no extra features)
    pub fn minimal() -> Self {
        Self {
            show_data_window: false,
            show_hidden_objects: false,
            show_lock_indicator: false,
            show_color_indicators: false,
            group_by_type: false,
            show_item_counts: false,
            show_expand_chevron: false,
            enable_drag_drop: false,
            enable_multi_select: false,
            enable_inline_edit: false,
            enable_context_menu: false,
            enable_keyboard_shortcuts: false,
            animate_transitions: false,
            ..Default::default()
        }
    }

    /// Create a compact configuration
    pub fn compact() -> Self {
        Self {
            row_height: 20.0,
            indent_width: 8.0,
            icon_size: 14.0,
            color_indicator_size: 8.0,
            toggle_size: 14.0,
            ..Default::default()
        }
    }
}
