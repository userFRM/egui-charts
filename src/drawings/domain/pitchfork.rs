//! Saved level and background settings for the pitchfork tools.

use crate::model::LineStyle;
use serde::{Deserialize, Serialize};

/// A multiple of the median-to-tine distance, mirrored on both sides of the median.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchforkLevel {
    /// Distance multiple: 0 is the median and 1 is the pair of tines.
    pub value: f32,
    /// Whether the level and its adjacent bands are drawn.
    pub visible: bool,
    /// Line colour as RGBA.
    pub color: [u8; 4],
    /// Stroke style of this level.
    pub line_style: LineStyle,
}

/// The pitchfork's level lines and the fill between adjacent levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchforkConfig {
    /// Levels, ordered by distance when rendered; negative and non-finite values are ignored.
    pub levels: Vec<PitchforkLevel>,
    /// Whether bands between adjacent visible levels are filled.
    pub show_background: bool,
    /// Band opacity, from 0 to 255.
    pub background_opacity: u8,
}

impl Default for PitchforkConfig {
    fn default() -> Self {
        Self {
            levels: [0.0, 0.5, 1.0, 1.5]
                .map(|value| PitchforkLevel {
                    value,
                    visible: true,
                    color: [33, 150, 243, 255],
                    line_style: LineStyle::Solid,
                })
                .to_vec(),
            show_background: true,
            background_opacity: 20,
        }
    }
}
