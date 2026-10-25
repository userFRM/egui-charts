use super::context::{ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering::axes::{PRICE_LABEL_GAP, TIME_LABEL_GAP, axis_font};
use crate::config::{PriceAxisOptions, TimeScaleOptions};
use crate::model::Bar;
use crate::scales::{
    HorizontalScale, PriceFormatter, PriceMap, PriceMark, PriceScaleId, TickMark, TickMarkLayout,
};
use crate::theme::ThemeExt;
use chrono::Duration;
use egui::layers::ShapeIdx;
use egui::{Color32, FontId, Pos2, Rect, Shape};

/// Length of the tick marks on the price and time axes, from the plot edge.
const AXIS_TICK_LENGTH: f32 = 3.0;

/// Paints the price labels on the right side into `slot`, using smart tick mark generation and
/// leaving out the labels the `tags` (the price line, last price and crosshair tags) would cut
/// through. `axis` sets the border, tick marks and label clipping. A host `formatter` writes the
/// labels with [`PriceFormatter::format_tick`].
pub fn render_price_labels(
    context: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    axis: &PriceAxisOptions,
    tags: &[Option<Rect>],
    slot: ShapeIdx,
    formatter: Option<&dyn PriceFormatter>,
) {
    render_price_labels_at_pos(
        context,
        price_scale,
        colors,
        axis,
        PriceScaleId::Right,
        tags,
        slot,
        formatter,
    );
}

/// Paints the price labels at the specified position (left or right) into `slot`
/// Core implementation for price label rendering
pub fn render_price_labels_at_pos(
    context: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    axis: &PriceAxisOptions,
    position: PriceScaleId,
    tags: &[Option<Rect>],
    slot: ShapeIdx,
    formatter: Option<&dyn PriceFormatter>,
) {
    let mut marks = price_marks(price_scale, formatter);

    // Determine positioning based on scale position
    let (edge, side, alignment) = match position {
        PriceScaleId::Right => (context.rect.max.x, 1.0, egui::Align2::LEFT_CENTER),
        PriceScaleId::Left => (context.rect.min.x, -1.0, egui::Align2::RIGHT_CENTER),
    };
    let x_offset = edge + side * PRICE_LABEL_GAP;

    let font = axis_font(context.painter.ctx());
    if axis.edge_ticks_visible {
        // Edge labels sit half a line inside the pane, so they show whole.
        let inset = context
            .painter
            .layout_no_wrap("0".to_owned(), font.clone(), colors.text)
            .size()
            .y
            / 2.0;
        // A pane too short for two labels gets none at its edges.
        if context.rect.height() >= inset * 4.0 {
            let edges = [context.rect.top() + inset, context.rect.bottom() - inset];
            // An edge label replaces the marks closer to it than they are to each other.
            let step = marks
                .windows(2)
                .map(|pair| (pair[1].y_coord - pair[0].y_coord).abs())
                .reduce(f32::min)
                .unwrap_or(inset * 4.0);
            marks.retain(|mark| edges.iter().all(|y| (mark.y_coord - y).abs() >= step));
            marks.extend(edges.map(|y| {
                let value = price_scale.y_to_scale(y);
                PriceMark {
                    price: value,
                    label: match formatter {
                        Some(formatter) => {
                            formatter.format_tick(price_scale.transform().axis_value(value))
                        }
                        None => price_scale.transform().label(
                            value,
                            tick_decimals(price_scale, y),
                            None,
                        ),
                    },
                    y_coord: y,
                    weight: 0,
                }
            }));
        }
    }

    let painter = if axis.hide_clipped_labels {
        context.painter.clone()
    } else {
        // Labels at the pane edges show cut, not spilling over the neighbouring pane or axis.
        let clip = context.painter.clip_rect();
        context.painter.with_clip_rect(Rect::from_x_y_ranges(
            clip.x_range(),
            context.rect.y_range().intersection(clip.y_range()),
        ))
    };
    let mut shapes = Vec::new();
    if axis.border_visible {
        let color = axis
            .border_color
            .unwrap_or_else(|| context.painter.ctx().active_theme().ui.border);
        shapes.push(Shape::line_segment(
            [
                Pos2::new(edge + side * 0.5, context.rect.top()),
                Pos2::new(edge + side * 0.5, context.rect.bottom()),
            ],
            egui::Stroke::new(1.0, color),
        ));
    }
    for mark in marks {
        let galley = if mark.weight >= 80 {
            painter.layout_job(egui::text::LayoutJob::single_section(
                mark.label,
                semibold(font.clone(), colors.text),
            ))
        } else {
            painter.layout_no_wrap(mark.label, font.clone(), colors.text)
        };
        let rect = alignment.anchor_size(Pos2::new(x_offset, mark.y_coord), galley.size());
        let shown = if axis.hide_clipped_labels {
            rect.top() >= context.rect.top()
                && rect.bottom() <= context.rect.bottom()
                && painter.clip_rect().contains_rect(rect)
        } else {
            painter.clip_rect().intersects(rect)
        };
        if !shown || under_tag(tags, rect) {
            continue;
        }
        if axis.ticks_visible {
            shapes.push(Shape::line_segment(
                [
                    Pos2::new(edge, mark.y_coord),
                    Pos2::new(edge + side * AXIS_TICK_LENGTH, mark.y_coord),
                ],
                egui::Stroke::new(1.0, colors.text),
            ));
        }
        shapes.push(Shape::galley(rect.min, galley, colors.text));
    }
    painter.set(slot, shapes);
}

/// Price axis marks of `price_scale` from the "nice numbers" generator, labelled with
/// [`PriceFormatter::format_tick`] when the host supplies a `formatter`.
pub(crate) fn price_marks(
    price_scale: &PriceMap,
    formatter: Option<&dyn PriceFormatter>,
) -> Vec<PriceMark> {
    let mut marks = price_scale.marks();
    if let Some(formatter) = formatter {
        for mark in &mut marks {
            mark.label = formatter.format_tick(mark.price);
        }
    }
    marks
}

/// Whether a label at `rect` is left out: a tag takes the place of the axis labels it overlaps.
fn under_tag(tags: &[Option<Rect>], rect: Rect) -> bool {
    tags.iter().flatten().any(|tag| tag.intersects(rect))
}

/// Decimals of the price axis tick label nearest to `y`, so that a price written with them reads
/// like the ticks.
pub(crate) fn tick_decimals(price_scale: &PriceMap, y: f32) -> usize {
    price_scale
        .marks()
        .into_iter()
        .min_by(|a, b| (a.y_coord - y).abs().total_cmp(&(b.y_coord - y).abs()))
        // Percentage labels end in a sign, which is not a decimal.
        .map_or(2, |mark| {
            mark.label.split_once('.').map_or(0, |(_, decimals)| {
                decimals.chars().take_while(char::is_ascii_digit).count()
            })
        })
}

/// The minimum distance between time labels: 80 points for labels of up to 8 characters.
fn min_time_label_spacing(options: &TimeScaleOptions) -> f32 {
    match options.tick_mark_max_character_length {
        0 => 80.0,
        chars => 10.0 * chars as f32,
    }
}

/// The tick marks of the time axis from `scale`, and the keys they are placed by: the bars in
/// view with their logical indices, extended into the whitespace on either side by keys
/// extrapolated at the median step. [`render_time_labels`] paints them.
pub(crate) fn time_axis_marks(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    scale: &dyn HorizontalScale,
    options: &TimeScaleOptions,
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
    let layout = TickMarkLayout {
        width: context.rect.width(),
        bar_spacing: coords.bar_spacing,
        min_spacing: min_time_label_spacing(options),
        uniform_distribution: options.uniform_distribution,
    };
    (scale.tick_marks(&bars, &layout), bars)
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

/// Paints time grid lines from the same calendar marks as the axis labels.
pub(crate) fn render_time_grid(
    context: &RenderContext,
    marks: &[TickMark],
    points: &[(chrono::DateTime<chrono::Utc>, usize)],
    coords: &ChartMapping,
    color: Color32,
    style: crate::model::LineStyle,
) {
    for mark in marks {
        let index = time_to_idx(mark.time, points);
        let delta = coords.base_idx as f32 + coords.right_offset - index;
        let x = context.rect.right() - (delta + 0.5) * coords.bar_spacing - 1.0;
        if x >= context.rect.left() && x <= context.rect.right() {
            crate::chart::rendering::grid::render_grid_line(
                context.painter,
                Pos2::new(x, context.rect.top()),
                Pos2::new(x, context.rect.bottom()),
                egui::Stroke::new(0.5, color),
                style,
            );
        }
    }
}

/// Paints the time labels of `marks`, placed by the keys of `points` as [`time_axis_marks`]
/// returns them, below `context.rect` into `slot`, spaced to prevent overlap, with the label
/// spacing, tick marks, bold labels and border of the time scale `options`.
pub fn render_time_labels(
    context: &RenderContext,
    marks: Vec<TickMark>,
    points: &[(chrono::DateTime<chrono::Utc>, usize)],
    coords: &ChartMapping,
    colors: &StyleColors,
    tags: &[Option<Rect>],
    options: &TimeScaleOptions,
    slot: ShapeIdx,
) {
    let font = axis_font(context.painter.ctx());
    let theme = context.painter.ctx().active_theme();
    let text_color = if colors.text == theme.chart.axis_text {
        theme.chart.axis_text_secondary
    } else {
        colors.text
    };

    // Track last rendered label position to prevent overlap
    let min_label_spacing = min_time_label_spacing(options); // Min pixels between label centers
    let mut last_label_x: Option<f32> = None;
    let mut labels = Vec::new();
    if options.border_visible {
        let y = context.painter.round_to_pixel_center(context.rect.bottom());
        labels.push(Shape::hline(
            context.rect.x_range(),
            y,
            egui::Stroke::new(1.0, options.border_color.unwrap_or(theme.ui.border)),
        ));
    }

    for mark in marks {
        // Use fractional index for smooth label movement during pan
        let index = time_to_idx(mark.time, points);
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
        let galley = if options.allow_bold_labels && mark.bold {
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
            if options.ticks_visible {
                labels.push(Shape::line_segment(
                    [
                        Pos2::new(x, context.rect.bottom()),
                        Pos2::new(x, context.rect.bottom() + AXIS_TICK_LENGTH),
                    ],
                    egui::Stroke::new(1.0, text_color),
                ));
            }
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

#[path = "legend/mod.rs"]
mod legend;
pub(crate) use legend::study_controls;
pub use legend::{Legend, LegendStatus, QuoteSide, StudyAction};

/// Formats text at weight 600 when the face has a weight axis.
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

    #[cfg(feature = "studies")]
    #[test]
    fn pane_and_price_ticks_share_font_and_colour() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            *chart.config_mut() = theme.apply_to_config(chart.config().clone());
            chart.config_mut().show_symbol_last_val = false;
            chart.config_mut().show_volume = false;
            chart.config_mut().show_indicator_last_val = false;
            chart.set_instrument(Some(
                crate::model::Instrument::new("SAMPLE").with_price_decimals(5),
            ));
            chart.add_indicator(Box::new(crate::studies::RSI::default()));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let pane = &chart.get_rendered_indicator_panes()[0];
            let ticks: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.pos.x > pane.chart_rect.right()
                            && text.galley.text().parse::<f64>().is_ok() =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            assert!(ticks.iter().any(|text| text.pos.y > pane.panel_rect.top()));
            assert!(ticks.iter().any(|text| text.pos.y < pane.panel_rect.top()));
            for tick in ticks {
                let format = &tick.galley.job.sections[0].format;
                assert_eq!(format.font_id, super::axis_font(&ctx));
                if tick.pos.y > pane.panel_rect.top() {
                    assert_eq!(
                        tick.galley
                            .text()
                            .split_once('.')
                            .map_or(0, |(_, decimals)| decimals.len()),
                        2
                    );
                }
                assert_eq!(
                    format.color,
                    theme.chart.axis_text,
                    "{}",
                    tick.galley.text()
                );
            }
        }
    }

    #[test]
    fn currency_button_reserves_ticks_and_opens_its_menu() {
        use crate::model::Instrument;
        for left in [false, true] {
            for currency in [None, Some(""), Some("USD")] {
                let ctx = Context::default();
                let mut chart = chart(&[1.0], &[2.0]);
                chart.config_mut().show_volume = false;
                chart.config_mut().right_axis_width = 0.0;
                chart.config_mut().left_axis_width = 0.0;
                chart.config_mut().show_right_axis = !left;
                chart.config_mut().show_left_axis = left;
                chart.set_instrument(currency.map(|code| {
                    Instrument::new("SAMPLE")
                        .with_currency(code)
                        .with_price_decimals(0)
                }));
                frame(&ctx, &mut chart, 0.0, vec![]);
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let header = painted.iter().find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text() == "USD" => Some(text),
                    _ => None,
                });
                assert_eq!(
                    header.is_some(),
                    currency.is_some_and(|code| !code.is_empty())
                );
                if let Some(header) = header {
                    assert_eq!(
                        header.galley.job.sections[0].format.font_id,
                        super::axis_font(&ctx)
                    );
                    let plot = chart.get_rendered_price_rect();
                    assert!(
                        (header.pos.y + header.galley.size().y / 2.0 - plot.top() - 12.0).abs()
                            < 0.1
                    );
                    let box_rect = painted
                        .iter()
                        .find_map(|shape| match shape {
                            Shape::Rect(rect)
                                if rect.rect.height() == 24.0 && rect.rect.contains(header.pos) =>
                            {
                                Some(rect.rect)
                            }
                            _ => None,
                        })
                        .expect("currency backdrop");
                    assert_eq!(
                        box_rect.right(),
                        if left {
                            plot.left()
                        } else {
                            plot.right()
                                + chart
                                    .config()
                                    .right_axis_width
                                    .max(chart.measured_axis_width)
                        }
                    );
                    for shape in &painted {
                        if let Shape::Text(text) = shape
                            && text.galley.text().parse::<f64>().is_ok()
                            && text.pos.x >= box_rect.left()
                        {
                            assert!(
                                !Rect::from_min_size(text.pos, text.galley.size())
                                    .intersects(box_rect)
                            );
                        }
                    }
                    let pos = box_rect.center();
                    let events = vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ];
                    frame(&ctx, &mut chart, 0.2, events);
                    let popup = shapes(&ctx, &mut chart, 0.3, vec![]);
                    assert_eq!(popup.iter().filter(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "USD")).count(), 2);
                }
            }
        }
    }

    /// Each price axis option draws its part: the border, a tick beside each label, labels at the
    /// top and bottom of the pane, and labels the pane edges cut.
    #[test]
    fn price_axis_options_draw_border_ticks_and_edge_labels() {
        use crate::config::PriceAxisOptions;
        let on = PriceAxisOptions {
            border_visible: true,
            ticks_visible: true,
            ..Default::default()
        };
        let edges = PriceAxisOptions {
            edge_ticks_visible: true,
            ..Default::default()
        };
        let clipped = PriceAxisOptions {
            hide_clipped_labels: false,
            ..Default::default()
        };
        // (options, border, a tick per label, labels at both edges, a label cut by the pane)
        for (axis, border, ticks, edge_labels, cut) in [
            (PriceAxisOptions::default(), false, false, false, false),
            (on, true, true, false, false),
            (edges, false, false, true, false),
            (clipped, false, false, false, true),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[120.0], &[180.0]);
            chart.config_mut().show_volume = false;
            chart.config_mut().show_symbol_last_val = false;
            chart.config_mut().price_axis = axis;
            // Ticks at 100 and 200 fall on the bottom and top of the pane.
            chart.price_scale_mut().set_range(100.0, 200.0);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let labels: Vec<Rect> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.pos.x > plot.right()
                            && text.galley.text().parse::<f64>().is_ok() =>
                    {
                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .collect();
            let segments: Vec<[Pos2; 2]> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, .. } => Some(*points),
                    _ => None,
                })
                .collect();
            let has_border = segments.iter().any(|[a, b]| {
                a.x == plot.right() + 0.5 && b.x == a.x && a.y == plot.top() && b.y == plot.bottom()
            });
            let tick_count = segments
                .iter()
                .filter(|[a, b]| a.x == plot.right() && b.x == plot.right() + 3.0 && a.y == b.y)
                .count();
            let at_edges = labels
                .iter()
                .any(|label| (label.top() - plot.top()).abs() < 0.01)
                && labels
                    .iter()
                    .any(|label| (label.bottom() - plot.bottom()).abs() < 0.01);
            let cut_label = labels.iter().any(|label| {
                label.top() < plot.top() - 0.01 || label.bottom() > plot.bottom() + 0.01
            });
            assert!(!labels.is_empty(), "{axis:?}");
            assert_eq!(
                (
                    has_border,
                    tick_count > 0 && tick_count == labels.len(),
                    at_edges,
                    cut_label
                ),
                (border, ticks, edge_labels, cut),
                "{axis:?}"
            );
        }
    }

    #[test]
    fn tick_metadata_controls_axis_and_legend_decimals() {
        use crate::model::Instrument;
        for (tick, price, decimals, explicit) in [
            (0.25, 125.25, 2, None),
            (0.00001, 1.23456, 5, None),
            (1e-18, 3e-18, 18, None),
            (1.0, 100.0, 0, None),
            (0.01, 0.0, 2, None),
            (0.25, 125.25, 4, Some(4)),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[price], &[price]);
            chart.config_mut().show_volume = false;
            chart.config_mut().show_indicator_last_val = false;
            #[cfg(feature = "studies")]
            chart.add_indicator(Box::new(crate::studies::MACD::default()));
            let mut instrument = Instrument::new("SAMPLE").with_tick_size(tick);
            instrument.price_decimals = explicit;
            chart.set_instrument(Some(instrument));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let text: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            let expected = format!("{price:.decimals$}");
            assert!(
                text.contains(&expected.as_str()),
                "last price {expected}: {text:?}"
            );
            assert!(
                text.contains(&format!("C {expected}").as_str()),
                "legend {expected}: {text:?}"
            );
            #[cfg(feature = "studies")]
            if decimals == 0 {
                for pane in chart.get_rendered_indicator_panes() {
                    let labels: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text)
                                if text.pos.x > pane.chart_rect.right()
                                    && text.pos.y > pane.panel_rect.top()
                                    && text.galley.text().parse::<f64>().is_ok() =>
                            {
                                Some(text.galley.text())
                            }
                            _ => None,
                        })
                        .collect();
                    let unique: std::collections::HashSet<_> = labels.iter().collect();
                    assert_eq!(
                        unique.len(),
                        labels.len(),
                        "pane ticks must not repeat rounded values: {labels:?}"
                    );
                }
            }
            let plot = chart.get_rendered_price_rect();
            for shape in &painted {
                if let Shape::Text(text) = shape
                    && text.pos.x > plot.right()
                    && text.galley.text().parse::<f64>().is_ok()
                {
                    assert_eq!(
                        text.galley
                            .text()
                            .split_once('.')
                            .map_or(0, |(_, fraction)| fraction.len()),
                        decimals
                    );
                }
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn study_axis_tags_follow_visible_last_values_and_separate_collisions() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use egui::Color32;
        for (overlay, shown) in [(true, true), (false, true), (true, false), (false, false)] {
            for side in [Some(false), Some(true), None] {
                let ctx = Context::default();
                let mut chart = chart(&[100.0], &[110.0]);
                chart.config_mut().show_right_axis = side == Some(false);
                chart.config_mut().show_left_axis = side == Some(true);
                chart.config_mut().show_indicator_last_val = shown;
                chart.config_mut().show_symbol_last_val = false;
                chart.config_mut().show_volume = false;
                let colors = [
                    Color32::from_rgb(41, 98, 255),
                    Color32::from_rgb(255, 167, 38),
                ];
                let id = chart.add_indicator(Box::new(
                    CustomIndicator::new("Colliding", |bars| {
                        vec![
                            IndicatorValue::Multiple(vec![105.0, 105.0, 106.0, f64::NAN]);
                            bars.len()
                        ]
                    })
                    .with_overlay(overlay)
                    .with_line_cnt(4)
                    .with_colors(vec![
                        colors[0],
                        colors[1],
                        Color32::RED,
                        Color32::GREEN,
                    ]),
                ));
                let study = chart.indicator_mut(id).unwrap();
                let mut settings = study.settings();
                settings.precision = Some(4);
                settings.plots[2].visible = false;
                study.apply_settings(&[], settings).unwrap();
                frame(&ctx, &mut chart, 0.0, vec![]);
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let plot = if overlay {
                    chart.get_rendered_price_rect()
                } else {
                    chart.get_rendered_indicator_panes()[0].chart_rect
                };
                let tags: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect) if colors.contains(&rect.fill) => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                assert!(!painted.iter().any(|shape| matches!(shape,
                    Shape::Text(text) if text.galley.text() == "106.0000"
                )));
                assert_eq!(
                    tags.len(),
                    if shown && side.is_some() { 2 } else { 0 },
                    "overlay={overlay}"
                );
                if shown && side.is_some() {
                    for tag in &tags {
                        assert_eq!(tag.height(), 15.0);
                        if side == Some(true) {
                            assert!((plot.left() - tag.right() - 5.0).abs() < 0.1);
                        } else {
                            assert!((tag.left() - plot.right() - 5.0).abs() < 0.1);
                        }
                        assert!(tag.top() >= plot.top() && tag.bottom() <= plot.bottom());
                        let text = painted
                            .iter()
                            .find_map(|shape| match shape {
                                Shape::Text(text)
                                    if tag.contains_rect(Rect::from_min_size(
                                        text.pos,
                                        text.galley.size(),
                                    )) =>
                                {
                                    Some(text)
                                }
                                _ => None,
                            })
                            .expect("value text inside its tag");
                        assert_eq!(text.galley.text(), "105.0000");
                        assert_eq!(text.galley.job.sections[0].format.color, Color32::WHITE);
                    }
                    let separation = (tags[0].center().y - tags[1].center().y).abs();
                    assert!(
                        (separation - (tags[0].height() + tags[1].height()) / 2.0).abs() < 0.01
                    );
                }
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn measured_axis_keeps_wide_tags_inside_shared_plot_insets() {
        use crate::model::{Instrument, PriceLine};
        use crate::studies::{CustomIndicator, IndicatorValue};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [320.0, 800.0] {
                for pane_widest in [false, true] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut chart = chart(&[100.0], &[110.0]);
                    chart.set_instrument(Some(Instrument::new("SAMPLE").with_price_decimals(8)));
                    chart.config_mut().show_volume = false;
                    chart.config_mut().show_symbol_last_val = false;
                    chart.config_mut().show_indicator_last_val = false;
                    if pane_widest {
                        chart.add_indicator(Box::new(CustomIndicator::new("Wide pane", |bars| {
                            vec![IndicatorValue::Single(12345678.0); bars.len()]
                        })));
                    } else {
                        chart.set_price_lines(
                            crate::widget::SeriesId::MAIN,
                            vec![PriceLine::new(105.0, egui::Color32::BLUE)],
                        );
                        chart.add_indicator(Box::new(crate::studies::RSI::default()));
                    }
                    let mut widget = Rect::NOTHING;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(width, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                widget = chart.show(ui).rect;
                            });
                        },
                    );
                    let painted = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let plot = chart.get_rendered_price_rect();
                    assert!((plot.left() - widget.left()).abs() <= 8.0);
                    assert!(plot.width() > 0.0);
                    for pane in chart.get_rendered_indicator_panes() {
                        assert_eq!(pane.chart_rect.x_range(), plot.x_range());
                    }
                    for shape in &painted {
                        if let Shape::Text(text) = &shape.shape
                            && text.pos.x > plot.right()
                            && text.galley.text().parse::<f64>().is_ok()
                        {
                            assert!(
                                text.pos.x + text.galley.size().x <= widget.right() - 8.0,
                                "axis text must fit on its first frame"
                            );
                        }
                    }
                    if !pane_widest {
                        let tag = painted
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                Shape::Rect(rect) if rect.fill == egui::Color32::BLUE => {
                                    Some(rect.rect)
                                }
                                _ => None,
                            })
                            .expect("custom price tag");
                        assert!((tag.left() - plot.right() - 5.0).abs() < 0.1);
                        assert!((widget.right() - tag.right() - 8.0).abs() < 0.1);
                    }
                }
            }
        }
    }

    /// Legend fields stay inside the plot and keep their positions during hover.
    #[test]
    fn legend_fields_stay_inside_the_plot() {
        use crate::theme::apply_to_egui;
        use egui::Vec2;

        for (_, theme) in crate::theme::presets() {
            for width in [160.0, 300.0, 387.0, 400.0, 599.0, 600.0, 1200.0, 1440.0] {
                let ctx = Context::default();
                #[cfg(feature = "icons")]
                egui_extras::install_image_loaders(&ctx);
                apply_to_egui(&ctx, &theme);
                let plot = Rect::from_min_size(Pos2::new(52.0, 40.0), Vec2::new(width, 400.0));
                let badge = ctx.load_texture(
                    "instrument",
                    egui::ColorImage::filled([18, 18], egui::Color32::BLUE),
                    Default::default(),
                );
                let statuses = [super::LegendStatus {
                    text: "S",
                    color: theme.ui.text,
                    background: theme.ui.panel_bg_secondary,
                }];
                let instrument = crate::model::Instrument::new("Sample equity fund")
                    .with_data_status(crate::model::DataStatus::Delayed);
                let bar = Bar::new(
                    Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    749.37,
                    751.09,
                    749.13,
                    750.74,
                    275_540.0,
                );
                let frame = |events| {
                    ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let mut legend = super::Legend::new("Sample equity fund", "1h", &bar);
                            legend.previous_close = Some(749.42);
                            legend.badge = Some(egui::ImageSource::Texture(
                                (badge.id(), Vec2::splat(18.0)).into(),
                            ));
                            legend.statuses = &statuses;
                            legend.instrument = Some(&instrument);
                            legend.show_volume = false;
                            legend.show(ui, plot);
                        },
                    )
                };
                let mut output = frame(vec![Event::PointerGone]);
                output.textures_delta.clear();
                let text: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect();
                assert!(!text.is_empty());
                for shape in &output.shapes {
                    if let Shape::Mesh(mesh) = &shape.shape
                        && mesh.texture_id == badge.id()
                    {
                        assert!(plot.shrink(8.0).contains_rect(shape.clip_rect));
                    }
                }
                for (index, field) in text.iter().enumerate() {
                    let bounds = Rect::from_min_size(field.pos, field.galley.size());
                    for other in &text[index + 1..] {
                        assert!(
                            !bounds.intersects(Rect::from_min_size(other.pos, other.galley.size()))
                        );
                    }
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            Shape::Rect(rect) if rect.fill == theme.chart.bg && rect.rect.height() >= 22.0 && rect.rect.contains_rect(bounds)
                        )), "missing row backdrop for {}", field.galley.text());
                    assert!(
                        bounds.right() <= plot.right() - 8.0 + 0.1,
                        "{width}: {} at {bounds:?}",
                        field.galley.text()
                    );
                }
                for shape in &output.shapes {
                    if let Shape::Rect(rect) = &shape.shape
                        && rect.fill == theme.chart.bg
                        && rect.rect.height() >= 22.0
                    {
                        assert!(
                            text.iter().any(|field| rect
                                .rect
                                .intersects(Rect::from_min_size(field.pos, field.galley.size()))),
                            "{width}: empty legend row at {:?}",
                            rect.rect
                        );
                    }
                }
                for (index, field) in ["O 749.37", "H 751.09", "L 749.13", "C 750.74"]
                    .into_iter()
                    .enumerate()
                {
                    assert_eq!(
                        text.iter().any(|text| text.galley.text() == field),
                        width >= 600.0 || index == 3,
                        "{width}: field visibility for {field}"
                    );
                }
                assert!(
                    text.iter()
                        .any(|text| text.galley.text() == "+1.32 (+0.18%)")
                );
                assert!(text.iter().any(|text| text.galley.text() == "D"
                    && text.galley.job.sections[0].format.color == theme.ui.warning));
                let centres: Vec<_> = text
                    .iter()
                    .map(|text| text.pos.y + text.galley.size().y / 2.0)
                    .collect();
                if width >= 1200.0 {
                    assert!(centres.iter().all(|y| (*y - centres[0]).abs() < 0.1));
                } else if width <= 300.0 {
                    assert!(centres.iter().any(|y| *y > centres[0]));
                }
                for field in text
                    .iter()
                    .filter(|text| text.galley.text().starts_with("O "))
                {
                    let sections = &field.galley.job.sections;
                    assert_eq!(sections[0].format.color, theme.ui.text_secondary);
                    assert_eq!(sections[1].format.color, theme.chart.candle_up);
                }
                let idle: Vec<_> = text
                    .iter()
                    .map(|text| (text.galley.text().to_owned(), text.pos, text.galley.size()))
                    .collect();
                let hovered = frame(vec![Event::PointerMoved(plot.min + Vec2::new(60.0, 20.0))]);
                let hovered_text: Vec<_> = hovered
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => {
                            Some((text.galley.text().to_owned(), text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .collect();
                hovered.drop_without_applying_deltas();
                output.drop_without_applying_deltas();
                assert_eq!(idle, hovered_text, "hover moved readout at {width}");
            }
        }
    }

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
                        && rect.rect.center().y < plot.max.y
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
        chart.config.show_volume = false;
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
