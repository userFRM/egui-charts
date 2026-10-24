//! Widget bar
//!
//! Contains alerts, object tree, watchlist, symbol details, time and sales, and other panels.

pub mod actions;
pub mod panels;
pub mod state;
pub mod tabs;
pub mod toolbar;

// Core toolbar exports
pub use actions::RightToolbarAction;
pub use state::RightToolbarState;

// Toolbar and tabs exports
pub use tabs::{RightPanelAction, RightPanelTab, RightPanelTabs, RightPanelTabsConfig};
pub use toolbar::{
    RightBarIcon, WidgetBar, WidgetBarAction, WidgetBarConfig, WidgetBarSection, WidgetBarWidget,
};

// Panel exports
pub use panels::*;
