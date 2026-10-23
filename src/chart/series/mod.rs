//! Series hit testing, selection state, selection dot rendering and
//! per-series display settings.

pub mod hit_test;
pub mod selection;
pub mod selection_render;
pub mod settings;

pub use hit_test::{HitTestConfig, hit_test_candles, hit_test_line, hit_test_volume};
pub use selection::{SeriesHitResult, SeriesId};
pub use selection_render::{
    SelectionHandleConfig, calculate_dot_interval, render_candle_selection_dots,
    render_series_selection_on_points,
};
pub use settings::SeriesSettings;

#[cfg(feature = "studies")]
mod trade_delta;
#[cfg(feature = "studies")]
pub use trade_delta::{TradeDeltaStyle, render_trade_delta};
