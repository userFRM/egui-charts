//! Replay types shared with the host: commands, playback state, speed and progress.

use chrono::{DateTime, Utc};

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
