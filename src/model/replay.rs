//! A replay checkpoint and the charts a replay covers, as plain data, for the settings store and
//! the host's storage.

use super::Timeframe;
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
    /// The charts the replay covers, as they were when it was saved: the chart the replay runs on
    /// first, then the others in slot order. `ReplayController::session` leaves it empty for the
    /// host to fill from its layout; the host puts each chart back on its symbol and interval
    /// before restoring the session. Sessions saved without it restore on the charts as they are.
    #[serde(default)]
    pub charts: Vec<ReplayChart>,
}

/// A chart a replay covers, as a saved [`ReplaySession`] records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReplayChart {
    /// The chart's slot in its layout, numbered as `ChartLayout` numbers its slots.
    pub slot: usize,
    /// The symbol the chart showed.
    pub symbol: String,
    /// The chart's own interval, which can differ from the update interval the replay steps by.
    pub interval: Timeframe,
}

impl ReplayChart {
    /// A chart in `slot` showing `symbol` at `interval`.
    pub fn new(slot: usize, symbol: impl Into<String>, interval: Timeframe) -> Self {
        Self {
            slot,
            symbol: symbol.into(),
            interval,
        }
    }
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
