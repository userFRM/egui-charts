//! State management for drawing toolbar.
//!
//! This module contains all state-related logic separated from rendering.
//! Pure state mutations with no UI coupling.

use super::DrawingTemplate;
use super::categories::cursors::CursorType;
use super::data;
use crate::drawings::DrawingToolType;
use std::collections::{HashMap, VecDeque};

/// Types of magnet snapping behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagnetType {
    /// Snap only within the configured pixel threshold
    Weak,
    /// Snap to the nearest OHLC value of the bar under the pointer
    Strong,
    /// Snap to OHLC price values, with the same behavior as strong magnet
    OHLC,
}

/// Drawing toolbar state
#[derive(Clone)]
pub struct ToolbarState {
    /// Currently selected tool
    pub sel_tool: Option<DrawingToolType>,
    /// Whether the toolbar is expanded (shows names)
    pub expanded: bool,
    /// Currently expanded category (for nested menus)
    pub expanded_category: Option<String>,
    /// Favorite tools
    pub favorites: Vec<DrawingToolType>,
    /// Recently used tools (max 5)
    pub recent: VecDeque<DrawingToolType>,
    /// Whether drawing anchors snap to nearby chart values and drawing points
    pub magnet_mode: bool,
    /// Type of magnet snapping when magnet mode is enabled
    pub magnet_type: MagnetType,
    /// Stay in drawing mode after placing tool
    pub stay_in_drawing_mode: bool,
    /// Currently selected drawing color
    pub drawing_color: [u8; 4],
    /// Whether to show the color picker popup
    pub show_color_picker: bool,
    /// Last selected tool per category (remembers which tool was last used in each category)
    pub category_last_tool: HashMap<String, DrawingToolType>,
    /// Whether the tool search bar is open
    pub search_open: bool,
    /// Current search query
    pub search_query: String,
    /// Glyph picker search, size and session recents.
    pub glyph_picker: super::GlyphPicker,
    /// Whether to show values tooltip on long press
    pub values_tooltip_on_long_press: bool,
    /// Whether eraser mode is currently active
    pub eraser_mode_enabled: bool,
    /// Whether all drawings are locked; the lock button shows it.
    pub lock_all: bool,
    /// Whether the Hide menu has hidden drawings; the menu shows a check on each hidden group.
    pub hide_drawings: bool,
    /// Whether the Hide menu has hidden indicators.
    pub hide_indicators: bool,
    /// Whether the Hide menu has hidden positions.
    pub hide_positions: bool,
    /// Whether the Hide menu has hidden orders.
    pub hide_orders: bool,
    /// Whether zoom history exists (for showing/hiding Zoom Out button)
    pub has_zoom_history: bool,
    /// Whether zoom-in mode is currently active (for button highlighting)
    pub zoom_mode_active: bool,
    /// Current cursor type (Cross, Dot, Arrow) - Arrow is default
    pub current_cursor_type: CursorType,
    /// Cached drawing templates loaded from the API
    pub templates: Vec<DrawingTemplate>,
    /// Whether the template name input dialog is showing
    pub show_template_name_input: bool,
    /// Current template name being entered
    pub template_name_input: String,
}

impl Default for ToolbarState {
    fn default() -> Self {
        let mut category_last_tool = HashMap::new();
        // Initialize with first tool from each category
        category_last_tool.insert("Lines".to_string(), DrawingToolType::TrendLine);
        category_last_tool.insert(
            "Fibonacci".to_string(),
            DrawingToolType::FibonacciRetracement,
        );
        category_last_tool.insert("Patterns".to_string(), DrawingToolType::XABCDPattern);
        category_last_tool.insert("Projection".to_string(), DrawingToolType::LongPos);
        category_last_tool.insert("Brushes/Shapes".to_string(), DrawingToolType::Brush);
        category_last_tool.insert("Text/Annotations".to_string(), DrawingToolType::TextLabel);

        Self {
            sel_tool: None,
            expanded: true,
            expanded_category: None,
            favorites: vec![
                DrawingToolType::TrendLine,
                DrawingToolType::HorizontalLine,
                DrawingToolType::FibonacciRetracement,
            ],
            recent: VecDeque::with_capacity(5),
            magnet_mode: false,
            magnet_type: MagnetType::Strong, // Default to strong magnet
            stay_in_drawing_mode: false,
            drawing_color: [242, 54, 69, 255], // Default red
            show_color_picker: false,
            category_last_tool,
            search_open: false,
            search_query: String::new(),
            glyph_picker: Default::default(),
            values_tooltip_on_long_press: true, // Default: enabled
            eraser_mode_enabled: false,
            lock_all: false,
            hide_drawings: false,
            hide_indicators: false,
            hide_positions: false,
            hide_orders: false,
            has_zoom_history: false,
            zoom_mode_active: false,
            current_cursor_type: CursorType::Arrow, // Default: Arrow
            templates: Vec::new(),
            show_template_name_input: false,
            template_name_input: String::new(),
        }
    }
}

impl ToolbarState {
    /// Create new toolbar state with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a drawing tool and add to recent
    pub fn select_tool(&mut self, tool: DrawingToolType) {
        self.sel_tool = Some(tool);
        self.eraser_mode_enabled = false;
        self.zoom_mode_active = false;
        self.add_to_recent(tool);

        // Update last selected tool for this tool's category
        if let Some(category) = data::get_tool_category(tool) {
            self.category_last_tool.insert(category.to_string(), tool);
        }
    }

    /// Add a tool to recent list
    pub fn add_to_recent(&mut self, tool: DrawingToolType) {
        // Remove if already present
        self.recent.retain(|t| *t != tool);
        // Add to front
        self.recent.push_front(tool);
        // Keep max 5
        while self.recent.len() > 5 {
            self.recent.pop_back();
        }
    }

    /// Clear tool selection (cursor mode)
    pub fn clear_selection(&mut self) {
        self.sel_tool = None;
        self.zoom_mode_active = false;
    }

    /// Toggle favorite status of a tool
    pub fn toggle_favorite(&mut self, tool: DrawingToolType) {
        if self.favorites.contains(&tool) {
            self.favorites.retain(|t| *t != tool);
        } else {
            self.favorites.push(tool);
        }
    }

    /// Check if tool is favorited
    pub fn is_favorite(&self, tool: &DrawingToolType) -> bool {
        self.favorites.contains(tool)
    }

    /// Toggle magnet mode
    pub fn toggle_magnet(&mut self) {
        self.magnet_mode = !self.magnet_mode;
    }

    /// Toggle stay-in-drawing mode
    pub fn toggle_stay_in_drawing(&mut self) {
        self.stay_in_drawing_mode = !self.stay_in_drawing_mode;
    }

    /// Sync toolbar state from external drawing state.
    pub fn sync_from(
        &mut self,
        active_tool: Option<DrawingToolType>,
        magnet_mode: bool,
        stay_in_drawing_mode: bool,
        eraser_mode: bool,
        drawing_color: [u8; 4],
    ) {
        self.sel_tool = active_tool;
        self.magnet_mode = magnet_mode;
        self.stay_in_drawing_mode = stay_in_drawing_mode;
        self.eraser_mode_enabled = eraser_mode;
        self.drawing_color = drawing_color;
    }
}
