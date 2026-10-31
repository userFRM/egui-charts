//! The account's fills on the chart: one mark per side and bar, up to three filled triangles
//! stacked outward from the bar, under its low for buys and over its high for sells.

use std::sync::Arc;

use crate::localization as l10n;
use egui::{FontId, Galley, Rect, Sense, Shape, Stroke, Ui, pos2};

use crate::model::{Execution, OrderSide};
use crate::theme::ThemeExt;
use crate::widget::Chart;

/// The most triangles one mark draws; a "+N" line counts the rest.
const MAX_STACK: usize = 3;
/// The most fills a mark's hover lists before "and N more".
const MAX_HOVER_FILLS: usize = 20;
/// Half the width of a triangle's base.
const HALF_WIDTH: f32 = 7.0;
/// The height of a triangle, from its tip to its base.
const HEIGHT: f32 = 17.0;
/// The space between stacked triangles.
const STEP: f32 = 2.0;
/// The space between a bar and its marks, and between a mark and its text.
const GAP: f32 = 4.0;

use super::account_panel::quantity_decimals;
use super::format::Numbers;
use super::widgets::{side_color, side_name};

/// The fills of one side in one bar, drawn as one mark.
struct Mark<'a> {
    /// The bar's index; a fractional index never occurs, `f64` matches `index_to_x`.
    index: f64,
    side: OrderSide,
    fills: Vec<&'a Execution>,
    /// Its lines of text, nearest the stack first, laid out once while its fills are known.
    lines: Vec<Arc<Galley>>,
}

/// Draws `fills` on the price pane of `chart`. The fills of one side in one bar form one mark: up
/// to [`MAX_STACK`] triangles stacked outward, pointing up in the buy colour just under the bar's
/// low, or down in the sell colour just over its high, and beyond them a "+N" line for the fills
/// not drawn. With `labels`, a line beyond the stack gives the side and total quantity. Hovering a
/// mark lists its fills with their side, quantity, price and time, up to [`MAX_HOVER_FILLS`].
pub(super) fn show(
    ui: &mut Ui,
    chart: &Chart,
    fills: &[&Execution],
    labels: bool,
    numbers: Numbers<'_>,
) {
    // A bar spans from its time to the next bar's, so a fill belongs to the bar at or before it.
    let bars = chart.state.shown();
    let mut placed: Vec<(f64, &Execution)> = fills
        .iter()
        .filter_map(|fill| {
            let index = if let Some(cutoff) = chart.replay_time() {
                if fill.time >= cutoff {
                    return None;
                }
                bars.partition_point(|bar| bar.time <= fill.time)
                    .checked_sub(1)
                    .map(|index| index as f64)
            } else {
                chart.time_to_index(fill.time).map(f64::floor)
            };
            Some((index?, *fill))
        })
        .collect();
    // Stable, so that a mark keeps its fills in the list's order.
    let is_sell = |fill: &Execution| matches!(fill.side, OrderSide::Sell);
    placed.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| is_sell(a.1).cmp(&is_sell(b.1)))
    });

    let mapping = chart.get_chart_mapping();
    let theme = ui.ctx().active_theme();
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let layout = |side: OrderSide, group: &[&Execution]| -> Vec<Arc<Galley>> {
        let more = (group.len() > MAX_STACK).then(|| format!("+{}", group.len() - MAX_STACK));
        let label = labels.then(|| {
            let quantity: f64 = group.iter().map(|fill| fill.quantity).sum();
            format!(
                "{} {}",
                l10n::text(ui.ctx(), side_name(side)),
                numbers.quantity(quantity, quantity_decimals(quantity))
            )
        });
        more.into_iter()
            .chain(label)
            .map(|line| {
                painter.layout_no_wrap(
                    line,
                    FontId::proportional(ui.active_theme().text.label),
                    chart.config().text_color,
                )
            })
            .collect()
    };
    let width = |lines: &[Arc<Galley>]| lines.iter().map(|l| l.size().x).fold(0.0, f32::max);
    // Coalesce neighbouring runs whose text would overlap horizontally; a mark without text has
    // nothing to overlap. Use the newest bar as the anchor and retain every fill for the total
    // and the hover details.
    let mut marks: Vec<Mark> = Vec::new();
    for run in placed.chunk_by(|a, b| a.0 == b.0 && a.1.side == b.1.side) {
        let (index, side) = (run[0].0, run[0].1.side);
        let group: Vec<&Execution> = run.iter().map(|(_, fill)| *fill).collect();
        let lines = layout(side, &group);
        match marks.iter_mut().rfind(|open| open.side == side) {
            Some(open)
                if !open.lines.is_empty()
                    && !lines.is_empty()
                    && chart
                        .index_to_x(open.index)
                        .zip(chart.index_to_x(index))
                        .is_some_and(|(a, b)| {
                            (b - a).abs() < (width(&open.lines) + width(&lines)) / 2.0 + GAP
                        }) =>
            {
                open.index = index;
                open.fills.extend(group);
                open.lines = layout(side, &open.fills);
            }
            _ => marks.push(Mark {
                index,
                side,
                fills: group,
                lines,
            }),
        }
    }
    let mut text_rects: Vec<Rect> = Vec::new();
    for mark in &marks {
        let Some(x) = chart.index_to_x(mark.index) else {
            continue;
        };
        if x < mapping.rect.left() - HALF_WIDTH || x > mapping.rect.right() + HALF_WIDTH {
            continue;
        }
        let quantity: f64 = mark.fills.iter().map(|fill| fill.quantity).sum();
        let price = mark
            .fills
            .iter()
            .map(|fill| fill.price * fill.quantity)
            .sum::<f64>()
            / quantity;
        // A fill timed after the last bar waits at its price for its bar.
        let (low, high) = usize::try_from(mark.index as i64)
            .ok()
            .and_then(|index| bars.get(index))
            .map_or((price, price), |bar| (bar.low, bar.high));
        let (low_y, high_y) = (mapping.price_to_y(low), mapping.price_to_y(high));
        let color = side_color(&theme, mark.side);
        let (down, edge) = match mark.side {
            OrderSide::Buy => (1.0, low_y.max(high_y) + GAP),
            OrderSide::Sell => (-1.0, low_y.min(high_y) - GAP),
        };
        if !edge.is_finite() {
            continue;
        }
        let at = |dx: f32, dy: f32| pos2(x + dx, edge + down * dy);
        let drawn = mark.fills.len().min(MAX_STACK);
        for triangle in 0..drawn {
            let dy = triangle as f32 * (HEIGHT + STEP);
            painter.add(Shape::convex_polygon(
                vec![
                    at(0.0, dy),
                    at(HALF_WIDTH, dy + HEIGHT),
                    at(-HALF_WIDTH, dy + HEIGHT),
                ],
                color,
                Stroke::NONE,
            ));
        }
        let length = drawn as f32 * HEIGHT + (drawn - 1) as f32 * STEP;
        let mut rect = Rect::from_two_pos(at(-HALF_WIDTH, 0.0), at(HALF_WIDTH, length));
        if !mark.lines.is_empty() {
            let height: f32 = mark.lines.iter().map(|line| line.size().y).sum::<f32>()
                + (mark.lines.len() - 1) as f32 * GAP / 2.0;
            let size = egui::vec2(width(&mark.lines), height);
            let top = if down > 0.0 {
                rect.bottom() + GAP
            } else {
                rect.top() - GAP - size.y
            };
            let mut block = Rect::from_min_size(pos2(x - size.x / 2.0, top), size);
            while text_rects
                .iter()
                .any(|other| other.expand(GAP / 2.0).intersects(block))
            {
                block = block.translate(egui::vec2(0.0, down * (size.y + GAP)));
            }
            text_rects.push(block);
            let mut y = if down > 0.0 {
                block.top()
            } else {
                block.bottom()
            };
            for line in &mark.lines {
                let size = line.size();
                let top = if down > 0.0 { y } else { y - size.y };
                let line_rect = Rect::from_min_size(pos2(x - size.x / 2.0, top), size);
                // The chart's background keeps the text readable over the bars beside its own.
                painter.rect_filled(line_rect.expand(1.0), 2.0, theme.chart.bg);
                painter.galley(line_rect.min, line.clone(), chart.config().text_color);
                y += down * (size.y + GAP / 2.0);
            }
            rect = rect.union(block);
        }
        let hover = rect.expand(2.0).intersect(mapping.rect);
        if hover.is_positive() {
            ui.interact(
                hover,
                ui.scope_id()
                    .with(("execution", side_name(mark.side), mark.index.to_bits())),
                Sense::hover(),
            )
            // Written only while it shows: a mark can hold every fill of the list.
            .on_hover_ui(|ui| {
                let time = chart.build_time_formatter();
                let decimals = numbers.decimals;
                let mut lines: Vec<_> = mark
                    .fills
                    .iter()
                    .take(MAX_HOVER_FILLS)
                    .map(|fill| {
                        format!(
                            "{} {} at {:.decimals$}, {}",
                            side_name(fill.side),
                            fill.quantity,
                            fill.price,
                            // With seconds, also on charts of daily bars.
                            time.format_crosshair(fill.time, 1)
                        )
                    })
                    .collect();
                let more = mark.fills.len().saturating_sub(MAX_HOVER_FILLS);
                if more > 0 {
                    lines.push(l10n::format_text(
                        ui.ctx(),
                        "and {0} more",
                        &[("0", more.to_string())],
                    ));
                }
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.label(l10n::text(ui.ctx(), lines.join("\n")));
            });
        }
    }
}
