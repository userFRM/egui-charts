//! Actions that can be returned from right toolbar interactions

use super::tabs::RightPanelTab;

/// Actions returned from right toolbar interactions
#[derive(Debug, Clone, PartialEq)]
pub enum RightToolbarAction {
    /// No action taken
    None,
    /// Tab was selected
    SelectTab(RightPanelTab),
    /// Tab was closed
    CloseTab,
    /// Panel width changed
    ResizePanel(f32),
}
