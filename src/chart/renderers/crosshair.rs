use super::context::{ChartMapping, RenderContext};
use super::labels::tick_decimals;
use crate::chart::rendering::axes::paint_crosshair_tag;
use crate::chart::rendering::draw_dashed_line;
use crate::config::{CrosshairLineStyle, CrosshairMode, CrosshairStyle};
use crate::model::Bar;
use crate::scales::{PriceMap, PriceScaleMode, TimeFormatter};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Pos2, Rect, Stroke};

/// Renders crosshair with full customization: style, mode, color, line width, line style
///
/// The vertical line runs down to `chart_bottom`, the bottom of the volume pane. The price label
/// sits in the price axis, which ends at `axis_right`, over the `last_price_tag` it overlaps, and
/// the time label on the time axis below `chart_bottom`, written by `formatter`. Returns the rects
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
    formatter: &dyn TimeFormatter,
    bar_seconds: i64,
) -> [Option<Rect>; 2] {
    // The bar nearest the pointer, the one the OHLC readout shows, and its global index
    let bar = coords
        .local_idx_at_x(hover_pos.x, visible_data.len())
        .map(|idx| (coords.start_idx + idx, &visible_data[idx]));

    // Determine actual hover position based on mode
    let (actual_hover_x, actual_hover_y) = if let Some((idx, candle)) = bar {
        match mode {
            CrosshairMode::Normal => (hover_pos.x, hover_pos.y),
            CrosshairMode::Magnet => {
                // Snap to nearest candle X position using ChartCoords helper
                let candle_x = coords.idx_to_x(idx);

                // Snap to nearest OHLC price
                let snap_price = snap_to_nearest_ohlc(hover_pos.y, price_scale, candle);
                let snap_y = price_scale.price_to_y(snap_price);

                (candle_x, snap_y)
            }
        }
    } else {
        (hover_pos.x, hover_pos.y)
    };

    let actual_pos = Pos2::new(actual_hover_x, actual_hover_y);

    // Render based on style
    match style {
        CrosshairStyle::Full => {
            // Full crosshair with vertical and horizontal lines
            match line_style {
                CrosshairLineStyle::Solid => {
                    // Solid vertical line
                    context.painter.line_segment(
                        [
                            Pos2::new(actual_pos.x, context.rect.min.y),
                            Pos2::new(actual_pos.x, chart_bottom),
                        ],
                        Stroke::new(line_width, crosshair_color),
                    );
                    // Solid horizontal line
                    context.painter.line_segment(
                        [
                            Pos2::new(context.rect.min.x, actual_pos.y),
                            Pos2::new(context.rect.max.x, actual_pos.y),
                        ],
                        Stroke::new(line_width, crosshair_color),
                    );
                }
                CrosshairLineStyle::Dashed | CrosshairLineStyle::Dotted => {
                    let (dash, gap) = if line_style == CrosshairLineStyle::Dashed {
                        (
                            DESIGN_TOKENS.spacing.sm + 1.0,
                            DESIGN_TOKENS.spacing.xs + 1.0,
                        )
                    } else {
                        (1.0, 3.0)
                    };
                    let stroke = Stroke::new(line_width, crosshair_color);
                    let rect = context.rect;
                    draw_dashed_line(
                        context.painter,
                        Pos2::new(actual_pos.x, rect.min.y),
                        Pos2::new(actual_pos.x, chart_bottom),
                        stroke,
                        dash,
                        gap,
                    );
                    draw_dashed_line(
                        context.painter,
                        Pos2::new(rect.min.x, actual_pos.y),
                        Pos2::new(rect.max.x, actual_pos.y),
                        stroke,
                        dash,
                        gap,
                    );
                }
            }
        }
        CrosshairStyle::Dot => {
            // Just a dot at the intersection point
            context.painter.circle_filled(
                actual_pos,
                DESIGN_TOKENS.sizing.crosshair.dot_radius,
                crosshair_color,
            );
        }
        CrosshairStyle::Arrow | CrosshairStyle::Demonstration => {}
    }

    // Calculate price at cursor
    let price = price_scale.y_to_price(actual_pos.y);

    // Price label, in the axis units of the relative modes and with the tick decimals otherwise
    let transform = price_scale.transform();
    let price_label = if matches!(
        transform.mode,
        PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
    ) {
        transform.label(price_scale.y_to_scale(actual_pos.y), 2)
    } else {
        let decimals = price_scale
            .price_precision()
            .unwrap_or_else(|| tick_decimals(price_scale, actual_pos.y));
        format!("{price:.decimals$}")
    };
    let price_tag = paint_crosshair_tag(
        context.painter,
        Pos2::new(context.rect.max.x - 17.0, actual_pos.y),
        Align2::LEFT_CENTER,
        context.rect.left()..=axis_right,
        price_label,
        (label_text, label_bg),
        21.0,
        21.0,
        last_price_tag,
    );

    let time_tag = bar.map(|(_, candle)| {
        paint_crosshair_tag(
            context.painter,
            Pos2::new(actual_pos.x, chart_bottom + 1.5),
            Align2::CENTER_TOP,
            context.rect.left()..=axis_right,
            formatter.format_crosshair(candle.time, bar_seconds),
            (label_text, label_bg),
            25.0,
            0.0,
            None,
        )
    });

    [Some(price_tag), time_tag]
}

/// Snap to nearest OHLC price (for Magnet mode)
pub(crate) fn snap_to_nearest_ohlc(hover_y: f32, price_scale: &PriceMap, candle: &Bar) -> f64 {
    // Get Y positions of all OHLC values
    let open_y = price_scale.price_to_y(candle.open);
    let high_y = price_scale.price_to_y(candle.high);
    let low_y = price_scale.price_to_y(candle.low);
    let close_y = price_scale.price_to_y(candle.close);

    // Find the nearest one
    let candidates = [
        (open_y, candle.open),
        (high_y, candle.high),
        (low_y, candle.low),
        (close_y, candle.close),
    ];

    let mut nearest_price = candle.close;
    let mut min_distance = f32::MAX;

    for (y, price) in candidates {
        let distance = (hover_y - y).abs();
        if distance < min_distance {
            min_distance = distance;
            nearest_price = price;
        }
    }

    nearest_price
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
                        formatter.as_ref(),
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
                assert_eq!(tags[0].unwrap().height(), 21.0);
                if index == 0 {
                    assert!(
                        labels.iter().any(|label| label == first_label),
                        "{labels:?}"
                    );
                }
                assert!(
                    labels.contains(&format!("{:.3}", map.y_to_price(hover_y))),
                    "{labels:?}"
                );
            }
        }
    }
}
