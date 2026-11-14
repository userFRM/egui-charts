//! How a bars pattern draws its copy of the source bars.

use serde::{Deserialize, Serialize};

/// How a bars pattern draws each copied bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BarsPatternMode {
    /// An OHLC bar: a line from the high to the low, the open ticked left and the close right.
    #[default]
    Bars,
    /// A line through the closes.
    Line,
}

/// How a bars pattern copies its source bars. Drawings saved without one use the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct BarsPatternConfig {
    /// How each copied bar is drawn.
    pub mode: BarsPatternMode,
    /// Whether the copy runs backwards in time: its first bar is the last source bar, and each
    /// bar's open and close trade places.
    pub mirrored: bool,
    /// Whether the copy is turned upside down around the middle of the source bars' range, so
    /// that it keeps that range.
    pub flipped: bool,
}
