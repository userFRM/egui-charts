//! Separate-pane indicator rendering with direct egui painting.
//!
//! This module renders oscillator and multi-line indicators (RSI, MACD, Stochastic,
//! etc.) in dedicated panels below the main chart. It paints directly through
//! [`egui::Painter`] to ensure pixel-perfect x-axis alignment with the main
//! candlestick chart.
//!
//! # Usage
//!
//! Indicator panes are typically drawn by [`super::Chart::show`], but
//! they can also be used standalone:
//!
//! ```rust
//! use egui_charts::widget::indicator_pane::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig};
//!
//! # fn example(
//! #     ui: &mut egui::Ui,
//! #     rsi_indicator: &egui_charts::studies::RSI,
//! #     bars: &[egui_charts::model::Bar],
//! #     visible_range: std::ops::Range<usize>,
//! #     coords: IndicatorCoordParams,
//! # ) {
//! let config = IndicatorPaneConfig::rsi();
//! let mut pane = IndicatorPane::with_config(config);
//! pane.show_aligned(ui, rsi_indicator, bars, visible_range, coords);
//! # }
//! ```
//!
//! # Coordinate Alignment
//!
//! The [`IndicatorCoordParams`] struct captures the main chart's bar spacing,
//! right offset, and base index so that indicator panes draw their data points
//! at exactly the same x-coordinates as the corresponding candlesticks above.

use crate::chart::rendering::draw_dashed_line;
use crate::model::Bar;
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::Indicator;
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui};
use std::ops::Range;

/// Chart layout constants matching main chart (from ChartConfig defaults)
mod layout {
    /// Left padding (matches ChartConfig::padding default)
    pub const LEFT_PADDING: f32 = crate::styles::sizing::chart::PADDING;
    /// Right price axis width for standalone panes
    pub const RIGHT_PADDING: f32 = crate::styles::sizing::chart::RIGHT_AXIS_WIDTH;
    /// Minimum pixel spacing between vertical grid lines
    pub const MIN_GRID_SPACING: f32 = 80.0;
}

/// Indicator panel sizing constants
mod indicator_sizing {
    /// Standard panel height for oscillators (RSI, Stochastic)
    pub const OSCILLATOR_HEIGHT: f32 = 120.0;
    /// Standard panel height for multi-line indicators (MACD)
    pub const MULTI_LINE_HEIGHT: f32 = 150.0;
}

/// Configuration for an indicator panel's visual appearance and behavior.
///
/// Controls the panel height, y-axis range, zone lines (overbought/oversold),
/// legend display, and grid visibility. Pre-built configurations are available
/// for common indicators via [`IndicatorPaneConfig::rsi`],
/// [`IndicatorPaneConfig::macd`], and [`IndicatorPaneConfig::stochastic`].
#[derive(Debug, Clone)]
pub struct IndicatorPaneConfig {
    /// Panel height in pixels
    pub height: f32,
    /// Fixed y-axis range (None = auto-scale)
    pub y_range: Option<(f64, f64)>,
    /// Show horizontal zone lines (e.g., RSI 70/30)
    pub show_zones: bool,
    /// Zone line levels and colors
    pub zones: Vec<(f64, Color32, &'static str)>,
    /// Fill between the lowest and highest zone with the first plot colour at 20% opacity.
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
                (70.0, Color32::from_gray(128), "Overbought"),
                (30.0, Color32::from_gray(128), "Oversold"),
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
            zones: vec![(0.0, DESIGN_TOKENS.semantic.extended.chart_text, "Zero Line")],
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
                (80.0, DESIGN_TOKENS.semantic.extended.bearish, "Overbought"),
                (20.0, DESIGN_TOKENS.semantic.extended.bullish, "Oversold"),
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
    /// Bar spacing in pixels
    pub bar_spacing: f32,
    /// Right offset (bars to the right of last data point)
    pub right_offset: f32,
    /// Base index (index of last data point)
    pub base_idx: usize,
    /// Start index (index of first visible bar)
    pub start_idx: usize,
}

impl IndicatorCoordParams {
    /// Creates coordinate parameters from the main chart's time-scale state.
    ///
    /// These values are typically obtained from the chart after rendering via
    /// [`super::Chart::get_chart_mapping`] or the internal time-scale accessors.
    pub fn new(bar_spacing: f32, right_offset: f32, base_idx: usize, start_idx: usize) -> Self {
        Self {
            bar_spacing,
            right_offset,
            base_idx,
            start_idx,
        }
    }

    /// Creates a [`ChartMapping`] from these params, the panel's screen rect, and y-range.
    ///
    /// The resulting mapping converts bar indices to x-pixel positions and
    /// price/indicator values to y-pixel positions within the given rect.
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
/// Returned by [`IndicatorPane::show_aligned_interactive`]. Besides the rects
/// and y-axis range used for click hit-testing, it carries the [`Response`] for
/// the legend's close "x" so the host can offer one-click pane removal.
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

/// A separate-pane indicator panel rendered via direct egui painting.
///
/// Each `IndicatorPane` renders one indicator (RSI, MACD, etc.) in its own
/// rectangular area below the main chart. The pane includes:
/// - Indicator line(s) with proper colors
/// - Horizontal zone lines (e.g., RSI 70/30 overbought/oversold)
/// - Semi-transparent zone fills for overbought/oversold regions
/// - Y-axis labels on the right side
/// - A legend showing the indicator name and current value
/// - Optional vertical grid lines aligned with the main chart
pub struct IndicatorPane {
    config: IndicatorPaneConfig,
    pub(super) bar_index: Option<usize>,
    pub(super) measured_axis_width: f32,
    pub(super) price_map: Option<PriceMap>,
    pub(super) price_formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    pub(super) overlay: bool,
    pub(super) legend_offset: f32,
    pub(super) pane_controls: bool,
    pub(super) axis_tags: Vec<Option<Rect>>,
}

impl IndicatorPane {
    /// Create a new indicator panel with custom configuration
    pub fn with_config(config: IndicatorPaneConfig) -> Self {
        Self {
            config,
            bar_index: None,
            measured_axis_width: 0.0,
            price_map: None,
            price_formatter: None,
            overlay: false,
            legend_offset: 0.0,
            pane_controls: false,
            axis_tags: Vec::new(),
        }
    }

    /// Render the indicator panel with proper x-axis alignment
    pub fn show(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        bars: &[Bar],
        visible_range: Range<usize>,
    ) {
        if !indicator.is_visible() {
            return;
        }

        // Get chart background and text colors from theme
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, _response) = ui.allocate_exact_size(panel_size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter();

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + left_padding, rect.min.y),
            Pos2::new(rect.max.x - right_padding, rect.max.y),
        );

        // Determine y-axis range
        let (y_min, y_max) = self.calculate_y_range(indicator, &visible_range);

        // Draw zone fills (colored bands)
        if self.config.show_zones && self.config.zones.len() >= 2 {
            self.draw_zone_fills(painter, chart_rect, y_min, y_max, indicator);
        }

        // Draw horizontal grid lines and zone lines
        self.draw_horizontal_lines(
            painter, chart_rect, y_min, y_max, grid_color, bullish, bearish,
        );

        // Draw indicator lines
        self.draw_indicator_lines(
            painter,
            chart_rect,
            indicator,
            bars,
            &visible_range,
            y_min,
            y_max,
        );

        // Draw y-axis labels
        self.draw_y_axis_labels(
            painter,
            rect,
            chart_rect,
            y_min,
            y_max,
            indicator,
            Some(crate::scales::PriceScaleId::Right),
            None,
        );

        // Draw indicator legend at top-left. The non-interactive path cannot
        // host a clickable close affordance, so no remove "x" is drawn here.
        self.draw_legend(painter, chart_rect, indicator, text_color);
    }

    /// Render with explicit coordinate parameters for perfect x-axis alignment.
    ///
    /// Uses [`IndicatorCoordParams`] from the main chart to ensure that each
    /// indicator data point is drawn at the exact same x-coordinate as its
    /// corresponding bar in the price chart above.
    pub fn show_aligned(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        bars: &[Bar],
        _visible_range: Range<usize>,
        coords: IndicatorCoordParams,
    ) {
        if !indicator.is_visible() {
            return;
        }

        // Get chart background and text colors from theme
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, _response) = ui.allocate_exact_size(panel_size, Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter();

        // Fill background
        painter.rect_filled(rect, 0.0, chart_bg);

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let chart_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + left_padding, rect.min.y),
            Pos2::new(rect.max.x - right_padding, rect.max.y),
        );

        // Determine y-axis range
        let horizontal = coords.to_mapping(chart_rect, 0.0, 1.0);
        let displayed_range = horizontal.x_to_idx(chart_rect.left())
            ..horizontal.x_to_idx(chart_rect.right()).saturating_add(1);
        let (y_min, y_max) = self.calculate_y_range(indicator, &displayed_range);

        // Draw zone fills (colored bands)
        if self.config.show_zones && self.config.zones.len() >= 2 {
            self.draw_zone_fills(painter, chart_rect, y_min, y_max, indicator);
        }

        // Draw horizontal grid lines and zone lines
        self.draw_horizontal_lines(
            painter, chart_rect, y_min, y_max, grid_color, bullish, bearish,
        );

        // Draw vertical grid lines (aligned with main chart)
        if self.config.show_grid {
            self.draw_vertical_grid_aligned(painter, chart_rect, &coords, grid_color);
        }

        // Draw indicator lines with aligned coordinates
        self.draw_indicator_lines_aligned(
            painter, chart_rect, indicator, bars, y_min, y_max, &coords,
        );

        // Draw y-axis labels
        self.draw_y_axis_labels(
            painter,
            rect,
            chart_rect,
            y_min,
            y_max,
            indicator,
            Some(crate::scales::PriceScaleId::Right),
            None,
        );

        // Draw indicator legend at top-left. The non-interactive path cannot
        // host a clickable close affordance, so no remove "x" is drawn here.
        self.draw_legend(painter, chart_rect, indicator, text_color);
    }

    /// Render with coordinate alignment and return layout info for hit testing.
    ///
    /// Like [`IndicatorPane::show_aligned`], but returns the panel and chart
    /// rects, y-axis range, and an interactive [`egui::Response`]. This allows
    /// the caller to implement click-on-line selection, tooltip display, or
    /// other interactive features on indicator panes.
    ///
    /// Returns `None` if the indicator is not visible.
    ///
    /// The returned [`PaneInteraction`] also carries the [`Response`] for the
    /// legend's close "x" (when the pane shows a legend), so the host can offer
    /// one-click pane removal.
    pub fn show_aligned_interactive(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        bars: &[Bar],
        _visible_range: Range<usize>,
        coords: IndicatorCoordParams,
    ) -> Option<PaneInteraction> {
        self.show_aligned_interactive_in(
            ui,
            indicator,
            bars,
            coords,
            None,
            false,
            Some(crate::scales::PriceScaleId::Right),
            None,
            &[],
            false,
        )
    }

    /// Shows the pane with attached `primitives`. For a bar series, `indicator` supplies only
    /// its legend; the caller draws its bars and the series primitive pass afterwards.
    pub(super) fn show_aligned_interactive_in(
        &mut self,
        ui: &mut Ui,
        indicator: &dyn Indicator,
        bars: &[Bar],
        coords: IndicatorCoordParams,
        plot_x: Option<egui::emath::Rangef>,
        collapsed: bool,
        axis: Option<crate::scales::PriceScaleId>,
        hover: Option<(Pos2, crate::config::CrosshairOptions)>,
        primitives: &[&super::primitives::Attached],
        bar_series: bool,
    ) -> Option<PaneInteraction> {
        if !indicator.is_visible() {
            return None;
        }

        // Get chart background and text colors from theme
        let colors = ui.active_theme().chart;
        let chart_bg = colors.bg;
        let text_color = ui.style().visuals.text_color();
        let grid_color = colors.grid_line;

        // Get live theme colors for zones
        let bullish = colors.candle_up;
        let bearish = colors.candle_down;

        // Allocate the panel area with click sense
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::click());

        if !ui.is_rect_visible(rect) {
            return None;
        }

        let painter = &ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));

        // Fill background
        if !self.overlay {
            painter.rect_filled(rect, 0.0, chart_bg);
        }

        // Calculate drawing area to match main chart layout
        let left_padding = layout::LEFT_PADDING;
        let right_padding = layout::RIGHT_PADDING;
        let left_axis = axis == Some(crate::scales::PriceScaleId::Left);
        let plot_x = plot_x.unwrap_or_else(|| {
            egui::emath::Rangef::new(
                rect.left() + left_padding,
                (rect.right() - right_padding).max(rect.left() + left_padding),
            )
        });
        let chart_rect = Rect::from_min_max(
            Pos2::new(plot_x.min, rect.top()),
            Pos2::new(plot_x.max, rect.bottom()),
        );

        // Determine y-axis range
        let horizontal = coords.to_mapping(chart_rect, 0.0, 1.0);
        let displayed_range = horizontal.x_to_idx(chart_rect.left())
            ..horizontal.x_to_idx(chart_rect.right()).saturating_add(1);
        let (y_min, y_max) = self.price_map.map_or_else(
            || self.calculate_y_range(indicator, &displayed_range),
            |map| {
                (
                    map.transform().to_price(map.range().min),
                    map.transform().to_price(map.range().max),
                )
            },
        );

        let mut mapping = coords.to_mapping(chart_rect, y_min, y_max);
        if let Some(map) = self.price_map {
            mapping.price = map;
        }
        let view = primitives
            .first()
            .map(|item| crate::chart::primitives::PrimitiveView {
                series: item.series,
                study: (!bar_series).then_some(indicator),
                mapping,
                bars,
                visible: displayed_range.clone(),
            });
        let draw = |painter: &egui::Painter, pass| {
            if let Some(view) = &view {
                for item in primitives {
                    item.get().draw(painter, view, pass);
                }
            }
        };
        let crosshair = hover.map(|(mut pos, options)| {
            if let Some(index) = self.bar_index {
                pos.x = mapping.idx_to_x(index);
                if options.mode.snaps()
                    && chart_rect.contains(pos)
                    && let Some(value) = crate::chart::renderers::snap_to_values(
                        pos.y,
                        &mapping.price,
                        (0..indicator.line_cnt())
                            .filter(|line| {
                                let plot = indicator.plot_style(*line);
                                plot.visible
                                    && !matches!(
                                        plot.plot_type,
                                        crate::studies::PlotType::BarColor
                                            | crate::studies::PlotType::BackgroundColor
                                    )
                            })
                            .filter_map(|line| indicator.plot_value(line, index)),
                    )
                {
                    pos.y = mapping.price_to_y(value);
                }
            }
            (pos, options)
        });
        if !collapsed {
            let plot_painter = &painter.with_clip_rect(chart_rect.intersect(painter.clip_rect()));
            // Draw zone fills (colored bands)
            if self.config.show_zones && self.config.zones.len() >= 2 {
                self.draw_zone_fills(plot_painter, chart_rect, y_min, y_max, indicator);
            }

            // Draw horizontal grid lines and zone lines
            self.draw_horizontal_lines(
                plot_painter,
                chart_rect,
                y_min,
                y_max,
                grid_color,
                bullish,
                bearish,
            );

            // Draw vertical grid lines (aligned with main chart)
            if self.config.show_grid {
                self.draw_vertical_grid_aligned(plot_painter, chart_rect, &coords, grid_color);
            }

            use crate::chart::primitives::PrimitivePass;
            draw(plot_painter, PrimitivePass::Below);
            // Draw indicator lines with aligned coordinates, unless a renderer draws them
            if !primitives.iter().any(|item| item.replaces) {
                self.draw_indicator_lines_aligned(
                    plot_painter,
                    chart_rect,
                    indicator,
                    bars,
                    y_min,
                    y_max,
                    &coords,
                );
            }
            if !bar_series {
                draw(plot_painter, PrimitivePass::Series);
            }

            if let (Some(view), Some(side)) = (&view, axis)
                && indicator.scale() != crate::studies::StudyScale::None
            {
                let (axis_rect, axis_edge) = if side == crate::scales::PriceScaleId::Left {
                    (
                        Rect::from_x_y_ranges(rect.left()..=chart_rect.left(), rect.y_range()),
                        chart_rect.left(),
                    )
                } else {
                    (
                        Rect::from_x_y_ranges(chart_rect.right()..=rect.right(), rect.y_range()),
                        rect.right(),
                    )
                };
                let axis_painter = painter.with_clip_rect(painter.clip_rect().intersect(axis_rect));
                for item in primitives {
                    let primitive = item.get();
                    primitive.draw_price_axis(&axis_painter, axis_rect, view);
                    for label in primitive.price_axis_labels(view) {
                        let map = mapping.price;
                        if map.range().contains(map.transform().to_scale(label.price)) {
                            let tag = crate::chart::rendering::axes::render_axis_value(
                                painter,
                                chart_rect,
                                axis_edge,
                                map.price_to_y(label.price),
                                label.text,
                                (
                                    crate::chart::rendering::axes::tag_text_color(label.color),
                                    label.color,
                                ),
                                &self.axis_tags,
                            );
                            self.axis_tags.push(tag);
                        }
                    }
                }
            }
            let context = crate::chart::renderers::RenderContext::new(painter, chart_rect);
            let tag = crosshair.and_then(|(pos, options)| {
                crate::chart::renderers::render_crosshair_lines(
                    &context,
                    pos,
                    &options,
                    chart_rect.bottom(),
                );
                axis?;
                if indicator.scale() == crate::studies::StudyScale::None {
                    return None;
                }
                crate::chart::renderers::render_crosshair_price_tag(
                    &context,
                    pos,
                    &mapping.price.with_price_precision(Some(
                        indicator.precision().unwrap_or(2).min(255) as u8,
                    )),
                    &options,
                    if left_axis {
                        chart_rect.left()
                    } else {
                        rect.right()
                    },
                    None,
                    Some(match self.price_formatter.as_deref() {
                        Some(formatter) => mapping.price.transform().label(
                            mapping.price.y_to_scale(pos.y),
                            indicator.precision().unwrap_or(2),
                            Some(formatter),
                        ),
                        None => indicator.format_value(mapping.price.y_to_price(pos.y)),
                    }),
                )
            });
            // Draw y-axis labels
            self.measured_axis_width = self.draw_y_axis_labels(
                painter, rect, chart_rect, y_min, y_max, indicator, axis, tag,
            );
        }

        if collapsed && let Some((pos, mut options)) = crosshair {
            options.horz_line_visible = false;
            crate::chart::renderers::render_crosshair_lines(
                &crate::chart::renderers::RenderContext::new(painter, chart_rect),
                pos,
                &options,
                chart_rect.bottom(),
            );
        }
        let controls = self.legend_controls_rect(painter, chart_rect, indicator);
        let responses = controls.map(|controls| {
            let row = Rect::from_min_max(
                chart_rect.min + egui::vec2(0.0, self.legend_offset),
                controls.right_bottom(),
            );
            crate::chart::renderers::study_controls(
                ui,
                row,
                controls,
                response.id.with("study_controls"),
                indicator.is_visible(),
            )
        });
        let legend_rect = Rect::from_min_max(
            chart_rect.min,
            Pos2::new(
                controls.map_or(chart_rect.right(), |rect| rect.left()),
                chart_rect.bottom(),
            ),
        );
        self.draw_legend(
            &painter.with_clip_rect(legend_rect.intersect(ui.clip_rect())),
            chart_rect,
            indicator,
            text_color,
        );
        let (visibility_response, settings_response, close_response, menu) = match responses {
            Some(([visibility, settings, close], menu)) => {
                (Some(visibility), Some(settings), Some(close), menu)
            }
            None => (None, None, None, None),
        };

        // Return panel info for hit testing
        Some(PaneInteraction {
            panel_rect: rect,
            chart_rect,
            y_min,
            y_max,
            response,
            close_response,
            visibility_response,
            settings_response,
            menu,
        })
    }

    pub(super) fn calculate_y_range(
        &self,
        indicator: &dyn Indicator,
        visible_range: &Range<usize>,
    ) -> (f64, f64) {
        if indicator.is_volume() {
            return super::volume::range(indicator, visible_range, None);
        }
        if let Some((min, max)) = self.config.y_range {
            return (min, max);
        }

        // Auto-scale based on visible indicator values
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;

        for line in (0..indicator.line_cnt()).filter(|line| {
            let plot = indicator.plot_style(*line);
            plot.visible
                && !matches!(
                    plot.plot_type,
                    crate::studies::PlotType::BarColor | crate::studies::PlotType::BackgroundColor
                )
        }) {
            for bar in visible_range.clone() {
                if let Some(value) = indicator.plot_value(line, bar) {
                    min_val = min_val.min(value);
                    max_val = max_val.max(value);
                }
            }
        }

        for candle in indicator.candles().iter().filter(|candle| {
            candle.bars.end > visible_range.start
                && candle.bars.start < visible_range.end
                && indicator.plot_style(candle.plot).visible
        }) {
            min_val = min_val.min(candle.bar.low);
            max_val = max_val.max(candle.bar.high);
        }

        for plot in indicator.volume_profiles(visible_range.clone()) {
            min_val = min_val.min(plot.profile.low());
            max_val = max_val.max(plot.profile.high());
        }

        for zone in indicator.zones().iter().filter(|zone| {
            zone.bars.end > visible_range.start
                && zone.bars.start < visible_range.end
                && indicator.plot_style(zone.plot).visible
        }) {
            min_val = min_val.min(zone.bottom);
            max_val = max_val.max(zone.top);
        }

        #[cfg(feature = "drawings")]
        for price in crate::studies::plots::drawing_prices(indicator, visible_range.clone()) {
            min_val = min_val.min(price);
            max_val = max_val.max(price);
        }

        if min_val != f64::MAX
            && (0..indicator.line_cnt()).any(|line| {
                let plot = indicator.plot_style(line);
                plot.visible
                    && matches!(
                        plot.plot_type,
                        crate::studies::PlotType::Histogram
                            | crate::studies::PlotType::Columns
                            | crate::studies::PlotType::Area
                    )
            })
        {
            min_val = min_val.min(0.0);
            max_val = max_val.max(0.0);
        }

        if min_val == f64::MAX {
            (0.0, 100.0) // Default fallback
        } else {
            // Add 10% padding
            let padding = crate::scales::auto_scale_span(min_val, max_val) * 0.1;
            (min_val - padding, max_val + padding)
        }
    }

    fn draw_zone_fills(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        indicator: &dyn Indicator,
    ) {
        if self.config.fill_between_zones {
            let lower = self
                .config
                .zones
                .iter()
                .map(|z| z.0)
                .fold(f64::INFINITY, f64::min);
            let upper = self
                .config
                .zones
                .iter()
                .map(|z| z.0)
                .fold(f64::NEG_INFINITY, f64::max);
            if lower < upper {
                let color = indicator.plot_style(0).color;
                painter.rect_filled(
                    Rect::from_two_pos(
                        Pos2::new(rect.left(), self.value_to_y(upper, rect, y_min, y_max)),
                        Pos2::new(rect.right(), self.value_to_y(lower, rect, y_min, y_max)),
                    )
                    .intersect(rect),
                    0.0,
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 51),
                );
            }
            return;
        }
        let colors = painter.ctx().active_theme().chart;
        let (bullish, bearish) = (colors.candle_up, colors.candle_down);
        for (level, color, label) in &self.config.zones {
            let live_color = if *color == DESIGN_TOKENS.semantic.extended.bullish {
                bullish
            } else if *color == DESIGN_TOKENS.semantic.extended.bearish {
                bearish
            } else {
                continue; // Skip neutral zones for fills
            };

            let y = self.value_to_y(*level, rect, y_min, y_max);

            // Create semi-transparent fill color
            let fill_color = Color32::from_rgba_unmultiplied(
                live_color.r(),
                live_color.g(),
                live_color.b(),
                25, // Very transparent
            );

            if label.contains("Overbought") || label.contains("above") {
                let fill_rect = Rect::from_two_pos(
                    Pos2::new(rect.left(), self.value_to_y(y_max, rect, y_min, y_max)),
                    Pos2::new(rect.right(), y),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            } else if label.contains("Oversold") || label.contains("below") {
                let fill_rect = Rect::from_two_pos(
                    Pos2::new(rect.left(), y),
                    Pos2::new(rect.right(), self.value_to_y(y_min, rect, y_min, y_max)),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
        }
    }

    fn draw_horizontal_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        grid_color: Color32,
        _bullish: Color32,
        _bearish: Color32,
    ) {
        // Draw zone lines with dashed style
        if self.config.show_zones {
            for (level, color, _label) in &self.config.zones {
                let live_color = if [
                    DESIGN_TOKENS.semantic.extended.bullish,
                    DESIGN_TOKENS.semantic.extended.bearish,
                    DESIGN_TOKENS.semantic.extended.chart_text,
                ]
                .contains(color)
                {
                    grid_color
                } else {
                    *color
                };

                let y = self.value_to_y(*level, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    // Draw dashed line
                    draw_dashed_line(
                        painter,
                        Pos2::new(rect.min.x, y),
                        Pos2::new(rect.max.x, y),
                        Stroke::new(1.0_f32, live_color),
                        if self.config.fill_between_zones {
                            4.0
                        } else {
                            5.0
                        },
                        if self.config.fill_between_zones {
                            4.0
                        } else {
                            3.0
                        },
                    );
                }
            }
        }

        // Draw subtle grid lines
        if self.config.show_grid {
            if let Some(map) = self.price_map {
                for mark in map.marks() {
                    painter.line_segment(
                        [
                            Pos2::new(rect.left(), mark.y_coord),
                            Pos2::new(rect.right(), mark.y_coord),
                        ],
                        Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
                    );
                }
                return;
            }
            let range = y_max - y_min;
            let step = self.calculate_nice_step(range, 5);

            let mut y_val = (y_min / step).ceil() * step;
            while y_val <= y_max {
                let y = self.value_to_y(y_val, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    painter.line_segment(
                        [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                        Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
                    );
                }
                y_val += step;
            }
        }
    }

    /// Draw vertical grid lines aligned with main chart
    /// Uses the same coordinate system as the main chart for seamless grid appearance
    fn draw_vertical_grid_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        coords: &IndicatorCoordParams,
        grid_color: Color32,
    ) {
        // Create mapping for coordinate conversion (y range doesn't matter for grid)
        let mapping = coords.to_mapping(rect, 0.0, 1.0);

        // Calculate grid interval based on bar spacing (same algorithm as main chart)
        let min_pixel_spacing = layout::MIN_GRID_SPACING;
        let bars_per_grid = (min_pixel_spacing / coords.bar_spacing).ceil().max(1.0) as usize;

        // Round to nice intervals: 1, 2, 5, 10, 20, 50, 100, etc.
        let bars_per_grid = Self::nice_interval(bars_per_grid);

        // Calculate visible bar range
        let chart_width = rect.width();
        let visible_bars = (chart_width / coords.bar_spacing).ceil() as usize + 2;

        // Use start_idx from coords
        let start_idx = coords.start_idx;

        // Find the first grid line that's visible
        let first_visible_idx = if start_idx > 0 {
            (start_idx / bars_per_grid) * bars_per_grid
        } else {
            0
        };

        // Draw grid lines
        for i in 0..=(visible_bars / bars_per_grid + 2) {
            let grid_idx = first_visible_idx + i * bars_per_grid;

            // Convert bar index to x coord using ChartMapping
            let x = mapping.idx_to_x(grid_idx);

            // Skip if outside visible area
            if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
                continue;
            }

            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );
        }
    }

    /// Round to a nice interval (1, 2, 5, 10, 20, 50, 100, etc.)
    /// Uses the same fixed breakpoints as the main chart for consistent grid alignment
    fn nice_interval(raw: usize) -> usize {
        if raw <= 1 {
            return 1;
        }
        if raw <= 2 {
            return 2;
        }
        if raw <= 5 {
            return 5;
        }
        if raw <= 10 {
            return 10;
        }
        if raw <= 20 {
            return 20;
        }
        if raw <= 50 {
            return 50;
        }
        if raw <= 100 {
            return 100;
        }
        if raw <= 200 {
            return 200;
        }
        if raw <= 500 {
            return 500;
        }
        1000
    }

    fn draw_indicator_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        bars: &[Bar],
        visible_range: &Range<usize>,
        y_min: f64,
        y_max: f64,
    ) {
        if indicator.is_volume() {
            let count = visible_range.end.saturating_sub(visible_range.start);
            if count == 0 {
                return;
            }
            let spacing = rect.width() / count as f32;
            let coords = IndicatorCoordParams::new(
                spacing,
                0.0,
                visible_range.end.saturating_sub(1),
                visible_range.start,
            );
            super::volume::paint(
                painter,
                rect,
                indicator,
                visible_range,
                &coords.to_mapping(rect, y_min, y_max),
                y_min,
                y_max,
                None,
            );
            return;
        }
        let count = visible_range.end.saturating_sub(visible_range.start);
        if count == 0 {
            return;
        }
        let spacing = rect.width() / count as f32;
        let coords = IndicatorCoordParams::new(
            spacing,
            0.0,
            visible_range.end.saturating_sub(1),
            visible_range.start,
        );
        let mapping = coords.to_mapping(rect, y_min, y_max);
        crate::chart::renderers::IndicatorRenderer::render_study(
            &crate::chart::renderers::RenderContext::new(painter, rect),
            indicator,
            bars,
            0,
            &mapping.price,
            &mapping,
        );
    }

    fn draw_indicator_lines_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        bars: &[Bar],
        y_min: f64,
        y_max: f64,
        coords: &IndicatorCoordParams,
    ) {
        let mut mapping = coords.to_mapping(rect, y_min, y_max);
        if let Some(map) = self.price_map {
            mapping.price = map;
        }
        crate::chart::renderers::IndicatorRenderer::render_study(
            &crate::chart::renderers::RenderContext::new(painter, rect),
            indicator,
            bars,
            0,
            &mapping.price,
            &mapping,
        );
    }

    fn draw_y_axis_labels(
        &mut self,
        painter: &egui::Painter,
        full_rect: Rect,
        chart_rect: Rect,
        y_min: f64,
        y_max: f64,
        indicator: &dyn Indicator,
        axis: Option<crate::scales::PriceScaleId>,
        crosshair_tag: Option<Rect>,
    ) -> f32 {
        if indicator.scale() == crate::studies::StudyScale::None {
            return 0.0;
        }
        let Some(axis) = axis else {
            return 0.0;
        };
        let left = axis == crate::scales::PriceScaleId::Left;
        let align = if left {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        };
        let axis_edge = if left {
            chart_rect.left()
        } else {
            full_rect.right()
        };
        let text_color = painter.ctx().active_theme().chart.axis_text;
        let map = self
            .price_map
            .unwrap_or_else(|| {
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(y_min, y_max),
                    chart_rect.top(),
                    chart_rect.bottom(),
                    false,
                )
            })
            .with_price_precision(Some(indicator.precision().unwrap_or(2).min(255) as u8));
        let mut tags = self.axis_tags.clone();
        tags.push(crosshair_tag);
        if self.config.show_values {
            if indicator.is_volume() {
                super::volume::tags(painter, chart_rect, &map, indicator, axis_edge, &mut tags);
            } else {
                crate::chart::rendering::axes::render_study_tags(
                    painter, chart_rect, &map, indicator, axis_edge, &mut tags, None,
                );
            }
        }
        let mut width = if indicator.is_volume() {
            0.0
        } else {
            crate::chart::rendering::axes::price_axis_width(
                painter,
                &map,
                &[],
                self.price_formatter.as_deref(),
            )
        };
        for tag in tags.iter().flatten() {
            width = width.max(tag.width() + 5.0 + 8.0);
        }
        self.axis_tags = tags.clone();
        if self.overlay {
            return width;
        }
        let label_x = if left {
            chart_rect.left() - DESIGN_TOKENS.spacing.sm
        } else {
            chart_rect.right() + DESIGN_TOKENS.spacing.sm
        };

        for mark in crate::chart::renderers::price_marks(&map, self.price_formatter.as_deref()) {
            let y = mark.y_coord;
            if y >= full_rect.min.y + 10.0 && y <= full_rect.max.y - 10.0 {
                let label = if self.price_formatter.is_some() {
                    mark.label
                } else {
                    indicator.format_value(mark.price)
                };
                let galley = painter.layout_no_wrap(
                    label,
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
                width = width.max(galley.size().x + 5.0 + 8.0);
                let bounds = align.anchor_size(Pos2::new(label_x, y), galley.size());
                if tags.iter().flatten().any(|tag| tag.intersects(bounds)) {
                    continue;
                }
                painter.text(
                    Pos2::new(label_x, y),
                    align,
                    galley.text(),
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
            }
        }
        width
    }

    /// Layout each value with its plot colour, using a swatch when text would be faint.
    fn legend_job(
        &self,
        indicator: &dyn Indicator,
        text_color: Color32,
        background: Color32,
    ) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        let format =
            |color| egui::TextFormat::simple(egui::FontId::proportional(typography::MD), color);
        if self.config.show_name {
            job.append(indicator.name(), 0.0, format(text_color));
        }
        if !self.config.inputs.is_empty() {
            if !job.text.is_empty() {
                job.append(" ", 0.0, format(text_color));
            }
            job.append(&self.config.inputs, 0.0, format(text_color));
        }
        if self.config.show_values
            && let Some(bar) = self
                .bar_index
                .or_else(|| indicator.values().len().checked_sub(1))
        {
            for line in (0..indicator.line_cnt()).filter(|line| indicator.plot_style(*line).visible)
            {
                if let Some(value) = indicator.plot_value(line, bar) {
                    if !job.text.is_empty() {
                        job.append(" ", 0.0, format(text_color));
                    }
                    let color = indicator.plot_style(line).color;
                    let readable = crate::theme::contrast(color, background) >= 4.5;
                    if !readable {
                        let mut swatch = format(color);
                        swatch.background = color;
                        swatch.font_id.size = 8.0;
                        swatch.valign = egui::Align::Center;
                        job.append("  ", 0.0, swatch);
                        job.append(" ", 0.0, format(text_color));
                    }
                    let label = indicator.format_value(value);
                    job.append(
                        &label,
                        0.0,
                        format(if readable { color } else { text_color }),
                    );
                }
            }
        }
        job
    }

    /// Reserves three controls after the legend, keeping them inside short panes.
    fn legend_controls_rect(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
    ) -> Option<Rect> {
        if !self.config.show_legend {
            return None;
        }
        let theme = painter.ctx().active_theme();
        let galley = painter.layout_job(self.legend_job(indicator, theme.ui.text, theme.chart.bg));
        let x =
            (rect.left() + DESIGN_TOKENS.spacing.sm + galley.size().x + DESIGN_TOKENS.spacing.md)
                .min(rect.right() - if self.pane_controls { 120.0 } else { 80.0 })
                .max(rect.left());
        Some(
            Rect::from_min_size(
                Pos2::new(x, rect.top() + self.legend_offset + 2.0),
                egui::vec2(80.0, 20.0),
            )
            .intersect(rect),
        )
    }

    /// Draws the selected bar's study values beside its name and inputs.
    fn draw_legend(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        text_color: Color32,
    ) {
        if !self.config.show_legend {
            return;
        }
        let galley = painter.layout_job(self.legend_job(
            indicator,
            text_color,
            painter.ctx().active_theme().chart.bg,
        ));
        let pos = Pos2::new(
            rect.left() + DESIGN_TOKENS.spacing.sm,
            rect.top() + self.legend_offset + 12.0_f32.min(rect.height() / 2.0)
                - galley.size().y / 2.0,
        );
        painter.galley(pos, galley, text_color);
    }

    fn value_to_y(&self, value: f64, rect: Rect, y_min: f64, y_max: f64) -> f32 {
        if let Some(map) = self.price_map {
            map.price_to_y(value)
        } else {
            let norm = (value - y_min) / (y_max - y_min);
            rect.max.y - (norm as f32 * rect.height())
        }
    }

    fn calculate_nice_step(&self, range: f64, target_lines: usize) -> f64 {
        let raw_step = range / target_lines as f64;
        let magnitude = 10f64.powf(raw_step.log10().floor());
        let normalized = raw_step / magnitude;

        let nice = if normalized <= 1.0 {
            1.0
        } else if normalized <= 2.0 {
            2.0
        } else if normalized <= 5.0 {
            5.0
        } else {
            10.0
        };

        nice * magnitude
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::studies::{CustomIndicator, IndicatorValue};

    #[test]
    fn candle_panes_fit_the_visible_period_extremes() {
        let mut study = crate::studies::ConfiguredStudy::new(Box::new(
            crate::studies::HigherIntervalCandles::default(),
        ));
        study.set_timeframe(crate::model::Timeframe::Hour1);
        study.set_overlay(false);
        let start: chrono::DateTime<chrono::Utc> = "2024-03-11T00:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..3)
            .map(|i| {
                Bar::new(
                    start + chrono::TimeDelta::hours(i),
                    10.0,
                    20.0,
                    1.0,
                    15.0,
                    1.0,
                )
            })
            .collect();
        study.calculate(&bars);
        let pane = IndicatorPane::with_config(IndicatorPaneConfig::from_study(&study));
        for range in [0..1, 1..2, 2..3] {
            let (low, high) = pane.calculate_y_range(&study, &range);
            assert!(low <= 1.0 && high >= 20.0, "{range:?}: {low}..{high}");
        }
        let mut settings = study.settings();
        settings.plots[0].visible = false;
        study.apply_settings(&[], settings).unwrap();
        assert_eq!(pane.calculate_y_range(&study, &(0..3)), (0.0, 100.0));
    }

    #[test]
    fn rsi_default_paints_only_the_middle_band_and_two_guides() {
        let config = IndicatorPaneConfig::rsi();
        assert_eq!(config.height, 138.0);
        assert_eq!(
            config.zones.iter().map(|z| z.0).collect::<Vec<_>>(),
            vec![70.0, 30.0]
        );
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::dark());
        for inverted in [false, true] {
            let mut pane = IndicatorPane::with_config(config.clone());
            pane.price_map = Some(PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 100.0),
                0.0,
                100.0,
                inverted,
            ));
            let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 100.0));
            let study = crate::studies::RSI::default();
            assert_eq!(study.colors(), vec![Color32::from_rgb(41, 98, 255)]);
            assert_eq!(study.plot_style(0).width, 2.0);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                pane.draw_zone_fills(ui.painter(), rect, 0.0, 100.0, &study);
                pane.draw_horizontal_lines(
                    ui.painter(),
                    rect,
                    0.0,
                    100.0,
                    Color32::BLACK,
                    Color32::GREEN,
                    Color32::RED,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let bands: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) => Some(r),
                    _ => None,
                })
                .collect();
            assert_eq!(bands.len(), 1);
            assert!((bands[0].rect.top() - 30.0).abs() < 0.001);
            assert!((bands[0].rect.bottom() - 70.0).abs() < 0.001);
            assert_eq!(
                bands[0].fill,
                Color32::from_rgba_unmultiplied(41, 98, 255, 51)
            );
            let guides: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::LineSegment { points, stroke }
                        if stroke.color == Color32::from_gray(128) =>
                    {
                        Some(points)
                    }
                    _ => None,
                })
                .collect();
            assert!(!guides.is_empty());
            for guide in guides {
                assert!((guide[0].y - 30.0).abs() < 0.001 || (guide[0].y - 70.0).abs() < 0.001);
                assert!((guide[1].x - guide[0].x - 4.0).abs() < 0.001);
            }
        }
    }

    #[test]
    fn volume_units_reach_pane_axes_and_legends() {
        use crate::studies::{OnBalanceVolume, StudyFormat};
        let bars: Vec<_> = [10.0, 11.0, 12.0]
            .into_iter()
            .map(|price| Bar::new(chrono::Utc::now(), price, price, price, price, 1_000_000.0))
            .collect();
        let shifted = CustomIndicator::new("Shifted", |_| {
            vec![
                IndicatorValue::Single(0.0),
                IndicatorValue::Single(1_000_000.0),
                IndicatorValue::Single(2_000_000.0),
            ]
        })
        .with_value_format(StudyFormat::Volume)
        .with_offsets(vec![1]);
        for (mut study, expected) in [
            (
                Box::new(OnBalanceVolume::new()) as Box<dyn Indicator>,
                "2.00M",
            ),
            (Box::new(shifted), "1.00M"),
        ] {
            study.calculate(&bars);
            let mut pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
            let job = pane.legend_job(study.as_ref(), Color32::WHITE, Color32::BLACK);
            assert!(job.text.contains(expected), "{}", job.text);
            let ctx = egui::Context::default();
            let mut study = crate::studies::ConfiguredStudy::new(study);
            study.set_overlay(true);
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(0);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
                pane.draw_y_axis_labels(
                    ui.painter(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(300.0, 300.0)),
                    0.0,
                    3_000_000.0,
                    studies[0].as_ref(),
                    Some(crate::scales::PriceScaleId::Right),
                    None,
                );
                let mut legend = crate::chart::renderers::Legend::new("SAMPLE", "1h", &bars[2]);
                legend.studies = &studies;
                legend.bar_index = 2;
                legend.instrument = Some(&instrument);
                legend.show(ui, rect);
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            // Both the last-value tag and overlay legend use displayed values and study units.
            assert_eq!(
                labels
                    .iter()
                    .filter(|label| label.as_str() == expected)
                    .count(),
                2,
                "{labels:?}"
            );
        }
    }

    #[test]
    fn legend_values_identify_each_plot_without_low_contrast_text() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let colors = vec![Color32::BLUE, Color32::YELLOW, Color32::GREEN];
            let mut study = CustomIndicator::new("Lines", |_| {
                vec![IndicatorValue::Multiple(vec![1.0, 2.0, 3.0])]
            })
            .with_colors(colors.clone())
            .with_line_cnt(3);
            study.calculate(&[]);
            let pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                pane.draw_legend(ui.painter(), ui.max_rect(), &study, theme.ui.text);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let text = shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t),
                    _ => None,
                })
                .unwrap();
            let sections = &text.galley.job.sections;
            for (i, color) in colors.iter().enumerate() {
                let value = format!("{:.2}", (i + 1) as f64);
                let section = sections
                    .iter()
                    .find(|s| {
                        text.galley.job.text[s.byte_range.start.0..s.byte_range.end.0]
                            .contains(&value)
                    })
                    .unwrap();
                assert!(crate::theme::contrast(section.format.color, theme.chart.bg) >= 4.5);
                assert!(
                    sections.iter().any(|s| s.format.color == *color),
                    "missing plot colour {color:?}"
                );
            }
            assert_eq!(sections[0].format.color, theme.ui.text);
        }
    }
}
