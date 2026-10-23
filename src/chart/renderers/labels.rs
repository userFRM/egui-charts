use super::context::{ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering::axes::{PRICE_LABEL_GAP, TIME_LABEL_GAP, axis_font};
use crate::model::{Bar, percent_change};
use crate::scales::{
    PriceMap, PriceMarkGenerator, PriceScaleId, TickMark, TickMarkGenerator,
    TickMarkGeneratorConfig, TickMarkType, TimeFormatter,
};
use crate::styles::typography;
use crate::theme::ThemeExt;
use chrono::Duration;
use egui::layers::ShapeIdx;
use egui::{Color32, FontId, Pos2, Rect, Shape};

/// Paints the price labels on the right side into `slot`, using smart tick mark generation and
/// leaving out the labels the `tags` (the price line, last price and crosshair tags) would cut
/// through.
pub fn render_price_labels(
    context: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    tags: &[Option<Rect>],
    slot: ShapeIdx,
) {
    render_price_labels_at_pos(
        context,
        price_scale,
        colors,
        PriceScaleId::Right,
        tags,
        slot,
    );
}

/// Paints the price labels at the specified position (left or right) into `slot`
/// Core implementation for price label rendering
pub fn render_price_labels_at_pos(
    context: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    position: PriceScaleId,
    tags: &[Option<Rect>],
    slot: ShapeIdx,
) {
    // Create price mark generator with default config
    let generator = PriceMarkGenerator::new();

    // Generate smart price marks using "nice numbers" algorithm
    let marks = generator.generate_marks(price_scale);

    // Determine positioning based on scale position
    let (x_offset, alignment) = match position {
        PriceScaleId::Right => (
            context.rect.max.x + PRICE_LABEL_GAP,
            egui::Align2::LEFT_CENTER,
        ),
        PriceScaleId::Left => (
            context.rect.min.x - PRICE_LABEL_GAP,
            egui::Align2::RIGHT_CENTER,
        ),
    };

    let font = axis_font(context.painter.ctx());
    let labels: Vec<Shape> = marks
        .into_iter()
        .filter_map(|mark| {
            let galley = context
                .painter
                .layout_no_wrap(mark.label, font.clone(), colors.text);
            let rect = alignment.anchor_size(Pos2::new(x_offset, mark.y_coord), galley.size());
            (rect.top() >= context.rect.top()
                && rect.bottom() <= context.rect.bottom()
                && context.painter.clip_rect().contains_rect(rect)
                && !under_tag(tags, rect))
            .then(|| Shape::galley(rect.min, galley, colors.text))
        })
        .collect();
    context.painter.set(slot, labels);
}

/// Whether a label at `rect` is left out: a tag takes the place of the axis labels it overlaps.
fn under_tag(tags: &[Option<Rect>], rect: Rect) -> bool {
    tags.iter().flatten().any(|tag| tag.intersects(rect))
}

/// Decimals of the price axis tick label nearest to `y`, so that a price written with them reads
/// like the ticks.
pub(crate) fn tick_decimals(price_scale: &PriceMap, y: f32) -> usize {
    PriceMarkGenerator::new()
        .generate_marks(price_scale)
        .into_iter()
        .min_by(|a, b| (a.y_coord - y).abs().total_cmp(&(b.y_coord - y).abs()))
        // Normal and logarithmic tick labels are plain numbers.
        .map_or(2, |mark| {
            mark.label
                .split_once('.')
                .map_or(0, |(_, decimals)| decimals.len())
        })
}

fn generate_time_marks(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    formatter: &dyn TimeFormatter,
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

    let generator = TickMarkGenerator::with_formatter(config, formatter.clone_box());

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
    let extra_right = coords
        .right_offset
        .ceil()
        .max(0.0)
        .min((context.rect.width() / coords.bar_spacing).ceil()) as i64
        + 2;

    // Extrapolation stops where chrono's range ends, so a bar near it cannot panic.
    let step = |i: i64| Duration::milliseconds(bar_duration_ms.saturating_mul(i));
    // Extrapolated bars BEFORE the visible data (for grid lines that fall before the visible range)
    let mut bars: Vec<_> = (1..=extra_left)
        .map_while(|i| {
            let time = first_visible_time.checked_sub_signed(step(i))?;
            Some((time, first_visible_idx.saturating_sub(i as usize)))
        })
        .collect();
    bars.reverse();
    bars.extend(
        visible_data
            .iter()
            .enumerate()
            .map(|(i, bar)| (bar.time, coords.start_idx + i)),
    );
    // Extrapolated bars AFTER the visible data (for grid lines in the future)
    bars.extend((1..=extra_right).map_while(|i| {
        let time = last_visible_time.checked_add_signed(step(i))?;
        Some((time, last_visible_idx.saturating_add(i as usize)))
    }));
    let (start_time, end_time) = (bars[0].0, bars[bars.len() - 1].0);

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

/// Renders time labels on the bottom using tick marks
/// Labels are spaced to prevent overlap
pub fn render_time_labels(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    colors: &StyleColors,
    formatter: &dyn TimeFormatter,
    tags: &[Option<Rect>],
    slot: ShapeIdx,
) {
    let (marks, bars) = generate_time_marks(context, visible_data, coords, formatter);
    let font = axis_font(context.painter.ctx());
    let theme = context.painter.ctx().active_theme();
    let text_color = if colors.text == theme.chart.axis_text {
        theme.chart.axis_text_secondary
    } else {
        colors.text
    };

    // Track last rendered label position to prevent overlap
    let min_label_spacing = 80.0; // Min pixels between label centers
    let mut last_label_x: Option<f32> = None;
    let mut labels = Vec::new();

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

        let y = context.rect.max.y + TIME_LABEL_GAP;
        let galley = if mark.mark_type == TickMarkType::Year {
            context
                .painter
                .layout_job(egui::text::LayoutJob::single_section(
                    mark.label,
                    semibold(font.clone(), text_color),
                ))
        } else {
            context
                .painter
                .layout_no_wrap(mark.label, font.clone(), text_color)
        };
        let rect = egui::Align2::CENTER_TOP.anchor_size(Pos2::new(x, y), galley.size());
        if rect.left() >= context.rect.left()
            && rect.right() <= context.rect.right()
            && context.painter.clip_rect().contains_rect(rect)
            && !under_tag(tags, rect)
        {
            labels.push(Shape::galley(rect.min, galley, text_color));
        }

        last_label_x = Some(x);
    }
    context.painter.set(slot, labels);
}

/// Decimals for a price by its magnitude: 2 from 100 up, 4 from 1 up, 6 below 1.
pub(crate) fn price_decimals(price: f64) -> usize {
    if price.abs() >= 100.0 {
        2
    } else if price.abs() >= 1.0 {
        4
    } else {
        6
    }
}

/// Formats a header price with [`price_decimals`].
fn format_price(price: f64) -> String {
    let decimals = price_decimals(price);
    format!("{price:.decimals$}")
}

/// Renders OHLC info header
pub fn render_ohlc_info(
    painter: &egui::Painter,
    rect: Rect,
    visible_data: &[Bar],
    padding: f32,
    text_color: Color32,
    show_volume: bool,
) {
    if visible_data.is_empty() {
        return;
    }

    let candle = &visible_data[visible_data.len() - 1];

    let mut info = format!(
        "open: {}  close: {}  high: {}  low: {}",
        format_price(candle.open),
        format_price(candle.close),
        format_price(candle.high),
        format_price(candle.low)
    );

    if show_volume {
        info.push_str(&format!("  volume: {:.2}", candle.volume));
    }

    painter.text(
        Pos2::new(rect.min.x + padding, rect.min.y + 10.0),
        egui::Align2::LEFT_TOP,
        info,
        FontId::proportional(typography::MD),
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
    // Text takes the theme's status colours, which read at 4.5:1, rather than the candle colours.
    let theme = painter.ctx().active_theme();
    let val_color = if is_bullish {
        theme.ui.success
    } else {
        theme.ui.error
    };

    // Start position
    let mut x = rect.min.x + padding;
    let y = rect.min.y + 12.0;

    // Symbol name (semibold)
    let symbol_font = FontId::proportional(typography::MD);
    let symbol_galley = painter.layout_job(egui::text::LayoutJob::single_section(
        symbol.to_string(),
        semibold(symbol_font, colors.text),
    ));
    painter.galley(Pos2::new(x, y), symbol_galley.clone(), colors.text);
    x += symbol_galley.rect.width() + 8.0;

    // Bullet separator, in the colour of the O, H, L and C labels
    let label_color = theme.chart.axis_text_secondary;
    let sep_font = FontId::proportional(typography::MD);
    let sep = painter.layout_no_wrap("•".to_string(), sep_font.clone(), label_color);
    painter.galley(Pos2::new(x, y), sep.clone(), label_color);
    x += sep.rect.width() + 8.0;

    // Timeframe
    let tf_galley = painter.layout_no_wrap(timeframe.to_string(), sep_font.clone(), colors.text);
    painter.galley(Pos2::new(x, y), tf_galley.clone(), colors.text);
    x += tf_galley.rect.width() + 20.0;

    // OHLC values with labels and color coding
    let label_font = FontId::proportional(typography::MD);
    let val_font = FontId::proportional(typography::MD);

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
    let change_pct = percent_change(reference_price, candle.close);

    let change_color = if change >= 0.0 {
        theme.ui.success
    } else {
        theme.ui.error
    };
    let sign = if change >= 0.0 { "+" } else { "" };

    let change_text = format!("{sign}{change:.2} ({sign}{change_pct:.2}%)");
    let change_galley = painter.layout_no_wrap(change_text, val_font, change_color);
    painter.galley(Pos2::new(x, y), change_galley, change_color);
}

/// Formats a symbol or year mark at weight 600 when the face has a weight axis.
fn semibold(font: FontId, color: Color32) -> egui::text::TextFormat {
    let mut format = egui::text::TextFormat::simple(font, color);
    format.coords.push("wght", 600.0);
    format
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarData};
    use crate::widget::Chart;
    use crate::widget::test_frames::{chart, frame, shapes};
    use chrono::{DateTime, TimeZone, Utc};
    use egui::{Context, Event, Pos2, Rect, Shape};

    /// The tick labels and the tags one frame paints in the price axis, with the pointer at
    /// `hover`. A number inside a tag is the tag's own text, not a tick.
    fn price_axis(
        ctx: &Context,
        chart: &mut Chart,
        hover: Option<Pos2>,
    ) -> (Vec<(String, Rect)>, Vec<Rect>) {
        let time = ctx.input(|i| i.time) + 0.1;
        let shapes = shapes(
            ctx,
            chart,
            time,
            hover.map(Event::PointerMoved).into_iter().collect(),
        );

        let plot = chart.get_chart_mapping().rect;
        let tags: Vec<Rect> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect)
                    if rect.fill.a() > 0
                        && rect.rect.max.x > plot.max.x
                        && rect.rect.height() < 40.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        let ticks = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text)
                    if text.pos.x > plot.max.x && text.galley.text().parse::<f64>().is_ok() =>
                {
                    let rect = Rect::from_min_size(text.pos, text.galley.size());
                    (!tags.iter().any(|tag| tag.contains_rect(rect)))
                        .then(|| (text.galley.text().to_owned(), rect))
                }
                _ => None,
            })
            .collect();
        (ticks, tags)
    }

    /// A tick label that the last price tag or the crosshair price label overlaps is left out,
    /// and every other tick label stays.
    #[test]
    fn tags_hide_the_tick_labels_they_overlap() {
        let ctx = Context::default();
        // The last bar closes at 197, a few pixels below the 200 tick.
        let mut chart = chart(&[100.0], &[200.0, 197.0]);
        chart.config_mut().show_symbol_last_val = false;
        price_axis(&ctx, &mut chart, None);
        let (reference, tags) = price_axis(&ctx, &mut chart, None);
        assert!(
            tags.is_empty() && reference.len() >= 3,
            "{reference:?} {tags:?}"
        );
        let plot = chart.get_chart_mapping().rect;
        let middle = reference[reference.len() / 2].1;
        let tick_y = middle.center().y;
        // One pixel more than a label's height: the text rects overlap only through the tag's padding.
        let pad = middle.height() + 1.0;

        // (last price tag shown, pointer offset from a tick label's centre)
        for case in [
            (true, None),
            (false, Some(0.0)),
            (false, Some(pad)),
            (false, Some(-pad)),
        ] {
            let (last_price, offset) = case;
            chart.config_mut().show_symbol_last_val = last_price;
            let hover = offset.map(|dy| Pos2::new(plot.center().x, tick_y + dy));
            price_axis(&ctx, &mut chart, hover);
            let (ticks, tags) = price_axis(&ctx, &mut chart, hover);
            let kept: Vec<_> = reference
                .iter()
                .filter(|(_, rect)| !tags.iter().any(|tag| tag.intersects(*rect)))
                .cloned()
                .collect();
            assert!(
                !tags.is_empty() && kept.len() < reference.len(),
                "{case:?}: {tags:?}"
            );
            assert_eq!(ticks, kept, "{case:?}: {tags:?}");
        }
    }

    #[test]
    fn axis_ticks_fit_and_keep_theme_or_custom_colours() {
        use crate::theme::{apply_to_egui, presets};
        use egui::Color32;

        for (name, theme) in presets() {
            for custom in [None, Some(Color32::from_rgb(170, 40, 190))] {
                let ctx = Context::default();
                apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0], &[200.0]);
                chart.config = theme.apply_to_config(chart.config.clone());
                if let Some(custom) = custom {
                    chart.config.text_color = custom;
                }
                shapes(&ctx, &mut chart, 0.0, Vec::new());
                chart.state.time_scale_mut().set_right_offset(-17.5);
                chart.state.price_scale_mut().set_range(100.0, 200.0);
                chart.config.show_symbol_last_val = false;
                let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
                let plot = chart.get_rendered_price_rect();
                let prices: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text)
                            if text.pos.x > plot.right()
                                && text.galley.text().parse::<f64>().is_ok() =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!prices.is_empty());
                for text in prices {
                    assert!(
                        text.pos.y >= plot.top()
                            && text.pos.y + text.galley.size().y <= plot.bottom(),
                        "price tick crosses plot edge: {}",
                        text.galley.text()
                    );
                }
                let ticks: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text) if text.pos.y >= plot.bottom() => Some(text),
                        _ => None,
                    })
                    .collect();
                assert!(!ticks.is_empty(), "{name}: no time ticks");
                for text in &ticks {
                    assert!(
                        text.pos.x >= plot.left()
                            && text.pos.x + text.galley.size().x <= plot.right(),
                        "time tick crosses plot edge: {}",
                        text.galley.text()
                    );
                }
                let expected = custom.unwrap_or(theme.chart.axis_text_secondary);
                assert!(
                    ticks.iter().all(|text| text.fallback_color == expected),
                    "{name}: {custom:?}"
                );
            }
        }
    }

    /// One frame renders without panicking for bars stamped at or near the end of chrono's date
    /// range, where the time axis extrapolates times past the range.
    #[test]
    fn time_axis_survives_timestamps_at_the_end_of_the_date_range() {
        let start = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let minutes = |extra: [DateTime<Utc>; 1]| {
            vec![start, start + chrono::Duration::minutes(1)]
                .into_iter()
                .chain(extra)
                .collect::<Vec<_>>()
        };
        let far = Utc.with_ymd_and_hms(60_000, 1, 1, 0, 0, 0).unwrap();
        // A millisecond timestamp read as seconds.
        let millis = DateTime::from_timestamp(1_710_496_800_000, 0).unwrap();
        for times in [
            vec![Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), far],
            minutes([millis]),
            vec![DateTime::<Utc>::MAX_UTC],
        ] {
            let bars = times
                .iter()
                .map(|&t| Bar::new(t, 10.0, 12.0, 9.0, 11.0, 1.0))
                .collect();
            frame(
                &Context::default(),
                &mut Chart::new(BarData::from_bars(bars)),
                0.1,
                vec![],
            );
        }
    }
}
