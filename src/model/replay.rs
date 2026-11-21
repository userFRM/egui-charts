//! A replay checkpoint and the charts a replay covers, as plain data, for the settings store and
//! the host's storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A resumable position in host-supplied history and the charts it covers. Store it with the
/// symbol's source interval and fetch that source before restoring it. Restoring always pauses
/// playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReplaySession {
    /// Source symbol, checked before restoration.
    pub symbol: String,
    /// Opening time of the last revealed source bar.
    pub time: DateTime<Utc>,
    /// Playback speed multiplier, validated when restoring.
    pub speed: f32,
    /// Charts of a layout the replay covers. Sessions saved without it cover the chart that
    /// saved them.
    #[serde(default)]
    pub scope: ReplayScope,
}

/// The charts of a layout that a replay covers, whatever their link groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReplayScope {
    /// The chart that starts the replay, alone.
    #[default]
    ThisChart,
    /// Every chart of the layout, hidden ones included.
    AllCharts,
}
