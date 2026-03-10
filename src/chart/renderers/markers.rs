use super::context::{ChartMapping, PriceScale, RenderContext};
/// Marker renderer for chart annotations
use crate::model::{Bar, Marker, MarkerPos, MarkerShape};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, epaint::StrokeKind};

/// Render markers on the chart
pub fn render_markers(
    context: &RenderContext,
    markers: &[Marker],
    visible_data: &[Bar],
    price_scale: &PriceScale,
    coords: &ChartMapping,
) {
    for marker in markers {
        // Find the corresponding bar for this marker
        if let Some(bar_idx) = visible_data.iter().position(|bar| bar.time == marker.time) {
            let global_idx = coords.start_idx + bar_idx;
            let bar = &visible_data[bar_idx];

            // Calculate marker position using ChartCoords helper
            let x = coords.idx_to_x(global_idx);

            // Calculate Y position based on marker position setting using PriceScale helper
            let y = match marker.position {
                MarkerPos::AboveBar => {
                    let high_y = price_scale.price_to_y(bar.high, context.rect);
                    high_y - 20.0 * marker.size // 20 pixels above high
                }
                MarkerPos::BelowBar => {
                    let low_y = price_scale.price_to_y(bar.low, context.rect);
                    low_y + 20.0 * marker.size // 20 pixels below low
                }
                MarkerPos::InBar => price_scale.price_to_y(bar.close, context.rect),
                MarkerPos::Top => context.rect.top() + 20.0 * marker.size,
                MarkerPos::Bottom => context.rect.bottom() - 20.0 * marker.size,
                MarkerPos::Left | MarkerPos::Right | MarkerPos::Absolute => {
                    price_scale.price_to_y(bar.close, context.rect)
                }
                MarkerPos::AbsoluteUp => {
                    let high_y = price_scale.price_to_y(bar.high, context.rect);
                    high_y - 10.0 * marker.size
                }
                MarkerPos::AbsoluteDown => {
                    let low_y = price_scale.price_to_y(bar.low, context.rect);
                    low_y + 10.0 * marker.size
                }
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
