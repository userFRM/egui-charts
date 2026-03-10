//! Replay actions
//!
//! User actions for controlling historical data replay/playback.

use chrono::{DateTime, Utc};

/// Actions that can be performed during replay mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ReplayAction {
    /// No action
    #[default]
    None,

    // Playback Controls
    /// Start playing from current position
    Play,
    /// Pause playback
    Pause,
    /// Toggle play/pause
    TogglePlayPause,
    /// Stop playback and reset to beginning
    Stop,
    /// Reset to initial state without stopping mode
    Reset,

    // Navigation
    /// Move forward one bar
    StepForward,
    /// Move backward one bar
    StepBackward,
    /// Move forward by specified number of bars
    StepForwardN(usize),
    /// Move backward by specified number of bars
    StepBackwardN(usize),
    /// Jump to specific bar index (0-based)
    JumpToBar(usize),
    /// Jump to specific date/time
    JumpToDate(DateTime<Utc>),
    /// Jump to beginning
    JumpToStart,
    /// Jump to end
    JumpToEnd,
    /// Jump to percentage position (0.0 - 1.0)
    JumpToPercent(f32),

    // Speed Control
    /// Set playback speed multiplier (0.1x - 100x)
    SetSpeed(f32),
    /// Increase speed by one preset level
    SpeedUp,
    /// Decrease speed by one preset level
    SlowDown,

    // Mode Control
    /// Enter replay mode
    EnterReplayMode,
    /// Exit replay mode
    ExitReplayMode,
    /// Toggle replay mode on/off
    ToggleReplayMode,

    // Data Loading
    /// Load replay data from source
    LoadData {
        symbol: String,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    },
    /// Unload current replay data
    UnloadData,

    // Range Selection
    /// Set replay start date
    SetStartDate(DateTime<Utc>),
    /// Set replay end date
    SetEndDate(DateTime<Utc>),
    /// Set both start and end dates
    SetDateRange {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },

    // Markers and Annotations
    /// Add marker at current position
    AddMarker {
        label: Option<String>,
        color: Option<[u8; 4]>,
    },
    /// Remove marker at bar index
    RemoveMarker(usize),
    /// Clear all markers
    ClearMarkers,

    // Trading Simulation
    /// Enable trading simulation during replay
    EnableTradingSimulation,
    /// Disable trading simulation
    DisableTradingSimulation,
    /// Reset trading simulation state
    ResetTradingSimulation,
}

impl ReplayAction {
    /// Check if this action affects playback state
    pub fn affects_playback(&self) -> bool {
        matches!(
            self,
            ReplayAction::Play
                | ReplayAction::Pause
                | ReplayAction::TogglePlayPause
                | ReplayAction::Stop
                | ReplayAction::SetSpeed(_)
                | ReplayAction::SpeedUp
                | ReplayAction::SlowDown
        )
    }

    /// Check if this action affects position
    pub fn affects_pos(&self) -> bool {
        matches!(
            self,
            ReplayAction::StepForward
                | ReplayAction::StepBackward
                | ReplayAction::StepForwardN(_)
                | ReplayAction::StepBackwardN(_)
                | ReplayAction::JumpToBar(_)
                | ReplayAction::JumpToDate(_)
                | ReplayAction::JumpToStart
                | ReplayAction::JumpToEnd
                | ReplayAction::JumpToPercent(_)
        )
    }

    /// Check if this action affects mode
    pub fn affects_mode(&self) -> bool {
        matches!(
            self,
            ReplayAction::EnterReplayMode
                | ReplayAction::ExitReplayMode
                | ReplayAction::ToggleReplayMode
        )
    }
}

/// Predefined speed presets for replay
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ReplaySpeed {
    /// 0.1x speed (very slow)
    Slowest,
    /// 0.25x speed
    Slower,
    /// 0.5x speed
    Slow,
    /// 1x speed (real-time)
    #[default]
    Normal,
    /// 2x speed
    Fast,
    /// 5x speed
    Faster,
    /// 10x speed
    VeryFast,
    /// 25x speed
    Fastest,
    /// 50x speed
    Turbo,
    /// 100x speed (max)
    Max,
    /// Custom speed value
    Custom(f32),
}

impl ReplaySpeed {
    /// Get the speed multiplier value
    pub fn multiplier(&self) -> f32 {
        match self {
            ReplaySpeed::Slowest => 0.1,
            ReplaySpeed::Slower => 0.25,
            ReplaySpeed::Slow => 0.5,
            ReplaySpeed::Normal => 1.0,
            ReplaySpeed::Fast => 2.0,
            ReplaySpeed::Faster => 5.0,
            ReplaySpeed::VeryFast => 10.0,
            ReplaySpeed::Fastest => 25.0,
            ReplaySpeed::Turbo => 50.0,
            ReplaySpeed::Max => 100.0,
            ReplaySpeed::Custom(v) => *v,
        }
    }

    /// Get the next faster preset
    pub fn faster(&self) -> Self {
        match self {
            ReplaySpeed::Slowest => ReplaySpeed::Slower,
            ReplaySpeed::Slower => ReplaySpeed::Slow,
            ReplaySpeed::Slow => ReplaySpeed::Normal,
            ReplaySpeed::Normal => ReplaySpeed::Fast,
            ReplaySpeed::Fast => ReplaySpeed::Faster,
            ReplaySpeed::Faster => ReplaySpeed::VeryFast,
            ReplaySpeed::VeryFast => ReplaySpeed::Fastest,
            ReplaySpeed::Fastest => ReplaySpeed::Turbo,
            ReplaySpeed::Turbo | ReplaySpeed::Max => ReplaySpeed::Max,
            ReplaySpeed::Custom(v) => ReplaySpeed::Custom((*v * 2.0).min(100.0)),
        }
    }

    /// Get the next slower preset
    pub fn slower(&self) -> Self {
        match self {
            ReplaySpeed::Max => ReplaySpeed::Turbo,
            ReplaySpeed::Turbo => ReplaySpeed::Fastest,
            ReplaySpeed::Fastest => ReplaySpeed::VeryFast,
            ReplaySpeed::VeryFast => ReplaySpeed::Faster,
            ReplaySpeed::Faster => ReplaySpeed::Fast,
            ReplaySpeed::Fast => ReplaySpeed::Normal,
            ReplaySpeed::Normal => ReplaySpeed::Slow,
            ReplaySpeed::Slow => ReplaySpeed::Slower,
            ReplaySpeed::Slower | ReplaySpeed::Slowest => ReplaySpeed::Slowest,
            ReplaySpeed::Custom(v) => ReplaySpeed::Custom((*v / 2.0).max(0.1)),
        }
    }

    /// Get display label for the speed
    pub fn label(&self) -> String {
        match self {
            ReplaySpeed::Custom(v) => format!("{v:.1}x"),
            _ => format!("{:.1}x", self.multiplier()),
        }
    }

    /// All standard presets
    pub fn presets() -> &'static [ReplaySpeed] {
        &[
            ReplaySpeed::Slowest,
            ReplaySpeed::Slower,
            ReplaySpeed::Slow,
            ReplaySpeed::Normal,
            ReplaySpeed::Fast,
            ReplaySpeed::Faster,
            ReplaySpeed::VeryFast,
            ReplaySpeed::Fastest,
            ReplaySpeed::Turbo,
            ReplaySpeed::Max,
        ]
    }
}

impl From<f32> for ReplaySpeed {
    fn from(value: f32) -> Self {
        // Try to match a preset
        for preset in Self::presets() {
            if (preset.multiplier() - value).abs() < 0.01 {
                return *preset;
            }
        }
        ReplaySpeed::Custom(value)
    }
}
