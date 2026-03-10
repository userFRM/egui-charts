//! Object Tree Panel - Object management
//!
//! A comprehensive panel for managing all chart objects including:
//! - Data Window: Real-time OHLCV data at cursor position
//! - Source Tree: Hierarchical view of indicators and drawings
//! - Full object management: visibility, lock, delete, duplicate, reorder
//!
//! # Architecture
//!
//! ```text
//! object_tree/
//! ├── mod.rs           # Public API
//! ├── config.rs        # Configuration
//! ├── state.rs         # State management
//! ├── types.rs         # Core types (SourceItem, DataWindowInfo, etc.)
//! ├── actions.rs       # Action enum
//! ├── panel.rs         # Main widget
//! ├── data_window.rs   # Data Window section
//! ├── source_tree.rs   # Source tree rendering
//! ├── item_row.rs      # Individual item rendering
//! └── context_menu.rs  # Right-click context menu
//! ```
//!
//! # Example
//!
//! ```no_run
//! use egui_open_trading_charts_rs::ui::widget_bar::panels::object_tree::{
//!     ObjectTreePanel, ObjectTreeAction, SourceItem, DataWindowInfo
//! };
//!
//! let mut panel = ObjectTreePanel::new();
//! let mut sources = vec![
//!     SourceItem::indicator(1, "SMA(20)", egui::Color32::YELLOW),
//!     SourceItem::drawing(2, DrawingToolType::TrendLine, egui::Color32::BLUE),
//! ];
//!
//! let action = panel.show(ui, Some(&data_window_info), &mut sources);
//! match action {
//!     ObjectTreeAction::Delete(id) => { /* Handle delete */ }
//!     ObjectTreeAction::ToggleVisibility(id) => { /* Handle visibility */ }
//!     _ => {}
//! }
//! ```
//!
//! Feature-complete object tree panel.

mod actions;
mod config;
mod context_menu;
mod data_window;
mod item_row;
mod panel;
mod source_tree;
mod state;
mod types;

// Re-export public API
pub use actions::ObjectTreeAction;
pub use config::ObjectTreeConfig;
pub use panel::ObjectTreePanel;
pub use state::ObjectTreeState;
pub use types::{DataWindowInfo, DrawingProperties, SourceItem, SourceType};
