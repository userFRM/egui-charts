//! Grid rendering logic
//! Grid lines are fixed to bar indices and price levels, moving 1:1 with the chart

use crate::chart::renderers::ChartMapping;
use crate::model::LineStyle;
use crate::painting::dashed_line::draw_dashed_segments;
use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// Renders horizontal price grid lines using smart mark generation
/// Lines anchor to "nice" price values and adapt density based on zoom
pub fn render_grid(
    painter: &Painter,
    rect: Rect,
    price_map: &PriceMap,
    grid_color: Color32,
    style: LineStyle,
) {
    let marks = price_map.shared_marks();
    render_grid_lines(
        painter,
        marks.iter().map(|mark| {
            [
                Pos2::new(rect.min.x, mark.y_coord),
                Pos2::new(rect.max.x, mark.y_coord),
            ]
        }),
        grid_color,
        style,
    );
}

/// Renders vertical grid lines at fixed bar intervals
/// Grid lines are tied directly to bar indices, so they move 1:1 with the chart during pan
pub fn render_vertical_grid(
    painter: &Painter,
    rect: Rect,
    coords: &ChartMapping,
    grid_color: Color32,
    style: LineStyle,
) {
    // Calculate grid interval based on bar spacing (zoom level)
    // More zoomed in (larger bar_spacing) = more grid lines
    // More zoomed out (smaller bar_spacing) = fewer grid lines
    let min_pixel_spacing = 80.0; // Min pixels between grid lines
    let bars_per_grid = (min_pixel_spacing / coords.bar_spacing).ceil().max(1.0) as usize;

    // Round to nice intervals: 1, 2, 5, 10, 20, 50, 100, etc.
    let bars_per_grid = nice_interval(bars_per_grid);

    // Calculate visible bar range
    let chart_width = rect.width();
    let visible_bars = ((chart_width / coords.bar_spacing).ceil() as usize).saturating_add(2);

    // Find the first grid line that's visible
    // Grid lines are at indices 0, bars_per_grid, 2*bars_per_grid, etc.
    let first_visible_idx = if coords.start_idx > 0 {
        (coords.start_idx / bars_per_grid) * bars_per_grid
    } else {
        0
    };

    let lines = (0..=(visible_bars / bars_per_grid + 2)).filter_map(|i| {
        let grid_idx = first_visible_idx.saturating_add(i.saturating_mul(bars_per_grid));

        // Convert bar index to x coord using same formula as candle rendering
        let x = coords.idx_to_x(grid_idx);

        // Skip if outside visible area
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
            return None;
        }
        Some([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)])
    });
    render_grid_lines(painter, lines, grid_color, style);
}

/// Paints one-pixel grid lines in `style`, batching their square dots into one mesh.
pub(crate) fn render_grid_lines(
    painter: &Painter,
    lines: impl IntoIterator<Item = [Pos2; 2], IntoIter: Clone>,
    color: Color32,
    style: LineStyle,
) {
    let stroke = Stroke::new(DESIGN_TOKENS.stroke.hairline, color);
    match style.hairline_pattern() {
        Some((dash, gap)) => draw_dashed_segments(
            painter,
            lines.into_iter().map(|[start, end]| (start, end, 0.0)),
            stroke,
            dash,
            gap,
        ),
        None => {
            painter.extend(
                lines
                    .into_iter()
                    .map(|points| egui::Shape::line_segment(points, stroke)),
            );
        }
    }
}

/// The smallest of 1, 2 or 5 times a power of ten that is at least `raw` (1, 2, 5, 10, 20, 50,
/// 100, etc.), so that grid lines stay at least the minimum spacing apart at any zoom.
pub(crate) fn nice_interval(raw: usize) -> usize {
    let mut power = 1usize;
    loop {
        for step in [1, 2, 5] {
            let nice = power.saturating_mul(step);
            if nice >= raw {
                return nice;
            }
        }
        power = power.saturating_mul(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform};
    use crate::theme::{Theme, apply_to_egui};
    use egui::{Context, LayerId, RawInput, Shape, vec2};

    #[test]
    fn grid_dots_follow_the_style_in_both_directions_with_the_supplied_colour() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 240.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            rect.top(),
            rect.bottom(),
            false,
        );
        let coords = ChartMapping::new(rect, 6.0, 0, 60, 0.0, price);
        for (theme, scale) in [
            (Theme::light(), 1.0),
            (Theme::light(), 2.0),
            (Theme::dark(), 1.0),
            (Theme::dark(), 2.0),
        ] {
            let ctx = Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.set_pixels_per_point(scale);
            for (style, dot_gap, vertical) in [
                (LineStyle::Dotted, 3.0, false),
                (LineStyle::Dotted, 3.0, true),
                (LineStyle::SparseDotted, 7.0, false),
                (LineStyle::SparseDotted, 7.0, true),
            ] {
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |_| {
                        let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                        if vertical {
                            render_vertical_grid(&painter, rect, &coords, Color32::RED, style);
                        } else {
                            render_grid(&painter, rect, &price, Color32::RED, style);
                        }
                    },
                );
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .filter(|shape| matches!(shape.shape, Shape::Mesh(_)))
                        .count(),
                    1,
                    "grid dots share one mesh"
                );
                let mut span_count = 0;
                for shape in &output.shapes {
                    match &shape.shape {
                        Shape::LineSegment { .. } => panic!("solid grid segment"),
                        Shape::Mesh(mesh) => {
                            let dots: Vec<_> = mesh
                                .vertices
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|vertices| {
                                    assert!(vertices.iter().all(|v| v.color == Color32::RED));
                                    let bounds = Rect::from_points(&vertices.map(|v| v.pos));
                                    assert_eq!(bounds.size(), vec2(1.0, 1.0));
                                    bounds
                                })
                                .collect();
                            for line in dots.chunk_by(|a, b| {
                                if vertical {
                                    a.left() == b.left()
                                } else {
                                    a.top() == b.top()
                                }
                            }) {
                                for pair in line.windows(2) {
                                    let gap = if vertical {
                                        pair[1].top() - pair[0].bottom()
                                    } else {
                                        pair[1].left() - pair[0].right()
                                    };
                                    assert_eq!(gap, dot_gap);
                                }
                            }
                            span_count += dots.len();
                        }
                        _ => {}
                    }
                }
                assert!(span_count > 0);
                let meshes = ctx.tessellate(output.shapes.clone(), output.pixels_per_point);
                output.drop_without_applying_deltas();
                let ink_area: f32 = meshes
                    .iter()
                    .filter_map(|primitive| match &primitive.primitive {
                        egui::epaint::Primitive::Mesh(mesh) => Some(mesh),
                        _ => None,
                    })
                    .flat_map(|mesh| {
                        mesh.indices.as_chunks::<3>().0.iter().map(|triangle| {
                            let a = mesh.vertices[triangle[0] as usize].pos;
                            let b = mesh.vertices[triangle[1] as usize].pos;
                            let c = mesh.vertices[triangle[2] as usize].pos;
                            let ab = b - a;
                            let ac = c - a;
                            (ab.x * ac.y - ab.y * ac.x).abs() * 0.5
                        })
                    })
                    .sum();
                assert!(ink_area > 0.0, "grid vanished during pixel rounding");
            }
        }
    }
}
