//! Series settings dialog - modal popup for editing series appearance.
//!
//! Provides a settings dialog for chart series with tabs:
//! - Style: Candle colors (bullish/bearish body, border, wick)
//! - Inputs: Price source selection
//! - Visibility: Timeframe visibility settings

mod actions;
mod config;
mod dialog;
mod tabs;

pub use actions::SeriesSettingsAction;
pub use config::SeriesSettingsConfig;
pub use dialog::SeriesSettingsDialog;
pub use tabs::SeriesSettingsTab;
