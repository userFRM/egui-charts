use super::context::{ChartMapping, RenderContext, StyleColors};
use super::labels::price_decimals;
use crate::config::{TooltipMode, TooltipOptions};
use crate::model::{Bar, Instrument, percent_change};
use crate::scales::{
    DefaultPriceFormatter, PriceFormatter, PriceMap, TimeFormatter, VolumeFormatter,
    instrument_price_precision,
};
use crate::styles::typography;
use crate::theme::ThemeExt;
/// Tooltip Renderers
///
/// Provides three tooltip variants:
/// - Floating: Classic tooltip that follows cursor position
/// - Tracking: Fixed horizontal bar at top of chart showing OHLC data
/// - Magnifier: Circular zoom lens for detailed data inspection
use crate::tokens::DESIGN_TOKENS;
use egui::{FontId, Painter, Pos2, Rect, Stroke, Vec2, epaint::StrokeKind};

/// One visible study plot's value at the selected bar. Missing or warming-up values stay absent.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyValue {
    /// Study title.
    pub study: String,
    /// Plot title within the study.
    pub plot: String,
    /// Finite value at the selected index, or `None` for a gap.
    pub value: Option<f64>,
    /// Number of decimal places used by the study's legend.
    pub precision: usize,
    /// Plot colour.
    pub color: egui::Color32,
}

impl StudyValue {
    /// Study and plot caption, omitting a repeated single-plot title.
    pub fn label(&self) -> String {
        if self.plot.is_empty() || self.plot == self.study {
            self.study.clone()
        } else {
            format!("{}: {}", self.study, self.plot)
        }
    }

    /// Value at the study's precision, or the missing-value marker.
    pub fn formatted_value(&self) -> String {
        self.value.filter(|value| value.is_finite()).map_or_else(
            || "∅".to_owned(),
            |value| format!("{value:.precision$}", precision = self.precision),
        )
    }
}

/// Renders tooltip based on configured mode
pub fn render_tooltip_with_options(
    context: &RenderContext,
    hover_pos: Pos2,
    candle: &Bar,
    previous_close: Option<f64>,
    options: &TooltipOptions,
    colors: &StyleColors,
    price_scale: &PriceMap,
    coords: &ChartMapping,
    visible_data: &[Bar],
    formatter: &dyn TimeFormatter,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
) {
    match options.mode {
        TooltipMode::Floating => {
            render_floating_tooltip(
                context.painter,
                hover_pos,
                context.rect,
                candle,
                previous_close,
                options,
                colors.bar_color(candle.close >= candle.open),
                formatter,
                bar_seconds,
                studies,
                instrument,
            );
        }
        TooltipMode::Tracking => {
            render_tracking_tooltip(
                context.painter,
                context.rect,
                candle,
                options,
                formatter,
                bar_seconds,
                studies,
                instrument,
            );
        }
        TooltipMode::Magnifier => {
            render_magnifier_tooltip(
                context,
                hover_pos,
                candle,
                options,
                price_scale,
                coords,
                visible_data,
            );
        }
    }
}

/// Renders the floating tooltip: a label and value grid in the top corner of `price_rect`
/// on the side away from the pointer, so it never covers the hovered bar.
pub fn render_floating_tooltip(
    painter: &Painter,
    hover_pos: Pos2,
    price_rect: Rect,
    candle: &Bar,
    previous_close: Option<f64>,
    options: &TooltipOptions,
    value_color: egui::Color32,
    formatter: &dyn TimeFormatter,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
) {
    let options = &options.resolved(painter.ctx());
    let price = |price: f64| {
        let decimals = instrument.and_then(instrument_price_precision).map_or_else(
            || price_decimals(price).min(options.price_precision),
            usize::from,
        );
        DefaultPriceFormatter::with_precision(decimals).format(price)
    };
    let mut rows = Vec::new();

    if options.show_time {
        rows.push(("Time", formatter.format_crosshair(candle.time, bar_seconds)));
    }

    if options.show_ohlc {
        rows.push(("Open", price(candle.open)));
        rows.push(("High", price(candle.high)));
        rows.push(("Low", price(candle.low)));
        rows.push(("Close", price(candle.close)));
    }

    if options.show_change {
        let reference = previous_close.unwrap_or(candle.open);
        let change = candle.close - reference;
        let change_pct = percent_change(reference, candle.close);
        rows.push((
            "Change",
            format!(
                "{change:+.*} ({change_pct:+.2}%)",
                instrument
                    .and_then(instrument_price_precision)
                    .map_or(2, usize::from)
            ),
        ));
    }
    if options.show_volume {
        let volume = VolumeFormatter {
            precision: instrument
                .and_then(|instrument| instrument.volume_decimals)
                .map_or(2, usize::from),
        }
        .format(candle.volume);
        rows.push(("Vol", volume));
    }

    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|(label, value)| (label.to_owned(), value, value_color))
        .collect();
    rows.extend(
        studies
            .iter()
            .map(|study| (study.label(), study.formatted_value(), study.color)),
    );

    if rows.is_empty() {
        return;
    }

    let theme = painter.ctx().active_theme();
    let font_id = FontId::proportional(options.font_size);
    let rows: Vec<_> = rows
        .into_iter()
        .map(|(label, value, color)| {
            (
                painter.layout_no_wrap(label, font_id.clone(), options.text_color),
                painter.layout_no_wrap(value, font_id.clone(), color),
            )
        })
        .collect();
    let label_width = rows
        .iter()
        .map(|(label, _)| label.size().x)
        .fold(0.0, f32::max);
    let value_width = rows
        .iter()
        .map(|(_, value)| value.size().x)
        .fold(0.0, f32::max);
    let row_gap = if options.on_long_press {
        6.0
    } else {
        theme.spacing.xs
    };
    let line_height = rows[0].0.size().y + row_gap;
    let padding = if options.on_long_press {
        14.0
    } else {
        theme.spacing.lg
    };
    let size = Vec2::new(
        label_width
            + if options.on_long_press {
                26.0
            } else {
                theme.spacing.lg
            }
            + value_width,
        rows.len() as f32 * line_height - row_gap,
    ) + Vec2::splat(2.0 * padding);

    let inset = theme.spacing.lg;
    let x = if hover_pos.x > price_rect.center().x {
        price_rect.min.x + inset
    } else {
        price_rect.max.x - inset - size.x
    };
    let pos = if options.on_long_press {
        let x = if hover_pos.x + 16.0 + size.x <= price_rect.right() {
            hover_pos.x + 16.0
        } else {
            hover_pos.x - 16.0 - size.x
        };
        Pos2::new(
            x.max(price_rect.left()),
            (hover_pos.y - size.y / 2.0).clamp(
                price_rect.top(),
                (price_rect.bottom() - size.y).max(price_rect.top()),
            ),
        )
    } else {
        Pos2::new(x, price_rect.min.y + inset)
    };
    let tooltip_rect = Rect::from_min_size(pos, size);
    if options.on_long_press {
        painter.add(
            egui::Shadow {
                offset: [0, 2],
                blur: 8,
                spread: 0,
                color: egui::Color32::from_black_alpha(40),
            }
            .as_shape(tooltip_rect, theme.radius.sm),
        );
    }

    let border_color = if candle.is_bullish() {
        options.border_color_bullish
    } else {
        options.border_color_bearish
    };
    painter.rect_filled(tooltip_rect, theme.radius.sm, options.background_color);
    painter.rect_stroke(
        tooltip_rect,
        theme.radius.sm,
        Stroke::new(1.0, border_color),
        StrokeKind::Inside,
    );

    // Labels left-aligned, values right-aligned
    let values_right = tooltip_rect.max.x - padding;
    for (i, (label, value)) in rows.into_iter().enumerate() {
        let y = tooltip_rect.min.y + padding + i as f32 * line_height;
        painter.galley(
            Pos2::new(tooltip_rect.min.x + padding, y),
            label,
            options.text_color,
        );
        painter.galley(
            Pos2::new(values_right - value.size().x, y),
            value,
            options.text_color,
        );
    }
}

/// Renders tracking tooltip as horizontal bar at top of chart
/// Shows: O: xxx  H: xxx  L: xxx  C: xxx  Vol: xxx  (+x.xx%)
pub fn render_tracking_tooltip(
    painter: &Painter,
    chart_rect: Rect,
    candle: &Bar,
    options: &TooltipOptions,
    formatter: &dyn TimeFormatter,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
) {
    let options = &options.resolved(painter.ctx());
    let bar_height = options.tracking_bar_height;
    let bar_rect = Rect::from_min_size(chart_rect.min, Vec2::new(chart_rect.width(), bar_height));

    // Draw bar background
    painter.rect_filled(bar_rect, 0.0, options.tracking_bar_background);

    // Build tracking text
    let precision = instrument
        .and_then(instrument_price_precision)
        .map_or(options.price_precision.min(4), usize::from);
    let mut parts = Vec::new();

    if options.show_time {
        parts.push(formatter.format_crosshair(candle.time, bar_seconds));
    }

    if options.show_ohlc {
        parts.push(format!("O: {:.precision$}", candle.open));
        parts.push(format!("H: {:.precision$}", candle.high));
        parts.push(format!("L: {:.precision$}", candle.low));
        parts.push(format!("C: {:.precision$}", candle.close));
    }

    if options.show_volume {
        parts.push(format!(
            "Vol: {}",
            VolumeFormatter {
                precision: instrument
                    .and_then(|instrument| instrument.volume_decimals)
                    .map_or(2, usize::from)
            }
            .format(candle.volume)
        ));
    }

    parts.extend(
        studies
            .iter()
            .map(|study| format!("{}: {}", study.label(), study.formatted_value())),
    );

    if options.show_change {
        let change_pct = percent_change(candle.open, candle.close);
        let sign = if change_pct >= 0.0 { "+" } else { "" };
        let chart = painter.ctx().active_theme().chart;
        let change_color = if change_pct >= 0.0 {
            chart.candle_up
        } else {
            chart.candle_down
        };

        // Draw change separately with color
        let main_text = parts.join("   ");
        let change_text = format!("({sign}{change_pct:.2}%)");

        let font_id = FontId::proportional(options.font_size);
        let text_y = bar_rect.center().y;

        // Draw main text
        let main_rect = painter.text(
            Pos2::new(bar_rect.min.x + 10.0, text_y),
            egui::Align2::LEFT_CENTER,
            &main_text,
            font_id.clone(),
            options.text_color,
        );

        // Draw change text with color
        painter.text(
            Pos2::new(main_rect.max.x + 15.0, text_y),
            egui::Align2::LEFT_CENTER,
            &change_text,
            font_id,
            change_color,
        );

        return;
    }

    // If no change display, just show combined text
    let text = parts.join("   ");
    let font_id = FontId::proportional(options.font_size);

    painter.text(
        Pos2::new(bar_rect.min.x + 10.0, bar_rect.center().y),
        egui::Align2::LEFT_CENTER,
        &text,
        font_id,
        options.text_color,
    );
}

/// Renders magnifier tooltip as circular zoom lens
/// Shows enlarged view of candles around cursor position
pub fn render_magnifier_tooltip(
    context: &RenderContext,
    hover_pos: Pos2,
    _candle: &Bar,
    options: &TooltipOptions,
    price_scale: &PriceMap,
    coords: &ChartMapping,
    visible_data: &[Bar],
) {
    let chart = context.painter.ctx().active_theme().chart;
    // A zoom or size of 0, NaN or infinity would make the lens span unboundedly many bars or none.
    let zoom = if options.magnifier_zoom.is_nan() {
        0.1
    } else {
        options.magnifier_zoom.clamp(0.1, 100.0)
    };
    let size = options
        .magnifier_size
        .max(0.0)
        .min(context.rect.width().min(context.rect.height()));
    let radius = size / 2.0;

    // Pos magnifier (centered on cursor, but kept in bounds)
    let mut center = hover_pos;
    if center.x - radius < context.rect.min.x {
        center.x = context.rect.min.x + radius;
    }
    if center.x + radius > context.rect.max.x {
        center.x = context.rect.max.x - radius;
    }
    if center.y - radius < context.rect.min.y {
        center.y = context.rect.min.y + radius;
    }
    if center.y + radius > context.rect.max.y {
        center.y = context.rect.max.y - radius;
    }

    // Draw magnifier background (dark circle)
    context
        .painter
        .circle_filled(center, radius, chart.bg_tooltip);

    // Draw magnifier border
    context.painter.circle_stroke(
        center,
        radius,
        Stroke::new(DESIGN_TOKENS.stroke.thick, chart.axis_text_secondary),
    );

    // Calculate visible range in the magnifier
    // The magnifier shows a zoomed view centered on the hover position
    let src_width = size / zoom;

    // Enlarges the chart about the cursor, so prices land where the chart's own scale puts them.
    let price_to_mag_y =
        |price: f64| center.y + (price_scale.price_to_y(price) - hover_pos.y) * zoom;

    // Calculate bar indices visible in magnifier. Degenerate bar spacing maps
    // every pixel onto the same bar rather than dividing by zero.
    let bars_per_pixel = if coords.bar_spacing.abs() < f32::EPSILON {
        0.0
    } else {
        1.0 / coords.bar_spacing
    };
    let center_bars_from_right =
        (context.rect.max.x - hover_pos.x) * bars_per_pixel - 0.5 - coords.right_offset;
    let src_bars = (src_width * bars_per_pixel) as isize;
    let half_bars = src_bars / 2;

    // Render candles within the magnifier clip region
    // Use clip rect to constrain drawing to the circle
    let clip_rect = Rect::from_center_size(center, Vec2::splat(size));

    // Draw zoomed candles
    let zoomed_bar_spacing = coords.bar_spacing * zoom;
    let candle_width = zoomed_bar_spacing * 0.7;

    // Only bars of `visible_data` can be drawn, so the loop never visits more than that many.
    let last = (coords.base_idx as f32 - center_bars_from_right - coords.start_idx as f32).floor()
        as isize;
    let first = last
        .saturating_add(1)
        .saturating_sub(visible_data.len() as isize);
    for i in (-half_bars).max(first)..=half_bars.min(last) {
        let bar_offset = center_bars_from_right + i as f32;
        let global_idx = (coords.base_idx as f32 - bar_offset).floor() as isize;

        if global_idx < coords.start_idx as isize {
            continue;
        }
        let local_idx = (global_idx as usize).saturating_sub(coords.start_idx);
        if local_idx >= visible_data.len() {
            continue;
        }

        let bar = &visible_data[local_idx];

        // Calculate position within magnifier
        let bar_x = center.x - (i as f32 * zoomed_bar_spacing);

        // Skip if outside magnifier bounds
        if bar_x < clip_rect.min.x - candle_width || bar_x > clip_rect.max.x + candle_width {
            continue;
        }

        let open_y = price_to_mag_y(bar.open);
        let close_y = price_to_mag_y(bar.close);
        let high_y = price_to_mag_y(bar.high);
        let low_y = price_to_mag_y(bar.low);

        let is_bullish = bar.close >= bar.open;
        let candle_color = if is_bullish {
            chart.candle_up
        } else {
            chart.candle_down
        };

        // Draw wick
        context.painter.line_segment(
            [Pos2::new(bar_x, high_y), Pos2::new(bar_x, low_y)],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, candle_color),
        );

        // Draw body
        let body_top = open_y.min(close_y);
        let body_bottom = open_y.max(close_y);
        let body_height = (body_bottom - body_top).max(1.0);

        let body_rect = Rect::from_min_size(
            Pos2::new(bar_x - candle_width / 2.0, body_top),
            Vec2::new(candle_width, body_height),
        );

        if is_bullish {
            context.painter.rect_stroke(
                body_rect,
                0.0,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, candle_color),
                StrokeKind::Inside,
            );
        } else {
            context.painter.rect_filled(body_rect, 0.0, candle_color);
        }
    }

    // Draw crosshair in center of magnifier
    let crosshair_color = chart.crosshair_line;
    context.painter.line_segment(
        [
            Pos2::new(center.x, center.y - radius * 0.3),
            Pos2::new(center.x, center.y + radius * 0.3),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.hairline, crosshair_color),
    );
    context.painter.line_segment(
        [
            Pos2::new(center.x - radius * 0.3, center.y),
            Pos2::new(center.x + radius * 0.3, center.y),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.hairline, crosshair_color),
    );

    // Draw zoom indicator
    let zoom_text = format!("{zoom:.1}x");
    context.painter.text(
        Pos2::new(center.x, center.y + radius - 12.0),
        egui::Align2::CENTER_BOTTOM,
        &zoom_text,
        FontId::proportional(typography::TINY),
        chart.axis_text_secondary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceScaleMode, PriceTransform};
    use chrono::{TimeZone, Utc};

    const HOVER: Pos2 = Pos2::new(400.0, 250.0);

    /// The shapes the lens paints over 80 bars on a logarithmic scale, cursor at `HOVER`.
    fn lens_shapes(options: &TooltipOptions) -> Vec<egui::epaint::ClippedShape> {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 500.0));
        let transform = PriceTransform {
            mode: PriceScaleMode::Logarithmic,
            base: 0.0,
        };
        let range = PriceRange::new(10f64.ln(), 10_000f64.ln());
        let map = PriceMap::new(transform, range, 0.0, 500.0, false);
        let p = map.y_to_price(HOVER.y);
        let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = vec![Bar::new(t, p, p * 2.0, p / 2.0, p, 1.0); 80];
        let coords = ChartMapping::new(rect, 10.0, 0, 79, 0.0, map);
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            let painter = Painter::new(ui.ctx().clone(), egui::LayerId::background(), rect);
            render_magnifier_tooltip(
                &RenderContext::new(&painter, rect),
                HOVER,
                &bars[0],
                options,
                &map,
                &coords,
                &bars,
            );
        });
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        shapes
    }

    /// The lens magnifies the chart about the cursor: on a logarithmic scale a wick
    /// from half to twice the price under the cursor is centred on the lens.
    #[test]
    fn magnifier_centres_the_cursor_price_on_a_log_scale() {
        let shapes = lens_shapes(&TooltipOptions::default());
        // Longer than the lens crosshair, so only a candle wick matches.
        let wick = shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::LineSegment { points, .. }
                    if points[0].x == points[1].x && (points[0].y - points[1].y).abs() > 50.0 =>
                {
                    Some(*points)
                }
                _ => None,
            })
            .expect("a wick in the lens");
        let mid = (wick[0].y + wick[1].y) / 2.0;
        assert!((mid - HOVER.y).abs() < 0.5, "wick {wick:?}");
    }

    /// A zoom of 0 or NaN neither hangs the frame nor paints non-finite geometry.
    #[test]
    fn degenerate_magnifier_zoom_draws_finite_lens() {
        for zoom in [f32::NAN, 0.0] {
            for c in lens_shapes(&TooltipOptions::magnifier_with_zoom(zoom)) {
                if let egui::Shape::LineSegment { points, .. } = &c.shape {
                    assert!(
                        points.iter().all(|p| p.is_finite()),
                        "zoom {zoom}: {points:?}"
                    );
                }
            }
        }
    }
}
