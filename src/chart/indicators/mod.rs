//! Indicator selection and interaction.
//!
//! Hit testing and selection-handle rendering for overlay indicators
//! (SMA, EMA, Bollinger Bands, …) drawn on the main chart and for separate-pane
//! indicators (RSI, MACD, …). Selection itself is tracked chart-wide by
//! [`SelectionState<ChartElementId>`](crate::chart::selection::SelectionState).

pub mod hit_test;
pub mod selection_render;

pub use hit_test::{hit_test_indicator, hit_test_pane_indicator};
pub use selection_render::{SelectionDotConfig, render_indicator_selection_dots};
