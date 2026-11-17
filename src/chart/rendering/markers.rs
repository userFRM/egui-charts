//! Marker renderer for chart annotations

use super::{ChartMapping, RenderContext};
use crate::model::{Bar, Marker, MarkerPos, MarkerShape};
use crate::scales::PriceMap;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, epaint::StrokeKind};

/// Render markers on the chart. Times between bars interpolate; whitespace extends the
/// most common positive spacing among the outermost 16 bars (shorter spacing breaks ties,
/// one second if none). Vertical placement uses the nearest bar.
/// `markers` come numbered by the caller, which names each tooltip. Returns each marker drawn
/// with its visible hit area, in painting order.
pub fn render_markers<'a>(
    context: &RenderContext,
    markers: impl IntoIterator<Item = (usize, &'a Marker)>,
    visible_data: &[Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
) -> Vec<(&'a Marker, Rect)> {
    let mut hits = Vec::new();
    let mut tooltip_shown = false;
    let theme = context.painter.ctx().active_theme().chart;
    let background = theme.bg;
    for (marker_index, marker) in markers {
        if !marker.size.is_finite() || marker.size <= 0.0 {
            continue;
        }
        if let Some(index) = crate::model::index_at(visible_data, marker.time) {
            let bar_idx = index.round().clamp(0.0, (visible_data.len() - 1) as f64) as usize;
            let bar = &visible_data[bar_idx];
            let x = coords.index_to_x(coords.start_idx as f64 + index);
            if x < context.rect.left() || x > context.rect.right() {
                continue;
            }

            let y = match marker_anchor(marker, bar, price_scale) {
                Some((price, offset)) => price_scale.price_to_y(price) + offset,
                None if marker.position == MarkerPos::Top => {
                    context.rect.top() + 20.0 * marker.size
                }
                None => context.rect.bottom() - 20.0 * marker.size,
            };
            if !y.is_finite() {
                continue;
            }

            let pos = Pos2::new(x, y);
            let color = themed(marker.color, theme.axis_text);

            // Render the marker shape
            render_marker_shape(context.painter, pos, marker.shape, color, marker.size);

            let mut hit_rect = Rect::from_center_size(pos, Vec2::splat(24.0 * marker.size));
            let ctx = context.painter.ctx();
            // Render text label if present
            if let Some(text) = &marker.text {
                let galley = context.painter.layout_no_wrap(
                    text.clone(),
                    FontId::proportional(10.0 * marker.size),
                    color,
                );
                let rect = egui::Align2::CENTER_TOP
                    .anchor_size(Pos2::new(pos.x, pos.y + 12.0 * marker.size), galley.size());
                // The chart's background keeps the text readable over the bars behind it.
                context
                    .painter
                    .rect_filled(rect.expand(1.0), 2.0, background);
                context.painter.galley(rect.min, galley, color);
                hit_rect = hit_rect.union(rect);
            }

            let hit_rect = hit_rect
                .intersect(context.rect)
                .intersect(context.painter.clip_rect());
            if hit_rect.is_positive() {
                hits.push((marker, hit_rect));
            }
            if !tooltip_shown
                && let Some(tooltip) = &marker.tooltip
                && ctx.pointer_hover_pos().is_some_and(|p| {
                    hit_rect.contains(p) && ctx.layer_id_at(p) == Some(context.painter.layer_id())
                })
            {
                egui::Tooltip::always_open(
                    ctx.clone(),
                    context.painter.layer_id(),
                    egui::Id::unique((
                        "bar-mark",
                        context.rect.min.x.to_bits(),
                        context.rect.min.y.to_bits(),
                        marker_index,
                    )),
                    hit_rect,
                )
                .show(|ui| {
                    ui.label(tooltip);
                });
                tooltip_shown = true;
            }
        }
    }
    hits
}

/// The price anchor and screen offset shared by painting and fitting. Pane-edge positions
/// do not move with the price range.
fn marker_anchor(marker: &Marker, bar: &Bar, map: &PriceMap) -> Option<(f64, f32)> {
    // A flat range puts both ends at the same y, so choose the anchor in scale units.
    let (high, low) = (
        map.transform().to_scale(bar.high),
        map.transform().to_scale(bar.low),
    );
    let reversed = (map.extent().0 > map.extent().1) ^ (map.range().min > map.range().max);
    let (top, bottom) = if map.is_inverted() ^ reversed {
        (high.min(low), high.max(low))
    } else {
        (high.max(low), high.min(low))
    };
    let price = |value| if value == high { bar.high } else { bar.low };
    let (top, bottom) = (price(top), price(bottom));
    let (price, offset) = match marker.position {
        MarkerPos::AboveBar => (top, -20.0),
        MarkerPos::BelowBar => (bottom, 20.0),
        MarkerPos::AbsoluteUp => (top, -10.0),
        MarkerPos::AbsoluteDown => (bottom, 10.0),
        // Offset by the shape's half size, 8 points at size 1.
        MarkerPos::AtPriceTop => (marker.price.unwrap_or(bar.close), -8.0),
        MarkerPos::AtPriceBottom => (marker.price.unwrap_or(bar.close), 8.0),
        MarkerPos::AtPriceMiddle => (marker.price.unwrap_or(bar.close), 0.0),
        MarkerPos::InBar | MarkerPos::Left | MarkerPos::Right | MarkerPos::Absolute => {
            (bar.close, 0.0)
        }
        MarkerPos::Top | MarkerPos::Bottom => return None,
    };
    Some((price, offset * marker.size))
}

/// A marker's price anchor and its painted extent above and below it, in points, including
/// strokes and the text background. Text uses the same font layout as the renderer.
pub(crate) fn marker_bounds(
    painter: &Painter,
    marker: &Marker,
    bar: &Bar,
    map: &PriceMap,
) -> Option<(f64, f32, f32)> {
    if !marker.size.is_finite() || marker.size <= 0.0 {
        return None;
    }
    let (price, offset) = marker_anchor(marker, bar, map)?;
    let base = 8.0 * marker.size;
    let (top, bottom) = match marker.shape {
        MarkerShape::ArrowUp | MarkerShape::TriangleUp => (-base * 1.2, base * 0.6),
        MarkerShape::ArrowDown | MarkerShape::TriangleDown => (-base * 0.6, base * 1.2),
        MarkerShape::Cross => (-base * 1.2, base * 1.2),
        MarkerShape::Flag => (-base * 1.2, base * 2.0),
        _ => (-base, base),
    };
    // Allow the outside square outline, polygon joins and antialiasing at the pane edge.
    let stroke = DESIGN_TOKENS.stroke.thick;
    let (mut top, mut bottom) = (top - stroke, bottom + stroke);
    if let Some(text) = &marker.text {
        let galley = painter.layout_no_wrap(
            text.clone(),
            FontId::proportional(10.0 * marker.size),
            Color32::WHITE,
        );
        let y = 12.0 * marker.size;
        top = top.min(y + galley.mesh_bounds.top().min(-1.0));
        bottom = bottom.max(y + galley.mesh_bounds.bottom().max(galley.size().y + 1.0));
    }
    Some((price, -offset - top, offset + bottom))
}

/// A marker colour with `PLACEHOLDER` resolved to the theme's text colour.
fn themed(color: Color32, text: Color32) -> Color32 {
    if color == Color32::PLACEHOLDER {
        text
    } else {
        color
    }
}

/// Draws a change mark at `pos`: a dot with a chevron above it for a rise or below it for a
/// fall.
pub(crate) fn render_change_mark(painter: &Painter, pos: Pos2, up: bool, color: Color32) {
    painter.circle_filled(pos, 4.0, color);
    let sign = if up { 1.0 } else { -1.0 };
    let y = pos.y - 7.0 * sign;
    painter.add(Shape::line(
        vec![
            Pos2::new(pos.x - 4.7, y),
            Pos2::new(pos.x, y - 3.5 * sign),
            Pos2::new(pos.x + 4.7, y),
        ],
        Stroke::new(2.0, color),
    ));
}

/// Draws a dashed vertical rule across `coords` at each time, placed as [`render_event_band`]
/// places its badges, in the badge colour.
pub(crate) fn render_event_breaks(
    ui: &egui::Ui,
    times: &[chrono::DateTime<chrono::Utc>],
    visible_data: &[Bar],
    coords: &ChartMapping,
) {
    let painter = ui.painter().with_clip_rect(coords.rect);
    let stroke = Stroke::new(1.0, ui.ctx().active_theme().ui.info);
    for &time in times {
        let Some(at) = crate::model::index_at(visible_data, time) else {
            continue;
        };
        let x = coords.index_to_x(coords.start_idx as f64 + at.ceil());
        painter.extend(Shape::dashed_line(
            &[
                Pos2::new(x, coords.rect.top()),
                Pos2::new(x, coords.rect.bottom()),
            ],
            stroke,
            4.0,
            4.0,
        ));
    }
}

/// Draws host-supplied event badges just above `baseline`, using the chart's time mapping.
/// An event between bars uses the first bar at or after its time; whitespace extrapolates the
/// most common positive spacing among the outermost 16 bars. Colliding badges stack upward,
/// 2 points apart.
/// Returns the clicked event ID; callers handle that action before starting a chart drag.
/// Each badge outlines the event's shape in its colour on the chart background, 22 points tall
/// at size 1 and scaled by its size, with its text inside; the arrow and triangle of one
/// direction draw the same outline. Events whose size is not a positive finite number are left
/// out. The badges use the event's tooltip without assigning any market meaning to them.
pub fn render_event_band(
    ui: &mut egui::Ui,
    events: &[Marker],
    visible_data: &[Bar],
    coords: &ChartMapping,
    baseline: f32,
) -> Option<String> {
    render_event_badges(
        ui,
        events.iter().enumerate(),
        visible_data,
        coords,
        baseline,
    )
}

/// [`render_event_band`] for events with their own keys, which name the badges' interactions.
pub(crate) fn render_event_badges<'a>(
    ui: &mut egui::Ui,
    events: impl IntoIterator<Item = (usize, &'a Marker)>,
    visible_data: &[Bar],
    coords: &ChartMapping,
    baseline: f32,
) -> Option<String> {
    let mut clicked = None;
    visible_data.first()?;
    let mut placed = Vec::<Rect>::new();
    let theme = ui.ctx().active_theme().chart;
    for (index, event) in events {
        let at = crate::model::index_at(visible_data, event.time)?.ceil();
        if !event.size.is_finite() || event.size <= 0.0 {
            continue;
        }
        let x = coords.index_to_x(coords.start_idx as f64 + at);
        let half = 11.0 * event.size;
        let mut rect =
            Rect::from_center_size(Pos2::new(x, baseline - half), Vec2::splat(2.0 * half));
        // Each badge sits 2 points above every badge it would overlap, whatever their sizes.
        while let Some(other) = placed.iter().find(|other| other.intersects(rect)) {
            rect = rect.translate(Vec2::new(0.0, other.top() - 2.0 - rect.bottom()));
        }
        if rect.left() < coords.rect.left()
            || rect.right() > coords.rect.right()
            || rect.top() < coords.rect.top()
        {
            continue;
        }
        placed.push(rect);
        let response = ui.interact(
            rect,
            ui.scope_id().with(("event", &event.id, index)),
            egui::Sense::click(),
        );
        let color = themed(event.color, theme.axis_text);
        // The outline's stroke and sharp joins reach past the box.
        let painter = ui.painter().with_clip_rect(rect.expand(1.0));
        let text_at = paint_badge(
            &painter,
            event.shape,
            rect.center(),
            10.0 * event.size,
            theme.bg,
            Stroke::new(1.2, color),
        );
        if let Some(text) = &event.text {
            painter.text(
                text_at,
                egui::Align2::CENTER_CENTER,
                text,
                FontId::proportional(11.0 * event.size),
                color,
            );
        }
        let label: std::borrow::Cow<'_, str> = event
            .tooltip
            .as_deref()
            .or(event.text.as_deref())
            .map_or_else(
                || crate::localization::text(ui.ctx(), "Event").into(),
                Into::into,
            );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &*label));
        if response.on_hover_text(&*label).clicked() {
            clicked.clone_from(&event.id);
        }
    }
    clicked
}

/// Paints a time axis badge of `shape` within `r` of `c`, filled with `fill` and outlined with
/// `stroke`, and returns where its text centres. The arrow and triangle of one direction draw
/// the same outline.
fn paint_badge(
    painter: &Painter,
    shape: MarkerShape,
    c: Pos2,
    r: f32,
    fill: Color32,
    stroke: Stroke,
) -> Pos2 {
    let at = |x: f32, y: f32| c + Vec2::new(x * r, y * r);
    let polygon = |points| painter.add(Shape::convex_polygon(points, fill, stroke));
    match shape {
        MarkerShape::Circle => {
            painter.circle(c, r, fill, stroke);
        }
        MarkerShape::Square => {
            painter.rect(
                Rect::from_center_size(c, Vec2::splat(2.0 * r)),
                0.0,
                fill,
                stroke,
                StrokeKind::Middle,
            );
        }
        MarkerShape::Diamond => {
            polygon(vec![
                at(0.0, -1.0),
                at(1.0, 0.0),
                at(0.0, 1.0),
                at(-1.0, 0.0),
            ]);
        }
        MarkerShape::ArrowUp | MarkerShape::TriangleUp => {
            polygon(vec![at(0.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]);
            // The triangle's centroid.
            return at(0.0, 1.0 / 3.0);
        }
        MarkerShape::ArrowDown | MarkerShape::TriangleDown => {
            polygon(vec![at(0.0, 1.0), at(-1.0, -1.0), at(1.0, -1.0)]);
            return at(0.0, -1.0 / 3.0);
        }
        MarkerShape::Star => paint_star(painter, c, r, fill, stroke),
        MarkerShape::Cross => {
            painter.line_segment([at(-1.0, -1.0), at(1.0, 1.0)], stroke);
            painter.line_segment([at(-1.0, 1.0), at(1.0, -1.0)], stroke);
        }
        MarkerShape::Flag => {
            painter.line_segment([at(-0.6, -1.0), at(-0.6, 1.0)], stroke);
            polygon(vec![at(-0.6, -1.0), at(1.0, -0.2), at(-0.6, 0.6)]);
            // The pennant's centroid.
            return at(-0.2 / 3.0, -0.2);
        }
    }
    c
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
    let outline = painter.ctx().active_theme().chart.crosshair_label_text;

    match shape {
        MarkerShape::Circle => {
            painter.circle_filled(pos, base_size, color);
            painter.circle_stroke(
                pos,
                base_size,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
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
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
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
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
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
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
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
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
            ));
        }
        MarkerShape::Star => paint_star(
            painter,
            pos,
            base_size,
            color,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
        ),
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
                Stroke::new(DESIGN_TOKENS.stroke.hairline, outline),
            ));
        }
    }
}

/// Fills a four-pointed star around `centre` with its points `outer` away and outlines it.
/// `Shape::convex_polygon` fans its fill from the first point, which would also fill the
/// notches between the points.
fn paint_star(painter: &Painter, centre: Pos2, outer: f32, fill: Color32, stroke: Stroke) {
    let inner = outer * 0.12;
    let points = vec![
        centre + Vec2::new(0.0, -outer),
        centre + Vec2::new(inner, -inner),
        centre + Vec2::new(outer, 0.0),
        centre + Vec2::new(inner, inner),
        centre + Vec2::new(0.0, outer),
        centre + Vec2::new(-inner, inner),
        centre + Vec2::new(-outer, 0.0),
        centre + Vec2::new(-inner, -inner),
    ];
    // Every point is visible from the centre, so a fan from it covers the star exactly.
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(centre, fill);
    for &point in &points {
        mesh.colored_vertex(point, fill);
    }
    for k in 0..8 {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % 8);
    }
    painter.add(mesh);
    painter.add(Shape::closed_line(points, stroke));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform};
    use chrono::{TimeZone, Utc};

    /// A bar from 95 to 105 on a 90..110 scale over 500 px spans y 125..375 in
    /// either orientation, so each marker lands at the same y: above positions
    /// offset up from 125, below positions down from 375. Price 100, not the close
    /// of 104, is at y 250, with a size-1 shape's 8 point half size above, below or across it.
    #[test]
    fn markers_sit_outside_the_bar_on_either_scale_orientation() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(160.0, 500.0));
        let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = [0, 2].map(|seconds| {
            Bar::new(
                t + chrono::Duration::seconds(seconds),
                100.0,
                105.0,
                95.0,
                104.0,
                1.0,
            )
        });
        for inverted in [false, true] {
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                0.0,
                500.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 10.0, 12, 13, 2.0, map);
            for (position, want) in [
                (MarkerPos::AboveBar, 105.0),
                (MarkerPos::BelowBar, 395.0),
                (MarkerPos::AbsoluteUp, 115.0),
                (MarkerPos::AbsoluteDown, 385.0),
                (MarkerPos::AtPriceTop, 242.0),
                (MarkerPos::AtPriceBottom, 258.0),
                (MarkerPos::AtPriceMiddle, 250.0),
            ] {
                for seconds in [-2, 0, 1, 3] {
                    let marker = Marker::new(t + chrono::Duration::seconds(seconds))
                        .with_tooltip("Host bar annotation")
                        .with_pos(position)
                        .with_shape(MarkerShape::Circle);
                    // Price 100 is the scale's middle in either orientation.
                    let marker = match position {
                        MarkerPos::AtPriceTop
                        | MarkerPos::AtPriceBottom
                        | MarkerPos::AtPriceMiddle => marker.at_price(100.0, position),
                        _ => marker,
                    };
                    let ctx = egui::Context::default();
                    let hover = Pos2::new(coords.idx_to_x(12) + seconds as f32 * 5.0, want);
                    let mut output = egui::FullOutput::default();
                    for _ in 0..3 {
                        output = ctx.run_ui(
                            egui::RawInput {
                                events: vec![egui::Event::PointerMoved(hover)],
                                ..Default::default()
                            },
                            |ui| {
                                let painter = Painter::new(
                                    ui.ctx().clone(),
                                    egui::LayerId::background(),
                                    rect,
                                );
                                render_markers(
                                    &RenderContext::new(&painter, rect),
                                    [(0, &marker)],
                                    &bars,
                                    &map,
                                    &coords,
                                );
                            },
                        );
                        output.textures_delta.clear();
                    }
                    assert!(
                        output.shapes.iter().any(|shape| matches!(&shape.shape,
                        Shape::Text(text) if text.galley.text() == "Host bar annotation")),
                        "the marker tooltip must appear at its painted position"
                    );
                    let center = output.shapes.iter().find_map(|c| match &c.shape {
                        Shape::Circle(circle) => Some(circle.center),
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    let center = center.expect("a marker circle");
                    assert_eq!(center.x, coords.idx_to_x(12) + seconds as f32 * 5.0);
                    let y = center.y;
                    assert!(
                        (y - want).abs() < 1e-3,
                        "{position:?}, inverted {inverted}: y {y}, want {want}"
                    );
                }
            }
        }
    }

    /// A marker without a colour follows the theme's text colour, so it reads on the light
    /// chart; one with a colour keeps it.
    #[test]
    fn default_marker_colour_reads_on_the_light_theme() {
        let ctx = egui::Context::default();
        let light = crate::theme::Theme::light();
        crate::theme::apply_to_egui(&ctx, &light);
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(160.0, 500.0));
        let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = [0, 2].map(|seconds| {
            Bar::new(
                t + chrono::Duration::seconds(seconds),
                100.0,
                105.0,
                95.0,
                104.0,
                1.0,
            )
        });
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(90.0, 110.0),
            0.0,
            500.0,
            false,
        );
        let coords = ChartMapping::new(rect, 10.0, 12, 13, 2.0, map);
        let markers = [
            Marker::new(t).with_text("default"),
            Marker::new(t + chrono::Duration::seconds(2))
                .with_shape(MarkerShape::Circle)
                .with_color(Color32::RED)
                .with_text("explicit"),
            Marker::new(t + chrono::Duration::seconds(4)).with_color(Color32::WHITE),
        ];
        let markers: Vec<Marker> =
            serde_json::from_str(&serde_json::to_string(&markers).unwrap()).unwrap();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = Painter::new(ui.ctx().clone(), egui::LayerId::background(), rect);
            render_markers(
                &RenderContext::new(&painter, rect),
                markers.iter().enumerate(),
                &bars,
                &map,
                &coords,
            );
        });
        let fills: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Circle(circle) if circle.fill != Color32::TRANSPARENT => Some(circle.fill),
                _ => None,
            })
            .collect();
        let text_matches = [
            ("default", light.chart.axis_text),
            ("explicit", Color32::RED),
        ]
        .map(|(label, color)| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                Shape::Text(text) if text.galley.text() == label
                    && text.galley.job.sections.iter().all(|s| s.format.color == color))
            })
        });
        output.drop_without_applying_deltas();
        assert_eq!(fills.len(), 3);
        // 3:1 is the minimum contrast for graphical objects.
        assert!(crate::theme::contrast(fills[0], light.chart.bg) >= 3.0);
        assert_eq!(fills[1], Color32::RED);
        assert_eq!(fills[2], Color32::WHITE);
        assert_eq!(markers[0].color, Color32::PLACEHOLDER);
        assert!(text_matches.into_iter().all(|matches| matches));
    }

    /// A star at size 4 fills its centre but not the notch between its upper and right points,
    /// in the price pane (outer radius 32) and on the time axis (outer radius 40). A convex
    /// polygon fill of the outline covers that notch point.
    #[cfg(feature = "drawings")]
    #[test]
    fn star_marks_fill_only_their_points() {
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::light());
        let time = Utc.with_ymd_and_hms(2024, 10, 1, 14, 30, 0).unwrap();
        let bars: Vec<_> = (0..30)
            .map(|i| {
                Bar::new(
                    time + chrono::Duration::hours(i),
                    100.0,
                    105.0,
                    95.0,
                    104.0,
                    1.0,
                )
            })
            .collect();
        let star = |i: usize| {
            Marker::new(bars[i].time)
                .with_shape(MarkerShape::Star)
                .with_size(4.0)
        };
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(160.0, 500.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(90.0, 110.0),
            0.0,
            500.0,
            false,
        );
        let pane_coords = ChartMapping::new(pane, 10.0, 12, 13, 2.0, map);
        let axis_coords = ChartMapping::new(
            Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 300.0)),
            10.0,
            0,
            35,
            0.0,
            map,
        );
        let pane_star = [star(0).at_price(100.0, MarkerPos::AtPriceMiddle)];
        let axis_star = [star(20)];
        let rows = [
            (Pos2::new(pane_coords.index_to_x(12.0), 250.0), 32.0, true),
            (Pos2::new(344.0, 256.0), 40.0, false),
        ];
        for (centre, outer, in_pane) in rows {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                if in_pane {
                    let painter = Painter::new(ui.ctx().clone(), egui::LayerId::background(), pane);
                    render_markers(
                        &RenderContext::new(&painter, pane),
                        pane_star.iter().enumerate(),
                        &bars,
                        &map,
                        &pane_coords,
                    );
                } else {
                    render_event_band(ui, &axis_star, &bars, &axis_coords, 300.0);
                }
            });
            let painted = |dx: f32, dy: f32| {
                crate::drawings::services::SelectionService::intersects_painted(
                    &ctx,
                    &output.shapes,
                    Rect::from_center_size(centre + Vec2::new(dx, dy) * outer, Vec2::splat(1.0)),
                )
            };
            let probes = [painted(0.0, 0.0), painted(0.25, -0.4)];
            output.drop_without_applying_deltas();
            assert_eq!(probes, [true, false], "star at {centre:?}");
        }
    }

    /// x = 500 - (35 + offset - index + 0.5) * spacing - 1.
    /// Events at slots 11 and 30 keep distinct click targets, including a stacked duplicate.
    /// Slot 20 holds a square at size 1.5, a diamond, an up arrow at size 0.5 and a down arrow,
    /// each drawn in its shape and size and stacked 2 points above the badges it would overlap.
    #[test]
    fn event_band_follows_time_mapping_and_returns_the_clicked_id() {
        use egui::{Context, Event, Modifiers, PointerButton, RawInput, epaint::ColorMode};
        let ctx = Context::default();
        let light = crate::theme::Theme::light();
        crate::theme::apply_to_egui(&ctx, &light);
        let time = Utc.with_ymd_and_hms(2024, 10, 1, 14, 30, 0).unwrap();
        let bars: Vec<_> = (0..30)
            .map(|i| {
                Bar::new(
                    time + chrono::Duration::hours(i),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    1.0,
                )
            })
            .collect();
        let event = Marker::new(bars[10].time + chrono::Duration::minutes(15))
            .with_text("E")
            .with_id("event-42");
        let shaped = |shape, size, id| {
            Marker::new(bars[20].time)
                .with_shape(shape)
                .with_size(size)
                .with_id(id)
        };
        let events = [
            event.clone(),
            event.with_id("event-43").with_color(Color32::RED),
            Marker::event(bars[29].time + chrono::Duration::hours(1), "N").with_id("future"),
            shaped(MarkerShape::Square, 1.5, "square"),
            shaped(MarkerShape::Diamond, 1.0, "diamond"),
            shaped(MarkerShape::ArrowUp, 0.5, "up"),
            shaped(MarkerShape::ArrowDown, 1.0, "down"),
        ];
        let (axis, info) = (light.chart.axis_text, events[2].color);
        for (spacing, offset, x) in [
            (10.0, 0.0, [254.0, 444.0, 344.0]),
            (15.0, 2.0, [101.5, 386.5, 236.5]),
        ] {
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 300.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                0.0,
                200.0,
                false,
            );
            let coords = ChartMapping::new(rect, spacing, 0, 35, offset, map);
            let p = |dx: f32, y: f32, slot: usize| Pos2::new(x[slot] + dx, y);
            // Per badge: its centre, its clip (the box grown by 1) and its outlines in paint
            // order, by kind, points and stroke colour.
            let expected = [
                (
                    p(0.0, 289.0, 0),
                    12.0,
                    vec![("circle", vec![p(-10.0, 279.0, 0), p(10.0, 299.0, 0)], axis)],
                ),
                (
                    p(0.0, 265.0, 0),
                    12.0,
                    vec![(
                        "circle",
                        vec![p(-10.0, 255.0, 0), p(10.0, 275.0, 0)],
                        Color32::RED,
                    )],
                ),
                (
                    p(0.0, 289.0, 1),
                    12.0,
                    vec![
                        ("line", vec![p(-6.0, 279.0, 1), p(-6.0, 299.0, 1)], info),
                        (
                            "path",
                            vec![p(-6.0, 279.0, 1), p(10.0, 287.0, 1), p(-6.0, 295.0, 1)],
                            info,
                        ),
                    ],
                ),
                (
                    p(0.0, 283.5, 2),
                    17.5,
                    vec![("rect", vec![p(-15.0, 268.5, 2), p(15.0, 298.5, 2)], axis)],
                ),
                (
                    p(0.0, 254.0, 2),
                    12.0,
                    vec![(
                        "path",
                        vec![
                            p(0.0, 244.0, 2),
                            p(10.0, 254.0, 2),
                            p(0.0, 264.0, 2),
                            p(-10.0, 254.0, 2),
                        ],
                        axis,
                    )],
                ),
                (
                    p(0.0, 235.5, 2),
                    6.5,
                    vec![(
                        "path",
                        vec![p(0.0, 230.5, 2), p(5.0, 240.5, 2), p(-5.0, 240.5, 2)],
                        axis,
                    )],
                ),
                (
                    p(0.0, 217.0, 2),
                    12.0,
                    vec![(
                        "path",
                        vec![p(0.0, 227.0, 2), p(-10.0, 207.0, 2), p(10.0, 207.0, 2)],
                        axis,
                    )],
                ),
            ];
            let near = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;
            for ((centre, ..), event) in expected.iter().zip(&events) {
                let centre = *centre;
                let mut clicked = None;
                for pressed in [None, Some(true), Some(false)] {
                    let mut input = RawInput::default();
                    input.events.push(Event::PointerMoved(centre));
                    if let Some(pressed) = pressed {
                        input.events.push(Event::PointerButton {
                            pos: centre,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    let output = ctx.run_ui(input, |ui| {
                        clicked = render_event_band(ui, &events, &bars, &coords, 300.0)
                            .or(clicked.take());
                    });
                    // Asserting before the output is dropped would abort the test binary.
                    let mut mismatches = Vec::new();
                    for (at, half, outlines) in &expected {
                        let clip = Rect::from_center_size(*at, Vec2::splat(2.0 * half));
                        let drawn: Vec<_> = output
                            .shapes
                            .iter()
                            .filter(|shape| {
                                near(shape.clip_rect.min, clip.min)
                                    && near(shape.clip_rect.max, clip.max)
                            })
                            .filter_map(|shape| match &shape.shape {
                                Shape::Circle(circle) => Some((
                                    "circle",
                                    vec![
                                        circle.center - Vec2::splat(circle.radius),
                                        circle.center + Vec2::splat(circle.radius),
                                    ],
                                    circle.stroke.color,
                                )),
                                Shape::Rect(r) => {
                                    Some(("rect", vec![r.rect.min, r.rect.max], r.stroke.color))
                                }
                                Shape::Path(path) => match path.stroke.color {
                                    ColorMode::Solid(color) => {
                                        Some(("path", path.points.clone(), color))
                                    }
                                    ColorMode::UV(_) => None,
                                },
                                Shape::LineSegment { points, stroke } => {
                                    Some(("line", points.to_vec(), stroke.color))
                                }
                                _ => None,
                            })
                            .collect();
                        let matches = drawn.len() == outlines.len()
                            && drawn.iter().zip(outlines).all(|(d, e)| {
                                d.0 == e.0
                                    && d.2 == e.2
                                    && d.1.len() == e.1.len()
                                    && d.1.iter().zip(&e.1).all(|(a, b)| near(*a, *b))
                            });
                        if !matches {
                            mismatches.push(format!("badge at {at:?}: {drawn:?}"));
                        }
                    }
                    output.drop_without_applying_deltas();
                    assert!(mismatches.is_empty(), "spacing {spacing}: {mismatches:?}");
                }
                assert_eq!(clicked.as_deref(), event.id.as_deref());
            }
        }
    }
}
