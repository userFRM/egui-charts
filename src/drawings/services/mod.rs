//! Selection, undo history, snapping, handles and draw ordering for drawings.
//!
//! [`DrawingManager`](super::DrawingManager) coordinates these operations for a chart. Use the individual
//! services when implementing your own drawing interaction or renderer.

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
