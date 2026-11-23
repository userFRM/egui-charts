//! Editable and serializable chart display settings.
//!
//! Use [`ChartSettingsState`] to read, modify or persist the settings exposed by
//! the chart settings dialog without depending on application UI components.

mod apply;
pub use apply::ChartSettingsError;
mod data;
mod types;

pub use data::{
    AlertsSettings, ButtonsSettings, CandleColorConfig, ChartBasicStylesSettings,
    ChartSettingsState, CrosshairSettings, CrosshairVisibility, DateFormat, GridLinesSettings,
    MarginsSettings, OrderFlowSettings, ScalesAndLinesSettings, ScalesAppearanceSettings,
    SessionBackgroundSettings, StatusLineOptions, ThousandsSeparator, TradingNotifications,
    TradingSettings, WatermarkSettings,
};
pub use types::{BackgroundType, ButtonVisibility, GridLinesMode, PrecisionMode, WatermarkMode};
