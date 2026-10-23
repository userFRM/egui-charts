//! Grid rendering logic
//! Grid lines are fixed to bar indices and price levels, moving 1:1 with the chart

use super::dashed_line::draw_dashed_line;
use crate::chart::renderers::ChartMapping;
use crate::scales::{PriceMap, PriceMarkGenerator};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// Renders horizontal price grid lines using smart mark generation
/// Lines anchor to "nice" price values and adapt density based on zoom
pub fn render_grid(painter: &Painter, rect: Rect, price_map: &PriceMap, grid_color: Color32) {
    let marks = PriceMarkGenerator::new().generate_marks(price_map);

    for mark in marks {
        let stroke_width = if grid_color.is_opaque() {
            DESIGN_TOKENS.stroke.hairline
        } else if mark.weight >= 80 {
            DESIGN_TOKENS.stroke.light
        } else {
            DESIGN_TOKENS.stroke.extra_thin
        };
        render_grid_line(
            painter,
            Pos2::new(rect.min.x, mark.y_coord),
            Pos2::new(rect.max.x, mark.y_coord),
            Stroke::new(stroke_width, grid_color),
        );
    }
}

/// Renders vertical grid lines at fixed bar intervals
/// Grid lines are tied directly to bar indices, so they move 1:1 with the chart during pan
pub fn render_vertical_grid(
    painter: &Painter,
    rect: Rect,
    coords: &ChartMapping,
    grid_color: Color32,
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
    let visible_bars = (chart_width / coords.bar_spacing).ceil() as usize + 2;

    // Find the first grid line that's visible
    // Grid lines are at indices 0, bars_per_grid, 2*bars_per_grid, etc.
    let first_visible_idx = if coords.start_idx > 0 {
        (coords.start_idx / bars_per_grid) * bars_per_grid
    } else {
        0
    };

    // Draw grid lines
    for i in 0..=(visible_bars / bars_per_grid + 2) {
        let grid_idx = first_visible_idx + i * bars_per_grid;

        // Convert bar index to x coord using same formula as candle rendering
        let x = coords.idx_to_x(grid_idx);

        // Skip if outside visible area
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
            continue;
        }

        render_grid_line(
            painter,
            Pos2::new(x, rect.min.y),
            Pos2::new(x, rect.max.y),
            Stroke::new(
                if grid_color.is_opaque() {
                    DESIGN_TOKENS.stroke.hairline
                } else {
                    DESIGN_TOKENS.stroke.extra_thin
                },
                grid_color,
            ),
        );
    }
}

/// Paints a grid line at the caller's price or time mark, dotted in light themes.
pub(crate) fn render_grid_line(painter: &Painter, start: Pos2, end: Pos2, mut stroke: Stroke) {
    if painter.ctx().active_theme().dark {
        painter.line_segment([start, end], stroke);
    } else {
        stroke.width = DESIGN_TOKENS.stroke.hairline;
        draw_dashed_line(painter, start, end, stroke, 1.0, 3.0);
    }
}

/// Round to a nice interval (1, 2, 5, 10, 20, 50, 100, etc.)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform};
    use crate::theme::{Theme, apply_to_egui};
    use egui::{Context, LayerId, RawInput, Shape, vec2};

    #[test]
    fn light_grid_is_dotted_in_both_directions_with_the_supplied_colour() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 240.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            rect.top(),
            rect.bottom(),
            false,
        );
        let coords = ChartMapping::new(rect, 6.0, 0, 60, 0.0, price);
        for (theme, dotted, scale) in [
            (Theme::light(), true, 1.0),
            (Theme::light(), true, 2.0),
            (Theme::dark(), false, 1.0),
            (Theme::dark(), false, 2.0),
        ] {
            let ctx = Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.set_pixels_per_point(scale);
            for vertical in [false, true] {
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |_| {
                        let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                        if vertical {
                            render_vertical_grid(&painter, rect, &coords, Color32::RED);
                        } else {
                            render_grid(&painter, rect, &price, Color32::RED);
                        }
                    },
                );
                let lines: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::LineSegment { points, stroke } => {
                            Some((points[0].distance(points[1]), stroke.width, stroke.color))
                        }
                        Shape::Mesh(dot) => {
                            let rect = dot.calc_bounds();
                            Some((
                                if vertical {
                                    rect.height()
                                } else {
                                    rect.width()
                                },
                                if vertical {
                                    rect.width()
                                } else {
                                    rect.height()
                                },
                                dot.vertices[0].color,
                            ))
                        }
                        _ => None,
                    })
                    .collect();
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
                assert!(!lines.is_empty());
                for (length, width, colour) in lines {
                    assert_eq!(colour, Color32::RED);
                    assert_eq!(width, 1.0);
                    if dotted {
                        assert!(length <= 1.001, "solid light grid segment: {length}");
                    } else {
                        assert!(length >= rect.height());
                    }
                }
            }
        }
    }
}
