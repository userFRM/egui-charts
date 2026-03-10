//! Upper toolbar components
//!
//! Individual components for the upper toolbar.

pub mod chart_grid_selector;
pub mod chart_type_selector;
pub mod indicator_dialog;
pub mod layout_menu;
pub mod settings_menu;
pub mod symbol_search;
pub mod sync_button;
pub mod timeframe_selector;

// Explicit re-exports to avoid ambiguous glob conflicts
// (indicator_dialog and settings_menu both have actions, config, data, dialog, types submodules)

// From chart_type_selector (ChartType is re-exported from crate::model)
pub use chart_type_selector::{
    ChartTypeAction, ChartTypeSelector, ChartTypeSelectorConfig, ChartTypeUiExt,
};

// From indicator_dialog
pub use indicator_dialog::{
    ConfiguredIndicator, IndicatorCategory, IndicatorDialog, IndicatorDialogAction,
    IndicatorDialogConfig, IndicatorInfo, IndicatorParams, IndicatorTab, IndicatorType,
};

// From chart_grid_selector
pub use chart_grid_selector::ChartGridSelector;

// From sync_button
pub use sync_button::SyncButton;

// From layout_menu
pub use layout_menu::{LayoutAction, LayoutInfo, LayoutMenu, LayoutMenuConfig};

// From settings_menu (ChartType comes from chart_type_selector - single source of truth)
pub use settings_menu::{
    CandleColorConfig, ChartSettings, ChartSettingsState, PrecisionMode, ScaleMode, SettingsAction,
    SettingsDialog, SettingsDialogConfig, SettingsMenu, SettingsTab, StatusLineOptions,
};

// From symbol_search
pub use symbol_search::{
    CompareAction, CompareSymbol, CompareSymbolsConfig, CompareSymbolsDialog, SymbolSearchConfig,
    SymbolSearchDialog,
};

// From timeframe_selector
pub use timeframe_selector::{
    Timeframe, TimeframeAction, TimeframeSelector, TimeframeSelectorConfig, TimeframeUnit,
};
