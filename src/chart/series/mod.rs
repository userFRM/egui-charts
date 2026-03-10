//! Series types and renderers for financial chart data.
//!
//! This module provides a family of chart series implementations, each tailored
//! to a specific visualization style:
//!
//! | Type | Description |
//! |------|-------------|
//! | [`LineSeries`] | Simple line connecting data points |
//! | [`AreaSeries`] | Line with filled region to a baseline |
//! | [`BaselineSeries`] | Dual-color area (above/below a reference value) |
//! | [`HistogramSeries`] | Vertical bars from a baseline (volume, MACD) |
//! | [`BarSeries`] | Traditional OHLC bars (high-low line + open/close ticks) |
//!
//! Additionally, the module provides hit testing ([`hit_test`]), selection
//! state ([`selection`]), and selection dot rendering ([`selection_render`])
//! for interactive series feedback.

mod area;
mod bars;
mod baseline;
mod histogram;
pub mod hit_test;
mod line;
pub mod selection;
pub mod selection_render;
pub mod settings;
mod types;

pub use area::AreaSeries;
pub use bars::BarSeries;
pub use baseline::BaselineSeries;
pub use histogram::HistogramSeries;
pub use hit_test::{HitTestConfig, hit_test_candles, hit_test_line, hit_test_volume};
pub use line::LineSeries;
pub use selection::{SeriesHitResult, SeriesId, SeriesSelectionState};
pub use selection_render::{
    SelectionHandleConfig, calculate_dot_interval, render_candle_selection_dots,
    render_series_selection_on_points,
};
pub use settings::SeriesSettings;
pub use types::{Series, SeriesData, SeriesType};
