//! The primary Chart widget for embedding interactive financial charts in egui.
//!
//! This module provides [`Chart`], the main entry point for rendering OHLCV
//! (Open-High-Low-Close-Volume) financial data as an interactive egui widget.
//! It supports multiple chart types (candlestick, line, area, bar, Renko, Kagi),
//! real-time data streaming, drawing tools, technical indicators, and
//! multi-chart synchronization.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use egui_charts::widget::Chart;
//! use egui_charts::model::BarData;
//!
//! // Create chart with OHLCV data
//! let mut chart = Chart::new(bar_data);
//!
//! // Show the chart in your egui update loop
//! egui::CentralPanel::default().show(ctx, |ui| {
//!     chart.show(ui);
//! });
//! ```
//!
//! # Builder Pattern
//!
//! Configure the chart before showing it:
//!
//! ```rust,ignore
//! let mut chart = Chart::new(data)
//!     .visible_bars(100)          // Show 100 bars at a time
//!     .config(my_chart_config)    // Custom visual config
//!     .with_chart_options(opts);  // Scroll/zoom behavior
//! ```
//!
//! # Real-Time Updates
//!
//! For live data feeds, reuse the chart instance across frames:
//!
//! ```rust,ignore
//! // In your app struct:
//! struct MyApp {
//!     chart: Chart,
//! }
//!
//! // On new data:
//! let auto_scrolled = self.chart.update_data(new_bar_data);
//! self.chart.show(ui);
//! ```
//!
//! # Drawing Tools and Indicators
//!
//! ```rust,ignore
//! // With drawing tools and indicators:
//! chart.show_with_indicators(
//!     ui,
//!     Some(&mut drawing_manager),
//!     Some(&indicator_registry),
//! );
//! ```
//!
//! # Multi-Chart Sync
//!
//! Synchronize crosshairs and time scales between multiple charts:
//!
//! ```rust,ignore
//! // Emit crosshair position from chart A
//! let hover_idx = chart_a.get_hover_bar_idx();
//!
//! // Apply to chart B
//! chart_b.set_synced_crosshair_bar_idx(hover_idx);
//!
//! // Sync time scales
//! let (spacing, offset) = chart_a.get_time_scale_state();
//! chart_b.apply_synced_time_scale(spacing, offset);
//! ```
//!
//! # Sub-modules
//!
//! - [`builder`] -- Constructor and configuration methods for [`Chart`]
//! - [`indicator_pane`] -- Separate indicator panels (RSI, MACD, Stochastic)

use crate::chart::cursor_modes::CursorModeState;
use crate::chart::renderers::{self, ChartMapping, PriceScale, RenderContext, StyleColors};
use crate::chart::series::SeriesSettings;
use crate::config::{BackgroundStyle, ChartConfig, ChartOptions, WatermarkPos};
use crate::drawings::DrawingManager;
use crate::model::ChartState;
use crate::model::ChartType;
use crate::scales::TimeFormatterBuilder;
use crate::studies::IndicatorRegistry;
use crate::validation::DataValidator;
pub mod indicator_pane;
use crate::styles::{sizing, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};
pub use indicator_pane::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig};

// Re-export from logic layer
use crate::chart::{helpers, rendering, state};

pub mod builder;

pub use helpers::{apply_price_zoom, y_to_price};
pub use state::{BoxZoomMode, BoxZoomState, ElasticBounceState, KineticScrollState};

/// Interactive financial chart widget for egui.
///
/// `Chart` is the core rendering and interaction engine for displaying OHLCV
/// financial data. It handles all aspects of chart visualization including
/// candlestick/bar/line rendering, pan/zoom interactions, crosshair display,
/// drawing tool integration, and indicator overlays.
///
/// # Creating a Chart
///
/// Use [`Chart::new`] or [`Chart::with_config`] to construct, then call
/// [`Chart::show`] each frame to render:
///
/// ```rust,ignore
/// use egui_charts::widget::Chart;
/// use egui_charts::model::BarData;
///
/// let mut chart = Chart::new(bar_data);
///
/// egui::CentralPanel::default().show(ctx, |ui| {
///     let response = chart.show(ui);
///     // response can be used for additional interaction handling
/// });
/// ```
///
/// # Supported Chart Types
///
/// - **Candles** -- Standard Japanese candlestick chart
/// - **Bars** -- OHLC bar chart
/// - **Line** -- Close-price line chart
/// - **Area** -- Filled area under the close-price line
/// - **Renko** -- Fixed-size brick chart (set brick size with [`Chart::set_renko_brick_size`])
/// - **Kagi** -- Reversal-based chart (set reversal amount with [`Chart::set_kagi_reversal_amount`])
///
/// # Interaction Features
///
/// - **Pan**: Click and drag to scroll through history (with kinetic scrolling)
/// - **Zoom**: Mouse wheel to zoom in/out, pinch-to-zoom on trackpads
/// - **Box zoom**: Drag-select a region to zoom into (when zoom mode is active)
/// - **Price scale drag**: Drag the price axis to scale vertically
/// - **Crosshair**: Hover to see price/time at cursor position
/// - **Keyboard shortcuts**: Arrow keys, Home/End, +/- for navigation
/// - **Double-click**: Reset zoom on price or time axis
///
/// # Architecture
///
/// `Chart` owns a [`ChartState`] (data + coordinate systems) and a
/// [`ChartConfig`] (visual styling). Rendering is delegated to specialized
/// modules in `crate::chart::rendering`, while interaction logic lives in
/// `crate::chart::state`.
pub struct Chart {
    /// Backend state holding OHLCV data and coordinate system (time scale, price range).
    pub state: ChartState,
    /// Visual configuration controlling colors, padding, grid visibility, and more.
    pub config: ChartConfig,
    /// Chart behavior options (bar spacing, scroll/zoom constraints, time scale settings).
    pub chart_options: ChartOptions,
    /// Starting index of visible range (for backward compatibility)
    pub(crate) start_idx: usize,
    /// Desired number of visible bars (from app state)
    pub(crate) desired_visible_bars: Option<usize>,
    /// Cache of last computed visible bars for external syncing
    pub(crate) last_visible_bars: usize,
    /// Whether to apply `desired_visible_bars` on next frame only
    pub(crate) apply_visible_bars_once: bool,
    /// Kinetic scroll animation state (UI state only)
    pub(crate) kinetic_scroll: KineticScrollState,
    /// Last scroll position for drag tracking (UI state only)
    pub(crate) scroll_start_pos: Option<Pos2>,
    /// Initial right offset when starting scroll (for drag) (UI state only)
    pub(crate) scroll_start_offset: Option<f32>,
    /// Previous widget width for resize handling (UI state only)
    pub(crate) prev_width: Option<f32>,
    /// Drag state for price-axis scaling (UI state only)
    pub(crate) price_scale_drag_start: Option<Pos2>,
    /// Apply external start-index once to time scale (UI state only)
    pub(crate) pending_start_idx: Option<usize>,
    /// Chart type (candlestick, line, area, bar, Renko, Kagi)
    pub(crate) chart_type: ChartType,
    /// Renko brick size (for Renko charts)
    pub(crate) renko_brick_size: f64,
    /// Kagi reversal amount (for Kagi charts)
    pub(crate) kagi_reversal_amount: f64,
    /// Whether tracking mode is currently active
    pub(crate) tracking_mode_active: bool,
    /// Mouse entered chart area (for tracking mode exit detection)
    pub(crate) mouse_in_chart: bool,
    /// Data validator for detecting data mismatches
    pub(crate) validator: Option<DataValidator>,
    /// Right-click box zoom state
    pub(crate) box_zoom: BoxZoomState,
    /// Whether zoom mode is currently active (controlled by zoom toolbar button)
    pub(crate) zoom_mode_active: bool,
    /// Whether zoom was just applied in the last frame (for auto-deactivation)
    pub(crate) zoom_just_applied: bool,
    /// Current symbol being displayed (for legend)
    pub(crate) symbol: String,
    /// Current timeframe (for legend)
    pub(crate) timeframe: String,
    /// Cursor mode state (Demonstration, Magic, Eraser effects)
    #[doc(hidden)]
    pub cursor_modes: CursorModeState,
    /// Last rendered price range (includes zoom adjustments) for external use
    pub(crate) last_rendered_price_range: (f64, f64),
    /// Last rendered price rect (actual rect used for candle rendering).
    /// Use [`get_rendered_price_rect`](Chart::get_rendered_price_rect) instead.
    #[doc(hidden)]
    pub last_rendered_price_rect: Rect,
    /// Last rendered volume rect (actual rect used for volume rendering)
    pub(crate) last_rendered_volume_rect: Rect,
    /// Last rendered indicator pane info for hit testing
    /// Each entry: (indicator_index, panel_rect, chart_rect, y_min, y_max, coords)
    pub(crate) last_rendered_indicator_panes: Vec<RenderedIndicatorPane>,
    // =========================================================================
    // Multi-Chart Sync State
    // =========================================================================
    /// External crosshair position from synced chart (bar index)
    pub(crate) synced_crosshair_bar_idx: Option<f64>,
    /// Last computed hover bar index (for sync emission to other charts)
    pub(crate) last_hover_bar_idx: Option<f64>,

    // =========================================================================
    // Marks (Widget API)
    // =========================================================================
    /// Bar marks (annotations on chart bars, e.g., trade signals)
    pub marks: Vec<crate::model::Marker>,
    /// Timescale marks (annotations on the time axis)
    pub timescale_marks: Vec<crate::model::Marker>,
}

/// Information about a rendered indicator pane, used for hit testing and coordinate mapping.
///
/// After calling [`Chart::show_with_indicators`], each visible separate-pane indicator
/// (RSI, MACD, etc.) produces a `RenderedIndicatorPane` entry stored in the chart.
/// Platform code can use these to implement click-on-indicator-line selection,
/// tooltip display, or other interactive features.
///
/// Retrieve with [`Chart::get_rendered_indicator_panes`].
#[derive(Clone, Debug)]
pub struct RenderedIndicatorPane {
    /// Index of the indicator in the registry
    pub indicator_idx: usize,
    /// Full panel rect (including y-axis labels)
    pub panel_rect: Rect,
    /// Chart drawing area rect (excluding y-axis labels)
    pub chart_rect: Rect,
    /// Y-axis minimum value
    pub y_min: f64,
    /// Y-axis maximum value
    pub y_max: f64,
    /// Coordinate parameters for x-axis calculation
    pub coords: IndicatorCoordParams,
}

/// Pre-computed layout rectangles for the chart's sub-regions.
///
/// Use [`Chart::calculate_layout_rects`] to obtain these rects for a given
/// widget area. They are useful for external hit-testing (e.g., determining
/// whether a click landed on the price area, volume area, or legend).
#[derive(Clone, Copy, Debug)]
pub struct ChartLayoutRects {
    /// The overall widget rect (entire chart area including axes and padding).
    pub widget_rect: Rect,
    /// The main price/candle area where OHLC data is rendered.
    pub price_rect: Rect,
    /// The volume sub-area below the price chart (empty if volume is hidden).
    pub volume_rect: Rect,
    /// The legend/OHLC info area at the top of the chart.
    pub legend_rect: Rect,
}

impl Default for ChartLayoutRects {
    fn default() -> Self {
        Self {
            widget_rect: Rect::NOTHING,
            price_rect: Rect::NOTHING,
            volume_rect: Rect::NOTHING,
            legend_rect: Rect::NOTHING,
        }
    }
}

impl Chart {
    /// Calculate the layout sub-rects for a given widget rect.
    ///
    /// Given the overall widget area, this computes where the price chart,
    /// volume bars, and legend/OHLC header will be drawn. Useful for
    /// external hit-testing, overlay placement, or custom drawing on top
    /// of specific chart regions.
    ///
    /// The layout respects current config flags like `show_ohlc_info`,
    /// `show_time_labels`, and `show_volume`.
    pub fn calculate_layout_rects(&self, widget_rect: Rect) -> ChartLayoutRects {
        let bottom_padding = if self.config.show_time_labels {
            30.0
        } else {
            20.0
        };
        let top_padding = if self.config.show_ohlc_info {
            40.0
        } else {
            20.0
        };
        let right_padding = self.config.padding * 2.0;

        // Legend rect is at the top of the widget
        let legend_rect = if self.config.show_ohlc_info {
            Rect::from_min_size(
                widget_rect.min + Vec2::new(self.config.padding, 4.0),
                Vec2::new(widget_rect.width() * 0.7, top_padding - 8.0),
            )
        } else {
            Rect::NOTHING
        };

        let chart_rect = Rect::from_min_size(
            widget_rect.min + Vec2::new(self.config.padding, top_padding),
            Vec2::new(
                widget_rect.width() - self.config.padding - right_padding,
                widget_rect.height() - top_padding - bottom_padding,
            ),
        );

        let (price_rect, volume_rect) = if self.config.show_volume {
            let split_y =
                chart_rect.min.y + chart_rect.height() * (1.0 - self.config.volume_height_fraction);
            (
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.max.x, split_y)),
                Rect::from_min_max(Pos2::new(chart_rect.min.x, split_y), chart_rect.max),
            )
        } else {
            (chart_rect, Rect::NOTHING)
        };

        ChartLayoutRects {
            widget_rect,
            price_rect,
            volume_rect,
            legend_rect,
        }
    }

    /// Activates or deactivates box-zoom mode.
    ///
    /// When active, left-click drag draws a selection rectangle and zooms into
    /// that region. The mode auto-deactivates after a successful zoom operation
    /// (check with [`Chart::zoom_was_applied`]).
    pub fn set_zoom_mode(&mut self, active: bool) {
        self.zoom_mode_active = active;
    }

    /// Returns `true` if a box-zoom was completed in the most recent frame.
    ///
    /// Use this to auto-deactivate zoom mode in your toolbar after the user
    /// completes a zoom selection.
    pub fn zoom_was_applied(&self) -> bool {
        self.zoom_just_applied
    }

    /// Sets the trading symbol displayed in the chart legend (e.g., "BTCUSD", "AAPL").
    pub fn set_symbol(&mut self, symbol: &str) {
        self.symbol = symbol.to_string();
    }

    /// Sets the timeframe label displayed in the chart legend (e.g., "1H", "1D", "1W").
    pub fn set_timeframe_label(&mut self, timeframe: &str) {
        self.timeframe = timeframe.to_string();
    }

    /// Sets the crosshair rendering style (Full, Dot, or Arrow).
    ///
    /// Use this to connect a toolbar cursor-type selector to the chart.
    /// The style controls how the crosshair lines and labels are drawn
    /// when the user hovers over the chart area.
    pub fn set_crosshair_style(&mut self, style: crate::config::CrosshairStyle) {
        self.chart_options.crosshair.style = style;
    }

    /// Apply series settings to chart colors and price source.
    ///
    /// Copies candlestick colors (bullish/bearish fill, border, wick) and the
    /// price source field from the given [`SeriesSettings`] into the chart's
    /// [`ChartConfig`]. Call this when the user changes series appearance in a
    /// settings dialog.
    pub fn apply_series_settings(&mut self, settings: &SeriesSettings) {
        self.config.bullish_color = settings.bullish_color;
        self.config.bearish_color = settings.bearish_color;
        self.config.bullish_border_color = settings.bullish_border_color;
        self.config.bearish_border_color = settings.bearish_border_color;
        self.config.bullish_wick_color = settings.bullish_wick_color;
        self.config.bearish_wick_color = settings.bearish_wick_color;
        self.config.price_source = settings.price_source;
    }

    /// Draw the chart background (solid or gradient)
    fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        // Skip background when chart is inside a container that handles its own background
        if self.config.skip_background {
            return;
        }

        match self.config.background_style {
            BackgroundStyle::Solid => {
                painter.rect_filled(rect, 0.0, self.config.background_color);
            }
            BackgroundStyle::VerticalGradient {
                top_color,
                bottom_color,
            } => {
                // Draw vertical gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
            BackgroundStyle::HorizontalGradient {
                left_color,
                right_color,
            } => {
                // Draw horizontal gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
        }
    }

    /// Draw watermark overlay (large symbol name)
    fn draw_watermark(&self, painter: &egui::Painter, rect: Rect) {
        if !self.config.show_watermark {
            return;
        }

        let text = self.config.watermark_text.as_deref().unwrap_or_else(|| {
            if self.symbol.is_empty() {
                "SYMBOL"
            } else {
                &self.symbol
            }
        });

        let font_id = egui::FontId::proportional(self.config.watermark_font_size);

        // Calculate position based on watermark_pos
        let pos = match self.config.watermark_pos {
            WatermarkPos::Center => rect.center(),
            WatermarkPos::TopLeft => Pos2::new(
                rect.min.x + 20.0,
                rect.min.y + self.config.watermark_font_size,
            ),
            WatermarkPos::TopRight => Pos2::new(
                rect.max.x - 20.0,
                rect.min.y + self.config.watermark_font_size,
            ),
            WatermarkPos::BottomLeft => Pos2::new(rect.min.x + 20.0, rect.max.y - 20.0),
            WatermarkPos::BottomRight => Pos2::new(rect.max.x - 20.0, rect.max.y - 20.0),
        };

        let anchor = match self.config.watermark_pos {
            WatermarkPos::Center => egui::Align2::CENTER_CENTER,
            WatermarkPos::TopLeft => egui::Align2::LEFT_TOP,
            WatermarkPos::TopRight => egui::Align2::RIGHT_TOP,
            WatermarkPos::BottomLeft => egui::Align2::LEFT_BOTTOM,
            WatermarkPos::BottomRight => egui::Align2::RIGHT_BOTTOM,
        };

        painter.text(pos, anchor, text, font_id, self.config.watermark_color);
    }

    /// Renders the chart with mouse interactions and optional drawing tools.
    ///
    /// This is the mid-level rendering method. Use this when you have drawing
    /// tools but no separate-pane indicators. For the simplest case, use
    /// [`Chart::show`]. For full functionality, use [`Chart::show_with_indicators`].
    pub fn show_with_drawings(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
    ) -> Response {
        self.show_internal(ui, drawing_manager, None)
    }

    /// Renders the chart with indicators and drawing tools.
    ///
    /// This is the most feature-complete rendering method. Overlay indicators
    /// (moving averages, Bollinger Bands, etc.) are drawn on the main price chart.
    /// Separate-pane indicators (RSI, MACD, Stochastic) are rendered in dedicated
    /// panels below the main chart with aligned x-axes.
    ///
    /// After rendering, use [`Chart::get_rendered_indicator_panes`] to access
    /// indicator pane layout information for hit testing.
    ///
    /// # Arguments
    ///
    /// * `ui` -- The egui UI to render into
    /// * `drawing_manager` -- Optional drawing tool manager for trend lines, etc.
    /// * `indicators` -- Optional indicator registry containing computed indicators
    pub fn show_with_indicators(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        // Clear previous frame's indicator pane info
        self.last_rendered_indicator_panes.clear();

        // Calculate total height needed for indicator panes FIRST
        // This allows us to reserve space before the main chart
        let indicator_pane_height = if let Some(indicators) = indicators {
            let mut total_height = 0.0f32;
            let mut pane_count = 0;

            for indicator in indicators.indicators() {
                if indicator.is_overlay() || !indicator.is_visible() {
                    continue;
                }
                pane_count += 1;
                let height = match indicator.name() {
                    "RSI" => IndicatorPaneConfig::rsi().height,
                    "MACD" => IndicatorPaneConfig::macd().height,
                    "Stochastic" => IndicatorPaneConfig::stochastic().height,
                    _ => IndicatorPaneConfig::default().height,
                };
                total_height += height;
            }

            if pane_count > 0 {
                // Add minimal gap between panes (seamless panes)
                total_height + 1.0 * pane_count as f32
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Calculate available height and reserve space for indicators
        let available = ui.available_size();
        let main_chart_height = (available.y - indicator_pane_height).max(200.0);

        // Allocate fixed height for main chart (prevents it from taking all space)
        let response = ui
            .allocate_ui_with_layout(
                egui::vec2(available.x, main_chart_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.show_internal(ui, drawing_manager, indicators),
            )
            .inner;

        // Render separate pane indicators below the main chart
        if let Some(indicators) = indicators {
            let (start_idx, end_idx) = self.state.visible_range();
            let visible_range = start_idx..end_idx;
            let bars = &self.state.data().bars;

            // Get coordinate parameters from time scale for x-axis alignment
            let time_scale = self.state.time_scale();
            let coords = IndicatorCoordParams::new(
                time_scale.bar_spacing(),
                time_scale.right_offset(),
                self.state.data().len().saturating_sub(1),
                start_idx,
            );

            let mut has_pane_indicators = false;
            for indicator in indicators.indicators() {
                if indicator.is_overlay() || !indicator.is_visible() {
                    continue;
                }
                has_pane_indicators = true;
                break;
            }

            if has_pane_indicators {
                for (idx, indicator) in indicators.indicators().iter().enumerate() {
                    if indicator.is_overlay() || !indicator.is_visible() {
                        continue;
                    }

                    // Minimal gap, no visible separator (seamless panes)
                    ui.add_space(DESIGN_TOKENS.spacing.hairline);

                    let config = match indicator.name() {
                        "RSI" => IndicatorPaneConfig::rsi(),
                        "MACD" => IndicatorPaneConfig::macd(),
                        "Stochastic" => IndicatorPaneConfig::stochastic(),
                        _ => IndicatorPaneConfig::default(),
                    };

                    let mut panel =
                        IndicatorPane::with_config(egui::Id::new("main_chart_x_axis"), config);

                    // Use show_aligned_interactive to get pane info for hit testing
                    if let Some((panel_rect, chart_rect, y_min, y_max, _response)) = panel
                        .show_aligned_interactive(
                            ui,
                            indicator.as_ref(),
                            bars,
                            visible_range.clone(),
                            coords,
                        )
                    {
                        // Store the pane info for hit testing by platform
                        self.last_rendered_indicator_panes
                            .push(RenderedIndicatorPane {
                                indicator_idx: idx,
                                panel_rect,
                                chart_rect,
                                y_min,
                                y_max,
                                coords,
                            });
                    }
                }
            }
        }

        response
    }

    /// Renders the chart with indicators using simple per-bar x-positioning.
    ///
    /// Unlike [`Chart::show_with_indicators`], which uses aligned coordinate
    /// parameters from the main chart's time scale, this method creates
    /// indicator panes with basic visible-range positioning. It is simpler
    /// but may not perfectly align indicator data points with the main chart
    /// when the user scrolls or zooms. Prefer [`Chart::show_with_indicators`]
    /// for production use.
    pub fn show_with_indicators_plot(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        let response = self.show_with_drawings(ui, drawing_manager);

        if let Some(indicators) = indicators {
            ui.separator();

            let (start_idx, end_idx) = self.state.visible_range();
            let visible_range = start_idx..end_idx;
            let bars = &self.state.data().bars;

            for indicator in indicators.indicators() {
                if indicator.is_overlay() {
                    continue;
                }

                if !indicator.is_visible() {
                    continue;
                }

                let config = match indicator.name() {
                    "RSI" => IndicatorPaneConfig::rsi(),
                    "MACD" => IndicatorPaneConfig::macd(),
                    "Stochastic" => IndicatorPaneConfig::stochastic(),
                    _ => IndicatorPaneConfig::default(),
                };

                let mut panel =
                    IndicatorPane::with_config(egui::Id::new("main_chart_x_axis"), config);

                panel.show(ui, indicator.as_ref(), bars, visible_range.clone());
            }
        }

        response
    }

    /// Renders the chart with standard mouse interactions.
    ///
    /// This is the simplest way to display a chart. It handles pan, zoom,
    /// crosshair, keyboard shortcuts, and all visual elements configured in
    /// [`ChartConfig`]. No drawing tools or separate-pane indicators are rendered.
    ///
    /// Returns an [`egui::Response`] for additional interaction handling.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// egui::CentralPanel::default().show(ctx, |ui| {
    ///     let response = chart.show(ui);
    ///     if response.hovered() {
    ///         // Chart is being hovered
    ///     }
    /// });
    /// ```
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        self.show_internal(ui, None, None)
    }

    /// Internal rendering method that orchestrates all modules
    pub(crate) fn show_internal(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        // Reset zoom_just_applied flag at the start of each frame
        self.zoom_just_applied = false;

        let available_size = ui.available_size();
        let (mut response, painter) = ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // Establish chart_rect FIRST before any operations
        let top_padding = if self.config.show_ohlc_info {
            sizing::chart::TOP_PADDING_WITH_OHLC
        } else {
            sizing::chart::TOP_PADDING_NO_OHLC
        };
        let bottom_padding = if self.config.show_time_labels {
            sizing::chart::BOTTOM_PADDING_WITH_TIME
        } else {
            sizing::chart::BOTTOM_PADDING_NO_TIME
        };
        let right_axis_width = sizing::chart::RIGHT_AXIS_WIDTH;

        let left_margin = sizing::chart::PADDING;
        let right_margin = sizing::chart::PADDING + right_axis_width;

        let chart_rect = Rect::from_min_size(
            rect.min + Vec2::new(left_margin, top_padding),
            Vec2::new(
                (rect.width() - left_margin - right_margin).max(sizing::chart::MIN_CHART_WIDTH),
                (rect.height() - top_padding - bottom_padding).max(sizing::chart::MIN_CHART_HEIGHT),
            ),
        );

        let chart_width = chart_rect.width();

        // CRITICAL: Apply TimeScale width configuration BEFORE any zoom operations
        // This ensures apply_constraints() inside zoom() uses the correct self.width
        // to calculate constraint bounds. Without this, drawings drift during zoom
        // because constraints are calculated with stale width values.
        self.apply_timescale_config(chart_width);

        // Handle tracking mode
        self.handle_tracking_mode(ui, &response);

        // Request focus on hover for keyboard shortcuts
        self.request_focus_if_needed(&mut response);

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui, &response, chart_width, chart_rect.min.x);

        // Calculate visible bars
        let logical_range = self.state.time_scale().visible_logical_range();
        let visible_bars = logical_range.length().ceil() as usize;
        self.last_visible_bars = visible_bars;

        // Show grabbing cursor during panning
        self.set_panning_cursor(ui, &response);

        // Define axis rects
        let price_axis_rect = Rect::from_min_max(
            Pos2::new(chart_rect.max.x, chart_rect.min.y),
            Pos2::new(rect.max.x, chart_rect.max.y),
        );
        let time_axis_rect = Rect::from_min_max(
            Pos2::new(chart_rect.min.x, chart_rect.max.y),
            Pos2::new(chart_rect.max.x, rect.max.y),
        );

        // Handle double-click to reset axes
        self.handle_double_click(&response, price_axis_rect, time_axis_rect);

        // Handle mouse wheel for zoom/scroll
        // Block pan/zoom when drawing tool is active OR when manipulating a drawing
        let is_drawing_interaction = drawing_manager.as_ref().is_some_and(|dm| {
            dm.active_tool.is_some() || dm.dragging_handle.is_some() || dm.curr_drawing.is_some()
        });
        let pending_price_zoom = self.handle_mouse_wheel(
            ui,
            &response,
            chart_width,
            chart_rect.min.x,
            price_axis_rect,
        );

        // Handle pinch-to-zoom for touch/trackpad gestures
        self.handle_pinch_zoom(ui, &response, chart_width, chart_rect.min.x);

        // Handle drag to pan (blocked when interacting with drawings)
        self.handle_drag_pan(
            ui,
            &response,
            price_axis_rect,
            time_axis_rect,
            chart_rect.min.x,
            is_drawing_interaction,
        );

        // Apply kinetic scrolling
        self.apply_kinetic_scroll(ui);

        // Handle box zoom (only when zoom mode is active from toolbar)
        // Right-click is reserved for context menu, zoom uses left-click when mode is active
        self.zoom_just_applied = self.handle_box_zoom(
            ui,
            &response,
            chart_rect,
            chart_width,
            self.zoom_mode_active,
        );
        if self.zoom_just_applied {
            log::info!("Zoom applied - chart will auto-deactivate zoom mode");
        }

        // Set zoom-in cursor when zoom mode is active
        if self.zoom_mode_active && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        // Set cursor icon based on crosshair style (cursor type)
        // This is only for default case - drawing manager may override for eraser mode
        else if response.hovered() {
            use crate::config::CrosshairStyle;
            match self.chart_options.crosshair.style {
                CrosshairStyle::Full => {
                    // Cross cursor mode - show crosshair cursor
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                CrosshairStyle::Dot => {
                    // Dot mode - default pointer (dot is rendered on chart)
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                CrosshairStyle::Arrow => {
                    // Arrow mode - default pointer
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            }
        }

        // Draw background (solid or gradient)
        self.draw_background(&painter, rect);

        // Draw watermark overlay (if enabled)
        self.draw_watermark(&painter, chart_rect);

        if self.state.data().is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(typography::LG),
                self.config.text_color,
            );
            return response;
        }

        // Split chart area into price and volume sections
        let (price_rect, volume_rect) = if self.config.show_volume {
            let split_y =
                chart_rect.min.y + chart_rect.height() * (1.0 - self.config.volume_height_fraction);
            (
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.max.x, split_y)),
                Rect::from_min_max(Pos2::new(chart_rect.min.x, split_y), chart_rect.max),
            )
        } else {
            (chart_rect, Rect::ZERO)
        };

        // Get visible range
        let (start_idx, _end_idx) = self.state.visible_range();
        self.start_idx = start_idx;

        // Capture near_live status for button
        let near_live_edge = self.state.time_scale().right_offset() >= -1.5;

        // Handle "Jump to Latest" button interaction
        if !near_live_edge {
            let btn_size = Vec2::new(
                DESIGN_TOKENS.sizing.charts_ext.realtime_button_width,
                DESIGN_TOKENS.sizing.button_md,
            );
            let btn_pos = Pos2::new(
                price_rect.center().x - btn_size.x / 2.0,
                price_rect.min.y + DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            );
            let btn_rect = Rect::from_min_size(btn_pos, btn_size);
            let btn_id = ui.id().with("jump_to_latest");
            let btn_res = ui.interact(btn_rect, btn_id, egui::Sense::click());

            if btn_res.clicked() {
                self.state.time_scale_mut().scroll_to_realtime();
            }
        }

        // Determine price bounds
        let (mut adjusted_min, mut adjusted_max) = self.state.price_range();

        // Apply price zoom
        let (new_min, new_max) = self.apply_price_zoom(
            pending_price_zoom,
            &response,
            chart_rect,
            adjusted_min,
            adjusted_max,
        );
        adjusted_min = new_min;
        adjusted_max = new_max;

        // Store the final rendered price range and rects for external use (selection dots, hit testing)
        self.last_rendered_price_range = (adjusted_min, adjusted_max);
        self.last_rendered_price_rect = price_rect;
        self.last_rendered_volume_rect = volume_rect;

        // Get visible data
        let visible_data = self.state.visible_data();

        if visible_data.is_empty() {
            return response;
        }

        // Calculate volume range
        let max_volume = if self.config.show_volume {
            visible_data
                .iter()
                .map(|c| c.volume)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1.0)
        } else {
            1.0
        };

        // Draw grid
        if self.config.show_horizontal_grid {
            rendering::render_grid(
                &painter,
                price_rect,
                adjusted_min,
                adjusted_max,
                self.config.grid_color,
            );
        }

        // Create rendering contexts
        let bar_spacing = self.state.time_scale().bar_spacing();
        let bar_width = bar_spacing * self.config.candle_width;
        let price_ctx = RenderContext::new(&painter, price_rect);
        let price_scale = PriceScale::new(adjusted_min, adjusted_max);
        let coords = ChartMapping::new(
            price_rect,
            bar_spacing,
            start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset(),
            adjusted_min,
            adjusted_max,
        );
        let colors = StyleColors {
            bullish: self.config.bullish_color,
            bearish: self.config.bearish_color,
            grid: self.config.grid_color,
            text: self.config.text_color,
            bullish_border: self.config.bullish_border_color,
            bearish_border: self.config.bearish_border_color,
            bullish_wick: self.config.bullish_wick_color,
            bearish_wick: self.config.bearish_wick_color,
            candle_border_width: self.config.candle_border_width,
        };

        let formatter = if self.config.show_time_labels || self.config.show_vertical_grid {
            Some(
                TimeFormatterBuilder::new()
                    .with_24_hour(true)
                    .with_seconds(true)
                    .with_timezone(self.chart_options.time_scale.timezone.clone())
                    .build(),
            )
        } else {
            None
        };

        if self.config.show_vertical_grid {
            // Simple bar-index-based vertical grid - moves 1:1 with chart
            rendering::render_vertical_grid(&painter, chart_rect, &coords, self.config.grid_color);
        }

        // Render chart type with clipping to prevent bars from overlapping axes
        // CRITICAL: Use chart_rect.width() for consistency with drawing coordinate system
        let chart_rect_width = chart_rect.width();
        let idx_to_coord = |idx: usize, min_x: f32| -> f32 {
            self.state
                .time_scale()
                .idx_to_coord(idx, min_x, chart_rect_width)
        };

        // Create clipped painter and contexts to prevent bars from rendering on axes
        let clipped_painter = painter.with_clip_rect(chart_rect);
        let clipped_price_ctx = RenderContext::new(&clipped_painter, price_rect);
        let clipped_volume_ctx = RenderContext::new(&clipped_painter, volume_rect);

        let render_ctx = rendering::CandleDataContext {
            price_ctx: &clipped_price_ctx,
            volume_ctx: &clipped_volume_ctx,
            price_scale: &price_scale,
            colors: &colors,
            visible_data,
            start_idx,
        };

        let render_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width, self.config.wick_width),
            rendering::VolumeSettings::new(self.config.show_volume, max_volume),
            rendering::JapaneseChartSettings::new(self.renko_brick_size, self.kagi_reversal_amount),
            rendering::TradingColors::new(self.config.bullish_color, self.config.bearish_color),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );

        rendering::render_chart_type(self.chart_type, &render_ctx, &render_params, idx_to_coord);

        // Draw indicators
        if let Some(indicator_registry) = indicators {
            renderers::IndicatorRenderer::render(
                &price_ctx,
                indicator_registry.indicators(),
                visible_data,
                &price_scale,
                &coords,
            );
        }

        // Render bar marks (Widget API annotations)
        if !self.marks.is_empty() {
            renderers::render_markers(
                &clipped_price_ctx,
                &self.marks,
                visible_data,
                &price_scale,
                &coords,
            );
        }

        // Draw price labels
        if self.config.show_right_axis {
            rendering::render_price_labels(
                &price_ctx,
                &price_scale,
                &colors,
                crate::scales::PriceScaleMode::Normal,
            );
        }

        // Last price line & label
        if self.config.show_symbol_last_val
            && let Some(last) = visible_data.last()
        {
            rendering::render_last_price_line(
                &painter,
                price_rect,
                last.close,
                last.open,
                adjusted_min,
                adjusted_max,
                self.config.bullish_color,
                self.config.bearish_color,
                self.config.show_right_axis,
            );
        }

        // Draw time labels
        if self.config.show_time_labels {
            let chart_ctx = RenderContext::new(&painter, chart_rect);
            rendering::render_time_labels(
                &chart_ctx,
                visible_data,
                &coords,
                &colors,
                formatter.as_deref(),
            );
        }

        // Draw OHLC info header (legend if symbol is set)
        if self.config.show_ohlc_info {
            if !self.symbol.is_empty() {
                // Calculate prev_close from second-to-last bar for change calculation
                let prev_close = if visible_data.len() >= 2 {
                    Some(visible_data[visible_data.len() - 2].close)
                } else {
                    None
                };
                rendering::render_legend(
                    &painter,
                    rect,
                    &self.symbol,
                    &self.timeframe,
                    visible_data,
                    prev_close,
                    &colors,
                    sizing::chart::PADDING,
                );
            } else {
                // Fallback to basic OHLC info
                rendering::render_ohlc_info(
                    &painter,
                    rect,
                    visible_data,
                    sizing::chart::PADDING,
                    self.config.text_color,
                );
            }
        }

        // Handle drawing tools
        if let Some(dm) = drawing_manager {
            // Clone/extract values before mutable borrow of self
            let timescale = self.state.time_scale().clone();
            let last_close = visible_data.last().map(|b| b.close);

            // Temporarily take cursor_modes to avoid borrow conflict
            let mut cursor_modes = std::mem::take(&mut self.cursor_modes);

            self.handle_drawings(
                ui,
                dm,
                &mut cursor_modes,
                &response,
                price_rect,
                adjusted_min,
                adjusted_max,
                &painter,
                last_close,
                &timescale,
            );

            // Render eraser highlight if in eraser mode
            self.render_eraser_highlight(&painter, dm, &cursor_modes);

            // Put cursor_modes back
            self.cursor_modes = cursor_modes;
        }

        // Render "Jump to Latest" button
        if self.config.show_realtime_btn {
            let btn_id = ui.id().with("jump_to_latest");
            let btn_size = Vec2::new(
                DESIGN_TOKENS.sizing.charts_ext.realtime_button_width,
                DESIGN_TOKENS.sizing.button_md,
            );
            let btn_pos = Pos2::new(
                price_rect.center().x - btn_size.x / 2.0,
                price_rect.min.y + DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            );
            let btn_rect = Rect::from_min_size(btn_pos, btn_size);
            let btn_res = ui.interact(btn_rect, btn_id, egui::Sense::click());

            rendering::render_realtime_btn(
                &painter,
                price_rect,
                near_live_edge,
                self.config.show_realtime_btn,
                self.config.realtime_button_size,
                self.config.realtime_button_pos,
                self.config.realtime_button_color,
                self.config.realtime_button_hover_color,
                self.config.realtime_button_text_color,
                self.config.realtime_button_text.as_deref(),
                btn_res.hovered(),
            );
        }

        // Draw crosshair with options from chart_options
        if let Some(hover_pos) = response.hover_pos()
            && price_rect.contains(hover_pos)
        {
            // Cache the hover bar index for multi-chart sync
            self.last_hover_bar_idx = Some(coords.x_to_idx_f32(hover_pos.x) as f64);

            rendering::render_crosshair_with_options(
                &price_ctx,
                hover_pos,
                visible_data,
                &price_scale,
                &coords,
                &self.chart_options.crosshair,
            );
        } else {
            // Clear hover bar index when not hovering locally
            self.last_hover_bar_idx = None;

            // Render synced crosshair from other charts (if available)
            {
                if let Some(bar_idx) = self.synced_crosshair_bar_idx {
                    // Convert bar index to screen x coordinate
                    let x = coords.idx_to_x(bar_idx as usize);
                    if coords.is_x_visible(x) {
                        // Create a synthetic hover position at the center of the price range
                        let center_y = price_rect.center().y;
                        let synced_pos = Pos2::new(x, center_y);

                        rendering::render_crosshair_with_options(
                            &price_ctx,
                            synced_pos,
                            visible_data,
                            &price_scale,
                            &coords,
                            &self.chart_options.crosshair,
                        );
                    }
                }
            }
        }

        // Draw box zoom rect
        rendering::render_box_zoom(&painter, &self.box_zoom);

        // Focus ring for keyboard accessibility
        crate::styles::focus::draw_focus_ring(ui, &response);

        response
    }

    // =========================================================================
    // Multi-Chart Sync Methods
    // =========================================================================

    /// Sets an external crosshair position from a synced chart (in bar-index coordinates).
    ///
    /// When set to `Some(idx)`, a crosshair is drawn at the given bar index even
    /// if the user is not hovering over this chart. Pass `None` to clear it.
    /// This is the receiver side of multi-chart crosshair synchronization.
    pub fn set_synced_crosshair_bar_idx(&mut self, bar_idx: Option<f64>) {
        self.synced_crosshair_bar_idx = bar_idx;
    }

    /// Returns the bar index that the user was hovering over in the last frame.
    ///
    /// Returns `None` if the cursor was not over the chart. This is the emitter
    /// side of multi-chart crosshair synchronization: read this value and pass
    /// it to [`Chart::set_synced_crosshair_bar_idx`] on other charts.
    pub fn get_hover_bar_idx(&self) -> Option<f64> {
        self.last_hover_bar_idx
    }

    /// Applies time-scale state from another chart for synchronized scrolling/zooming.
    ///
    /// Sets both bar spacing and right offset to match the source chart so that
    /// both charts display the same time range. Use together with
    /// [`Chart::get_time_scale_state`] on the source chart.
    pub fn apply_synced_time_scale(&mut self, bar_spacing: f32, right_offset: f32) {
        self.state.time_scale_mut().set_bar_spacing(bar_spacing);
        self.state.time_scale_mut().set_right_offset(right_offset);
    }

    /// Returns the current time-scale state as `(bar_spacing, right_offset)`.
    ///
    /// This is the emitter side of multi-chart time-scale synchronization.
    /// Pass the returned values to [`Chart::apply_synced_time_scale`] on other
    /// charts to keep them scrolled/zoomed in unison.
    pub fn get_time_scale_state(&self) -> (f32, f32) {
        (
            self.state.time_scale().bar_spacing(),
            self.state.time_scale().right_offset(),
        )
    }

    /// Get a [`ChartMapping`] for coordinate conversions.
    ///
    /// Returns a mapping constructed from the last rendered frame's parameters
    /// (price rect, bar spacing, right offset, price range). This is used for
    /// converting between screen coordinates and data coordinates, particularly
    /// for drawing tool restoration and hit testing.
    pub fn get_chart_mapping(&self) -> ChartMapping {
        ChartMapping::new(
            self.last_rendered_price_rect,
            self.state.time_scale().bar_spacing(),
            self.start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset(),
            self.last_rendered_price_range.0,
            self.last_rendered_price_range.1,
        )
    }
}
