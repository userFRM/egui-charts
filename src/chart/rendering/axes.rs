//! Price and time axis rendering

use crate::chart::rendering::draw_dashed_line;
use crate::model::PriceLine;
use crate::scales::PriceMap;
use crate::theme::{ThemeExt, contrast};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Gap between the plot's right edge and the start of the price axis labels.
pub(crate) const PRICE_LABEL_GAP: f32 = 8.0;

/// Gap between the plot's bottom edge and the top of the time axis labels.
pub(crate) const TIME_LABEL_GAP: f32 = 5.0;

/// Padding between an axis tag's text and its edge.
const TAG_PADDING: Vec2 = Vec2::new(4.0, 2.0);

/// The font of every axis label and tag: the theme's label text.
pub(crate) fn axis_font(ctx: &egui::Context) -> FontId {
    FontId::proportional(ctx.active_theme().text.label)
}

/// Paints `text` on a `fill` tag in the axis font and returns the tag's rect.
///
/// The text sits where an axis label placed at `anchor` with `align` would, and the tag pads it.
/// A tag that would end past `max_x` moves left to end there, and one that overlaps `covered`, a
/// tag painted before it, widens to hide it across its width.
pub(crate) fn paint_axis_tag(
    painter: &Painter,
    anchor: Pos2,
    align: Align2,
    max_x: f32,
    covered: Option<Rect>,
    text: String,
    text_color: Color32,
    fill: Color32,
) -> Rect {
    let theme = painter.ctx().active_theme();
    let font = axis_font(painter.ctx());
    let galley = painter.layout_no_wrap(text, font, text_color);
    let mut tag = align
        .anchor_size(anchor, galley.size())
        .expand2(TAG_PADDING);
    tag = tag.translate(Vec2::new((max_x - tag.max.x).min(0.0), 0.0));
    let text_pos = tag.min + TAG_PADDING;
    if let Some(covered) = covered.filter(|covered| covered.intersects(tag)) {
        tag.min.x = tag.min.x.min(covered.min.x);
        tag.max.x = tag.max.x.max(covered.max.x);
    }
    painter.rect_filled(tag, theme.radius.xs, fill);
    painter.galley(text_pos, galley, text_color);
    tag
}

/// Renders `line` from `start_x`, clamped to the plot's left edge, to its right edge and, with
/// `show_axis` and the line's `axis_label`, its tag in the price axis, which ends at `axis_right`.
/// Returns the tag's rect, or `None` when no tag is drawn.
pub fn render_price_line(
    painter: &Painter,
    price_rect: Rect,
    price_map: &PriceMap,
    line: &PriceLine,
    start_x: f32,
    show_axis: bool,
    axis_right: f32,
) -> Option<Rect> {
    let x0 = start_x.max(price_rect.min.x);
    if x0 >= price_rect.max.x {
        return None;
    }
    let transform = price_map.transform();
    // In scale units: a flat range maps every price, NaN included, to the pane's middle.
    if !price_map.range().contains(transform.to_scale(line.price)) {
        return None;
    }
    let y = price_map.price_to_y(line.price);

    let start = Pos2::new(x0, y);
    let end = Pos2::new(price_rect.max.x, y);
    let stroke = Stroke::new(line.width, line.color);
    if let Some((dash, gap)) = line.style.dash_pattern() {
        draw_dashed_line(painter, start, end, stroke, dash, gap);
    } else {
        painter.line_segment([start, end], stroke);
    }

    if let Some(title) = &line.title {
        painter.with_clip_rect(price_rect).text(
            start,
            Align2::LEFT_BOTTOM,
            title,
            axis_font(painter.ctx()),
            line.color,
        );
    }

    if !show_axis || !line.axis_label {
        return None;
    }
    let label = transform.label(transform.to_scale(line.price), 2);
    // Black or white, whichever reads better on the line colour.
    let text_color = if contrast(Color32::WHITE, line.color) >= contrast(Color32::BLACK, line.color)
    {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    Some(paint_axis_tag(
        painter,
        Pos2::new(price_rect.max.x + PRICE_LABEL_GAP, y),
        Align2::LEFT_CENTER,
        axis_right,
        None,
        label,
        text_color,
        line.color,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceScaleMode, PriceTransform};
    use egui::{Context, RawInput, Shape};

    #[test]
    fn price_line_follows_the_scale() {
        use PriceScaleMode::{Logarithmic, Normal, Percentage};

        // ln(100) is midway between ln(10) and ln(1000); +25% is 3/4 up -50%..50%.
        let cases = [
            (
                "logarithmic",
                Logarithmic,
                0.0,
                (10_f64.ln(), 1000_f64.ln()),
                100.0,
                true,
                Some((50.0, Some("100.00"))),
            ),
            (
                "percentage",
                Percentage,
                100.0,
                (-50.0, 50.0),
                125.0,
                true,
                Some((25.0, Some("+25.00%"))),
            ),
            (
                "off the scale",
                Normal,
                0.0,
                (100.0, 200.0),
                250.0,
                true,
                None,
            ),
            (
                "not finite",
                Normal,
                0.0,
                (150.0, 150.0),
                f64::NAN,
                true,
                None,
            ),
            ("flat range", Normal, 0.0, (150.0, 150.0), 250.0, true, None),
            (
                "hidden label",
                Normal,
                0.0,
                (100.0, 200.0),
                150.0,
                false,
                Some((50.0, None)),
            ),
        ];
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let mut failures = Vec::new();
        for (name, mode, base, (min, max), price, axis_label, expected) in cases {
            let ctx = Context::default();
            let map = PriceMap::new(
                PriceTransform { mode, base },
                PriceRange::new(min, max),
                0.0,
                100.0,
                false,
            );
            let line = PriceLine::new(price, Color32::RED).with_axis_label(axis_label);
            let output = ctx.run_ui(RawInput::default(), |ui| {
                render_price_line(ui.painter(), pane, &map, &line, pane.min.x, true, 200.0);
            });
            let segments: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let Shape::LineSegment { points, .. } = &shape.shape {
                        Some(*points)
                    } else {
                        None
                    }
                })
                .collect();
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let Shape::Text(text) = &shape.shape {
                        Some(text.galley.text().to_owned())
                    } else {
                        None
                    }
                })
                .collect();
            let matches = match expected {
                None => output.shapes.is_empty(),
                Some((y, label)) => {
                    !segments.is_empty()
                        && segments.iter().flatten().all(|p| (p.y - y).abs() < 1e-3)
                        && labels.iter().map(String::as_str).eq(label)
                }
            };
            if !matches {
                failures.push(format!("{name}: expected {expected:?}, segments {segments:?}, labels {labels:?}, {} shapes", output.shapes.len()));
            }
            output.drop_without_applying_deltas();
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
