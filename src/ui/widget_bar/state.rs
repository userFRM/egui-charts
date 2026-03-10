//! State management for right toolbar
//!
//! Manages the state of the right sidebar panels and tabs.

use super::tabs::RightPanelTab;
use crate::tokens::DESIGN_TOKENS;

/// Right toolbar state
#[derive(Clone, Debug)]
pub struct RightToolbarState {
    /// Currently active tab
    pub active_tab: Option<RightPanelTab>,
    /// Whether the panel is expanded
    pub expanded: bool,
    /// Panel width
    pub panel_width: f32,
}

impl Default for RightToolbarState {
    fn default() -> Self {
        Self {
            active_tab: None,
            expanded: false,
            panel_width: DESIGN_TOKENS.sizing.toolbar.right_panel_width,
        }
    }
}

impl RightToolbarState {
    /// Create new right toolbar state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set active tab
    pub fn set_active_tab(&mut self, tab: Option<RightPanelTab>) {
        self.active_tab = tab;
        self.expanded = tab.is_some();
    }

    /// Toggle tab
    pub fn toggle_tab(&mut self, tab: RightPanelTab) {
        if self.active_tab == Some(tab) {
            self.active_tab = None;
            self.expanded = false;
        } else {
            self.active_tab = Some(tab);
            self.expanded = true;
        }
    }

    /// Check if tab is active
    pub fn is_tab_active(&self, tab: RightPanelTab) -> bool {
        self.active_tab == Some(tab)
    }

    /// Sync from external panel state values.
    pub fn sync_from(&mut self, panel_width: f32, active_tab: Option<RightPanelTab>) {
        self.panel_width = panel_width;
        self.active_tab = active_tab;
        self.expanded = active_tab.is_some();
    }
}
