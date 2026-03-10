use super::context::{ChartMapping, PriceScale, RenderContext, StyleColors};
use crate::model::Bar;
use crate::scales::{
    PriceMarkGenerator, PriceScaleId, PriceScaleMode, TickMark, TickMarkGenerator,
    TickMarkGeneratorConfig, TickMarkWeight, TimeFormatter,
};
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use chrono::Duration;
use egui::{Color32, FontId, Pos2, Rect, Stroke};

/// Renders price labels on the right side using smart tick mark generation
/// Uses intelligent price mark distribution system
pub fn render_price_labels(
    context: &RenderContext,
    price_scale: &PriceScale,
    colors: &StyleColors,
    scale_mode: PriceScaleMode,
) {
    render_price_labels_at_pos(
        context,
        price_scale,
        colors,
        scale_mode,
        PriceScaleId::Right,
    );
}

/// Renders price labels on the left side using smart tick mark generation
/// Uses intelligent price mark distribution system
pub fn render_price_labels_left(
    context: &RenderContext,
    price_scale: &PriceScale,
    colors: &StyleColors,
    scale_mode: PriceScaleMode,
) {
    render_price_labels_at_pos(context, price_scale, colors, scale_mode, PriceScaleId::Left);
}

/// Renders price labels at the specified position (left or right)
/// Core implementation for price label rendering
pub fn render_price_labels_at_pos(
    context: &RenderContext,
    price_scale: &PriceScale,
    colors: &StyleColors,
    scale_mode: PriceScaleMode,
    position: PriceScaleId,
) {
    // Create price mark generator with default config
    let generator = PriceMarkGenerator::new();

    // Generate smart price marks using "nice numbers" algorithm
    let marks = generator.generate_marks(
        price_scale.min_price,
        price_scale.max_price,
        context.rect.height(),
        scale_mode,
        context.rect.min.y,
        context.rect.max.y,
    );

    // Determine positioning based on scale position
    let (x_offset, alignment) = match position {
        PriceScaleId::Right => (context.rect.max.x + 8.0, egui::Align2::LEFT_CENTER),
        PriceScaleId::Left => (context.rect.min.x - 8.0, egui::Align2::RIGHT_CENTER),
    };

    // Render marks with weight-based styling for visual hierarchy
    for mark in marks {
        // Use larger font for higher weight marks (major boundaries)
        let font_size = if mark.weight >= 80 {
            11.5 // Major marks (e.g., round numbers like 100, 1000)
        } else if mark.weight >= 60 {
            11.0 // Medium marks
        } else {
            10.5 // Minor marks
        };

        // Render label (mark.label is already formatted by the generator)
        // Pos labels in the price axis area with offset for better visibility
        context.painter.text(
            Pos2::new(x_offset, mark.y_coord),
            alignment,
            &mark.label,
            FontId::proportional(font_size),
            colors.text,
        );
    }
}

fn generate_time_marks(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    formatter: Option<&dyn TimeFormatter>,
) -> (Vec<TickMark>, Vec<(chrono::DateTime<chrono::Utc>, usize)>) {
    if visible_data.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut deltas_ms: Vec<i64> = visible_data
        .windows(2)
        .filter_map(|pair| {
            let delta = (pair[1].time - pair[0].time).num_milliseconds().abs();
            if delta > 0 { Some(delta) } else { None }
        })
        .collect();

    let bar_duration_ms = if deltas_ms.is_empty() {
        60_000
    } else {
        deltas_ms.sort_unstable();
        deltas_ms[deltas_ms.len() / 2]
    };

    let zoom_factor = (coords.bar_spacing / 8.0).clamp(0.5, 2.5);
    let min_spacing = (100.0 / zoom_factor).clamp(60.0, 140.0);
    let max_marks = ((context.rect.width() / min_spacing).ceil() as usize + 2).clamp(6, 24);

    let config = TickMarkGeneratorConfig {
        min_spacing,
        max_marks,
        show_subseconds: bar_duration_ms < 1000,
        use_24_hour: true,
        target_density: (1.0 * zoom_factor).clamp(0.6, 2.5),
    };

    let generator = if let Some(fmt) = formatter {
        TickMarkGenerator::with_formatter(config, fmt.clone_box())
    } else {
        TickMarkGenerator::with_config(config)
    };

    let bar_duration_ms = bar_duration_ms.max(1);
    // Safe: guarded by is_empty() check above
    let (Some(first), Some(last)) = (visible_data.first(), visible_data.last()) else {
        return (Vec::new(), Vec::new());
    };
    let first_visible_time = first.time;
    let last_visible_time = last.time;
    let first_visible_idx = coords.start_idx;
    let last_visible_idx = coords.start_idx + visible_data.len().saturating_sub(1);

    // Extend range in BOTH directions to ensure grid lines outside visible data
    // still get correct indices (prevents async movement when panning)
    let extra_left = (coords.start_idx as i64).min(10); // Extend left up to 10 bars or to index 0
    let extra_right = coords.right_offset.ceil().max(0.0) as i64 + 2;

    let start_time =
        first_visible_time - Duration::milliseconds(bar_duration_ms.saturating_mul(extra_left));
    let end_time =
        last_visible_time + Duration::milliseconds(bar_duration_ms.saturating_mul(extra_right));

    let mut bars =
        Vec::with_capacity(extra_left as usize + visible_data.len() + extra_right as usize);

    // Add extrapolated bars BEFORE visible data (for grid lines that fall before visible range)
    for i in (1..=extra_left).rev() {
        let time = first_visible_time - Duration::milliseconds(bar_duration_ms.saturating_mul(i));
        let index = first_visible_idx.saturating_sub(i as usize);
        bars.push((time, index));
    }

    // Add visible data bars
    for (i, bar) in visible_data.iter().enumerate() {
        bars.push((bar.time, coords.start_idx + i));
    }

    // Add extrapolated bars AFTER visible data (for grid lines in the future)
    for i in 1..=extra_right {
        let time = last_visible_time + Duration::milliseconds(bar_duration_ms.saturating_mul(i));
        let index = last_visible_idx.saturating_add(i as usize);
        bars.push((time, index));
    }

    let marks = generator.generate_marks(start_time, end_time, context.rect.width(), &bars);
    (marks, bars)
}

fn time_to_idx(
    time: chrono::DateTime<chrono::Utc>,
    bars: &[(chrono::DateTime<chrono::Utc>, usize)],
) -> f32 {
    if bars.is_empty() {
        return 0.0;
    }

    if time <= bars[0].0 {
        return bars[0].1 as f32;
    }

    let last = bars.len() - 1;
    if time >= bars[last].0 {
        return bars[last].1 as f32;
    }

    let mut lo = 0usize;
    let mut hi = last;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if bars[mid].0 <= time {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let (t0, i0) = bars[lo];
    let (t1, i1) = bars[hi];
    let span_ms = (t1 - t0).num_milliseconds() as f32;
    if span_ms.abs() < f32::EPSILON {
        return i0 as f32;
    }

    let offset_ms = (time - t0).num_milliseconds() as f32;
    let frac = (offset_ms / span_ms).clamp(0.0, 1.0);
    i0 as f32 + frac * (i1 as f32 - i0 as f32)
}

/// Renders vertical grid lines using tick marks
/// Grid lines are anchored to time boundaries and move with the chart
pub fn render_time_grid(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    colors: &StyleColors,
    formatter: Option<&dyn TimeFormatter>,
) {
    let (marks, bars) = generate_time_marks(context, visible_data, coords, formatter);

    for mark in marks {
        // Use fractional index for smooth grid line movement during pan
        let index = time_to_idx(mark.time, &bars);
        // Calculate x using same formula as candle rendering
        let delta_from_right = coords.base_idx as f32 + coords.right_offset - index;
        let x = context.rect.min.x + context.rect.width()
            - (delta_from_right + 0.5) * coords.bar_spacing
            - 1.0;

        if x < context.rect.min.x || x > context.rect.max.x {
            continue;
        }

        let stroke_width = if mark.weight >= TickMarkWeight::MONTH {
            1.0
        } else if mark.weight >= TickMarkWeight::DAY {
            0.7
        } else {
            0.5
        };

        let line_color = if mark.weight >= TickMarkWeight::MONTH {
            Color32::from_rgba_premultiplied(
                colors.grid.r(),
                colors.grid.g(),
                colors.grid.b(),
                (colors.grid.a() as f32 * 1.5).min(255.0) as u8,
            )
        } else {
            colors.grid
        };

        context.painter.line_segment(
            [
                Pos2::new(x, context.rect.min.y),
                Pos2::new(x, context.rect.max.y),
            ],
            Stroke::new(stroke_width, line_color),
        );
    }
}

/// Renders time labels on the bottom using tick marks
/// Labels are spaced to prevent overlap
pub fn render_time_labels(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    colors: &StyleColors,
    formatter: Option<&dyn TimeFormatter>,
) {
    let (marks, bars) = generate_time_marks(context, visible_data, coords, formatter);

    // Track last rendered label position to prevent overlap
    let min_label_spacing = 80.0; // Min pixels between label centers
    let mut last_label_x: Option<f32> = None;

    for mark in marks {
        // Use fractional index for smooth label movement during pan
        let index = time_to_idx(mark.time, &bars);
        // Calculate x using same formula as candle rendering
        let delta_from_right = coords.base_idx as f32 + coords.right_offset - index;
        let x = context.rect.min.x + context.rect.width()
            - (delta_from_right + 0.5) * coords.bar_spacing
            - 1.0;

        if x < context.rect.min.x || x > context.rect.max.x {
            continue;
        }

        // Skip if too close to the last rendered label (prevent overlap)
        if let Some(last_x) = last_label_x
            && (x - last_x).abs() < min_label_spacing
        {
            continue;
        }

        let y = context.rect.max.y + 5.0;
        context.painter.text(
            Pos2::new(x, y),
            egui::Align2::CENTER_TOP,
            &mark.label,
            FontId::proportional(if mark.weight >= TickMarkWeight::MONTH {
                11.0
            } else if mark.weight >= TickMarkWeight::DAY {
                10.5
            } else {
                10.0
            }),
            colors.text,
        );

        last_label_x = Some(x);
    }
}

/// Renders OHLC info header
pub fn render_ohlc_info(
    painter: &egui::Painter,
    rect: Rect,
    visible_data: &[Bar],
    padding: f32,
    text_color: Color32,
) {
    if visible_data.is_empty() {
        return;
    }

    let candle = &visible_data[visible_data.len() - 1];

    let info = format!(
        "open: {:.8}  close: {:.8}  high: {:.8}  low: {:.8}  24h volume: {:.2}",
        candle.open, candle.close, candle.high, candle.low, candle.volume
    );

    painter.text(
        Pos2::new(rect.min.x + padding, rect.min.y + 10.0),
        egui::Align2::LEFT_TOP,
        info,
        FontId::proportional(typography::SM),
        text_color,
    );
}

/// Legend showing symbol info, OHLC values with colors, and change percentage
/// Example: "BTCUSD . 1h   O 43,521.50  H 43,650.00  L 43,400.00  C 43,580.25  +58.75 (+0.14%)"
pub fn render_legend(
    painter: &egui::Painter,
    rect: Rect,
    symbol: &str,
    timeframe: &str,
    visible_data: &[Bar],
    prev_close: Option<f64>,
    colors: &StyleColors,
    padding: f32,
) {
    if visible_data.is_empty() {
        return;
    }

    let candle = &visible_data[visible_data.len() - 1];
    let is_bullish = candle.close >= candle.open;
    let val_color = if is_bullish {
        colors.bullish
    } else {
        colors.bearish
    };

    // Start position
    let mut x = rect.min.x + padding;
    let y = rect.min.y + 12.0;

    // Symbol name (bold, larger)
    let symbol_font = FontId::proportional(typography::MD);
    let symbol_galley =
        painter.layout_no_wrap(symbol.to_string(), symbol_font.clone(), colors.text);
    painter.galley(Pos2::new(x, y - 2.0), symbol_galley.clone(), colors.text);
    x += symbol_galley.rect.width() + 8.0;

    // Bullet separator
    let sep_font = FontId::proportional(typography::SM);
    let sep = painter.layout_no_wrap("•".to_string(), sep_font.clone(), Color32::GRAY);
    painter.galley(Pos2::new(x, y), sep.clone(), Color32::GRAY);
    x += sep.rect.width() + 8.0;

    // Timeframe
    let tf_galley = painter.layout_no_wrap(timeframe.to_string(), sep_font.clone(), colors.text);
    painter.galley(Pos2::new(x, y), tf_galley.clone(), colors.text);
    x += tf_galley.rect.width() + 20.0;

    // Format price helper - auto-detect decimals based on price magnitude
    let format_price = |price: f64| -> String {
        if price >= 100.0 {
            format!("{price:.2}")
        } else if price >= 1.0 {
            format!("{price:.4}")
        } else {
            format!("{price:.6}")
        }
    };

    // OHLC values with labels and color coding
    let label_font = FontId::proportional(typography::SM);
    let val_font = FontId::proportional(typography::SM);
    let label_color = DESIGN_TOKENS.semantic.extended.disabled; // Dim gray for labels

    // O (Open)
    let o_label = painter.layout_no_wrap("O ".to_string(), label_font.clone(), label_color);
    painter.galley(Pos2::new(x, y), o_label.clone(), label_color);
    x += o_label.rect.width();
    let o_val = painter.layout_no_wrap(format_price(candle.open), val_font.clone(), val_color);
    painter.galley(Pos2::new(x, y), o_val.clone(), val_color);
    x += o_val.rect.width() + 12.0;

    // H (High)
    let h_label = painter.layout_no_wrap("H ".to_string(), label_font.clone(), label_color);
    painter.galley(Pos2::new(x, y), h_label.clone(), label_color);
    x += h_label.rect.width();
    let h_val = painter.layout_no_wrap(format_price(candle.high), val_font.clone(), val_color);
    painter.galley(Pos2::new(x, y), h_val.clone(), val_color);
    x += h_val.rect.width() + 12.0;

    // L (Low)
    let l_label = painter.layout_no_wrap("L ".to_string(), label_font.clone(), label_color);
    painter.galley(Pos2::new(x, y), l_label.clone(), label_color);
    x += l_label.rect.width();
    let l_val = painter.layout_no_wrap(format_price(candle.low), val_font.clone(), val_color);
    painter.galley(Pos2::new(x, y), l_val.clone(), val_color);
    x += l_val.rect.width() + 12.0;

    // C (Close)
    let c_label = painter.layout_no_wrap("C ".to_string(), label_font.clone(), label_color);
    painter.galley(Pos2::new(x, y), c_label.clone(), label_color);
    x += c_label.rect.width();
    let c_val = painter.layout_no_wrap(format_price(candle.close), val_font.clone(), val_color);
    painter.galley(Pos2::new(x, y), c_val.clone(), val_color);
    x += c_val.rect.width() + 16.0;

    // Change amount and percentage (if we have previous close)
    let reference_price = prev_close.unwrap_or(candle.open);
    let change = candle.close - reference_price;
    let change_pct = if reference_price != 0.0 {
        (change / reference_price) * 100.0
    } else {
        0.0
    };

    let change_color = if change >= 0.0 {
        colors.bullish
    } else {
        colors.bearish
    };
    let sign = if change >= 0.0 { "+" } else { "" };

    let change_text = format!("{sign}{change:.2} ({sign}{change_pct:.2}%)");
    let change_galley = painter.layout_no_wrap(change_text, val_font, change_color);
    painter.galley(Pos2::new(x, y), change_galley, change_color);
}
