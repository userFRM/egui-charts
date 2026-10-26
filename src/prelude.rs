//! Common types and extension traits for embedding a chart.
//!
//! ```
//! use egui_charts::prelude::*;
//!
//! let bars = vec![Bar::new(
//!     chrono::Utc::now(), 100.0, 102.0, 99.0, 101.0, 10.0,
//! )];
//! # #[cfg(feature = "widget")]
//! let mut chart = ChartBuilder::new().build();
//! # #[cfg(feature = "widget")]
//! chart.set_data(SeriesId::MAIN, bars);
//! ```

pub use crate::data::{Datafeed, FeedError, HistoryRequest, Reply, Sink};
pub use crate::model::{Bar, BarData, ChartType, Instrument, LineStyle, OrderSide, Timeframe};
pub use crate::studies::{CustomIndicator, Indicator, IndicatorValue};

#[cfg(feature = "widget")]
pub use crate::config::{ChartConfig, ChartOptions};
#[cfg(feature = "widget")]
pub use crate::theme::{Theme, ThemeExt};
#[cfg(feature = "widget")]
pub use crate::widget::{Chart, ChartBuilder, SeriesId};

#[cfg(feature = "ui")]
pub use crate::ui::top_toolbar::components::settings_menu::ChartSettingsState;
