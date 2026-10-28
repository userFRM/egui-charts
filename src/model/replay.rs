//! A replay checkpoint as plain data, for the settings store and the host's storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A resumable position in host-supplied history. Store it with the symbol's source interval
/// and fetch that same source before restoring it. Restoring always pauses playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplaySession {
    /// Source symbol, checked before restoration.
    pub symbol: String,
    /// Opening time of the last revealed source bar.
    pub time: DateTime<Utc>,
    /// Playback speed multiplier, validated when restoring.
    pub speed: f32,
}
