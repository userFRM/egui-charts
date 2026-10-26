//! Domain model shared by the application UI components.
//!
//! Connection, layout, replay, and template types consumed across the
//! toolbars, panels, and dialogs of the `ui` feature.

use chrono::{DateTime, Utc};

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
// Replay (toolbar-level types used by replay/toolbar.rs)
// =============================================================================

/// Replay command for the host to apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplayCommand {
    /// Resume playback.
    Resume,
    /// Pause playback.
    Pause,
    /// Stop playback.
    Stop,
    /// Advance playback by one step.
    StepForward,
    /// Move playback back by one step.
    StepBackward,
    /// Set the playback speed.
    SetSpeed(PlaybackSpeed),
}

/// Replay playback state (toolbar-level)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolbarReplayState {
    /// Playback is advancing.
    Playing,
    /// Playback is paused.
    Paused,
    #[default]
    /// No playback session is running.
    Stopped,
    /// Playback has reached the end of the available data.
    Finished,
    /// Playback is waiting for data.
    Buffering,
    /// Playback failed.
    Error,
}

impl ToolbarReplayState {
    /// Whether a replay session is playing, paused or buffering.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Playing | Self::Paused | Self::Buffering)
    }

    /// Whether the replay can be started from this state
    pub fn can_start(&self) -> bool {
        matches!(self, Self::Stopped | Self::Finished)
    }
}

/// Playback speed for replay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackSpeed {
    #[default]
    /// Play at the original speed.
    RealTime,
    /// Play at four times the original speed.
    Fast,
    /// Play at ten times the original speed.
    VeryFast,
    /// Play at one hundred times the original speed.
    UltraFast,
}

impl PlaybackSpeed {
    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::RealTime => "1x",
            Self::Fast => "4x",
            Self::VeryFast => "10x",
            Self::UltraFast => "100x",
        }
    }

    /// Speed multiplier as f64
    pub fn multiplier(&self) -> f64 {
        match self {
            Self::RealTime => 1.0,
            Self::Fast => 4.0,
            Self::VeryFast => 10.0,
            Self::UltraFast => 100.0,
        }
    }
}

/// Progress information for replay (tick-file replay)
#[derive(Debug, Clone)]
pub struct ReplayProgress {
    /// Market time reached by playback.
    pub current_time: DateTime<Utc>,
    /// Number of bars processed so far.
    pub bars_played: usize,
    /// Total number of bars available for playback.
    pub total_bars: usize,
    /// Number of executions processed so far.
    pub ticks_processed: u64,
    /// Playback progress in percent.
    pub progress_pct: f32,
}

impl ReplayProgress {
    /// Bars played divided by total bars, or zero when the total is zero.
    ///
    /// Unlike the `progress_pct` field, this method returns a fraction, not percent.
    pub fn progress_pct(&self) -> f64 {
        if self.total_bars == 0 {
            0.0
        } else {
            self.bars_played as f64 / self.total_bars as f64
        }
    }
}

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
