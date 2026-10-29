//! Common types and extension traits for embedding a chart.
//!
//! Implement a feed with the same imports used to embed a chart:
//!
//! ```
//! use egui_charts::prelude::*;
//!
//! struct Feed;
//!
//! impl Datafeed for Feed {
//!     fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
//!         reply.send(Ok(HistoryPage::new(Vec::new(), true)));
//!     }
//!
//!     fn subscribe(&self, _: Subscription) {}
//! }
//! ```
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

pub use crate::data::{
    Datafeed, FeedError, HistoryPage, HistoryRequest, Reply, Sink, Subscription,
};
pub use crate::model::{Bar, BarData, ChartType, Instrument, LineStyle, OrderSide, Timeframe};
pub use crate::studies::{CustomIndicator, Indicator, IndicatorValue};

#[cfg(feature = "widget")]
pub use crate::config::{ChartConfig, ChartOptions};
#[cfg(feature = "widget")]
pub use crate::theme::{Theme, ThemeExt};
#[cfg(feature = "widget")]
pub use crate::widget::{Chart, ChartBuilder, SeriesId};

#[cfg(feature = "widget")]
pub use crate::widget::settings::ChartSettingsState;
