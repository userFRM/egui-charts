use super::indicator_sizing;
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::Indicator;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response};

/// Configuration for an indicator panel's visual appearance and behavior.
///
/// Controls the panel height, y-axis range, zone lines (overbought/oversold),
/// legend display, and grid visibility. Pre-built configurations are available
/// for common indicators via [`IndicatorPaneConfig::rsi`],
/// [`IndicatorPaneConfig::macd`], and [`IndicatorPaneConfig::stochastic`].
#[derive(Debug, Clone)]
pub struct IndicatorPaneConfig {
    /// Panel height in egui logical points.
    pub height: f32,
    /// Fixed y-axis range (None = auto-scale)
    pub y_range: Option<(f64, f64)>,
    /// Show horizontal zone lines (e.g., RSI 70/30)
    pub show_zones: bool,
    /// Zone levels, colours and captions. A zone in `Color32::TRANSPARENT` draws no line and
    /// still bounds the fill.
    pub zones: Vec<(f64, Color32, &'static str)>,
    /// Fill between the lowest and highest zone in the study's
    /// [`Indicator::level_fill_color`].
    pub fill_between_zones: bool,
    /// Show legend
    pub show_legend: bool,
    /// Show the study name in its legend.
    pub show_name: bool,
    /// Host-supplied input caption; empty omits it.
    pub inputs: String,
    /// Show the study output values in its legend.
    pub show_values: bool,
    /// Show grid
    pub show_grid: bool,
}

impl Default for IndicatorPaneConfig {
    fn default() -> Self {
        Self {
            height: DESIGN_TOKENS.sizing.panel.bottom_default_height,
            y_range: None,
            show_zones: false,
            zones: Vec::new(),
            fill_between_zones: false,
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }
}

impl IndicatorPaneConfig {
    /// Builds the initial appearance from a study's pane metadata.
    pub fn from_study(study: &dyn Indicator) -> Self {
        let pane = study.pane();
        Self {
            height: pane.height.unwrap_or_else(|| Self::default().height),
            y_range: pane
                .range
                .filter(|(min, max)| min.is_finite() && max.is_finite() && min < max),
            show_zones: !pane.levels.is_empty(),
            zones: pane
                .levels
                .into_iter()
                .filter(|(level, _, _)| level.is_finite())
                .collect(),
            fill_between_zones: pane.fill_between_levels,
            ..Self::default()
        }
    }

    /// Create config for RSI indicator
    pub fn rsi() -> Self {
        Self {
            height: 138.0,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (70.0, Color32::from_rgb(120, 123, 134), "Overbought"),
                (30.0, Color32::from_rgb(120, 123, 134), "Oversold"),
            ],
            fill_between_zones: true,
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }

    /// Create config for MACD indicator
    pub fn macd() -> Self {
        Self {
            height: indicator_sizing::MULTI_LINE_HEIGHT,
            y_range: None, // Auto-scale
            show_zones: true,
            zones: vec![(
                0.0,
                Color32::from_rgba_unmultiplied(120, 123, 134, 128),
                "Zero Line",
            )],
            fill_between_zones: false,
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }

    /// Create config for Stochastic indicator
    pub fn stochastic() -> Self {
        Self {
            height: indicator_sizing::OSCILLATOR_HEIGHT,
            y_range: Some((0.0, 100.0)),
            show_zones: true,
            zones: vec![
                (80.0, Color32::from_rgb(120, 123, 134), "Overbought"),
                (20.0, Color32::from_rgb(120, 123, 134), "Oversold"),
            ],
            fill_between_zones: false,
            show_legend: true,
            show_name: true,
            inputs: String::new(),
            show_values: true,
            show_grid: true,
        }
    }
}

// Re-export ChartMapping from coords module for x-axis alignment
pub use crate::chart::coords::ChartMapping;

/// Coordinate parameters for aligning indicator panes with the main chart.
///
/// Wraps the parameters needed to construct a [`ChartMapping`] so that
/// indicator data points are drawn at the same x-positions as their
/// corresponding bars in the main chart. Obtained from the chart's time scale
/// after rendering.
#[derive(Clone, Copy, Debug)]
pub struct IndicatorCoordParams {
    /// Bar spacing in egui logical points.
    pub bar_spacing: f32,
    /// Right offset (bars to the right of last data point)
    pub right_offset: f64,
    /// Base index (index of last data point)
    pub base_idx: usize,
    /// Start index (index of first visible bar)
    pub start_idx: usize,
}

impl IndicatorCoordParams {
    /// Creates coordinate parameters from the main chart's time-scale state.
    ///
    /// These values are typically obtained from the chart after rendering via
    /// [`crate::widget::Chart::get_chart_mapping`].
    pub fn new(
        bar_spacing: f32,
        right_offset: impl Into<f64>,
        base_idx: usize,
        start_idx: usize,
    ) -> Self {
        Self {
            bar_spacing,
            right_offset: right_offset.into(),
            base_idx,
            start_idx,
        }
    }

    /// Creates a [`ChartMapping`] from these params, the panel's screen rect, and y-range.
    ///
    /// The resulting mapping converts bar indices and price/indicator values to
    /// screen positions in egui logical points within the given rect.
    pub fn to_mapping(&self, rect: Rect, y_min: f64, y_max: f64) -> ChartMapping {
        ChartMapping::new(
            rect,
            self.bar_spacing,
            self.start_idx,
            self.base_idx,
            self.right_offset,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(y_min, y_max),
                rect.min.y,
                rect.max.y,
                false,
            ),
        )
    }
}

/// Layout and interaction results from rendering one interactive indicator pane.
///
/// Returned by [`super::IndicatorPane::show_aligned_interactive`]. Besides the rects
/// and y-axis range used for click hit-testing, it carries the [`Response`] for
/// the legend's close "x" so the host can offer one-click pane removal.
#[non_exhaustive]
pub struct PaneInteraction {
    /// Full panel rect, including the y-axis label gutter.
    pub panel_rect: Rect,
    /// Chart drawing area rect, excluding the y-axis label gutter.
    pub chart_rect: Rect,
    /// Y-axis minimum value used for this frame.
    pub y_min: f64,
    /// Y-axis maximum value used for this frame.
    pub y_max: f64,
    /// Interaction response for the whole pane (line selection, hover).
    pub response: Response,
    /// Interaction response for the legend's close "x", or `None` when the pane
    /// shows no legend and therefore exposes no remove affordance.
    pub close_response: Option<Response>,
    /// Visibility button response, when the legend is shown.
    pub visibility_response: Option<Response>,
    /// Settings button response, when the legend is shown.
    pub settings_response: Option<Response>,
    /// Corner of the study menu, when the legend's more-actions button was clicked or its row
    /// secondary-clicked this frame.
    pub menu: Option<Pos2>,
}
