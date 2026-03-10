//! Drawing struct - represents a single drawing on the chart
//!
//! This is the core data structure for chart drawings. It contains:
//! - Dual coordinate system (screen coords + chart coords)
//! - Styling properties (color, stroke, line style)
//! - Tool-specific configuration (Fibonacci levels, etc.)
//!
//! Note: Rendering methods are in the rendering module, not here.
//! This keeps the domain layer pure and testable.

use super::{
    ArrowStyle, ChartPoint, DrawingToolType, FibonacciConfig, FontWeight, LineStyle,
    TimeframeVisibility,
};

/// A drawing on the chart
///
/// Drawings use a dual coordinate system:
/// - `points: Vec<Pos2>` - Screen coordinates, updated each frame
/// - `chart_points: Vec<ChartPoint>` - Chart coordinates (bar_idx, price), stable across pan/zoom
///
/// # Example
///
/// ```ignore
/// use egui_charts::drawings::{Drawing, DrawingToolType};
///
/// let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
/// drawing.color = [33, 150, 243, 255]; // Blue
/// drawing.stroke_width = 2.0;
/// ```
#[derive(Debug, Clone)]
pub struct Drawing {
    /// Unique identifier for this drawing
    pub id: usize,
    /// Type of drawing tool
    pub tool_type: DrawingToolType,
    /// Screen coordinates (updated each frame from chart_points)
    pub points: Vec<egui::Pos2>,
    /// Persistent chart coordinates (bar_idx, price) - stable across pan/zoom
    pub chart_points: Vec<ChartPoint>,
    /// Drawing color as RGBA
    pub color: [u8; 4],
    /// Stroke width in pixels
    pub stroke_width: f32,
    /// Whether the drawing is complete
    pub completed: bool,
    /// Optional text content (for text labels, notes, etc.)
    pub text: Option<String>,

    // === Styling properties ===
    /// Line style (Solid, Dashed, Dotted)
    pub line_style: LineStyle,
    /// Fill color for shapes - None means no fill
    pub fill_color: Option<[u8; 4]>,
    /// Whether the drawing is locked (prevents editing/moving)
    pub locked: bool,
    /// Whether the drawing is visible
    pub visible: bool,
    /// Font size for text annotations
    pub font_size: f32,
    /// Font weight for text annotations
    pub font_weight: FontWeight,
    /// Extend line to left edge
    pub extend_left: bool,
    /// Extend line to right edge
    pub extend_right: bool,
    /// Arrow style at start of line
    pub arrow_start_style: ArrowStyle,
    /// Arrow style at end of line
    pub arrow_end_style: ArrowStyle,
    /// Per-timeframe visibility
    pub timeframe_visibility: TimeframeVisibility,

    // === Position tracking fields ===
    /// Position quantity (e.g., 100 shares)
    pub quantity: Option<f32>,
    /// Position status (open/closed)
    pub is_closed: bool,
    /// Real-time price for P&L calculation
    pub curr_price: Option<f64>,

    // === Forecast tracking fields ===
    /// Price at forecast start
    pub forecast_start_price: Option<f64>,
    /// Predicted target price
    pub forecast_target_price: Option<f64>,
    /// Timestamp when forecast created
    pub forecast_start_time: Option<i64>,
    /// Predicted target time
    pub forecast_target_time: Option<i64>,

    // === Fibonacci configuration ===
    /// Customizable Fibonacci levels for Fib tools
    pub fib_config: Option<FibonacciConfig>,

    // === Z-Order (rendering layer) ===
    /// Z-order for rendering priority (higher = drawn on top)
    pub z_order: i32,
}

impl Drawing {
    /// Create a new drawing with default properties
    pub fn new(id: usize, tool_type: DrawingToolType) -> Self {
        Self {
            id,
            tool_type,
            points: Vec::new(),
            chart_points: Vec::new(),
            color: [242, 54, 69, 255], // Default red (#F23645)
            stroke_width: 2.0,
            completed: false,
            text: None,
            // Styling defaults
            line_style: LineStyle::Solid,
            fill_color: None,
            locked: false,
            visible: true,
            font_size: 12.0,
            font_weight: FontWeight::Normal,
            extend_left: false,
            extend_right: false,
            arrow_start_style: ArrowStyle::None,
            arrow_end_style: ArrowStyle::Arrow,
            timeframe_visibility: TimeframeVisibility::all(),
            // Position tracking defaults
            quantity: Some(100.0),
            is_closed: false,
            curr_price: None,
            // Forecast tracking defaults
            forecast_start_price: None,
            forecast_target_price: None,
            forecast_start_time: None,
            forecast_target_time: None,
            // Fibonacci config
            fib_config: None,
            // Z-order (default: 0, higher values render on top)
            z_order: 0,
        }
    }

    /// Create a new drawing with a specific color
    pub fn with_color(id: usize, tool_type: DrawingToolType, color: [u8; 4]) -> Self {
        let mut drawing = Self::new(id, tool_type);
        drawing.color = color;
        drawing
    }

    /// Create a new text label drawing
    pub fn with_text(id: usize, text: String) -> Self {
        let mut drawing = Self::new(id, DrawingToolType::TextLabel);
        drawing.text = Some(text);
        drawing.color = [255, 255, 255, 255]; // White
        drawing.arrow_end_style = ArrowStyle::None;
        drawing
    }

    /// Update screen coordinates from chart_points using current transforms
    ///
    /// This should be called each frame before rendering.
    pub fn update_screen_coords<F, G>(&mut self, bar_to_x: F, price_to_y: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f64) -> f32,
    {
        self.points.clear();
        for cp in &self.chart_points {
            let x = bar_to_x(cp.bar_idx);
            let y = price_to_y(cp.price);
            self.points.push(egui::Pos2::new(x, y));
        }
    }

    /// Add a point with both screen and chart coordinates
    pub fn add_point_with_chart_coords<F, G>(
        &mut self,
        screen_point: egui::Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        let bar_idx = x_to_bar(screen_point.x);
        let price = y_to_price(screen_point.y);
        self.chart_points.push(ChartPoint::new(bar_idx, price));
        self.points.push(screen_point);
        self.check_completion();
    }

    /// Update the last point with both screen and chart coordinates
    pub fn update_last_point_with_chart_coords<F, G>(
        &mut self,
        screen_point: egui::Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if let (Some(last_point), Some(last_chart_point)) =
            (self.points.last_mut(), self.chart_points.last_mut())
        {
            *last_point = screen_point;
            *last_chart_point =
                ChartPoint::new(x_to_bar(screen_point.x), y_to_price(screen_point.y));
        }
    }

    /// Add a screen point (legacy method)
    pub fn add_point(&mut self, point: egui::Pos2) {
        self.points.push(point);
        self.check_completion();
    }

    /// Shift all bar indices by a delta (used when historical data is prepended)
    pub fn shift_bar_indices(&mut self, delta: f32) {
        for cp in &mut self.chart_points {
            cp.shift_bar_idx(delta);
        }
    }

    /// Check and set completion status based on tool type
    fn check_completion(&mut self) {
        if let Some(required) = self.tool_type.required_points()
            && self.points.len() >= required
        {
            self.completed = true;
        }
    }

    /// Get the egui Color32 from the RGBA array
    #[inline]
    pub fn color32(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        )
    }

    /// Get the fill color as Color32, if set
    #[inline]
    pub fn fill_color32(&self) -> Option<egui::Color32> {
        self.fill_color
            .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
    }

    /// Get the stroke for rendering
    #[inline]
    pub fn stroke(&self) -> egui::Stroke {
        egui::Stroke::new(self.stroke_width, self.color32())
    }

    /// Check if this drawing is visible on the given timeframe
    #[inline]
    pub fn is_visible_on_timeframe(&self, timeframe: &str) -> bool {
        self.visible && self.timeframe_visibility.is_visible_on(timeframe)
    }

    /// Get the z-order of this drawing
    #[inline]
    pub fn z_order(&self) -> i32 {
        self.z_order
    }

    /// Set the z-order of this drawing
    #[inline]
    pub fn set_z_order(&mut self, order: i32) {
        self.z_order = order;
    }

    /// Get the bounding rect of the drawing's points
    pub fn bounding_rect(&self) -> Option<egui::Rect> {
        if self.points.is_empty() {
            return None;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }

        Some(egui::Rect::from_min_max(
            egui::pos2(min_x, min_y),
            egui::pos2(max_x, max_y),
        ))
    }

    /// Quadratic bezier curve interpolation
    ///
    /// Returns a point on the bezier curve defined by p0, p1, p2 at parameter t.
    /// Used for smooth curve rendering (e.g., arcs, spirals).
    pub fn quadratic_bezier(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, t: f32) -> egui::Pos2 {
        let u = 1.0 - t;
        egui::Pos2::new(
            u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
            u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
        )
    }

    /// Format Unix timestamp to readable date string
    ///
    /// Returns a formatted date string like "2024-01-15 14:30".
    /// Used for displaying dates on drawings.
    pub fn format_timestamp(ts: i64) -> String {
        use chrono::{DateTime, Utc};
        match DateTime::<Utc>::from_timestamp(ts, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
            None => "N/A".to_string(),
        }
    }
}

impl Default for Drawing {
    fn default() -> Self {
        Self::new(0, DrawingToolType::TrendLine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_drawing() {
        let drawing = Drawing::new(1, DrawingToolType::TrendLine);
        assert_eq!(drawing.id, 1);
        assert_eq!(drawing.tool_type, DrawingToolType::TrendLine);
        assert!(drawing.points.is_empty());
        assert!(!drawing.completed);
    }

    #[test]
    fn test_with_color() {
        let drawing = Drawing::with_color(1, DrawingToolType::TrendLine, [0, 255, 0, 255]);
        assert_eq!(drawing.color, [0, 255, 0, 255]);
    }

    #[test]
    fn test_shift_bar_indices() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        drawing.chart_points.push(ChartPoint::new(100.0, 150.0));
        drawing.chart_points.push(ChartPoint::new(110.0, 160.0));

        drawing.shift_bar_indices(50.0);

        assert_eq!(drawing.chart_points[0].bar_idx, 150.0);
        assert_eq!(drawing.chart_points[1].bar_idx, 160.0);
    }

    #[test]
    fn test_completion() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        assert!(!drawing.completed);

        drawing.add_point(egui::pos2(0.0, 0.0));
        assert!(!drawing.completed);

        drawing.add_point(egui::pos2(100.0, 100.0));
        assert!(drawing.completed); // TrendLine requires 2 points
    }

    #[test]
    fn test_bounding_rect() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        assert!(drawing.bounding_rect().is_none());

        drawing.points.push(egui::pos2(10.0, 20.0));
        drawing.points.push(egui::pos2(30.0, 50.0));

        let rect = drawing.bounding_rect().unwrap();
        assert_eq!(rect.min, egui::pos2(10.0, 20.0));
        assert_eq!(rect.max, egui::pos2(30.0, 50.0));
    }
}
