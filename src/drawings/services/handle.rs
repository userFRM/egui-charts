//! Handle manipulation service for drawing tools.
//!
//! Provides all handle-related operations for selected drawings:
//!
//! - **Discovery** -- [`HandleService::get_handles`] returns the handle
//!   positions for a drawing based on its tool type.
//! - **Hit testing** -- [`HandleService::hit_test_handle`] determines which
//!   handle (if any) is under the cursor.
//! - **Dragging** -- [`HandleService::update_handle`] moves a handle and
//!   updates both screen and chart coordinates.
//! - **Rendering** -- [`HandleService::render_handles`] draws round handles
//!   with selection highlighting.

use crate::drawings::domain::{Drawing, DrawingToolType, HandlePos};
use crate::styles::typography::font_size;
use crate::theme::ThemeExt;
use egui::{Color32, Pos2};

/// Configuration for selection handle appearance and hit testing.
#[derive(Clone, Debug)]
pub struct HandleConfig {
    /// Handle diameter in pixels.
    pub size: f32,
    /// Ring color of a handle as RGBA bytes (`Color32::PLACEHOLDER.to_array()`: the theme's
    /// accent).
    pub color: [u8; 4],
    /// Fill color of the handle being dragged as RGBA bytes
    /// (`Color32::PLACEHOLDER.to_array()`: the theme's accent).
    pub selected_color: [u8; 4],
    /// Whether to render handles at all (can be disabled for screenshot mode, etc.).
    pub show_handles: bool,
}

impl Default for HandleConfig {
    fn default() -> Self {
        Self {
            size: 8.0,
            color: Color32::PLACEHOLDER.to_array(),
            selected_color: Color32::PLACEHOLDER.to_array(),
            show_handles: true,
        }
    }
}

/// Stateless service for selection handle operations on drawings.
///
/// All methods are static (`&self`-free) since `HandleService` carries no state.
/// It computes handle positions, performs hit testing, updates handle positions
/// during drags, and renders the handles onto the chart.
pub struct HandleService;

impl HandleService {
    /// Returns the selection handles for a drawing based on its tool type.
    ///
    /// Handle layouts vary by tool type:
    /// - **Two-point tools** (TrendLine, Fibonacci, etc.): Start and End handles
    /// - **Rectangle and Gann box**: Four corner handles and four edge handles
    /// - **Single-point tools** (HorizontalLine, VerticalLine): One Start handle
    /// - **Glyphs**: Four resize corners around the centre and a Middle move handle
    /// - **Three-anchor channels**: Four indexed handles, one at each line end
    /// - **Multi-point tools**: Indexed handles at each anchor
    pub fn get_handles(drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        let mut handles = Vec::new();

        match drawing.tool_type {
            DrawingToolType::TrendLine | DrawingToolType::Measure => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
                if drawing.points.len() >= 2 {
                    handles.push((HandlePos::End, drawing.points[1]));
                }
            }
            DrawingToolType::Rect | DrawingToolType::GannBox => {
                if drawing.points.len() >= 2 {
                    let p1 = drawing.points[0];
                    let p2 = drawing.points[1];
                    handles.push((HandlePos::TopLeft, p1));
                    handles.push((HandlePos::TopRight, Pos2::new(p2.x, p1.y)));
                    handles.push((HandlePos::BottomLeft, Pos2::new(p1.x, p2.y)));
                    handles.push((HandlePos::BottomRight, p2));
                    let mid = p1 + (p2 - p1) / 2.0;
                    handles.push((HandlePos::Top, Pos2::new(mid.x, p1.y)));
                    handles.push((HandlePos::Bottom, Pos2::new(mid.x, p2.y)));
                    handles.push((HandlePos::Left, Pos2::new(p1.x, mid.y)));
                    handles.push((HandlePos::Right, Pos2::new(p2.x, mid.y)));
                }
            }
            DrawingToolType::FontIcon | DrawingToolType::Sticker => {
                if let Some(&center) = drawing.points.first() {
                    let rect = egui::Rect::from_center_size(
                        center,
                        egui::Vec2::splat(font_size(drawing.font_size)),
                    );
                    handles.push((HandlePos::TopLeft, rect.left_top()));
                    handles.push((HandlePos::TopRight, rect.right_top()));
                    handles.push((HandlePos::BottomLeft, rect.left_bottom()));
                    handles.push((HandlePos::BottomRight, rect.right_bottom()));
                    handles.push((HandlePos::Middle, center));
                }
            }
            DrawingToolType::HorizontalLine => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
            }
            DrawingToolType::VerticalLine => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
            }
            // Fibonacci and similar two-point tools
            DrawingToolType::FibonacciRetracement
            | DrawingToolType::GannFan
            | DrawingToolType::GannSquare => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
                if drawing.points.len() >= 2 {
                    handles.push((HandlePos::End, drawing.points[1]));
                }
            }
            // A position: the entry and the width on the entry line, the target and the stop
            // on the box's left edge, and the box's centre, which the body drag moves.
            DrawingToolType::LongPos | DrawingToolType::ShortPos => {
                if let &[entry, target, ..] = drawing.points.as_slice() {
                    let right = Drawing::position_right(entry, target);
                    handles.push((HandlePos::Start, entry));
                    handles.push((HandlePos::End, Pos2::new(right, entry.y)));
                    handles.push((HandlePos::Point(1), Pos2::new(entry.x, target.y)));
                    if let Some(stop) = drawing.points.get(2) {
                        handles.push((HandlePos::Point(2), Pos2::new(entry.x, stop.y)));
                    }
                    handles.push((
                        HandlePos::Middle,
                        Pos2::new((entry.x + right) / 2.0, entry.y),
                    ));
                } else if let Some(&entry) = drawing.points.first() {
                    handles.push((HandlePos::Start, entry));
                }
            }
            DrawingToolType::ParallelChannel
            | DrawingToolType::FlatTopBottom
            | DrawingToolType::DisjointChannel => {
                for (i, &point) in drawing.points.iter().take(2).enumerate() {
                    handles.push((HandlePos::Point(i), point));
                }
                if let Some(ends) = drawing.channel_second_line() {
                    for (i, point) in ends.into_iter().enumerate() {
                        handles.push((HandlePos::Point(i + 2), point));
                    }
                }
            }
            DrawingToolType::RegressionTrend => {
                for (i, &point) in drawing.points.iter().take(2).enumerate() {
                    handles.push((HandlePos::Point(i), point));
                }
            }
            // Multi-point tools - show indexed handles
            _ => {
                for (i, &point) in drawing.points.iter().enumerate() {
                    handles.push((HandlePos::Point(i), point));
                }
            }
        }

        handles
    }

    /// Hit-tests to find which handle (if any) is at a screen position.
    ///
    /// Returns `Some(HandlePos)` if `point` is within `handle_size * 1.5` pixels
    /// of a handle center, `None` otherwise.
    pub fn hit_test_handle(drawing: &Drawing, point: Pos2, handle_size: f32) -> Option<HandlePos> {
        let hit_radius = handle_size * 1.5;
        // The nearest handle wins, so a small glyph's centre still moves it instead of
        // grabbing a corner that lies within reach.
        Self::get_handles(drawing)
            .into_iter()
            .map(|(pos, handle_point)| (pos, point.distance(handle_point)))
            .filter(|&(_, dist)| dist <= hit_radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(pos, _)| pos)
    }

    /// Moves a handle to a new screen position, updating both screen coordinates
    /// and chart coordinates using the provided conversion closures.
    ///
    /// For the `Middle` handle, all points are translated by the delta between
    /// the old and new centroid positions (move entire drawing).
    /// `translate_price(price, from_y, to_y)` shifts their prices in scale units;
    /// other handles use `y_to_price`.
    pub fn update_handle<F, G, H>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: F,
        y_to_price: G,
        translate_price: H,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
        H: Fn(f64, f32, f32) -> f64,
    {
        if handle == HandlePos::Middle {
            let count = drawing.points.len().max(1) as f32;
            let centroid = drawing
                .points
                .iter()
                .fold(egui::Vec2::ZERO, |sum, p| sum + p.to_vec2())
                / count;
            let center = Self::get_handles(drawing)
                .into_iter()
                .find(|(h, _)| *h == handle)
                .map_or(Pos2::ZERO + centroid, |(_, p)| p);
            if !drawing.points.is_empty() {
                let bars = x_to_bar(new_pos.x) - x_to_bar(center.x);
                for point in &mut drawing.chart_points {
                    point.bar_idx += bars;
                    point.price = translate_price(point.price, center.y, new_pos.y);
                }
                for point in &mut drawing.points {
                    *point += new_pos - center;
                }
            }
            return;
        }
        match drawing.tool_type {
            DrawingToolType::TrendLine
            | DrawingToolType::Measure
            | DrawingToolType::FibonacciRetracement
            | DrawingToolType::GannFan
            | DrawingToolType::GannSquare => {
                Self::update_two_point_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
            DrawingToolType::Rect | DrawingToolType::GannBox => {
                Self::update_rect_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
            DrawingToolType::FontIcon | DrawingToolType::Sticker
                if matches!(
                    handle,
                    HandlePos::TopLeft
                        | HandlePos::TopRight
                        | HandlePos::BottomLeft
                        | HandlePos::BottomRight
                ) =>
            {
                if let Some(&center) = drawing.points.first() {
                    // The centre-to-corner distance is half the square's diagonal.
                    drawing.font_size =
                        font_size(center.distance(new_pos) * std::f32::consts::SQRT_2);
                }
            }
            DrawingToolType::HorizontalLine => {
                if !drawing.points.is_empty() && !drawing.chart_points.is_empty() {
                    drawing.points[0].y = new_pos.y;
                    drawing.chart_points[0].price = y_to_price(new_pos.y);
                }
            }
            DrawingToolType::VerticalLine => {
                if !drawing.points.is_empty() && !drawing.chart_points.is_empty() {
                    drawing.points[0].x = new_pos.x;
                    drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                }
            }
            DrawingToolType::ParallelChannel
            | DrawingToolType::FlatTopBottom
            | DrawingToolType::DisjointChannel => {
                let HandlePos::Point(index) = handle else {
                    return;
                };
                if index < 2 {
                    Self::update_multi_point_handle(
                        drawing,
                        handle,
                        new_pos,
                        &x_to_bar,
                        &y_to_price,
                    );
                } else if index < 4 && drawing.chart_points.len() >= 3 {
                    let Some(ends) = drawing.channel_second_line() else {
                        return;
                    };
                    let delta = new_pos - ends[index - 2];
                    match drawing.tool_type {
                        DrawingToolType::DisjointChannel => {
                            // Keep the opposite ends fixed and reflect this end's price move.
                            let base = drawing.points[index - 2];
                            drawing.points[index - 2] = Pos2::new(new_pos.x, base.y - delta.y);
                            let anchor = &mut drawing.chart_points[index - 2];
                            anchor.bar_idx = x_to_bar(new_pos.x);
                            anchor.price = translate_price(anchor.price, base.y, base.y - delta.y);
                            Self::update_multi_point_handle(
                                drawing,
                                HandlePos::Point(2),
                                new_pos,
                                &x_to_bar,
                                &y_to_price,
                            );
                        }
                        DrawingToolType::ParallelChannel
                            if (drawing.points[1].x - drawing.points[0].x).abs() < 1e-3 =>
                        {
                            drawing.points[2].x += delta.x;
                            drawing.chart_points[2].bar_idx = x_to_bar(drawing.points[2].x);
                        }
                        _ => {
                            drawing.points[2].y += delta.y;
                            drawing.chart_points[2].price =
                                if drawing.tool_type == DrawingToolType::ParallelChannel {
                                    translate_price(
                                        drawing.chart_points[2].price,
                                        ends[index - 2].y,
                                        new_pos.y,
                                    )
                                } else {
                                    y_to_price(new_pos.y)
                                };
                        }
                    }
                }
            }
            DrawingToolType::RegressionTrend => {
                let index = match handle {
                    HandlePos::Start | HandlePos::Point(0) => 0,
                    HandlePos::End | HandlePos::Point(1) => 1,
                    _ => return,
                };
                if let (Some(point), Some(anchor)) = (
                    drawing.points.get_mut(index),
                    drawing.chart_points.get_mut(index),
                ) {
                    point.x = new_pos.x;
                    anchor.bar_idx = x_to_bar(new_pos.x);
                }
            }
            DrawingToolType::TimeCycles => {
                if drawing.points.len() < 2 || drawing.chart_points.len() < 2 {
                    return;
                }
                match handle {
                    HandlePos::Start | HandlePos::Point(0) => {
                        drawing.points[0] = new_pos;
                        drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                        drawing.chart_points[0].price = y_to_price(new_pos.y);
                    }
                    HandlePos::End | HandlePos::Point(1) => {
                        drawing.points[1].x = new_pos.x;
                        drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
                    }
                    _ => return,
                }
                drawing.level_time_cycles();
            }
            // The target and the stop move in price only; the width handle moves the right
            // edge, where the target and the stop anchors sit, never left of the entry.
            DrawingToolType::LongPos | DrawingToolType::ShortPos => match handle {
                HandlePos::Point(level @ (1 | 2))
                    if level < drawing.points.len() && level < drawing.chart_points.len() =>
                {
                    drawing.points[level].y = new_pos.y;
                    drawing.chart_points[level].price = y_to_price(new_pos.y);
                }
                HandlePos::End if drawing.points.len() > 1 && drawing.chart_points.len() > 1 => {
                    let x = new_pos.x.max(drawing.points[0].x);
                    for level in 1..drawing.points.len().min(drawing.chart_points.len()) {
                        drawing.points[level].x = x;
                        drawing.chart_points[level].bar_idx = x_to_bar(x);
                    }
                }
                _ => Self::update_multi_point_handle(
                    drawing,
                    handle,
                    new_pos,
                    &x_to_bar,
                    &y_to_price,
                ),
            },
            _ => {
                Self::update_multi_point_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
        }
    }

    /// Update handle for two-point tools (trendline, fib, etc.)
    fn update_two_point_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        match handle {
            HandlePos::Start if !drawing.points.is_empty() => {
                drawing.points[0] = new_pos;
                if !drawing.chart_points.is_empty() {
                    drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                    drawing.chart_points[0].price = y_to_price(new_pos.y);
                }
            }
            HandlePos::End if drawing.points.len() >= 2 => {
                drawing.points[1] = new_pos;
                if drawing.chart_points.len() >= 2 {
                    drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
                    drawing.chart_points[1].price = y_to_price(new_pos.y);
                }
            }
            _ => {}
        }
    }

    /// Update handle for rectangle tool
    fn update_rect_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if drawing.points.len() < 2 || drawing.chart_points.len() < 2 {
            return;
        }

        match handle {
            HandlePos::TopLeft => {
                drawing.points[0] = new_pos;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[0].price = y_to_price(new_pos.y);
            }
            HandlePos::TopRight => {
                drawing.points[0].y = new_pos.y;
                drawing.points[1].x = new_pos.x;
                drawing.chart_points[0].price = y_to_price(new_pos.y);
                drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
            }
            HandlePos::BottomLeft => {
                drawing.points[0].x = new_pos.x;
                drawing.points[1].y = new_pos.y;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[1].price = y_to_price(new_pos.y);
            }
            HandlePos::BottomRight => {
                drawing.points[1] = new_pos;
                drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[1].price = y_to_price(new_pos.y);
            }
            HandlePos::Top | HandlePos::Bottom => {
                let index = usize::from(handle == HandlePos::Bottom);
                drawing.points[index].y = new_pos.y;
                drawing.chart_points[index].price = y_to_price(new_pos.y);
            }
            HandlePos::Left | HandlePos::Right => {
                let index = usize::from(handle == HandlePos::Right);
                drawing.points[index].x = new_pos.x;
                drawing.chart_points[index].bar_idx = x_to_bar(new_pos.x);
            }
            _ => {}
        }
    }

    /// Update handle for multi-point tools
    fn update_multi_point_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        match handle {
            HandlePos::Point(idx)
                if idx < drawing.points.len() && idx < drawing.chart_points.len() =>
            {
                drawing.points[idx] = new_pos;
                drawing.chart_points[idx].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[idx].price = y_to_price(new_pos.y);
            }
            HandlePos::Start if !drawing.points.is_empty() && !drawing.chart_points.is_empty() => {
                drawing.points[0] = new_pos;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[0].price = y_to_price(new_pos.y);
            }
            HandlePos::End if drawing.points.len() >= 2 && drawing.chart_points.len() >= 2 => {
                let last = drawing.points.len() - 1;
                drawing.points[last] = new_pos;
                drawing.chart_points[last].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[last].price = y_to_price(new_pos.y);
            }
            _ => {}
        }
    }

    /// Renders selection handles like the selection dots of a series: an accent ring around
    /// the chart background.
    ///
    /// The actively dragged handle (if any) is filled with
    /// [`HandleConfig::selected_color`]; the others are ringed with [`HandleConfig::color`].
    pub fn render_handles(
        painter: &egui::Painter,
        drawing: &Drawing,
        config: &HandleConfig,
        dragging_handle: Option<HandlePos>,
    ) {
        if !config.show_handles {
            return;
        }

        let theme = painter.ctx().active_theme();
        let color = |[r, g, b, a]: [u8; 4]| {
            if [r, g, b, a] == Color32::PLACEHOLDER.to_array() {
                theme.ui.accent
            } else {
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
        };
        let radius = config.size / 2.0;
        let glyph = matches!(
            drawing.tool_type,
            DrawingToolType::FontIcon | DrawingToolType::Sticker
        );
        if glyph && let Some(&center) = drawing.points.first() {
            painter.rect_stroke(
                egui::Rect::from_center_size(
                    center,
                    egui::Vec2::splat(font_size(drawing.font_size)),
                ),
                0.0,
                egui::Stroke::new(1.0, color(config.color)),
                egui::StrokeKind::Outside,
            );
        }

        let position = matches!(
            drawing.tool_type,
            DrawingToolType::LongPos | DrawingToolType::ShortPos
        );
        for (pos, point) in Self::get_handles(drawing) {
            // A position moves by its body; its target and stop handles are squares.
            if (position || glyph) && pos == HandlePos::Middle {
                continue;
            }
            let ring = if dragging_handle == Some(pos) {
                None
            } else {
                Some(theme.chart.bg)
            };
            let mut fill = color(if ring.is_some() {
                config.color
            } else {
                config.selected_color
            });
            if position && ring.is_some() {
                fill = fill.lerp_to_gamma(theme.chart.bg, 0.15);
            }
            if position && matches!(pos, HandlePos::Point(_)) {
                let square = egui::Rect::from_center_size(point, egui::Vec2::splat(2.0 * radius));
                let corner = radius / 2.0;
                painter.rect(
                    square,
                    corner,
                    ring.unwrap_or(fill),
                    egui::Stroke::new(1.0, fill),
                    egui::StrokeKind::Inside,
                );
            } else if position {
                painter.circle(
                    point,
                    radius - 0.5,
                    ring.unwrap_or(fill),
                    egui::Stroke::new(1.0, fill),
                );
            } else {
                painter.circle_filled(point, radius, fill);
                if let Some(bg) = ring {
                    painter.circle_filled(point, radius - RING_WIDTH, bg);
                }
            }
        }
    }
}

/// Width of a handle's ring, as on a series' selection dots.
const RING_WIDTH: f32 = 2.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::ChartPoint;
    use egui::pos2;

    /// Screen to chart mapping used by the tests: 10 px per bar, price 1000 at
    /// the top falling one unit per pixel.
    fn x_to_bar(x: f32) -> f32 {
        x / 10.0
    }

    fn y_to_price(y: f32) -> f64 {
        1000.0 - y as f64
    }

    fn create_test_drawing(tool_type: DrawingToolType, points: Vec<Pos2>) -> Drawing {
        let mut drawing = Drawing::new(1, tool_type);
        drawing.chart_points = points
            .iter()
            .map(|p| ChartPoint::new(x_to_bar(p.x), y_to_price(p.y)))
            .collect();
        drawing.points = points;
        drawing
    }

    #[test]
    fn get_handles_by_tool() {
        use HandlePos::*;
        let cases = [
            (
                DrawingToolType::FontIcon,
                vec![pos2(100.0, 200.0)],
                vec![
                    (TopLeft, pos2(84.0, 184.0)),
                    (TopRight, pos2(116.0, 184.0)),
                    (BottomLeft, pos2(84.0, 216.0)),
                    (BottomRight, pos2(116.0, 216.0)),
                    (Middle, pos2(100.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::Sticker,
                vec![pos2(100.0, 200.0)],
                vec![
                    (TopLeft, pos2(84.0, 184.0)),
                    (TopRight, pos2(116.0, 184.0)),
                    (BottomLeft, pos2(84.0, 216.0)),
                    (BottomRight, pos2(116.0, 216.0)),
                    (Middle, pos2(100.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0), pos2(200.0, 200.0)],
                vec![
                    (Point(0), pos2(100.0, 200.0)),
                    (Point(1), pos2(300.0, 400.0)),
                    (Point(2), pos2(100.0, 100.0)),
                    (Point(3), pos2(300.0, 300.0)),
                ],
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0), pos2(200.0, 200.0)],
                vec![
                    (Point(0), pos2(100.0, 200.0)),
                    (Point(1), pos2(300.0, 400.0)),
                    (Point(2), pos2(100.0, 200.0)),
                    (Point(3), pos2(300.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0), pos2(200.0, 200.0)],
                vec![
                    (Point(0), pos2(100.0, 200.0)),
                    (Point(1), pos2(300.0, 400.0)),
                    (Point(2), pos2(100.0, 300.0)),
                    (Point(3), pos2(300.0, 100.0)),
                ],
            ),
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                vec![(Start, pos2(100.0, 200.0)), (End, pos2(300.0, 400.0))],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                vec![
                    (TopLeft, pos2(100.0, 100.0)),
                    (TopRight, pos2(200.0, 100.0)),
                    (BottomLeft, pos2(100.0, 300.0)),
                    (BottomRight, pos2(200.0, 300.0)),
                    (Top, pos2(150.0, 100.0)),
                    (Bottom, pos2(150.0, 300.0)),
                    (Left, pos2(100.0, 200.0)),
                    (Right, pos2(200.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::GannBox,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                vec![
                    (TopLeft, pos2(100.0, 100.0)),
                    (TopRight, pos2(200.0, 100.0)),
                    (BottomLeft, pos2(100.0, 300.0)),
                    (BottomRight, pos2(200.0, 300.0)),
                    (Top, pos2(150.0, 100.0)),
                    (Bottom, pos2(150.0, 300.0)),
                    (Left, pos2(100.0, 200.0)),
                    (Right, pos2(200.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::LongPos,
                vec![pos2(100.0, 200.0), pos2(400.0, 100.0), pos2(400.0, 250.0)],
                vec![
                    (Start, pos2(100.0, 200.0)),
                    (End, pos2(400.0, 200.0)),
                    (Point(1), pos2(100.0, 100.0)),
                    (Point(2), pos2(100.0, 250.0)),
                    (Middle, pos2(250.0, 200.0)),
                ],
            ),
            // Narrower than the minimum width: the width handle sits at the box's drawn edge.
            (
                DrawingToolType::ShortPos,
                vec![pos2(100.0, 200.0), pos2(150.0, 250.0), pos2(150.0, 120.0)],
                vec![
                    (Start, pos2(100.0, 200.0)),
                    (End, pos2(300.0, 200.0)),
                    (Point(1), pos2(100.0, 250.0)),
                    (Point(2), pos2(100.0, 120.0)),
                    (Middle, pos2(200.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::HorizontalLine,
                vec![pos2(50.0, 120.0)],
                vec![(Start, pos2(50.0, 120.0))],
            ),
            (
                DrawingToolType::FibonacciRetracement,
                vec![pos2(0.0, 0.0), pos2(100.0, 50.0)],
                vec![(Start, pos2(0.0, 0.0)), (End, pos2(100.0, 50.0))],
            ),
            (
                DrawingToolType::FibonacciChannel,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                vec![
                    (Point(0), pos2(0.0, 0.0)),
                    (Point(1), pos2(30.0, 0.0)),
                    (Point(2), pos2(0.0, 60.0)),
                ],
            ),
            (
                DrawingToolType::Triangle,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                vec![
                    (Point(0), pos2(0.0, 0.0)),
                    (Point(1), pos2(30.0, 0.0)),
                    (Point(2), pos2(0.0, 60.0)),
                ],
            ),
        ];
        for (tool, points, expected) in cases {
            let drawing = create_test_drawing(tool, points);
            assert_eq!(HandleService::get_handles(&drawing), expected, "{tool:?}");
        }
        // Renderer control counts, including generated position stops and freehand samples.
        let anchor_counts: &[(&[DrawingToolType], usize)] = &[
            (
                &[
                    DrawingToolType::CrossCursor,
                    DrawingToolType::DotCursor,
                    DrawingToolType::ArrowCursor,
                    DrawingToolType::Eraser,
                ],
                0,
            ),
            (
                &[
                    DrawingToolType::HorizontalLine,
                    DrawingToolType::HorizontalRay,
                    DrawingToolType::VerticalLine,
                    DrawingToolType::CrossLine,
                    DrawingToolType::TextLabel,
                    DrawingToolType::AnchoredText,
                    DrawingToolType::AnchoredNote,
                    DrawingToolType::PriceLabel,
                    DrawingToolType::FlagNote,
                    DrawingToolType::ArrowMarkUp,
                    DrawingToolType::ArrowMarkDown,
                    DrawingToolType::AnchoredVWAP,
                    DrawingToolType::AnchoredVolumeProfile,
                    DrawingToolType::Table,
                    DrawingToolType::Comment,
                    DrawingToolType::Signpost,
                    DrawingToolType::FontIcon,
                    DrawingToolType::Tweet,
                    DrawingToolType::Idea,
                ],
                1,
            ),
            (
                &[
                    DrawingToolType::TrendLine,
                    DrawingToolType::Ray,
                    DrawingToolType::InfoLine,
                    DrawingToolType::ExtendedLine,
                    DrawingToolType::TrendAngle,
                    DrawingToolType::RegressionTrend,
                    DrawingToolType::FibonacciRetracement,
                    DrawingToolType::FibonacciTimeZones,
                    DrawingToolType::FibonacciSpeedFan,
                    DrawingToolType::FibonacciCircles,
                    DrawingToolType::FibonacciSpiral,
                    DrawingToolType::FibonacciSpeedResistanceArcs,
                    DrawingToolType::GannBox,
                    DrawingToolType::GannSquare,
                    DrawingToolType::GannFixed,
                    DrawingToolType::GannFan,
                    DrawingToolType::CyclicLines,
                    DrawingToolType::TimeCycles,
                    DrawingToolType::SineLine,
                    DrawingToolType::Forecast,
                    DrawingToolType::FixedRangeVolumeProfile,
                    DrawingToolType::Measure,
                    DrawingToolType::PriceRange,
                    DrawingToolType::DateRange,
                    DrawingToolType::DateAndPriceRange,
                    DrawingToolType::ArrowMarker,
                    DrawingToolType::Arrow,
                    DrawingToolType::Rect,
                    DrawingToolType::Circle,
                    DrawingToolType::Ellipse,
                    DrawingToolType::Note,
                    DrawingToolType::PriceNote,
                    DrawingToolType::Callout,
                    DrawingToolType::Image,
                ],
                2,
            ),
            (
                &[
                    DrawingToolType::Pitchfork,
                    DrawingToolType::SchiffPitchfork,
                    DrawingToolType::ModifiedSchiffPitchfork,
                    DrawingToolType::InsidePitchfork,
                    DrawingToolType::FibonacciExtension,
                    DrawingToolType::FibonacciChannel,
                    DrawingToolType::FibonacciWedge,
                    DrawingToolType::TrendBasedFibTime,
                    DrawingToolType::Pitchfan,
                    DrawingToolType::Triangle,
                    DrawingToolType::RotatedRect,
                    DrawingToolType::Arc,
                    DrawingToolType::Curve,
                    DrawingToolType::ProjectionTool,
                    DrawingToolType::BarsPattern,
                    DrawingToolType::GhostFeed,
                    DrawingToolType::ElliottDoubleCombo,
                ],
                3,
            ),
            (
                &[
                    DrawingToolType::ABCDPattern,
                    DrawingToolType::ElliottCorrection,
                    DrawingToolType::TrianglePattern,
                    DrawingToolType::DoubleCurve,
                ],
                4,
            ),
            (
                &[
                    DrawingToolType::XABCDPattern,
                    DrawingToolType::CypherPattern,
                    DrawingToolType::ElliottTripleCombo,
                    DrawingToolType::Brush,
                    DrawingToolType::Highlighter,
                    DrawingToolType::Polyline,
                    DrawingToolType::Path,
                ],
                5,
            ),
            (
                &[
                    DrawingToolType::ElliottImpulse,
                    DrawingToolType::ElliottTriangle,
                ],
                6,
            ),
            (
                &[
                    DrawingToolType::HeadAndShoulders,
                    DrawingToolType::ThreeDrivesPattern,
                ],
                7,
            ),
        ];
        for &(tools, count) in anchor_counts {
            for &tool in tools {
                let points: Vec<_> = (0..count)
                    .map(|i| pos2(i as f32 * 30.0, i as f32 * 20.0))
                    .collect();
                let drawing = create_test_drawing(tool, points.clone());
                let handles = HandleService::get_handles(&drawing);
                for point in points {
                    assert!(
                        handles.iter().any(|(_, p)| *p == point),
                        "{tool:?}: no handle at {point:?}"
                    );
                }
            }
        }
    }

    /// Corner drags keep the centre fixed, clamp to loadable sizes and undo as one gesture.
    #[test]
    fn glyph_corner_drags_resize_about_the_centre() {
        use crate::drawings::DrawingManager;
        for tool in [DrawingToolType::FontIcon, DrawingToolType::Sticker] {
            for handle in [
                HandlePos::TopLeft,
                HandlePos::TopRight,
                HandlePos::BottomLeft,
                HandlePos::BottomRight,
            ] {
                let mut manager = DrawingManager::new();
                let drawing = create_test_drawing(tool, vec![pos2(100.0, 200.0)]);
                let anchors = drawing.chart_points.clone();
                manager.drawings.push(drawing);
                manager.start_drag_handle(1, handle);
                // A corner is half a diagonal from the centre: side = radius * sqrt(2).
                // Offsets (30, 40) have radius 50, giving side 70.710678.
                for (point, size) in [
                    (pos2(70.0, 160.0), 70.710_68),
                    (pos2(100.0, 200.0), 1.0),
                    (pos2(1000.0, 2000.0), 256.0),
                    (pos2(148.0, 248.0), 96.0),
                ] {
                    manager.update_drag_handle(point, x_to_bar, y_to_price, |p, from, to| {
                        p + f64::from(from - to)
                    });
                    let drawing = &manager.drawings[0];
                    assert!(
                        (drawing.font_size - size).abs() < 1e-4,
                        "{tool:?} {handle:?}"
                    );
                    assert_eq!(drawing.points, [pos2(100.0, 200.0)]);
                    assert_eq!(drawing.chart_points, anchors);
                    assert!(
                        crate::drawings::persistence::drawing_to_stored(drawing, |bar| {
                            bar as i64 * 60_000
                        })
                        .invalid_field()
                        .is_none()
                    );
                }
                let ctx = egui::Context::default();
                let output = ctx.run_ui(egui::RawInput::default(), |_| {
                    HandleService::render_handles(
                        &egui::Painter::new(
                            ctx.clone(),
                            egui::LayerId::background(),
                            egui::Rect::EVERYTHING,
                        ),
                        &manager.drawings[0],
                        &HandleConfig::default(),
                        Some(handle),
                    );
                });
                assert!(
                    output.shapes.iter().any(|s| matches!(&s.shape,
                        egui::Shape::Rect(r) if r.rect.center() == pos2(100.0, 200.0)
                            && (r.rect.width() - 96.0).abs() < 1e-4
                            && (r.rect.height() - 96.0).abs() < 1e-4 && r.stroke.width > 0.0
                    )),
                    "selected glyph has a square frame"
                );
                output.drop_without_applying_deltas();
                manager.end_drag_handle();
                assert!(manager.undo());
                assert_eq!(manager.drawings[0].font_size, 32.0);
                assert!(!manager.undo(), "one undo for the whole drag");
                assert!(manager.redo());
                assert!((manager.drawings[0].font_size - 96.0).abs() < 1e-4);
                HandleService::update_handle(
                    &mut manager.drawings[0],
                    HandlePos::Middle,
                    pos2(110.0, 220.0),
                    x_to_bar,
                    y_to_price,
                    |p, from, to| p + f64::from(from - to),
                );
                assert_eq!(manager.drawings[0].points, [pos2(110.0, 220.0)]);
                assert_eq!(
                    manager.drawings[0].chart_points,
                    [ChartPoint::new(11.0, 780.0)]
                );
            }
        }
    }

    #[test]
    fn test_hit_test_handle() {
        let drawing = create_test_drawing(
            DrawingToolType::TrendLine,
            vec![Pos2::new(100.0, 200.0), Pos2::new(300.0, 400.0)],
        );

        // Test hitting the start handle
        let hit = HandleService::hit_test_handle(&drawing, Pos2::new(102.0, 202.0), 8.0);
        assert_eq!(hit, Some(HandlePos::Start));

        // Test missing all handles
        let miss = HandleService::hit_test_handle(&drawing, Pos2::new(500.0, 500.0), 8.0);
        assert_eq!(miss, None);

        // A 16 px glyph has its corners within reach of its centre: the centre still moves it.
        let mut glyph = create_test_drawing(DrawingToolType::Sticker, vec![pos2(100.0, 200.0)]);
        glyph.font_size = 16.0;
        for (point, expected) in [
            (pos2(100.0, 200.0), HandlePos::Middle),
            (pos2(108.0, 208.0), HandlePos::BottomRight),
        ] {
            assert_eq!(
                HandleService::hit_test_handle(&glyph, point, 8.0),
                Some(expected)
            );
        }
    }

    /// A handle drag moves the screen points and the persisted chart points
    /// together: one anchor for an endpoint, one coordinate of each anchor for
    /// a rectangle corner, and every anchor for the Middle handle of a two-point
    /// or multi-point tool.
    #[test]
    fn update_handle_moves_screen_and_chart_points() {
        // Edges ignore the pointer's other coordinate, including reversed and crossed sides.
        let cases = [
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::Top,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 80.0), pos2(200.0, 300.0)],
                vec![ChartPoint::new(10.0, 920.0), ChartPoint::new(20.0, 700.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(200.0, 300.0), pos2(100.0, 100.0)],
                HandlePos::Top,
                pos2(250.0, 80.0),
                vec![pos2(200.0, 80.0), pos2(100.0, 100.0)],
                vec![ChartPoint::new(20.0, 920.0), ChartPoint::new(10.0, 900.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::Bottom,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 100.0), pos2(200.0, 80.0)],
                vec![ChartPoint::new(10.0, 900.0), ChartPoint::new(20.0, 920.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(200.0, 300.0), pos2(100.0, 100.0)],
                HandlePos::Bottom,
                pos2(250.0, 80.0),
                vec![pos2(200.0, 300.0), pos2(100.0, 80.0)],
                vec![ChartPoint::new(20.0, 700.0), ChartPoint::new(10.0, 920.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::Left,
                pos2(250.0, 80.0),
                vec![pos2(250.0, 100.0), pos2(200.0, 300.0)],
                vec![ChartPoint::new(25.0, 900.0), ChartPoint::new(20.0, 700.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(200.0, 300.0), pos2(100.0, 100.0)],
                HandlePos::Left,
                pos2(250.0, 80.0),
                vec![pos2(250.0, 300.0), pos2(100.0, 100.0)],
                vec![ChartPoint::new(25.0, 700.0), ChartPoint::new(10.0, 900.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::Right,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 100.0), pos2(250.0, 300.0)],
                vec![ChartPoint::new(10.0, 900.0), ChartPoint::new(25.0, 700.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(200.0, 300.0), pos2(100.0, 100.0)],
                HandlePos::Right,
                pos2(250.0, 80.0),
                vec![pos2(200.0, 300.0), pos2(250.0, 100.0)],
                vec![ChartPoint::new(20.0, 700.0), ChartPoint::new(25.0, 900.0)],
            ),
            // The width anchor can be off screen while both boundary handles are visible.
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 100.0), pos2(200.0, 200.0), pos2(600.0, 700.0)],
                HandlePos::Point(2),
                pos2(150.0, 225.0),
                vec![pos2(100.0, 100.0), pos2(200.0, 200.0), pos2(600.0, 725.0)],
                vec![
                    ChartPoint::new(10.0, 900.0),
                    ChartPoint::new(20.0, 800.0),
                    ChartPoint::new(60.0, 275.0),
                ],
            ),
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(0),
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(15.0, 750.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(1),
                pos2(350.0, 250.0),
                vec![pos2(100.0, 200.0), pos2(350.0, 250.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(35.0, 750.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(2),
                pos2(150.0, 75.0),
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 125.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 875.0),
                ],
            ),
            (
                DrawingToolType::ParallelChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(3),
                pos2(350.0, 175.0),
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 125.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 875.0),
                ],
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(0),
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(15.0, 750.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(1),
                pos2(350.0, 250.0),
                vec![pos2(100.0, 200.0), pos2(350.0, 250.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(35.0, 750.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(2),
                pos2(150.0, 125.0),
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 125.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 875.0),
                ],
            ),
            (
                DrawingToolType::FlatTopBottom,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(3),
                pos2(350.0, 125.0),
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 125.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 875.0),
                ],
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(0),
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(15.0, 750.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(1),
                pos2(350.0, 250.0),
                vec![pos2(100.0, 200.0), pos2(350.0, 250.0), pos2(200.0, 100.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(35.0, 750.0),
                    ChartPoint::new(20.0, 900.0),
                ],
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(2),
                pos2(150.0, 175.0),
                vec![pos2(150.0, 175.0), pos2(300.0, 300.0), pos2(150.0, 175.0)],
                vec![
                    ChartPoint::new(15.0, 825.0),
                    ChartPoint::new(30.0, 700.0),
                    ChartPoint::new(15.0, 825.0),
                ],
            ),
            (
                DrawingToolType::DisjointChannel,
                vec![pos2(100.0, 200.0), pos2(300.0, 300.0), pos2(200.0, 100.0)],
                HandlePos::Point(3),
                pos2(350.0, 75.0),
                vec![pos2(100.0, 200.0), pos2(350.0, 275.0), pos2(350.0, 75.0)],
                vec![
                    ChartPoint::new(10.0, 800.0),
                    ChartPoint::new(35.0, 725.0),
                    ChartPoint::new(35.0, 925.0),
                ],
            ),
            (
                DrawingToolType::TimeCycles,
                vec![pos2(100.0, 200.0), pos2(300.0, 200.0)],
                HandlePos::Point(0),
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 250.0)],
                vec![ChartPoint::new(15.0, 750.0), ChartPoint::new(30.0, 750.0)],
            ),
            (
                DrawingToolType::TimeCycles,
                vec![pos2(100.0, 200.0), pos2(300.0, 200.0)],
                HandlePos::Point(1),
                pos2(50.0, 250.0),
                vec![pos2(100.0, 200.0), pos2(50.0, 200.0)],
                vec![ChartPoint::new(10.0, 800.0), ChartPoint::new(5.0, 800.0)],
            ),
            (
                DrawingToolType::TimeCycles,
                vec![pos2(100.0, 200.0), pos2(300.0, 200.0)],
                HandlePos::Middle,
                pos2(250.0, 280.0),
                vec![pos2(150.0, 280.0), pos2(350.0, 280.0)],
                vec![ChartPoint::new(15.0, 720.0), ChartPoint::new(35.0, 720.0)],
            ),
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                HandlePos::Start,
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 400.0)],
                vec![ChartPoint::new(15.0, 750.0), ChartPoint::new(30.0, 600.0)],
            ),
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                HandlePos::Middle,
                pos2(250.0, 280.0),
                vec![pos2(150.0, 180.0), pos2(350.0, 380.0)],
                vec![ChartPoint::new(15.0, 820.0), ChartPoint::new(35.0, 620.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::TopRight,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 80.0), pos2(250.0, 300.0)],
                vec![ChartPoint::new(10.0, 920.0), ChartPoint::new(25.0, 700.0)],
            ),
            (
                DrawingToolType::GannBox,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::TopRight,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 80.0), pos2(250.0, 300.0)],
                vec![ChartPoint::new(10.0, 920.0), ChartPoint::new(25.0, 700.0)],
            ),
            (
                DrawingToolType::Triangle,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                HandlePos::Middle,
                pos2(20.0, 30.0),
                vec![pos2(10.0, 10.0), pos2(40.0, 10.0), pos2(10.0, 70.0)],
                vec![
                    ChartPoint::new(1.0, 990.0),
                    ChartPoint::new(4.0, 990.0),
                    ChartPoint::new(1.0, 930.0),
                ],
            ),
        ];
        for (tool, points, handle, to, screen, chart) in cases {
            for logarithmic in [false, true] {
                let price = |y: f32| {
                    if logarithmic {
                        1000.0 * (-f64::from(y) / 100.0).exp()
                    } else {
                        y_to_price(y)
                    }
                };
                let mut drawing = create_test_drawing(tool, points.clone());
                for (anchor, point) in drawing.chart_points.iter_mut().zip(&drawing.points) {
                    anchor.price = price(point.y);
                }
                HandleService::update_handle(
                    &mut drawing,
                    handle,
                    to,
                    x_to_bar,
                    |y: f32| price(y.clamp(0.0, 350.0)),
                    |p, from, to| {
                        if logarithmic {
                            p * (-f64::from(to - from) / 100.0).exp()
                        } else {
                            p - f64::from(to - from)
                        }
                    },
                );
                assert_eq!(drawing.points, screen, "{tool:?} {handle:?}");
                let expected = if logarithmic {
                    screen
                        .iter()
                        .map(|p| ChartPoint::new(x_to_bar(p.x), price(p.y)))
                        .collect()
                } else {
                    chart.clone()
                };
                if logarithmic {
                    assert_eq!(drawing.chart_points.len(), expected.len());
                    for (got, want) in drawing.chart_points.iter().zip(&expected) {
                        assert_eq!(got.bar_idx, want.bar_idx, "{tool:?} {handle:?}");
                        assert!(
                            (got.price - want.price).abs() < 1e-12 * want.price.abs(),
                            "{tool:?} {handle:?}: {got:?}, expected {want:?}"
                        );
                    }
                } else {
                    assert_eq!(drawing.chart_points, expected, "{tool:?} {handle:?}");
                }
            }
        }
    }
}
