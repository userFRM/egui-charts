use super::context::{ChartMapping, RenderContext};
use super::labels::tick_decimals;
use crate::chart::rendering::axes::{PRICE_LABEL_GAP, TIME_LABEL_GAP, paint_axis_tag};
use crate::config::{CrosshairLineStyle, CrosshairMode, CrosshairStyle};
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::model::Bar;
use crate::scales::{PriceMap, PriceScaleMode, TickMarkType, TimeFormatter};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
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
) -> [Option<Rect>; 2] {
    // Arrow style = no crosshair at all (just standard mouse cursor)
    if style == CrosshairStyle::Arrow {
        return [None; 2];
    }

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
                        (DESIGN_TOKENS.spacing.xs, DESIGN_TOKENS.spacing.xs)
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
        CrosshairStyle::Arrow => {
            // Already handled by early return above
            unreachable!()
        }
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
        let decimals = tick_decimals(price_scale, actual_pos.y);
        format!("{price:.decimals$}")
    };
    let price_tag = paint_axis_tag(
        context.painter,
        Pos2::new(context.rect.max.x + PRICE_LABEL_GAP, actual_pos.y),
        Align2::LEFT_CENTER,
        axis_right,
        last_price_tag,
        price_label,
        label_text,
        label_bg,
    );

    let time_tag = bar.map(|(_, candle)| {
        let bar_seconds = visible_data.get(1).map_or(1, |next| {
            next.time
                .signed_duration_since(visible_data[0].time)
                .num_seconds()
                .abs()
        });
        paint_axis_tag(
            context.painter,
            Pos2::new(actual_pos.x, chart_bottom + TIME_LABEL_GAP),
            Align2::CENTER_TOP,
            axis_right,
            None,
            time_label(candle.time, bar_seconds, formatter),
            label_text,
            label_bg,
        )
    });

    [Some(price_tag), time_tag]
}

/// The crosshair's time label for a bar `bar_seconds` long: its date as the time axis writes
/// it, with the year unless it is the current one, then the time of day for intraday bars.
fn time_label(time: DateTime<Utc>, bar_seconds: i64, formatter: &dyn TimeFormatter) -> String {
    let mut label = formatter.format(time, TickMarkType::DayOfMonth);
    let year = formatter.format(time, TickMarkType::Year);
    if year != formatter.format(Utc::now(), TickMarkType::Year) {
        label = format!("{label}, {year}");
    }
    match bar_seconds {
        86_400.. => label,
        60.. => format!("{label} {}", formatter.format(time, TickMarkType::Time)),
        1.. => format!(
            "{label} {}",
            formatter.format(time, TickMarkType::TimeWithSeconds)
        ),
        _ => format!(
            "{label} {}{}",
            formatter.format(time, TickMarkType::TimeWithSeconds),
            time.format("%.3f")
        ),
    }
}

/// Snap to nearest OHLC price (for Magnet mode)
fn snap_to_nearest_ohlc(hover_y: f32, price_scale: &PriceMap, candle: &Bar) -> f64 {
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
