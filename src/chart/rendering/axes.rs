//! Price and time axis rendering

use crate::chart::rendering::draw_dashed_line;
use crate::model::PriceLine;
use crate::scales::PriceMap;
use crate::theme::{ThemeExt, contrast};
use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Gap between the plot's right edge and the start of the price axis labels.
pub(crate) const PRICE_LABEL_GAP: f32 = 8.0;

/// Gap between the plot's bottom edge and the top of the time axis labels.
pub(crate) const TIME_LABEL_GAP: f32 = 5.0;

/// Padding between an axis tag's text and its edge.
const TAG_PADDING: Vec2 = Vec2::new(8.0, 0.5);

/// The font of every axis label and tag: the theme's label text.
pub(crate) fn axis_font(ctx: &egui::Context) -> FontId {
    FontId::proportional(ctx.active_theme().text.label)
}

/// Paints `text` on a `fill` tag in the axis font and returns the tag's rect.
///
/// The text sits where an axis label placed at `anchor` with `align` would, and the tag pads it.
/// A tag that would end past `max_x` moves left to end there, but stays inside the painter's left edge.
pub(crate) fn paint_axis_tag(
    painter: &Painter,
    anchor: Pos2,
    align: Align2,
    max_x: f32,
    text: String,
    text_color: Color32,
    fill: Color32,
) -> Rect {
    let max_x = if align == Align2::LEFT_CENTER {
        max_x - 8.0
    } else {
        max_x
    };
    let theme = painter.ctx().active_theme();
    let font = axis_font(painter.ctx());
    let galley = painter.layout_no_wrap(text, font, text_color);
    let mut tag = align
        .anchor_size(anchor, galley.size())
        .expand2(TAG_PADDING);
    tag = tag.translate(Vec2::new((max_x - tag.max.x).min(0.0), 0.0));
    tag = tag.translate(Vec2::new(
        (painter.clip_rect().left() - tag.min.x).max(0.0),
        0.0,
    ));
    let text_pos = tag.min + TAG_PADDING;
    painter.rect_filled(
        tag,
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: theme.radius.xs as u8,
            se: theme.radius.xs as u8,
        },
        fill,
    );
    painter.galley(text_pos, galley, text_color);
    tag
}

/// The axis ending at the plot's left edge uses right-aligned tags.
fn price_tag_anchor(plot: Rect, axis_right: f32, y: f32) -> (Pos2, Align2) {
    if axis_right <= plot.left() {
        (
            Pos2::new(plot.left() - 5.0 - TAG_PADDING.x, y),
            Align2::RIGHT_CENTER,
        )
    } else {
        (
            Pos2::new(plot.right() + 5.0 + TAG_PADDING.x, y),
            Align2::LEFT_CENTER,
        )
    }
}

/// Paints a later value beside the nearest free vertical slot, preserving earlier tags.
/// Tags that cannot fit inside the pane are omitted.
pub(crate) fn render_axis_value(
    painter: &Painter,
    plot: Rect,
    axis_right: f32,
    y: f32,
    text: String,
    colors: (Color32, Color32),
    occupied: &[Option<Rect>],
) -> Option<Rect> {
    let (text_color, fill) = colors;
    let height = painter
        .layout_no_wrap(text.clone(), axis_font(painter.ctx()), Color32::WHITE)
        .size()
        .y
        + 2.0 * TAG_PADDING.y;
    if !y.is_finite() || height > plot.height() {
        return None;
    }
    let clamp = |y: f32| y.clamp(plot.top() + height / 2.0, plot.bottom() - height / 2.0);
    let free = |y: f32| {
        occupied.iter().flatten().all(|rect| {
            y + height / 2.0 <= rect.top() + 0.001 || y - height / 2.0 >= rect.bottom() - 0.001
        })
    };
    let centre = std::iter::once(clamp(y))
        .chain(occupied.iter().flatten().flat_map(|rect| {
            [
                clamp(rect.top() - height / 2.0),
                clamp(rect.bottom() + height / 2.0),
            ]
        }))
        .filter(|y| free(*y))
        .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))?;
    let (anchor, align) = price_tag_anchor(plot, axis_right, centre);
    Some(paint_axis_tag(
        painter, anchor, align, axis_right, text, text_color, fill,
    ))
}

/// Last-bar values of visible study lines, in the same units and colours as their plot.
pub(crate) fn render_study_tags(
    painter: &Painter,
    plot: Rect,
    map: &PriceMap,
    study: &dyn crate::studies::Indicator,
    axis_right: f32,
    occupied: &mut Vec<Option<Rect>>,
) {
    if !study.is_visible() || study.scale() == crate::studies::StudyScale::None {
        return;
    }
    let values = study.values();
    let Some(bar) = values.len().checked_sub(1) else {
        return;
    };
    let transform = map.transform();
    for line in 0..study.line_cnt() {
        let plot_style = study.plot_style(line);
        let Some(value) = study.plot_value(line, bar) else {
            continue;
        };
        if !plot_style.visible
            || matches!(
                plot_style.plot_type,
                crate::studies::PlotType::BarColor | crate::studies::PlotType::BackgroundColor
            )
            || plot_style.width <= 0.0
            || !map.range().contains(transform.to_scale(value))
        {
            continue;
        }
        let label = if study.value_format() == crate::studies::StudyFormat::Price {
            transform.label(
                transform.to_scale(value),
                study
                    .precision()
                    .unwrap_or_else(|| map.price_precision().unwrap_or(2)),
            )
        } else {
            study.format_value(value)
        };
        let color = plot_style.color;
        occupied.push(render_axis_value(
            painter,
            plot,
            axis_right,
            map.price_to_y(value),
            label,
            (Color32::WHITE, color),
            occupied,
        ));
    }
}

/// Renders `line` from `start_x`, clamped to the plot's left edge, to its right edge and, with
/// `show_axis` and the line's `axis_label`, its tag in the price axis, which ends at `axis_right`.
/// Pass the plot's left edge as `axis_right` to place tags in the left axis.
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
    let pattern = match line.style {
        crate::model::LineStyle::Dashed => Some((4.0, 4.0)),
        crate::model::LineStyle::Dotted => Some((1.0, 3.0)),
        crate::model::LineStyle::Solid => None,
    };
    if line.width > 0.0 {
        if let Some((dash, gap)) = pattern {
            draw_dashed_line(painter, start, end, stroke, dash, gap);
        } else {
            painter.line_segment([start, end], stroke);
        }
    }
    if let Some(title) = &line.title {
        painter.with_clip_rect(price_rect).text(
            start,
            Align2::LEFT_BOTTOM,
            title,
            axis_font(painter.ctx()),
            line.title_color.unwrap_or(line.color),
        );
    }

    if !show_axis || !line.axis_label {
        return None;
    }
    let label = transform.label(
        transform.to_scale(line.price),
        price_map.price_precision().unwrap_or(2),
    );
    let (text_color, fill) = axis_label_colors(line);
    let (anchor, align) = price_tag_anchor(price_rect, axis_right, y);
    Some(paint_axis_tag(
        painter, anchor, align, axis_right, label, text_color, fill,
    ))
}

/// The text colour and fill of `line`'s axis tag.
pub(crate) fn axis_label_colors(line: &PriceLine) -> (Color32, Color32) {
    let fill = line.axis_label_color.unwrap_or(line.color);
    (
        line.axis_label_text_color
            .unwrap_or_else(|| tag_text_color(fill)),
        fill,
    )
}

// Keep white text on financial colours; pale custom fills still need dark text.
pub(crate) fn tag_text_color(fill: Color32) -> Color32 {
    if contrast(Color32::WHITE, fill) >= 2.0 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

/// Paints a compact hover tag within its axis bounds.
pub(crate) fn paint_crosshair_tag(
    painter: &Painter,
    anchor: Pos2,
    align: Align2,
    bounds: std::ops::RangeInclusive<f32>,
    text: String,
    colors: (Color32, Color32),
    height: f32,
    covered: Option<Rect>,
) -> Rect {
    let (text_color, fill) = colors;
    let padding = 8.0;
    let available = (bounds.end() - bounds.start()).max(0.0);
    let mut job =
        egui::text::LayoutJob::simple_singleline(text, axis_font(painter.ctx()), text_color);
    job.wrap = egui::text::TextWrapping::truncate_at_width((available - padding).max(0.0));
    let galley = painter.layout_job(job);
    let size = Vec2::new(
        (galley.size().x + padding).min(available),
        height.max(galley.size().y),
    );
    let mut tag = align.anchor_size(anchor, size);
    tag = tag.translate(Vec2::new((bounds.end() - tag.right()).min(0.0), 0.0));
    tag = tag.translate(Vec2::new((bounds.start() - tag.left()).max(0.0), 0.0));
    tag = tag.translate(Vec2::new(
        0.0,
        (painter.clip_rect().bottom() - tag.bottom()).min(0.0),
    ));
    tag = tag.translate(Vec2::new(
        0.0,
        (painter.clip_rect().top() - tag.top()).max(0.0),
    ));
    if let Some(covered) = covered.filter(|covered| covered.intersects(tag)) {
        let cover = covered.intersect(Rect::from_x_y_ranges(
            *bounds.start()..=*bounds.end(),
            painter.clip_rect().y_range(),
        ));
        painter.rect_filled(cover, 0.0, painter.ctx().active_theme().chart.bg);
    }
    tag.min.x = tag.min.x.max(*bounds.start());
    tag.max.x = tag.max.x.min(*bounds.end());
    let painter = painter.with_clip_rect(tag.intersect(painter.clip_rect()));
    painter.rect_filled(tag, 1.0, fill);
    let text_pos = Pos2::new(tag.left() + 4.0, tag.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, text_color);
    tag
}

/// Width of the current price labels and tags, including their plot gap and right clearance.
/// Include formatted session and volume labels in `tag_labels` when allocating their column.
pub fn price_axis_width(painter: &Painter, price_map: &PriceMap, tag_labels: &[&str]) -> f32 {
    crate::scales::PriceMarkGenerator::new()
        .generate_marks(price_map)
        .into_iter()
        .map(|mark| mark.label)
        .chain(tag_labels.iter().map(|label| (*label).to_owned()))
        .map(|label| {
            painter
                .layout_no_wrap(label, axis_font(painter.ctx()), Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0, f32::max)
        + 5.0
        + TAG_PADDING.x * 2.0
        + 8.0
}

/// Paints independent current close, session and volume readouts, returning their rectangles
/// in that order. Pass all three rectangles to tick occlusion and pointer routing.
///
/// Session lines stay at their data price. Tags move apart only to keep the rows readable;
/// volume is a value readout at the bottom of the pane, never a price coordinate. The caller supplies
/// the current values and colours independently of the historical bar under the pointer.
/// Off-scale prices do not hide other rows. A short pane keeps close, session, then volume.
pub fn render_quote_tags(
    painter: &Painter,
    price_rect: Rect,
    price_map: &PriceMap,
    close: &PriceLine,
    session: Option<(&crate::model::quote::SessionQuote, Color32)>,
    volume: Option<(f64, Color32)>,
    axis_right: f32,
) -> [Option<Rect>; 3] {
    render_quote_tags_with_countdown(
        painter, price_rect, price_map, close, session, volume, axis_right, None, None, 0.0,
    )
}

/// The chart's current-value stack, optionally including the supplied quote's bar countdown.
pub(crate) fn render_quote_tags_with_countdown(
    painter: &Painter,
    price_rect: Rect,
    price_map: &PriceMap,
    close: &PriceLine,
    session: Option<(&crate::model::quote::SessionQuote, Color32)>,
    volume: Option<(f64, Color32)>,
    axis_right: f32,
    countdown: Option<&str>,
    volume_precision: Option<u8>,
    reserved_top: f32,
) -> [Option<Rect>; 3] {
    use crate::scales::{PriceFormatter, VolumeFormatter};
    let mut tags = [None; 3];
    let transform = price_map.transform();
    let decimals = price_map.price_precision().unwrap_or(2);
    let height = painter
        .layout_no_wrap("0".into(), axis_font(painter.ctx()), Color32::WHITE)
        .size()
        .y
        + TAG_PADDING.y * 2.0;
    let visible = |price| price_map.range().contains(transform.to_scale(price));
    let close_y = price_map.price_to_y(close.price);
    let mut rows = Vec::with_capacity(3);
    let mut line = close.clone();
    line.axis_label = false;
    render_price_line(
        painter,
        price_rect,
        price_map,
        &line,
        price_rect.left(),
        false,
        axis_right,
    );
    if visible(close.price) && close.axis_label {
        rows.push((
            0,
            close_y,
            {
                let price = transform.label(transform.to_scale(close.price), decimals);
                countdown.map_or_else(|| price.clone(), |time| format!("{price}\n{time}"))
            },
            close.color,
        ));
    }
    if let Some((quote, color)) = session.filter(|(quote, _)| visible(quote.price)) {
        let line = PriceLine::new(quote.price, color)
            .with_style(crate::model::LineStyle::Dotted)
            .with_axis_label(false)
            .with_width(0.7);
        render_price_line(
            painter,
            price_rect,
            price_map,
            &line,
            price_rect.left(),
            false,
            axis_right,
        );
        let mut y = price_map.price_to_y(quote.price);
        let separation = height
            + if countdown.is_some() {
                (height - TAG_PADDING.y * 2.0) / 2.0
            } else {
                0.0
            };
        if !rows.is_empty() && (y - close_y).abs() < separation {
            y = close_y - separation;
        }
        let label = transform.label(transform.to_scale(quote.price), decimals);
        rows.push((1, y, format!("{} {label}", quote.qualifier), color));
    }
    if let Some((value, color)) = volume.filter(|(value, _)| value.is_finite() && *value >= 0.0) {
        rows.push((
            2,
            price_rect.bottom() - height / 2.0,
            VolumeFormatter {
                precision: usize::from(volume_precision.unwrap_or(2)),
            }
            .format(value),
            color,
        ));
    }
    let row_height = |text: &str| {
        painter
            .layout_no_wrap(text.to_owned(), axis_font(painter.ctx()), Color32::WHITE)
            .size()
            .y
            + TAG_PADDING.y * 2.0
    };
    while rows.iter().map(|row| row_height(&row.2)).sum::<f32>()
        > price_rect.height() - reserved_top
    {
        if rows.pop().is_none() {
            break;
        }
    }
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut next = price_rect.top() + reserved_top;
    for row in &mut rows {
        let height = row_height(&row.2);
        row.1 = row.1.max(next + height / 2.0);
        next = row.1 + height / 2.0;
    }
    next = price_rect.bottom();
    for row in rows.iter_mut().rev() {
        let height = row_height(&row.2);
        row.1 = row.1.min(next - height / 2.0);
        next = row.1 - height / 2.0;
    }
    // Keep paint order stable even when the price coordinates reverse the row order.
    rows.sort_by_key(|row| row.0);
    for (slot, y, text, fill) in rows {
        let (anchor, align) = price_tag_anchor(price_rect, axis_right, y);
        tags[slot] = Some(paint_axis_tag(
            painter,
            anchor,
            align,
            axis_right,
            text,
            tag_text_color(fill),
            fill,
        ));
    }
    tags
}

/// Routes a click on a crosshair price tag. A click returns the tagged price
/// for the host's alert, order or price-line menu. Call before handling price-axis drags.
/// Pass the resolved label text colour as `foreground` for the hover outline.
#[cfg(feature = "icons")]
pub fn crosshair_price_action(
    ui: &mut egui::Ui,
    tag: Rect,
    price: f64,
    foreground: Color32,
) -> Option<f64> {
    let response = ui.interact(
        tag,
        ui.scope_id().with("crosshair_price_action"),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_stroke(
            tag,
            1.0,
            Stroke::new(1.0, foreground),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Add at price"));
    response
        .on_hover_text("Add at price")
        .clicked()
        .then_some(price)
}

/// A 24-point scale-settings target in the price/time-axis corner. The caller opens its
/// existing scale menu when the returned response is clicked, before handling scale drags.
#[cfg(feature = "icons")]
pub fn axis_settings_button(ui: &mut egui::Ui, corner: Rect) -> egui::Response {
    let target = Rect::from_center_size(corner.center(), Vec2::splat(24.0)).intersect(corner);
    let response = ui.interact(
        target,
        ui.scope_id().with("axis_settings"),
        egui::Sense::click(),
    );
    crate::icons::icons::SETTINGS
        .as_image_tinted(Vec2::splat(16.0), ui.ctx().active_theme().chart.axis_text)
        .paint_at(
            ui,
            Rect::from_center_size(target.center(), Vec2::splat(16.0)),
        );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Scale settings"));
    response.on_hover_text("Scale settings")
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

        for (fill, expected) in [
            (Color32::from_rgb(241, 54, 69), Color32::WHITE),
            (Color32::WHITE, Color32::BLACK),
        ] {
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(700.0, 710.0),
                    0.0,
                    100.0,
                    false,
                )
                .with_price_precision(Some(3));
                render_price_line(
                    ui.painter(),
                    pane,
                    &map,
                    &PriceLine::new(704.125, fill),
                    0.0,
                    true,
                    200.0,
                );
            });
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some(text),
                    _ => None,
                })
                .unwrap();
            assert_eq!(text.galley.text(), "704.125");
            assert_eq!(text.fallback_color, expected);
            let tag = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Rect(rect) => Some(rect),
                    _ => None,
                })
                .unwrap();
            assert_eq!(tag.corner_radius.nw, 0);
            assert_eq!(tag.corner_radius.sw, 0);
            output.drop_without_applying_deltas();
        }
    }
    /// Current values stay independent and ordered when price coordinates coincide.
    #[test]
    fn quote_tags_separate_colliding_prices_and_volume() {
        use crate::model::quote::SessionQuote;
        for (close, session_price, font_size, pane_height, show_close, slots, countdown) in [
            (704.61, 704.64, 12.0, 100.0, true, [true, true, true], None),
            (709.99, 709.99, 12.0, 100.0, true, [true, true, true], None),
            (700.01, 700.01, 12.0, 100.0, true, [true, true, true], None),
            (704.61, 704.64, 24.0, 100.0, true, [true, true, true], None),
            (709.99, 700.01, 12.0, 100.0, true, [true, true, true], None),
            (720.0, 704.64, 12.0, 100.0, true, [false, true, true], None),
            (
                704.61,
                704.64,
                12.0,
                100.0,
                false,
                [false, true, true],
                None,
            ),
            (704.61, 704.64, 12.0, 25.0, true, [true, false, false], None),
            (
                704.61,
                704.64,
                12.0,
                100.0,
                true,
                [true, true, true],
                Some("00:15"),
            ),
        ] {
            for mut theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
                let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, pane_height));
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(700.0, 710.0),
                    0.0,
                    pane_height,
                    false,
                )
                .with_price_precision(Some(2));
                let session = SessionQuote::new(session_price, "EXT");
                let ctx = Context::default();
                theme.text.label = font_size;
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut tags = [None; 3];
                let mut width = 0.0;
                let text = [
                    countdown.map_or_else(
                        || format!("{close:.2}"),
                        |time| format!("{close:.2}\n{time}"),
                    ),
                    format!("EXT {session_price:.2}"),
                    "1.59M".into(),
                ];
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    width = price_axis_width(
                        ui.painter(),
                        &map,
                        &text.iter().map(String::as_str).collect::<Vec<_>>(),
                    );
                    tags = render_quote_tags_with_countdown(
                        ui.painter(),
                        pane,
                        &map,
                        &PriceLine::new(close, Color32::RED).with_axis_label(show_close),
                        Some((&session, Color32::from_rgb(251, 142, 1))),
                        Some((1_590_000.0, Color32::RED)),
                        pane.right() + width,
                        countdown,
                        None,
                        0.0,
                    );
                });
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                assert_eq!(tags.map(|tag| tag.is_some()), slots);
                let rects: Vec<_> = tags.into_iter().flatten().collect();
                assert!(
                    rects
                        .iter()
                        .all(|rect| rect.top() >= 0.0 && rect.bottom() <= pane_height)
                );
                assert!(
                    rects
                        .iter()
                        .all(|rect| rect.left() >= pane.right() && rect.width() + 5.0 <= width)
                );
                assert!(
                    rects.iter().enumerate().all(|(i, a)| rects
                        .iter()
                        .skip(i + 1)
                        .all(|b| a.bottom() <= b.top() || b.bottom() <= a.top())),
                    "{rects:?}"
                );
                let expected: Vec<_> = text
                    .into_iter()
                    .zip(slots)
                    .filter_map(|(text, shown)| shown.then_some(text))
                    .collect();
                assert_eq!(labels, expected);
            }
        }
    }

    /// Add and settings cells emit their own clicks without a chart drag.
    #[cfg(feature = "icons")]
    #[test]
    fn axis_controls_emit_their_actions() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = Context::default();
        let tag = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(180.0, 121.0));
        let corner = Rect::from_min_max(Pos2::new(200.0, 200.0), Pos2::new(260.0, 228.0));
        let mut actions = Vec::new();
        for (pos, expected) in [
            (Pos2::new(110.0, 110.0), "add"),
            (corner.center(), "settings"),
        ] {
            for pressed in [None, Some(true), Some(false)] {
                let mut input = RawInput::default();
                input.events.push(Event::PointerMoved(pos));
                if let Some(pressed) = pressed {
                    input.events.push(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let output = ctx.run_ui(input, |ui| {
                    if let Some(price) = crosshair_price_action(ui, tag, 704.64, Color32::WHITE) {
                        assert_eq!(price, 704.64);
                        actions.push("add");
                    }
                    if axis_settings_button(ui, corner).clicked() {
                        actions.push("settings");
                    }
                });
                output.drop_without_applying_deltas();
            }
            assert_eq!(actions.last(), Some(&expected));
        }
        assert_eq!(actions, ["add", "settings"]);
    }
}
