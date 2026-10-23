//! Business logic services for the drawing system.
//!
//! Each service encapsulates a single concern and can be used independently or
//! composed via [`DrawingManager`]. This
//! decomposition makes the code testable in isolation and allows custom
//! integrations to pick only the services they need.
//!
//! # Services
//!
//! | Service | Responsibility |
//! |---|---|
//! | `SelectionService` | Single and multi-select for drawings |
//! | `HistoryService` | Undo/redo via the command pattern |
//! | `SnapService` | Magnet mode, snap-to-price, snap-to-time |
//! | `HandleService` | Selection handle discovery, hit testing, dragging, rendering |
//! | `DrawingInteraction` | Self-contained hit testing + selection + handle dragging |
//! | `ZOrderService` | Bring-to-front / send-to-back layer ordering |
//!
//! # Usage
//!
//! ```
//! use egui::pos2;
//! use egui_charts::drawings::services::{
//!     HistoryService, SelectionService, SnapService, SnapTargets, ZOrderService,
//! };
//! use egui_charts::drawings::{Drawing, DrawingToolType};
//!
//! let mut drawings = vec![
//!     Drawing::new(1, DrawingToolType::TrendLine),
//!     Drawing::new(2, DrawingToolType::HorizontalLine),
//! ];
//!
//! // Selection
//! let mut selection = SelectionService::new();
//! selection.select(1);
//! selection.toggle(2); // Ctrl+click toggle
//!
//! // Undo/redo
//! let mut history = HistoryService::new();
//! let added = Drawing::new(3, DrawingToolType::Rect);
//! drawings.push(added.clone());
//! history.push_add(added);
//! history.undo(&mut drawings);
//!
//! // Snapping
//! let snap = SnapService::new();
//! let snap_targets = SnapTargets::with_price_time(vec![100.0, 120.0], vec![40.0, 80.0]);
//! let snapped = snap.snap_point(pos2(41.0, 101.0), &snap_targets);
//!
//! // Z-ordering
//! ZOrderService::bring_to_front(&mut drawings, 1);
//! ZOrderService::sort_by_z_order(&mut drawings);
//! ```

mod handle;
mod history;
mod interaction;
mod selection;
mod snap;
mod z_order;

pub use handle::{HandleConfig, HandleService};
pub use history::{DrawingCommand, HistoryService};
pub use interaction::{DrawingInteraction, point_to_line_distance};
pub use selection::SelectionService;
pub use snap::{SnapOptions, SnapService, SnapTargets};
pub use z_order::ZOrderService;
pub(crate) use z_order::in_z_order;
