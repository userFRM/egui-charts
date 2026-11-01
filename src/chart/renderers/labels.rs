use super::context::{ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering::axes::{PRICE_LABEL_GAP, axis_font};
use crate::config::{PriceAxisOptions, TimeScaleOptions};
use crate::model::Bar;
use crate::scales::{
    HorizontalScale, PriceFormatter, PriceMap, PriceMark, PriceScaleId, TickMark, TickMarkLayout,
};
use crate::theme::ThemeExt;
use chrono::Datelike;
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
                        None => price_scale.label(value, tick_decimals(price_scale, y), None),
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
        .shared_marks()
        .iter()
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

/// At most four keys per point on either side keep a host's tiny bar spacing bounded.
fn max_whitespace_keys(width: f32) -> usize {
    (4.0 * f64::from(width).ceil()) as usize
}

type TimeAxisPoints = Vec<(chrono::DateTime<chrono::Utc>, usize)>;

/// The tick marks of the time axis from `scale`, the keys they are placed by and the mapping
/// that places the keys. The keys are the bars in view, extended by keys that `time_at` maps
/// from the full chart history: up to ten bars to the left, and into the whitespace two slots
/// past either edge of the view. With no bar in view the keys start at the left edge of the
/// view. Next to the bars the keys skip the weekdays on which the series has no bars
/// (`closed_weekdays`), so a series of weekday bars labels no weekend there. Whitespace on
/// either side gets at most four keys per point of plot width.
///
/// Keys carry unsigned indices, so when they reach before the first bar every index, and the
/// bar indices of the returned mapping, are shifted by the same number of slots.
/// [`render_time_labels`] paints them.
pub(crate) fn time_axis_marks(
    context: &RenderContext,
    visible_data: &[Bar],
    coords: &ChartMapping,
    scale: &dyn HorizontalScale,
    options: &TimeScaleOptions,
    calendar: Option<&crate::model::TimezoneId>,
    time_at: impl Fn(f64) -> Option<chrono::DateTime<chrono::Utc>>,
) -> (Vec<TickMark>, TimeAxisPoints, ChartMapping) {
    // The logical indices at the right and left edges of the plot; the left one is negative
    // when the view reaches before the first bar.
    let right = coords.base_idx as f64 + coords.right_offset;
    let left = (right + 1.0 - f64::from(context.rect.width() / coords.bar_spacing)).floor();
    let first = if visible_data.is_empty() {
        left.max(0.0) as usize
    } else {
        coords.start_idx
    };
    let next = first.saturating_add(visible_data.len());
    // Two keys past the right edge, and at least one past the bars in view.
    let end = ((right.ceil() + 2.0).max(0.0) as usize).max(next.saturating_add(1));
    // Ten keys left of the bars in view keep the marks steady while panning. With whitespace
    // before the first bar in view, the keys reach two slots past the left edge instead, at
    // most four slots per point of plot width before the first bar so that a host's tiny bar
    // spacing cannot build millions of keys.
    let whitespace_limit = max_whitespace_keys(context.rect.width());
    let mut remaining = whitespace_limit;
    let last = coords.base_idx;
    let lead = if left < 0.0 {
        (left - 2.0).max(-(whitespace_limit as f64)) as i64
    } else {
        first.saturating_sub(10) as i64
    };
    let shift = lead.min(0).unsigned_abs() as usize;

    // Extrapolation stops where chrono's range ends, so a bar near it cannot panic.
    let mut bars: Vec<_> = (lead..first as i64)
        .rev()
        .map_while(|index| {
            // A view wholly after the bars can prefetch right whitespace here too.
            if index > last as i64 {
                remaining = remaining.checked_sub(1)?;
            }
            Some((time_at(index as f64)?, (index + shift as i64) as usize))
        })
        .collect();
    bars.reverse();
    bars.extend(
        visible_data
            .iter()
            .enumerate()
            .map(|(i, bar)| (bar.time, first + shift + i)),
    );
    // History keys do not use the budget; prefetched keys after the last bar already have.
    let trailing = last
        .saturating_add(1)
        .saturating_sub(next)
        .saturating_add(remaining);
    bars.extend(
        (next..=end)
            .take(trailing)
            .map_while(|index| Some((time_at(index as f64)?, index + shift))),
    );
    // Next to the bars, a daily series gets no keys on the weekdays it has no bars.
    let weekday = |time| {
        let local = match calendar {
            Some(zone) => crate::model::local_time(zone.clone(), time),
            None => crate::model::local_time(chrono::Local, time),
        };
        local.weekday().num_days_from_monday() as usize
    };
    let after = closed_weekdays(
        last.saturating_sub(15)..=last,
        last as f64 + 1.0,
        &time_at,
        weekday,
    );
    let before = if shift > 0 {
        closed_weekdays(0..=last.min(15), -1.0, &time_at, weekday)
    } else {
        [false; 7]
    };
    bars.retain(|(time, index)| {
        if *index < shift {
            !before[weekday(*time)]
        } else {
            *index <= last + shift || !after[weekday(*time)]
        }
    });
    let layout = TickMarkLayout {
        width: context.rect.width(),
        bar_spacing: coords.bar_spacing,
        min_spacing: min_time_label_spacing(options),
        uniform_distribution: options.uniform_distribution,
    };
    let mapping = ChartMapping {
        start_idx: coords.start_idx + shift,
        base_idx: coords.base_idx + shift,
        ..*coords
    };
    (scale.tick_marks(&bars, &layout), bars, mapping)
}

/// The weekdays, Monday first, on which none of the keys of `bars` falls, when those keys
/// span two weeks or more and the key at `outside`, the slot next to the nearer end of
/// `bars`, lies within a day of it: the days a daily series has no bars, such as the weekend
/// of a market open on weekdays. `bars` are the 16 bars at one end of the series. A weekday
/// counts as closed only when both of its dates in those two weeks lack a bar, so one holiday
/// closes nothing. Weekly and longer intervals, merged daily bars and series shorter than two
/// weeks close no day. `weekday` reads each key's weekday in the chart's calendar zone.
fn closed_weekdays(
    bars: std::ops::RangeInclusive<usize>,
    outside: f64,
    time_at: impl Fn(f64) -> Option<chrono::DateTime<chrono::Utc>>,
    weekday: impl Fn(chrono::DateTime<chrono::Utc>) -> usize,
) -> [bool; 7] {
    let Some((first, last)) = time_at(*bars.start() as f64)
        .zip(time_at(*bars.end() as f64))
        .filter(|(first, last)| *last - *first >= chrono::Duration::days(13))
    else {
        return [false; 7];
    };
    let near = if outside < *bars.start() as f64 {
        first
    } else {
        last
    };
    if time_at(outside).is_none_or(|time| (time - near).abs() > chrono::Duration::days(1)) {
        return [false; 7];
    }
    let mut closed = [true; 7];
    for time in bars.filter_map(|index| time_at(index as f64)) {
        closed[weekday(time)] = false;
    }
    closed
}

fn time_to_idx(
    time: chrono::DateTime<chrono::Utc>,
    bars: &[(chrono::DateTime<chrono::Utc>, usize)],
) -> f64 {
    if bars.is_empty() {
        return 0.0;
    }

    if time <= bars[0].0 {
        return bars[0].1 as f64;
    }

    let last = bars.len() - 1;
    if time >= bars[last].0 {
        return bars[last].1 as f64;
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
    let span_ms = (t1 - t0).num_milliseconds() as f64;
    if span_ms.abs() < f64::EPSILON {
        return i0 as f64;
    }

    let offset_ms = (time - t0).num_milliseconds() as f64;
    let frac = (offset_ms / span_ms).clamp(0.0, 1.0);
    i0 as f64 + frac * (i1 as f64 - i0 as f64)
}

/// Paints time grid lines from the axis marks and an optional price grid, batching matching styles.
pub(crate) fn render_time_grid(
    context: &RenderContext,
    marks: &[TickMark],
    points: &[(chrono::DateTime<chrono::Utc>, usize)],
    coords: &ChartMapping,
    color: Color32,
    style: crate::model::LineStyle,
    horizontal: Option<(Color32, crate::model::LineStyle)>,
) {
    let lines = marks.iter().filter_map(|mark| {
        let index = time_to_idx(mark.time, points);
        let delta = (coords.base_idx as f64 - index) + coords.right_offset;
        let x = (f64::from(context.rect.right())
            - (delta + 0.5) * f64::from(coords.bar_spacing)
            - 1.0) as f32;
        (x >= context.rect.left() && x <= context.rect.right()).then_some([
            Pos2::new(x, context.rect.top()),
            Pos2::new(x, context.rect.bottom()),
        ])
    });
    let price_marks = horizontal
        .map(|_| coords.price.shared_marks())
        .unwrap_or_default();
    let price_lines = price_marks.iter().map(|mark| {
        [
            Pos2::new(context.rect.left(), mark.y_coord),
            Pos2::new(context.rect.right(), mark.y_coord),
        ]
    });
    if let Some((price_color, price_style)) = horizontal {
        if price_color == color && price_style == style {
            // One grid mesh avoids copying it again when tessellation grows for the other axis.
            crate::chart::rendering::grid::render_grid_lines(
                context.painter,
                price_lines.chain(lines),
                color,
                style,
            );
            return;
        }
        crate::chart::rendering::grid::render_grid_lines(
            context.painter,
            price_lines,
            price_color,
            price_style,
        );
    }
    crate::chart::rendering::grid::render_grid_lines(context.painter, lines, color, style);
}

/// Paints the time labels of `marks`, placed by the keys of `points` with the mapping
/// `coords`, as [`time_axis_marks`] returns both, below `context.rect` into `slot`, spaced to
/// prevent overlap, with the label spacing, tick marks, bold labels and border of the time
/// scale `options`.
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
    // Labels centre in the axis row below the plot.
    let y = context.rect.max.y
        + crate::styles::sizing::chart::BOTTOM_PADDING_WITH_TIME.max(options.minimum_height) / 2.0;
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
        let delta_from_right = (coords.base_idx as f64 - index) + coords.right_offset;
        let x = (f64::from(context.rect.min.x) + f64::from(context.rect.width())
            - (delta_from_right + 0.5) * f64::from(coords.bar_spacing)
            - 1.0) as f32;

        if x < context.rect.min.x || x > context.rect.max.x {
            continue;
        }

        // Skip if too close to the last rendered label (prevent overlap)
        if let Some(last_x) = last_label_x
            && (x - last_x).abs() < min_label_spacing
        {
            continue;
        }

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
        let rect = egui::Align2::CENTER_CENTER.anchor_size(Pos2::new(x, y), galley.size());
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

/// Formats text at weight 600 when the face has a weight axis.
pub(super) fn semibold(font: FontId, color: Color32) -> egui::text::TextFormat {
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

    #[test]
    fn combined_grid_preserves_geometry_and_independent_styles() {
        use super::{ChartMapping, RenderContext, render_time_grid};
        use crate::model::LineStyle;
        use crate::scales::{
            PriceMap, PriceRange, PriceTransform, TickMark, TickMarkType, TickMarkWeight,
        };
        use egui::{Color32, LayerId, Mesh, Painter, RawInput, vec2};

        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 240.0));
        let price_rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 180.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(100.0, 200.0),
            price_rect.top(),
            price_rect.bottom(),
            false,
        );
        let coords = ChartMapping::new(price_rect, 6.0, 0, 60, 0.0, price);
        let points = [
            (DateTime::<Utc>::UNIX_EPOCH, 0),
            (
                DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::minutes(60),
                60,
            ),
        ];
        let marks: Vec<_> = [0, 20, 40, 60]
            .map(|index| TickMark {
                time: DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::minutes(index as i64),
                mark_type: TickMarkType::Time,
                weight: TickMarkWeight::MINUTE,
                label: String::new(),
                index,
                bold: false,
            })
            .into();
        let red = Color32::from_rgba_unmultiplied(255, 0, 0, 80);
        for scale in [1.0, 1.25, 2.0] {
            for (horizontal, vertical, color) in [
                (LineStyle::Dotted, LineStyle::Dotted, red),
                (LineStyle::SparseDotted, LineStyle::SparseDotted, red),
                (LineStyle::Solid, LineStyle::Solid, red),
                (LineStyle::Dashed, LineStyle::LargeDashed, red),
                (LineStyle::Dotted, LineStyle::Dotted, Color32::BLUE),
            ] {
                let render = |combined: bool| {
                    let ctx = Context::default();
                    ctx.set_pixels_per_point(scale);
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(rect),
                            ..Default::default()
                        },
                        |_| {
                            let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                            if !combined {
                                crate::chart::rendering::render_grid(
                                    &painter, price_rect, &price, red, horizontal,
                                );
                            }
                            render_time_grid(
                                &RenderContext::new(&painter, rect),
                                &marks,
                                &points,
                                &coords,
                                color,
                                vertical,
                                combined.then_some((red, horizontal)),
                            );
                        },
                    );
                    if combined
                        && horizontal == LineStyle::Dotted
                        && vertical == horizontal
                        && color == red
                    {
                        assert_eq!(
                            output
                                .shapes
                                .iter()
                                .filter(|shape| matches!(shape.shape, Shape::Mesh(_)))
                                .count(),
                            1
                        );
                    }
                    let mut mesh = Mesh::default();
                    for primitive in ctx.tessellate(output.shapes.clone(), output.pixels_per_point)
                    {
                        if let egui::epaint::Primitive::Mesh(part) = primitive.primitive {
                            mesh.append(part);
                        }
                    }
                    output.drop_without_applying_deltas();
                    mesh
                };
                let combined = render(true);
                let separate = render(false);
                assert!(!combined.is_empty());
                assert_eq!(combined.vertices, separate.vertices);
                assert_eq!(combined.indices, separate.indices);
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn pane_and_price_ticks_share_font_and_colour() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            *chart.config_mut() = chart.config().clone().with_theme(&theme);
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

    /// Prices of four or more integer digits group their thousands in the axis ticks, the last
    /// price and crosshair tags and the legend with `ChartConfig::thousands_sep`, and the axis
    /// column widens to fit them. `None`, `'.'` (it would read as the decimal point) and host
    /// formatters keep plain digits, and prices under 1000 never change.
    #[test]
    fn thousands_sep_groups_axis_tag_crosshair_and_legend_prices() {
        use crate::model::Instrument;
        use crate::scales::{DefaultPriceFormatter, PriceScaleId};
        const BIG: (f64, f64) = (1_234_500.0, 1_234_567.89);
        // (low, high, separator, host formatter, last price)
        let rows = [
            (BIG.0, BIG.1, None, false, "1234567.89"),
            (BIG.0, BIG.1, Some(','), false, "1,234,567.89"),
            (BIG.0, BIG.1, Some(' '), false, "1 234 567.89"),
            (BIG.0, BIG.1, Some('.'), false, "1234567.89"),
            (BIG.0, BIG.1, Some(','), true, "1234567.89"),
            (998.5, 999.99, Some(','), false, "999.99"),
            (999.5, 1_000.0, Some(','), false, "1,000.00"),
        ];
        let mut widths = Vec::new();
        for (index, (low, high, separator, host, last)) in rows.into_iter().enumerate() {
            let ctx = Context::default();
            let mut chart = chart(&[low], &[high]);
            chart.config_mut().show_volume = false;
            chart.config_mut().thousands_sep = separator;
            chart.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.01)));
            if host {
                chart.set_price_formatter(Some(std::sync::Arc::new(
                    DefaultPriceFormatter::with_precision(2),
                )));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            widths.push(chart.price_axis_width(PriceScaleId::Right));
            let pointer = chart.get_rendered_price_rect().center();
            let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pointer)]);
            let plot = chart.get_rendered_price_rect();
            let text: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text),
                    _ => None,
                })
                .collect();
            let names: Vec<_> = text.iter().map(|text| text.galley.text()).collect();
            assert!(names.contains(&last), "row {index} last price: {names:?}");
            assert!(
                names.contains(&format!("C {last}").as_str()),
                "row {index} legend: {names:?}"
            );
            let group = separator.filter(|separator| *separator != '.' && !host);
            // The ticks, the last price tag and the crosshair tag: digits right of the plot and
            // above the time axis.
            let axis: Vec<_> = text
                .iter()
                .filter(|text| {
                    text.pos.x > plot.right()
                        && text.pos.y < plot.bottom()
                        && text.galley.text().contains(|c: char| c.is_ascii_digit())
                })
                .collect();
            assert!(!axis.is_empty(), "row {index}: {names:?}");
            for label in &axis {
                let value = label.galley.text();
                assert!(
                    label.pos.x + label.galley.size().x <= plot.right() + widths[index] + 0.5,
                    "row {index}: {value} is cut by the axis column"
                );
                let integer = value.split('.').next().unwrap().trim_start_matches('-');
                match group {
                    Some(separator) => {
                        let groups: Vec<_> = integer.split(separator).collect();
                        assert!(
                            groups.iter().all(|group| {
                                group.bytes().all(|b| b.is_ascii_digit()) && !group.is_empty()
                            }) && (1..=3).contains(&groups[0].len())
                                && groups[1..].iter().all(|group| group.len() == 3),
                            "row {index}: {value}"
                        );
                    }
                    None => assert!(
                        integer.bytes().all(|b| b.is_ascii_digit()),
                        "row {index}: {value}"
                    ),
                }
            }
            if index <= 4 {
                // The crosshair tag spans the pointer's y and hides the ticks under it.
                let tag = axis
                    .iter()
                    .find(|label| {
                        (label.pos.y..=label.pos.y + label.galley.size().y).contains(&pointer.y)
                    })
                    .unwrap_or_else(|| panic!("row {index}: no crosshair tag in {names:?}"));
                assert_eq!(
                    group.is_some_and(|separator| tag.galley.text().contains(separator)),
                    group.is_some(),
                    "row {index}: {}",
                    tag.galley.text()
                );
            }
        }
        assert!(widths[1] > widths[0] && widths[2] > widths[0], "{widths:?}");
        assert_eq!(widths[3], widths[0]);
        assert_eq!(widths[4], widths[0]);
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
                let untagged = Color32::from_rgb(171, 71, 188);
                let id = chart.add_indicator(Box::new(
                    CustomIndicator::new("Colliding", |bars| {
                        vec![
                            IndicatorValue::Multiple(vec![105.0, 105.0, 106.0, f64::NAN, 107.0]);
                            bars.len()
                        ]
                    })
                    .with_overlay(overlay)
                    .with_line_count(5)
                    .with_colors(vec![
                        colors[0],
                        colors[1],
                        Color32::RED,
                        Color32::GREEN,
                        untagged,
                    ]),
                ));
                let study = chart.indicator_mut(id).unwrap();
                let mut settings = study.settings();
                settings.precision = Some(4);
                settings.plots[2].visible = false;
                settings.untagged_plots = vec![4];
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
                assert!(
                    !painted
                        .iter()
                        .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == untagged)),
                    "an untagged plot keeps its value off the scale"
                );
                assert!(!painted.iter().any(|shape| matches!(shape,
                    Shape::Rect(rect) if rect.fill == Color32::RED
                )));
                assert_eq!(
                    tags.len(),
                    if shown && side.is_some() { 2 } else { 0 },
                    "overlay={overlay}"
                );
                if shown && side.is_some() {
                    for tag in &tags {
                        assert_eq!(tag.height(), 17.0);
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
                        assert!((widget.right() - tag.right() - 6.0).abs() < 0.1);
                    }
                }
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
                chart.config = chart.config.clone().with_theme(&theme);
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

    /// Whitespace keys stop at chrono's date limits and stay bounded at a host's tiny minimum
    /// bar spacing, so the chart renders a frame at either limit.
    #[test]
    fn time_axis_bounds_whitespace_at_date_and_spacing_limits() {
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
        // (bar times, host minimum spacing)
        for (times, minimum) in [
            (
                vec![Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), far],
                0.5,
            ),
            (minutes([millis]), 0.5),
            (vec![DateTime::<Utc>::MAX_UTC], 0.5),
            (vec![start, start + chrono::Duration::days(1)], 1e-6),
        ] {
            // Each bar closes ten points above the last, which makes completed one-point bricks.
            let bars = times
                .iter()
                .enumerate()
                .map(|(i, &t)| {
                    let lift = 10.0 * i as f64;
                    Bar::new(t, 10.0, 12.0 + lift, 9.0, 11.0 + lift, 1.0)
                })
                .collect();
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::from_bars(bars));
            let mut options = chart.chart_options().clone();
            options.time_scale.min_bar_spacing = minimum;
            chart.set_chart_options(options);
            if minimum < 0.25 {
                // Price-based charts keep their axis slots unmerged even at tiny spacing.
                chart.set_chart_type(crate::model::ChartType::Renko);
                chart.set_chart_type_params(crate::model::ChartTypeParams {
                    renko_brick_size: 1.0,
                    ..Default::default()
                });
                chart.set_timeframe(crate::model::Timeframe::Day1);
                frame(&ctx, &mut chart, 0.0, vec![]);
                assert!(
                    chart.set_visible_logical_range(crate::model::LogicalRange::new(
                        10_000.0,
                        700_000_000.0,
                    ))
                );
                let coords = chart.get_chart_mapping();
                let painter =
                    egui::Painter::new(ctx.clone(), egui::LayerId::background(), Rect::EVERYTHING);
                // Check the keys as well as rendering the unmerged axis.
                let (_, keys, axis) = super::time_axis_marks(
                    &super::RenderContext::new(&painter, coords.rect),
                    chart.state.visible_data(),
                    &coords,
                    &chart.build_time_formatter(),
                    &chart.chart_options.time_scale,
                    Some(&crate::model::TimezoneId::default()),
                    |index| chart.index_to_time(index),
                );
                let whitespace = keys
                    .iter()
                    .filter(|(_, index)| *index > axis.base_idx)
                    .count();
                assert!(
                    whitespace > 0 && whitespace <= (4.0 * coords.rect.width().ceil()) as usize,
                    "{whitespace} whitespace keys for {} points at spacing {}",
                    coords.rect.width(),
                    coords.bar_spacing
                );
            }
            frame(&ctx, &mut chart, 0.1, vec![]);
        }
    }
}
