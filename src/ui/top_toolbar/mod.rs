//! Upper toolbar module
//!
//! Contains symbol search, interval selector, chart type, indicators, layouts, and settings.

// Core modules
pub mod actions;
pub mod buttons;
pub mod components;
pub mod config;
mod host;
pub mod state;
mod toolbar;

// Public re-exports
pub use actions::TopToolbarAction;
pub use buttons::{IconTextButton, TextButton, ToolbarIconButton, separator};
pub use components::{
    // Upper toolbar components (ChartType re-exported from crate::model)
    ChartTypeAction,
    ChartTypeSelector,
    ChartTypeSelectorConfig,
    ChartTypeUiExt,
    CompareAction,
    ComparePlacement,
    CompareSymbol,
    CompareSymbolsConfig,
    CompareSymbolsDialog,
    IndicatorCategory,
    IndicatorDialog,
    IndicatorDialogAction,
    IndicatorDialogConfig,
    IndicatorInfo,
    IndicatorTab,
    LayoutAction,
    LayoutMenu,
    LayoutMenuConfig,
    SettingsAction,
    SettingsDialog,
    SettingsDialogConfig,
    SettingsMenu,
    SettingsTab,
    SymbolSearchConfig,
    SymbolSearchDialog,
    TimeframeAction,
    TimeframePreferences,
    TimeframeSelector,
    TimeframeSelectorConfig,
    TimeframeUnit,
};
pub use config::TopToolbarConfig;
pub use host::ToolbarEntry;
pub use state::TopToolbarState;
pub use toolbar::TopToolbar;
