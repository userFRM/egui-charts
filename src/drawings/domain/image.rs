//! Size and opacity of the image tool's frame.

use serde::{Deserialize, Serialize};

/// Size and opacity of an [`Image`](crate::drawings::DrawingToolType::Image) drawing's frame.
///
/// The frame's top-left corner is the drawing's anchor. Its size is in points, so the
/// picture keeps its size and proportions when the chart zooms or scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ImageConfig {
    /// Width and height of the frame in points, each from 1 to 10,000. `None` draws the
    /// default 120 x 90 frame with the picture fitted inside it, as before frames were sized.
    pub size: Option<[f32; 2]>,
    /// Opacity of the picture, from 0 (hidden) to 255 (opaque). The border and the
    /// placeholder are not affected.
    pub opacity: u8,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            size: None,
            opacity: 255,
        }
    }
}
