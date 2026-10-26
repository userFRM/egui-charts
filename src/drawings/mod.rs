//! Drawing tools and annotations anchored to chart coordinates.
//!
//! Use [`DrawingManager`] to select tools and manage drawings, undo and redo. With a
//! [`Chart`](crate::Chart), prefer its drawing APIs for validated timestamp anchors
//! and save/load operations; [`persistence`] exposes the JSON format for host storage.
//!
//! # Interactive drawing
//!
//! Select a tool through the chart's drawing manager; the chart collects its points
//! from pointer input and handles selection and editing:
//!
//! ```
//! use egui_charts::{Chart, drawings::DrawingToolType, model::BarData};
//!
//! let mut chart = Chart::new(BarData::default());
//! chart.drawings_mut().set_active_tool(Some(DrawingToolType::TrendLine));
//! ```
//!
//! [`DrawingToolRegistry`] configures built-in tool availability and lets hosts register
//! custom renderers. Inspect availability before exposing a tool in a host picker.
//! For lower-level composition, [`domain`] contains drawing records, [`services`]
//! provides selection, history, snapping and handle operations, and [`rendering`]
//! paints drawings against a chart view.

pub mod categories;
pub mod domain;
pub(crate) mod glyphs;
pub mod manager;
pub mod persistence;
pub mod registry;
pub mod rendering;
pub mod repositories;
pub mod services;
mod validation;

// Re-export domain types at module root for convenience
pub use domain::{
    ArrowStyle, ChartPoint, Drawing, DrawingInteractionMode, DrawingOptions, DrawingTemplate,
    DrawingToolType, FibonacciConfig, FibonacciLevel, FontWeight, GannConfig, HandlePos,
    ParseToolTypeError, PitchforkConfig, PitchforkLevel, PositionSize, RegressionTrendConfig,
    TimeframeVisibility,
};

// Re-export manager types
pub use manager::{DrawingManager, DrawingManagerOptions, DrawingState};
pub use registry::{DrawingRenderer, DrawingToolRegistry, RegisteredDrawingTool, ToolAvailability};
