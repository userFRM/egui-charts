//! Widget bar
//!
//! The right edge bar and its panels: watchlist and symbol details, alerts, object tree,
//! highlights and news, screener, hotlists, calendars, notifications, chats, products, and time
//! and sales.

pub(crate) mod actions;
pub(crate) mod panels;
pub(crate) mod state;
pub(crate) mod tabs;
pub(crate) mod toolbar;

pub use actions::RightToolbarAction;
pub use panels::alerts_panel::{
    AlertListOptions, AlertPanelAction, AlertSortKey, AlertTrigger, AlertsPanel, AlertsPanelConfig,
};
pub use panels::calendar::{CalendarAction, CalendarPanel};
pub use panels::chats::ChatsPanel;
pub use panels::hotlists::{HotlistsAction, HotlistsPanel};
pub use panels::market_details::{
    MarketDetail, MarketDetailRequest, PeriodReturn, Seasonality, SeasonalityMonth,
    WatchlistDetails,
};
pub use panels::market_heatmap::{
    MarketHeatmapEntry, MarketHeatmapMetric, MarketHeatmapPanel, MarketHeatmapUniverse,
};
pub use panels::news::{NewsAction, NewsPanel};
pub use panels::notifications::{NotificationsAction, NotificationsPanel};
pub use panels::object_tree::{
    DataWindowInfo, DataWindowSeries, DrawingProperties, ObjectTreeAction, ObjectTreeConfig,
    ObjectTreePanel, ObjectTreeState, SourceItem, SourceType, show_data_window,
};
#[cfg(feature = "detectors")]
pub use panels::order_flow::OrderFlowPanel;
pub use panels::products::{Product, ProductsAction, ProductsPanel};
pub use panels::screener::{ScreenerAction, ScreenerPanel};
pub use panels::symbol_details::SymbolDetailsPanel;
pub use panels::time_and_sales::{TimeAndSalesConfig, TimeAndSalesPanel, TradeSizeFilter};
pub use panels::watchlist::{WatchlistAction, WatchlistGrouping, WatchlistPanel};
pub use state::RightToolbarState;
pub use tabs::{RightPanelAction, RightPanelTab, RightPanelTabs, RightPanelTabsConfig};
pub use toolbar::{
    RightBarIcon, WidgetBar, WidgetBarAction, WidgetBarConfig, WidgetBarSection, WidgetBarWidget,
};
