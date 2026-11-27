mod gradient;

use crate::chart::rendering::{ChartMapping, RenderContext};
use crate::model::Bar;
use crate::model::LineStyle;
use crate::painting::dashed_line::{draw_dashed_polyline, draw_dotted_polyline};
use crate::scales::PriceMap;
use crate::studies::{
    CrossDirection, Indicator, IndicatorLineStyle, PlotStyle, PlotType, ShapeSide,
};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke};

/// Indicator Renderer
/// Renders indicator lines overlay on the chart
pub struct IndicatorRenderer;

/// Paint one study plot through `points`: one uninterrupted run for a type with breaks, the
/// defined values across gaps for a joined type.
pub(crate) fn paint_study_plot(
    painter: &egui::Painter,
    points: &[Pos2],
    plot: PlotStyle,
    baseline: f32,
) {
    if !painter.clip_rect().is_positive() {
        return;
    }
    let baseline = if baseline.is_finite() {
        baseline.clamp(painter.clip_rect().top(), painter.clip_rect().bottom())
    } else {
        painter.clip_rect().bottom()
    };
    match plot.plot_type {
        PlotType::StepLine | PlotType::StepLineWithBreaks | PlotType::StepLineWithDiamonds => {
            let mut steps = Vec::new();
            for (i, point) in points.iter().enumerate() {
                if i > 0 {
                    steps.push(Pos2::new(point.x, points[i - 1].y));
                }
                steps.push(*point);
            }
            paint_study_plot(
                painter,
                &steps,
                PlotStyle {
                    plot_type: PlotType::Line,
                    ..plot
                },
                baseline,
            );
            if plot.plot_type == PlotType::StepLineWithDiamonds {
                // A diamond marks the first value and each change of value.
                for (i, &point) in points.iter().enumerate() {
                    if i == 0 || point.y != points[i - 1].y {
                        paint_diamond(painter, point, plot.width + 2.0, plot.color);
                    }
                }
            }
        }
        PlotType::Area | PlotType::AreaWithBreaks => {
            // The fill lies under the line at a fifth of its opacity, so the guides and grid
            // beneath stay visible.
            let fill = plot.color.gamma_multiply(0.2);
            for pair in points.windows(2) {
                paint_band(
                    painter,
                    pair[0],
                    pair[1],
                    Pos2::new(pair[0].x, baseline),
                    Pos2::new(pair[1].x, baseline),
                    fill,
                    None,
                );
            }
            paint_study_plot(
                painter,
                points,
                PlotStyle {
                    plot_type: PlotType::Line,
                    ..plot
                },
                baseline,
            );
        }
        PlotType::Crosses => {
            for &point in points {
                for delta in [
                    egui::vec2(plot.width + 1.0, 0.0),
                    egui::vec2(0.0, plot.width + 1.0),
                ] {
                    painter
                        .line_segment([point - delta, point + delta], Stroke::new(1.0, plot.color));
                }
            }
        }
        PlotType::Character(character) => {
            let galley = painter.layout_no_wrap(
                character.to_string(),
                egui::FontId::proportional(character_size(plot.width)),
                plot.color,
            );
            for &point in points {
                let at = egui::Align2::CENTER_CENTER
                    .anchor_size(point, galley.size())
                    .min;
                painter.galley(at, galley.clone(), plot.color);
            }
        }
        PlotType::TriangleUp | PlotType::TriangleDown => {
            let sign = if plot.plot_type == PlotType::TriangleUp {
                -1.0
            } else {
                1.0
            };
            for &point in points {
                let size = plot.width + 2.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        point + egui::vec2(0.0, sign * size),
                        point + egui::vec2(-size, -sign * size),
                        point + egui::vec2(size, -sign * size),
                    ],
                    plot.color,
                    Stroke::NONE,
                ));
            }
        }
        PlotType::BackgroundColor => {
            for point in points {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        Pos2::new(point.x - plot.width * 0.5, painter.clip_rect().top()),
                        Pos2::new(point.x + plot.width * 0.5, painter.clip_rect().bottom()),
                    ),
                    0.0,
                    plot.color,
                );
            }
        }
        PlotType::BarColor => {}
        PlotType::Line | PlotType::LineWithBreaks => match plot.pattern {
            LineStyle::Solid => {
                painter.add(egui::Shape::line(
                    points.to_vec(),
                    Stroke::new(plot.width, plot.color),
                ));
            }
            LineStyle::Dashed | LineStyle::LargeDashed => {
                let scale = if plot.pattern == LineStyle::Dashed {
                    1.0
                } else {
                    2.0
                };
                draw_dashed_polyline(
                    painter,
                    points,
                    Stroke::new(plot.width, plot.color),
                    6.0 * scale,
                    4.0 * scale,
                );
            }
            LineStyle::Dotted | LineStyle::SparseDotted => {
                let spacing = if plot.pattern == LineStyle::Dotted {
                    4.0
                } else {
                    8.0
                };
                draw_dotted_polyline(painter, points, plot.color, spacing, plot.width * 0.5);
            }
        },
        PlotType::Histogram | PlotType::Columns => {
            for point in points {
                painter.rect_filled(
                    egui::Rect::from_two_pos(
                        Pos2::new(point.x - plot.width * 0.5, baseline),
                        Pos2::new(point.x + plot.width * 0.5, point.y),
                    ),
                    0.0,
                    plot.color,
                );
            }
        }
        PlotType::Circles => {
            for point in points {
                painter.circle_filled(*point, plot.width, plot.color);
            }
        }
    }
}

/// A filled diamond centred on `at`, `half` from the centre to each corner.
fn paint_diamond(painter: &egui::Painter, at: Pos2, half: f32, color: Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            at - egui::vec2(0.0, half),
            at + egui::vec2(half, 0.0),
            at + egui::vec2(0.0, half),
            at - egui::vec2(half, 0.0),
        ],
        color,
        Stroke::NONE,
    ));
}

/// Points between a triangle drawn beside its value and that value.
const TRIANGLE_GAP: f32 = 2.0;

/// The centre of the triangle a plot of stroke `width` draws for `value` at `at`, the value's
/// point: `at` itself when centred, else moved along the pane until the triangle's near edge
/// lies `TRIANGLE_GAP` beyond the value, toward higher prices for `Above` and lower ones for
/// `Below`, whichever way `to_y` runs.
pub(crate) fn triangle_centre(
    at: Pos2,
    value: f64,
    width: f32,
    side: ShapeSide,
    to_y: impl Fn(f64) -> f32,
) -> Pos2 {
    // A price beyond the value on its side, far enough to leave its point on every scale.
    let beyond = match side {
        ShapeSide::Centred => return at,
        ShapeSide::Above => value + value.abs().max(1.0),
        ShapeSide::Below => value - value.abs().max(1.0),
    };
    let step = to_y(beyond) - at.y;
    // A flat range maps every price to one y: then lower prices lie down the pane.
    let down = if step == 0.0 || step.is_nan() {
        side == ShapeSide::Below
    } else {
        step > 0.0
    };
    // Half the painted triangle's height (paint_study_plot), then the gap.
    let shift = width + 2.0 + TRIANGLE_GAP;
    at + egui::vec2(0.0, if down { shift } else { -shift })
}

/// The font size of a character plot of stroke `width`: its half, the width plus the hit
/// tolerance, is the radius within which a click selects a marker plot.
pub(crate) fn character_size(width: f32) -> f32 {
    2.0 * (width + crate::chart::hit_test::HIT_TOLERANCE)
}

/// Split crossing boundaries at their intersection, so neither triangle crosses the band.
fn paint_band(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    c: Pos2,
    d: Pos2,
    color: Color32,
    inverted: Option<Color32>,
) {
    let polygon = |points: Vec<Pos2>, below: bool| {
        let color = if below {
            inverted.unwrap_or(color)
        } else {
            color
        };
        // Adjacent bands share edges; antialiasing each polygon leaves visible seams.
        let mut mesh = egui::Mesh::default();
        for point in points {
            mesh.colored_vertex(point, color);
        }
        for index in 2..mesh.vertices.len() as u32 {
            mesh.add_triangle(0, index - 1, index);
        }
        painter.add(egui::Shape::mesh(mesh));
    };
    let (left, right) = (a.y - c.y, b.y - d.y);
    if (left < 0.0 && right > 0.0) || (left > 0.0 && right < 0.0) {
        let t = left / (left - right);
        let crossing = a.lerp(b, t);
        polygon(vec![a, crossing, c], left > 0.0);
        polygon(vec![crossing, b, d], right > 0.0);
    } else {
        polygon(vec![a, b, d, c], left + right > 0.0);
    }
}

fn paint_fills(
    painter: &egui::Painter,
    study: &dyn Indicator,
    range: std::ops::Range<usize>,
    map: &PriceMap,
    coords: &ChartMapping,
) {
    use crate::studies::FillBoundary;
    for fill in study.fills() {
        // Colour selection uses values so inverted price maps retain the same meaning.
        let colors = if map.y_to_price(coords.rect.top()) < map.y_to_price(coords.rect.bottom()) {
            (fill.inverted_color.unwrap_or(fill.color), Some(fill.color))
        } else {
            (fill.color, fill.inverted_color)
        };
        if let (FillBoundary::Level(a), FillBoundary::Level(b)) = (fill.first, fill.second) {
            if !a.is_finite() || !b.is_finite() {
                continue;
            }
            let (a, b) = (map.price_to_y(a), map.price_to_y(b));
            if a.is_finite() && b.is_finite() {
                paint_band(
                    painter,
                    Pos2::new(coords.rect.left(), a),
                    Pos2::new(coords.rect.right(), a),
                    Pos2::new(coords.rect.left(), b),
                    Pos2::new(coords.rect.right(), b),
                    colors.0,
                    colors.1,
                );
            }
            continue;
        }
        let value = |boundary, bar| match boundary {
            FillBoundary::Level(value) => value.is_finite().then_some(value),
            FillBoundary::Plot(line) => (line < study.line_count())
                .then(|| study.plot_value(line, bar))
                .flatten(),
        };
        for bar in range.start..range.end.saturating_sub(1) {
            let Some((a, b, c, d)) = value(fill.first, bar)
                .zip(value(fill.first, bar + 1))
                .zip(value(fill.second, bar))
                .zip(value(fill.second, bar + 1))
                .map(|(((a, b), c), d)| (a, b, c, d))
            else {
                continue;
            };
            let (x, next_x) = (coords.idx_to_x(bar), coords.idx_to_x(bar + 1));
            let points = [
                Pos2::new(x, map.price_to_y(a)),
                Pos2::new(next_x, map.price_to_y(b)),
                Pos2::new(x, map.price_to_y(c)),
                Pos2::new(next_x, map.price_to_y(d)),
            ];
            if points.iter().all(|point| point.is_finite()) {
                paint_band(
                    painter, points[0], points[1], points[2], points[3], colors.0, colors.1,
                );
            }
        }
    }
}

impl IndicatorRenderer {
    pub fn render(
        context: &RenderContext,
        indicators: &[Box<dyn Indicator>],
        bars: &[Bar],
        price_scale: &PriceMap,
        coords: &ChartMapping,
    ) {
        for indicator in indicators {
            if !indicator.is_visible() {
                continue;
            }

            // Only render overlay indicators
            if !indicator.is_overlay()
                || (indicator.is_volume()
                    && !matches!(indicator.scale(), crate::studies::StudyScale::Shared(_)))
            {
                continue;
            }

            let mapping = super::study_mapping(
                indicator.as_ref(),
                &ChartMapping {
                    price: *price_scale,
                    ..coords.clone()
                },
            );
            if let Some(color) = indicator.background() {
                context
                    .painter
                    .with_clip_rect(context.rect)
                    .rect_filled(context.rect, 0.0, color);
            }
            Self::render_study(
                context,
                indicator.as_ref(),
                bars,
                coords.start_idx,
                &mapping.price,
                &mapping,
            );
        }
    }

    /// Shared study geometry for overlays and indicator panes. Each paints the study's tint first,
    /// beneath what else it paints there.
    pub(crate) fn render_study(
        context: &RenderContext,
        indicator: &dyn Indicator,
        bars: &[Bar],
        bars_start: usize,
        price_scale: &PriceMap,
        coords: &ChartMapping,
    ) {
        let painter = context.painter.with_clip_rect(context.rect);
        if indicator.is_overlay() {
            crate::chart::indicators::plots::paint_overlay_levels(&painter, indicator, price_scale);
        }
        #[cfg(feature = "drawings")]
        if !indicator.drawings().is_empty() {
            crate::chart::indicators::plots::paint_drawings(
                context,
                indicator,
                price_scale,
                coords,
            );
        }
        if !indicator.candles().is_empty() {
            crate::chart::indicators::plots::paint_candles(context, indicator, price_scale, coords);
            crate::chart::indicators::plots::paint_price_lines(&painter, indicator, price_scale);
            return;
        }
        let visible = crate::chart::rendering::volume_profile::profile_range(coords);
        for plot in indicator.volume_profiles(visible.clone()) {
            crate::painting::volume_profile::paint_profile(
                &context.painter.with_clip_rect(context.rect),
                &plot.profile,
                plot.config,
                plot.color,
                crate::chart::rendering::volume_profile::profile_span(&plot, coords),
                crate::painting::volume_profile::ProfileStyle::Study {
                    right_aligned: plot.right_aligned,
                    strip: plot.strip.is_some(),
                    walls_visible: if plot.profile.walls.is_some() {
                        [3, 4].map(|line| indicator.plot_style(line).visible)
                    } else {
                        [true; 2]
                    },
                    width: plot.width,
                    volume_decimals: plot.volume_decimals,
                },
                &|price| price_scale.price_to_y(price),
            );
        }
        // Levels lie over every profile they cross.
        for level in indicator.profile_levels(visible) {
            crate::painting::volume_profile::paint_level(
                &painter,
                &level,
                crate::chart::rendering::volume_profile::level_span(&level, coords),
                &|price| price_scale.price_to_y(price),
            );
        }
        let values = indicator.values();
        let painter = context.painter.with_clip_rect(context.rect);
        let display_start = coords
            .start_idx
            .min(coords.x_to_idx(context.rect.left()).saturating_sub(1));
        let display_end = coords.x_to_idx(context.rect.right()).saturating_add(2);
        paint_fills(
            &painter,
            indicator,
            display_start..display_end,
            price_scale,
            coords,
        );
        gradient::paint(
            &painter,
            indicator,
            display_start..display_end,
            price_scale,
            coords,
        );
        crate::chart::indicators::plots::paint_zones(&painter, indicator, price_scale, coords);
        crate::chart::indicators::projection::paint(&painter, indicator, coords, price_scale);

        // Handle multi-line indicators
        let line_count = indicator.line_count();
        let bar_spacing = coords.bar_spacing_for_width();
        let ctx = context.painter.ctx();
        // Only the layer under the pointer shows a tooltip, as for host markers.
        let hover = ctx.pointer_hover_pos().filter(|&pos| {
            context.rect.contains(pos) && ctx.layer_id_at(pos) == Some(context.painter.layer_id())
        });
        let mut tooltip_shown = false;

        for line_idx in 0..line_count {
            // Plot 0 is painted as the drawings it styles.
            #[cfg(feature = "drawings")]
            if line_idx == 0
                && !indicator.drawings().is_empty()
                && indicator.drawings_use_plot_style()
            {
                continue;
            }
            let mut plot = indicator.plot_style(line_idx);
            if matches!(
                plot.plot_type,
                PlotType::Columns | PlotType::BackgroundColor
            ) || (indicator.is_volume() && plot.plot_type == PlotType::Histogram)
            {
                plot.width = bar_spacing
                    * if plot.plot_type != PlotType::BackgroundColor {
                        0.8
                    } else {
                        1.0
                    };
            }
            if !plot.visible {
                continue;
            }
            // A step line with diamonds walks as a step line; its diamonds are painted per bar
            // below, each in its own bar's colour.
            let diamonds = plot.plot_type == PlotType::StepLineWithDiamonds;
            if diamonds {
                plot.plot_type = PlotType::StepLine;
            }
            let joins = plot.plot_type.joins_gaps();
            // Triangles that mark a bar's high or low are drawn beside their value.
            let side = if matches!(
                plot.plot_type,
                PlotType::TriangleUp | PlotType::TriangleDown
            ) {
                indicator.plot_shape_side(line_idx)
            } else {
                ShapeSide::Centred
            };
            let label_side = indicator.plot_shape_side(line_idx);
            let mut color = plot.color;
            let offset = indicator.plot_offset(line_idx);
            let source_start = display_start
                .checked_add_signed(offset.saturating_neg())
                .unwrap_or(0);
            let source_end = display_end
                .checked_add_signed(offset.saturating_neg())
                .unwrap_or(0)
                .min(values.len());
            // A joined plot also walks from the defined value before the view to the one after
            // it, so a gap across either edge is bridged.
            // shortcut: scans a gap across an edge on every frame, O(gap length); keep the
            // nearest defined bars per line if long gaps show in frame profiles.
            let defined = |i: usize| {
                i.checked_add_signed(offset)
                    .is_some_and(|bar| indicator.plot_value(line_idx, bar).is_some())
            };
            let walk = if joins {
                let before = (0..source_start).rev().find(|&i| defined(i));
                let after = (source_end..values.len()).find(|&i| defined(i));
                // A value beyond one edge joins the path through a value in view or beyond the
                // other edge; alone it would carry the plot's label into a view it is not in.
                if before.is_some() == after.is_some() || (source_start..source_end).any(defined) {
                    before.unwrap_or(source_start)..after.map_or(source_end, |i| i + 1)
                } else {
                    source_start..source_end
                }
            } else {
                source_start..source_end
            };
            let walk_start = walk.start;

            let mut style = indicator.line_style(line_idx);
            let mut points = Vec::new();
            let provisional = indicator.provisional_after(line_idx);
            let mut dashed = false;

            // values[i] belongs to bar i of the full series and `bars` are the
            // visible ones from coords.start_idx, so only that slice is walked.
            let visible_end = source_end;
            for i in walk {
                let Some(display_bar) = i.checked_add_signed(offset) else {
                    continue;
                };
                let price = indicator.plot_value(line_idx, display_bar);
                // A joined plot passes over a missing value: the path neither ends nor changes
                // colour or width there.
                if joins && price.is_none() {
                    continue;
                }
                let x = coords.idx_to_x(display_bar);
                if !dashed && provisional.is_some_and(|after| i > after) {
                    dashed = true;
                    // The dashes start at the value on `after`, the last final one.
                    if matches!(plot.plot_type, PlotType::Line | PlotType::LineWithBreaks)
                        && plot.pattern == LineStyle::Solid
                    {
                        if !points.is_empty() {
                            Self::draw_line(
                                context,
                                &points,
                                color,
                                style,
                                PlotStyle { color, ..plot },
                                price_scale.price_to_y(0.0),
                            );
                            let last = points.last().copied();
                            points.clear();
                            points.extend(last);
                        }
                        plot.pattern = LineStyle::Dashed;
                    }
                }
                let next_color = indicator.plot_color(line_idx, i).unwrap_or(plot.color);
                let next_width = if matches!(
                    plot.plot_type,
                    PlotType::Columns | PlotType::BackgroundColor
                ) || (indicator.is_volume()
                    && plot.plot_type == PlotType::Histogram)
                {
                    plot.width
                } else {
                    indicator.plot_width(line_idx, i)
                };
                if (next_color != color || next_width != plot.width) && !points.is_empty() {
                    Self::draw_line(
                        context,
                        &points,
                        color,
                        style,
                        PlotStyle { color, ..plot },
                        price_scale.price_to_y(0.0),
                    );
                    let last = points.last().copied();
                    points.clear();
                    if matches!(
                        plot.plot_type,
                        PlotType::Line
                            | PlotType::LineWithBreaks
                            | PlotType::StepLine
                            | PlotType::StepLineWithBreaks
                            | PlotType::Area
                            | PlotType::AreaWithBreaks
                    ) {
                        points.extend(last);
                    }
                }
                color = next_color;
                plot.width = next_width;
                style.width = next_width;
                if plot.plot_type == PlotType::BarColor {
                    if values.value(line_idx, i).is_some()
                        && let Some(bar) = display_bar
                            .checked_sub(bars_start)
                            .and_then(|i| bars.get(i))
                    {
                        let width = bar_spacing * 0.8;
                        let open = price_scale.price_to_y(bar.open);
                        let close = price_scale.price_to_y(bar.close);
                        let top = open.min(close);
                        let bottom = open.max(close);
                        let high = price_scale.price_to_y(bar.high);
                        let low = price_scale.price_to_y(bar.low);
                        if [top, bottom, high, low].iter().all(|y| y.is_finite()) {
                            painter.line_segment(
                                [Pos2::new(x, high), Pos2::new(x, low)],
                                Stroke::new(1.0, color),
                            );
                            painter.rect_filled(
                                egui::Rect::from_two_pos(
                                    Pos2::new(x - width * 0.5, top),
                                    Pos2::new(x + width * 0.5, bottom.max(top + 1.0)),
                                ),
                                0.0,
                                color,
                            );
                        }
                    }
                    continue;
                }

                if let Some(price) = price {
                    // Convert price to screen Y coord
                    let y = if plot.plot_type == PlotType::BackgroundColor {
                        context.rect.center().y
                    } else {
                        price_scale.price_to_y(price)
                    };

                    // Keep off-scale endpoints: clipping must retain segments crossing the plot.
                    if y.is_finite() {
                        let at = triangle_centre(Pos2::new(x, y), price, plot.width, side, |p| {
                            price_scale.price_to_y(p)
                        });
                        points.push((at, price));
                    }
                } else {
                    // Break the line if we have a None value
                    if !points.is_empty() {
                        Self::draw_line(
                            context,
                            &points,
                            color,
                            style,
                            plot,
                            price_scale.price_to_y(0.0),
                        );
                    }
                    points.clear();
                }
            }

            if matches!(plot.plot_type, PlotType::Line | PlotType::LineWithBreaks)
                && indicator.extend_right()
                && visible_end == values.len()
                && points.len() >= 2
            {
                let ((a, pa), (b, pb)) = (points[points.len() - 2], points[points.len() - 1]);
                if b.x > a.x && b.x < context.rect.right() {
                    let painter = context.painter.with_clip_rect(context.rect);
                    let mut previous = b;
                    let mut extension = vec![b];
                    // Project prices before mapping them, so a logarithmic axis preserves the fit.
                    for step in 1..=((context.rect.right() - b.x).ceil() as usize) {
                        let x = (b.x + step as f32).min(context.rect.right());
                        let price = pb + (pb - pa) * ((x - b.x) / (b.x - a.x)) as f64;
                        let point = Pos2::new(x, price_scale.price_to_y(price));
                        if point.y.is_finite() && previous.y.is_finite() {
                            if plot.pattern == LineStyle::Solid {
                                painter.line_segment(
                                    [previous, point],
                                    Stroke::new(style.width, color),
                                );
                            } else {
                                extension.push(point);
                            }
                        }
                        previous = point;
                    }
                    if plot.pattern != LineStyle::Solid {
                        paint_study_plot(&painter, &extension, plot, price_scale.price_to_y(0.0));
                    }
                }
            }
            if let Some((line, label)) = indicator.plot_label()
                && line == line_idx
                && let Some(last) = points.last()
            {
                let at = Pos2::new(
                    last.0
                        .x
                        .min(context.rect.right() - 90.0)
                        .max(context.rect.left()),
                    last.0.y,
                );
                context.painter.with_clip_rect(context.rect).text(
                    at,
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::proportional(12.0),
                    color,
                );
            }

            // Draw final segment
            if !points.is_empty() {
                Self::draw_line(
                    context,
                    &points,
                    color,
                    style,
                    plot,
                    price_scale.price_to_y(0.0),
                );
            }
            if diamonds {
                // A diamond marks the first value and each change of value, so the first value in
                // view is compared with the defined value the walk starts at before it.
                let mut previous = None;
                for i in walk_start..source_end {
                    let Some(display_bar) = i.checked_add_signed(offset) else {
                        continue;
                    };
                    let Some(price) = indicator.plot_value(line_idx, display_bar) else {
                        continue;
                    };
                    let changed = previous != Some(price);
                    previous = Some(price);
                    let at = Pos2::new(coords.idx_to_x(display_bar), price_scale.price_to_y(price));
                    if changed && i >= source_start && at.is_finite() {
                        paint_diamond(
                            &painter,
                            at,
                            indicator.plot_width(line_idx, i) + 2.0,
                            indicator.plot_color(line_idx, i).unwrap_or(plot.color),
                        );
                    }
                }
            }
            if matches!(
                plot.plot_type,
                PlotType::Circles
                    | PlotType::Crosses
                    | PlotType::TriangleUp
                    | PlotType::TriangleDown
                    | PlotType::Character(_)
            ) {
                for i in source_start..source_end {
                    let (Some(display_bar), Some(value)) =
                        (i.checked_add_signed(offset), values.value(line_idx, i))
                    else {
                        continue;
                    };
                    let at = Pos2::new(coords.idx_to_x(display_bar), price_scale.price_to_y(value));
                    if !at.is_finite() {
                        continue;
                    }
                    let width = indicator.plot_width(line_idx, i);
                    let centre =
                        triangle_centre(at, value, width, side, |p| price_scale.price_to_y(p));
                    // Covers the shape, a character's box or a half-size of the stroke width plus
                    // 2, and the gap back to the value of a shape drawn beside it.
                    let size = match plot.plot_type {
                        PlotType::Character(_) => character_size(width),
                        _ => 2.0 * (width + 2.0),
                    };
                    let mut target = egui::Rect::from_center_size(centre, egui::Vec2::splat(size))
                        .union(egui::Rect::from_pos(at));
                    if let Some(text) = indicator.plot_text(line_idx, i) {
                        // Clear of the shape on its declared side, else under an upward
                        // triangle and over the other shapes.
                        let below = if label_side == ShapeSide::Centred {
                            plot.plot_type == PlotType::TriangleUp
                        } else {
                            triangle_centre(at, value, width, label_side, |p| {
                                price_scale.price_to_y(p)
                            })
                            .y > at.y
                        };
                        let (gap, anchor) = match plot.plot_type {
                            PlotType::Character(_) => {
                                (-(size / 2.0 + 2.0), egui::Align2::CENTER_BOTTOM)
                            }
                            _ if below => (width + 4.0, egui::Align2::CENTER_TOP),
                            _ => (-(width + 4.0), egui::Align2::CENTER_BOTTOM),
                        };
                        let color = indicator.plot_color(line_idx, i).unwrap_or(plot.color);
                        let (background, foreground) = indicator
                            .plot_label_colors(line_idx)
                            .map_or((None, color), |(background, foreground)| {
                                (Some(background), foreground)
                            });
                        let galley = painter.layout_no_wrap(
                            text,
                            egui::FontId::proportional(11.0),
                            foreground,
                        );
                        let bounds =
                            anchor.anchor_size(centre + egui::vec2(0.0, gap), galley.size());
                        if let Some(background) = background {
                            painter.rect_filled(bounds.expand(2.0), 0.0, background);
                        }
                        painter.galley(bounds.min, galley, foreground);
                        target = target.union(bounds.expand(2.0));
                    }
                    if !tooltip_shown
                        && hover.is_some_and(|pos| target.contains(pos))
                        && let Some(tooltip) = indicator.plot_tooltip(line_idx, i)
                    {
                        egui::Tooltip::always_open(
                            ctx.clone(),
                            context.painter.layer_id(),
                            // One id per pane, so the tooltips of several studies stack.
                            egui::Id::unique((
                                "study-mark",
                                context.rect.min.x.to_bits(),
                                context.rect.min.y.to_bits(),
                            )),
                            target,
                        )
                        .show(|ui| {
                            ui.label(tooltip);
                        });
                        tooltip_shown = true;
                    }
                }
            }
        }
        let painter = context.painter.with_clip_rect(context.rect);
        for marker in indicator
            .markers()
            .iter()
            .filter(|_| (0..line_count).any(|line| indicator.plot_style(line).visible))
        {
            if marker.bar < display_start || marker.bar >= display_end || !marker.value.is_finite()
            {
                continue;
            }
            let at = Pos2::new(
                coords.idx_to_x(marker.bar),
                price_scale.price_to_y(marker.value),
            );
            let (sign, color) = match marker.direction {
                CrossDirection::Up => (1.0, DESIGN_TOKENS.semantic.extended.success),
                CrossDirection::Down => (-1.0, DESIGN_TOKENS.semantic.extended.error),
            };
            if !at.is_finite() {
                continue;
            }
            if let Some(style) = indicator.marker_style() {
                let half = style.width + 2.0;
                let stroke = Stroke::new(style.width, style.color);
                painter.line_segment(
                    [at - egui::vec2(half, 0.0), at + egui::vec2(half, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [at - egui::vec2(0.0, half), at + egui::vec2(0.0, half)],
                    stroke,
                );
                continue;
            }
            let triangle = vec![
                at + egui::vec2(0.0, sign * 4.0),
                at + egui::vec2(-4.0, sign * 11.0),
                at + egui::vec2(4.0, sign * 11.0),
            ];
            painter.add(egui::Shape::convex_polygon(triangle, color, Stroke::NONE));
        }
        if !indicator.labels_on_price() {
            crate::chart::indicators::plots::paint_labels(&painter, indicator, price_scale, coords);
        }
        crate::chart::indicators::plots::paint_price_lines(&painter, indicator, price_scale);
    }

    fn draw_line(
        context: &RenderContext,
        points: &[(Pos2, f64)],
        color: Color32,
        style: IndicatorLineStyle,
        plot: PlotStyle,
        baseline: f32,
    ) {
        let painter = context.painter.with_clip_rect(context.rect);
        let plot = PlotStyle { color, ..plot };
        if !matches!(plot.plot_type, PlotType::Line | PlotType::LineWithBreaks)
            || plot.pattern != LineStyle::Solid
        {
            let positions: Vec<_> = points.iter().map(|(point, _)| *point).collect();
            paint_study_plot(&painter, &positions, plot, baseline);
            return;
        }
        if let [(point, _)] = points {
            painter.circle_filled(*point, DESIGN_TOKENS.stroke.thick, color);
            return;
        }
        if style.rising.is_none() && style.falling.is_none() && style.fill.is_none() {
            painter.add(egui::Shape::line(
                points.iter().map(|(point, _)| *point).collect(),
                Stroke::new(style.width, color),
            ));
            return;
        }
        let mut segment_color = color;
        for segment in points.windows(2) {
            let [(a, pa), (b, pb)] = segment else {
                unreachable!()
            };
            if pb > pa {
                segment_color = style.rising.unwrap_or(color);
            } else if pb < pa {
                segment_color = style.falling.unwrap_or(color);
            }
            if let Some(fill) = style.fill {
                let top = Color32::from_rgba_unmultiplied(
                    fill.color.r(),
                    fill.color.g(),
                    fill.color.b(),
                    fill.top_alpha,
                );
                let bottom = Color32::from_rgba_unmultiplied(
                    fill.color.r(),
                    fill.color.g(),
                    fill.color.b(),
                    fill.bottom_alpha,
                );
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(*a, top);
                mesh.colored_vertex(*b, top);
                mesh.colored_vertex(Pos2::new(a.x, context.rect.bottom()), bottom);
                mesh.colored_vertex(Pos2::new(b.x, context.rect.bottom()), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(1, 3, 2);
                painter.add(egui::Shape::mesh(mesh));
            }
            painter.line_segment([*a, *b], Stroke::new(style.width, segment_color));
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceScaleMode, PriceTransform};
    use crate::studies::LinearRegressionChannel;
    use crate::studies::{AverageLine, AverageType, Indicator, MACombination, MARibbon};
    use crate::tokens::DESIGN_TOKENS;
    use crate::widget::test_frames::{chart, shapes};
    use chrono::{TimeZone, Utc};
    use egui::{Color32, Context, Shape};
    use egui::{RawInput, Rect, Vec2};

    use crate::studies::{BollingerBands, SMA, StandardErrorBands};

    /// Each study's labels, in its plot's colour and with `chosen` text, as painted rectangles.
    fn painted_labels(
        shapes: &[Shape],
        color: Color32,
        chosen: impl Fn(&str) -> bool,
    ) -> Vec<(String, Rect)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text)
                    if chosen(text.galley.text())
                        && text
                            .galley
                            .job
                            .sections
                            .iter()
                            .all(|s| s.format.color == color) =>
                {
                    Some((
                        text.galley.text().to_owned(),
                        Rect::from_min_size(text.pos, text.galley.size()),
                    ))
                }
                _ => None,
            })
            .collect()
    }

    /// Pivot labels take the instrument's price precision, and a Precision chosen in the
    /// study's settings wins from the next frame. 1.180958 rounds to 1.1810 at four decimals and
    /// to 1.18096 at five, and 65845.4 to 65845 at none.
    #[test]
    fn pivot_labels_take_the_price_precision() {
        type Row = (
            &'static [f64],
            &'static [f64],
            f64,
            Option<usize>,
            [&'static str; 2],
        );
        let fx_lows: &'static [f64] = &[1.13, 1.14, 1.160958, 1.15, 1.14];
        let fx_highs: &'static [f64] = &[1.15, 1.16, 1.180958, 1.17, 1.16];
        let rows: [Row; 3] = [
            (fx_lows, fx_highs, 0.0001, None, ["1.1300", "1.1810"]),
            (fx_lows, fx_highs, 0.0001, Some(5), ["1.13000", "1.18096"]),
            (
                &[65500.4, 65520.6, 65545.4, 65530.2, 65510.0],
                &[65800.4, 65820.6, 65845.4, 65830.2, 65810.0],
                1.0,
                None,
                ["65500", "65845"],
            ),
        ];
        let mut wrong = Vec::new();
        for (lows, highs, tick, chosen, want) in rows {
            let mut chart = chart(lows, highs);
            chart.set_instrument(Some(
                crate::model::Instrument::new("X").with_tick_size(tick),
            ));
            let id = chart.add_indicator(Box::new(crate::studies::PivotPointsHighLow::new(
                2, 2, 2, 2,
            )));
            let ctx = Context::default();
            shapes(&ctx, &mut chart, 0.0, Vec::new());
            if chosen.is_some() {
                let study = chart.indicator_mut(id).unwrap();
                let mut settings = study.settings();
                settings.precision = chosen;
                study.apply_settings(&[], settings).unwrap();
            }
            let painted = shapes(&ctx, &mut chart, 1.0, Vec::new());
            let mut got: Vec<String> =
                painted_labels(&painted, Color32::from_rgb(54, 58, 69), |_| true)
                    .into_iter()
                    .map(|(text, _)| text)
                    .collect();
            got.sort();
            got.dedup();
            if got != want {
                wrong.push(format!(
                    "tick {tick}, precision {chosen:?}: {got:?}, want {want:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// The label at the last bar reaches past the plot unless it is moved back, and the axis tags
    /// start right of the plot. Every bar of the Zig Zag series reverses the last by 9% or 10%, so
    /// the last pivot is bar 99, a high, and the last final one bar 98, a low.
    #[test]
    fn study_labels_stay_in_the_plot_and_provisional_legs_are_dashed() {
        let mut chart = chart(&[100.0, 101.0], &[110.0, 111.0]);
        chart.add_indicator(Box::new(
            crate::studies::PivotPoints::new()
                .with_timeframe(crate::studies::PivotTimeframe::Daily)
                .with_labels(true, true),
        ));
        let ctx = Context::default();
        let shapes = shapes(&ctx, &mut chart, 0.0, Vec::new());
        let price = chart.get_rendered_price_rect();
        let labels = painted_labels(&shapes, crate::studies::palette::DARK_ORANGE, |text| {
            text.split_once(" (")
                .is_some_and(|(name, _)| name.len() <= 2)
        });
        assert!(!labels.is_empty());
        for (text, rect) in labels {
            assert!(
                rect.left() >= price.left() && rect.right() <= price.right(),
                "{text} at {rect:?} leaves {price:?}"
            );
        }

        let blue = crate::studies::palette::BLUE;
        let mut chart = crate::widget::test_frames::chart(&[100.0, 105.0], &[105.0, 110.0]);
        chart.add_indicator(Box::new(
            crate::studies::ZigZag::new(5.0, 1).with_labels(true, None, false),
        ));
        let ctx = Context::default();
        let shapes = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, Vec::new());
        let price = chart.get_rendered_price_rect();
        let labels = painted_labels(&shapes, blue, |text| matches!(text, "100.00" | "110.00"));
        let bottom = |text: &str| {
            let rects = labels.iter().filter(|(label, _)| label == text);
            rects
                .map(|(_, rect)| rect.bottom())
                .fold(f32::MIN, f32::max)
        };
        let top = |text: &str| {
            let rects = labels.iter().filter(|(label, _)| label == text);
            rects.map(|(_, rect)| rect.top()).fold(f32::MAX, f32::min)
        };
        assert!(bottom("110.00") < top("100.00"), "{labels:?}");
        assert!(labels.iter().all(|(text, rect)| {
            text.len() == 6 && rect.left() >= price.left() && rect.right() <= price.right()
        }));
        let (from, to) = (
            chart.index_to_x(98.0).unwrap(),
            chart.index_to_x(99.0).unwrap(),
        );
        assert!(shapes.iter().any(|shape| matches!(
            shape,
            Shape::LineSegment { points, stroke }
                if stroke.color == blue && points[0].x >= from && points[1].x <= to
        )));
        let solid_points = shapes.iter().filter_map(|shape| match shape {
            Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(blue) => {
                Some(&path.points)
            }
            _ => None,
        });
        assert!(solid_points.flatten().all(|point| point.x <= from + 0.5));
    }

    #[test]
    fn patterned_study_plots_only_walk_visible_lengths() {
        let clip = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let color = Color32::from_rgb(17, 71, 191);
        let mut counts = Vec::new();
        for (pattern, period) in [
            (LineStyle::Dashed, 10.0),
            (LineStyle::LargeDashed, 20.0),
            (LineStyle::Dotted, 4.0),
            (LineStyle::SparseDotted, 8.0),
        ] {
            let mut paths = vec![
                vec![Pos2::new(50.0, -1e7), Pos2::new(50.0, 1e7)],
                // Lengths 13 and 24 carry the pattern across the corner.
                vec![
                    Pos2::new(10.0, 10.0),
                    Pos2::new(23.0, 10.0),
                    Pos2::new(23.0, 34.0),
                ],
            ];
            if matches!(pattern, LineStyle::Dotted | LineStyle::SparseDotted) {
                // These centres are outside the clip, but their radius overlaps it.
                paths.push(vec![
                    Pos2::new(10.0, -1.0),
                    Pos2::new(23.0, -1.0),
                    Pos2::new(23.0, 34.0),
                ]);
            }
            for points in paths {
                let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                    paint_study_plot(
                        &ui.painter().with_clip_rect(clip),
                        &points,
                        PlotStyle {
                            color,
                            width: 4.0,
                            pattern,
                            plot_type: PlotType::Line,
                            visible: true,
                        },
                        0.0,
                    );
                });
                let painted: Vec<_> = std::mem::take(&mut output.shapes)
                    .into_iter()
                    .map(|shape| shape.shape)
                    .collect();
                output.drop_without_applying_deltas();
                if points.len() == 2 {
                    counts.push(painted.len());
                } else {
                    let expected = match pattern {
                        LineStyle::Dashed => {
                            Shape::dashed_line(&points, Stroke::new(4.0, color), 6.0, 4.0)
                        }
                        LineStyle::LargeDashed => {
                            Shape::dashed_line(&points, Stroke::new(4.0, color), 12.0, 8.0)
                        }
                        _ => Shape::dotted_line(&points, color, period, 2.0),
                    };
                    assert_eq!(painted, expected, "in-pane pattern {pattern:?}");
                }
            }
        }
        // Periods 10 and 20 give 11 and 6 dashes including the border stroke; dots include edge centres.
        assert_eq!(
            counts,
            [11, 6, 26, 13],
            "shape counts must follow the visible length"
        );
    }

    #[test]
    fn plot_geometry_uses_the_selected_stroke_and_zero_baseline() {
        let color = Color32::from_rgb(17, 71, 191);
        let points = [Pos2::new(10.0, 15.0), Pos2::new(20.0, 5.0)];
        for (plot_type, pattern, width) in [
            (PlotType::Histogram, LineStyle::Solid, 4.0),
            (PlotType::Columns, LineStyle::Solid, 4.0),
            (PlotType::StepLine, LineStyle::Solid, 4.0),
            (PlotType::Crosses, LineStyle::Solid, 4.0),
            (PlotType::TriangleUp, LineStyle::Solid, 4.0),
            (PlotType::TriangleDown, LineStyle::Solid, 4.0),
            (PlotType::Area, LineStyle::Solid, 1.0),
            (PlotType::Area, LineStyle::Solid, 6.0),
            (PlotType::Area, LineStyle::Dashed, 6.0),
            (PlotType::AreaWithBreaks, LineStyle::Solid, 6.0),
            (PlotType::StepLineWithDiamonds, LineStyle::Solid, 4.0),
            (PlotType::BackgroundColor, LineStyle::Solid, 4.0),
            (PlotType::Circles, LineStyle::Solid, 4.0),
            (PlotType::Character('d'), LineStyle::Solid, 4.0),
            (PlotType::Line, LineStyle::Dashed, 4.0),
            (PlotType::Line, LineStyle::Dotted, 4.0),
        ] {
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                paint_study_plot(
                    &ui.painter()
                        .with_clip_rect(Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0))),
                    &points,
                    PlotStyle {
                        color,
                        width,
                        pattern,
                        plot_type,
                        visible: true,
                    },
                    20.0,
                );
            });
            let painted: Vec<_> = std::mem::take(&mut output.shapes)
                .into_iter()
                .map(|s| s.shape)
                .collect();
            output.drop_without_applying_deltas();
            match plot_type {
                PlotType::Histogram | PlotType::Columns => {
                    let rects: Vec<_> = painted
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Rect(r) if r.fill == color => Some(r.rect),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        rects,
                        [
                            Rect::from_min_max(Pos2::new(8.0, 15.0), Pos2::new(12.0, 20.0)),
                            Rect::from_min_max(Pos2::new(18.0, 5.0), Pos2::new(22.0, 20.0)),
                        ]
                    );
                }
                PlotType::Circles => {
                    let circles = painted.iter().filter(
                        |s| matches!(s, Shape::Circle(c) if c.fill == color && c.radius == 4.0),
                    );
                    assert_eq!(circles.count(), 2);
                }
                PlotType::Line if pattern == LineStyle::Dashed => {
                    assert!(painted.iter().any(|s| {
                        matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == color && stroke.width == 4.0)
                    }));
                }
                PlotType::StepLine => {
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                        if path.points == [points[0], Pos2::new(20.0, 15.0), points[1]])));
                }
                // The steps, and a diamond at the first value and at the change, 4 + 2 px from its
                // centre to each corner.
                PlotType::StepLineWithDiamonds => {
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                        if !path.closed && path.points == [points[0], Pos2::new(20.0, 15.0), points[1]])));
                    let diamonds: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Path(path) if path.closed && path.fill == color => {
                                Some(path.points.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        diamonds,
                        [
                            [(10.0, 9.0), (16.0, 15.0), (10.0, 21.0), (4.0, 15.0)],
                            [(20.0, -1.0), (26.0, 5.0), (20.0, 11.0), (14.0, 5.0)],
                        ]
                        .map(|corners| corners.map(|(x, y)| Pos2::new(x, y)))
                    );
                }
                PlotType::Crosses => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == color)).count(), 4);
                }
                PlotType::TriangleUp | PlotType::TriangleDown => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::Path(path) if path.points.len() == 3 && path.fill == color)).count(), 2);
                }
                // The line in the plot's width, colour and pattern over a fill of its colour at
                // 20% of its opacity: 17, 71, 191 and 255 times 0.2, rounded.
                PlotType::Area | PlotType::AreaWithBreaks => {
                    let fill = Color32::from_rgba_premultiplied(3, 14, 38, 51);
                    let meshes: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Mesh(mesh) => Some(mesh),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(meshes.len(), 1, "{width} {pattern:?}");
                    assert_eq!(
                        meshes[0]
                            .vertices
                            .iter()
                            .map(|vertex| vertex.pos)
                            .collect::<Vec<_>>(),
                        [
                            points[0],
                            points[1],
                            Pos2::new(20.0, 20.0),
                            Pos2::new(10.0, 20.0)
                        ]
                    );
                    assert!(meshes[0].vertices.iter().all(|vertex| vertex.color == fill));
                    let stroke = Stroke::new(width, color);
                    if pattern == LineStyle::Solid {
                        assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                            if path.points == points && !path.closed && path.stroke == stroke.into())), "{width}");
                    } else {
                        assert!(painted.iter().any(|shape| matches!(shape, Shape::LineSegment { stroke: dash, .. } if *dash == stroke)));
                    }
                }
                PlotType::BackgroundColor => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == color && rect.rect.height() == 100.0)).count(), 2);
                }
                // A "d" centred on each point, 2 x (4 + 5) = 18 px, in the plot colour.
                PlotType::Character(_) => {
                    let texts: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) => Some(text),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(texts.len(), 2);
                    for (text, point) in texts.into_iter().zip(points) {
                        assert_eq!(text.galley.text(), "d");
                        let format = &text.galley.job.sections[0].format;
                        assert_eq!((format.font_id.size, format.color), (18.0, color));
                        let centre = text.pos + text.galley.size() / 2.0;
                        assert!((centre - point).length() < 1e-3, "{centre:?}");
                    }
                }
                _ => {
                    assert!(painted.iter().any(|s| {
                        matches!(s, Shape::Circle(c) if c.fill == color && c.radius == 2.0)
                    }));
                }
            }
        }
    }

    /// Line, step line and area join the values on either side of a missing one, in the view and
    /// across its edges, and their types with breaks leave the gap. A step line with diamonds
    /// marks its first value and each change of value.
    #[test]
    fn plots_join_a_missing_value_unless_drawn_with_breaks() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        let color = Color32::from_rgb(17, 71, 191);
        let fill = Color32::from_rgba_premultiplied(3, 14, 38, 51);
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 100.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
        // The open paths, the fills and the diamonds painted for `values` as `kind`.
        let paint = |values: Vec<Option<f64>>, coords: &ChartMapping, kind: PlotType| {
            let mut study = CustomIndicator::new("Gap", move |_| {
                values
                    .iter()
                    .map(|value| value.map_or(IndicatorValue::None, IndicatorValue::Single))
                    .collect()
            })
            .with_color(color)
            .with_plot_types(vec![kind]);
            study.calculate(&[]);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &[],
                    0,
                    &map,
                    coords,
                );
            });
            let shapes: Vec<_> = std::mem::take(&mut output.shapes)
                .into_iter()
                .map(|shape| shape.shape)
                .collect();
            output.drop_without_applying_deltas();
            let paths: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Path(path) if !path.closed => {
                        assert_eq!(path.stroke, Stroke::new(1.0, color).into(), "{kind:?}");
                        Some(path.points.clone())
                    }
                    _ => None,
                })
                .collect();
            let fills = shapes
                .iter()
                .filter(|shape| matches!(shape, Shape::Mesh(mesh) if mesh.vertices.iter().all(|vertex| vertex.color == fill)))
                .count();
            let diamonds: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Path(path) if path.closed && path.fill == color => Some(path.points[0]),
                    _ => None,
                })
                .collect();
            (paths, fills, diamonds)
        };
        let pos = |points: &[(f32, f32)]| -> Vec<Pos2> {
            points.iter().map(|&(x, y)| Pos2::new(x, y)).collect()
        };
        // Values 25, 50, none, 75 and 87.5 at x 19, 59, 99, 139 and 179; y is 100 minus the
        // value.
        let coords = ChartMapping::new(rect, 40.0, 0, 4, 0.0, map);
        let (a, b, d, e) = ((19.0, 75.0), (59.0, 50.0), (139.0, 25.0), (179.0, 12.5));
        let steps = [a, (59.0, 75.0), b, (139.0, 50.0), d, (179.0, 25.0), e];
        let line = vec![pos(&[a, b, d, e])];
        let broken = vec![pos(&[a, b]), pos(&[d, e])];
        for (kind, paths, fills, diamonds) in [
            (PlotType::Line, line.clone(), 0, vec![]),
            (PlotType::LineWithBreaks, broken.clone(), 0, vec![]),
            (PlotType::StepLine, vec![pos(&steps)], 0, vec![]),
            (
                PlotType::StepLineWithBreaks,
                vec![pos(&steps[..3]), pos(&steps[4..])],
                0,
                vec![],
            ),
            // Diamond tops 1 + 2 px above the values.
            (
                PlotType::StepLineWithDiamonds,
                vec![pos(&steps)],
                0,
                pos(&[(19.0, 72.0), (59.0, 47.0), (139.0, 22.0), (179.0, 9.5)]),
            ),
            (PlotType::Area, line, 3, vec![]),
            (PlotType::AreaWithBreaks, broken, 2, vec![]),
        ] {
            let values = vec![Some(25.0), Some(50.0), None, Some(75.0), Some(87.5)];
            assert_eq!(
                paint(values, &coords, kind),
                (paths, fills, diamonds),
                "{kind:?}"
            );
        }
        // Diamonds mark the first value and each change of value: 25, 25, none, 25 and 75 mark
        // bars 0 and 4 only, a repeat being unmarked on the next bar and across the gap alike.
        let diamonds = |values: Vec<Option<f64>>, coords: &ChartMapping| {
            paint(values, coords, PlotType::StepLineWithDiamonds).2
        };
        assert_eq!(
            diamonds(
                vec![Some(25.0), Some(25.0), None, Some(25.0), Some(75.0)],
                &coords
            ),
            pos(&[(19.0, 72.0), (179.0, 22.0)])
        );
        // Only the first and last of twelve values are defined, at x -61 and 379, outside the
        // view of bars 1 to 8: the joined line crosses it, the line with breaks draws nothing.
        let coords = ChartMapping::new(rect, 40.0, 2, 6, 0.0, map);
        let mut values = vec![None; 12];
        values[0] = Some(25.0);
        values[11] = Some(75.0);
        for (kind, paths) in [
            (PlotType::Line, vec![pos(&[(-61.0, 75.0), (379.0, 25.0)])]),
            (PlotType::LineWithBreaks, vec![]),
        ] {
            assert_eq!(
                paint(values.clone(), &coords, kind),
                (paths, 0, vec![]),
                "{kind:?} across the view"
            );
        }
        // The first value in view is compared with the one before the view: the 25 on bar 3
        // repeats bar 0's and stays unmarked, the 75 on bar 5 at x 139 is marked.
        values[3] = Some(25.0);
        values[5] = Some(75.0);
        assert_eq!(diamonds(values, &coords), pos(&[(139.0, 22.0)]));
    }

    /// A joined line reaches past the view only to join a value in it or beyond its other edge,
    /// so a regression channel fitted to bars 9 to 11 draws no fit label over bars 0 to 4.
    #[test]
    fn fit_label_stays_hidden_while_the_channel_is_out_of_view() {
        let bars: Vec<_> = (0..12)
            .map(|i| {
                let c = 10.0 + i as f64;
                Bar::new(Utc.timestamp_opt(i, 0).unwrap(), c, c, c, c, 1.0)
            })
            .collect();
        let mut study = LinearRegressionChannel::new(3, 0.0);
        study.calculate(&bars);
        assert!(study.plot_label().is_some());
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 100.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 30.0),
            0.0,
            100.0,
            false,
        );
        let coords = ChartMapping::new(rect, 40.0, 0, 4, 0.0, price);
        let mut output = Context::default().run_ui(RawInput::default(), |ui| {
            IndicatorRenderer::render_study(
                &RenderContext::new(ui.painter(), rect),
                &study,
                &bars,
                0,
                &price,
                &coords,
            );
        });
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        assert!(
            !shapes
                .iter()
                .any(|clipped| matches!(clipped.shape, Shape::Text(_)))
        );
    }

    #[test]
    fn ao_price_lines_follow_the_latest_bar_independently_of_tags() {
        use crate::studies::{AwesomeOscillator, ConfiguredStudy};
        let bars: Vec<_> = [1.0, 2.0, 4.0, 3.0, 1.0, 4.0]
            .into_iter()
            .map(|p| Bar::new(Utc.timestamp_opt(0, 0).unwrap(), p, p, p, p, 1.0))
            .collect();
        let mut study = ConfiguredStudy::new(Box::new(AwesomeOscillator::new(2, 3)));
        // On the price pane, where the tint lies beneath the shared geometry.
        study.set_overlay(true);
        study.set_scale(crate::studies::StudyScale::Main);
        study.calculate(&bars);
        // At bar 5: (1 + 4)/2 - (3 + 1 + 4)/3 = -1/6, rising from -2/3.
        assert!((study.values().value(0, 5).unwrap() + 1.0 / 6.0).abs() < 1e-12);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(240.0, 120.0));
        for alternate_color in [
            Color32::from_rgb(17, 71, 191),
            Color32::from_rgb(191, 71, 17),
            Color32::from_rgb_additive(191, 71, 17),
        ] {
            for (tagged, visible, enabled, missing, offset, last, expected) in [
                (true, true, true, false, 0, 4.0, Some(-1.0 / 6.0)),
                (false, true, true, false, 1, 4.0, Some(-1.0 / 6.0)),
                (false, true, true, false, -100, 4.0, Some(-1.0 / 6.0)),
                (false, true, true, false, 100, 4.0, Some(-1.0 / 6.0)),
                (true, true, true, false, 0, 0.0, Some(-5.0 / 6.0)),
                (true, true, false, false, 0, 4.0, None),
                (true, false, true, false, 0, 4.0, None),
                (true, true, true, true, 0, 4.0, None),
            ] {
                let mut settings = serde_json::to_value(study.settings()).unwrap();
                settings["price_line_plots"] =
                    serde_json::json!(if enabled { vec![0] } else { vec![] });
                settings["untagged_plots"] =
                    serde_json::json!(if tagged { vec![] } else { vec![0] });
                settings["plots"][0]["visible"] = visible.into();
                settings["plots"][0]["color"] = serde_json::json!([17, 71, 191, 255]);
                settings["plots"][0]["width"] = 2.0.into();
                settings["plots"][0]["plot_type"] = serde_json::json!("Histogram");
                let tint = Color32::from_rgba_unmultiplied(20, 40, 60, 16);
                settings["background"] = serde_json::to_value(tint).unwrap();
                settings["plot_variants"] = serde_json::json!([[{
                    "name": "Decreasing bars", "color": alternate_color.to_array(), "width": 5.0
                }]]);
                study
                    .apply_settings(
                        &[(
                            "plot_offset".into(),
                            crate::studies::InputValue::Integer(offset),
                        )],
                        serde_json::from_value(settings).unwrap(),
                    )
                    .unwrap();
                let mut data = bars.clone();
                data[5] = Bar::new(data[5].time, last, last, last, last, 1.0);
                if missing {
                    data[5].high = f64::NAN;
                }
                study.calculate(&data);
                for (inverted, tinted) in
                    [(false, true), (true, true), (false, false), (true, false)]
                {
                    let mut settings = serde_json::to_value(study.settings()).unwrap();
                    settings["background_visible"] = tinted.into();
                    study
                        .apply_settings(&[], serde_json::from_value(settings).unwrap())
                        .unwrap();
                    let map = PriceMap::new(
                        PriceTransform::default(),
                        PriceRange::new(-1.0, 1.0),
                        0.0,
                        120.0,
                        inverted,
                    );
                    // Latest-value lines span the viewport even while the last bar is offscreen.
                    let coords = ChartMapping::new(rect, 30.0, 0, 2, 0.0, map);
                    let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                        IndicatorRenderer::render(
                            &RenderContext::new(ui.painter(), rect),
                            std::slice::from_ref(&study.clone_box()),
                            &data,
                            &map,
                            &coords,
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    // The latest-value line is a row of one-pixel dots in a single mesh.
                    let lines: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            Shape::Mesh(mesh) if !mesh.vertices.is_empty() => {
                                let (min, max) = mesh
                                    .vertices
                                    .iter()
                                    .fold((f32::MAX, f32::MIN), |(min, max), v| {
                                        (min.min(v.pos.y), max.max(v.pos.y))
                                    });
                                (max - min <= 1.0)
                                    .then_some(((min + max) / 2.0, mesh.vertices[0].color))
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        !lines.is_empty(),
                        expected.is_some(),
                        "tagged={tagged}, visible={visible}, enabled={enabled}, missing={missing}, offset={offset}"
                    );
                    if let Some(value) = expected {
                        assert!((lines[0].0 - map.price_to_y(value)).abs() <= 0.5);
                        assert_eq!(
                            lines[0].1,
                            if last == 0.0 {
                                alternate_color
                            } else {
                                Color32::from_rgb(17, 71, 191)
                            }
                        );
                    }
                    assert_eq!(
                        shapes.iter().any(
                            |s| matches!(&s.shape, Shape::Rect(r) if r.fill == tint && r.rect == rect)
                        ),
                        tinted
                    );
                    assert_eq!(
                        serde_json::to_value(study.settings()).unwrap()["background"],
                        serde_json::to_value(tint).unwrap()
                    );
                    // Histogram bars have independently editable rising and falling widths.
                    if visible && offset == 0 {
                        for (color, width) in [
                            (Color32::from_rgb(17, 71, 191), 2.0),
                            (alternate_color, 5.0),
                        ] {
                            assert!(shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == color && (r.rect.width() - width).abs() < 0.001)));
                        }
                    }
                }
            }
        }
        assert_eq!(study.plot_style(0).plot_type, PlotType::Histogram);
    }

    #[test]
    fn displaced_bands_split_crossings_without_bridging_gaps() {
        use crate::studies::{
            ConfiguredStudy, CustomIndicator, FillBoundary, IndicatorValue, StudyFill,
        };
        let blue = Color32::from_rgba_unmultiplied(41, 98, 255, 51);
        let red = Color32::from_rgba_unmultiplied(255, 0, 0, 51);
        // (inverted scale, fill switch, boundary plot shown, fill saved since the switch alone
        // decided that the band is drawn)
        for (inverted, visible, boundary_visible, independent) in [
            (false, true, true, true),
            (true, true, true, true),
            (false, false, true, true),
            (true, false, true, true),
            (false, true, false, true),
            (true, true, false, true),
            // Saved before then, a hidden boundary plot also hid the band.
            (false, true, false, false),
        ] {
            let mut study = CustomIndicator::new("Band", |_| {
                vec![
                    IndicatorValue::Multiple(vec![2.0, 0.0]),
                    IndicatorValue::Multiple(vec![0.0, 2.0]),
                    IndicatorValue::None,
                    IndicatorValue::Multiple(vec![2.0, 0.0]),
                ]
            })
            .with_line_count(2)
            .with_offsets(vec![2, 2])
            .with_fills(vec![StudyFill {
                first: FillBoundary::Plot(0),
                second: FillBoundary::Plot(1),
                color: Color32::WHITE,
                inverted_color: None,
            }]);
            study.calculate(&[]);
            let mut study = ConfiguredStudy::new(Box::new(study));
            let mut settings = serde_json::to_value(study.settings()).unwrap();
            // The band keeps its own colours even when its boundary plot colours change.
            settings["plots"][0]["color"] = serde_json::json!([10, 20, 30, 255]);
            settings["plots"][0]["visible"] = boundary_visible.into();
            settings["fills"] = serde_json::json!([{
                "color": blue.to_array(), "inverted_color": red.to_array(), "visible": visible
            }]);
            if independent {
                settings["fills"][0]["independent"] = true.into();
            }
            study
                .apply_settings(&[], serde_json::from_value(settings).unwrap())
                .unwrap();
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(240.0, 120.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 2.0),
                0.0,
                120.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 30.0, 0, 3, 0.0, map);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &[],
                    0,
                    &map,
                    &coords,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let bands: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Mesh(mesh)
                        if mesh.vertices.iter().all(|vertex| vertex.color.a() == 51) =>
                    {
                        Some(mesh)
                    }
                    _ => None,
                })
                .collect();
            // Values cross halfway from displayed bar 2 to 3, at price 1.
            let drawn = visible && (boundary_visible || independent);
            assert_eq!(bands.len(), if drawn { 2 } else { 0 });
            if !drawn {
                continue;
            }
            let crossing = Pos2::new(209.0, 60.0);
            for band in &bands {
                assert_eq!(band.vertices.len(), 3);
                assert!(band.vertices.iter().any(|vertex| vertex.pos == crossing));
                assert!(
                    band.vertices
                        .iter()
                        .all(|vertex| (194.0..=224.0).contains(&vertex.pos.x))
                );
            }
            assert!(bands[0].vertices.iter().all(|vertex| vertex.color == blue));
            assert!(bands[1].vertices.iter().all(|vertex| vertex.color == red));
        }
    }

    #[test]
    fn bar_colours_preserve_bodies_on_inverted_scales() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        let bars: Vec<_> = [(1.0, 2.0), (2.0, 0.5)]
            .into_iter()
            .map(|(open, close)| {
                Bar::new(Utc.timestamp_opt(0, 0).unwrap(), open, 2.5, 0.5, close, 1.0)
            })
            .collect();
        let mut study = CustomIndicator::new("Colour", |_| vec![IndicatorValue::Single(1.0); 2])
            .with_plot_types(vec![PlotType::BarColor])
            .with_plot_color(|_, bar, _| {
                Some(if bar == 0 {
                    Color32::GREEN
                } else {
                    Color32::RED
                })
            });
        study.calculate(&bars);
        for (inverted, offset) in [(false, 0), (true, 0), (false, 1), (true, 1)] {
            let study = study.clone().with_offsets(vec![offset]);
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(240.0, 120.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 3.0),
                0.0,
                120.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 30.0, 0, 1, 0.0, map);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &bars,
                    0,
                    &map,
                    &coords,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            for (color, bottom) in [
                (Color32::GREEN, if offset == 0 { 80.0 } else { 100.0 }),
                (Color32::RED, 100.0),
            ] {
                if offset == 1 && color == Color32::RED {
                    assert!(!shapes.iter().any(
                        |shape| matches!(&shape.shape, Shape::Rect(body) if body.fill == color)
                    ));
                    continue;
                }
                let body = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Rect(body) if body.fill == color => Some(body.rect),
                        _ => None,
                    })
                    .unwrap();
                let expected = if inverted {
                    egui::emath::Rangef::new(120.0 - bottom, 80.0)
                } else {
                    egui::emath::Rangef::new(40.0, bottom)
                };
                assert_eq!(body.y_range(), expected);
            }
        }
    }

    #[test]
    fn overlay_lines_use_joined_one_point_strokes_and_keep_gaps_and_study_colours() {
        let blue = Color32::from_rgb(41, 98, 255);
        let orange = Color32::from_rgb(255, 109, 0);
        let custom = Color32::from_rgb(10, 200, 90);
        let mut recoloured = BollingerBands::default();
        recoloured.set_colors(vec![custom]);
        let mut sma = SMA::default();
        sma.set_colors(vec![custom]);
        let cases: [(Box<dyn Indicator>, Vec<Color32>); 5] = [
            (
                Box::new(BollingerBands::default()),
                vec![
                    Color32::from_rgb(242, 54, 69),
                    blue,
                    Color32::from_rgb(8, 153, 129),
                ],
            ),
            (Box::new(sma), vec![custom]),
            (Box::new(recoloured), vec![custom; 3]),
            (
                Box::new(StandardErrorBands::new(20, 2.0)),
                vec![Color32::from_rgb(33, 150, 243), orange],
            ),
            (
                Box::new(
                    crate::studies::CustomIndicator::new("Gaps", |bars| {
                        bars.iter()
                            .enumerate()
                            .map(|(i, bar)| {
                                if (40..60).contains(&i) {
                                    crate::studies::IndicatorValue::None
                                } else {
                                    crate::studies::IndicatorValue::Single(bar.close)
                                }
                            })
                            .collect()
                    })
                    .with_color(custom)
                    .with_overlay(true)
                    .with_plot_types(vec![PlotType::LineWithBreaks]),
                ),
                vec![custom],
            ),
        ];
        for (study, colours) in cases {
            let name = study.name().to_owned();
            let fill_alpha = study.fills().first().map(|fill| fill.color.a());
            let mut chart = chart(&[100.0], &[110.0]);
            chart.add_indicator(study);
            let ctx = Context::default();
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            if name == "BB" {
                assert!(
                    painted
                        .iter()
                        .any(|shape| matches!(shape, Shape::Mesh(mesh) if mesh.vertices.iter().all(|vertex| Some(vertex.color.a()) == fill_alpha))),
                    "band between the outer lines"
                );
            }
            let strokes: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Path(path) if path.points.len() > 2 => {
                        let egui::epaint::ColorMode::Solid(color) = path.stroke.color else {
                            return None;
                        };
                        colours
                            .contains(&color)
                            .then_some(Stroke::new(path.stroke.width, color))
                    }
                    _ => None,
                })
                .collect();
            assert!(!strokes.is_empty(), "{name}: no overlay lines");
            assert!(
                strokes.iter().all(|stroke| stroke.width == 1.0),
                "{name}: {strokes:?}"
            );
            for colour in colours {
                assert!(
                    strokes.iter().any(|stroke| stroke.color == colour),
                    "{name}: missing {colour:?}"
                );
            }
            if name == "Gaps" {
                assert_eq!(
                    strokes.len(),
                    2,
                    "one joined stroke on each side of the gap"
                );
                let gap = chart.index_to_x(50.0).unwrap();
                for shape in &painted {
                    if let Shape::Path(path) = shape
                        && path.stroke.color == egui::epaint::ColorMode::Solid(custom)
                    {
                        assert!(
                            path.points
                                .windows(2)
                                .all(|pair| { !(pair[0].x < gap && pair[1].x > gap) })
                        );
                    }
                }
            }
        }
    }

    /// Overlay rendering consumes ribbon widths/fades and both crossing directions.
    #[test]
    fn ribbon_styles_and_crossing_markers_are_painted() {
        let cross = MACombination::new(vec![
            AverageLine::new(AverageType::Simple, 1, Color32::BLUE),
            AverageLine::new(AverageType::Simple, 2, Color32::RED),
        ])
        .unwrap();
        for (study, filled, marked) in [
            (Box::new(MARibbon::new()) as Box<dyn Indicator>, true, false),
            (Box::new(MARibbon::muted()), false, false),
            (Box::new(cross.clone()), false, true),
            (
                {
                    let mut pane = crate::studies::ConfiguredStudy::new(Box::new(cross));
                    pane.set_overlay(false);
                    Box::new(pane)
                },
                false,
                true,
            ),
        ] {
            for (_, theme) in crate::theme::presets() {
                let mut chart = chart(&[0.0], &[1.0, 2.0]);
                chart.add_indicator(study.clone_box());
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let shapes = shapes(&ctx, &mut chart, 0.0, Vec::new());
                let fading = shapes.iter().any(|shape| matches!(shape,Shape::Mesh(mesh)
                if mesh.vertices.len()==4 && mesh.vertices[0].color.a()==38 && mesh.vertices[2].color.a()==10));
                assert_eq!(fading, filled, "bottom fade");
                let thick = shapes.iter().any(
                    |shape| matches!(shape,Shape::LineSegment {stroke,..} if stroke.width==3.0),
                );
                assert_eq!(thick, filled, "slowest ribbon width");
                for color in [
                    DESIGN_TOKENS.semantic.extended.success,
                    DESIGN_TOKENS.semantic.extended.error,
                ] {
                    let triangle = shapes.iter().any(|shape| matches!(shape,Shape::Path(path) if path.points.len()==3 && path.fill==color));
                    assert_eq!(triangle, marked, "crossing direction {color:?}");
                }
            }
        }
    }
    #[cfg(feature = "drawings")]
    fn pattern_bars(anchors: &[f64]) -> Vec<Bar> {
        let mut closes = vec![anchors[0]];
        for pair in anchors.windows(2) {
            closes.extend((1..=5).map(|i| pair[0] + (pair[1] - pair[0]) * f64::from(i) / 5.0));
        }
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    /// A study that draws still paints its plots above its drawings: a chart pattern study marks
    /// the bar confirming a double top with a triangle and its abbreviation in the Bearish colour
    /// of its Style tab, draws the neckline in that colour beneath them, and both can be
    /// selected.
    #[cfg(feature = "drawings")]
    #[test]
    fn chart_pattern_confirmations_are_painted_above_their_boundaries() {
        use crate::chart::indicators::hit_test_indicator;
        use crate::studies::{ChartPatterns, ConfiguredStudy, PatternFamily};
        // Legs of five flat bars through 100, 120, 105, 119, 60: tops at bars 5 and 15 over
        // the valley at 105 on bar 10; bar 18 closes at 83.6 once the second top is known.
        let bars = pattern_bars(&[100.0, 120.0, 105.0, 119.0, 60.0]);
        let bearish = Color32::from_rgb(200, 0, 120);
        let mut study = ConfiguredStudy::new(Box::new(ChartPatterns::new(PatternFamily::Reversal)));
        let mut settings = study.settings();
        settings.plots[1].color = bearish;
        study.apply_settings(&[], settings).unwrap();
        study.calculate(&bars);
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(50.0, 130.0),
            0.0,
            300.0,
            false,
        );
        let coords = ChartMapping::new(rect, 15.0, 0, 20, 0.0, price);
        let at = Pos2::new(coords.idx_to_x(18), price.price_to_y(83.6));
        assert!(rect.contains(at));
        let output = Context::default().run_ui(RawInput::default(), |ui| {
            IndicatorRenderer::render_study(
                &RenderContext::new(ui.painter(), rect),
                &study,
                &bars,
                0,
                &price,
                &coords,
            );
        });
        let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        let triangle = shapes.iter().position(|shape| {
            matches!(shape, Shape::Path(path)
            if path.points.len() == 3 && path.fill == bearish
                && (path.points.iter().map(|p| p.x).sum::<f32>() / 3.0 - at.x).abs() < 0.01)
        });
        let neckline = shapes.iter().position(|shape| {
            matches!(shape, Shape::LineSegment { points, stroke }
            if stroke.color == bearish && stroke.width == 1.0
                && points.iter().all(|p| (p.y - price.price_to_y(105.0)).abs() < 0.01))
        });
        let label = shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "DT"));
        let triangle = triangle.expect("no Bearish triangle on bar 18");
        assert!(label, "no DT label");
        assert!(neckline.expect("no neckline in the Bearish colour") < triangle);
        let to_y = |p| price.price_to_y(p);
        assert_eq!(
            hit_test_indicator(at, &study, 0, 0..21, &coords, to_y).map(|hit| hit.bar_idx),
            Some(18)
        );
        let on_neckline = Pos2::new(coords.idx_to_x(8), price.price_to_y(105.0));
        assert!(hit_test_indicator(on_neckline, &study, 0, 0..21, &coords, to_y).is_some());
    }

    /// Neutral pattern circles mark the low, and their labels follow lower prices on either
    /// scale direction. With 15 points per unit, low 98 maps to y 180, or 120 inverted.
    #[test]
    fn neutral_pattern_marks_sit_on_the_low_with_their_label_below() {
        use crate::studies::{
            CandlestickPatterns, ConfiguredStudy, IndicatorFactory, PatternTrend,
        };

        for (name, ohlc, abbr) in [
            ("Doji", (100.0_f64, 102.0, 98.0, 100.0625_f64), "D"),
            ("Spinning Top Black", (100.5, 102.0, 98.0, 100.0), "STB"),
            ("Spinning Top White", (100.0, 102.0, 98.0, 100.5), "STW"),
            (
                "All Candlestick Patterns",
                (100.0, 102.0, 98.0, 100.0625),
                "D",
            ),
        ] {
            for inverted in [false, true] {
                let mut previous = 99.0_f64;
                let mut bars: Vec<_> = (0..14)
                    .map(|i| {
                        let close = 100.0 + f64::from(i % 2);
                        let open = previous;
                        previous = close;
                        Bar::new(
                            Utc::now(),
                            open,
                            open.max(close) + 0.25,
                            open.min(close) - 0.25,
                            close,
                            1.0,
                        )
                    })
                    .collect();
                let (open, high, low, close) = ohlc;
                bars.push(Bar::new(Utc::now(), open, high, low, close, 1.0));
                let study: Box<dyn Indicator> = if name == "All Candlestick Patterns" {
                    Box::new(CandlestickPatterns::new(PatternTrend::NoDetection))
                } else {
                    IndicatorFactory::new().create(name).unwrap()
                };
                let mut study = ConfiguredStudy::new(study);
                study.calculate(&bars);
                let line = if name == "All Candlestick Patterns" {
                    2
                } else {
                    0
                };
                assert_eq!(study.values().value(line, 14), Some(98.0), "{name}");
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
                let price = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(90.0, 110.0),
                    0.0,
                    300.0,
                    inverted,
                );
                let coords = ChartMapping::new(rect, 15.0, 0, 20, 0.0, price);
                let output = Context::default().run_ui(RawInput::default(), |ui| {
                    IndicatorRenderer::render_study(
                        &RenderContext::new(ui.painter(), rect),
                        &study,
                        &bars,
                        0,
                        &price,
                        &coords,
                    );
                });
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text() == abbr => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .expect("neutral pattern label");
                assert!(
                    if inverted {
                        text.bottom() <= 115.0
                    } else {
                        text.top() >= 185.0
                    },
                    "{name}, inverted {inverted}: {text:?}"
                );
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Triangles that mark a bar at its high or low lie wholly outside the bar, beyond the price
    /// they mark, on either scale direction and whichever way they point, and a click on the far
    /// corner of one still selects its study. Prices map at 15 points a unit over 0 to 20: a
    /// width-1 mark on a low of 6 (y 210) spans y 212 to 218 and one on a high of 12 (y 120)
    /// spans 112 to 118.
    #[test]
    fn bar_marks_lie_outside_the_bars_they_mark() {
        use crate::chart::indicators::hit_test_indicator;
        use crate::studies::{BookerReversal, ConfiguredStudy, Reversal, WilliamsFractals};
        type Row = (
            &'static str,
            Box<dyn Indicator>,
            &'static [(f64, f64, f64)],
            bool,
            &'static [(usize, &'static str)],
        );
        // (high, low, close), labelled in reversal.rs: marks on the low of bar 4 and the high of
        // bar 7 with a lookback of 3.
        const REVERSAL: &[(f64, f64, f64)] = &[
            (10.0, 8.0, 9.0),
            (9.5, 7.5, 8.0),
            (8.5, 7.0, 7.5),
            (8.0, 6.5, 7.0),
            (8.5, 6.0, 8.2),
            (9.0, 7.8, 8.8),
            (11.0, 8.5, 10.5),
            (12.0, 8.0, 7.5),
        ];
        // Bar 2 is an up fractal on its high and a down fractal on its low.
        const FRACTAL: &[(f64, f64, f64)] = &[
            (10.0, 7.0, 8.0),
            (11.0, 6.0, 8.0),
            (12.0, 5.0, 8.0),
            (11.0, 6.0, 8.0),
            (10.0, 7.0, 8.0),
        ];
        let rows: [Row; 5] = [
            (
                "Reversal",
                Box::new(Reversal::new(3)),
                REVERSAL,
                false,
                &[(4, "below"), (7, "above")],
            ),
            (
                "Reversal, inverted",
                Box::new(Reversal::new(3)),
                REVERSAL,
                true,
                &[(4, "below"), (7, "above")],
            ),
            // Labelled in booker_reversal.rs: a sell on the high of bar 2, then a buy on its low.
            (
                "Booker Reversal sell",
                Box::new(BookerReversal::new(1, 2, 3, 0)),
                &[(10.0, 3.0, 10.0), (13.0, 12.0, 13.0), (12.0, 10.5, 11.0)],
                false,
                &[(2, "above")],
            ),
            (
                "Booker Reversal buy",
                Box::new(BookerReversal::new(1, 2, 3, 0)),
                &[(17.0, 9.0, 10.0), (8.0, 7.0, 7.0), (9.5, 8.5, 9.0)],
                false,
                &[(2, "below")],
            ),
            (
                "Williams Fractals",
                Box::new(WilliamsFractals::new(2)),
                FRACTAL,
                false,
                &[(2, "above"), (2, "below")],
            ),
        ];
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let mut wrong = Vec::new();
        for (label, study, hlc, inverted, want) in rows {
            let bars: Vec<Bar> = hlc
                .iter()
                .enumerate()
                .map(|(i, &(high, low, close))| {
                    let time = Utc.timestamp_opt(i as i64 * 86_400, 0).unwrap();
                    Bar::new(time, close, high, low, close, 1.0)
                })
                .collect();
            // Wrapped as `Chart::add_indicator` wraps every study.
            let mut study = ConfiguredStudy::new(study);
            study.calculate(&bars);
            let price = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 20.0),
                0.0,
                300.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 15.0, 0, 20, 0.0, price);
            let output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &bars,
                    0,
                    &price,
                    &coords,
                );
            });
            let triangles: Vec<[Pos2; 3]> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Path(path) if path.closed && path.points.len() == 3 => {
                        Some([path.points[0], path.points[1], path.points[2]])
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            let mut got = Vec::new();
            for triangle in triangles {
                let x = triangle.iter().map(|p| p.x).sum::<f32>() / 3.0;
                let Some(bar) = (0..bars.len()).find(|&i| (coords.idx_to_x(i) - x).abs() < 0.5)
                else {
                    wrong.push(format!("{label}: a triangle at x {x} marks no bar"));
                    continue;
                };
                let (low, high) = (
                    price.price_to_y(bars[bar].low),
                    price.price_to_y(bars[bar].high),
                );
                // Every vertex past `edge`, on the side away from `other`.
                let beyond = |edge: f32, other: f32| {
                    triangle.iter().all(|p| (p.y - edge) * (edge - other) > 0.0)
                };
                got.push((
                    bar,
                    if beyond(low, high) {
                        "below"
                    } else if beyond(high, low) {
                        "above"
                    } else {
                        "inside"
                    },
                ));
                // The vertex farthest from the bar's middle.
                let middle = (low + high) / 2.0;
                let far = triangle
                    .into_iter()
                    .max_by(|a, b| (a.y - middle).abs().total_cmp(&(b.y - middle).abs()))
                    .unwrap();
                let hit = hit_test_indicator(far, &study, 0, 0..bars.len(), &coords, |p| {
                    price.price_to_y(p)
                })
                .map(|hit| hit.bar_idx);
                if hit != Some(bar) {
                    wrong.push(format!(
                        "{label}: a click at {far:?} selects {hit:?}, not bar {bar}"
                    ));
                }
            }
            got.sort();
            if got != want {
                wrong.push(format!("{label}: {got:?}, want {want:?}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Boundaries follow their direction or developing plot in paint, selection and autoscale.
    #[cfg(feature = "drawings")]
    #[test]
    fn chart_pattern_boundaries_take_the_styles_of_their_plots() {
        use crate::chart::indicators::hit_test_indicator;
        use crate::studies::{ChartPatterns, ConfiguredStudy, PatternFamily};
        let bullish = Color32::from_rgb(0, 160, 80);
        let bearish = Color32::from_rgb(200, 0, 120);
        let developing = Color32::from_rgb(230, 150, 0);
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(50.0, 130.0),
            0.0,
            300.0,
            false,
        );
        let coords = ChartMapping::new(rect, 15.0, 0, 20, 0.0, price);
        let y = price.price_to_y(105.0);
        // Tops at bars 5 and 15, valley at 105 on bar 10. The second top is known on
        // bar 18, closing at 119 + (60 - 119) * 3 / 5 = 83.6, below the neckline.
        // Ending at 112 instead keeps every close after the second top above 105.
        for (anchors, plot, end, width, color, dashed) in [
            (
                [100.0, 120.0, 105.0, 119.0, 60.0],
                1,
                18,
                1.0,
                bearish,
                false,
            ),
            (
                [100.0, 120.0, 105.0, 119.0, 112.0],
                2,
                20,
                2.0,
                developing,
                true,
            ),
        ] {
            let bars = pattern_bars(&anchors);
            let mut study =
                ConfiguredStudy::new(Box::new(ChartPatterns::new(PatternFamily::Reversal)));
            let mut settings = study.settings();
            for (plot, color) in settings
                .plots
                .iter_mut()
                .zip([bullish, bearish, developing])
            {
                plot.color = color;
            }
            settings.plots[2].width = 2.0;
            // Exercise the boundary independently of its target and price line.
            for style in &mut settings.plots[3..] {
                style.visible = false;
            }
            study.apply_settings(&[], settings.clone()).unwrap();
            study.calculate(&bars);
            let paint = |study: &dyn Indicator| {
                let output = Context::default().run_ui(RawInput::default(), |ui| {
                    IndicatorRenderer::render_study(
                        &RenderContext::new(ui.painter(), rect),
                        study,
                        &bars,
                        0,
                        &price,
                        &coords,
                    );
                });
                fn collect(shape: &Shape, y: f32, segments: &mut Vec<([Pos2; 2], Stroke)>) {
                    match shape {
                        Shape::Vec(shapes) => {
                            for shape in shapes {
                                collect(shape, y, segments);
                            }
                        }
                        Shape::LineSegment { points, stroke }
                            if points.iter().all(|p| (p.y - y).abs() < 0.01) =>
                        {
                            segments.push((*points, *stroke));
                        }
                        _ => {}
                    }
                }
                let mut segments = Vec::new();
                for shape in &output.shapes {
                    collect(&shape.shape, y, &mut segments);
                }
                output.drop_without_applying_deltas();
                segments
            };
            let segments = paint(&study);
            assert!(!segments.is_empty(), "plot {plot}");
            assert!(
                segments
                    .iter()
                    .all(|(_, stroke)| *stroke == Stroke::new(width, color))
            );
            assert!((segments[0].0[0].x - coords.index_to_x(5.0)).abs() < 0.01);
            if dashed {
                assert!(segments.len() > 1);
                assert!(
                    segments
                        .iter()
                        .all(|(points, _)| points[0].distance(points[1]) <= 8.01)
                );
            } else {
                assert_eq!(segments.len(), 1);
                assert!((segments[0].0[1].x - coords.index_to_x(f64::from(end))).abs() < 0.01);
            }
            for (hidden, drawn) in [(0, true), (plot, false)] {
                let mut hidden_settings = settings.clone();
                hidden_settings.plots[hidden].visible = false;
                study.apply_settings(&[], hidden_settings).unwrap();
                assert_eq!(
                    !paint(&study).is_empty(),
                    drawn,
                    "plot {plot}, hidden {hidden}"
                );
                assert_eq!(
                    crate::studies::plots::drawing_prices(&study, 0..21).collect::<Vec<_>>(),
                    if drawn {
                        vec![105.0, 105.0]
                    } else {
                        Vec::new()
                    },
                    "plot {plot}, hidden {hidden}",
                );
                assert_eq!(
                    hit_test_indicator(
                        Pos2::new(coords.idx_to_x(8), y),
                        &study,
                        0,
                        0..21,
                        &coords,
                        |p| price.price_to_y(p)
                    )
                    .is_some(),
                    drawn,
                    "plot {plot}, hidden {hidden}",
                );
            }
        }
    }

    /// A fit of prices 2, 4, 6 continues by two per bar on every price scale.
    #[test]
    fn regression_extension_projects_prices_before_mapping_to_the_axis() {
        let bars: Vec<_> = [2.0, 4.0, 6.0]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
            .collect();
        for mode in [PriceScaleMode::Normal, PriceScaleMode::Logarithmic] {
            for inverted in [false, true] {
                for (width, low, high) in [(300.0, 1.0, 20.0), (70.0, 3.0, 5.0)] {
                    let rect = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(width, 300.0));
                    let transform = PriceTransform { mode, base: 2.0 };
                    let price = PriceMap::new(
                        transform,
                        PriceRange::new(transform.to_scale(low), transform.to_scale(high)),
                        rect.top(),
                        rect.bottom(),
                        inverted,
                    );
                    let coords = ChartMapping::new(rect, 20.0, 0, 2, 3.0, price);
                    let mut study = LinearRegressionChannel::new(3, 0.0).with_extend_right(true);
                    study.set_colors(vec![Color32::BLUE]);
                    study.calculate(&bars);
                    let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
                    let mut output = Context::default().run_ui(
                        RawInput {
                            screen_rect: Some(rect),
                            ..Default::default()
                        },
                        |ui| {
                            IndicatorRenderer::render(
                                &RenderContext::new(ui.painter(), rect),
                                &indicators,
                                &bars,
                                &price,
                                &coords,
                            );
                        },
                    );
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let want_price =
                        6.0 + 2.0 * ((rect.right() - coords.idx_to_x(2)) / 20.0) as f64;
                    let ends: Vec<_> = shapes
                        .iter()
                        .filter_map(|clipped| match clipped.shape {
                            Shape::LineSegment { points, stroke }
                                if stroke.color == Color32::BLUE && points[1].x == rect.right() =>
                            {
                                Some(points[1].y)
                            }
                            _ => None,
                        })
                        .collect();
                    assert!(
                        shapes.iter().any(|clipped| matches!(
                            &clipped.shape,
                            Shape::Path(path)
                                if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::BLUE)
                                    && path.points.windows(2).any(|points|
                                        points[0].x == coords.idx_to_x(0)
                                            && points[1].x == coords.idx_to_x(1))
                        )),
                        "the segment entering the visible price range must survive"
                    );
                    for clipped in &shapes {
                        if let Shape::Text(text) = &clipped.shape {
                            assert!(clipped.clip_rect == rect, "fit label must stay in the plot");
                            assert!(text.pos.x >= rect.left(), "fit label escapes a narrow plot");
                        }
                    }
                    assert_eq!(ends.len(), 3);
                    for y in ends {
                        assert!(
                            (y - price.price_to_y(want_price)).abs() < 1e-3,
                            "{mode:?}, inverted={inverted}: {y}"
                        );
                    }
                }
            }
        }
    }

    /// An AO saved as a 4 px histogram before falling bars had a width of their own draws its
    /// falling bars as it did then, at the plot width.
    #[test]
    fn older_ao_histogram_keeps_its_falling_bar_width() {
        use crate::studies::{AwesomeOscillator, ConfiguredStudy, StudyTemplate};
        let bars: Vec<_> = [1.0, 2.0, 4.0, 3.0, 1.0, 4.0]
            .into_iter()
            .map(|p| Bar::new(Utc.timestamp_opt(0, 0).unwrap(), p, p, p, p, 1.0))
            .collect();
        let study = ConfiguredStudy::new(Box::new(AwesomeOscillator::new(2, 3)));
        let mut saved = serde_json::to_value(StudyTemplate::from_study(&study)).unwrap();
        for key in [
            "price_line_plots",
            "plot_variants",
            "background",
            "background_visible",
            "fills",
        ] {
            saved["settings"].as_object_mut().unwrap().remove(key);
        }
        saved["settings"]["plots"][0]["plot_type"] = serde_json::json!("Histogram");
        saved["settings"]["plots"][0]["width"] = 4.0.into();
        let template: StudyTemplate = serde_json::from_value(saved).unwrap();
        let mut loaded = template.create().unwrap();
        loaded.calculate(&bars);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(240.0, 120.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(-1.0, 1.0),
            0.0,
            120.0,
            false,
        );
        let coords = ChartMapping::new(rect, 30.0, 0, 5, 0.0, map);
        let mut output = Context::default().run_ui(RawInput::default(), |ui| {
            IndicatorRenderer::render_study(
                &RenderContext::new(ui.painter(), rect),
                loaded.as_ref(),
                &bars,
                0,
                &map,
                &coords,
            );
        });
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        // AO(2, 3) is 2/3, 1/2, -2/3 and -1/6 from bar 2, so bars 3 and 4 fall.
        let falling = AwesomeOscillator::new(2, 3).colors()[1];
        let widths: Vec<f32> = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Rect(r) if r.fill == falling => Some(r.rect.width()),
                _ => None,
            })
            .collect();
        assert_eq!(widths, [4.0, 4.0], "falling bar widths {widths:?}");
    }
}
