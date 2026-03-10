//! Upper toolbar module
//!
//! Contains symbol search, interval selector, chart type, indicators, layouts, and settings.

// Core modules
pub mod actions;
pub mod buttons;
pub mod components;
pub mod config;
pub mod state;
mod toolbar;

// Public re-exports
pub use actions::TopToolbarAction;
pub use buttons::{IconTextButton, TextButton, ToolbarIconButton, separator};
pub use components::{
    CandleColorConfig,
    ChartSettingsState,
    // Upper toolbar components (ChartType re-exported from crate::model)
    ChartTypeAction,
    ChartTypeSelector,
    ChartTypeSelectorConfig,
    ChartTypeUiExt,
    CompareAction,
    CompareSymbol,
    CompareSymbolsConfig,
    CompareSymbolsDialog,
    ConfiguredIndicator,
    IndicatorCategory,
    IndicatorDialog,
    IndicatorDialogAction,
    IndicatorDialogConfig,
    IndicatorInfo,
    IndicatorParams,
    IndicatorTab,
    IndicatorType,
    LayoutAction,
    LayoutInfo,
    LayoutMenu,
    LayoutMenuConfig,
    PrecisionMode,
    ScaleMode,
    SettingsAction,
    SettingsDialog,
    SettingsDialogConfig,
    SettingsMenu,
    SettingsTab,
    StatusLineOptions,
    SymbolSearchConfig,
    SymbolSearchDialog,
    Timeframe,
    TimeframeAction,
    TimeframeSelector,
    TimeframeSelectorConfig,
    TimeframeUnit,
};
pub use config::TopToolbarConfig;
pub use state::TopToolbarState;
pub use toolbar::TopToolbar;
