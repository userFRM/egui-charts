//! Domain model shared by the application UI components.
//!
//! Connection, layout, replay, and template types consumed across the
//! toolbars, panels, and dialogs of the `ui` feature.

// =============================================================================
// Connection
// =============================================================================

/// Connection state for WebSocket or data provider
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// The host reports an established connection.
    Connected,
    /// A connection attempt is in progress.
    Connecting,
    /// The host is retrying the connection.
    Reconnecting {
        /// Reconnect attempt number supplied by the host.
        attempt: u32,
    },
    /// The connection is closed.
    Disconnected,
    /// Connection failure with a host-supplied message.
    Failed(String),
}

// =============================================================================
// Chart Layout
// =============================================================================

pub use crate::widget::ChartLayoutMode;

// =============================================================================
// Layout Style
// =============================================================================

/// UI layout style variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutStyle {
    /// Modern layout styling.
    Modern,
    /// Classic layout styling.
    Classic,
}

// =============================================================================
// Replay
// =============================================================================

pub use super::replay::model::{PlaybackSpeed, ReplayCommand, ReplayProgress, ToolbarReplayState};

// =============================================================================
// Templates
// =============================================================================

/// An indicator parameter template
#[derive(Debug, Clone)]
pub struct IndicatorTemplate {
    /// Host identifier for this record.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Study identifier used by the host for this template.
    pub indicator_type: String,
    /// Named numeric parameter values supplied by the host.
    pub parameters: Vec<(String, f64)>,
    /// Host-reported sharing state; the host manages permissions.
    pub is_shared: bool,
}

impl IndicatorTemplate {
    /// Summary of parameter values
    pub fn parameter_summary(&self) -> String {
        if self.parameters.is_empty() {
            "Default".to_string()
        } else {
            self.parameters
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}
