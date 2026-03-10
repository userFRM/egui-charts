//! Chart rendering parameters
//!
//! Consolidates the many parameters needed for chart rendering into organized structs.

use egui::Color32;

use crate::chart::renderers::{PriceScale, RenderContext, StyleColors};
use crate::model::{Bar, PriceSource};

/// Core rendering contexts and data
pub struct CandleDataContext<'a> {
    pub price_ctx: &'a RenderContext<'a>,
    pub volume_ctx: &'a RenderContext<'a>,
    pub price_scale: &'a PriceScale,
    pub colors: &'a StyleColors,
    pub visible_data: &'a [Bar],
    pub start_idx: usize,
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

    pub fn hidden() -> Self {
        Self {
            show_volume: false,
            max_volume: 0.0,
        }
    }
}

/// Japanese chart type settings (Renko, Kagi, etc.)
#[derive(Clone, Copy)]
pub struct JapaneseChartSettings {
    pub renko_brick_size: f64,
    pub kagi_reversal_amount: f64,
}

impl JapaneseChartSettings {
    pub fn new(renko_brick_size: f64, kagi_reversal_amount: f64) -> Self {
        Self {
            renko_brick_size,
            kagi_reversal_amount,
        }
    }

    pub fn default_renko() -> Self {
        Self {
            renko_brick_size: 1.0,
            kagi_reversal_amount: 1.0,
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
pub struct ChartTypeParams {
    pub dimensions: BarDimensions,
    pub volume: VolumeSettings,
    pub japanese: JapaneseChartSettings,
    pub colors: TradingColors,
    pub coords: CoordMapping,
    pub price_source: PriceSource,
}

impl ChartTypeParams {
    /// Create chart type params from pre-composed structs
    pub fn new(
        dimensions: BarDimensions,
        volume: VolumeSettings,
        japanese: JapaneseChartSettings,
        colors: TradingColors,
        coords: CoordMapping,
        price_source: PriceSource,
    ) -> Self {
        Self {
            dimensions,
            volume,
            japanese,
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
    pub fn renko_brick_size(&self) -> f64 {
        self.japanese.renko_brick_size
    }
    pub fn kagi_reversal_amount(&self) -> f64 {
        self.japanese.kagi_reversal_amount
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
