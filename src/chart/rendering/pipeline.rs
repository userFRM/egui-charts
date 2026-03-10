//! Unified rendering pipeline for chart components
//!
//! This module provides a trait-based rendering architecture that WRAPS
//! the existing sophisticated rendering implementations. All trait adapters
//! DELEGATE to the existing functions - no functionality is reimplemented.
//!
//! ## Architecture
//! - `ChartRenderContext` - Unified context with conversion methods to old types
//! - `ChartRenderer` trait - Common interface for all renderers
//! - `RenderLayer` - Ordering enum for render pipeline
//! - Adapter structs - Thin wrappers that delegate to existing implementations

use crate::config::{CrosshairMode, RealtimeButtonPos};
use crate::model::Bar;
use crate::scales::{PriceScaleMode, TimeFormatter};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect};

// Import existing types from renderers module
use super::super::renderers::{
    BarRenderParams, ChartMapping, PriceScale, RenderContext, StyleColors,
};
use super::super::state::BoxZoomState;

// ============================================================================
// Unified Render Context
// ============================================================================

/// Unified rendering context that encapsulates all state needed for chart rendering.
///
/// This provides a cleaner API while maintaining full compatibility with
/// existing rendering functions through conversion methods.
pub struct ChartRenderContext<'a> {
    /// The egui painter for drawing
    pub painter: &'a Painter,
    /// The rect to render within
    pub rect: Rect,
    /// Min price in view
    pub min_price: f64,
    /// Max price in view
    pub max_price: f64,
    /// Start index of visible data
    pub start_idx: usize,
    /// Last global index
    pub last_idx_global: usize,
    /// Bar spacing (pixels between bars)
    pub bar_spacing: f32,
    /// Right offset for coord calculations
    pub right_offset: f32,
    /// Color scheme
    pub colors: StyleColors,
    /// Bar width for rendering
    pub bar_width: f32,
    /// Wick width for candle rendering
    pub wick_width: f32,
}

impl<'a> ChartRenderContext<'a> {
    /// Convert to RenderContext
    pub fn to_render_ctx(&self) -> RenderContext<'a> {
        RenderContext::new(self.painter, self.rect)
    }

    /// Convert to legacy PriceScale
    pub fn to_price_scale(&self) -> PriceScale {
        PriceScale::new(self.min_price, self.max_price)
    }

    /// Convert to ChartMapping
    pub fn to_mapping(&self) -> ChartMapping {
        ChartMapping::new(
            self.rect,
            self.bar_spacing,
            self.start_idx,
            self.last_idx_global,
            self.right_offset,
            self.min_price,
            self.max_price,
        )
    }

    /// Get a reference to colors
    pub fn style_colors(&self) -> &StyleColors {
        &self.colors
    }

    /// Create BarRenderParams for a specific bar
    pub fn bar_params(&self, x: f32) -> BarRenderParams {
        BarRenderParams::new(x, self.bar_width, self.wick_width)
    }

    /// Calculate X coord for a global index
    pub fn idx_to_x(&self, global_idx: usize) -> f32 {
        self.to_mapping().idx_to_x(global_idx)
    }

    /// Calculate Y coord for a price
    pub fn price_to_y(&self, price: f64) -> f32 {
        self.to_price_scale().price_to_y(price, self.rect)
    }
}

// ============================================================================
// Trait Definition
// ============================================================================

/// Trait for chart renderers
///
/// Implement this trait to create custom renderers that can be plugged into
/// the rendering pipeline. Built-in renderers delegate to existing implementations.
pub trait ChartRenderer {
    /// Render the component
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]);

    /// Get the rendering layer (for ordering)
    fn layer(&self) -> RenderLayer {
        RenderLayer::Main
    }
}

/// Rendering layer for ordering renders
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderLayer {
    /// Background elements (grid, etc.)
    Background = 0,
    /// Main chart content (candles, bars, lines)
    Main = 1,
    /// Overlays (indicators, annotations)
    Overlay = 2,
    /// Foreground elements (crosshair, tooltips)
    Foreground = 3,
}

/// Rendering pipeline that orchestrates all renderers
pub struct RenderPipeline {
    renderers: Vec<Box<dyn ChartRenderer>>,
}

impl RenderPipeline {
    /// Create a new empty pipeline
    pub fn new() -> Self {
        Self {
            renderers: Vec::new(),
        }
    }

    /// Add a renderer to the pipeline
    pub fn add<R: ChartRenderer + 'static>(&mut self, renderer: R) {
        self.renderers.push(Box::new(renderer));
    }

    /// Execute the rendering pipeline
    pub fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        // Sort by layer and render
        let mut sorted: Vec<_> = self.renderers.iter().collect();
        sorted.sort_by_key(|r| r.layer());

        for renderer in sorted {
            renderer.render(ctx, data);
        }
    }
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Grid Renderers (DELEGATE to existing implementations)
// ============================================================================

/// Horizontal grid renderer - DELEGATES to render_grid()
pub struct GridRenderer {
    pub color: Color32,
}

impl GridRenderer {
    pub fn new(color: Color32) -> Self {
        Self { color }
    }
}

impl ChartRenderer for GridRenderer {
    fn render(&self, ctx: &ChartRenderContext, _data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        super::render_grid(
            ctx.painter,
            ctx.rect,
            ctx.min_price,
            ctx.max_price,
            self.color,
        );
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }
}

/// Vertical grid renderer - DELEGATES to render_vertical_grid()
pub struct VerticalGridRenderer {
    pub color: Color32,
}

impl VerticalGridRenderer {
    pub fn new(color: Color32) -> Self {
        Self { color }
    }
}

impl ChartRenderer for VerticalGridRenderer {
    fn render(&self, ctx: &ChartRenderContext, _data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        let coords = ctx.to_mapping();
        super::render_vertical_grid(ctx.painter, ctx.rect, &coords, self.color);
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }
}

// ============================================================================
// Price/Time Labels Renderers (DELEGATE to existing implementations)
// ============================================================================

/// Price labels renderer - DELEGATES to render_price_labels()
pub struct PriceLabelsRenderer {
    pub mode: PriceScaleMode,
}

impl PriceLabelsRenderer {
    pub fn new() -> Self {
        Self {
            mode: PriceScaleMode::Normal,
        }
    }

    pub fn with_mode(mode: PriceScaleMode) -> Self {
        Self { mode }
    }
}

impl Default for PriceLabelsRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartRenderer for PriceLabelsRenderer {
    fn render(&self, ctx: &ChartRenderContext, _data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        let render_ctx = ctx.to_render_ctx();
        let price_scale = ctx.to_price_scale();
        super::render_price_labels(&render_ctx, &price_scale, &ctx.colors, self.mode);
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

/// Time labels renderer - DELEGATES to render_time_labels()
pub struct TimeLabelsRenderer<'f> {
    pub formatter: Option<&'f dyn TimeFormatter>,
}

impl<'f> TimeLabelsRenderer<'f> {
    pub fn new() -> Self {
        Self { formatter: None }
    }

    pub fn with_formatter(formatter: &'f dyn TimeFormatter) -> Self {
        Self {
            formatter: Some(formatter),
        }
    }
}

impl Default for TimeLabelsRenderer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartRenderer for TimeLabelsRenderer<'_> {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        let render_ctx = ctx.to_render_ctx();
        let coords = ctx.to_mapping();
        super::render_time_labels(&render_ctx, data, &coords, &ctx.colors, self.formatter);
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

// ============================================================================
// Last Price Line Renderer (DELEGATE to existing implementation)
// ============================================================================

/// Last price line renderer - DELEGATES to render_last_price_line()
pub struct LastPriceLineRenderer {
    pub show_right_axis: bool,
}

impl LastPriceLineRenderer {
    pub fn new(show_right_axis: bool) -> Self {
        Self { show_right_axis }
    }
}

impl ChartRenderer for LastPriceLineRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        if let Some(last_bar) = data.last() {
            // DELEGATE to existing sophisticated implementation
            super::render_last_price_line(
                ctx.painter,
                ctx.rect,
                last_bar.close,
                last_bar.open,
                ctx.min_price,
                ctx.max_price,
                ctx.colors.bullish,
                ctx.colors.bearish,
                self.show_right_axis,
            );
        }
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Overlay
    }
}

// ============================================================================
// Crosshair Renderer (DELEGATE to existing implementation)
// ============================================================================

/// Crosshair renderer - DELEGATES to render_crosshair_with_mode()
pub struct CrosshairRenderer {
    pub hover_pos: Option<Pos2>,
    pub mode: CrosshairMode,
}

impl CrosshairRenderer {
    pub fn new(hover_pos: Option<Pos2>) -> Self {
        Self {
            hover_pos,
            mode: CrosshairMode::Normal,
        }
    }

    pub fn with_mode(mut self, mode: CrosshairMode) -> Self {
        self.mode = mode;
        self
    }
}

impl ChartRenderer for CrosshairRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        if let Some(pos) = self.hover_pos {
            // DELEGATE to existing sophisticated 280+ line implementation
            let render_ctx = ctx.to_render_ctx();
            let price_scale = ctx.to_price_scale();
            let coords = ctx.to_mapping();

            // Use the crosshair with mode support
            crate::chart::renderers::render_crosshair_with_mode(
                &render_ctx,
                pos,
                data,
                &price_scale,
                &coords,
                self.mode,
            );
        }
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

// ============================================================================
// Legend/OHLC Info Renderers (DELEGATE to existing implementations)
// ============================================================================

/// Legend renderer - DELEGATES to render_legend()
pub struct LegendRenderer {
    pub symbol: String,
    pub timeframe: String,
    pub prev_close: Option<f64>,
    pub padding: f32,
}

impl LegendRenderer {
    pub fn new(symbol: impl Into<String>, timeframe: impl Into<String>, padding: f32) -> Self {
        Self {
            symbol: symbol.into(),
            timeframe: timeframe.into(),
            prev_close: None,
            padding,
        }
    }

    pub fn with_prev_close(mut self, prev_close: Option<f64>) -> Self {
        self.prev_close = prev_close;
        self
    }
}

impl ChartRenderer for LegendRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        super::render_legend(
            ctx.painter,
            ctx.rect,
            &self.symbol,
            &self.timeframe,
            data,
            self.prev_close,
            &ctx.colors,
            self.padding,
        );
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

/// OHLC info renderer - DELEGATES to render_ohlc_info()
pub struct OhlcInfoRenderer {
    pub padding: f32,
}

impl OhlcInfoRenderer {
    pub fn new(padding: f32) -> Self {
        Self { padding }
    }
}

impl ChartRenderer for OhlcInfoRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        super::render_ohlc_info(ctx.painter, ctx.rect, data, self.padding, ctx.colors.text);
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

// ============================================================================
// Box Zoom Renderer (DELEGATE to existing implementation)
// ============================================================================

/// Box zoom renderer - DELEGATES to render_box_zoom()
pub struct BoxZoomRenderer<'a> {
    pub box_zoom: &'a BoxZoomState,
}

impl<'a> BoxZoomRenderer<'a> {
    pub fn new(box_zoom: &'a BoxZoomState) -> Self {
        Self { box_zoom }
    }
}

impl ChartRenderer for BoxZoomRenderer<'_> {
    fn render(&self, ctx: &ChartRenderContext, _data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        super::render_box_zoom(ctx.painter, self.box_zoom);
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

// ============================================================================
// Realtime Button Renderer (DELEGATE to existing implementation)
// ============================================================================

/// Realtime button renderer - DELEGATES to render_realtime_btn()
pub struct RealtimeBtnRenderer {
    pub near_live_edge: bool,
    pub show_btn: bool,
    pub btn_size: (f32, f32),
    pub position: RealtimeButtonPos,
    pub btn_color: Color32,
    pub hover_color: Color32,
    pub text_color: Color32,
    pub btn_text: Option<String>,
    pub is_hovered: bool,
}

impl RealtimeBtnRenderer {
    pub fn new() -> Self {
        Self {
            near_live_edge: true,
            show_btn: true,
            btn_size: (110.0, 28.0),
            position: RealtimeButtonPos::TopCenter,
            btn_color: DESIGN_TOKENS.semantic.extended.chart_crosshair_label_bg,
            hover_color: DESIGN_TOKENS.semantic.extended.chart_tooltip_bg,
            text_color: DESIGN_TOKENS.semantic.extended.chart_text,
            btn_text: None,
            is_hovered: false,
        }
    }
}

impl Default for RealtimeBtnRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartRenderer for RealtimeBtnRenderer {
    fn render(&self, ctx: &ChartRenderContext, _data: &[Bar]) {
        // DELEGATE to existing sophisticated implementation
        super::render_realtime_btn(
            ctx.painter,
            ctx.rect,
            self.near_live_edge,
            self.show_btn,
            self.btn_size,
            self.position,
            self.btn_color,
            self.hover_color,
            self.text_color,
            self.btn_text.as_deref(),
            self.is_hovered,
        );
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Foreground
    }
}

// ============================================================================
// Candlestick/Bar/Volume Renderers (DELEGATE to existing implementations)
// ============================================================================

/// Candlestick renderer - DELEGATES to renderers::render_candle() for each bar
pub struct CandleRenderer;

impl ChartRenderer for CandleRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        let render_ctx = ctx.to_render_ctx();
        let price_scale = ctx.to_price_scale();

        for (i, bar) in data.iter().enumerate() {
            let global_idx = ctx.start_idx + i;
            let x = ctx.idx_to_x(global_idx);
            let params = ctx.bar_params(x);

            // DELEGATE to existing sophisticated candle renderer
            crate::chart::renderers::render_candle(
                &render_ctx,
                bar,
                &price_scale,
                &ctx.colors,
                &params,
            );
        }
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Main
    }
}

/// OHLC Bar renderer - DELEGATES to renderers::render_bar() for each bar
pub struct OhlcBarRenderer;

impl ChartRenderer for OhlcBarRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        let render_ctx = ctx.to_render_ctx();
        let price_scale = ctx.to_price_scale();

        for (i, bar) in data.iter().enumerate() {
            let global_idx = ctx.start_idx + i;
            let x = ctx.idx_to_x(global_idx);
            let params = ctx.bar_params(x);

            // DELEGATE to existing OHLC bar renderer
            crate::chart::renderers::render_ohlc_bar(
                &render_ctx,
                bar,
                &price_scale,
                &ctx.colors,
                &params,
            );
        }
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Main
    }
}

/// Volume renderer - DELEGATES to renderers::render_volume_bar() for each bar
pub struct VolumeRenderer {
    pub max_volume: f64,
}

impl VolumeRenderer {
    pub fn new(max_volume: f64) -> Self {
        Self { max_volume }
    }
}

impl ChartRenderer for VolumeRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        if self.max_volume <= 0.0 {
            return;
        }

        let render_ctx = ctx.to_render_ctx();

        for (i, bar) in data.iter().enumerate() {
            let global_idx = ctx.start_idx + i;
            let x = ctx.idx_to_x(global_idx);
            let params = ctx.bar_params(x);

            // DELEGATE to existing volume renderer (26% opacity)
            crate::chart::renderers::render_volume_bar(
                &render_ctx,
                bar,
                self.max_volume,
                &ctx.colors,
                &params,
            );
        }
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Background
    }
}

/// Line chart renderer - DELEGATES to existing line rendering logic
pub struct LineRenderer {
    pub color: Color32,
    pub stroke_width: f32,
}

impl LineRenderer {
    pub fn new(color: Color32) -> Self {
        Self {
            color,
            stroke_width: 2.0,
        }
    }
}

impl ChartRenderer for LineRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        if data.len() < 2 {
            return;
        }

        let mut points = Vec::with_capacity(data.len());
        for (i, bar) in data.iter().enumerate() {
            let global_idx = ctx.start_idx + i;
            let x = ctx.idx_to_x(global_idx);
            let y = ctx.price_to_y(bar.close);
            points.push(Pos2::new(x, y));
        }

        ctx.painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(self.stroke_width, self.color),
        ));
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Main
    }
}

/// Area chart renderer - renders line with filled area below
pub struct AreaRenderer {
    pub line_color: Color32,
    pub fill_color: Color32,
    pub stroke_width: f32,
}

impl AreaRenderer {
    pub fn new(color: Color32) -> Self {
        Self {
            line_color: color,
            fill_color: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 50),
            stroke_width: 2.0,
        }
    }
}

impl ChartRenderer for AreaRenderer {
    fn render(&self, ctx: &ChartRenderContext, data: &[Bar]) {
        if data.len() < 2 {
            return;
        }

        let mut line_points = Vec::with_capacity(data.len());
        let mut fill_points = Vec::with_capacity(data.len() + 2);

        for (i, bar) in data.iter().enumerate() {
            let global_idx = ctx.start_idx + i;
            let x = ctx.idx_to_x(global_idx);
            let y = ctx.price_to_y(bar.close);
            line_points.push(Pos2::new(x, y));
            fill_points.push(Pos2::new(x, y));
        }

        // Close the fill polygon
        if fill_points.len() >= 2 {
            let first_x = fill_points[0].x;
            let last_x = fill_points[fill_points.len() - 1].x;
            fill_points.push(Pos2::new(last_x, ctx.rect.max.y));
            fill_points.push(Pos2::new(first_x, ctx.rect.max.y));
        }

        // Draw fill
        ctx.painter.add(egui::Shape::convex_polygon(
            fill_points,
            self.fill_color,
            egui::Stroke::NONE,
        ));

        // Draw line
        ctx.painter.add(egui::Shape::line(
            line_points,
            egui::Stroke::new(self.stroke_width, self.line_color),
        ));
    }

    fn layer(&self) -> RenderLayer {
        RenderLayer::Main
    }
}
