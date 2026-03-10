use super::context::{ChartMapping, PriceScale, RenderContext};
use crate::config::{TooltipMode, TooltipOptions};
use crate::model::Bar;
use crate::styles::typography;
/// Tooltip Renderers
///
/// Provides three tooltip variants:
/// - Floating: Classic tooltip that follows cursor position
/// - Tracking: Fixed horizontal bar at top of chart showing OHLC data
/// - Magnifier: Circular zoom lens for detailed data inspection
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2, epaint::StrokeKind};

/// Renders tooltip based on configured mode
pub fn render_tooltip_with_options(
    context: &RenderContext,
    hover_pos: Pos2,
    candle: &Bar,
    options: &TooltipOptions,
    price_scale: &PriceScale,
    coords: &ChartMapping,
    visible_data: &[Bar],
) {
    match options.mode {
        TooltipMode::Floating => {
            render_floating_tooltip(context.painter, hover_pos, context.rect, candle, options);
        }
        TooltipMode::Tracking => {
            render_tracking_tooltip(context.painter, context.rect, candle, options);
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

/// Renders floating tooltip near cursor
/// Classic tooltip that follows the mouse
pub fn render_floating_tooltip(
    painter: &Painter,
    hover_pos: Pos2,
    price_rect: Rect,
    candle: &Bar,
    options: &TooltipOptions,
) {
    let mut tooltip_lines = Vec::new();

    if options.show_time {
        tooltip_lines.push(format!("Time: {}", candle.time.format("%Y-%m-%d %H:%M:%S")));
    }

    if options.show_ohlc {
        let precision = options.price_precision;
        tooltip_lines.push(format!("Open:   {:.precision$}", candle.open));
        tooltip_lines.push(format!("High:   {:.precision$}", candle.high));
        tooltip_lines.push(format!("Low:    {:.precision$}", candle.low));
        tooltip_lines.push(format!("Close:  {:.precision$}", candle.close));
    }

    if options.show_volume {
        tooltip_lines.push(format!("Volume: {:.2}", candle.volume));
    }

    if options.show_change {
        let change_pct = (candle.close - candle.open) / candle.open * 100.0;
        let sign = if change_pct >= 0.0 { "+" } else { "" };
        tooltip_lines.push(format!("Change: {sign}{change_pct:.2}%"));
    }

    if tooltip_lines.is_empty() {
        return;
    }

    let font_id = FontId::monospace(options.font_size);
    let line_height = options.font_size + DESIGN_TOKENS.spacing.sm + DESIGN_TOKENS.spacing.hairline;
    let padding = DESIGN_TOKENS.spacing.lg;

    let border_color = if candle.is_bullish() {
        options.border_color_bullish
    } else {
        options.border_color_bearish
    };

    // Calculate tooltip size
    let mut max_width = 0.0f32;
    for line in &tooltip_lines {
        let text_size = painter.text(
            Pos2::ZERO,
            egui::Align2::LEFT_TOP,
            line,
            font_id.clone(),
            Color32::TRANSPARENT,
        );
        max_width = max_width.max(text_size.width());
    }

    let tooltip_width = max_width + padding * 2.0;
    let tooltip_height = tooltip_lines.len() as f32 * line_height + padding * 2.0;

    // Pos tooltip (prefer right of cursor, fallback to left)
    let cursor_offset = DESIGN_TOKENS.sizing.tooltip.cursor_offset_x;
    let mut tooltip_x = hover_pos.x + cursor_offset;
    let mut tooltip_y = hover_pos.y - tooltip_height / 2.0;

    // Keep within bounds
    if tooltip_x + tooltip_width > price_rect.max.x {
        tooltip_x = hover_pos.x - tooltip_width - cursor_offset;
    }
    if tooltip_y < price_rect.min.y {
        tooltip_y = price_rect.min.y;
    }
    if tooltip_y + tooltip_height > price_rect.max.y {
        tooltip_y = price_rect.max.y - tooltip_height;
    }

    let tooltip_rect = Rect::from_min_size(
        Pos2::new(tooltip_x, tooltip_y),
        Vec2::new(tooltip_width, tooltip_height),
    );

    // Draw tooltip background with border
    painter.rect_filled(
        tooltip_rect,
        DESIGN_TOKENS.rounding.sm,
        options.background_color,
    );
    painter.rect_stroke(
        tooltip_rect,
        DESIGN_TOKENS.rounding.sm,
        Stroke::new(DESIGN_TOKENS.spacing.xs, border_color),
        StrokeKind::Outside,
    );

    // Draw tooltip text
    for (i, line) in tooltip_lines.iter().enumerate() {
        let text_pos = Pos2::new(
            tooltip_rect.min.x + padding,
            tooltip_rect.min.y + padding + i as f32 * line_height,
        );
        painter.text(
            text_pos,
            egui::Align2::LEFT_TOP,
            line,
            font_id.clone(),
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
) {
    let bar_height = options.tracking_bar_height;
    let bar_rect = Rect::from_min_size(chart_rect.min, Vec2::new(chart_rect.width(), bar_height));

    // Draw bar background
    painter.rect_filled(bar_rect, 0.0, options.tracking_bar_background);

    // Build tracking text
    let precision = options.price_precision.min(4); // Use shorter precision for tracking bar
    let mut parts = Vec::new();

    if options.show_time {
        parts.push(candle.time.format("%H:%M:%S").to_string());
    }

    if options.show_ohlc {
        parts.push(format!("O: {:.precision$}", candle.open));
        parts.push(format!("H: {:.precision$}", candle.high));
        parts.push(format!("L: {:.precision$}", candle.low));
        parts.push(format!("C: {:.precision$}", candle.close));
    }

    if options.show_volume {
        parts.push(format!("Vol: {:.0}", candle.volume));
    }

    if options.show_change {
        let change_pct = (candle.close - candle.open) / candle.open * 100.0;
        let sign = if change_pct >= 0.0 { "+" } else { "" };
        let change_color = if change_pct >= 0.0 {
            options.border_color_bullish
        } else {
            options.border_color_bearish
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
    price_scale: &PriceScale,
    coords: &ChartMapping,
    visible_data: &[Bar],
) {
    let zoom = options.magnifier_zoom;
    let size = options.magnifier_size;
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
    let lens_bg = DESIGN_TOKENS.semantic.extended.chart_tooltip_bg;
    context.painter.circle_filled(center, radius, lens_bg);

    // Draw magnifier border
    let border_color = DESIGN_TOKENS.semantic.extended.chart_text_muted;
    context.painter.circle_stroke(
        center,
        radius,
        Stroke::new(DESIGN_TOKENS.stroke.thick, border_color),
    );

    // Calculate visible range in the magnifier
    // The magnifier shows a zoomed view centered on the hover position
    let src_width = size / zoom;
    let src_height = size / zoom;

    // Calculate the price range visible in magnifier
    let price_per_pixel = price_scale.price_range / context.rect.height() as f64;
    let center_price =
        price_scale.min_price + (context.rect.max.y - hover_pos.y) as f64 * price_per_pixel;

    let mag_price_range = src_height as f64 * price_per_pixel;
    let mag_min_price = center_price - mag_price_range / 2.0;
    let mag_max_price = center_price + mag_price_range / 2.0;

    // Calculate bar indices visible in magnifier
    let bars_per_pixel = 1.0 / coords.bar_spacing;
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

    for i in -half_bars..=half_bars {
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

        // Map prices to magnifier coords
        let price_to_mag_y = |price: f64| -> f32 {
            let ratio = ((price - mag_min_price) / (mag_max_price - mag_min_price)) as f32;
            center.y + radius - ratio * size
        };

        let open_y = price_to_mag_y(bar.open);
        let close_y = price_to_mag_y(bar.close);
        let high_y = price_to_mag_y(bar.high);
        let low_y = price_to_mag_y(bar.low);

        let is_bullish = bar.close >= bar.open;
        let candle_color = if is_bullish {
            options.border_color_bullish
        } else {
            options.border_color_bearish
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
    let crosshair_color = DESIGN_TOKENS.semantic.chart.crosshair_line;
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
        DESIGN_TOKENS.semantic.extended.disabled,
    );
}
