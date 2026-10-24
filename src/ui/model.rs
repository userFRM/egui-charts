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
    Connected,
    Connecting,
    Reconnecting { attempt: u32 },
    Disconnected,
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
    Modern,
    Classic,
}

// =============================================================================
// Replay (toolbar-level types used by replay/toolbar.rs)
// =============================================================================

/// Command sent to the replay service
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplayCommand {
    Resume,
    Pause,
    Stop,
    StepForward,
    StepBackward,
    SetSpeed(PlaybackSpeed),
}

/// Replay playback state (toolbar-level)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolbarReplayState {
    Playing,
    Paused,
    #[default]
    Stopped,
    Finished,
    Buffering,
    Error,
}

impl ToolbarReplayState {
    /// Whether the replay is actively running (playing or buffering)
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
    RealTime,
    Fast,
    VeryFast,
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
    pub current_time: DateTime<Utc>,
    pub bars_played: usize,
    pub total_bars: usize,
    pub ticks_processed: u64,
    pub progress_pct: f32,
}

impl ReplayProgress {
    /// Get progress as a fraction (0.0 to 1.0)
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
    pub id: String,
    pub name: String,
    pub indicator_type: String,
    pub parameters: Vec<(String, f64)>,
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
