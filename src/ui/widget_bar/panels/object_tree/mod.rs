//! Object list and cursor data readout for a chart sidebar.
//!
//! Use [`ObjectTreePanel`] with host-supplied [`SourceItem`] records and optional
//! [`DataWindowInfo`]. Apply drawing and study actions through
//! [`ObjectTreePanel::apply_to_chart`]. Copy each drawing's `folder` into its source row.
//! Folder paths persist with drawings. Hosts retain [`ObjectTreeState::folders`] per chart
//! when empty folders must survive closing the panel or restarting the application.

mod actions;
mod chart_actions;
mod config;
mod context_menu;
mod data_window;
mod folders;
mod item_row;
mod panel;
mod source_tree;
mod state;
mod types;

// Re-export public API
pub use actions::ObjectTreeAction;
pub use config::ObjectTreeConfig;
pub use data_window::show_data_window;
pub(crate) use data_window::{format_price, price_decimals};
pub use panel::ObjectTreePanel;
pub use state::ObjectTreeState;
pub use types::{DataWindowInfo, DrawingProperties, SourceItem, SourceType};
