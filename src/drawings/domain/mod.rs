//! Drawing records, chart coordinates and tool-specific settings.
//!
//! Use [`Drawing`] and [`ChartPoint`] when working with the drawing manager directly.
//! For persistence across history or interval changes, use timestamp anchors through
//! [`persistence`](super::persistence) or the chart's drawing APIs.

mod coordinates;
mod drawing;
mod elliott;
mod gann;
mod ghost_feed;
mod options;
mod pitchfork;
mod template;
mod vwap;

pub use crate::config::{DrawingInteractionMode, DrawingToolType, ParseToolTypeError};
pub use coordinates::ChartPoint;
pub use drawing::Drawing;
pub use elliott::ElliottDegree;
pub(crate) use gann::{GANN_MAX_ARC, GANN_SQUARE_UNITS};
pub use gann::{GannConfig, GannFanDirection, GannLevel};
pub use ghost_feed::{GhostCandle, GhostFeedConfig, MAX_GHOST_CANDLES};
pub use options::{
    ArrowStyle, DrawingOptions, FibonacciConfig, FibonacciLevel, FontWeight, HandlePos,
    PositionSize, PositionSizing, RegressionTrendConfig, TimeframeVisibility,
};
pub use pitchfork::{PitchforkConfig, PitchforkLevel};
pub use template::DrawingTemplate;
pub use vwap::{VwapBand, VwapConfig};
