//! Business logic services for the drawing system.
//!
//! Each service encapsulates a single concern and can be used independently or
//! composed via [`DrawingManager`](crate::drawings::DrawingManager). This
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
//! ```ignore
//! use egui_charts::drawings::services::{
//!     SelectionService, HistoryService, SnapService, ZOrderService,
//! };
//!
//! // Selection
//! let mut selection = SelectionService::new();
//! selection.select(drawing_id);
//! selection.toggle(other_id);  // Ctrl+click toggle
//!
//! // Undo/redo
//! let mut history = HistoryService::new();
//! history.push_add(drawing.clone());
//! history.undo(&mut drawings);
//!
//! // Snapping
//! let snap = SnapService::new();
//! let snapped = snap.snap_point(cursor_pos, &snap_targets);
//!
//! // Z-ordering
//! ZOrderService::bring_to_front(&mut drawings, selected_id);
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
