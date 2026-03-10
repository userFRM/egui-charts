//! State management for top toolbar.
//!
//! Separates mutable state from configuration.

/// Top toolbar state
///
/// Note: Symbol, interval, and chart style are NOT stored here.
/// They live in AppState.chart and are accessed directly during rendering.
#[derive(Clone, Debug, Default)]
pub struct TopToolbarState {
    /// Whether replay mode is active
    pub replay_active: bool,
}

impl TopToolbarState {
    /// Create new state with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle replay mode
    pub fn toggle_replay(&mut self) {
        self.replay_active = !self.replay_active;
    }

    /// Sync from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.replay_active = app_state.is_replay_active();
    }
}
