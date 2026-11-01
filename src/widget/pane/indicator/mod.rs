//! Separate-pane indicator rendering with direct egui painting.
//!
//! This module renders oscillator and multi-line indicators (RSI, MACD, Stochastic,
//! etc.) in dedicated panels below the main chart. It paints directly through
//! [`egui::Painter`] using the same horizontal coordinate mapping as the main
//! candlestick chart.
//!
//! # Usage
//!
//! Indicator panes are typically drawn by [`crate::widget::Chart::show`], but
//! they can also be used standalone:
//!
//! ```rust
//! use egui_charts::widget::{
//!     IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig
//! };
//!
//! # fn example(
//! #     ui: &mut egui::Ui,
//! #     indicator: &egui_charts::studies::CustomIndicator,
//! #     bars: &[egui_charts::model::Bar],
//! #     visible_range: std::ops::Range<usize>,
//! #     coords: IndicatorCoordParams,
//! # ) {
//! let config = IndicatorPaneConfig::default();
//! let mut pane = IndicatorPane::with_config(config);
//! pane.show_aligned(ui, indicator, bars, visible_range, coords);
//! # }
//! ```
//!
//! # Coordinate Alignment
//!
//! The [`IndicatorCoordParams`] struct captures the main chart's bar spacing,
//! right offset, and base index so that indicator panes draw their data points
//! at exactly the same x-coordinates as the corresponding candlesticks above.

mod config;
mod paint;
pub use config::{IndicatorCoordParams, IndicatorPaneConfig, PaneInteraction};

use crate::localization as l10n;
use crate::model::Bar;
use crate::scales::PriceMap;
use crate::studies::Indicator;
use crate::theme::ThemeExt;
use egui::{Pos2, Rect, Sense, Ui};
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
    pub(super) axis_rect: Option<Rect>,
    pub(super) axis_tags: Vec<Option<Rect>>,
    pub(super) axis_options: crate::config::PriceAxisOptions,
    pub(super) legend_backdrop: f32,
}

impl IndicatorPane {
    /// Create a new indicator panel with custom configuration
    pub fn with_config(config: IndicatorPaneConfig) -> Self {
        let axis_options = crate::config::PriceAxisOptions {
            show_study_values: config.show_values,
            ..Default::default()
        };
        Self {
            config,
            bar_index: None,
            measured_axis_width: 0.0,
            price_map: None,
            price_formatter: None,
            overlay: false,
            legend_offset: 0.0,
            pane_controls: false,
            axis_rect: None,
            axis_tags: Vec::new(),
            axis_options,
            legend_backdrop: 0.6,
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

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::hover());

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
        self.draw_horizontal_lines(painter, chart_rect, y_min, y_max, grid_color, indicator);

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
        if let Some(table) = indicator.table() {
            crate::studies::table::paint(ui, chart_rect, table, response.id);
        }
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

        // Allocate the panel area
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::hover());

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
        self.draw_horizontal_lines(painter, chart_rect, y_min, y_max, grid_color, indicator);

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
        if let Some(table) = indicator.table() {
            crate::studies::table::paint(ui, chart_rect, table, response.id);
        }
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
    /// The returned [`PaneInteraction`] also carries the [`egui::Response`] for the
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
        let interaction = self.show_aligned_interactive_in(
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
        )?;
        if let Some(table) = indicator.table() {
            crate::studies::table::paint(
                ui,
                interaction.chart_rect,
                table,
                interaction.response.id,
            );
        }
        Some(interaction)
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
        primitives: &[&crate::widget::primitives::Attached],
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

        // Allocate the panel area with click sense
        let available_width = ui.available_width();
        let panel_size = egui::vec2(available_width, self.config.height);
        let (rect, response) = ui.allocate_exact_size(panel_size, Sense::click());
        let translated_label =
            l10n::format_text(ui.ctx(), "{0} pane", &[("0", indicator.name().to_string())]);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::GraphicsObject,
                response.enabled(),
                &translated_label,
            )
        });

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
                    item.draw(painter, view, pass);
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
                        (0..indicator.line_count())
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
                indicator,
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
                let axis_rect = self.axis_rect.unwrap_or(axis_rect);
                let axis_edge = self.axis_rect.map_or(axis_edge, |axis| axis.right());
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
                                self.axis_options.allow_label_overlap,
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
        if indicator.is_volume()
            && !matches!(indicator.scale(), crate::studies::StudyScale::Shared(_))
        {
            return crate::widget::volume::range(indicator, visible_range, None);
        }
        if let Some((min, max)) = self.config.y_range {
            return (min, max);
        }

        // Auto-scale based on visible indicator values
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;

        for line in (0..indicator.line_count()).filter(|line| {
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

        for projection in indicator.projections().iter().filter(|projection| {
            projection.bars.end > visible_range.start as f64
                && projection.bars.start < visible_range.end as f64
                && indicator.plot_style(projection.plot).visible
        }) {
            min_val = min_val.min(projection.bottom);
            max_val = max_val.max(projection.top);
        }

        #[cfg(feature = "drawings")]
        for price in crate::studies::plots::drawing_prices(indicator, visible_range.clone()) {
            min_val = min_val.min(price);
            max_val = max_val.max(price);
        }

        if min_val != f64::MAX
            && (0..indicator.line_count()).any(|line| {
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
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;

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
}
