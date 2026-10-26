//! Top toolbar selection, visibility and interaction state.
//!
//! Keep [`TopToolbarState`] across frames when using the toolbar in a host layout.

/// Top toolbar state
///
/// Note: Symbol, interval, and chart style are NOT stored here.
/// They live in AppState.chart and are accessed directly during rendering.
#[derive(Clone, Debug)]
pub struct TopToolbarState {
    /// Whether replay mode is active
    pub replay_active: bool,
    /// Whether there is an action to undo; the undo button is disabled while this is `false`.
    pub can_undo: bool,
    /// Whether there is an undone action to redo; the redo button is disabled while this is `false`.
    pub can_redo: bool,
    /// Whether a broker is attached; the Trade button is hidden while this is `false`.
    pub can_trade: bool,
}

impl Default for TopToolbarState {
    fn default() -> Self {
        Self {
            replay_active: false,
            can_undo: true,
            can_redo: true,
            can_trade: false,
        }
    }
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
