//! Chart drawing tools -- the annotation layer for financial charts.
//!
//! This module implements a full TradingView-compatible drawing system with **97
//! drawing tools** spanning trend lines, Fibonacci studies, harmonic patterns,
//! Elliott Wave counts, Gann tools, and more. Drawings live in a persistent
//! coordinate system ([`ChartPoint`]) so they survive pan/zoom operations, and
//! are rendered each frame by converting chart coordinates back to screen space.
//!
//! # Architecture
//!
//! The module is split into four layers with clear dependency boundaries:
//!
//! ```text
//! src/drawings/
//! ├── domain/          # Pure types, zero external dependencies
//! │   ├── tool_type/   # DrawingToolType enum (97 tools), interaction modes
//! │   ├── drawing      # Drawing struct with ChartPoint
//! │   ├── coordinates  # ChartPoint, coordinate conversions
//! │   └── options      # LineStyle, ArrowStyle, FibConfig
//! ├── services/        # Business logic services
//! │   ├── selection    # SelectionService (single + multi-select)
//! │   ├── history      # HistoryService (undo/redo via command pattern)
//! │   ├── snap         # SnapService (magnet mode, price/time snapping)
//! │   ├── handle       # HandleService (selection handle rendering & hit testing)
//! │   ├── interaction  # DrawingInteraction (comprehensive hit testing)
//! │   └── z_order      # ZOrderService (render layer ordering)
//! ├── rendering/       # Consolidated rendering dispatch
//! │   ├── traits       # DrawingRenderer trait, RenderContext, helpers
//! │   └── *            # Category-specific renderers (lines, fibonacci, etc.)
//! ├── categories.rs    # Static toolbar category definitions (97 tools, 8 groups)
//! ├── persistence.rs   # JSON serialization via StoredDrawing / DrawingStorage
//! ├── repositories.rs  # Repository pattern for tool data access
//! └── manager.rs       # DrawingManager -- thin coordinator over services
//! ```
//!
//! # Drawing lifecycle
//!
//! 1. **Creation** -- The user selects a [`DrawingToolType`] from the toolbar.
//!    The [`DrawingManager`] creates a new [`Drawing`] and begins collecting
//!    points according to the tool's [`DrawingInteractionMode`] (single-click,
//!    click-click, drag-to-draw, multi-point, or continuous-draw).
//!
//! 2. **Storage** -- Points are stored as [`ChartPoint`] values (bar index +
//!    price), which remain stable across pan/zoom. Screen coordinates
//!    ([`egui::Pos2`]) are recomputed each frame via
//!    [`Drawing::update_screen_coords`].
//!
//! 3. **Rendering** -- On each frame, [`Drawing::render`] dispatches to a
//!    category-specific renderer (lines, shapes, fibonacci, etc.) using the
//!    current screen coordinates.
//!
//! 4. **Interaction** -- Hit testing, handle dragging, selection, and snapping
//!    are managed by the services layer.
//!
//! 5. **Persistence** -- Drawings can be serialized to JSON via
//!    [`persistence::DrawingStorage`] for saving/loading across sessions.
//!
//! # Tool categories
//!
//! | Category | Examples | Count |
//! |---|---|---|
//! | Cursors | Cross, Dot, Eraser | 6 |
//! | Lines | Trend Line, Ray, Horizontal/Vertical | 9 |
//! | Channels | Parallel Channel, Regression Trend | 4 |
//! | Pitchforks | Andrews, Schiff, Modified Schiff | 4 |
//! | Fibonacci | Retracement, Extension, Circles, Spiral | 11 |
//! | Gann | Fan, Box, Square | 4 |
//! | Patterns | XABCD, Head & Shoulders, Three Drives | 6 |
//! | Elliott Wave | Impulse, Correction, Triangle, Combo | 5 |
//! | Cycles | Cyclic Lines, Sine Line | 3 |
//! | Projection | Long/Short Position, Forecast | 6 |
//! | Volume | Anchored VWAP, Volume Profile | 3 |
//! | Measurers | Price Range, Date Range | 4 |
//! | Brushes | Brush, Highlighter | 3 |
//! | Arrows | Arrow, Arrow Mark Up/Down | 4 |
//! | Shapes | Rectangle, Circle, Ellipse, Polyline | 10 |
//! | Annotations | Text, Note, Callout, Price Label | 11 |
//! | Content | Image, Tweet, Idea (placeholders) | 3 |
//! | Icons | Font Icon / Emoji | 1 |
//!
//! # Quick start
//!
//! ```ignore
//! use egui_charts::drawings::{
//!     Drawing, DrawingToolType, DrawingManager, ChartPoint,
//!     LineStyle, ArrowStyle, FibonacciConfig,
//! };
//!
//! // Create a drawing programmatically
//! let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
//! drawing.chart_points.push(ChartPoint::new(100.0, 150.50));
//! drawing.chart_points.push(ChartPoint::new(120.0, 160.25));
//! drawing.color = [33, 150, 243, 255]; // Blue
//! drawing.stroke_width = 2.0;
//! drawing.completed = true;
//!
//! // Or use DrawingManager for interactive creation
//! let mut manager = DrawingManager::new();
//! manager.set_active_tool(Some(DrawingToolType::FibonacciRetracement));
//!
//! // Services can be used independently
//! use egui_charts::drawings::services::{
//!     SelectionService, HistoryService, SnapService,
//! };
//!
//! let mut selection = SelectionService::new();
//! selection.select(drawing.id);
//! ```

/// Static category and toolbar grouping definitions for all 97 drawing tools.
pub mod categories;
/// Pure domain types with minimal dependencies: `Drawing`, `ChartPoint`,
/// `DrawingToolType`, and styling/configuration options.
pub mod domain;
/// `DrawingManager` -- thin coordinator that delegates to services.
pub mod manager;
/// JSON-based serialization and storage for drawings via `DrawingStorage`.
pub mod persistence;
/// Rendering implementations for all drawing tool types, dispatched per-category.
pub mod rendering;
/// Repository pattern for tool metadata access (categories, search).
pub mod repositories;
/// Business logic services: selection, undo/redo, snapping, handles, z-ordering.
pub mod services;

// Re-export domain types at module root for convenience
pub use domain::{
    ArrowStyle, ChartPoint, Drawing, DrawingInteractionMode, DrawingOptions, DrawingToolType,
    FibonacciConfig, FibonacciLevel, FontWeight, HandlePos, LineStyle, TimeframeVisibility,
};

// Re-export manager types
pub use manager::{DrawingManager, DrawingManagerOptions, DrawingState};
