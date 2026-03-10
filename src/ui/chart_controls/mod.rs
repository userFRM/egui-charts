//! Chart control components.
//!
//! Provides:
//! - Floating zoom/scale controls at bottom-right of chart
//! - Pane controls (maximize, minimize, close) at top-right of each pane

mod actions;
mod config;
mod control_bar;
mod pane_controls;

pub use actions::ChartControlAction;
pub use config::ChartControlBarConfig;
pub use control_bar::{ChartControlBar, ChartControlBarState};
pub use pane_controls::{PaneControlAction, PaneControls, PaneControlsConfig, PaneId};
