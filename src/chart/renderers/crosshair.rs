use super::context::{ChartMapping, RenderContext};
use super::labels::tick_decimals;
use crate::chart::rendering::axes::paint_crosshair_tag;
use crate::chart::rendering::draw_dashed_line;
use crate::config::{CrosshairLineStyle, CrosshairMode, CrosshairOptions, CrosshairStyle};
use crate::model::Bar;
use crate::scales::{HorizontalScale, PriceFormatter, PriceMap, PriceScaleMode};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Pos2, Rect, Stroke};

/// Renders crosshair with full customization: style, mode, color, line width, line style
///
/// The vertical line runs down to `chart_bottom`, the bottom of the volume pane. The price label
/// sits in the price axis, which ends at `axis_right`, over the `last_price_tag` it overlaps, and
/// the time label on the time axis below `chart_bottom`, written by `scale`. Returns the rects
/// of the price and time labels drawn.
pub fn render_crosshair_full(
    context: &RenderContext,
    hover_pos: Pos2,
    visible_data: &[Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
    mode: CrosshairMode,
    style: CrosshairStyle,
    crosshair_color: Color32,
    line_width: f32,
    line_style: CrosshairLineStyle,
    label_bg: Color32,
    label_text: Color32,
    chart_bottom: f32,
    axis_right: f32,
    last_price_tag: Option<Rect>,
    scale: &dyn HorizontalScale,
    bar_seconds: i64,
) -> [Option<Rect>; 2] {
    render_crosshair_with_options(
        context,
        hover_pos,
        visible_data,
        price_scale,
        coords,
        &CrosshairOptions {
            mode,
            style,
            vert_line_color: crosshair_color,
            horz_line_color: crosshair_color,
            vert_line_width: line_width,
            horz_line_width: line_width,
            vert_line_style: line_style,
            horz_line_style: line_style,
            vert_label_background_color: label_bg,
            horz_label_background_color: label_bg,
            label_text_color: label_text,
            ..Default::default()
        },
        chart_bottom,
        axis_right,
        last_price_tag,
        scale,
        bar_seconds,
        None,
    )
}

/// Renders both crosshair lines and axis labels with independently configured visibility and strokes.
pub fn render_crosshair_with_options(
    context: &RenderContext,
    hover_pos: Pos2,
    visible_data: &[Bar],
    price_scale: &PriceMap,
    coords: &ChartMapping,
    options: &CrosshairOptions,
    chart_bottom: f32,
    axis_right: f32,
    last_price_tag: Option<Rect>,
    scale: &dyn HorizontalScale,
    bar_seconds: i64,
    price_formatter: Option<&dyn PriceFormatter>,
) -> [Option<Rect>; 2] {
    let options = options.resolved(context.painter.ctx());
    let bar = coords
        .local_idx_at_x(hover_pos.x, visible_data.len())
        .map(|idx| (coords.start_idx + idx, &visible_data[idx]));
    let mut pos = hover_pos;
    if let Some((idx, candle)) = bar {
        pos.x = coords.idx_to_x(idx);
        if options.mode.snaps() && context.rect.contains(hover_pos) {
            let price = if options.mode == CrosshairMode::MagnetClose {
                candle.close
            } else {
                snap_to_nearest_ohlc(pos.y, price_scale, candle)
            };
            pos.y = price_scale.price_to_y(price);
        }
    }
    render_crosshair_lines(context, pos, &options, chart_bottom);
    let price_tag = render_crosshair_price_tag(
        context,
        pos,
        price_scale,
        &options,
        axis_right,
        last_price_tag,
        price_formatter.map(|price_formatter| {
            price_scale
                .transform()
                .label(price_scale.y_to_scale(pos.y), 2, Some(price_formatter))
        }),
    );
    let time_tag = bar
        .filter(|_| {
            options.label_visible
                && options.vert_line_visible
                && options.style != CrosshairStyle::Hidden
        })
        .map(|(_, candle)| {
            paint_crosshair_tag(
                context.painter,
                Pos2::new(pos.x, chart_bottom + 1.5),
                Align2::CENTER_TOP,
                context.rect.left()..=axis_right.max(context.rect.right()),
                scale.format_crosshair(candle.time, bar_seconds),
                (
                    options.label_text_color,
                    options.vert_label_background_color,
                ),
                25.0,
                None,
            )
        });
    [price_tag, time_tag]
}

pub(crate) fn render_crosshair_lines(
    context: &RenderContext,
    pos: Pos2,
    options: &CrosshairOptions,
    chart_bottom: f32,
) {
    let line = |from, to, color, width, style| {
        let stroke = Stroke::new(width, color);
        match style {
            CrosshairLineStyle::Solid => {
                context.painter.line_segment([from, to], stroke);
            }
            CrosshairLineStyle::Dashed => {
                draw_dashed_line(context.painter, from, to, stroke, 5.0, 5.0)
            }
            CrosshairLineStyle::Dotted => {
                draw_dashed_line(context.painter, from, to, stroke, 1.0, 3.0)
            }
            CrosshairLineStyle::LargeDashed => {
                draw_dashed_line(context.painter, from, to, stroke, 10.0, 10.0)
            }
            CrosshairLineStyle::SparseDotted => {
                draw_dashed_line(context.painter, from, to, stroke, 1.0, 7.0)
            }
        }
    };
    match options.style {
        CrosshairStyle::Full => {
            if options.vert_line_visible {
                line(
                    Pos2::new(pos.x, context.rect.top()),
                    Pos2::new(pos.x, chart_bottom),
                    options.vert_line_color,
                    options.vert_line_width,
                    options.vert_line_style,
                );
            }
            if options.horz_line_visible && context.rect.y_range().contains(pos.y) {
                line(
                    Pos2::new(context.rect.left(), pos.y),
                    Pos2::new(context.rect.right(), pos.y),
                    options.horz_line_color,
                    options.horz_line_width,
                    options.horz_line_style,
                );
            }
        }
        CrosshairStyle::Dot if context.rect.contains(pos) => {
            context.painter.circle_filled(
                pos,
                DESIGN_TOKENS.sizing.crosshair.dot_radius,
                options.vert_line_color,
            );
        }
        _ => {}
    }
}

pub(crate) fn render_crosshair_price_tag(
    context: &RenderContext,
    pos: Pos2,
    price_scale: &PriceMap,
    options: &CrosshairOptions,
    axis_right: f32,
    last_price_tag: Option<Rect>,
    label: Option<String>,
) -> Option<Rect> {
    if !options.label_visible
        || !options.horz_line_visible
        || options.style == CrosshairStyle::Hidden
        || !context.rect.y_range().contains(pos.y)
    {
        return None;
    }
    let price = price_scale.y_to_price(pos.y);
    let transform = price_scale.transform();
    let label = if let Some(label) = label {
        label
    } else if matches!(
        transform.mode,
        PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
    ) {
        transform.label(price_scale.y_to_scale(pos.y), 2, None)
    } else {
        let decimals = price_scale
            .price_precision()
            .unwrap_or_else(|| tick_decimals(price_scale, pos.y));
        format!("{price:.decimals$}")
    };
    let left = axis_right <= context.rect.left();
    let painter = context
        .painter
        .with_clip_rect(context.painter.clip_rect().intersect(Rect::from_x_y_ranges(
            context.painter.clip_rect().x_range(),
            context.rect.y_range(),
        )));
    Some(paint_crosshair_tag(
        &painter,
        Pos2::new(
            if left {
                context.rect.left()
            } else {
                context.rect.right()
            },
            pos.y,
        ),
        if left {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        },
        if left {
            context.painter.clip_rect().left()..=context.rect.left()
        } else {
            context.rect.right()..=axis_right
        },
        label,
        (
            options.label_text_color,
            options.horz_label_background_color,
        ),
        21.0,
        last_price_tag,
    ))
}

/// Snap to nearest OHLC price (for Magnet mode)
pub(crate) fn snap_to_nearest_ohlc(hover_y: f32, price_scale: &PriceMap, candle: &Bar) -> f64 {
    snap_to_values(
        hover_y,
        price_scale,
        [candle.open, candle.high, candle.low, candle.close],
    )
    .unwrap_or(candle.close)
}

pub(crate) fn snap_to_values(
    hover_y: f32,
    price_scale: &PriceMap,
    values: impl IntoIterator<Item = f64>,
) -> Option<f64> {
    values
        .into_iter()
        .filter(|value| value.is_finite())
        .min_by(|a, b| {
            (price_scale.price_to_y(*a) - hover_y)
                .abs()
                .total_cmp(&(price_scale.price_to_y(*b) - hover_y).abs())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform, TimeFormatterBuilder, TimezoneMode};
    use chrono::{Duration, TimeZone, Utc};
    use egui::{Context, RawInput, Shape};

    /// Edge tags remain inside the pane, use supplied precision, and name the nearest bar
    /// in the selected timezone, including when the pointer is below the price area.
    #[test]
    fn hover_tags_keep_the_bar_time_and_fit_both_edges() {
        let time = Utc.with_ymd_and_hms(2024, 10, 1, 0, 30, 0).unwrap();
        let bars: Vec<_> = (0..30)
            .map(|i| Bar::new(time + Duration::hours(i), 704.0, 706.0, 703.0, 705.0, 10.0))
            .collect();
        for (zone, first_label) in [
            (TimezoneMode::Utc, "Tue 01 Oct '24 00:30"),
            (TimezoneMode::nyse(), "Mon 30 Sep '24 20:30"),
        ] {
            let formatter = TimeFormatterBuilder::new().with_timezone(zone).build();
            for (index, hover_y, width, style) in [
                (0, 100.0, 300.0, CrosshairStyle::Full),
                (0, 100.0, 300.0, CrosshairStyle::Arrow),
                (0, 100.0, 300.0, CrosshairStyle::Dot),
                (29, 100.0, 300.0, CrosshairStyle::Full),
                (0, 250.0, 300.0, CrosshairStyle::Full),
                (0, 100.0, 60.0, CrosshairStyle::Full),
                (0, 1.0, 300.0, CrosshairStyle::Full),
                (0, 199.0, 300.0, CrosshairStyle::Full),
            ] {
                let rect =
                    Rect::from_min_max(Pos2::new(100.0, 0.0), Pos2::new(100.0 + width, 200.0));
                let axis_right = rect.right() + 60.0;
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(700.0, 710.0),
                    0.0,
                    200.0,
                    false,
                )
                .with_price_precision(Some(3));
                let coords = ChartMapping::new(rect, 10.0, 0, 29, 0.0, map);
                let ctx = Context::default();
                let mut tags = [None; 2];
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    tags = render_crosshair_full(
                        &RenderContext::new(ui.painter(), rect),
                        Pos2::new(coords.idx_to_x(index), hover_y),
                        &bars,
                        &map,
                        &coords,
                        CrosshairMode::Normal,
                        style,
                        Color32::GRAY,
                        1.0,
                        CrosshairLineStyle::Dotted,
                        Color32::BLACK,
                        Color32::WHITE,
                        300.0,
                        axis_right,
                        None,
                        &formatter,
                        3600,
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
                let date = tags[1].unwrap();
                assert!(
                    date.left() >= rect.left() && date.right() <= axis_right,
                    "{date:?}"
                );
                assert_eq!(date.height(), 25.0);
                if let Some(tag) = tags[0] {
                    assert_eq!(tag.height(), 21.0);
                    assert!(
                        tag.top() >= rect.top() && tag.bottom() <= rect.bottom(),
                        "{tag:?}"
                    );
                    assert!(
                        tag.left() >= rect.right() && tag.right() <= axis_right,
                        "{tag:?}"
                    );
                } else {
                    assert!(!rect.y_range().contains(hover_y));
                }
                if index == 0 {
                    assert!(
                        labels.iter().any(|label| label == first_label),
                        "{labels:?}"
                    );
                }
                if rect.y_range().contains(hover_y) {
                    assert!(
                        labels.contains(&format!("{:.3}", map.y_to_price(hover_y))),
                        "{labels:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn options_control_each_stroke_and_label_at_the_bar_center() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 0.0), Pos2::new(400.0, 200.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(700.0, 710.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping::new(rect, 10.0, 0, 29, 0.0, map);
        let bar = Bar::new(
            Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap(),
            704.0,
            706.0,
            703.0,
            705.0,
            10.0,
        );
        let bars = vec![bar; 30];
        for vertical in [false, true] {
            for horizontal in [false, true] {
                for labels in [false, true] {
                    for style in [CrosshairStyle::Full, CrosshairStyle::Hidden] {
                        let shown = style != CrosshairStyle::Hidden;
                        let (vertical_shown, horizontal_shown) =
                            (vertical && shown, horizontal && shown);
                        let options = CrosshairOptions {
                            style,
                            vert_line_visible: vertical,
                            horz_line_visible: horizontal,
                            label_visible: labels,
                            vert_line_color: Color32::RED,
                            horz_line_color: Color32::BLUE,
                            vert_line_width: 2.0,
                            horz_line_width: 3.0,
                            vert_line_style: CrosshairLineStyle::Solid,
                            horz_line_style: CrosshairLineStyle::Dotted,
                            vert_label_background_color: Color32::DARK_RED,
                            horz_label_background_color: Color32::DARK_GREEN,
                            ..Default::default()
                        };
                        let ctx = Context::default();
                        let mut tags = [None; 2];
                        let x = coords.idx_to_x(10);
                        let output = ctx.run_ui(RawInput::default(), |ui| {
                            tags = crate::chart::rendering::render_crosshair_with_options(
                                &RenderContext::new(ui.painter(), rect),
                                Pos2::new(x + 3.0, 100.0),
                                &bars,
                                &map,
                                &coords,
                                &options,
                                300.0,
                                480.0,
                                None,
                                &TimeFormatterBuilder::new().build(),
                                3600,
                                None,
                            );
                        });
                        let (lines, dots): (Vec<_>, Vec<_>) = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match shape.shape {
                                Shape::LineSegment { points, stroke } => Some((points, stroke)),
                                _ => None,
                            })
                            .partition(|(_, stroke)| stroke.color == Color32::RED);
                        // The dotted horizontal line is a row of one-point dots.
                        assert!(dots.iter().all(|([a, b], stroke)| {
                            *stroke == Stroke::new(3.0, Color32::BLUE)
                                && a.y == 100.0
                                && b.y == 100.0
                                && b.x - a.x <= 1.0
                        }));
                        let dots = !dots.is_empty();
                        let fill = |color| {
                            output.shapes.iter().any(
                                |shape| matches!(&shape.shape, Shape::Rect(r) if r.fill == color),
                            )
                        };
                        let (time_fill, price_fill) =
                            (fill(Color32::DARK_RED), fill(Color32::DARK_GREEN));
                        output.drop_without_applying_deltas();
                        assert_eq!(lines.len(), usize::from(vertical_shown));
                        if vertical_shown {
                            assert!(lines.contains(&(
                                [Pos2::new(x, 0.0), Pos2::new(x, 300.0)],
                                Stroke::new(2.0, Color32::RED)
                            )));
                        }
                        assert_eq!(dots, horizontal_shown);
                        assert_eq!(tags[0].is_some(), labels && horizontal_shown);
                        assert_eq!(tags[1].is_some(), labels && vertical_shown);
                        assert_eq!(price_fill, tags[0].is_some());
                        assert_eq!(time_fill, tags[1].is_some());
                    }
                }
            }
        }
    }
}
