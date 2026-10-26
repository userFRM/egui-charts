//! Chart toolbars, panels, dialogs and trading controls for host applications.
//!
//! Place the components in your egui layout and apply their returned actions to
//! the chart or host services. The components share [`crate::ui_kit`] controls and
//! the active theme; market data, account connections and storage come from the host.

// Application state trait for UI synchronization
pub mod app_state;
pub use app_state::{ChartAppState, Control};

// Shared UI domain model
pub mod model;

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
mod axis_context_menu;
pub mod connection_status;
pub mod context_menu;
pub mod export;
pub mod floating_replay_control;
pub mod multi_chart;
pub mod replay_status;
pub mod strategy_report;
pub mod symbol_header;
mod tpo_context_menu;
pub mod trading;

mod trade_profile_menu;
pub use trade_profile_menu::trade_profile_menu;
#[cfg(test)]
mod accessibility_tests;
