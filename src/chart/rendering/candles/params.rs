//! Chart rendering parameters
//!
//! Consolidates the many parameters needed for chart rendering into organized structs.

use crate::scales::PriceMap;
use egui::Color32;

use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, PriceSource};

/// Core rendering contexts and data
pub struct CandleDataContext<'a> {
    pub price_ctx: &'a RenderContext<'a>,
    pub volume_ctx: &'a RenderContext<'a>,
    pub price_scale: &'a PriceMap,
    pub colors: &'a StyleColors,
    /// The bars in the current visible window.
    pub visible_data: &'a [Bar],
    /// The complete dataset the visible window is a contiguous slice of.
    ///
    /// `visible_data` equals `full_data[start_idx..start_idx + visible_data.len()]`.
    /// Chart types whose values depend on bars outside the window (e.g.
    /// Heikin-Ashi, a cumulative recurrence) compute over `full_data` and slice
    /// the window out, so their output is invariant under panning and zooming.
    pub full_data: &'a [Bar],
    /// [`ChartState::revision`](crate::studies::ChartState::revision) of `full_data`.
    pub revision: u64,
    /// Index of the first visible bar within `full_data`.
    pub start_idx: usize,
    /// Interval and calendar of the bars, whose sessions the TPO profiles follow.
    pub clock: &'a crate::model::BarClock,
    /// The instrument's minimum price increment, if known.
    pub tick_size: Option<f64>,
}

/// Bar/candle rendering dimensions
#[derive(Clone, Copy)]
pub struct BarDimensions {
    pub bar_width: f32,
    pub wick_width: f32,
}

impl BarDimensions {
    pub fn new(bar_width: f32, wick_width: f32) -> Self {
        Self {
            bar_width,
            wick_width,
        }
    }
}

/// Volume rendering settings
#[derive(Clone, Copy)]
pub struct VolumeSettings {
    pub show_volume: bool,
    pub max_volume: f64,
}

impl VolumeSettings {
    pub fn new(show_volume: bool, max_volume: f64) -> Self {
        Self {
            show_volume,
            max_volume,
        }
    }
}

/// Bullish/bearish color pair
#[derive(Clone, Copy)]
pub struct TradingColors {
    pub bullish: Color32,
    pub bearish: Color32,
}

impl TradingColors {
    pub fn new(bullish: Color32, bearish: Color32) -> Self {
        Self { bullish, bearish }
    }
}

/// Coordinate mapping for rendering
pub struct CoordMapping {
    pub chart_rect_min_x: f32,
}

impl CoordMapping {
    pub fn new(chart_rect_min_x: f32) -> Self {
        Self { chart_rect_min_x }
    }
}

/// Combined parameters for chart type rendering
pub struct ChartTypeParams<'a> {
    pub dimensions: BarDimensions,
    pub volume: VolumeSettings,
    /// The parameters of the price-based chart types
    pub type_params: &'a crate::model::ChartTypeParams,
    pub colors: TradingColors,
    pub coords: CoordMapping,
    pub price_source: PriceSource,
}

impl<'a> ChartTypeParams<'a> {
    /// Create chart type params from pre-composed structs
    pub fn new(
        dimensions: BarDimensions,
        volume: VolumeSettings,
        type_params: &'a crate::model::ChartTypeParams,
        colors: TradingColors,
        coords: CoordMapping,
        price_source: PriceSource,
    ) -> Self {
        Self {
            dimensions,
            volume,
            type_params,
            colors,
            coords,
            price_source,
        }
    }

    // Convenience accessors
    pub fn bar_width(&self) -> f32 {
        self.dimensions.bar_width
    }
    pub fn wick_width(&self) -> f32 {
        self.dimensions.wick_width
    }
    pub fn show_volume(&self) -> bool {
        self.volume.show_volume
    }
    pub fn max_volume(&self) -> f64 {
        self.volume.max_volume
    }
    pub fn bullish_color(&self) -> Color32 {
        self.colors.bullish
    }
    pub fn bearish_color(&self) -> Color32 {
        self.colors.bearish
    }
    pub fn chart_rect_min_x(&self) -> f32 {
        self.coords.chart_rect_min_x
    }
    pub fn price_source(&self) -> PriceSource {
        self.price_source
    }
}
