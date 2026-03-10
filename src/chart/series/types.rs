//! Core series trait and common data types.
//!
//! Defines the [`Series`] trait that all chart series types implement,
//! along with [`SeriesData`] (the universal data point), [`SeriesType`]
//! (the enumeration of series kinds), and coordinate helper functions.

use crate::model::Bar;
use egui::{Color32, Painter, Rect};

/// Rendering context passed to [`Series::render`].
///
/// Contains everything a series renderer needs: the painter, the visible
/// rectangle, the index range, price bounds, and bar spacing.
pub struct SeriesRenderContext<'a> {
    pub painter: &'a Painter,
    pub rect: Rect,
    pub start_idx: usize,
    pub end_idx: usize,
    pub price_min: f64,
    pub price_max: f64,
    pub bar_spacing: f32,
    pub right_offset: f32,
}

/// Enumeration of available series visualization types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesType {
    Candles,
    Bar,
    Line,
    Area,
    Baseline,
    Histogram,
}

/// Universal data point for all series types.
///
/// Holds OHLCV fields for candlestick/bar series and a `value` field for
/// single-value series (line, area, histogram, baseline). Use
/// [`from_bar`](Self::from_bar) or [`from_val`](Self::from_val) constructors.
#[derive(Debug, Clone)]
pub struct SeriesData {
    /// Ts (as index in data array)
    pub time: usize,

    /// OHLC values (for candlestick / bar series)
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,

    /// Single value (for line/area/histogram/baseline)
    pub value: Option<f64>,

    /// Volume (optional)
    pub volume: Option<f64>,

    /// Color override (optional)
    pub color: Option<Color32>,
}

impl SeriesData {
    /// Create from a Bar
    pub fn from_bar(bar: &Bar, index: usize) -> Self {
        Self {
            time: index,
            open: Some(bar.open),
            high: Some(bar.high),
            low: Some(bar.low),
            close: Some(bar.close),
            value: Some(bar.close),
            volume: Some(bar.volume),
            color: None,
        }
    }

    /// Create a simple value point (for line/area/histogram)
    pub fn from_val(value: f64, index: usize) -> Self {
        Self {
            time: index,
            open: None,
            high: None,
            low: None,
            close: None,
            value: Some(value),
            volume: None,
            color: None,
        }
    }

    /// Get the main value for this data point
    pub fn main_val(&self) -> Option<f64> {
        self.value.or(self.close)
    }
}

/// Trait that all chart series types must implement.
///
/// Provides a uniform interface for querying data, computing visible price
/// ranges, and rendering into an egui `Painter`.
pub trait Series {
    /// Get series type
    fn series_type(&self) -> SeriesType;

    /// Get series data
    fn data(&self) -> &[SeriesData];

    /// Get price range (min, max) for visible data
    fn price_range(&self, start_idx: usize, end_idx: usize) -> Option<(f64, f64)>;

    /// Render the series
    fn render(&self, ctx: &SeriesRenderContext);

    /// Get series name (for legend)
    fn name(&self) -> &str;

    /// Get primary color
    fn color(&self) -> Color32;
}

/// Convert a price value to a Y screen coordinate within the given rectangle.
///
/// Higher prices map to lower Y values (screen coordinates are top-down).
pub fn price_to_y(price: f64, price_min: f64, price_max: f64, rect: Rect) -> f32 {
    let price_range = (price_max - price_min).max(1e-12);
    let ratio = ((price - price_min) / price_range) as f32;
    rect.max.y - ratio * rect.height()
}

/// Convert a bar index to an X screen coordinate using right-to-left layout.
///
/// The `base_idx` (typically the last bar index) anchors the coordinate system,
/// and `right_offset` shifts the right edge for scrolling.
pub fn idx_to_x(
    global_idx: usize,
    base_idx: usize,
    bar_spacing: f32,
    right_offset: f32,
    rect: Rect,
) -> f32 {
    let delta_from_right = base_idx as f32 + right_offset - global_idx as f32;
    let width = rect.width();
    let relative_x = width - (delta_from_right + 0.5) * bar_spacing - 1.0;
    rect.min.x + relative_x
}
