use super::context::{ChartMapping, RenderContext};
/// Marker renderer for chart annotations
use crate::model::{Bar, Marker, MarkerPos, MarkerShape};
use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, epaint::StrokeKind};

/// Render markers on the chart
pub fn render_markers(
    context: &RenderContext,
    markers: &[Marker],
    visible_data: &[Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
) {
    for marker in markers {
        // Find the corresponding bar for this marker
        if let Some(bar_idx) = visible_data.iter().position(|bar| bar.time == marker.time) {
            let global_idx = coords.start_idx + bar_idx;
            let bar = &visible_data[bar_idx];

            // Calculate marker position using ChartCoords helper
            let x = coords.idx_to_x(global_idx);

            // The bar's ends on screen; the high is the lower end on an inverted scale.
            let (high_y, low_y) = (
                price_scale.price_to_y(bar.high),
                price_scale.price_to_y(bar.low),
            );
            let (top, bottom) = (high_y.min(low_y), high_y.max(low_y));

            // Calculate Y position based on marker position setting
            let y = match marker.position {
                MarkerPos::AboveBar => top - 20.0 * marker.size,
                MarkerPos::BelowBar => bottom + 20.0 * marker.size,
                MarkerPos::InBar => price_scale.price_to_y(bar.close),
                MarkerPos::Top => context.rect.top() + 20.0 * marker.size,
                MarkerPos::Bottom => context.rect.bottom() - 20.0 * marker.size,
                MarkerPos::Left | MarkerPos::Right | MarkerPos::Absolute => {
                    price_scale.price_to_y(bar.close)
                }
                MarkerPos::AbsoluteUp => top - 10.0 * marker.size,
                MarkerPos::AbsoluteDown => bottom + 10.0 * marker.size,
            };

            let pos = Pos2::new(x, y);

            // Render the marker shape
            render_marker_shape(
                context.painter,
                pos,
                marker.shape,
                marker.color,
                marker.size,
            );

            // Render text label if present
            if let Some(text) = &marker.text {
                let text_pos = Pos2::new(pos.x, pos.y + 12.0 * marker.size);
                context.painter.text(
                    text_pos,
                    egui::Align2::CENTER_TOP,
                    text,
                    FontId::proportional(10.0 * marker.size),
                    marker.color,
                );
            }

            // TODO: Add tooltip on hover (requires hover detection in chart widget)
        }
    }
}

/// Render a single marker shape
fn render_marker_shape(
    painter: &Painter,
    pos: Pos2,
    shape: MarkerShape,
    color: Color32,
    size: f32,
) {
    let base_size = 8.0 * size;

    match shape {
        MarkerShape::Circle => {
            painter.circle_filled(pos, base_size, color);
            painter.circle_stroke(
                pos,
                base_size,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            );
        }
        MarkerShape::Square => {
            painter.rect_filled(
                Rect::from_center_size(pos, Vec2::splat(base_size * 2.0)),
                0.0,
                color,
            );
            painter.rect_stroke(
                Rect::from_center_size(pos, Vec2::splat(base_size * 2.0)),
                0.0,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
                StrokeKind::Outside,
            );
        }
        MarkerShape::ArrowUp => {
            // Draw triangle pointing up
            let points = vec![
                pos + Vec2::new(0.0, -base_size * 1.2),             // top
                pos + Vec2::new(-base_size * 0.8, base_size * 0.6), // bottom left
                pos + Vec2::new(base_size * 0.8, base_size * 0.6),  // bottom right
            ];
            painter.add(Shape::convex_polygon(
                points,
                color,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            ));
        }
        MarkerShape::ArrowDown => {
            // Draw triangle pointing down
            let points = vec![
                pos + Vec2::new(0.0, base_size * 1.2),               // bottom
                pos + Vec2::new(-base_size * 0.8, -base_size * 0.6), // top left
                pos + Vec2::new(base_size * 0.8, -base_size * 0.6),  // top right
            ];
            painter.add(Shape::convex_polygon(
                points,
                color,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            ));
        }
        MarkerShape::TriangleUp => {
            // Hollow triangle pointing up
            let points = vec![
                pos + Vec2::new(0.0, -base_size * 1.2),
                pos + Vec2::new(-base_size * 0.8, base_size * 0.6),
                pos + Vec2::new(base_size * 0.8, base_size * 0.6),
            ];
            painter.add(Shape::convex_polygon(
                points,
                Color32::TRANSPARENT,
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            ));
        }
        MarkerShape::TriangleDown => {
            // Hollow triangle pointing down
            let points = vec![
                pos + Vec2::new(0.0, base_size * 1.2),
                pos + Vec2::new(-base_size * 0.8, -base_size * 0.6),
                pos + Vec2::new(base_size * 0.8, -base_size * 0.6),
            ];
            painter.add(Shape::convex_polygon(
                points,
                Color32::TRANSPARENT,
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            ));
        }
        MarkerShape::Diamond => {
            // Draw diamond (rotated square)
            let points = vec![
                pos + Vec2::new(0.0, -base_size),
                pos + Vec2::new(-base_size, 0.0),
                pos + Vec2::new(0.0, base_size),
                pos + Vec2::new(base_size, 0.0),
            ];
            painter.add(Shape::convex_polygon(
                points,
                color,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            ));
        }
        MarkerShape::Star => {
            // Draw simple 4-pointed star
            let outer = base_size;
            let inner = base_size * 0.4;
            let points = vec![
                pos + Vec2::new(0.0, -outer), // top
                pos + Vec2::new(inner * 0.3, -inner * 0.3),
                pos + Vec2::new(outer, 0.0), // right
                pos + Vec2::new(inner * 0.3, inner * 0.3),
                pos + Vec2::new(0.0, outer), // bottom
                pos + Vec2::new(-inner * 0.3, inner * 0.3),
                pos + Vec2::new(-outer, 0.0), // left
                pos + Vec2::new(-inner * 0.3, -inner * 0.3),
            ];
            painter.add(Shape::convex_polygon(
                points,
                color,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            ));
        }
        MarkerShape::Cross => {
            // Draw X
            let size = base_size * 1.2;
            painter.line_segment(
                [pos + Vec2::new(-size, -size), pos + Vec2::new(size, size)],
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            );
            painter.line_segment(
                [pos + Vec2::new(-size, size), pos + Vec2::new(size, -size)],
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            );
        }
        MarkerShape::Flag => {
            // Draw flag pole and triangle
            painter.line_segment(
                [pos, pos + Vec2::new(0.0, base_size * 2.0)],
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            );
            let points = vec![
                pos,
                pos + Vec2::new(base_size * 1.2, -base_size * 0.6),
                pos + Vec2::new(0.0, -base_size * 1.2),
            ];
            painter.add(Shape::convex_polygon(
                points,
                color,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.chart.crosshair_label_text,
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform};
    use chrono::{TimeZone, Utc};

    /// A bar from 95 to 105 on a 90..110 scale over 500 px spans y 125..375 in
    /// either orientation, so each marker lands at the same y: above positions
    /// offset up from 125, below positions down from 375.
    #[test]
    fn markers_sit_outside_the_bar_on_either_scale_orientation() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 500.0));
        let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = [Bar::new(t, 100.0, 105.0, 95.0, 100.0, 1.0)];
        for inverted in [false, true] {
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                0.0,
                500.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 10.0, 0, 0, 0.0, map);
            for (position, want) in [
                (MarkerPos::AboveBar, 105.0),
                (MarkerPos::BelowBar, 395.0),
                (MarkerPos::AbsoluteUp, 115.0),
                (MarkerPos::AbsoluteDown, 385.0),
            ] {
                let marker = Marker::new(t)
                    .with_pos(position)
                    .with_shape(MarkerShape::Circle);
                let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    let painter = Painter::new(ui.ctx().clone(), egui::LayerId::background(), rect);
                    render_markers(
                        &RenderContext::new(&painter, rect),
                        std::slice::from_ref(&marker),
                        &bars,
                        &map,
                        &coords,
                    );
                });
                let y = output
                    .shapes
                    .iter()
                    .find_map(|c| match &c.shape {
                        Shape::Circle(circle) => Some(circle.center.y),
                        _ => None,
                    })
                    .expect("a marker circle");
                output.drop_without_applying_deltas();
                assert!(
                    (y - want).abs() < 1e-3,
                    "{position:?}, inverted {inverted}: y {y}, want {want}"
                );
            }
        }
    }
}
