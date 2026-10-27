//! Drawing records, chart coordinates and tool-specific settings.
//!
//! Use [`Drawing`] and [`ChartPoint`] when working with the drawing manager directly.
//! For persistence across history or interval changes, use timestamp anchors through
//! [`persistence`](super::persistence) or the chart's drawing APIs.

mod coordinates;
mod drawing;
mod gann;
mod ghost_feed;
mod options;
mod pitchfork;
mod template;

pub use crate::config::{DrawingInteractionMode, DrawingToolType, ParseToolTypeError};
pub use coordinates::ChartPoint;
pub use drawing::Drawing;
pub use gann::{GannConfig, GannFanDirection};
pub(crate) use ghost_feed::GhostCandleScreen;
pub use ghost_feed::{GhostCandle, GhostFeedConfig, MAX_GHOST_CANDLES};
pub use options::{
    ArrowStyle, DrawingOptions, FibonacciConfig, FibonacciLevel, FontWeight, HandlePos,
    PositionSize, PositionSizing, RegressionTrendConfig, TimeframeVisibility,
};
pub use pitchfork::{PitchforkConfig, PitchforkLevel};
pub use template::DrawingTemplate;
