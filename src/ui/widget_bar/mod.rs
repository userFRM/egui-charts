//! Widget bar
//!
//! The right edge bar and its panels: watchlist and symbol details, alerts, object tree,
//! highlights and news, screener, hotlists, calendars, notifications, chats, products, and time
//! and sales.

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
