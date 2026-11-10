//! Vertical study ramps, split at colour stops before tessellation.

use crate::chart::rendering::ChartMapping;
use crate::scales::PriceMap;
use crate::studies::Indicator;
use egui::{Color32, Pos2};

// Clip in value space so inverted scales keep the same colours and crossing positions.
fn clip(points: &[[f64; 2]], level: f64, above: bool) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let Some(mut previous) = points.last().copied() else {
        return out;
    };
    for &point in points {
        let inside = (point[1] >= level) == above;
        if inside != ((previous[1] >= level) == above) {
            let t = (level - previous[1]) / (point[1] - previous[1]);
            out.push([previous[0] + t * (point[0] - previous[0]), level]);
        }
        if inside {
            out.push(point);
        }
        previous = point;
    }
    out
}

pub(super) fn paint(
    painter: &egui::Painter,
    study: &dyn Indicator,
    bars: std::ops::Range<usize>,
    map: &PriceMap,
    coords: &ChartMapping,
) {
    for fill in study.gradients() {
        let [start, end] = fill.range;
        if fill
            .colors
            .iter()
            .all(|color| *color == Color32::TRANSPARENT)
            || fill.plot >= study.line_count()
            || ![start, end, fill.baseline].iter().all(|v| v.is_finite())
            || start == end
        {
            continue;
        }
        let color = |value: f64| {
            fill.colors[0].lerp_to_gamma(
                fill.colors[1],
                ((value - start) / (end - start)).clamp(0.0, 1.0) as f32,
            )
        };
        for bar in bars.start..bars.end.saturating_sub(1) {
            let Some((a, b)) = study
                .plot_value(fill.plot, bar)
                .zip(study.plot_value(fill.plot, bar + 1))
            else {
                continue;
            };
            if !a.is_finite() || !b.is_finite() {
                continue;
            }
            let (x, next_x) = (
                f64::from(coords.idx_to_x(bar)),
                f64::from(coords.idx_to_x(bar + 1)),
            );
            let base = fill.baseline;
            // Split a baseline crossing before clipping: a crossing quad is not convex.
            let polygons = if (a < base && b > base) || (a > base && b < base) {
                let cross = x + (base - a) / (b - a) * (next_x - x);
                vec![
                    vec![[x, a], [cross, base], [x, base]],
                    vec![[cross, base], [next_x, b], [next_x, base]],
                ]
            } else {
                vec![vec![[x, a], [next_x, b], [next_x, base], [x, base]]]
            };
            for side in polygons {
                for (low, high) in [
                    (None, Some(start.min(end))),
                    (Some(start.min(end)), Some(start.max(end))),
                    (Some(start.max(end)), None),
                ] {
                    let mut points = side.clone();
                    if let Some(low) = low {
                        points = clip(&points, low, true);
                    }
                    if let Some(high) = high {
                        points = clip(&points, high, false);
                    }
                    let mut mesh = egui::Mesh::default();
                    for [x, value] in points {
                        mesh.colored_vertex(
                            Pos2::new(x as f32, map.price_to_y(value)),
                            color(value),
                        );
                    }
                    if mesh.vertices.len() < 3
                        || !mesh.calc_bounds().is_positive()
                        || mesh.vertices.iter().any(|v| !v.pos.is_finite())
                        || mesh
                            .vertices
                            .iter()
                            .all(|v| v.color == Color32::TRANSPARENT)
                    {
                        continue;
                    }
                    for index in 2..mesh.vertices.len() as u32 {
                        mesh.add_triangle(0, index - 1, index);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                }
            }
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::chart::rendering::{IndicatorRenderer, RenderContext};
    use crate::scales::{PriceRange, PriceTransform};
    use crate::studies::{ConfiguredStudy, StochasticMomentumIndex};

    #[test]
    fn smi_gradients_split_crossings_keep_colours_and_skip_gaps() {
        let bars: Vec<_> = [110.0, 90.0, f64::NAN, 110.0]
            .into_iter()
            .map(|close| {
                crate::model::Bar::new(chrono::DateTime::UNIX_EPOCH, 100.0, 110.0, 90.0, close, 1.0)
            })
            .collect();
        let mut study = ConfiguredStudy::new(Box::new(StochasticMomentumIndex::new(1, 1, 1, 1)));
        study.calculate(&bars);
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(240.0, 200.0));
        for inverted in [false, true] {
            for visible in [false, true] {
                let mut settings = serde_json::to_value(study.settings()).unwrap();
                for plot in settings["plots"].as_array_mut().unwrap() {
                    plot["visible"] = false.into();
                }
                for fill in settings["gradients"].as_array_mut().into_iter().flatten() {
                    fill["visible"] = visible.into();
                }
                study
                    .apply_settings(&[], serde_json::from_value(settings).unwrap())
                    .unwrap();
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(-100.0, 100.0),
                    0.0,
                    200.0,
                    inverted,
                );
                let coords = ChartMapping::new(rect, 40.0, 0, 4, 0.0, map);
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    IndicatorRenderer::render_study(
                        &RenderContext::new(ui.painter(), rect),
                        &study,
                        &bars,
                        0,
                        &map,
                        &coords,
                    );
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                let meshes: Vec<_> = shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if !mesh.vertices.is_empty() => Some(mesh),
                        _ => None,
                    })
                    .collect();
                assert_eq!(meshes.len(), if visible { 2 } else { 0 });
                if visible {
                    let fit = crate::chart::rendering::fit_study_mapping(&study, &coords, None);
                    for value in [-100.0, 100.0] {
                        assert!(rect.y_range().contains(fit.price.price_to_y(value)));
                    }
                    for (mesh, sign) in meshes.iter().zip([1.0, -1.0]) {
                        let bounds = mesh.calc_bounds();
                        // 100 to -100 crosses +40 at 30% and -40 at 70% of one bar pitch.
                        assert!((bounds.width() - 12.0).abs() < 1e-4);
                        assert!((bounds.height() - 60.0).abs() < 1e-4);
                        assert!(mesh.vertices.iter().all(|v| v.pos.x <= coords.idx_to_x(1)));
                        let outer_y = map.price_to_y(sign * 100.0);
                        let inner_y = map.price_to_y(sign * 40.0);
                        assert!(
                            mesh.vertices
                                .iter()
                                .any(|v| (v.pos.y - outer_y).abs() < 1e-4 && v.color.a() == 128)
                        );
                        assert!(
                            mesh.vertices
                                .iter()
                                .any(|v| (v.pos.y - inner_y).abs() < 1e-4 && v.color.a() == 0)
                        );
                    }
                }
            }
        }
    }
}
