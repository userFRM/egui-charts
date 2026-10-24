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
    ArrowStyle, ChartPoint, DrawingToolType, FibonacciConfig, FontWeight, GannConfig, LineStyle,
    PitchforkConfig, PositionSize, RegressionTrendConfig, TimeframeVisibility, VolumeProfileConfig,
};
use serde::{Deserialize, Serialize};

/// A drawing on the chart
///
/// Drawings use a dual coordinate system:
/// - `points: Vec<Pos2>` - Screen coordinates, updated each frame
/// - `chart_points: Vec<ChartPoint>` - Chart coordinates (bar_idx, price), stable across pan/zoom
///
/// # Example
///
/// ```
/// use egui_charts::drawings::{Drawing, DrawingToolType};
///
/// let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
/// drawing.color = [33, 150, 243, 255]; // Blue
/// drawing.stroke_width = 2.0;
/// ```
///
/// Serde covers the tool and its settings only: the id, the points and the
/// completion state are left out, since the anchors are stored as
/// timestamps by [`persistence`](crate::drawings::persistence). A field added
/// later needs `#[serde(default)]`, so that saved version 2 files still load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drawing {
    /// Unique identifier for this drawing
    #[serde(skip)]
    pub id: usize,
    /// Id of the stored record this drawing was restored from; `None` for a drawing never
    /// stored, which is saved under its `id`
    #[serde(skip)]
    pub stored_id: Option<String>,
    /// Creation time, in Unix milliseconds, of the stored record this drawing was restored
    /// from; `None` for a drawing never stored, which is saved with the time of the save
    #[serde(skip)]
    pub created_at: Option<i64>,
    /// User notes, saved with the drawing and restored from its stored record
    #[serde(skip)]
    pub notes: Option<String>,
    /// Optional object-tree name, saved with the drawing.
    #[serde(default)]
    pub label: Option<String>,
    /// Owning study id, or `None` for the main price pane. Follows that study when panes move
    /// or merge; hosts restoring studies must retain or remap their ids. Older files use main.
    #[serde(default)]
    pub pane: Option<usize>,
    /// Type of drawing tool
    pub tool_type: DrawingToolType,
    /// Screen coordinates (updated each frame from chart_points)
    #[serde(skip)]
    pub points: Vec<egui::Pos2>,
    /// Persistent chart coordinates (bar_idx, price) - stable across pan/zoom
    #[serde(skip)]
    pub chart_points: Vec<ChartPoint>,
    /// Screen y of each visible level of a Fibonacci retracement or extension, in config order
    /// (empty for other tools); rebuilt with `points` by [`Self::update_screen_coords`]
    #[serde(skip)]
    pub level_ys: Vec<f32>,
    /// Position as fractions of the pane width and height for anchored text and notes.
    /// `None` adopts the first displayed position, including for older saved drawings.
    #[serde(default)]
    pub screen_anchor: Option<[f32; 2]>,
    /// Drawing color as RGBA
    pub color: [u8; 4],
    /// Stroke width in pixels
    pub stroke_width: f32,
    /// Whether the drawing is complete
    #[serde(skip)]
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

    // === Tool-specific configuration (None uses the defaults) ===
    /// Quantity of the long and short position tools
    pub position_size: Option<PositionSize>,
    /// Customizable Fibonacci levels for Fib tools
    pub fib_config: Option<FibonacciConfig>,
    /// Rows and value area of the volume profile tools
    pub volume_profile: Option<VolumeProfileConfig>,
    /// Band width of the regression trend tool
    pub regression_trend: Option<RegressionTrendConfig>,

    /// Price and time divisions of Gann boxes and squares.
    #[serde(default)]
    pub gann: Option<GannConfig>,
    /// Whether a parallel channel draws its dashed halfway line.
    #[serde(default)]
    pub channel_midline: bool,
    /// Level lines and band fill of the pitchfork tools.
    #[serde(default)]
    pub pitchfork: Option<PitchforkConfig>,

    // === Z-Order (rendering layer) ===
    /// Z-order for rendering priority (higher = drawn on top)
    pub z_order: i32,
}

impl Drawing {
    /// Create a new drawing with default properties
    pub fn new(id: usize, tool_type: DrawingToolType) -> Self {
        Self {
            id,
            stored_id: None,
            created_at: None,
            notes: None,
            label: None,
            tool_type,
            points: Vec::new(),
            chart_points: Vec::new(),
            level_ys: Vec::new(),
            screen_anchor: None,
            pane: None,
            color: [242, 54, 69, 255], // Default red (#F23645)
            stroke_width: 2.0,
            completed: false,
            text: (tool_type == DrawingToolType::Sticker).then(|| "sticker:bull".into()),
            // Styling defaults
            line_style: LineStyle::Solid,
            fill_color: None,
            locked: false,
            visible: true,
            font_size: if matches!(
                tool_type,
                DrawingToolType::FontIcon | DrawingToolType::Sticker
            ) {
                32.0
            } else {
                12.0
            },
            font_weight: FontWeight::Normal,
            extend_left: false,
            extend_right: false,
            arrow_start_style: ArrowStyle::None,
            arrow_end_style: if matches!(
                tool_type,
                DrawingToolType::Arrow | DrawingToolType::ArrowMarker | DrawingToolType::Path
            ) {
                ArrowStyle::Arrow
            } else {
                ArrowStyle::None
            },
            timeframe_visibility: TimeframeVisibility::all(),
            position_size: None,
            fib_config: None,
            volume_profile: None,
            regression_trend: None,
            pitchfork: None,
            channel_midline: false,
            gann: None,
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
        self.level_ys = self
            .fib_levels()
            .map(|(_, prices)| prices.into_iter().map(price_to_y).collect())
            .unwrap_or_default();
    }

    /// Rebuilds anchored text and note coordinates relative to the pane. Call after
    /// `update_screen_coords`; other tools retain their chart coordinates. Older drawings
    /// adopt their first displayed position. Resizing preserves the width and height fractions.
    pub fn update_pane_coords(&mut self, pane: egui::Rect) {
        if !matches!(
            self.tool_type,
            DrawingToolType::AnchoredText | DrawingToolType::AnchoredNote
        ) || !pane.is_finite()
            || pane.width() <= 0.0
            || pane.height() <= 0.0
        {
            return;
        }
        if let Some(pos) = self.points.first_mut() {
            let anchor = self.screen_anchor.get_or_insert_with(|| {
                [
                    (pos.x - pane.left()) / pane.width(),
                    (pos.y - pane.top()) / pane.height(),
                ]
            });
            *pos = pane.min + egui::vec2(anchor[0] * pane.width(), anchor[1] * pane.height());
        }
    }

    /// The Fibonacci config in use and the price of each of its visible levels, for a retracement
    /// or an extension projected from its third anchor (the first anchor for legacy two-point drawings).
    pub(crate) fn fib_levels(&self) -> Option<(FibonacciConfig, Vec<f64>)> {
        let extension = match self.tool_type {
            DrawingToolType::FibonacciRetracement => false,
            DrawingToolType::FibonacciExtension => true,
            _ => return None,
        };
        let [start, end, ..] = &self.chart_points[..] else {
            return None;
        };
        let config = self.fib_config.clone().unwrap_or_else(|| {
            if extension {
                FibonacciConfig::extension_default()
            } else {
                FibonacciConfig::default()
            }
        });
        let offset = if extension {
            self.chart_points
                .get(2)
                .map_or(0.0, |origin| origin.price - start.price)
        } else {
            0.0
        };
        let prices = config
            .level_prices(start.price, end.price, extension)
            .map(|price| price + offset)
            .collect();
        Some((config, prices))
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

    /// Shift the bar indices at or after `from` by a delta (used when bars are inserted in front
    /// of them)
    pub fn shift_bar_indices(&mut self, from: f32, delta: f32) {
        for cp in &mut self.chart_points {
            if cp.bar_idx >= from {
                cp.shift_bar_idx(delta);
            }
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
    fn test_shift_bar_indices() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        drawing.chart_points.push(ChartPoint::new(100.0, 150.0));
        drawing.chart_points.push(ChartPoint::new(110.0, 160.0));

        drawing.shift_bar_indices(105.0, 50.0);

        assert_eq!(
            drawing.chart_points,
            [ChartPoint::new(100.0, 150.0), ChartPoint::new(160.0, 160.0)]
        );
    }

    /// A drawing completes once it holds its tool's required number of
    /// anchors, and each anchor added from the screen gets its chart
    /// coordinates.
    #[test]
    fn test_completion() {
        let x_to_bar = |x: f32| x / 10.0;
        let y_to_price = |y: f32| 1000.0 - y as f64;
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);

        drawing.add_point_with_chart_coords(egui::pos2(100.0, 300.0), x_to_bar, y_to_price);
        assert!(!drawing.completed);

        drawing.add_point_with_chart_coords(egui::pos2(200.0, 50.0), x_to_bar, y_to_price);
        assert!(drawing.completed); // TrendLine requires 2 points
        assert_eq!(
            drawing.chart_points,
            [ChartPoint::new(10.0, 700.0), ChartPoint::new(20.0, 950.0)]
        );
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
