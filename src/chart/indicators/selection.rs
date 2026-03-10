//! Indicator selection state.
//!
//! Tracks which overlay indicator is currently selected for interactive editing.

/// Unique numeric identifier for an indicator (index in the indicator registry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IndicatorId(pub usize);

impl IndicatorId {
    /// Get display name based on ID (for debugging)
    pub fn debug_name(&self) -> String {
        format!("Indicator({})", self.0)
    }
}

/// Tracks which overlay indicator is currently selected or hovered.
///
/// Used by the chart to render selection dots on the active indicator line
/// and to show context menus or settings panels.
#[derive(Clone, Debug, Default)]
pub struct IndicatorSelectionState {
    /// Currently selected indicator (by index in registry)
    pub selected_indicator: Option<IndicatorId>,
    /// Selected line index within the indicator (for multi-line indicators)
    pub selected_line_idx: Option<usize>,
    /// Currently hovered indicator (desktop only)
    pub hovered_indicator: Option<IndicatorId>,
    /// Bar index where selection occurred
    pub selected_bar_idx: Option<usize>,
}

impl IndicatorSelectionState {
    /// Create new selection state
    pub fn new() -> Self {
        Self::default()
    }

    /// Select an indicator
    pub fn select(
        &mut self,
        indicator_id: IndicatorId,
        line_idx: Option<usize>,
        bar_idx: Option<usize>,
    ) {
        self.selected_indicator = Some(indicator_id);
        self.selected_line_idx = line_idx;
        self.selected_bar_idx = bar_idx;
    }

    /// Deselect all indicators
    pub fn deselect(&mut self) {
        self.selected_indicator = None;
        self.selected_line_idx = None;
        self.selected_bar_idx = None;
    }

    /// Set hovered indicator
    pub fn set_hovered(&mut self, indicator_id: Option<IndicatorId>) {
        self.hovered_indicator = indicator_id;
    }

    /// Check if a specific indicator is selected
    pub fn is_selected(&self, indicator_id: IndicatorId) -> bool {
        self.selected_indicator == Some(indicator_id)
    }

    /// Check if any indicator is selected
    pub fn has_selection(&self) -> bool {
        self.selected_indicator.is_some()
    }

    /// Get the currently selected indicator ID
    pub fn selected(&self) -> Option<IndicatorId> {
        self.selected_indicator
    }

    /// Get the selected line index
    pub fn selected_line(&self) -> Option<usize> {
        self.selected_line_idx
    }
}
