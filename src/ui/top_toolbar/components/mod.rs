//! Symbol search, interval and chart type pickers, study selection and layout controls.
//!
//! Use these components individually when composing a custom top toolbar.

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
    ChartTypeAction, ChartTypePreferences, ChartTypeSelector, ChartTypeSelectorConfig,
    ChartTypeUiExt,
};

// From indicator_dialog
pub use indicator_dialog::{
    IndicatorCategory, IndicatorDialog, IndicatorDialogAction, IndicatorDialogConfig,
    IndicatorInfo, IndicatorTab,
};

// From chart_grid_selector
pub use chart_grid_selector::ChartGridSelector;

// From sync_button
pub use sync_button::SyncButton;

// From layout_menu
pub use layout_menu::{LayoutAction, LayoutMenu, LayoutMenuConfig};

// From settings_menu (ChartType comes from chart_type_selector - single source of truth)
pub use settings_menu::{
    ChartSettings, SettingsAction, SettingsDialog, SettingsDialogConfig, SettingsMenu, SettingsTab,
};

// From symbol_search
pub use symbol_search::{
    CompareAction, ComparePlacement, CompareSymbol, CompareSymbolsConfig, CompareSymbolsDialog,
    SymbolSearchConfig, SymbolSearchDialog,
};

// From timeframe_selector
pub use timeframe_selector::{
    TimeframeAction, TimeframePreferences, TimeframeSelector, TimeframeSelectorConfig,
    TimeframeUnit,
};
