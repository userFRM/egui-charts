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
    /// Stroke width in points. `None` inherits the drawing width, as older files do.
    #[serde(default)]
    pub width: Option<f32>,
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
    /// The median in red and nine levels in their own colours, of which 0.5 and 1 are shown,
    /// with bands at 20%.
    fn default() -> Self {
        let level = |value, color, visible| PitchforkLevel {
            value,
            visible,
            color,
            line_style: LineStyle::Solid,
            width: None,
        };
        Self {
            levels: vec![
                level(0.0, [242, 54, 69, 255], true),
                level(0.25, [255, 183, 77, 255], false),
                level(0.382, [129, 199, 132, 255], false),
                level(0.5, [8, 153, 129, 255], true),
                level(0.618, [8, 153, 129, 255], false),
                level(0.75, [0, 188, 212, 255], false),
                level(1.0, [41, 98, 255, 255], true),
                level(1.5, [156, 39, 176, 255], false),
                level(1.75, [233, 30, 99, 255], false),
                level(2.0, [247, 124, 128, 255], false),
            ],
            show_background: true,
            background_opacity: 51,
        }
    }
}

impl PitchforkConfig {
    /// The levels of pitchforks saved without a config: 0, 0.5, 1 and 1.5, all shown, with
    /// faint bands. Renderers draw them in the drawing colour.
    pub(crate) fn legacy() -> Self {
        Self {
            levels: [0.0, 0.5, 1.0, 1.5]
                .map(|value| PitchforkLevel {
                    value,
                    visible: true,
                    color: [33, 150, 243, 255],
                    line_style: LineStyle::Solid,
                    width: None,
                })
                .to_vec(),
            show_background: true,
            background_opacity: 20,
        }
    }
}
