//! Chart-specific UI components.
//!
//! Built on [`crate::ui_kit`] primitives, these modules provide the
//! complete chart interface: drawing toolbar, top toolbar, timeframe
//! bar, replay controls, dialogs, and widget panels.

// Application state trait for UI synchronization
pub mod app_state;
pub use app_state::ChartAppState;

// Stub types for missing frontend dependencies
pub mod stubs;

// Toolbars
pub mod drawing_toolbar;
pub mod floating_toolbar;
pub mod timeframe_toolbar;
pub mod top_toolbar;

// Panels and sidebars
pub mod widget_bar;

// Chart controls
pub mod chart_controls;
pub mod replay;

// Dialogs
pub mod dialogs;

// Icons
pub mod icons;

// Standalone components
pub mod connection_status;
pub mod context_menu;
pub mod export;
pub mod floating_replay_control;
pub mod multi_chart;
pub mod replay_status;
pub mod symbol_header;
pub mod watermark;
