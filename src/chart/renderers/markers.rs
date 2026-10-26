//! Marker renderer for chart annotations

use super::context::{ChartMapping, RenderContext};
use crate::model::{Bar, Marker, MarkerPos, MarkerShape};
use crate::scales::PriceMap;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, epaint::StrokeKind};

/// Render markers on the chart. Times between bars interpolate; whitespace extends the
/// nearest distinct pair (one second for a single bar). Vertical placement uses the nearest bar.
pub fn render_markers(
    context: &RenderContext,
    markers: &[Marker],
    visible_data: &[Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
) {
    let price_y =
        |marker: &Marker, bar: &Bar| price_scale.price_to_y(marker.price.unwrap_or(bar.close));
    let mut tooltip_shown = false;
    let background = context.painter.ctx().active_theme().chart.bg;
    for (marker_index, marker) in markers.iter().enumerate() {
        if !marker.size.is_finite() || marker.size <= 0.0 {
            continue;
        }
        if let Some(index) = crate::widget::sync::index_at(visible_data, marker.time) {
            let bar_idx = index.round().clamp(0.0, (visible_data.len() - 1) as f64) as usize;
            let bar = &visible_data[bar_idx];
            let x = coords.idx_to_x(coords.start_idx) + index as f32 * coords.bar_spacing;
            if x < context.rect.left() || x > context.rect.right() {
                continue;
            }

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
                // Offset by the shape's half size, 8 points at size 1.
                MarkerPos::AtPriceTop => price_y(marker, bar) - 8.0 * marker.size,
                MarkerPos::AtPriceBottom => price_y(marker, bar) + 8.0 * marker.size,
                MarkerPos::AtPriceMiddle => price_y(marker, bar),
            };
            if !y.is_finite() {
                continue;
            }

            let pos = Pos2::new(x, y);

            // Render the marker shape
            render_marker_shape(
                context.painter,
                pos,
                marker.shape,
                marker.color,
                marker.size,
            );

            let mut hit_rect = Rect::from_center_size(pos, Vec2::splat(24.0 * marker.size));
            let ctx = context.painter.ctx();
            // Render text label if present
            if let Some(text) = &marker.text {
                let galley = context.painter.layout_no_wrap(
                    text.clone(),
                    FontId::proportional(10.0 * marker.size),
                    marker.color,
                );
                let rect = egui::Align2::CENTER_TOP
                    .anchor_size(Pos2::new(pos.x, pos.y + 12.0 * marker.size), galley.size());
                // The chart's background keeps the text readable over the bars behind it.
                context
                    .painter
                    .rect_filled(rect.expand(1.0), 2.0, background);
                context.painter.galley(rect.min, galley, marker.color);
                hit_rect = hit_rect.union(rect);
            }

            let hit_rect = hit_rect
                .intersect(context.rect)
                .intersect(context.painter.clip_rect());
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

/// Draws host-supplied event badges just above `baseline`, using the chart's time mapping.
/// An event between bars uses the first bar at or after its time; whitespace extrapolates the
/// nearest bar interval. Colliding badges stack upward. Returns the clicked event ID;
/// callers handle that action before starting a chart drag. The badges use the event's colour,
/// text and tooltip, without assigning any market meaning to them.
pub fn render_event_band(
    ui: &mut egui::Ui,
    events: &[Marker],
    visible_data: &[Bar],
    coords: &ChartMapping,
    baseline: f32,
) -> Option<String> {
    let mut clicked = None;
    visible_data.first()?;
    let mut placed = Vec::<Rect>::new();
    for (index, event) in events.iter().enumerate() {
        let at = crate::widget::sync::index_at(visible_data, event.time)?.ceil();
        let x = coords.idx_to_x(coords.start_idx) + at as f32 * coords.bar_spacing;
        let mut rect = Rect::from_center_size(Pos2::new(x, baseline - 11.0), Vec2::splat(22.0));
        while placed.iter().any(|other| other.intersects(rect)) {
            rect = rect.translate(Vec2::new(0.0, -24.0));
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
        let painter = ui.painter().with_clip_rect(rect);
        painter.circle(
            rect.center(),
            10.0,
            ui.ctx().active_theme().chart.bg,
            Stroke::new(1.2, event.color),
        );
        if let Some(text) = &event.text {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                FontId::proportional(11.0),
                event.color,
            );
        }
        let label = event
            .tooltip
            .as_deref()
            .or(event.text.as_deref())
            .unwrap_or("Event");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        if response.on_hover_text(label).clicked() {
            clicked.clone_from(&event.id);
        }
    }
    clicked
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
                                    std::slice::from_ref(&marker),
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
    /// x = 500 - (35 + offset - index + 0.5) * spacing - 1.
    /// Events at slots 11 and 30 keep distinct click targets, including a stacked duplicate.
    #[test]
    fn event_band_follows_time_mapping_and_returns_the_clicked_id() {
        use egui::{Context, Event, Modifiers, PointerButton, RawInput};
        let ctx = Context::default();
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
        let event =
            Marker::event(bars[10].time + chrono::Duration::minutes(15), "E").with_id("event-42");
        let events = [
            event.clone(),
            event.with_id("event-43"),
            Marker::event(bars[29].time + chrono::Duration::hours(1), "N").with_id("future"),
        ];
        for (spacing, offset, x) in [(10.0, 0.0, [254.0, 444.0]), (15.0, 2.0, [101.5, 386.5])] {
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 300.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                0.0,
                200.0,
                false,
            );
            let coords = ChartMapping::new(rect, spacing, 0, 35, offset, map);
            let centres = [
                Pos2::new(x[0], 289.0),
                Pos2::new(x[0], 265.0),
                Pos2::new(x[1], 289.0),
            ];
            for (centre, event) in centres.into_iter().zip(&events) {
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
                    let circles: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Circle(circle) => Some(circle.center),
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    assert_eq!(circles, centres);
                }
                assert_eq!(clicked.as_deref(), event.id.as_deref());
            }
        }
    }
}
