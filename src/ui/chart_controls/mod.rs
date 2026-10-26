//! Pane maximize, restore and close buttons.
//!
//! Use [`PaneControls`] when composing pane headers outside the chart widget, and
//! apply the returned [`PaneControlAction`] to the relevant pane.

mod pane_controls;

pub use pane_controls::{PaneControlAction, PaneControls, PaneControlsConfig, PaneId};
