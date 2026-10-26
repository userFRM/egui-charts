//! Pure domain types for the drawing system.
//!
//! This layer contains the core data structures with minimal dependencies:
//!
//! - [`Drawing`] -- the central struct representing a drawing on the chart,
//!   with dual coordinates (screen + chart), styling, and tool-specific config.
//! - [`ChartPoint`] -- persistent `(bar_idx, price)` coordinate that survives
//!   pan/zoom operations.
//! - [`DrawingToolType`] -- enum of all drawing tool types with interaction
//!   mode, display name, and serialization support.
//! - [`DrawingInteractionMode`] -- how mouse input maps to point capture.
//! - Options types: [`LineStyle`], [`ArrowStyle`], [`FontWeight`],
//!   [`HandlePos`], [`TimeframeVisibility`], [`DrawingOptions`],
//!   [`FibonacciConfig`], [`FibonacciLevel`], [`VolumeProfileConfig`],
//!   [`RegressionTrendConfig`], [`PositionSize`].
//!
//! The only external dependency is `egui` (for `Pos2` and `Color32` in
//! `Drawing`). All other types are pure Rust with no framework coupling.

mod coordinates;
mod drawing;
mod gann;
mod options;
mod pitchfork;
mod template;

pub use crate::config::{DrawingInteractionMode, DrawingToolType, ParseToolTypeError};
pub use coordinates::ChartPoint;
pub use drawing::Drawing;
pub use gann::GannConfig;
pub use options::{
    ArrowStyle, DrawingOptions, FibonacciConfig, FibonacciLevel, FontWeight, HandlePos, LineStyle,
    PositionSize, ProfileDisplay, RegressionTrendConfig, TimeframeVisibility, VolumeProfileConfig,
};
pub use pitchfork::{PitchforkConfig, PitchforkLevel};
pub use template::DrawingTemplate;
