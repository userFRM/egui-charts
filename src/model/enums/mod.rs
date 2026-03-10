//! Wire-format enum types for chart display and rendering.
//!
//! These enums mirror the TradingView Charting Library's wire protocol types.
//! They are used for serialization and conversion to/from the domain-level
//! types in [`crate::model`] (e.g. [`ChartStyle`] <-> [`ChartType`](super::ChartType)).

pub mod chart;
pub mod marks;
pub mod study;

pub use chart::{
    BackgroundType, ChartMode, ChartStyle, ChartTheme, ColorType, CrosshairMode, LineStyle,
    MagnetMode, Orientation, OverrideLineStyle, OverridePriceAxisLastValueMode, PriceLineSource,
    PriceScaleMode, Resolution, ScaleLocation, Size, Timezone,
};
pub use marks::{ClearMarksMode, MarkLocation};
pub use study::{
    FilledAreaType, LineStudyPlotStyle, OhlcStudyPlotStyle, StudyDisplayTarget, StudyInputType,
    StudyPlotType, StudyTargetPriceScale,
};
