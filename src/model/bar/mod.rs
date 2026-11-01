//! OHLCV bars and a series data container.
//!
//! Use [`Bar`] for a timestamped price and volume observation, and [`BarData`] to
//! hold a series for charting or analysis. Bars can also represent a single value
//! or a whitespace slot on the time axis.

mod bar;
mod bar_data;
mod io;
mod levels;
mod patterns;

// Re-export main types
#[cfg(feature = "widget")]
pub(crate) use bar::SpanCache;
#[cfg(feature = "drawings")]
pub(crate) use bar::SpanFingerprint;
pub use bar::{Bar, BarColors};
pub(crate) use bar::{last_with_value, percent_change};
pub use bar_data::BarData;
pub(crate) use levels::{bar_level_keys, level_grid, level_key};
pub use patterns::CandlePatterns;

#[cfg(feature = "widget")]
mod time;
#[cfg(feature = "widget")]
pub(crate) use time::{index_at, index_at_with_timeframe, time_at, time_at_with_timeframe};
