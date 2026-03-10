//! Stub types for UI components
//!
//! These are placeholder types that allow the UI modules to compile
//! independently of the frontend application. When integrating with
//! a real application, replace these with actual implementations.

use chrono::{DateTime, Utc};
use std::fmt;

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

/// Multi-chart grid layout mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartLayoutMode {
    Single,
    TwoHorizontal,
    TwoVertical,
    Three,
    Four,
    Six,
}

impl ChartLayoutMode {
    /// All available layout modes
    pub fn all() -> &'static [ChartLayoutMode] {
        &[
            Self::Single,
            Self::TwoHorizontal,
            Self::TwoVertical,
            Self::Three,
            Self::Four,
            Self::Six,
        ]
    }

    /// Human-readable label
    pub fn label(&self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::TwoHorizontal => "2 Horizontal",
            Self::TwoVertical => "2 Vertical",
            Self::Three => "3 Charts",
            Self::Four => "2x2 Grid",
            Self::Six => "3x2 Grid",
        }
    }

    /// Grid dimensions (columns, rows)
    pub fn grid_size(&self) -> (usize, usize) {
        match self {
            Self::Single => (1, 1),
            Self::TwoHorizontal => (2, 1),
            Self::TwoVertical => (1, 2),
            Self::Three => (2, 2),
            Self::Four => (2, 2),
            Self::Six => (3, 2),
        }
    }
}

impl fmt::Display for ChartLayoutMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

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
// Alerts
// =============================================================================

/// Alert status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertStatus {
    Active,
    Triggered,
    Disabled,
    Expired,
}

/// Price alert condition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertCondition {
    CrossesAbove,
    CrossesBelow,
    Above,
    Below,
}

impl AlertCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CrossesAbove => "Crosses Above",
            Self::CrossesBelow => "Crosses Below",
            Self::Above => "Above",
            Self::Below => "Below",
        }
    }
}

/// Indicator alert condition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorCondition {
    CrossAbove,
    CrossBelow,
    Above,
    Below,
}

impl IndicatorCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CrossAbove => "Crosses Above",
            Self::CrossBelow => "Crosses Below",
            Self::Above => "Above",
            Self::Below => "Below",
        }
    }
}

/// A price alert
#[derive(Debug, Clone)]
pub struct PriceAlert {
    pub id: usize,
    pub symbol: String,
    pub target_price: f64,
    pub condition: AlertCondition,
    pub status: AlertStatus,
    pub message: Option<String>,
    pub repeating: bool,
}

/// An indicator-based alert
#[derive(Debug, Clone)]
pub struct IndicatorAlert {
    pub id: usize,
    pub indicator_a: String,
    pub indicator_b: Option<String>,
    pub threshold: Option<f64>,
    pub condition: IndicatorCondition,
    pub symbol: Option<String>,
    pub status: AlertStatus,
}

/// A volume spike alert
#[derive(Debug, Clone)]
pub struct VolumeAlert {
    pub id: usize,
    pub symbol: String,
    pub spike_multiplier: f64,
    pub lookback_periods: usize,
    pub status: AlertStatus,
}

/// A calendar/economic event alert
#[derive(Debug, Clone)]
pub struct EventAlert {
    pub id: usize,
    pub event_type: String,
    pub minutes_before: u32,
    pub min_impact: String,
    pub status: AlertStatus,
}

/// Manages all alert types
#[derive(Debug, Clone, Default)]
pub struct AlertManager {
    price_alerts: Vec<PriceAlert>,
    indicator_alerts: Vec<IndicatorAlert>,
    volume_alerts: Vec<VolumeAlert>,
    event_alerts: Vec<EventAlert>,
}

impl AlertManager {
    pub fn price_alerts(&self) -> &[PriceAlert] {
        &self.price_alerts
    }

    pub fn indicator_alerts(&self) -> &[IndicatorAlert] {
        &self.indicator_alerts
    }

    pub fn volume_alerts(&self) -> &[VolumeAlert] {
        &self.volume_alerts
    }

    pub fn event_alerts(&self) -> &[EventAlert] {
        &self.event_alerts
    }
}

// =============================================================================
// Templates
// =============================================================================

/// Manages chart settings templates
#[derive(Debug, Clone, Default)]
pub struct TemplateManager {
    templates: Vec<SettingsTemplate>,
}

impl TemplateManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn template_cnt(&self) -> usize {
        self.templates.len()
    }

    pub fn save_template(
        &mut self,
        name: &str,
        settings: crate::ui::top_toolbar::components::settings_menu::data::ChartSettingsState,
    ) {
        // Remove existing template with same name
        self.templates.retain(|t| t.name != name);
        self.templates.push(SettingsTemplate {
            name: name.to_string(),
            settings,
        });
    }

    pub fn get_template(&self, name: &str) -> Option<&SettingsTemplate> {
        self.templates.iter().find(|t| t.name == name)
    }

    pub fn list_templates(&self) -> Vec<&str> {
        self.templates.iter().map(|t| t.name.as_str()).collect()
    }
}

/// A saved chart settings template
#[derive(Debug, Clone)]
pub struct SettingsTemplate {
    pub name: String,
    pub settings: crate::ui::top_toolbar::components::settings_menu::data::ChartSettingsState,
}

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
